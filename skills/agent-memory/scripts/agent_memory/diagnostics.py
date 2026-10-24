"""Read-only runtime and repository diagnostics, without setup or repair writes."""
from __future__ import annotations

import argparse
from collections import Counter
import ctypes
import fcntl
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys

from . import artifact, bank_contract, bank_writes, developer_identity, installation
from . import local_registry, project_identity, projections, repository_config
from .frontmatter import ContractError
from .identity_git import git
from .remote_identity import normalize

ERRORS = (OSError, ValueError, TypeError, KeyError, UnicodeError, AttributeError,
          RecursionError, subprocess.SubprocessError)
SCHEMA = 'agent-memory/diagnostic-result-v1'
RECOVER = 'Use a trusted complete package outside the affected targets: python3 -I "$SKILL_DIR/scripts/installer.py" recover; inspect its result before retrying.'


class DiagnosticFailure(ContractError):
    """Only fixed diagnostic text and validated numeric metadata may be exposed."""


def read(path, maximum=2**20):
    """Never follow a redirected diagnostic record or load an unlimited file."""
    path = Path(path)
    installation.require(not any(p.is_symlink() for p in (path, *path.parents)),
                         'Diagnostic path crosses a symlink')
    installation.require(path.is_file(), 'Diagnostic input is not a regular file')
    with path.open('rb') as stream:
        raw = stream.read(maximum + 1)
    installation.require(len(raw) <= maximum, 'Diagnostic input exceeds its reader bound')
    return raw


def record(path, maximum=2**20):
    return developer_identity.load(read(path, maximum))


def source(pack):
    pack = Path(pack).resolve(strict=True)
    inspected = artifact.inspect(pack)
    root = pack.parents[1] if pack.parent.name == 'skills' else pack
    top = git(root, 'rev-parse', '--show-toplevel', optional=True)
    versioned = top is not None and Path(top).resolve() == root
    unavailable = top is None and os.path.lexists(root / '.git')
    revision = git(root, 'rev-parse', '--verify', 'HEAD') if versioned else None
    dirty = bool(git(root, 'status', '--porcelain', '--untracked-files=all')) if versioned else None
    return {'path': str(root), 'pack': str(pack), 'version': inspected['version'],
            'revision': revision, 'dirty': dirty,
            'cleanliness': 'unavailable' if unavailable else ('dirty' if dirty else ('clean' if versioned else 'unversioned')),
            'artifact_digest': inspected['artifact_digest'], 'compatibility': inspected['compatibility']}


def channel(value):
    # Do not echo credential-bearing remote URLs from an untrusted receipt.
    remote = value.get('remote')
    return {'kind': value.get('kind'), 'ref': value.get('ref'),
            'remote': normalize(remote) if remote is not None else None}


class Report:
    def __init__(self, operation):
        self.value = {'schema': SCHEMA, 'operation': operation, 'writes_performed': False,
                      'network_performed': False, 'repairs_performed': False, 'checks': []}

    def add(self, component, state, detail, repair=None, path=None):
        item = {'component': component, 'state': state, 'detail': detail}
        if repair:
            item['repair'] = repair
        if path is not None:
            item['path'] = str(path)
        self.value['checks'].append(item)

    def attempt(self, component, function, repair, path=None):
        try:
            return function()
        except DiagnosticFailure as exc:
            self.add(component, 'failure', str(exc), repair, path)
            return None
        except ERRORS:
            # Exceptions may contain remote URLs, private record bodies, or keys.
            # Keep source data at its original path; report a bounded component.
            self.add(component, 'failure', 'Cannot verify this component from its current local evidence.', repair, path)
            return None

    def finish(self):
        self.value['failures'] = [v for v in self.value['checks'] if v['state'] == 'failure']
        self.value['warnings'] = [v for v in self.value['checks'] if v['state'] == 'warning']
        self.value['ok'] = not self.value['failures']
        return self.value


