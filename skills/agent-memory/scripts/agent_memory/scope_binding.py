"""Shared attribution/project resolution and explicit, recoverable scope binding."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from . import repository_config
from . import bank_contract as bank
from . import bank_transactions as transactions
from . import bank_writes as writes
from . import developer_identity as developer
from . import local_registry as storage
from . import project_identity as project
from . import frontmatter
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError

SCHEMA = 'agent-memory/scope-binding-v1'
FIELDS = {'schema', 'operation_id', 'developer_id', 'developer_sha256', 'project', 'base_files', 'transaction'}


def result(operation, **values):
    return {'schema': 'agent-memory/scope-result-v1', 'operation': operation,
            'identity_authenticated': False, 'security_boundary': developer.SPEC['security-boundary'], **values}


def principal(repo, observations=(), harness=None, session=None):
    value = developer.status(observations, harness, session, repo)
    if value['setup_required'] or value['attribution_conflicts'] or value['lock_present']:
        raise ContractError('Developer attribution is missing, conflicting or locked; explicitly create/adopt/select or repair it')
    return value


def status(repo, observations=(), harness=None, session=None):
    issues = []
    try:
        dev = developer.status(observations, harness, session, repo)
        if dev['setup_required'] or dev['attribution_conflicts'] or dev['lock_present']:
            issues.append('Developer attribution requires explicit setup or clarification')
    except (ContractError, OSError, ValueError) as exc:
        dev = {'developer_id': None, 'error': str(exc)}
        issues.append(str(exc))
    try:
        proj = project.status(repo)
        issues.extend(proj['issues'])
        if storage.paths('projects')[2].exists():
            issues.append('Project registry is locked; inspect/recover before a write')
    except (ContractError, OSError, ValueError) as exc:
        proj = {'project_id': None, 'resolved': False, 'error': str(exc)}
        issues.append(str(exc))
    return result('status', developer_id=dev['developer_id'], project_id=proj['project_id'],
                  resolved=not issues, issues=issues, developer=dev, project=proj, writes_performed=False)


def require(repo, project_id=None, *, expected=None, initialize=False, owned_lock=None, owned_transaction=None):
    """Common guard for every mediated writer; raw file inspection stays independent."""
    dev = principal(repo)
    proj = project.status(repo, owned_lock=owned_lock, owned_transaction=owned_transaction)
    if storage.paths('projects')[2].exists():
        raise ContractError('Project registry is locked; inspect/recover before a write')
    if initialize:
        # A registered empty checkout is sufficient to start initialization. No
        # existing bank ID is required, and this never authorizes other writes.
        proof, checkout = proj['evidence'], proj['checkout_entry']
        if (proof['bank_project_id'] is not None or not checkout or not proj['project_entry']
                or proj['issues'] != ['Bank initialization or identity update is pending']):
            raise ContractError('Initialization requires an explicitly registered empty checkout with unambiguous authority')
    elif not proj['resolved']:
        raise ContractError('Project identity is unresolved: ' + '; '.join(proj['issues']))
    if project_id is not None and project_id != proj['project_id']:
        raise ContractError('Operation project ID differs from the resolved repository')
    context = {'developer_id': dev['developer_id'], 'project_id': proj['project_id'],
               'developer_sha256': dev['observed_sha256'], 'project_sha256': proj['observed_sha256'],
               'checkout_key': proj['evidence']['checkout_key'], 'repository_settings_sha256': digest(encoded(proj['evidence']['repository_settings'])), 'initialization_only': initialize}
    if expected is not None and context != expected:
        raise ContractError('Resolved identity changed during the operation; retain original provenance and prepare again')
    return context


def capture(repo):
    """A read-only proposal can exist while setup is pending, but cannot be applied."""
    try:
        return require(repo)
    except (ContractError, OSError, ValueError):
        return None


def context_value(context, *, initialize=False):
    if context is None:
        return
    if (not isinstance(context, dict) or set(context) != {'developer_id', 'project_id', 'developer_sha256',
            'project_sha256', 'checkout_key', 'repository_settings_sha256', 'initialization_only'}
            or not uuid_valid(context['developer_id']) or not uuid_valid(context['project_id'])
            or context['initialization_only'] is not initialize):
        raise ContractError('Invalid pinned scope context')


def identity_files(files, identity):
    """Change namespace fields only; preserve record identities and all source metadata."""
    updated = dict(files)
    root, _, _ = bank.parse_router(files['MEMORY.md'])
    _, body = frontmatter.read(files['MEMORY.md'], allowed=root)
    root['project-id'] = identity['project-id']
    if identity['canonical-remote'] is None:
        root.pop('canonical-remote', None)
    else:
        root['canonical-remote'] = identity['canonical-remote']
    updated['MEMORY.md'] = frontmatter.write(root) + body
    checked = bank.validate_files(files)
    for entry in checked['entries']:
        if entry['format'] == 'context-prose':
            continue
        path = entry['path']
        # Read only the existing top-level keys, then change the known namespace
        # field. The complete shared validator checks the resulting format.
        original = files[path]
        end = original.find('\n---\n', 4)
        keys = [line.split(':', 1)[0] for line in original[4:end].splitlines() if not line.startswith(' ')]
        metadata, content = frontmatter.read(original, allowed=keys)
        key = 'project-id' if 'project-id' in metadata else 'scope'  # Five-field tombstone exception.
        metadata[key] = identity['project-id']
        updated[path] = frontmatter.write(metadata) + content
    bank.validate_files(updated)
    return updated


def preview(repo, action, target=None, reason=None, authority=None):
    dev = principal(repo)
    prepared = project.preview(repo, action, target, reason, authority)['proposal']
    root = Path(prepared['evidence']['repo_root'])
    base, transaction = None, None
    if prepared['evidence']['bank_project_id'] is not None:
        _, observed = writes.snapshot(root)
        base = transactions.files_at(repository_config.bank_path(root))
        if transactions.file_hashes(base) != observed:
            raise ContractError('Bank changed while preparing binding')
        desired = identity_files(base, prepared['bank_identity'])
        if desired != base:
            transaction = transactions.preview(root, desired, 'bind')['proposal']
    bundle = {'schema': SCHEMA, 'operation_id': prepared['operation_id'], 'developer_id': dev['developer_id'],
              'developer_sha256': dev['observed_sha256'], 'project': prepared, 'base_files': base, 'transaction': transaction}
    return result('preview', proposal=bundle, project_id=prepared['project_id'], developer_id=dev['developer_id'],
                  registry_change=prepared, changed_memory_paths=[] if transaction is None else
                  [repository_config.relative(root) + '/' + path for path in transactions.changed(transaction)],
                  initialization_pending=base is None, writes_performed=False, verified=False)


def bundle_value(prepared):
    p = prepared.get('proposal', prepared) if isinstance(prepared, dict) else prepared
    if (not isinstance(p, dict) or set(p) != FIELDS or p['schema'] != SCHEMA
            or not uuid_valid(p['operation_id']) or not uuid_valid(p['developer_id'])
            or not isinstance(p['project'], dict) or p['project'].get('operation_id') != p['operation_id']):
        raise ContractError('Invalid scope binding bundle')
    proj = p['project']
    if p['base_files'] is not None:
        bank.validate_files(p['base_files'])
        if digest(p['base_files']['MEMORY.md'].encode()) != proj['evidence']['router_sha256']:
            raise ContractError('Binding base differs from the project preview')
        desired = identity_files(p['base_files'], proj['bank_identity'])
        tx = p['transaction']
        if desired == p['base_files']:
            if tx is not None:
                raise ContractError('Unnecessary or unknown bank transaction in binding')
        elif (tx is None or transactions.proposal_value(tx)['operation'] != 'bind'
              or tx['repo_root'] != proj['evidence']['repo_root']
              or tx['project_id'] != proj['evidence']['bank_project_id']
              or tx['observed_hashes'] != transactions.file_hashes(p['base_files']) or tx['files'] != desired):
            raise ContractError('Binding may change namespace fields only, using the observed bank transaction')
    elif p['transaction'] is not None or proj['evidence']['bank_project_id'] is not None or proj['action'] != 'create':
        raise ContractError('Only explicit create can precede bank initialization')
    return p


def binding_guard(p, owner=None, marker=None):
    root = p['project']['evidence']['repo_root']
    dev = principal(root)
    if dev['developer_id'] != p['developer_id'] or dev['observed_sha256'] != p['developer_sha256']:
        raise ContractError('Developer selection/registry changed after preview; do not reattribute this binding')
    data, _ = project.observe()
    if storage.paths('projects')[2].exists():
        raise ContractError('Project registry is locked; retry after inspection')
    proof = project.evidence(root, data, owned_lock=owner, owned_transaction=marker)
    operation = data['operations'].get(p['operation_id'])
    original = p['project']['evidence']
    expected_ids = {original['bank_project_id'], p['project']['project_id']}
    if (not operation or operation['proposal_sha256'] != digest(encoded(p['project']))
            or any(proof[key] != value for key, value in original.items()
                   if key not in {'bank_project_id', 'bank_canonical_remote', 'router_sha256'})
            or data['checkouts'][proof['checkout_key']]['project_id'] != p['project']['project_id']
            or data['projects'][p['project']['project_id']]['canonical_remote'] != original['canonical_remote']
            or proof['bank_project_id'] not in expected_ids):
        raise ContractError('Binding decision no longer matches the registry/repository; preserve the pending bank')


def apply(prepared):
    p = bundle_value(prepared)
    root = Path(p['project']['evidence']['repo_root'])
    dev = principal(root)
    if dev['developer_id'] != p['developer_id'] or dev['observed_sha256'] != p['developer_sha256']:
        raise ContractError('Developer selection/registry changed after binding preview')
    tx = p['transaction']
    registered, _ = project.observe()
    if p['operation_id'] not in registered['operations'] and p['base_files'] is not None:
        if writes.snapshot(root)[1] != transactions.file_hashes(p['base_files']):
            raise ContractError('Bank changed after binding preview; preserve the edit and prepare again')
    from . import security
    audit_context = {'developer_id': dev['developer_id'], 'project_id': p['project']['project_id']}
    security.audit(root, p['operation_id'], 'mutation', 'started', audit_context, ['external'], action='bind')
    bundle_path = None
    if tx is not None:
        # Persist the exact two-store decision before either step. There is no
        # cross-store atomicity claim; a registry-only success stays pending.
        journal, receipt_path = transactions.locations(root, repository_config.bank_path(root), tx['operation_id'])
        bundle_path = journal.parent / 'binding.json'
        raw = encoded(p)
        if bundle_path.is_symlink() or (bundle_path.exists() and bundle_path.read_bytes() != raw):
            raise ContractError('Unknown binding recovery bundle; preserve it')
        if not bundle_path.exists() and transactions.receipt_value(receipt_path, tx) is None:
            bundle_path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            storage.replace(bundle_path, raw, None)
    # Existing operation replay never re-applies an old registry decision.
    registry = project.apply(p['project'])
    writes.fault('binding-after-registry')
    marker_path = repository_config.bank_path(root) / transactions.MARKER
    marker = transactions.marker_bytes(tx) if tx is not None and marker_path.exists() else None
    binding_guard(p, marker=marker)
    bank_result = None
    if tx is not None:
        if marker_path.exists():
            expected_marker = transactions.marker_bytes(tx)
            if marker_path.is_symlink() or marker_path.read_bytes() != expected_marker:
                raise ContractError('Another interrupted operation is present; inspect/recover it first')
            recovery = transactions._recover(root, 'finish')['proposal']
            bank_result = transactions._recover(root, prepared=recovery,
                check_identity=lambda owner, marker: binding_guard(p, owner, marker))
        else:
            bank_result = transactions._apply(tx, check_identity=lambda owner: binding_guard(p, owner))
    if bundle_path is not None and bundle_path.exists() and bank_result and bank_result.get('verified'):
        # Completed receipts are content-free. Do not retain whole-bank binding
        # backups after the transaction has a verified outcome.
        transactions.cleanup(bundle_path, encoded(p))
    current = status(root)
    pending = current['project'].get('evidence', {}).get('bank_project_id') is None
    same_project = current['project_id'] == p['project']['project_id']
    still_current = same_project and (bank_result is None or bank_result.get('still_current', False))
    verified = not pending and current['resolved'] and still_current
    if not pending and not verified:
        raise ContractError('Binding readback differs or was superseded; preserve current state and inspect')
    audit = security.audit(root, p['operation_id'], 'mutation', 'completed', audit_context,
                           ['external'], action='bind') if verified else None
    return result('apply', operation_id=p['operation_id'], project_id=p['project']['project_id'], developer_id=p['developer_id'],
                  registry=registry, bank=bank_result, status=current, resolved=verified,
                  initialization_pending=pending, initialization_context=require(root, initialize=True) if pending else None,
                  recovery_bundle=str(bundle_path) if bundle_path else None,
                  writes_performed=registry['writes_performed'] or bool(bank_result and bank_result['writes_performed']),
                  changed_memory_paths=bank_result['changed_memory_paths'] if bank_result else [],
                  verified=verified, registry_verified=registry['verified'], still_current=still_current,
                  security_audit=audit)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('status', 'preview', 'require'):
        sub = commands.add_parser(name)
        sub.add_argument('--repo', default='.')
        if name == 'status':
            sub.add_argument('--harness')
            sub.add_argument('--session')
            sub.add_argument('--observe', action='append', default=[])
        elif name == 'require':
            sub.add_argument('--initialize', action='store_true')
        else:
            sub.add_argument('--action', choices=project.SPEC['actions'], required=True)
            sub.add_argument('--project-id')
            sub.add_argument('--authority')
            sub.add_argument('--reason', required=True)
    writer = commands.add_parser('apply')
    writer.add_argument('--proposal', required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == 'status':
            value = status(args.repo, developer.declared(args.observe), args.harness, args.session)
        elif args.command == 'preview':
            value = preview(args.repo, args.action, args.project_id, args.reason, args.authority)
        elif args.command == 'require':
            value = result('require', context=require(args.repo, initialize=args.initialize), writes_performed=False, verified=True)
        else:
            value = apply(load(Path(args.proposal).read_bytes()))
        print(json.dumps(value, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps(result(args.command, error=str(exc), verified=False, inspection_required=True)))
        return 1


if __name__ == '__main__':
    sys.exit(main())
