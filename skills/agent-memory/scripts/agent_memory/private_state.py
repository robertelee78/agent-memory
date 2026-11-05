"""One fail-closed union of bank-local private runtime requirements."""
from __future__ import annotations

from contextlib import ExitStack, contextmanager
from contextvars import ContextVar
from pathlib import Path
import os

from . import artifact, layout_recovery_state as state, project_identity, repository_config
from .developer_identity import digest, encoded
from .frontmatter import ContractError
from .identity_git import git

TOKENS = {'layout-frontier-v1', 'omp-harness-v1'}


_barriers = ContextVar('private-state-lifecycle-barriers', default=None)
def compatibility(target):
    if target is None:
        return set()
    value = target.get('compatibility', target)
    tokens = value.get('private_state_capabilities', [])
    if not artifact.strings(tokens, allow_empty=True) or not set(tokens).issubset(TOKENS):
        raise ContractError('Target runtime has an unknown private-state compatibility declaration')
    return set(tokens)


def repository_inventory(installation=None):
    """Discovery is a union, not an intersection with currently available banks."""
    roots, unknown = set(), []
    cwd = Path.cwd().resolve()
    top = git(cwd, 'rev-parse', '--show-toplevel', optional=True)
    roots.add(Path(top).resolve() if top else cwd)
    registry, registry_sha = project_identity.observe()
    for entry in registry['checkouts'].values():
        for location in entry['locations']:
            path = Path(location)
            if not path.is_dir():
                unknown.append({'source': 'project-registry', 'location': location})
                continue
            listed = git(path, 'worktree', 'list', '--porcelain', optional=True)
            if listed is None:
                if (path / '.git').exists() or path.name == '.git':
                    unknown.append({'source': 'project-registry', 'location': location})
                else:
                    roots.add(path)
                continue
            for line in listed.splitlines():
                if line.startswith('worktree '):
                    roots.add(Path(line[9:]))
    bindings = state.bindings()
    for row in bindings['bindings'].values():
        if row['lifecycle_state'] in {'active', 'interrupted'}:
            roots.add(Path(row['repo_root']))
    if installation:
        for key in ('rollback_repositories', 'observed_repositories'):
            values = installation.get(key, [])
            if not isinstance(values, list) or any(not isinstance(value, str) for value in values):
                raise ContractError('Saved runtime bank inventory is malformed')
            roots.update(Path(value) for value in values)
        for row in installation.get('banks', []):
            roots.add(Path(row['repository']))
    try:
        from . import omp_boundary
    except ImportError:
        omp_boundary = None
    producer = getattr(omp_boundary, 'source_capability_repositories', None)
    if producer is not None:
        roots.update(Path(value) for value in producer(installation))
    elif installation and ('omp' in installation.get('presets', [])
            or any(row.get('harness') == 'omp' for row in installation.get('source_bindings', []))):
        unknown.append({'source': 'omp-bindings', 'reason': 'complete enrolled-bank discovery is unavailable'})
    if len(roots) > 1024:
        raise ContractError('Private-state bank inventory exceeds its complete finite bound')
    pins = {'project_registry_sha256': registry_sha, 'layout_bindings_sha256': digest(encoded(bindings))}
    return roots, unknown, pins


