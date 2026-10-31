"""Qualify opaque cache additions for whole-runtime replacement, never reuse them.

Only the signed source target is eligible. No cache is imported, unmarshalled or
judged valid; the normal transaction preserves the complete old tree.
"""
from __future__ import annotations

import os
from pathlib import Path
import re
import stat
import tempfile

from . import artifact, release_distribution as release, trust_paths
from .frontmatter import ContractError

MAX_CACHES = 256
MAX_CACHE_FILE = 2 * 1024 * 1024
MAX_CACHE_BYTES = 16 * 1024 * 1024
CACHE = re.compile(r'(.+)\.cpython-3[0-9]{2}(?:\.opt-[0-9])?\.pyc')


def require(condition, reason):
    if not condition:
        raise ContractError('Signed runtime cache recovery refused: ' + reason
                            + '. Preserve the source and receipts; use the trusted signed installer to diagnose this exact path, not cache deletion or receipt editing.')


def _pin(value):
    return (value.st_dev, value.st_ino, value.st_mode, value.st_uid, value.st_gid,
            value.st_nlink, value.st_size, value.st_mtime_ns, value.st_ctime_ns)


def observe(root):
    """Bounded descriptor/named-binding observation with no followed links."""
    from . import installation as install
    root = Path(root)
    require(root.is_absolute() and '..' not in root.parts, 'noncanonical source path')
    descriptors, bindings, observations, tree_pins = [], [], {}, {}
    total = 0
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    node = Path(root.anchor)
    protected = False
    try:
        fd = os.open(node, flags); descriptors.append(fd)
        info = os.fstat(fd)
        protected, _ = trust_paths.inspect_entry(node, info, protected)
        # Ancestor timestamp changes (e.g. another /tmp user) are irrelevant;
        # their identity, owner and mode are still pinned.
        ancestors = [(fd, (info.st_dev, info.st_ino, info.st_mode, info.st_uid))]
        for piece in root.parts[1:]:
            parent = fd; node = node / piece
            fd = os.open(piece, flags, dir_fd=parent); descriptors.append(fd)
            info = os.fstat(fd)
            protected, _ = trust_paths.inspect_entry(node, info, protected)
            policy = (info.st_dev, info.st_ino, info.st_mode, info.st_uid)
            ancestors.append((fd, policy)); bindings.append((parent, piece, policy, False))
        root_mode = stat.S_IMODE(info.st_mode)
        root_pin = _pin(info)
        tree_pins['.'] = root_pin
        def walk(parent, relative, barrier):
            nonlocal total
            require(len(relative.parts) <= 64, 'source directory depth exceeds 64 components')
            names = []
            with os.scandir(parent) as entries:
                for entry in entries:
                    require(len(names) + len(observations) < 4096,
                            'complete source tree exceeds 4096 entries')
                    names.append(entry.name)
            for name in sorted(names):
                rel = (relative / name).as_posix()
                require(len(observations) < 4096, 'complete source tree exceeds 4096 entries')
                named = os.stat(name, dir_fd=parent, follow_symlinks=False)
                path = root / rel
                is_dir = stat.S_ISDIR(named.st_mode)
                require(is_dir or stat.S_ISREG(named.st_mode), 'unsafe file type at ' + rel)
                child = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | (os.O_DIRECTORY if is_dir else os.O_NONBLOCK), dir_fd=parent)
                descriptors.append(child)
                opened = os.fstat(child)
                require(_pin(named) == _pin(opened), 'source binding changed at ' + rel)
                child_barrier, _ = trust_paths.inspect_entry(path, opened, barrier, final=not is_dir)
                bindings.append((parent, name, _pin(opened), True))
                tree_pins[rel] = _pin(opened)
                if is_dir:
                    observations[rel] = {'kind': 'directory', 'mode': stat.S_IMODE(opened.st_mode),
                                         'uid': opened.st_uid, 'nlink': opened.st_nlink}
                    walk(child, Path(rel), child_barrier)
                else:
                    try:
                        require(opened.st_nlink == 1 and opened.st_size <= release.MAX_FILE,
                                'linked or oversized file at ' + rel)
                        chunks = []; size = 0
                        while True:
                            raw = os.read(child, min(65536, release.MAX_FILE + 1 - size))
                            if not raw: break
                            chunks.append(raw); size += len(raw)
                            require(size <= release.MAX_FILE, 'oversized file at ' + rel)
                        total += size
                        require(total <= release.MAX_BYTES + MAX_CACHE_BYTES + 2 * release.MAX_METADATA,
                                'source byte inventory exceeds its bound')
                        require(_pin(os.fstat(child)) == _pin(opened), 'source bytes changed at ' + rel)
                        observations[rel] = {'kind': 'file', 'mode': stat.S_IMODE(opened.st_mode),
                                             'uid': opened.st_uid, 'nlink': opened.st_nlink, 'raw': b''.join(chunks)}
                    finally:
                        os.close(child); descriptors.remove(child)
        walk(fd, Path(), protected)
        install.fault('cache-recovery-before-final-recheck')
        require(_pin(os.fstat(fd)) == root_pin, 'source root changed during observation')
        for descriptor, expected in ancestors:
            value = os.fstat(descriptor)
            require((value.st_dev, value.st_ino, value.st_mode, value.st_uid) == expected,
                    'source ancestor changed during observation')
        for parent, name, expected, full in bindings:
            value = os.stat(name, dir_fd=parent, follow_symlinks=False)
            actual = _pin(value) if full else (value.st_dev, value.st_ino, value.st_mode, value.st_uid)
            require(actual == expected, 'source named binding changed during observation')
        def identity(excluded=()):
            entries = {}
            for rel, item in observations.items():
                if rel in excluded: continue
                entries[rel] = ({'kind': 'directory', 'mode': item['mode']} if item['kind'] == 'directory'
                                else {'kind': 'file', 'hash': install.sha(item['raw']), 'mode': item['mode']})
            return {'kind': 'directory', 'hash': install.sha(install.encoded(entries)), 'mode': root_mode}
        return observations, identity, tree_pins
    finally:
        for descriptor in reversed(descriptors): os.close(descriptor)


