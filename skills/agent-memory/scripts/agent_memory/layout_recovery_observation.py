"""Bounded raw evidence for layout recovery; never a normal bank snapshot."""
from __future__ import annotations
import errno

import ctypes
import hashlib
import os
from pathlib import Path, PurePosixPath
import stat
import sys
import uuid

from . import bank_contract as contract, bank_observation, bank_writes as writes
from . import layout_recovery_state as state, local_registry, repository_config
from . import project_identity, scope_binding
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError
from .identity_git import git

MAX_FILE = 1048576
MAX_BYTES = 67108864
MAX_FILES = 65536
MAX_ENTRIES = 131072
MAX_DEPTH = 64
CHUNK = 8192
SIGNATURE = ('st_dev', 'st_ino', 'st_mode', 'st_uid', 'st_gid', 'st_nlink',
             'st_size', 'st_mtime_ns', 'st_ctime_ns')


class RawCapacityError(ContractError):
    def __init__(self, dimension, limit, measured, measurement='lower-bound'):
        super().__init__('pending-recovery-capacity')
        self.details = {'dimension': dimension, 'limit': limit, 'measured': measured,
                        'measurement': measurement, 'observation_complete': False}


def signature(info):
    return {key[3:]: getattr(info, key) for key in SIGNATURE}


def path_value(relative):
    if (not isinstance(relative, str) or not relative or len(relative.encode()) > 4096
            or str(PurePosixPath(relative)) != relative or PurePosixPath(relative).is_absolute()
            or any(part in ('.', '..') for part in relative.split('/'))
            or any(ord(c) < 32 or c in '\\\x7f\x85\u2028\u2029' for c in relative)):
        raise ContractError('Unsafe raw-source path; preserve the physical bank')
    return relative


def metadata_supported(path):
    """Refuse metadata which this per-file protocol cannot reproduce exactly."""
    info = path.lstat()
    if getattr(info, 'st_flags', 0):
        raise ContractError('pending-metadata-preservation')
    if hasattr(os, 'listxattr') and os.listxattr(path, follow_symlinks=False):
        raise ContractError('pending-metadata-preservation')
    if sys.platform == 'darwin':
        libc = ctypes.CDLL(None, use_errno=True)
        libc.acl_get_file.argtypes = [ctypes.c_char_p, ctypes.c_int]
        libc.acl_get_file.restype = ctypes.c_void_p
        libc.acl_get_entry.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.POINTER(ctypes.c_void_p)]
        libc.acl_free.argtypes = [ctypes.c_void_p]
        acl = libc.acl_get_file(os.fsencode(path), 0x100)
        if not acl:
            raise ContractError('pending-metadata-preservation')
        try:
            entry = ctypes.c_void_p()
            result = libc.acl_get_entry(acl, 0, ctypes.byref(entry))
            if result == 0 or (result == -1 and ctypes.get_errno() != errno.EINVAL):
                raise ContractError('pending-metadata-preservation')
        finally:
            libc.acl_free(acl)