def required_private_state_capabilities(installation, target_artifact, operation):
    """Observe every producer under source barriers; never infer missing = capable.

    Lifecycle callers already hold their runtime lock. This function takes only
    the suffix source barrier and never invokes a lifecycle helper underneath it.
    The returned pins must be re-observed at each activation/readback boundary.
    """
    supported = compatibility(target_artifact)
    roots, unknown, inventory_pins = repository_inventory(installation)
    requirements, pins = {}, []
    target_compatibility = target_artifact.get('compatibility', target_artifact) if target_artifact else {}
    receipt_schema = installation.get('schema') if installation else None
    readable_receipts = target_compatibility.get('installation_receipt_schemas', ['agent-memory/install-receipt-v1'])
    if operation != 'uninstall' and receipt_schema == 'agent-memory/install-receipt-v2' and receipt_schema not in readable_receipts:
        unknown.append({'source': 'installation-receipt', 'reason': 'target cannot read retained v2 ownership receipt'})
    from . import recovery_frontier, bank_writes
    try:
        from . import omp_boundary
    except ImportError:
        omp_boundary = None
    with ExitStack() as stack:
        # Stable order avoids same-family cross-bank inversion.
        for root in sorted(roots, key=str):
            if not root.is_absolute() or root.is_symlink() or not root.is_dir():
                unknown.append({'source': 'bank-inventory', 'location': str(root)})
                continue
            try:
                settings = repository_config.settings(root)
                bank = root / settings['bank_root']
                if not bank.exists():
                    bound = any(row['repo_root'] == str(root) and row['lifecycle_state'] in {'active', 'interrupted'}
                        for row in state.bindings()['bindings'].values())
                    if bound:
                        raise ContractError('Bound recovery bank is unavailable')
                    continue
                capabilities = set()
                operations, _ = bank_writes.state_paths(root, bank, '00000000-0000-4000-8000-000000000000')
                source_marker = operations.parent / 'turn-sources-compat-v1.json'
                source_tree = operations.parent / 'omp-source-v1'
                held = _barriers.get()
                if held is not None and str(root) not in held:
                    raise ContractError('A bank joined the private-state inventory after lifecycle quiescence')
                from .turn_sources import source_barrier
                if held is None:
                    stack.enter_context(source_barrier(root, exclusive=True))
                producer = getattr(omp_boundary, 'required_source_capabilities', None)
                if producer is None:
                    if os.path.lexists(source_marker) or os.path.lexists(source_tree):
                        raise ContractError('Private native source requirement cannot be completely observed')
                else:
                    capabilities.update(producer(root))
                token = recovery_frontier.observe(root)
                indexed = state.index(root)
                layout_required = token['hold_sha256'] is not None or any(
                    row['state'] in {'active', 'interrupted'} for row in indexed['operations'].values())
                if layout_required:
                    capabilities.add('layout-frontier-v1')
                    target_digest = target_artifact.get('artifact_digest') if target_artifact else None
                    frontier_operation = next(operation_id for operation_id, indexed_row in indexed['operations'].items()
                        if indexed_row['state'] in {'active', 'interrupted'})
                    override = target_artifact.get('layout_quiescence_overrides', {}).get(frontier_operation) if target_artifact else None
                    if override is not None:
                        recovery_frontier.validate_quiescence(root, override, runtime_artifact_digest=target_digest,
                            required_routes=target_artifact.get('layout_required_routes', ()))
                    else:
                        recovery_frontier.verified_hold(root)
                    if target_digest is not None:
                        from .recovery_quiescence import selected_attestation
                        directory, _, ledger_value = recovery_frontier.ledger(root, frontier_operation, indexed['operations'][frontier_operation])
                        _, proposal = recovery_frontier.evidence(directory, ledger_value, indexed['operations'][frontier_operation])
                        selected = selected_attestation(root, frontier_operation, proposal, target_digest)
                        if selected is None and override is None:
                            raise ContractError('Target capable runtime has no renewed bank-bound quiescence attestation')
                        if override is None and selected is not None and not set(target_artifact.get('layout_required_routes', ())).issubset(selected['declared_invocation_routes']):
                            raise ContractError('Target capable reader has an invocation route omitted from quiescence')
                if not capabilities.issubset(TOKENS):
                    raise ContractError('A private-state producer returned an unknown capability')
                requirements[str(bank)] = sorted(capabilities)
                pins.append({'repo_root': str(root), 'bank_path': str(bank), 'recovery_frontier': token,
                    'requirements': sorted(capabilities)})
            except (ContractError, OSError, ValueError, TypeError, KeyError) as exc:
                unknown.append({'source': 'private-state', 'location': str(root),
                    'reason': 'private state is missing, inaccessible, interrupted, malformed or changed'})
    required = set().union(*(set(values) for values in requirements.values())) if requirements else set()
    if isinstance(target_artifact, dict) and target_artifact.get('already_active_verified') is True:
        from . import installation as install
        receipt_path = Path(target_artifact['reader_receipt_path'])
        other = install.read_receipt(receipt_path.parent)
        if (other is None or digest(state.read_bytes(receipt_path)) != target_artifact['reader_receipt_sha256']
                or other['source']['artifact_digest'] != target_artifact['artifact_digest']):
            raise ContractError('Selected preserved capable runtime receipt changed or is no longer active')
        install.verify_targets(other['targets'], source=other['source'])
    missing = required - supported
    if operation == 'uninstall' and required:
        # An explicit equivalent already-active runtime must be passed as target.
        equivalent = isinstance(target_artifact, dict) and target_artifact.get('already_active_verified') is True
        if not equivalent:
            missing.update(required)
    return {'requirements': requirements, 'required_capabilities': sorted(required),
        'target_capabilities': sorted(supported), 'missing_capabilities': sorted(missing),
        'unknown_state': unknown, 'evidence_pins': {'inventory': inventory_pins, 'banks': pins},
        'decision': 'compatible' if not unknown and not missing else 'pending',
        'operation': operation, 'private_evidence_preserved': True}