def receipt_view(report):
    state, _ = installation.locations()
    path = state / 'receipt.json'
    active = None
    report.value['installation'] = {'state': 'not_installed', 'receipt_path': str(path),
                                    'receipt_schema': None, 'mode': None, 'enabled_adapters': [],
                                    'health': 'not_audited', 'update_availability': 'not_cached_or_fetched',
                                    'purge_transactions': 'not_scanned'}
    if os.path.lexists(path):
        raw = record(path)
        installation.require(isinstance(raw, dict), 'Invalid receipt')
        active = installation.read_receipt(state)
        installation.require(raw.get('schema') == installation.RECEIPT_SCHEMA, 'Unknown receipt schema')
        report.value['installation'].update(
            state='active' if active else 'uninstalled', receipt_schema=raw['schema'],
            mode=raw.get('mode'), enabled_adapters=raw.get('enabled_adapters', []) if active else [],
            channel=channel(raw['source']), recorded_source={key: raw['source'].get(key) for key in
                ('path', 'pack', 'version', 'revision', 'artifact_digest', 'dirty')},
            preserved_target_count=len(raw.get('preserved', [])))
    journal = state / 'journal.json'
    if os.path.lexists(journal):
        read(journal)
        data = installation.read_journal(state)
        report.value['installation']['transaction'] = {'operation': data.get('operation'), 'state': data['status']}
        if data['status'] not in {'complete', 'rolled-back'}:
            report.add('runtime.transaction', 'warning', 'Runtime activation is pending; targets may be partially switched.', RECOVER, journal)
        elif data['status'] == 'complete' and installation.fingerprint(path) != data.get('receipt_identity'):
            report.add('runtime.receipt', 'failure', 'Receipt differs from its completed transaction.',
                       'Preserve the receipt, journal and backups; reconcile ownership before install/update/removal.', path)
    return active


def lightweight_bank(root):
    """Observe candidate locators, not full-bank validity or proof of absence.

    No repository-wide physical search and no routed content reads. The full
    established one-bank locator remains doctor's authority.
    """
    top = git(root, 'rev-parse', '--show-toplevel', optional=True)
    root = Path(top).resolve() if top else root
    explicit = repository_config.local_value(root, 'agent-memory.bankRoot') if top else None
    candidates = {explicit or 'memory-bank'}
    if explicit:
        repository_config.validate_root(explicit)
    listed = git(root, 'ls-files', '--cached', '-z', '--', '*/MEMORY.md', 'MEMORY.md') if top else ''
    paths = [p for p in listed.split('\0') if p]
    installation.require(len(paths) <= repository_config.MAX_ROUTERS, 'Too many possible bank locators')
    for name in paths:
        relative = Path(name)
        installation.require(not relative.is_absolute() and '..' not in relative.parts, 'Unsafe router path')
        if relative.name == 'MEMORY.md' and not any(p.startswith('.') for p in relative.parts):
            path = root / relative
            if path.exists() and repository_config.router_kind(read(path, 16384)):
                candidates.add(relative.parent.as_posix())
    found = []
    for relative in sorted(candidates):
        router = root / relative / 'MEMORY.md'
        if os.path.lexists(router):
            metadata, routes, _ = bank_contract.parse_router(read(router, 16384).decode('utf-8'))
            found.append((relative, metadata, routes))
    installation.require(len(found) <= 1 and (not explicit or not found or found[0][0] == explicit),
                         'Competing bank locators')
    bank = {'state': 'not_located', 'path': str(root / (explicit or 'memory-bank')),
            'schema': None, 'project_id': None, 'counts_by_lifecycle': None,
            'counts_scope': 'not_scanned', 'discovery': 'configured/default-and-indexed-routers-only',
            'absence_verified': False, 'uniqueness_verified': False,
            'pending_capture': 'not_assessed', 'projection_position': 'no_global_projection_registry',
            'index_freshness': 'not_assessed'}
    if found:
        relative, metadata, routes = found[0]
        bank.update(state='router_observed', path=str(root / relative), schema=metadata['schema'],
                    project_id=metadata['project-id'], router_updated=metadata['router-updated'],
                    root_routes_by_lifecycle=dict(Counter(r['lifecycle'] for r in routes)))
    bank['pending_transaction'] = os.path.lexists(Path(bank['path']) / '.agent-memory-transaction.md')
    bank['writer_marker_present'] = os.path.lexists(Path(bank['path']) / '.agent-memory.lock')
    return root, bank


