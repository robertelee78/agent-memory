"""Explicit source-bound format conversion; runtime updates never invoke it."""
from __future__ import annotations

import argparse
import importlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys
import tempfile
import uuid

from . import artifact, bank_contract as bank, bank_transactions as tx, bank_writes as writes
from . import local_registry as storage, migration_contract, repository_config, scope_binding, trust_paths, security
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError, read

PACK = Path(__file__).resolve().parents[2]
SCHEMA = 'agent-memory/migration-proposal-v1'
MAX_FILES = 100000
MAX_FILE = 65536
MAX_BYTES = 64 * 1024 * 1024
MAX_PLAN = 32 * 1024 * 1024
MAX_RECORD = 8 * MAX_BYTES + MAX_PLAN
FIELDS = {'schema', 'operation_id', 'repo_root', 'bank_root', 'project_id', 'scope_context',
          'source_schema', 'target_schema', 'artifact_digest', 'catalog_sha256', 'chain',
          'source_hashes', 'source_modes', 'resulting_hashes', 'resulting_modes', 'preservation_sha256',
          'kind', 'parent'}


def result(operation, **values):
    return {'schema': 'agent-memory/migration-result-v1', 'operation': operation,
            'publication_verified': False, 'identity_authenticated': False,
            'semantic_truth_verified': False, **values}


def router_metadata(raw):
    """Locate a format-family header, not validate or adopt its memory."""
    if not isinstance(raw, bytes):
        raise ContractError('Migration needs an actual bounded router observation')
    if not re.search(br'(?m)^schema:[ \t]*[\"\x27]?agent-memory/', raw[:16384]):
        return None
    if len(raw) > 16384 or not raw.startswith(b'---\n'):
        raise ContractError('Migration router requires bounded LF frontmatter at byte zero')
    text = raw.decode('utf-8')
    end = text.find('\n---\n', 4)
    if end < 0:
        raise ContractError('Migration router header is incomplete')
    keys = {line.split(':', 1)[0] for line in text[4:end].splitlines() if not line.startswith(' ')}
    metadata, _ = read(text, allowed=keys)
    if (not isinstance(metadata.get('schema'), str)
            or not re.fullmatch(r'agent-memory/[A-Za-z0-9_.-]{1,128}', metadata['schema'])
            or not uuid_valid(metadata.get('project-id'))):
        raise ContractError('Migration router needs an explicit format and canonical project UUID')
    return metadata


def locate(repo):
    root = Path(repo).expanduser().resolve(strict=True)
    settings = repository_config.settings(root, router_reader=router_metadata)
    target = root / settings['bank_root']
    bank.bank_root(target)
    if target.is_symlink() or not target.is_dir():
        raise ContractError('Migration requires the existing canonical bank; it never initializes one')
    return root, target


def bounded_files(files):
    if (not isinstance(files, dict) or not files or 'MEMORY.md' not in files or len(files) > MAX_FILES
            or any(not isinstance(body, str) or len(body.encode()) > MAX_FILE for body in files.values())
            or sum(len(body.encode()) for body in files.values()) > MAX_BYTES):
        raise ContractError('Complete migration source/result exceeds explicit file/byte bounds; nothing is truncated')
    for path in files:
        if path != 'MEMORY.md':
            bank.path_value(path)
    bank.file_topology(files)
    return files


def read_regular(directory, relative, limit):
    """Read through pinned no-follow directory descriptors, including ancestors."""
    parts = PurePosixPath(relative).parts
    if not parts or PurePosixPath(relative).is_absolute() or any(p in {'.', '..'} for p in parts):
        raise ContractError('Migration paths must remain relative to their exact root')
    descriptors = []
    try:
        descriptors.append(os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW))
        for part in parts[:-1]:
            descriptors.append(os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptors[-1]))
        fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=descriptors[-1])
        try:
            before = os.fstat(fd)
            if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_size > limit:
                raise ContractError('Migration reads bounded ordinary single-link files only')
            chunks, size = [], 0
            while size <= limit:
                block = os.read(fd, min(8192, limit + 1 - size))
                if not block:
                    break
                chunks.append(block); size += len(block)
            after = os.fstat(fd)
            if (size != before.st_size or size > limit or
                    (before.st_size, before.st_mtime_ns, before.st_ctime_ns) !=
                    (after.st_size, after.st_mtime_ns, after.st_ctime_ns)):
                raise ContractError('Migration source changed during its bounded read')
            return b''.join(chunks), before.st_mode & 0o777
        finally:
            os.close(fd)
    finally:
        for fd in reversed(descriptors):
            os.close(fd)


def snapshot(target, *, owner=None, marker=None):
    for name, expected in (('.agent-memory.lock', owner), (tx.MARKER, marker)):
        path = target / name
        if os.path.lexists(path):
            raw, _ = read_regular(target, name, MAX_FILE)
            if expected is None or raw != expected:
                raise ContractError('Bank is busy/interrupted; use the matching owned recovery operation')
    files, modes, physical, total = {}, {}, [], 0
    def unreadable(_):
        raise ContractError('Migration inventory is unreadable; no source absence is inferred')
    for directory, names, leaves in os.walk(target, followlinks=False, onerror=unreadable):
        parent = Path(directory)
        for name in [*names, *leaves]:
            node = parent / name
            if node.is_symlink():
                raise ContractError('Migration cannot follow bank symlinks')
            physical.append(node.relative_to(target).as_posix())
        names[:] = [name for name in names if not bank.runtime_path((parent / name).relative_to(target))]
        for name in leaves:
            relative = (parent / name).relative_to(target).as_posix()
            if bank.runtime_path(relative):
                continue  # Preserved runtime aids are never canonical learning.
            if relative != 'MEMORY.md':
                bank.path_value(relative)
            raw, mode = read_regular(target, relative, MAX_FILE)
            files[relative], modes[relative] = raw.decode('utf-8'), mode
            total += len(raw)
            if len(files) > MAX_FILES or total > MAX_BYTES:
                raise ContractError('Migration source inventory exceeds its complete bound')
    bank.portable_aliases(physical)
    bounded_files(files)
    return files, modes


