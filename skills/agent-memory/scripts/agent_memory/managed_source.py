"""Verify legacy checkout payload independently of mutable Git index metadata."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import threading

from .frontmatter import ContractError


MAX_FILES = 40000
MAX_BYTES = 512 * 2**20
MAX_FILE = 64 * 2**20
MAX_OBJECT = 2 * 2**20
MAX_INDEX = 64 * 2**20


class PayloadMismatch(ContractError):
    """Observed source data differs; diagnostics may label it dirty, never use it."""


def payload_require(ok, message):
    if not ok:
        raise PayloadMismatch('Legacy managed source: ' + message)


def git(root, *args, check=True, binary=False, maximum=None):
    """Local ownership evidence cannot be redirected by ambient Git settings."""
    env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
    env.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull,
               GIT_NO_REPLACE_OBJECTS='1', GIT_OPTIONAL_LOCKS='0',
               GIT_TERMINAL_PROMPT='0', GIT_NO_LAZY_FETCH='1')
    command = ['git', '--no-replace-objects', '--literal-pathspecs', '-c', 'core.fsmonitor=false',
               '-c', 'core.untrackedCache=false',
               '-c', 'protocol.allow=never',
               '-c', 'core.hooksPath=' + os.devnull, '-C', str(root), *args]
    if maximum is None:
        result = subprocess.run(command, env=env, text=not binary,
                                capture_output=True, timeout=30)
    else:
        # Bound the actual second read, not only cat-file's earlier size report.
        # A deadline kills and reaps this owned child if it stops producing data.
        process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.DEVNULL)
        timer = threading.Timer(30, process.kill)
        try:
            timer.start()
            raw = process.stdout.read(maximum + 1)
            if len(raw) > maximum:
                process.kill()
            code = process.wait(timeout=30)
            require(len(raw) <= maximum, 'Git object exceeded its bound while reading')
            result = subprocess.CompletedProcess(command, code, raw if binary else raw.decode(), '')
        finally:
            timer.cancel(); timer.join()
            if process.poll() is None:
                process.kill(); process.wait()
            process.stdout.close()
    require(not check or result.returncode == 0, 'local Git ownership observation failed')
    return result


def require(ok, message):
    if not ok:
        raise ContractError('Legacy managed source: ' + message)


def object_bytes(root, kind, oid, algorithm):
    """Anchor every traversed tree to the recorded commit, ignoring replacements."""
    size = git(root, 'cat-file', '-s', oid, binary=True, maximum=128).stdout
    require(size.strip().isdigit() and int(size) <= MAX_OBJECT, 'Git object exceeds its bound')
    raw = git(root, 'cat-file', kind, oid, binary=True, maximum=MAX_OBJECT).stdout
    require(len(raw) == int(size), 'Git object changed while reading')
    digest = hashlib.new(algorithm, kind.encode() + b' ' + str(len(raw)).encode() + b'\0' + raw)
    require(digest.hexdigest() == oid, 'Git object does not match its recorded identity')
    return raw


def payload(root, revision):
    require(isinstance(revision, str) and re.fullmatch(r'(?:[0-9a-f]{40}|[0-9a-f]{64})', revision),
            'recorded commit identity is malformed')
    algorithm = 'sha1' if len(revision) == 40 else 'sha256'
    oid_size = len(revision) // 2
    commit = object_bytes(root, 'commit', revision, algorithm)
    tree = commit.split(b'\n', 1)[0]
    require(tree.startswith(b'tree ') and len(tree) == 5 + len(revision), 'commit has no valid tree')
    files, directories = {}, set()

    def walk(oid, prefix='', depth=0):
        require(depth <= 64, 'tree nesting exceeds its bound')
        raw = object_bytes(root, 'tree', oid, algorithm)
        offset = 0
        while offset < len(raw):
            space = raw.find(b' ', offset); zero = raw.find(b'\0', space + 1)
            require(space > offset and zero > space and zero + 1 + oid_size <= len(raw),
                    'malformed tree entry')
            mode = raw[offset:space]
            name = raw[space + 1:zero].decode('utf-8')
            require(name not in ('.', '..') and name.casefold() != '.git'
                    and '/' not in name and '\\' not in name, 'unsafe tree path')
            path = prefix + name
            require(len(os.fsencode(path)) <= 4096 and path not in files and path not in directories,
                    'duplicate or oversized tree path')
            child = raw[zero + 1:zero + 1 + oid_size].hex()
            offset = zero + 1 + oid_size
            require(len(files) + len(directories) < MAX_FILES, 'tree inventory exceeds its bound')
            if mode == b'40000':
                directories.add(path); walk(child, path + '/', depth + 1)
            else:
                require(mode in (b'100644', b'100755'), 'linked, special or submodule source is unsupported')
                files[path] = (child, mode == b'100755')
    walk(tree[5:].decode('ascii'))
    return files, directories, algorithm


def blob_identity(node, observed, algorithm):
    """Bound the actual descriptor read, including replacement/growth races."""
    fd = os.open(node, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    def identity(info):
        return (info.st_dev, info.st_ino, info.st_mode, info.st_nlink,
                info.st_size, info.st_mtime_ns, info.st_ctime_ns)
    try:
        before = os.fstat(fd)
        require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
                and identity(before) == identity(observed), 'source file changed before reading')
        require(before.st_size <= MAX_FILE, 'source file exceeds its bound')
        digest = hashlib.new(algorithm, b'blob ' + str(before.st_size).encode() + b'\0')
        total = 0
        while True:
            chunk = os.read(fd, min(65536, MAX_FILE + 1 - total))
            if not chunk:
                break
            total += len(chunk)
            require(total <= MAX_FILE, 'source file grew beyond its bound')
            digest.update(chunk)
        require(total == before.st_size and identity(os.fstat(fd)) == identity(before)
                and identity(node.lstat()) == identity(before), 'source file changed while reading')
        return digest.hexdigest()
    finally:
        os.close(fd)


def verify_index(root, files):
    """Staged work vetoes clean ownership; it never proves disk-byte identity."""
    raw = git(root, 'ls-files', '--stage', '--full-name', '-z', binary=True, maximum=MAX_INDEX).stdout
    seen = set()
    for row in raw.split(b'\0'):
        if not row:
            continue
        header, name = row.split(b'\t', 1)
        mode, oid, stage = header.split(b' ')
        path = name.decode('utf-8')
        expected = files.get(path)
        payload_require(expected is not None and path not in seen and len(seen) < MAX_FILES,
                        'index contains unexpected staged paths')
        payload_require(stage == b'0' and oid.decode('ascii') == expected[0]
                        and mode == (b'100755' if expected[1] else b'100644'),
                        'index contains staged source changes')
        seen.add(path)
    payload_require(seen == set(files), 'index contains staged source removals')


def verify_payload(root, revision):
    """No index flags, status shortcuts, checkout filters, or replace refs."""
    require((root / '.git').is_dir() and not (root / '.git').is_symlink(),
            'managed checkout requires an ordinary Git directory')
    require(not any(os.path.lexists(root / '.git' / name) for name in
                    ('commondir', 'objects/info/alternates', 'objects/info/http-alternates')),
            'redirected or alternate object stores are unsupported')
    files, directories, algorithm = payload(root, revision)
    actual_files, actual_directories, total = set(), set(), 0
    for parent, dirs, names in os.walk(root, followlinks=False,
                                     onerror=lambda exc: (_ for _ in ()).throw(exc)):
        if Path(parent) == root:
            dirs.remove('.git')
        for name in dirs + names:
            node = Path(parent) / name
            relative = node.relative_to(root).as_posix()
            info = node.lstat()
            require(len(actual_files) + len(actual_directories) < MAX_FILES, 'source inventory exceeds its bound')
            if stat.S_ISDIR(info.st_mode):
                payload_require(relative in directories, 'extra source directory: ' + relative)
                actual_directories.add(relative)
                continue
            payload_require(relative in files and stat.S_ISREG(info.st_mode) and info.st_nlink == 1,
                    'extra, linked or special source file: ' + relative)
            require(info.st_size <= MAX_FILE, 'source file exceeds its bound')
            total += info.st_size
            require(total <= MAX_BYTES, 'source bytes exceed their bound')
            oid = blob_identity(node, info, algorithm)
            expected_oid, executable = files[relative]
            payload_require(oid == expected_oid and bool(info.st_mode & stat.S_IXUSR) == executable,
                    'source bytes or executable intent differ: ' + relative)
            actual_files.add(relative)
    payload_require(actual_files == set(files) and actual_directories == directories, 'source entries are missing')
    verify_index(root, files)
    return {'qualification': 'legacy-git-payload-equivalent', 'files': len(files), 'bytes': total,
            'revision': revision}


def verify(root, source):
    """Re-establish payload ownership; transaction fingerprints remain exact."""
    from . import installation as install
    root = Path(root)
    require(source and source.get('kind') == 'managed' and str(root) == source.get('path'),
            'receipt does not own this legacy checkout')
    install.trust_paths.source(root)
    require(git(root, 'remote', 'get-url', 'origin').stdout.strip() == source['remote']
            and git(root, 'symbolic-ref', '--short', 'HEAD').stdout.strip() == source['ref'],
            'recorded channel differs')
    observed = install.source_info(root, 'managed', source['remote'], source['ref'])
    require(observed == source, 'revision, artifact, version or cleanliness differs from receipt')
    # source_info verified the actual payload before reporting clean.
    return {'qualification': 'legacy-git-payload-equivalent', 'revision': source['revision']}