def identities(report, root, bank):
    # Bounded registries; selection is observed, never bootstrapped.
    def developer():
        _, path, lock = developer_identity.paths()
        data = developer_identity.validate(record(path)) if path.exists() else developer_identity.empty()
        report.value['developer'] = {'id': data['selected'], 'resolved': data['selected'] is not None,
                                     'pending_operation': os.path.lexists(lock)}
        if data['selected'] is None:
            report.add('identity.developer', 'warning', 'No developer is selected; file diagnostics remain available.',
                       'Use the identity operation to preview and select an explicit stable developer ID.')
    report.attempt('identity.developer', developer, 'Preserve the local developer registry; inspect it through the identity operation.')

    def project():
        _, path, lock = local_registry.paths('projects')
        data = project_identity.validate(record(path)) if path.exists() else {'projects': {}, 'checkouts': {}}
        common = git(root, 'rev-parse', '--git-common-dir', optional=True)
        location = (root / common).resolve() if common else root
        identity = location.stat()
        entry = data['checkouts'].get(f'{identity.st_dev}:{identity.st_ino}')
        project_id = bank.get('project_id') or (entry['project_id'] if entry else None)
        matches = entry is not None and project_id == entry['project_id'] and project_id in data['projects']
        report.value['project'] = {'id': project_id, 'checkout_binding_matches': matches,
                                   'authority_verified': False, 'pending_operation': os.path.lexists(lock)}
        if not matches:
            report.add('identity.project', 'warning', 'Project binding is missing or differs; no identity is inferred or created.',
                       'Inspect the identity operation and explicitly bind/link/fork only after reviewing the repository evidence.')
    report.attempt('identity.project', project, 'Preserve the project registry and bank; use the identity operation for explicit reconciliation.')


def permission_check(report, path, component):
    """Check existing nodes only. No mkdir, probe, lock acquisition or chmod."""
    node = Path(path)
    installation.require(not any(p.is_symlink() for p in (node, *node.parents)), 'Redirected directory')
    while not node.exists():
        node = node.parent
    mode = node.stat().st_mode
    if not node.is_dir() or not mode & 0o444 or not mode & 0o222 or not mode & 0o111 or not os.access(node, os.R_OK | os.W_OK | os.X_OK):
        report.add(component, 'failure', 'Existing directory/ancestor is not readable, writable and searchable.',
                   'Review this directory’s ownership and permissions; grant only the intended developer access, then rerun doctor.', node)
    else:
        report.add(component, 'pass', 'Existing directory/ancestor has read, write and search access; no write probe performed.', path=node)