def catalog():
    trust_paths.source(PACK)
    measured = artifact.inspect(PACK)
    if measured['migration_catalog'] is None:
        raise ContractError('This trusted runtime bundles no explicit migration catalog')
    return measured


def invoke(entry, files):
    module_path = PACK / entry['module']
    if digest(module_path.read_bytes()) != entry['sha256']:
        raise ContractError('Bundled migration code changed after catalog inspection')
    module = importlib.import_module('agent_memory.' + PurePosixPath(entry['module']).stem)
    function = getattr(module, entry['function'])
    if not callable(function):
        raise ContractError('Bundled migration entry is not callable')
    return function(dict(files))


def validate_codec(measured, schema, files):
    bounded_files(files)
    codec = measured['migration_catalog']['codecs'].get(schema)
    if codec is None:
        raise ContractError('No bundled validator for the requested migration schema')
    value = invoke(codec, files)
    header = router_metadata(files['MEMORY.md'].encode())
    if (header is None or header['schema'] != schema or not isinstance(value, dict)
            or set(value) != {'project_id', 'preservation_sha256'} or value['project_id'] != header['project-id']
            or not uuid_valid(value['project_id']) or not isinstance(value['preservation_sha256'], str)
            or not re.fullmatch('[0-9a-f]{64}', value['preservation_sha256'])):
        raise ContractError('Migration codec did not validate exact schema, project and preservation projection')
    return value


def transform(measured, source, target, files):
    chain = migration_contract.route(measured['migration_catalog'], source, target)
    if chain is None:
        raise ContractError('No complete bundled migration chain supports this source and target')
    original = validate_codec(measured, source, files)
    current = dict(files)
    for step in chain:
        current = invoke(step['converter'], current)
        if validate_codec(measured, step['to'], current) != original:
            raise ContractError('Migration changes project identity or its complete preservation projection')
    return current, chain, original


def context(root, project, *, expected=None, owner=None, marker=None):
    return scope_binding.require(root, project, expected=expected, owned_lock=owner,
        owned_transaction=marker, migration_reader=router_metadata)


def hashes(files):
    return {path: digest(body.encode()) for path, body in files.items()}


def private_directory(path):
    trust_paths.state(path)
    missing, current = [], path
    while not current.exists():
        missing.append(current); current = current.parent
    for node in reversed(missing):
        node.mkdir(mode=0o700)
        tx.flush_directory(node.parent)
    trust_paths.state(path)


def locations(root, target, operation_id, *, new=False):
    if not uuid_valid(operation_id):
        raise ContractError('Migration operation requires its original canonical UUID')
    operations, _ = writes.state_paths(root, target, operation_id)
    state = operations.parents[2]
    directory = state / 'migrations' / digest(str(target).encode()) / operation_id
    runtime_root = PACK.parents[1] if PACK.parent.name == 'skills' else PACK
    if directory.is_relative_to(root) or root.is_relative_to(directory):
        raise ContractError('Migration recovery state must remain outside the repository')
    # All modes can write audits, including historical readback. Check actual
    # destinations before any bank lock, mkdir or record write. A retained
    # helper may share a state ancestor with new sibling operations.
    from . import installation
    install_state, managed_source = installation.locations()
    runtime_roots = {runtime_root, managed_source}
    previous = installation.read_receipt(install_state)
    if previous:
        runtime_roots.update(Path(t['path']) for t in previous['targets'] if t['kind'] in {'source', 'copy', 'link'})
    audit_directory = operations.parent / 'security-audit'
    targets = [directory / name for name in ('intent.json', 'backup.json', 'journal.json', 'receipt.json')]
    if any(path.is_relative_to(protected) for path in [*targets, audit_directory] for protected in runtime_roots):
        raise ContractError('Migration records and audits must remain outside installer-owned and executing runtime')
    retained = directory / 'trusted-package'
    if retained != PACK and any(retained.is_relative_to(protected) or protected.is_relative_to(retained)
                                for protected in runtime_roots):
        raise ContractError('Retained recovery code must not overwrite a trusted runtime')
    trust_paths.state(audit_directory)
    trust_paths.state(directory)
    return directory


def read_record(path, limit=MAX_RECORD):
    trust_paths.state(path.parent)
    trust_paths.record(path)
    raw, _ = read_regular(path.parent, path.name, limit)
    return load(raw), raw


def keep_record(path, value):
    payload = encoded(value)
    if len(payload) > MAX_RECORD:
        raise ContractError('Complete migration recovery record exceeds its bound')
    private_directory(path.parent)
    if os.path.lexists(path):
        _, raw = read_record(path)
        if raw != payload:
            raise ContractError('Existing migration recovery record differs; preserve it')
    else:
        keep_blob(path, payload, 0o600)
    return digest(payload)


def keep_blob(path, raw, mode):
    private_directory(path.parent)
    if os.path.lexists(path):
        actual, actual_mode = read_regular(path.parent, path.name, max(MAX_RECORD, len(raw)))
        if actual != raw or actual_mode != mode:
            raise ContractError('Existing migration backup/runtime file differs; preserve it')
        return
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix='.agent-memory-migration-', dir=path.parent, delete=False) as stream:
            temporary = Path(stream.name)
            os.fchmod(stream.fileno(), mode)
            stream.write(raw); stream.flush(); os.fsync(stream.fileno())
        from .installation import native_rename
        native_rename(temporary, path)  # Existing native no-replace primitive.
        temporary = None
        tx.flush_directory(path.parent)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def retain_runtime(measured, directory):
    retained = directory / 'trusted-package'
    if retained != PACK:
        private_directory(retained)
        for name, identity in measured['files'].items():
            raw, mode = read_regular(PACK, name, MAX_BYTES)
            if digest(raw) != identity['sha256'] or oct(mode) != identity['mode']:
                raise ContractError('Trusted migration package changed before recovery retention')
            keep_blob(retained / name, raw, mode)
    if artifact.inspect(retained)['artifact_digest'] != measured['artifact_digest']:
        raise ContractError('Retained recovery package does not match the original trusted artifact')
    return retained


