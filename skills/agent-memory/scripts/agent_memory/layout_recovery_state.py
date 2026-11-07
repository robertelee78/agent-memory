"""Checked immutable recovery evidence; never a second canonical memory bank."""
from __future__ import annotations

import os
from pathlib import Path
import re
import stat
import uuid

from . import bank_writes as writes, local_registry as storage, sharing_limits, trust_paths
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError

BINDINGS = 'agent-memory/layout-recovery-bindings-v1'
INDEX = 'agent-memory/layout-recovery-index-v1'
SHA = re.compile(r'[0-9a-f]{64}\Z')


def sha(value):
    return isinstance(value, str) and SHA.fullmatch(value) is not None


def signature(info):
    return tuple(getattr(info, field) for field in ('st_dev', 'st_ino', 'st_mode', 'st_uid', 'st_gid',
        'st_nlink', 'st_size', 'st_mtime_ns', 'st_ctime_ns'))


def read_bytes(path, maximum=sharing_limits.MAX_PRIVATE, *, missing=False):
    """Read one stable no-follow private record and pin its directory binding."""
    path = Path(path)
    trust_paths.record(path)
    if missing and not path.exists():
        return None
    parent = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        before_parent = os.fstat(parent)
        fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
        with os.fdopen(fd, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if (not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid()
                    or stat.S_IMODE(before.st_mode) != 0o600 or before.st_nlink != 1
                    or before.st_size > maximum):
                raise ContractError('Recovery evidence must be bounded owner-only regular single-link files')
            chunks, remaining = [], maximum + 1
            while remaining:
                block = stream.read(min(8192, remaining))
                if not block:
                    break
                chunks.append(block)
                remaining -= len(block)
            after = os.fstat(stream.fileno())
        raw = b''.join(chunks)
        named = os.stat(path.name, dir_fd=parent, follow_symlinks=False)
        current_parent = path.parent.lstat()
        if (len(raw) != before.st_size or signature(before) != signature(after)
                or signature(after) != signature(named)
                or (before_parent.st_dev, before_parent.st_ino) != (current_parent.st_dev, current_parent.st_ino)):
            raise ContractError('Recovery evidence or its named parent changed during observation')
        return raw
    finally:
        os.close(parent)


def read(path, maximum=sharing_limits.MAX_PRIVATE, *, missing=False):
    raw = read_bytes(path, maximum, missing=missing)
    if raw is None:
        return None
    value = load(raw)
    if encoded(value) != raw:
        raise ContractError('Immutable recovery JSON is not its exact canonical encoded record')
    return value


def private_parent(path):
    path = Path(path)
    trust_paths.inspect(path)
    storage.private_mkdir(path)
    trust_paths.state(path)
    if stat.S_IMODE(path.stat().st_mode) != 0o700:
        raise ContractError('Recovery evidence directories must be exactly 0700; do not chmod existing state')


def put_bytes(path, raw):
    path = Path(path)
    if not isinstance(raw, bytes) or len(raw) > sharing_limits.MAX_PRIVATE:
        raise ContractError('Recovery evidence exceeds its private artifact bound')
    private_parent(path.parent)
    existing = read_bytes(path, missing=True)
    if existing is not None:
        if existing != raw:
            raise ContractError('Immutable recovery evidence already exists with different bytes')
        return digest(raw)
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        fd = os.open(path.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=directory)
        with os.fdopen(fd, 'wb') as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        os.fsync(directory)
    finally:
        os.close(directory)
    if read_bytes(path) != raw:
        raise ContractError('Recovery evidence readback differs; retain the interrupted operation')
    return digest(raw)


def write_once(path, value):
    return put_bytes(path, sharing_limits.private_bytes(value))


def replace(path, value, expected_sha256):
    path = Path(path)
    private_parent(path.parent)
    current = read_bytes(path, missing=True)
    if digest(current) != expected_sha256:
        raise ContractError('Recovery state changed; retain evidence and inspect the current generation')
    raw = sharing_limits.private_bytes(value)
    storage.replace(path, raw, expected_sha256)
    if read_bytes(path) != raw:
        raise ContractError('Recovery state atomic publication was not verified')
    return digest(raw)


def bindings():
    _, path, lock = storage.paths('layout-recovery-bindings')
    if lock.exists():
        raise ContractError('Recovery bindings are locked; retain private evidence')
    value = read(path, missing=True)
    if value is None:
        return {'schema': BINDINGS, 'bindings': {}}
    if not isinstance(value, dict) or set(value) != {'schema', 'bindings'} or value['schema'] != BINDINGS or not isinstance(value['bindings'], dict):
        raise ContractError('Unknown recovery state binding registry')
    fields = {'repo_root', 'bank_path', 'checkout_key', 'project_id', 'state_root', 'index_path', 'lifecycle_state'}
    for operation, row in value['bindings'].items():
        if (not uuid_valid(operation) or not isinstance(row, dict) or set(row) != fields
                or not uuid_valid(row['project_id']) or not isinstance(row['checkout_key'], str)
                or row['lifecycle_state'] not in {'active', 'terminal', 'interrupted', 'cleaned'}):
            raise ContractError('Unknown recovery state binding')
        for name in ('repo_root', 'bank_path', 'state_root', 'index_path'):
            supplied = Path(row[name])
            if not supplied.is_absolute() or str(supplied) != str(supplied.resolve()):
                raise ContractError('Recovery binding moved or became redirected; restore the original locations')
    return value


def paths(repo, operation_id=None):
    from . import repository_config
    root = Path(repo).resolve()
    bank = root / repository_config.recovery_settings(root)['bank_root']
    return _paths_for_bank(root, bank, operation_id)


def _paths_for_bank(root, bank, operation_id=None):
    """Repeat filesystem and private-binding guards for an already located bank."""
    from . import bank_contract
    root, bank = Path(root), Path(bank)
    bank_contract.bank_root(bank)
    if bank.is_symlink() or not bank.is_dir():
        raise ContractError('Recovery requires the existing configured ordinary bank directory')
    operations, _ = writes.state_paths(root, bank, operation_id or str(uuid.uuid4()))
    base = operations.parent / 'layout-recovery'
    runtime = Path(__file__).resolve().parents[2]
    if base.is_relative_to(root) or base.is_relative_to(runtime):
        raise ContractError('Recovery state must remain outside the repository and executing runtime')
    trust_paths.inspect(base)
    for row in bindings()['bindings'].values():
        if row['repo_root'] == str(root) or row['bank_path'] == str(bank):
            if (row['repo_root'] != str(root) or row['bank_path'] != str(bank)
                    or row['index_path'] != str(base / 'index.json')
                    or row['state_root'] != str(operations.parents[2])):
                raise ContractError('Recovery state location changed; restore its original verified binding')
            if row['lifecycle_state'] in {'active', 'interrupted'} and not Path(row['index_path']).exists():
                raise ContractError('Bound recovery index is missing; coverage remains pending')
    if operation_id is not None and not uuid_valid(operation_id):
        raise ContractError('Recovery operations require canonical UUIDv4 identifiers')
    return root, bank, base, base / operation_id if operation_id else None


def index(repo):
    root, bank, _, _ = paths(repo)
    return _index_for_bank(root, bank)


def _index_for_bank(root, bank):
    root, bank, base, _ = _paths_for_bank(root, bank)
    value = read(base / 'index.json', missing=True)
    if value is None:
        return {'schema': INDEX, 'repo_root': str(root), 'bank_path': str(bank), 'operations': {}}
    if (not isinstance(value, dict) or set(value) != {'schema', 'repo_root', 'bank_path', 'operations'}
            or value['schema'] != INDEX or value['repo_root'] != str(root) or value['bank_path'] != str(bank)
            or not isinstance(value['operations'], dict)):
        raise ContractError('Unknown, foreign or damaged recovery operation index')
    fields = {'state', 'project_id', 'ledger_head_sha256', 'proposal_id', 'hold_id'}
    for operation, row in value['operations'].items():
        if (not uuid_valid(operation) or not isinstance(row, dict) or set(row) != fields
                or not uuid_valid(row['project_id']) or not uuid_valid(row['proposal_id'])
                or not uuid_valid(row['hold_id']) or not sha(row['ledger_head_sha256'])
                or row['state'] not in {'active', 'terminal', 'interrupted', 'rolled-back', 'cleaned'}):
            raise ContractError('Recovery operation index has an invalid generation or identity')
    return value


def set_index(repo, value, expected_sha256):
    _, _, base, _ = paths(repo)
    return replace(base / 'index.json', value, expected_sha256)


def bind(repo, operation_id, project_id, checkout_key, lifecycle_state):
    root, bank, base, _ = paths(repo, operation_id)
    operations, _ = writes.state_paths(root, bank, operation_id)
    config, target, lock = storage.paths('layout-recovery-bindings')
    private_parent(config)
    current = bindings()
    old = digest(read_bytes(target, missing=True))
    row = {'repo_root': str(root), 'bank_path': str(bank), 'checkout_key': checkout_key,
        'project_id': project_id, 'state_root': str(operations.parents[2]), 'index_path': str(base / 'index.json'),
        'lifecycle_state': lifecycle_state}
    existing = current['bindings'].get(operation_id)
    if existing and any(existing[name] != row[name] for name in row if name != 'lifecycle_state'):
        raise ContractError('Recovery binding cannot move or change identity')
    # Bank writers serialize one bank; compare-and-replace also prevents a
    # concurrent different-bank binding from being lost without a stale lock.
    current['bindings'][operation_id] = row
    replace(target, current, old)
    return row