def qualify(root, receipt):
    """Return one transient source-bound qualification; never modify the source."""
    from . import installation as install
    root = Path(root)
    source = receipt['source']
    require(source.get('kind') == 'release' and source.get('path') == str(root)
            and source.get('pack') == str(root / 'skills/agent-memory'), 'source is not the recorded signed runtime')
    observed, identity, tree_pins = observe(root)
    def file(name):
        require(name in observed and observed[name]['kind'] == 'file', 'missing ordinary signed metadata ' + name)
        return observed[name]['raw']
    raw_manifest = file(release.MANIFEST)
    manifest = release.verify_manifest(raw_manifest, file(release.SIGNATURE))
    require(release.digest(raw_manifest) == source.get('release_manifest_sha256')
            and manifest['artifact_digest'] == source.get('artifact_digest')
            and manifest['source_commit'] == source.get('revision')
            and manifest['version'] == source.get('version'), 'authenticated manifest differs from the receipt')
    expected = receipt.get('artifact_files')
    require(isinstance(expected, dict) and 0 < len(expected) <= release.MAX_ENTRIES
            and release.inventory_digest(expected) == manifest['artifact_digest'], 'receipt payload inventory is not authenticated by the signed manifest')
    prefix = 'skills/agent-memory/'
    signed = {prefix + name for name in expected}
    wanted = signed | {release.MANIFEST, release.SIGNATURE}
    directories = set()
    for name in wanted:
        path = Path(name)
        require(not path.is_absolute() and '..' not in path.parts, 'signed path escapes the source')
        directories.update(parent.as_posix() for parent in path.parents if parent != Path('.'))
    caches, extra_dirs = [], set()
    for name, item in observed.items():
        if name in directories:
            require(item['kind'] == 'directory', 'signed directory is missing or changed at ' + name)
        elif name in wanted:
            require(item['kind'] == 'file', 'signed file is missing or changed at ' + name)
        elif item['kind'] == 'directory':
            path = Path(name)
            require(path.name == '__pycache__' and name.startswith(prefix)
                    and item['uid'] == os.geteuid() and not item['mode'] & 0o022
                    and any(Path(value).parent == path.parent and value.endswith('.py') for value in signed),
                    'unrelated or unsafe extra directory ' + name)
            extra_dirs.add(name)
        else:
            path = Path(name); match = CACHE.fullmatch(path.name)
            module = path.parent.parent / ((match.group(1) + '.py') if match else '')
            require(match is not None and path.parent.name == '__pycache__'
                    and module.as_posix() in signed and item['uid'] == os.geteuid()
                    and item['nlink'] == 1 and item['mode'] in (0o600, 0o644),
                    'unrelated or unsafe opaque cache extra ' + name)
            require(len(item['raw']) <= MAX_CACHE_FILE, 'opaque cache file exceeds 2 MiB at ' + name)
            caches.append(name)
    missing = (wanted | directories) - set(observed)
    require(not missing, 'signed payload path is missing: ' + (sorted(missing)[0] if missing else ''))
    require(0 < len(caches) + len(extra_dirs) and len(caches) <= MAX_CACHES,
            'no eligible cache drift or more than 256 caches')
    cache_bytes = sum(len(observed[name]['raw']) for name in caches)
    require(cache_bytes <= MAX_CACHE_BYTES, 'opaque cache bytes exceed 16 MiB')
    actual_files = {}
    for name in sorted(signed):
        item = observed[name]
        actual_files[name[len(prefix):]] = {'sha256': release.digest(item['raw']), 'mode': oct(item['mode'])}
        require(actual_files[name[len(prefix):]] == expected[name[len(prefix):]],
                'signed payload bytes or modes changed at ' + name)
    require(actual_files == expected and release.inventory_digest(actual_files) == manifest['artifact_digest'],
            'signed payload bytes, names or modes changed')
    require(set(observed) == wanted | directories | set(caches) | extra_dirs, 'unaccounted source entries')
    target = next((t for t in receipt['targets'] if t['kind'] == 'source' and t['path'] == str(root)), None)
    require(target is not None and identity(set(caches) | extra_dirs) == target['identity'],
            'original signed source metadata, directories or modes changed')
    with tempfile.TemporaryDirectory(prefix='am-clean-old-payload-') as temporary:
        pack = Path(temporary)
        for name in signed:
            item = observed[name]; destination = pack / name[len(prefix):]
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(item['raw']); destination.chmod(item['mode'])
        package = artifact.inspect(pack)
    actual_source = {'kind': 'release', 'path': str(root), 'pack': str(root / 'skills/agent-memory'),
                     'remote': install.DEFAULT_REMOTE, 'ref': 'releases/latest', 'revision': manifest['source_commit'],
                     'dirty': False, 'version': package['version'], 'artifact_digest': package['artifact_digest'],
                     'compatibility': package['compatibility'], 'release_manifest_sha256': release.digest(raw_manifest),
                     'signature_namespace': release.NAMESPACE, 'signature_principal': release.PRINCIPAL}
    require(actual_source == source, 'authenticated source fields differ from the receipt')
    return {'root': str(root), 'source': actual_source, 'source_identity': identity(),
            'receipt': receipt, 'tree_pins': tree_pins,
            'files': {name[len(prefix):]: (observed[name]['raw'], observed[name]['mode']) for name in signed},
            'summary': {'schema': 'agent-memory/opaque-cache-recovery-v1', 'planned': True,
                        'cache_count': len(caches), 'cache_bytes': cache_bytes,
                        'opaque_cache_files': {name: release.digest(observed[name]['raw']) for name in sorted(caches)},
                        'old_source_identity': identity(), 'receipt_sha256': release.digest(install.encoded(receipt)),
                        'manifest_sha256': release.digest(raw_manifest),
                        'authenticated_inventory_digest': manifest['artifact_digest'],
                        'cache_validity_verified': False, 'cache_code_executed': False}}


def recheck(qualification):
    _, identity, tree_pins = observe(Path(qualification['root']))
    require(identity() == qualification['source_identity'] and tree_pins == qualification['tree_pins'], 'qualified source changed; obtain a fresh trusted-source retry')