def proposal_value(value):
    p = value.get('proposal', value) if isinstance(value, dict) else value
    if (not isinstance(p, dict) or set(p) != FIELDS or p['schema'] != SCHEMA
            or not uuid_valid(p['operation_id']) or not uuid_valid(p['project_id'])
            or not isinstance(p['repo_root'], str) or not Path(p['repo_root']).is_absolute()
            or str(Path(p['repo_root']).resolve()) != p['repo_root']
            or p['kind'] not in {'migration', 'restore'} or p['scope_context'] is None
            or not isinstance(p['chain'], list) or len(p['chain']) > 64
            or len(set(p['chain'])) != len(p['chain'])
            or any(not isinstance(v, str) or not re.fullmatch(r'[a-z0-9][a-z0-9_.-]{0,127}', v) for v in p['chain'])):
        raise ContractError('Invalid explicit migration proposal; use the original preview output')
    if p['kind'] == 'migration':
        if not p['chain'] or p['parent'] is not None:
            raise ContractError('Format conversion needs its complete chain and no restoration parent')
    else:
        parent = p['parent']
        parent_fields = {'operation_id', 'proposal_sha256', 'journal_sha256', 'backup_sha256', 'receipt_sha256'}
        if (p['chain'] or not isinstance(parent, dict) or set(parent) != parent_fields
                or not uuid_valid(parent['operation_id']) or parent['operation_id'] == p['operation_id']
                or any(not isinstance(parent[k], str) or not re.fullmatch('[0-9a-f]{64}', parent[k])
                       for k in parent_fields - {'operation_id'})):
            raise ContractError('Restoration needs a distinct verified original operation and exact source-record pins')
    repository_config.validate_root(p['bank_root'])
    scope_binding.context_value(p['scope_context'])
    if p['scope_context']['project_id'] != p['project_id']:
        raise ContractError('Migration project differs from the pinned scope')
    for key in ('source_schema', 'target_schema'):
        if not isinstance(p[key], str) or not re.fullmatch(r'agent-memory/[A-Za-z0-9_.-]{1,128}', p[key]):
            raise ContractError('Invalid migration schema selector')
    for key in ('artifact_digest', 'catalog_sha256', 'preservation_sha256'):
        if not isinstance(p[key], str) or not re.fullmatch('[0-9a-f]{64}', p[key]):
            raise ContractError('Migration needs exact code/catalog/preservation pins')
    for key in ('source_hashes', 'resulting_hashes'):
        values = p[key]
        if not isinstance(values, dict) or 'MEMORY.md' not in values or not 1 <= len(values) <= MAX_FILES:
            raise ContractError('Migration requires complete source/result inventories')
        for path, sha in values.items():
            if path != 'MEMORY.md':
                bank.path_value(path)
            if not isinstance(sha, str) or not re.fullmatch('[0-9a-f]{64}', sha):
                raise ContractError('Migration inventory requires exact SHA256 values')
    for key, inventory in (('source_modes', 'source_hashes'), ('resulting_modes', 'resulting_hashes')):
        if (not isinstance(p[key], dict) or set(p[key]) != set(p[inventory])
                or any(type(mode) is not int or not 0 <= mode <= 0o777 for mode in p[key].values())):
            raise ContractError('Migration modes differ from the complete source/result inventory')
    bank.file_topology(set(p['source_hashes']) | set(p['resulting_hashes']))
    if len(encoded(p)) > MAX_PLAN:
        raise ContractError('Complete migration proposal exceeds its reader bound')
    return p


def check_code(p):
    measured = catalog()
    if (measured['artifact_digest'] != p['artifact_digest']
            or measured['migration_catalog']['sha256'] != p['catalog_sha256']):
        raise ContractError('Migration runtime/catalog changed; retain the original plan and trusted recovery package')
    if p['kind'] == 'restore':
        return measured
    if p['target_schema'] not in measured['compatibility']['readable_bank_schemas']:
        raise ContractError('Migration target is no longer directly readable')
    chain = migration_contract.route(measured['migration_catalog'], p['source_schema'], p['target_schema'])
    if chain is None or [s['id'] for s in chain] != p['chain']:
        raise ContractError('Migration no longer has its exact original complete chain')
    return measured


def check_source(p, root, target, *, owner=None, marker=None, expected_scope=None):
    if root != Path(p['repo_root']) or target != root / p['bank_root']:
        raise ContractError('Migration repository or bank location changed')
    files, modes = snapshot(target, owner=owner, marker=marker)
    if hashes(files) != p['source_hashes'] or modes != p['source_modes']:
        raise ContractError('Migration source bytes/modes changed; preserve edits and preview again')
    context(root, p['project_id'], expected=expected_scope or p['scope_context'], owner=owner, marker=marker)
    return files, modes