def runtime_doctor(report, active):
    state, _ = installation.locations()
    permission_check(report, state, 'runtime.permissions')
    lock = state / 'lock'
    descriptor = None
    report.value['pending_purges'] = []
    try:
        installation.require(active is None or os.path.lexists(lock), 'An active installation is missing its lifecycle lock')
        if os.path.lexists(lock):
            read(lock, 65536)
            descriptor = os.open(lock, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
            installation.require(stat.S_ISREG(os.fstat(descriptor).st_mode) and os.fstat(descriptor).st_nlink == 1,
                                 'Unsafe lifecycle lock')
            try:
                fcntl.flock(descriptor, fcntl.LOCK_SH | fcntl.LOCK_NB)
            except BlockingIOError:
                report.add('runtime.lock', 'warning', 'A lifecycle writer is active; runtime ownership checks were deferred.',
                           'Wait for the lifecycle operation to finish, then rerun doctor. Do not steal the lock.', lock)
                return
            installation.require(record(lock).get('schema') == 'agent-memory/lifecycle-lock-v1', 'Unknown lock schema')
        report.add('runtime.lock', 'pass', 'No lifecycle writer held the existing lock during inspection.', path=lock)
        # Re-read under the existing shared lock, then compare again before releasing it.
        # Never combine ownership from a receipt that changed across this inspection.
        before = {name: installation.fingerprint(state / name) for name in ('receipt.json', 'journal.json')}
        active = receipt_view(report)
        owner_path = state / 'owner.json'
        if os.path.lexists(owner_path):
            def check_owner():
                read(owner_path)
                installation.read_owner(state)
                report.add('runtime.owner', 'pass', 'Existing lifecycle owner metadata is readable and accepted by the lifecycle lock protocol; PID and age do not grant lock ownership.', path=owner_path)
            report.attempt('runtime.owner', check_owner,
                           'Preserve this owner record and the lifecycle lock. Reconcile the malformed or redirected metadata before retrying a lifecycle operation; do not reset or steal ownership based on PID or age.', owner_path)
        if active:
            def check_source():
                observed = source(active['source']['pack'])
                report.value['installed_source'] = observed
                expected = active['source']
                matches = all(observed[k] == expected.get(k) for k in ('path', 'pack', 'version', 'revision', 'dirty', 'artifact_digest'))
                # Unversioned source uses the install receipt's explicit marker.
                if observed['revision'] is None and expected.get('revision') == 'unversioned':
                    matches = all(observed[k] == expected.get(k) for k in ('path', 'pack', 'version', 'dirty', 'artifact_digest'))
                if expected['kind'] == 'managed':
                    installation.require(git(expected['path'], 'remote', 'get-url', 'origin') == expected['remote'] and
                                         git(expected['path'], 'symbolic-ref', '--short', 'HEAD') == expected['ref'],
                                         'Managed source channel differs')
                if not matches:
                    report.add('runtime.source', 'warning' if expected['kind'] == 'development' else 'failure',
                               'Source differs from its recorded installation.',
                               'For a developer source, inspect edits and preview reinstall; for managed source, preserve edits and reconcile the recorded channel before update.', expected['path'])
                else:
                    report.add('runtime.source', 'pass', 'Source revision, artifact and recorded channel match.', path=expected['path'])
            report.attempt('runtime.source', check_source, 'Use a trusted outside source helper; preserve the source and receipt and preview reinstall after reconciling the exact channel.', active['source'].get('path'))
            for target in active['targets']:
                if target['kind'] == 'source':
                    continue  # Source identity and channel are checked above, without copying Git metadata.
                path = Path(target['path'])
                def check_target():
                    if target['kind'] == 'activation':
                        raw = read(path)
                        try:
                            installation.activation(raw, target['block_hash'])
                        except (ContractError, UnicodeError):
                            raise DiagnosticFailure('Activation block is missing, malformed, duplicated or differs from its recorded block hash; outside text is preserved.') from None
                    else:
                        observed = installation.fingerprint(path)
                        if observed != target['identity']:
                            expected_mode = (target['identity'] or {}).get('mode')
                            if (target['kind'] == 'wrapper' and observed and observed.get('kind') == 'file'
                                    and type(expected_mode) is int and 0 <= expected_mode <= 0o7777
                                    and observed.get('mode') != expected_mode):
                                raise DiagnosticFailure('Wrapper mode is ' + oct(observed['mode']) + '; recorded mode is ' + oct(expected_mode) + '.')
                            raise DiagnosticFailure('Owned ' + target['kind'] + ' target is missing, redirected, or has different bytes or modes than its recorded identity.')
                        if target['kind'] in {'link', 'copy'}:
                            if artifact.inspect(path)['artifact_digest'] != target['artifact_digest']:
                                raise DiagnosticFailure('Client artifact digest differs from the installation receipt.')
                        if target['kind'] == 'wrapper':
                            installation.require(read(path) == installation.wrapper(Path(active['source']['pack'])) and
                                                 os.access(path, os.X_OK), 'Wrapper is not the owned executable entrypoint')
                    report.add('runtime.target.' + target['kind'], 'pass', 'Owned target is intact; surrounding activation text may differ.', path=path)
                report.attempt('runtime.target.' + target['kind'], check_target,
                               'Preserve user edits and backups; compare this path with the ownership receipt. Preview reinstall from a trusted helper after explicit reconciliation; do not force replacement.', path)
                if target['kind'] != 'activation':
                    report.attempt('runtime.target_permissions', lambda: permission_check(report, path.parent, 'runtime.target_permissions'),
                                   'Inspect this target parent’s ownership and access; preserve existing files and preview reinstall after resolving permissions.', path.parent)
            for backup in active.get('backups', []):
                backup_path = Path(backup['path'])
                report.attempt('runtime.backup_permissions', lambda: permission_check(report, backup_path.parent, 'runtime.backup_permissions'),
                               'Preserve existing backups; restore intended owner access to their directory before lifecycle changes.', backup_path.parent)
        else:
            report.add('runtime.ownership', 'warning', 'No active installation receipt; source and bank diagnostics still work.',
                       'Use the installation operation to preview an installation if an installed CLI is wanted.')
        purges = state / 'purges'
        if os.path.lexists(purges):
            installation.require(purges.is_dir() and not purges.is_symlink(), 'Unsafe purge record directory')
            paths = []
            for path in purges.iterdir():
                paths.append(path)
                installation.require(len(paths) <= 1024, 'Purge journal inventory exceeds its bound')
            from .purging import SCHEMA as PURGE_SCHEMA, MAX_RECORD
            for path in sorted(paths):
                if path.suffix != '.json':
                    continue
                def check_purge():
                    value = record(path, MAX_RECORD)
                    installation.require(value.get('schema') == PURGE_SCHEMA and value.get('status') in {'staging', 'detached', 'complete'}, 'Unknown purge journal')
                    if value['status'] != 'complete':
                        report.value['pending_purges'].append({'journal_path': str(path),
                            'repository': value['preview']['repository'], 'path': value['preview']['path'],
                            'state': value['status']})
                        report.add('data.purge', 'warning', 'A separately confirmed purge is unfinished; runtime recovery does not resume data deletion.',
                                   'Inspect the original exact preview and retained journal. Resume only the same previously authorized purge confirmation; doctor grants no deletion consent.', path)
                    else:
                        report.add('data.purge', 'pass', 'Historical purge journal is complete; no data deletion performed.', path=path)
                report.attempt('data.purge', check_purge, 'Preserve this purge record and quarantine. Review the original confirmation before resuming; do not remove or reset it.', path)
        purge_failed = any(c['component'] == 'data.purge' and c['state'] == 'failure' for c in report.value['checks'])
        report.value['installation']['purge_transactions'] = ('inspection_failed' if purge_failed else
            ('pending' if report.value['pending_purges'] else 'none_pending'))
        installation.require(before == {name: installation.fingerprint(state / name) for name in before},
                             'Lifecycle records changed during inspection')
    finally:
        if descriptor is not None:
            os.close(descriptor)


def bank_doctor(report, root, projection_path):
    if any(v['repository'] == str(root) for v in report.value.get('pending_purges', [])):
        report.value['bank']['state'] = 'purge_pending'
        report.add('bank.purge', 'warning', 'This repository has an unfinished separately confirmed data purge; bank validation and initialization are deferred.',
                   'Preserve the original bank/quarantine and purge journal; follow only the original explicitly authorized purge operation.')
        return
    bank = repository_config.bank_path(root)
    report.value['bank']['path'] = str(bank)
    report.value['bank']['uniqueness_verified'] = True
    permission_check(report, bank, 'bank.permissions')
    if not bank.exists():
        report.value['bank'].update(state='not_initialized', absence_verified=True)
        report.add('bank.setup', 'warning', 'No bank was found by the established repository locator.',
                   'Use the initialization operation only when useful project evidence is available; diagnostics do not require a bank.')
        installation.require(projection_path is None, 'A selected projection has no canonical bank to verify against')
        return
    transaction = bank / '.agent-memory-transaction.md'
    lock = bank / '.agent-memory.lock'
    if os.path.lexists(transaction) or os.path.lexists(lock):
        report.add('bank.transaction', 'warning', 'Bank is busy or interrupted; no stable content inventory was claimed.',
                   'Wait for a live writer. For an interrupted write, follow the writes recovery operation with its original operation and receipt; preserve markers and edits.', transaction if os.path.lexists(transaction) else lock)
        report.value['bank']['state'] = 'busy_or_interrupted'
        return
    checked, hashes = bank_writes.snapshot(root)
    report.value['bank'].update(state='inspecting', source_stability_verified=False, schema='agent-memory/v1', project_id=checked['project_id'],
        counts_by_lifecycle=dict(Counter(e['lifecycle'] for e in checked['entries'])),
        counts_scope='validated_canonical_entries', source_files=len(hashes),
        index_freshness='canonical_topic_routes_validated', pending_capture='not_centrally_registered')
    report.add('bank.schema_and_readability', 'pass', 'Native schema, routes, topic indices, record/history/tombstone formats and readable canonical files are valid.', path=bank)
    if projection_path:
        def check_projection():
            value = projections.value(record(projection_path, projections.MAX_BYTES))
            active = {(e.get('id'), e.get('version'), e['path']) for e in checked['entries'] if e['lifecycle'] == 'active'}
            installation.require(value['project_id'] == checked['project_id'] and value['source_hashes'] == hashes and
                                 all((v['id'], v['version'], v['path']) in active for v in value['records']),
                                 'Derived projection is stale or contains inactive/foreign references')
            report.add('bank.projection', 'pass', 'Explicit projection binds current source hashes and active canonical references; applicability remains a retrieval decision.', path=projection_path)
            report.value['bank']['projection_position'] = 'explicit_projection_current'
        report.attempt('bank.projection', check_projection,
                       'Discard this derived projection and rebuild from current canonical Markdown through the projections operation; retain canonical memory and do not edit cached references into validity.', projection_path)
    else:
        report.add('bank.projection', 'not_enabled', 'No projection file was selected; derived references have no global authoritative registry.')
    installation.require(bank_writes.snapshot(root)[1] == hashes, 'Bank changed during diagnostics')
    report.value['bank'].update(state='validated', source_stability_verified=True)


def run(operation, repo='.', projection=None):
    report = Report(operation)
    active = report.attempt('runtime.receipt', lambda: receipt_view(report),
                            'Preserve installation state and backups. Inspect the external receipt/journal from a trusted package; do not guess or reset ownership.')
    if operation in {'version', 'doctor'}:
        current = report.attempt('runtime.executing_artifact', lambda: source(artifact.PACK),
                                 'Obtain a complete trusted portable package and run its diagnostic helper; preserve the affected installation.', artifact.PACK)
        report.value['executing_source'] = current
        if current is not None:
            report.add('runtime.executing_artifact', 'pass', 'Executing package inventory, route graph and declared compatibility are valid.', path=artifact.PACK)
    if operation == 'version':
        return report.finish()
    root = Path(repo).expanduser().resolve(strict=True)
    installation.require(root.is_dir(), 'Repository must be an existing directory')
    located = report.attempt('bank.locator', lambda: lightweight_bank(root),
                             'Inspect repository bankRoot configuration and existing routers; preserve all possible banks and use doctor before initialization.', root)
    if located:
        root, bank = located
    else:
        bank = {'state': 'unresolved', 'path': None, 'project_id': None, 'counts_by_lifecycle': None}
    report.value.update(repository=str(root), bank=bank)
    identities(report, root, bank)
    if operation == 'doctor':
        report.add('prerequisite.python', 'pass' if sys.version_info >= (3, 11) else 'failure',
                   'Python ' + '.'.join(map(str, sys.version_info[:3])), 'Use Python 3.11 or newer.')
        report.add('prerequisite.git', 'pass' if shutil.which('git') else 'failure',
                   'Git is available.' if shutil.which('git') else 'Git is unavailable.', 'Install Git for governed repository operations; Markdown remains readable.')
        library = ctypes.CDLL(None)
        native = (sys.platform == 'darwin' and hasattr(library, 'renamex_np')) or (sys.platform.startswith('linux') and hasattr(library, 'renameat2'))
        report.add('filesystem.native_api', 'pass' if native else 'failure',
                   'Native no-replace/exchange API is available; actual per-filesystem support is established by lifecycle activation, not a diagnostic write probe.' if native else 'Required native no-replace/exchange API is unavailable.',
                   'Use a supported macOS/Linux runtime and filesystem; preserve receipt and backups. Do not replace activation with a rename-away fallback.')
        report.add('distribution.trust', 'warning', 'The initial Git channel has no pinned signature verification; an artifact digest establishes identity, not publisher authenticity.',
                   'Use an operator-trusted source and review the recorded remote/ref; signed immutable distribution is a later channel capability.')
        report.attempt('runtime.inspection', lambda: runtime_doctor(report, active), RECOVER)
        report.attempt('bank.inspection', lambda: bank_doctor(report, root, projection),
                       'Preserve canonical files, markers and operation receipts. Follow the bank inspection/writes operation; rebuild a selected stale projection from current canonical sources instead of repairing cached content.', bank.get('path'))
        if report.value['bank'].get('state') == 'validated':
            def full_identity():
                value = project_identity.status(root)
                report.value['project']['authority_verified'] = value['resolved']
                if not value['resolved']:
                    report.add('identity.authority', 'warning', 'Full repository/project authority remains unresolved.',
                               'Use the identity operation to inspect sanitized repository evidence and explicitly reconcile binding or authority; do not infer a new project.')
                else:
                    report.add('identity.authority', 'pass', 'Local registry, checkout, bank and canonical remote identify one project.')
            report.attempt('identity.authority', full_identity, 'Inspect explicit project binding and authority with the identity operation; preserve existing memory.')
        for name, detail in (
            ('authoritative_event_log', 'Markdown/Git is authoritative; optional historical records are validated with the bank.'),
            ('vector_index', 'Native recall reads canonical Markdown; no vector service is enabled.'),
            ('migration', 'Native v1 is directly readable; no bank migrator is bundled or implicitly run.'),
            ('optional_services', 'GitHub publication is opt-in per operation; diagnostics perform no authentication, API calls, or provider availability claim.')):
            report.add(name, 'not_enabled', detail)
    result = report.finish()
    if operation == 'doctor':
        result['installation']['health'] = 'failures_detected' if result['failures'] else 'inspected_with_qualifications'
    return result


def execute(args):
    try:
        value = run(args.command, getattr(args, 'repo', '.'), getattr(args, 'projection', None))
    except ERRORS:
        report = Report(args.command)
        report.add('diagnostic.inputs', 'failure', 'Diagnostic input or local layout cannot be inspected safely.',
                   'Use an existing repository directory and canonical readable configuration paths; rerun this helper from a trusted complete package.')
        value = report.finish()
    print(json.dumps(value, indent=2))
    return 0 if value['ok'] else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('version', 'status', 'doctor'):
        command = commands.add_parser(name)
        command.add_argument('--json', action='store_true', help='Emit structured JSON (also the default)')
        if name != 'version':
            command.add_argument('--repo', default='.')
        if name == 'doctor':
            command.add_argument('--projection', help='Explicit optional reference-projection JSON to check without writing an audit')
    return execute(parser.parse_args(argv))