def read_source(bank, relative, limit=MAX_FILE):
    """Chunked no-follow read with exact named binding and all parent pins."""
    path_value(relative)
    descriptors = []
    before_parents = []
    try:
        descriptors.append(os.open(bank, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW))
        before_parents.append(signature(os.fstat(descriptors[-1])))
        parts = PurePosixPath(relative).parts
        for part in parts[:-1]:
            descriptors.append(os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                       dir_fd=descriptors[-1]))
            before_parents.append(signature(os.fstat(descriptors[-1])))
        fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                     dir_fd=descriptors[-1])
        with os.fdopen(fd, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
                raise ContractError('Recovery requires ordinary single-link sources')
            if before.st_size > limit:
                raise RawCapacityError('source-bytes', limit, before.st_size, 'exact')
            blocks = []; measured = 0
            while True:
                block = stream.read(min(CHUNK, limit + 1 - measured))
                if not block:
                    break
                measured += len(block)
                if measured > limit:
                    raise RawCapacityError('source-bytes', limit, measured)
                blocks.append(block)
            after = os.fstat(stream.fileno())
            named = os.stat(parts[-1], dir_fd=descriptors[-1], follow_symlinks=False)
            if signature(before) != signature(after) or signature(after) != signature(named) or measured != before.st_size:
                raise ContractError('Raw source changed during observation')
        for index, descriptor in enumerate(descriptors):
            named = (os.stat(bank, follow_symlinks=False) if index == 0 else
                     os.stat(parts[index-1], dir_fd=descriptors[index-1], follow_symlinks=False))
            if (signature(named) != before_parents[index]
                    or signature(os.fstat(descriptor)) != before_parents[index]):
                raise ContractError('Raw source parent changed during observation')
        return b''.join(blocks), signature(before)
    except OSError as exc:
        raise ContractError('Raw source binding/type/permissions changed; preserve evidence') from exc
    finally:
        for descriptor in reversed(descriptors):
            os.close(descriptor)


def locate(repo):
    root = Path(repo).resolve()
    bank = root / repository_config.recovery_settings(root)['bank_root']
    contract.bank_root(bank)
    if not bank.is_dir() or bank.is_symlink():
        raise ContractError('Recovery requires the existing configured bank')
    return root, bank


def git_pins(root):
    common = git(root, 'rev-parse', '--git-common-dir', optional=True)
    if common is None:
        return {'head': None, 'branch': None, 'index': None, 'configuration': None,
                'common_git_dir': None, 'common_identity': None}
    common_path = (root / common).resolve()
    index_name = git(root, 'rev-parse', '--git-path', 'index')
    index_path = (root / index_name).resolve()
    # An index is not a raw memory source, but is still a bounded stable pin.
    raw, facts = read_source(index_path.parent, index_path.name, 64 * 1024 * 1024) if index_path.exists() else (None, None)
    return {'head': git(root, 'rev-parse', '--verify', 'HEAD', optional=True),
            'branch': git(root, 'symbolic-ref', '-q', 'HEAD', optional=True),
            'index': {'path': str(index_path), 'sha256': digest(raw), 'facts': facts},
            'configuration': digest(os.fsencode(git(root, 'config', '--null', '--list'))),
            'common_git_dir': str(common_path),
            'common_identity': {'dev': common_path.stat().st_dev, 'ino': common_path.stat().st_ino}}


def identity(repo, raw, review=None, *, owner=None, transaction=None):
    token = project_identity.RawIdentityEvidence.observe(repo, raw, review)
    dev = scope_binding.principal(repo)
    proj = project_identity.status(repo, owned_lock=owner, owned_transaction=transaction, raw_identity=token)
    if local_registry.paths('projects')[2].exists() or not proj['resolved']:
        raise ContractError('pending-identity-review')
    proof = proj['evidence']
    context = {'developer_id': dev['developer_id'], 'project_id': proj['project_id'],
               'developer_sha256': dev['observed_sha256'], 'project_sha256': proj['observed_sha256'],
               'checkout_key': proof['checkout_key'],
               'repository_settings_sha256': digest(encoded(proof['repository_settings'])),
               'initialization_only': False}
    return {'scope_context': context, 'project_evidence': proof, 'identity_review': token.review}


def inventory(bank, *, retain_directory=None, prior_sources=None):
    physical = []; sources = []; totals = {'source_bytes': 0, 'source_files': 0, 'physical_entries': 0, 'depth': 0}
    old = {row['path']: row for row in (prior_sources or [])}
    def walk(descriptor, prefix, depth):
        before = signature(os.fstat(descriptor))
        names = sorted(os.listdir(descriptor))
        for name in names:
            relative = path_value(prefix + name)
            level = depth + 1
            if level > MAX_DEPTH:
                raise RawCapacityError('depth', MAX_DEPTH, level)
            totals['depth'] = max(totals['depth'], level)
            totals['physical_entries'] += 1
            if totals['physical_entries'] > MAX_ENTRIES:
                raise RawCapacityError('physical-entries', MAX_ENTRIES, totals['physical_entries'])
            info = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
            kind = 'directory' if stat.S_ISDIR(info.st_mode) else 'file' if stat.S_ISREG(info.st_mode) else 'unsupported'
            if kind == 'unsupported' or (kind == 'file' and info.st_nlink != 1):
                raise ContractError('Unsupported raw source type/alias; preserve the bank')
            metadata_supported(bank / relative)
            runtime = contract.runtime_path(relative)
            # The exclusive owner marker is checked separately; do not bind its
            # ephemeral inode/timestamps into long-lived source evidence.
            if relative == '.agent-memory.lock':
                continue
            if runtime and ('transaction' in relative or 'recovery' in relative):
                # Only exact owned markers may pass the outer marker guard.
                if relative not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md'):
                    raise ContractError('Unknown runtime recovery artifact')
            row = {'path': relative, 'type': kind, 'runtime': runtime, **signature(info)}
            if kind == 'directory':
                child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
                try:
                    if signature(os.fstat(child)) != signature(info):
                        raise ContractError('Raw directory binding changed')
                    physical.append(row)
                    walk(child, relative + '/', level)
                finally:
                    os.close(child)
            else:
                raw, facts = read_source(bank, relative)
                if facts != signature(info):
                    raise ContractError('Raw named source changed')
                row['sha256'] = digest(raw)
                physical.append(row)
                if not runtime:
                    totals['source_files'] += 1
                    totals['source_bytes'] += len(raw)
                    if totals['source_files'] > MAX_FILES:
                        raise RawCapacityError('source-files', MAX_FILES, totals['source_files'])
                    if totals['source_bytes'] > MAX_BYTES:
                        raise RawCapacityError('aggregate-bytes', MAX_BYTES, totals['source_bytes'])
                    source_id = old[relative]['source_id'] if relative in old else str(uuid.uuid4())
                    blob = 'originals/' + source_id + '.bin'
                    if retain_directory is not None:
                        state.put_bytes(retain_directory / blob, raw)
                        writes.fault('layout-recovery-partial-original-staging')
                    sources.append({'source_id': source_id, 'path': relative, 'byte_count': len(raw),
                                    'sha256': row['sha256'], 'blob': blob})
        if signature(os.fstat(descriptor)) != before or sorted(os.listdir(descriptor)) != names:
            raise ContractError('Raw directory changed during inventory')
    root_fd = os.open(bank, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        root_info = os.fstat(root_fd)
        walk(root_fd, '', 0)
        named = os.stat(bank, follow_symlinks=False)
        if (root_info.st_dev, root_info.st_ino) != (named.st_dev, named.st_ino):
            raise ContractError('Bank binding changed during inventory')
    finally:
        os.close(root_fd)
    contract.portable_aliases([row['path'] for row in physical])
    return sorted(physical, key=lambda row: row['path']), sorted(sources, key=lambda row: row['path']), totals


def observe(repo, operation_id, *, owner=None, transaction=None, retain=True, identity_review=None):
    if not uuid_valid(operation_id):
        raise ContractError('Recovery operation requires a canonical UUIDv4')
    root, bank = locate(repo)
    bank_observation.markers(bank, owner, transaction)
    if transaction is None and (bank / '.agent-memory-recovery.md').exists():
        raise ContractError('Known recovery must use its operation-specific interface')
    router, _ = read_source(bank, 'MEMORY.md')
    resolved = identity(root, router, identity_review, owner=owner, transaction=transaction)
    directory = state.paths(root, operation_id)[3] if retain else None
    before_git = git_pins(root)
    physical, sources, totals = inventory(bank, retain_directory=directory)
    after, after_sources, measured = inventory(bank, prior_sources=sources)
    if (after, after_sources, measured) != (physical, sources, totals) or git_pins(root) != before_git:
        raise ContractError('Recovery observation changed; preserve staged evidence')
    if identity(root, router, identity_review, owner=owner, transaction=transaction) != resolved:
        raise ContractError('Recovery identity changed')
    bank_observation.markers(bank, owner, transaction)
    return {'schema': 'agent-memory/layout-source-manifest-v1', 'operation_id': operation_id,
            'inspected_at': local_registry.now(), 'repo_root': str(root), 'bank_path': str(bank),
            'project_id': resolved['scope_context']['project_id'], 'identity': resolved,
            'observation_complete': True, 'physical_entries': physical, 'sources': sources,
            'totals': totals, 'git': before_git,
            'bank_binding': {key: signature(bank.stat())[key] for key in ('dev', 'ino', 'mode', 'uid', 'gid')}}


def recheck(repo, manifest, owner=None, transaction=None):
    root, bank = locate(repo)
    if str(root) != manifest['repo_root'] or str(bank) != manifest['bank_path'] or not manifest['observation_complete']:
        raise ContractError('Recovery manifest location/completeness changed')
    bank_observation.markers(bank, owner, transaction)
    physical, sources, totals = inventory(bank, prior_sources=manifest['sources'])
    binding = {key: signature(bank.stat())[key] for key in ('dev', 'ino', 'mode', 'uid', 'gid')}
    router, _ = read_source(bank, 'MEMORY.md')
    resolved = identity(root, router, manifest['identity'].get('identity_review'), owner=owner, transaction=transaction)
    if (physical != manifest['physical_entries'] or sources != manifest['sources'] or totals != manifest['totals']
            or git_pins(root) != manifest['git'] or binding != manifest['bank_binding'] or resolved != manifest['identity']):
        raise ContractError('Recovery source/metadata/identity/Git pins changed')
    return True