def preview(repo, target_schema):
    root, target = locate(repo)
    measured = catalog()
    if target_schema not in measured['compatibility']['readable_bank_schemas']:
        raise ContractError('Migration target is not directly readable by this runtime')
    files, modes = snapshot(target)
    header = router_metadata(files['MEMORY.md'].encode())
    if header is None:
        raise ContractError('Existing bank has no recognized format-family router')
    source = header['schema']
    desired, chain, projection = transform(measured, source, target_schema, files)
    scope = context(root, projection['project_id'])
    current, current_modes = snapshot(target)
    if current != files or current_modes != modes or catalog()['artifact_digest'] != measured['artifact_digest']:
        raise ContractError('Migration sources/runtime changed during preview; nothing was applied')
    operation_id = str(uuid.uuid4())
    directory = locations(root, target, operation_id, new=True)
    audit = security.audit(root, operation_id, 'mutation', 'checked', scope, ['external'],
                           action='migrate', bank_location=target)
    if not chain:
        return result('preview', state='already-current', source_schema=source, target_schema=target_schema,
            project_id=projection['project_id'], migration_needed=False, proposal=None, verified=True,
            canonical_writes_performed=False, backup_created=False, audit=audit, private_audit_written=True)
    p = proposal_value({'schema': SCHEMA, 'operation_id': operation_id, 'repo_root': str(root),
        'kind': 'migration', 'parent': None,
        'bank_root': target.relative_to(root).as_posix(), 'project_id': projection['project_id'],
        'scope_context': scope, 'source_schema': source, 'target_schema': target_schema,
        'artifact_digest': measured['artifact_digest'], 'catalog_sha256': measured['migration_catalog']['sha256'],
        'chain': [step['id'] for step in chain], 'source_hashes': hashes(files), 'source_modes': modes,
        'resulting_hashes': hashes(desired), 'resulting_modes': {path: modes.get(path, 0o644) for path in desired},
        'preservation_sha256': projection['preservation_sha256']})
    return result('preview', state='prepared', proposal=p, migration_needed=True, verified=False,
        source_schema=source, target_schema=target_schema, project_id=p['project_id'],
        changed_paths=sorted(path for path in set(p['source_hashes']) | set(p['resulting_hashes'])
                             if p['source_hashes'].get(path) != p['resulting_hashes'].get(path)),
        backup_path=str(directory / 'backup.json'), retained_helper=str(directory / 'trusted-package/scripts/migrate_bank.py'),
        canonical_writes_performed=False, backup_created=False, private_audit_written=True, audit=audit)


def marker_value(p):
    return encoded({'schema': 'agent-memory/migration-marker-v1', 'operation_id': p['operation_id'],
                    'request_sha256': digest(encoded(p)), 'project_id': p['project_id']})


def owned_marker(target, p):
    if not os.path.lexists(target / tx.MARKER):
        return None
    raw, _ = read_regular(target, tx.MARKER, MAX_FILE)
    if raw != marker_value(p):
        raise ContractError('A different/unknown bank transaction owns the visible marker')
    return raw


def stage(p, root, target, files, modes, desired, measured):
    directory = locations(root, target, p['operation_id'], new=True)
    keep_record(directory / 'intent.json', {'schema': 'agent-memory/migration-intent-v1', 'proposal': p})
    writes.fault('migration-before-backup')
    retained = retain_runtime(measured, directory)
    backup = {'schema': 'agent-memory/migration-backup-v1', 'operation_id': p['operation_id'],
              'request_sha256': digest(encoded(p)), 'project_id': p['project_id'], 'files': files, 'modes': modes}
    backup_sha = keep_record(directory / 'backup.json', backup)
    readback, raw = read_record(directory / 'backup.json')
    if readback != backup or digest(raw) != backup_sha:
        raise ContractError('Complete migration backup readback failed before canonical mutation')
    writes.fault('migration-after-backup')
    journal = {'schema': 'agent-memory/migration-journal-v1', 'proposal': p,
               'backup_sha256': backup_sha, 'target_files': desired,
               'target_modes': p['resulting_modes'],
               'retained_artifact_digest': measured['artifact_digest']}
    keep_record(directory / 'journal.json', journal)
    return directory, journal, backup, retained


def read_intent(root, target, operation_id):
    directory = locations(root, target, operation_id)
    intent, _ = read_record(directory / 'intent.json', MAX_PLAN + 1024)
    if not isinstance(intent, dict) or set(intent) != {'schema', 'proposal'} or intent['schema'] != 'agent-memory/migration-intent-v1':
        raise ContractError('Unknown migration intent; preserve all state')
    p = proposal_value(intent['proposal'])
    if (p['repo_root'] != str(root) or target != root / p['bank_root'] or p['operation_id'] != operation_id):
        raise ContractError('Migration intent belongs to another repository, bank or operation')
    return directory, p


def read_journal(directory, p):
    journal, _ = read_record(directory / 'journal.json')
    if (not isinstance(journal, dict) or set(journal) != {'schema', 'proposal', 'backup_sha256', 'target_files',
            'target_modes', 'retained_artifact_digest'} or journal['schema'] != 'agent-memory/migration-journal-v1'
            or journal['proposal'] != p or journal['retained_artifact_digest'] != p['artifact_digest']):
        raise ContractError('Migration journal differs from the original reviewed proposal')
    backup, raw = read_record(directory / 'backup.json')
    if (digest(raw) != journal['backup_sha256'] or not isinstance(backup, dict)
            or set(backup) != {'schema', 'operation_id', 'request_sha256', 'project_id', 'files', 'modes'}
            or backup['schema'] != 'agent-memory/migration-backup-v1'
            or backup['operation_id'] != p['operation_id'] or backup['project_id'] != p['project_id']
            or backup['request_sha256'] != digest(encoded(p))
            or hashes(bounded_files(backup['files'])) != p['source_hashes']
            or backup['modes'] != p['source_modes']
            or hashes(bounded_files(journal['target_files'])) != p['resulting_hashes']
            or journal['target_modes'] != p['resulting_modes']):
        raise ContractError('Migration backup/target bytes or modes no longer match the original plan')
    return journal, backup