def require_compatible(installation, target_artifact, operation, expected=None):
    current = required_private_state_capabilities(installation, target_artifact, operation)
    if current['decision'] != 'compatible':
        raise ContractError('Private-state runtime compatibility is pending; preserve the capable active runtime and evidence')
    if expected is not None and current != expected:
        raise ContractError('Private-state inventory changed before runtime activation; quiesce and inspect again')
    return current


@contextmanager
def activation_guard(installation, target_artifact, operation, transition=None):
    """Retain the common source barriers through activation and exact rollback."""
    existing = _barriers.get()
    if existing is not None:
        if transition is not None:
            transition()
        proof = require_compatible(installation, target_artifact, operation)
        yield proof
        require_compatible(installation, target_artifact, operation, expected=proof)
        return
    roots, unknown, _ = repository_inventory(installation)
    if unknown:
        raise ContractError('Private-state bank inventory is unavailable; preserve the active runtime')
    from .turn_sources import source_barrier
    with ExitStack() as stack:
        for root in sorted(roots, key=str):
            settings = repository_config.recovery_settings(root)
            if (root / settings['bank_root']).is_dir():
                stack.enter_context(source_barrier(root, exclusive=True))
        bound = _barriers.set({str(root) for root in roots})
        try:
            if transition is not None:
                transition()
            proof = require_compatible(installation, target_artifact, operation)
            yield proof
            require_compatible(installation, target_artifact, operation, expected=proof)
        finally:
            _barriers.reset(bound)


def public_summary(value):
    """Never serialize private ledger/source/proof digests into lifecycle stdout."""
    return {key: value[key] for key in ('decision', 'required_capabilities', 'target_capabilities', 'missing_capabilities')} | {
        'observed_banks': len(value['requirements']), 'unknown_private_states': len(value['unknown_state'])}


def prepare_lifecycle(plan, check=False):
    """Consume explicit private renewals; check mode never publishes state."""
    from . import admission, recovery_frontier
    selected = plan.get('preserved_private_state_reader') if plan.get('operation') == 'uninstall' else plan['source']
    target = dict(selected) if selected is not None else None
    supplied = plan.get('layout_quiescence', []) or []
    if not isinstance(supplied, list) or len(supplied) > 1024 or any(not isinstance(path, str) for path in supplied):
        raise ContractError('Lifecycle quiescence inputs must be explicit protected-file paths')
    if target is None and supplied:
        raise ContractError('Uninstall renewal needs an actually preserved capable reader')
    if target is None:
        return None
    routes = list(target.get('reader_invocation_routes', ()))
    if not routes:
        routes = [action['path'] for action in plan['actions'] if action.get('changed')
            and action.get('after') is not None
            and action['kind'] in {'wrapper', 'hook-link', 'hook-config', 'copy', 'link', 'activation'}]
    target['layout_required_routes'] = routes
    overrides = {}
    for path in supplied:
        value = admission.read_input(path, 65536)
        if not isinstance(value, dict) or set(value) != {'attestation', 'review'}:
            raise ContractError('Lifecycle renewal uses the exact attestation/independent-review envelope')
        operation = value['attestation'].get('operation_id')
        repo = value['attestation'].get('repo_root')
        if operation in overrides and overrides[operation] != value:
            raise ContractError('Conflicting runtime quiescence declarations cannot select a winner')
        recovery_frontier.validate_quiescence(repo, value,
            runtime_artifact_digest=target['artifact_digest'], required_routes=routes, publish=not check)
        overrides[operation] = value
    if check:
        target['layout_quiescence_overrides'] = overrides
    return target