def receipt_value(directory, p):
    path = directory / 'receipt.json'
    if not os.path.lexists(path):
        return None
    value, _ = read_record(path, MAX_PLAN + 4096)
    fields = {'schema', 'operation_id', 'project_id', 'request_sha256', 'outcome', 'resulting_hashes',
              'resulting_modes', 'backup_sha256', 'preparation_cancelled',
              'outcome_format_revalidated', 'outcome_directly_readable', 'readback_at', 'verified'}
    if (not isinstance(value, dict) or set(value) != fields or value['schema'] != 'agent-memory/migration-receipt-v1'
            or value['operation_id'] != p['operation_id'] or value['project_id'] != p['project_id']
            or value['request_sha256'] != digest(encoded(p)) or value['verified'] is not True
            or value['outcome'] not in {'finished', 'rolled-back'}
            or type(value['preparation_cancelled']) is not bool
            or (value['preparation_cancelled'] and value['outcome'] != 'rolled-back')
            or type(value['outcome_format_revalidated']) is not bool
            or type(value['outcome_directly_readable']) is not bool):
        raise ContractError('Unknown migration receipt; do not replay over later changes')
    pin = value['backup_sha256']
    if not (isinstance(pin, str) and re.fullmatch('[0-9a-f]{64}', pin)):
        if pin is not None or not value['preparation_cancelled']:
            raise ContractError('Migration receipt requires its exact canonical backup identity')
    if os.path.lexists(directory / 'journal.json'):
        journal, _ = read_record(directory / 'journal.json')
        if (not isinstance(journal, dict) or journal.get('schema') != 'agent-memory/migration-journal-v1'
                or journal.get('proposal') != p or journal.get('backup_sha256') != pin
                or journal.get('retained_artifact_digest') != p['artifact_digest']):
            raise ContractError('Migration receipt backup identity differs from its original journal')
    elif not value['preparation_cancelled']:
        raise ContractError('Completed migration receipt requires its original journal identity')
    expected = p['resulting_hashes'] if value['outcome'] == 'finished' else p['source_hashes']
    modes = p['resulting_modes'] if value['outcome'] == 'finished' else p['source_modes']
    if value['resulting_hashes'] != expected or value['resulting_modes'] != modes:
        raise ContractError('Migration receipt outcome differs from its exact original inventories')
    storage.time_value(value['readback_at'])
    return value


def partial(files, modes, p, journal):
    actual = hashes(files)
    for path in set(actual) | set(p['source_hashes']) | set(p['resulting_hashes']):
        before = (p['source_hashes'].get(path), p['source_modes'].get(path))
        after = (p['resulting_hashes'].get(path), journal['target_modes'].get(path))
        if (actual.get(path), modes.get(path)) not in {before, after}:
            raise ContractError('Interrupted migration contains a newer external edit; automatic recovery preserves it')


def current_result(target, p, value, owner, marker):
    files, modes = snapshot(target, owner=owner, marker=marker)
    return hashes(files) == value['resulting_hashes'] and modes == value['resulting_modes']


def historical(directory, root, target, p, value, owner, marker):
    scope = context(root, p['project_id'], owner=owner, marker=marker)
    security.audit(root, p['operation_id'], 'retrieval', 'checked', scope, ['external'],
        action='migration-finish' if value['outcome'] == 'finished' else 'migration-rollback', bank_location=target)
    current = current_result(target, p, value, owner, marker)
    if marker is not None and current:
        tx.check_marker(target, marker)
        (target / tx.MARKER).unlink(); tx.flush_directory(target)
    return result('readback', state=value['outcome'], receipt=value, verified=True, still_current=current,
        receipt_is_historical=True, current_runtime_readability=None,
        already_applied=True, canonical_writes_performed=False, cleanup_pending=marker is not None and not current,
        receipt_path=str(directory / 'receipt.json'), backup_path=str(directory / 'backup.json'),
        backup_availability='not-rechecked-historical')


def converge(directory, root, target, p, journal, backup, mode, owner, scope):
    marker = marker_value(p)
    tx.check_marker(target, marker)
    known = receipt_value(directory, p)
    if known is not None:
        if known['outcome'] != ('finished' if mode == 'finish' else 'rolled-back'):
            raise ContractError('Migration already has a different verified outcome; use a new preview')
        return historical(directory, root, target, p, known, owner, marker)
    desired = journal['target_files'] if mode == 'finish' else backup['files']
    desired_modes = journal['target_modes'] if mode == 'finish' else backup['modes']
    measured = check_code(p) if mode == 'finish' else catalog()
    if mode == 'finish' and p['kind'] == 'migration':
        projection = {'project_id': p['project_id'], 'preservation_sha256': p['preservation_sha256']}
        if (validate_codec(measured, p['source_schema'], backup['files']) != projection
                or validate_codec(measured, p['target_schema'], desired) != projection):
            raise ContractError('Recorded migration representations differ from the reviewed preservation proof')
    files, modes = snapshot(target, owner=owner, marker=marker)
    partial(files, modes, p, journal)
    running = hashes(files)
    running_modes = dict(modes)
    participants = set(p['source_hashes']) | set(p['resulting_hashes'])
    desired_hashes = hashes(desired)
    changed = sorted((path for path in set(running) | set(desired)
        if (running.get(path), running_modes.get(path)) != (desired_hashes.get(path), desired_modes.get(path))),
        key=lambda path: (path == 'MEMORY.md', path))
    action = 'migration-finish' if mode == 'finish' else 'migration-rollback'
    security.audit(root, p['operation_id'], 'mutation', 'started', scope, ['external'], action=action, bank_location=target)
    def check_current():
        context(root, p['project_id'], expected=scope, owner=owner, marker=marker)
        actual, actual_modes = snapshot(target, owner=owner, marker=marker)
        if hashes(actual) != running or actual_modes != running_modes:
            raise ContractError('Bank bytes/modes changed immediately before migration replacement')
        if mode == 'finish':
            check_code(p)
    for path in changed:
        check_current()
        tx.replace_file(target, path, desired.get(path), running, marker, participants,
                        mode_bits=desired_modes.get(path), before_replace=check_current)
        if path in desired:
            running[path] = digest(desired[path].encode()); running_modes[path] = desired_modes[path]
        else:
            running.pop(path, None); running_modes.pop(path, None)
        writes.fault('migration-after-write:' + path)
    check_current()
    final, final_modes = snapshot(target, owner=owner, marker=marker)
    if final != desired or final_modes != desired_modes:
        raise ContractError('Complete migration outcome readback differs; preserve recovery evidence')
    outcome_schema = p['target_schema'] if mode == 'finish' else p['source_schema']
    outcome_revalidated = outcome_schema in measured['migration_catalog']['codecs']
    if outcome_revalidated:
        validate_codec(measured, outcome_schema, final)
    writes.fault('migration-before-receipt')
    check_current()
    actual_journal, actual_backup = read_journal(directory, p)
    if actual_journal != journal or actual_backup != backup:
        raise ContractError('Migration recovery evidence changed before receipt')
    if artifact.inspect(directory / 'trusted-package')['artifact_digest'] != p['artifact_digest']:
        raise ContractError('Retained trusted recovery package changed before receipt')
    value = {'schema': 'agent-memory/migration-receipt-v1', 'operation_id': p['operation_id'],
        'project_id': p['project_id'], 'request_sha256': digest(encoded(p)),
        'outcome': 'finished' if mode == 'finish' else 'rolled-back',
        'resulting_hashes': running, 'resulting_modes': running_modes,
        'backup_sha256': journal['backup_sha256'], 'preparation_cancelled': False,
        'outcome_format_revalidated': outcome_revalidated,
        'outcome_directly_readable': outcome_schema in measured['compatibility']['readable_bank_schemas'],
        'readback_at': storage.now(), 'verified': True}
    keep_record(directory / 'receipt.json', value)
    writes.fault('migration-after-receipt')
    if not current_result(target, p, value, owner, marker):
        raise ContractError('Bank changed after migration receipt; preserve the marker and newer edit')
    context(root, p['project_id'], expected=scope, owner=owner, marker=marker)
    (target / tx.MARKER).unlink(); tx.flush_directory(target)
    writes.fault('migration-after-marker-removal')
    security.audit(root, p['operation_id'], 'mutation', 'completed', scope, ['external'], action=action, bank_location=target)
    return result(mode, state=value['outcome'], migration_kind=p['kind'], verified=True, still_current=True, receipt=value,
        canonical_writes_performed=bool(changed), already_applied=False, receipt_path=str(directory / 'receipt.json'),
        backup_path=str(directory / 'backup.json'), retained_helper=str(directory / 'trusted-package/scripts/migrate_bank.py'))


def apply(value, *, recovery_scope=None):
    p = proposal_value(value)
    root, target = locate(p['repo_root'])
    if target != root / p['bank_root']:
        raise ContractError('Migration bank configuration changed')
    directory = locations(root, target, p['operation_id'])
    marker = owned_marker(target, p)
    context(root, p['project_id'], marker=marker)
    with storage.exclusive(target / '.agent-memory.lock', p['operation_id'], 'bank-migrate') as owner:
        scope = context(root, p['project_id'], owner=owner, marker=marker)
        known = receipt_value(directory, p)
        if known is not None:
            return historical(directory, root, target, p, known, owner, marker)
        if marker is not None:
            raise ContractError('Migration is interrupted; inspect explicit recover finish/rollback before retrying')
        context(root, p['project_id'], expected=recovery_scope or p['scope_context'], owner=owner)
        measured = check_code(p)
        files, modes = check_source(p, root, target, owner=owner, expected_scope=scope)
        if p['kind'] == 'migration':
            desired, chain, projection = transform(measured, p['source_schema'], p['target_schema'], files)
            if (hashes(desired) != p['resulting_hashes'] or [s['id'] for s in chain] != p['chain']
                    or projection['preservation_sha256'] != p['preservation_sha256']):
                raise ContractError('Migration converter output differs from the original reviewed plan')
        else:
            desired = restoration_payload(root, target, p)
        security.audit(root, p['operation_id'], 'mutation', 'started', scope, ['external'],
            action='migrate' if p['kind'] == 'migration' else 'migration-restore', bank_location=target)
        directory, journal, backup, _ = stage(p, root, target, files, modes, desired, measured)
        if read_journal(directory, p) != (journal, backup):
            raise ContractError('Migration recovery evidence changed before its first canonical edit')
        check_source(p, root, target, owner=owner, expected_scope=scope)
        check_code(p)
        writes.fault('migration-before-marker')
        check_source(p, root, target, owner=owner, expected_scope=scope)
        tx.create_marker(target / tx.MARKER, marker_value(p))
        writes.fault('migration-after-marker')
        return converge(directory, root, target, p, journal, backup, 'finish', owner, scope)


def parent_records(root, target, operation_id):
    directory, parent = read_intent(root, target, operation_id)
    journal, backup = read_journal(directory, parent)
    receipt = receipt_value(directory, parent)
    if receipt is None or receipt['outcome'] != 'finished':
        raise ContractError('Restoration requires an actually completed original migration or restoration')
    _, journal_raw = read_record(directory / 'journal.json')
    _, receipt_raw = read_record(directory / 'receipt.json', MAX_PLAN + 4096)
    pins = {'operation_id': operation_id, 'proposal_sha256': digest(encoded(parent)),
            'journal_sha256': digest(journal_raw), 'backup_sha256': journal['backup_sha256'],
            'receipt_sha256': digest(receipt_raw)}
    return parent, journal, backup, receipt, pins


def restoration_payload(root, target, p):
    parent, journal, backup, receipt, pins = parent_records(root, target, p['parent']['operation_id'])
    if (pins != p['parent'] or parent['project_id'] != p['project_id']
            or p['source_schema'] != parent['target_schema'] or p['target_schema'] != parent['source_schema']
            or p['source_hashes'] != receipt['resulting_hashes'] or p['source_modes'] != receipt['resulting_modes']
            or p['resulting_hashes'] != parent['source_hashes'] or p['resulting_modes'] != parent['source_modes']
            or p['preservation_sha256'] != parent['preservation_sha256']):
        raise ContractError('Restoration no longer matches the exact original finished operation and backup')
    return backup['files']


def restore_preview(repo, operation_id):
    root, target = locate(repo)
    measured = catalog()
    parent, _, backup, receipt, pins = parent_records(root, target, operation_id)
    files, modes = snapshot(target)
    scope = context(root, parent['project_id'])
    if hashes(files) != receipt['resulting_hashes'] or modes != receipt['resulting_modes']:
        raise ContractError('Newer memory differs from the original migrated result; backup restoration must not erase it')
    fresh_id = str(uuid.uuid4())
    directory = locations(root, target, fresh_id, new=True)
    p = proposal_value({'schema': SCHEMA, 'operation_id': fresh_id, 'kind': 'restore', 'parent': pins,
        'repo_root': str(root), 'bank_root': target.relative_to(root).as_posix(), 'project_id': parent['project_id'],
        'scope_context': scope, 'source_schema': parent['target_schema'], 'target_schema': parent['source_schema'],
        'artifact_digest': measured['artifact_digest'], 'catalog_sha256': measured['migration_catalog']['sha256'],
        'chain': [], 'source_hashes': receipt['resulting_hashes'], 'source_modes': modes,
        'resulting_hashes': parent['source_hashes'], 'resulting_modes': backup['modes'],
        'preservation_sha256': parent['preservation_sha256']})
    restoration_payload(root, target, p)
    current, current_modes = snapshot(target)
    context(root, p['project_id'], expected=scope)
    if current != files or current_modes != modes or catalog()['artifact_digest'] != p['artifact_digest']:
        raise ContractError('Restoration sources/runtime changed during preview')
    audit = security.audit(root, fresh_id, 'mutation', 'checked', scope, ['external'],
        action='migration-restore', bank_location=target)
    return result('restore-preview', state='prepared', proposal=p, verified=False, original_operation_id=operation_id,
        source_schema=p['source_schema'], target_schema=p['target_schema'],
        target_directly_readable=p['target_schema'] in measured['compatibility']['readable_bank_schemas'],
        canonical_writes_performed=False, backup_created=False, private_audit_written=True, audit=audit,
        backup_path=str(directory / 'backup.json'), retained_helper=str(directory / 'trusted-package/scripts/migrate_bank.py'))


RECOVERY_FIELDS = {'schema', 'repo_root', 'bank_root', 'operation_id', 'mode', 'scope_context',
                   'observed_hashes', 'observed_modes', 'marker_sha256', 'journal_sha256', 'receipt_sha256'}


def optional_record_sha(path):
    if not os.path.lexists(path):
        return None
    _, raw = read_record(path)
    return digest(raw)


def recovery_view(root, target, p, directory, mode):
    if mode not in {'finish', 'rollback'}:
        raise ContractError('Choose explicit migration recovery finish or rollback')
    marker = owned_marker(target, p)
    scope = context(root, p['project_id'], marker=marker)
    files, modes = snapshot(target, marker=marker)
    known = receipt_value(directory, p)
    if known is not None:
        if known['outcome'] != ('finished' if mode == 'finish' else 'rolled-back'):
            raise ContractError('A finished operation is immutable; use restore to preview a NEW backup-restoration transaction')
    elif marker is not None:
        journal, _ = read_journal(directory, p)
        partial(files, modes, p, journal)
        if mode == 'finish':
            check_code(p)
    elif hashes(files) != p['source_hashes'] or modes != p['source_modes']:
        raise ContractError('Missing migration marker with changed bank; preserve all state for explicit inspection')
    elif mode == 'finish':
        check_code(p)
    return {'schema': 'agent-memory/migration-recovery-v1', 'repo_root': str(root),
        'bank_root': target.relative_to(root).as_posix(), 'operation_id': p['operation_id'], 'mode': mode,
        'scope_context': scope, 'observed_hashes': hashes(files), 'observed_modes': modes,
        'marker_sha256': digest(marker), 'journal_sha256': optional_record_sha(directory / 'journal.json'),
        'receipt_sha256': optional_record_sha(directory / 'receipt.json')}


def recover(repo, mode=None, operation_id=None, prepared=None):
    root, target = locate(repo)
    if prepared is not None:
        r = prepared.get('proposal', prepared) if isinstance(prepared, dict) else prepared
        if (not isinstance(r, dict) or set(r) != RECOVERY_FIELDS
                or r['schema'] != 'agent-memory/migration-recovery-v1' or r['repo_root'] != str(root)
                or r['bank_root'] != target.relative_to(root).as_posix()):
            raise ContractError('Use the original bounded migration recovery preview for this repository')
        operation_id, mode = r['operation_id'], r['mode']
    else:
        r = None
    if operation_id is None:
        if not os.path.lexists(target / tx.MARKER):
            return result('recover', state='no-visible-migration', verified=True,
                canonical_writes_performed=False, private_preparations_not_scanned=True)
        raw, _ = read_regular(target, tx.MARKER, MAX_FILE)
        marker = load(raw)
        if not isinstance(marker, dict) or marker.get('schema') != 'agent-memory/migration-marker-v1':
            raise ContractError('Visible marker belongs to a different transaction; use its own recovery helper')
        operation_id = marker.get('operation_id')
    directory, p = read_intent(root, target, operation_id)
    if mode is None:
        return result('recover', state='recovery-required', operation_id=operation_id,
            available_modes=['finish', 'rollback'], verified=False, canonical_writes_performed=False,
            backup_path=str(directory / 'backup.json'))
    current = recovery_view(root, target, p, directory, mode)
    if r is None:
        return result('recover-preview', state='prepared', proposal=current, verified=False,
            canonical_writes_performed=False, backup_path=str(directory / 'backup.json'))
    scope_binding.context_value(r['scope_context'])
    if current != r:
        raise ContractError('Migration recovery observation changed; preserve new work and preview again')
    # An interrupted private preparation has not created its canonical marker.
    # Resume the original pinned conversion under apply's own bank lock.
    if current['marker_sha256'] is None and current['receipt_sha256'] is None and mode == 'finish':
        return apply(p, recovery_scope=r['scope_context'])
    with storage.exclusive(target / '.agent-memory.lock', p['operation_id'], 'bank-migrate') as owner:
        marker = owned_marker(target, p)
        scope = context(root, p['project_id'], expected=r['scope_context'], owner=owner, marker=marker)
        files, modes = snapshot(target, owner=owner, marker=marker)
        if hashes(files) != r['observed_hashes'] or modes != r['observed_modes']:
            raise ContractError('Bank bytes/modes changed after migration recovery preview')
        if (digest(marker) != r['marker_sha256']
                or optional_record_sha(directory / 'journal.json') != r['journal_sha256']
                or optional_record_sha(directory / 'receipt.json') != r['receipt_sha256']):
            raise ContractError('Migration recovery marker/journal/receipt changed under lock')
        known = receipt_value(directory, p)
        if known is not None:
            return historical(directory, root, target, p, known, owner, marker)
        if marker is None:
            if mode != 'rollback' or hashes(files) != p['source_hashes'] or modes != p['source_modes']:
                raise ContractError('Unfinished preparation cannot overwrite a changed source bank')
            measured = catalog()
            revalidated = p['source_schema'] in measured['migration_catalog']['codecs']
            if revalidated:
                validate_codec(measured, p['source_schema'], files)
            receipt = {'schema': 'agent-memory/migration-receipt-v1', 'operation_id': p['operation_id'],
                'project_id': p['project_id'], 'request_sha256': digest(encoded(p)), 'outcome': 'rolled-back',
                'resulting_hashes': p['source_hashes'], 'resulting_modes': p['source_modes'],
                'backup_sha256': optional_record_sha(directory / 'backup.json'), 'preparation_cancelled': True,
                'outcome_format_revalidated': revalidated,
                'outcome_directly_readable': p['source_schema'] in measured['compatibility']['readable_bank_schemas'],
                'readback_at': storage.now(), 'verified': True}
            keep_record(directory / 'receipt.json', receipt)
            return result('recover', state='rolled-back', verified=True, still_current=True,
                receipt=receipt, canonical_writes_performed=False, private_preparation_preserved=True)
        journal, backup = read_journal(directory, p)
        return converge(directory, root, target, p, journal, backup, mode, owner, scope)


def recover_lock(repo, prepared=None):
    root, target = locate(repo)
    lock = target / '.agent-memory.lock'
    if not os.path.lexists(lock):
        return result('recover-lock', state='no-lock', verified=True, canonical_writes_performed=False)
    info = lock.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.getuid()
            or info.st_mode & 0o022):
        raise ContractError('Migration lock is foreign, linked or writable by another principal; preserve it')
    raw, _ = read_regular(target, lock.name, MAX_FILE)
    metadata = load(raw)
    locations(root, target, metadata.get('operation_id') if isinstance(metadata, dict) else None)
    value = storage.recover(target / 'MEMORY.md', lock, {'bank-migrate'}, prepared,
                            schema='agent-memory/migration-lock-recovery-v1')
    return result('recover-lock', **value, canonical_writes_performed=False)


def input_value(path):
    selected = Path(path).expanduser()
    if not selected.is_absolute():
        selected = Path.cwd() / selected
    selected = selected.parent.resolve(strict=True) / selected.name
    trust_paths.record(selected)
    raw, _ = read_regular(selected.parent, selected.name, MAX_PLAN)
    return load(raw)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    view = commands.add_parser('preview')
    view.add_argument('--repo', default='.')
    view.add_argument('--to', required=True)
    writer = commands.add_parser('apply')
    writer.add_argument('--proposal', required=True)
    recovery = commands.add_parser('recover')
    recovery.add_argument('--repo', default='.')
    recovery.add_argument('--mode', choices=('finish', 'rollback'))
    recovery.add_argument('--operation')
    recovery.add_argument('--proposal')
    unlock = commands.add_parser('recover-lock')
    unlock.add_argument('--repo', default='.')
    unlock.add_argument('--proposal')
    restore = commands.add_parser('restore')
    restore.add_argument('--repo', default='.')
    restore.add_argument('--operation')
    restore.add_argument('--proposal')
    args = parser.parse_args(argv)
    try:
        if args.command == 'preview':
            value = preview(args.repo, args.to)
        elif args.command == 'apply':
            value = apply(input_value(args.proposal))
        elif args.command == 'restore':
            if args.proposal:
                prepared = proposal_value(input_value(args.proposal))
                if prepared['kind'] != 'restore' or Path(args.repo).resolve() != Path(prepared['repo_root']):
                    raise ContractError('Restore apply requires its own explicit proposal and selected repository')
                value = apply(prepared)
            elif args.operation:
                value = restore_preview(args.repo, args.operation)
            else:
                raise ContractError('Restore needs the original finished operation or its new restoration proposal')
        elif args.command == 'recover-lock':
            value = recover_lock(args.repo, input_value(args.proposal) if args.proposal else None)
        else:
            value = recover(args.repo, args.mode, args.operation,
                            input_value(args.proposal) if args.proposal else None)
        print(json.dumps(value, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError,
            SyntaxError, RecursionError) as exc:
        message = str(exc)[:2048]
        scanned = security.scan(message)
        if isinstance(exc, SyntaxError) or scanned['reasons']:
            message = 'Migration source, code or recovery validation failed; unsafe details withheld.'
        may_have_changed = args.command in {'apply', 'recover'} or (args.command == 'restore' and bool(args.proposal))
        print(json.dumps(result(args.command, state='inspection-required', verified=False,
            error=message, error_type=type(exc).__name__, canonical_writes_performed=None if may_have_changed else False,
            canonical_state_requires_inspection=may_have_changed,
            repair='Preserve the bank, visible marker, private intent/backups and retained trusted helper. '
                   'Recover only the owned dead lock, then preview the exact finish/rollback or a new completed-restoration operation. '
                   'Do not force replacement, rebind identity or initialize another bank.')))
        return 1


if __name__ == '__main__':
    sys.exit(main())
