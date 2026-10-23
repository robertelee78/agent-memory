"""Install a portable runtime with external ownership and recoverable activation.

The source helper remains a recovery entry point even before a wrapper exists.
Directory replacement uses native exchange, never a rename-away visibility gap.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import ctypes
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import uuid

from . import artifact
from .bank_writes import fault
from .frontmatter import ContractError, unique_object

RECEIPT_SCHEMA = 'agent-memory/install-receipt-v1'
JOURNAL_SCHEMA = 'agent-memory/install-journal-v1'
BEGIN = '<!-- agent-memory:activation:begin -->'
END = '<!-- agent-memory:activation:end -->'
SHIM = (BEGIN + '\nWhen this repository contains memory-bank/MEMORY.md, or the user asks to initialize, '
        'resume, checkpoint, remember, correct or forget project memory, load the agent-memory skill '
        'and follow its selected operation.\n' + END)
DEFAULT_REMOTE = 'https://github.com/robertelee78/agent-memory.git'


class LayoutError(ContractError):
    """Unsafe storage layout; recovery must not be suggested under that layout."""


def require_layout(condition, message):
    if not condition:
        raise LayoutError(message)


def require(condition, message):
    if not condition:
        raise ContractError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2) + '\n').encode()


def load(path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= 2**20,
            'Missing, linked or oversized installation record: ' + str(path))
    return json.loads(path.read_bytes(), object_pairs_hook=unique_object)


def absolute(value):
    path = Path(value).expanduser()
    require(path.is_absolute(), 'Installation paths must be absolute: ' + str(path))
    require('..' not in path.parts, 'Installation paths must not contain parent traversal')
    # Resolve ancestors, retaining the final node so a collision cannot hide behind a link.
    return path.parent.resolve() / path.name


def fingerprint(path):
    """Identity includes type, bytes and modes, never dereferences a symlink."""
    if not os.path.lexists(path):
        return None
    mode = path.lstat().st_mode
    if stat.S_ISLNK(mode):
        return {'kind': 'link', 'target': os.readlink(path)}
    if stat.S_ISREG(mode):
        require(path.stat().st_size <= 64 * 2**20, 'Oversized managed file: ' + str(path))
        return {'kind': 'file', 'hash': sha(path.read_bytes()), 'mode': stat.S_IMODE(mode)}
    require(stat.S_ISDIR(mode), 'Unsupported filesystem node: ' + str(path))
    entries = {}; size = 0
    for parent, dirs, files in os.walk(path, followlinks=False,
                                       onerror=lambda exc: (_ for _ in ()).throw(exc)):
        for name in sorted(dirs + files):
            node = Path(parent) / name
            relative = node.relative_to(path).as_posix()
            current = node.lstat().st_mode
            if stat.S_ISDIR(current):
                item = {'kind': 'directory', 'mode': stat.S_IMODE(current)}
            else:
                item = fingerprint(node)
                if item['kind'] == 'file':
                    size += node.stat().st_size
            entries[relative] = item
            require(len(entries) <= 40000 and size <= 512 * 2**20,
                    'Managed directory inventory exceeds its bound')
    return {'kind': 'directory', 'hash': sha(encoded(entries)), 'mode': stat.S_IMODE(mode)}


def native_rename(source, target, *, exchange=False):
    """Native no-replace / exchange. Unsupported filesystems fail closed.

    Darwin renamex_np flags: RENAME_SWAP=2, RENAME_EXCL=4 (XNU stdio.h).
    Linux renameat2 flags: RENAME_EXCHANGE=2, RENAME_NOREPLACE=1.
    """
    library = ctypes.CDLL(None, use_errno=True)
    left, right = os.fsencode(source), os.fsencode(target)
    if sys.platform == 'darwin' and hasattr(library, 'renamex_np'):
        call = library.renamex_np
        call.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
        result = call(left, right, 2 if exchange else 4)
    elif sys.platform.startswith('linux') and hasattr(library, 'renameat2'):
        call = library.renameat2
        call.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        result = call(-100, left, -100, right, 2 if exchange else 1)
    else:
        raise ContractError('Atomic no-replace/exchange unavailable; use a supported filesystem/runtime')
    if result:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code), str(target))


def sync_dir(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def unchanged_parent(path):
    require(path.parent.resolve() == path.parent,
            'Installation parent changed or became a symlink: ' + str(path.parent))


def copy_node(source, target):
    if source.is_symlink():
        target.symlink_to(os.readlink(source), target_is_directory=source.is_dir())
    elif source.is_dir():
        shutil.copytree(source, target, symlinks=True)
    else:
        shutil.copy2(source, target, follow_symlinks=False)
    sync_tree(target); sync_dir(target.parent)


def sync_tree(path):
    if path.is_symlink():
        return
    if path.is_file():
        fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
        return
    for parent, dirs, files in os.walk(path, topdown=False, followlinks=False):
        for name in files:
            sync_tree(Path(parent)/name)
        sync_dir(parent)


def write_file(path, raw, mode=0o600):
    with path.open('xb') as stream:
        stream.write(raw); stream.flush(); os.fsync(stream.fileno())
    path.chmod(mode)


def replace_record(path, value):
    stage = path.parent / ('.record-' + uuid.uuid4().hex)
    write_file(stage, encoded(value))
    try:
        os.replace(stage, path)
        sync_dir(path.parent)
    finally:
        if stage.exists():
            stage.unlink()


def locations():
    home = Path.home()
    state = absolute(Path(os.environ.get('XDG_STATE_HOME', home/'.local/state'))/'agent-memory/install')
    code = absolute(Path(os.environ.get('XDG_DATA_HOME', home/'.local/share'))/'agent-memory/source')
    return state, code


@contextmanager
def locked(state):
    preflight_path(state)
    unchanged_parent(state)
    require(not state.is_symlink(), 'Installation state directory is a symlink')
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    path = state/'lock'
    require(not path.is_symlink(), 'Lifecycle lock is a symlink: ' + str(path))
    fd = os.open(path, os.O_RDWR | os.O_CREAT | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ContractError('Another lifecycle process holds the lock; retry after it finishes') from None
        require(stat.S_ISREG(os.fstat(fd).st_mode) and os.fstat(fd).st_nlink == 1,
                'Lifecycle lock is not a private regular file')
        prior = os.read(fd, 65537)
        if prior:
            lock_value = json.loads(prior)
            require(len(prior) <= 65536 and isinstance(lock_value, dict)
                    and lock_value.get('schema') == 'agent-memory/lifecycle-lock-v1',
                    'Unrelated lifecycle lock file; preserve it for reconciliation')
        os.ftruncate(fd, 0); os.lseek(fd, 0, os.SEEK_SET)
        os.write(fd, encoded({'schema':'agent-memory/lifecycle-lock-v1', 'pid': os.getpid(), 'started_at': datetime.now(timezone.utc).isoformat()}))
        os.fsync(fd)
        yield
    finally:
        os.close(fd)


def git(root, *args, check=True):
    result = subprocess.run(['git', '-C', str(root), *args], text=True, capture_output=True,
                            env={**os.environ, 'GIT_OPTIONAL_LOCKS': '0', 'GIT_TERMINAL_PROMPT': '0'},
                            timeout=120)
    if check:
        require(result.returncode == 0, 'Source Git operation failed: ' + result.stderr.strip())
    return result


def source_info(root, kind, remote=None, ref=None):
    root = root.resolve(strict=True)
    pack = root/'skills/agent-memory' if (root/'skills/agent-memory').is_dir() else root
    package = artifact.inspect(pack)
    result = git(root, 'rev-parse', '--show-toplevel', check=False)
    revision = 'unversioned'; dirty = None
    # An arbitrary copied package inside an unrelated Git parent is unversioned.
    if result.returncode == 0 and Path(result.stdout.strip()).resolve() == root:
        revision = git(root, 'rev-parse', 'HEAD').stdout.strip()
        dirty = bool(git(root, 'status', '--porcelain', '--untracked-files=all').stdout.strip())
    require(kind != 'managed' or (revision != 'unversioned' and not dirty),
            'Managed source must be a clean versioned checkout')
    return {'kind': kind, 'path': str(root), 'pack': str(pack), 'remote': remote, 'ref': ref,
            'revision': revision, 'dirty': dirty, 'version': package['version'],
            'artifact_digest': package['artifact_digest'], 'compatibility': package['compatibility']}


def read_receipt(state):
    path = state/'receipt.json'
    if not os.path.lexists(path):
        return None
    value = load(path)
    require(isinstance(value, dict) and value.get('schema') == RECEIPT_SCHEMA
            and value.get('verified') is True and isinstance(value.get('installation_id'), str)
            and isinstance(value.get('targets'), list) and isinstance(value.get('source'), dict),
            'Invalid installation ownership receipt: ' + str(path))
    for item in value['targets']:
        require(isinstance(item, dict) and item.get('kind') in {'copy', 'link', 'wrapper', 'activation', 'source'}
                and absolute(item['path']) == Path(item['path']) and 'identity' in item,
                'Malformed target ownership receipt')
    return value


def activation(raw, expected=None):
    text = raw.decode('utf-8')
    require(text.count(BEGIN) == text.count(END) and text.count(BEGIN) <= 1,
            'Unbalanced/duplicate activation sentinels; preserve and reconcile manually')
    if BEGIN in text:
        start, finish = text.index(BEGIN), text.index(END) + len(END)
        require(text.index(BEGIN) < text.index(END) and expected is not None and sha(text[start:finish].encode()) == expected,
                'Activation block is unrelated or modified; preserve and reconcile manually')
        return (text[:start] + SHIM + text[finish:]).encode()
    require(expected is None, 'Owned activation block is missing; preserve and reconcile manually')
    return raw + (b'' if not raw or raw.endswith(b'\n') else b'\n') + SHIM.encode() + b'\n'


def presets(names):
    home = Path.home(); config = Path(os.environ.get('XDG_CONFIG_HOME', home/'.config'))
    codex = Path(os.environ.get('CODEX_HOME', home/'.codex'))
    claude = Path(os.environ.get('CLAUDE_CONFIG_DIR', home/'.claude'))
    values = {
        'agents': (home/'.agents/skills', None),
        'codex': (codex/'skills', codex/'AGENTS.md'),
        'claude': (claude/'skills', claude/'CLAUDE.md'),
        'opencode': (config/'opencode/skills', config/'opencode/AGENTS.md'),
    }
    if names == 'all':
        names = ','.join(values)
    if names == 'none':
        return []
    keys = names.split(',')
    require(keys and len(set(keys)) == len(keys) and all(k in values for k in keys),
            'Presets must be all, none, or a unique comma-separated agents,codex,claude,opencode list')
    return [(key, *values[key]) for key in keys]


def wrapper(pack):
    # Python repr handles spaces, quotes and shell punctuation without shell evaluation.
    return ('#!/usr/bin/env python3\nimport runpy\nimport sys\nsys.dont_write_bytecode = True\n'
            + 'runpy.run_path(' + repr(str(pack/'scripts/installer.py')) + ", run_name='__main__')\n").encode()


def preflight_path(path):
    require(path != Path.home().resolve() and path != Path('/'), 'Unsafe installation target: ' + str(path))
    unchanged_parent(path)
    parent = path.parent
    while not parent.exists():
        parent = parent.parent
    require(parent.is_dir() and os.access(parent, os.W_OK | os.X_OK)
            and parent.stat().st_mode & 0o222, 'Target ancestor is not writable/searchable: ' + str(parent))
    for ancestor in [path, *path.parents]:
        require_layout(ancestor.name != 'memory-bank' and not (ancestor/'MEMORY.md').is_file(),
                'Installer runtime and targets must not overlap a project Memory Bank: ' + str(path))


def configuration(args, old):
    return {
        'mode': args.mode or (old['mode'] if old else 'link'),
        'presets': args.presets or (old['presets'] if old else 'all'),
        'custom_targets': args.target_dir if args.target_dir is not None else (old['custom_targets'] if old else []),
        'no_guidance': args.no_guidance or (old.get('no_guidance', False) if old else False),
        'bin_dir': absolute(args.bin_dir or (old['bin_dir'] if old else Path.home()/'.local/bin')),
    }


def preflight_layout(args, state, code, old):
    # This must precede locked(): its mkdir/lock metadata are writes too.
    preflight_path(state)
    require_layout(not state.is_relative_to(code) and not code.is_relative_to(state),
                   'Installation state and managed code paths must be separate')
    pack = artifact.PACK.resolve()
    sources = {pack}
    if pack.parent.name == 'skills' and (pack.parents[1]/'.git').exists():
        sources.add(pack.parents[1])
    if old:
        sources.add(absolute(old['source']['path']).resolve())
    journal_path = state/'journal.json'
    if journal_path.exists():
        journal = load(journal_path)
        require(isinstance(journal, dict) and journal.get('schema') == JOURNAL_SCHEMA,
                'Unsupported lifecycle journal')
        recorded = journal.get('receipt')
        require(isinstance(recorded, dict) and isinstance(recorded.get('source'), dict),
                'Lifecycle journal has no source identity')
        declared = recorded['source'].get('path')
        require(isinstance(declared, str), 'Lifecycle journal has no source identity')
        sources.add(absolute(declared).resolve())
    paths = [state]
    if args.command == 'install':
        cfg = configuration(args, old)
        paths.append(cfg['bin_dir']/'agent-memory')
        for _, target, guidance in presets(cfg['presets']):
            paths.append(absolute(target/'agent-memory'))
            if guidance and not cfg['no_guidance']:
                paths.append(absolute(guidance))
        paths.extend(absolute(target)/'agent-memory' for target in cfg['custom_targets'])
        if args.development_source:
            sources.add(absolute(args.development_source).resolve(strict=True))
        for path in paths[1:]:
            preflight_path(path)
            require_layout(not path.is_relative_to(state) and not state.is_relative_to(path),
                           'Installation target overlaps the external receipt directory')
    for path in paths:
        for source in sources:
            require_layout(not path.is_relative_to(source) and not source.is_relative_to(path),
                           'Installer must not mutate a source package or development source: ' + str(path))


def public_plan(plan):
    return {'schema': 'agent-memory/install-plan-v1', 'operation': 'install', 'verified': False,
            'installation_writes_performed': False, 'source': plan['source'], 'mode': plan['mode'],
            'enabled_adapters': plan['adapters'], 'data_roots': [],
            'memory_data_location': 'repository-local memory-bank/; not installer-owned', 'receipt_path': str(plan['state']/'receipt.json'),
            'targets': [{k: v for k, v in item.items() if k in ('path', 'kind', 'before', 'after', 'changed')}
                        for item in plan['actions']],
            'restart_clients': plan['restart'], 'atomicity': 'per target; journaled rollback across targets'}


@contextmanager
def prepare(args, state, code, old):
    with tempfile.TemporaryDirectory(prefix='agent-memory-install-source-') as scratch:
        scratch = Path(scratch)
        saved = old['source'] if old else None
        if args.development_source:
            require(not args.remote and not args.ref, 'Choose development source or managed remote/ref, not both')
            root = absolute(args.development_source).resolve(strict=True)
            source = source_info(root, 'development')
        elif saved and saved['kind'] == 'development':
            require(not args.remote and not args.ref, 'Changing source channel requires a separate installation')
            root = Path(saved['path']); source = source_info(root, 'development')
        elif saved:
            require((not args.remote or args.remote == saved['remote'])
                    and (not args.ref or args.ref == saved['ref']), 'Managed channel differs from ownership receipt')
            root = Path(saved['path'])
            require(root == code and git(root, 'remote', 'get-url', 'origin').stdout.strip() == saved['remote']
                    and git(root, 'symbolic-ref', '--short', 'HEAD').stdout.strip() == saved['ref'],
                    'Managed source path, remote or branch differs from receipt')
            source = source_info(root, 'managed', saved['remote'], saved['ref'])
            require(source['revision'] == saved['revision'] and source['artifact_digest'] == saved['artifact_digest'],
                    'Managed source changed outside the updater; preserve it for reconciliation')
        else:
            require(not os.path.lexists(code), 'Unrelated managed-source collision: ' + str(code))
            remote = args.remote or DEFAULT_REMOTE; ref = args.ref or 'main'
            require(not ref.startswith('-') and ref.strip() == ref and ref, 'Invalid source branch')
            root = scratch/'source'
            result = subprocess.run(['git', 'clone', '--branch', ref, '--single-branch', '--', remote, str(root)],
                                    text=True, capture_output=True, timeout=120,
                                    env={**os.environ, 'GIT_TERMINAL_PROMPT': '0'})
            require(result.returncode == 0, 'Could not stage source: ' + result.stderr.strip())
            source = source_info(root, 'managed', remote, ref)
        require(not state.is_relative_to(root) and not root.is_relative_to(state),
                'Installation state must not overlap the selected source')
        pack_source = Path(source['pack'])
        active_root = root if source['kind'] == 'development' else code
        active_pack = active_root / pack_source.relative_to(root)
        source = {**source, 'path': str(active_root), 'pack': str(active_pack)}
        cfg = configuration(args, old)
        mode, selected, custom = cfg['mode'], cfg['presets'], cfg['custom_targets']
        no_guidance, bin_dir = cfg['no_guidance'], cfg['bin_dir']
        prior = {item['path']: item for item in old['targets']} if old else {}
        actions = []; adapters = []
        def add(path, kind, raw=None, source_path=None, mode_bits=0o644, block=False):
            path = absolute(path); preflight_path(path); before = fingerprint(path)
            previous = prior.get(str(path))
            if block:
                require(before is None or before['kind'] == 'file', 'Activation file collision: ' + str(path))
                raw = activation(path.read_bytes() if before else b'',
                                 previous.get('block_hash') if previous and previous['kind']=='activation' else None)
                mode_bits = before['mode'] if before else 0o644
            elif before is not None:
                require(previous is not None and previous['identity'] == before,
                        'Unrelated or user-modified target; preserve: ' + str(path))
            if kind == 'link':
                after = {'kind': 'link', 'target': str(active_pack)}
            elif kind in ('copy', 'source'):
                after = fingerprint(source_path)
            else:
                after = {'kind': 'file', 'hash': sha(raw), 'mode': mode_bits}
            actions.append({'path': str(path), 'kind': kind, 'before': before, 'after': after,
                            'raw': raw, 'source_path': str(source_path) if source_path else None,
                            'mode_bits': mode_bits, 'changed': before != after})
        if source['kind'] == 'managed':
            add(code, 'source', source_path=root)
        groups = {}
        for name, target, guidance in presets(selected):
            groups[name] = [str(absolute(target/'agent-memory'))]
            if guidance and not no_guidance:
                groups[name].append(str(absolute(guidance)))
            adapters.append(name)
            add(target/'agent-memory', mode, source_path=pack_source)
            if guidance and not no_guidance:
                add(guidance, 'activation', block=True)
        for target in custom:
            add(absolute(target)/'agent-memory', mode, source_path=pack_source)
            adapter = 'custom:' + str(absolute(target))
            adapters.append(adapter); groups[adapter] = [str(absolute(target)/'agent-memory')]
        add(bin_dir/'agent-memory', 'wrapper', raw=wrapper(active_pack), mode_bits=0o755)
        paths = [Path(a['path']) for a in actions]
        for index, path in enumerate(paths):
            require(not path.is_relative_to(state) and not state.is_relative_to(path),
                    'Installation target overlaps the external receipt directory')
            for other in paths[index+1:]:
                require(not path.is_relative_to(other) and not other.is_relative_to(path),
                        'Duplicate or overlapping installation targets: ' + str(path) + ' / ' + str(other))
            if source['kind'] == 'development':
                require(not path.is_relative_to(root) and not root.is_relative_to(path),
                        'Installer must not mutate a development source')
        require(not (set(prior) - {str(p) for p in paths}),
                'Changing enabled targets would strand ownership; uninstall first')
        def loaded_change(item):
            previous = prior.get(item['path'])
            if item['kind'] in ('copy','link'):
                return previous is None or previous.get('artifact_digest') != source['artifact_digest']
            if item['kind'] == 'activation':
                return previous is None or previous.get('block_hash') != sha(SHIM.encode())
            return False
        restart = [a for a in adapters if any(item['path'] in groups[a] and loaded_change(item)
                                             for item in actions)]
        yield {'state': state, 'source': source, 'mode': mode, 'presets': selected,
               'custom_targets': [str(absolute(t)) for t in custom], 'no_guidance': no_guidance,
               'bin_dir': str(bin_dir), 'adapters': adapters, 'actions': actions, 'restart': restart,
               'old': old, 'pack_source': pack_source}


def make_parents(parent, journal, state):
    if parent.exists():
        require(parent.is_dir(), 'Target parent is not a directory')
        return
    make_parents(parent.parent, journal, state)
    journal['created_directories'].append(str(parent))
    replace_record(state/'journal.json', journal)
    parent.mkdir(mode=0o700); sync_dir(parent.parent)


def verify_targets(targets):
    for target in targets:
        path = Path(target['path'])
        require(fingerprint(path) == target['identity'], 'Installed target readback failed: ' + str(path))
        if target['kind'] in ('copy', 'link'):
            require(artifact.inspect(path)['artifact_digest'] == target['artifact_digest'],
                    'Client-visible artifact readback failed: ' + str(path))


def rollback(state, journal):
    conflicts = []
    for action in reversed(journal['actions']):
        path, stage = Path(action['path']), Path(action['stage'])
        unchanged_parent(path)
        current, staged = fingerprint(path), fingerprint(stage)
        if current == action['before']:
            continue
        if current != action['after'] or (action['before'] is not None and staged != action['before']):
            conflicts.append(str(path)); continue
        if action['before'] is None:
            require(staged is None, 'Fresh activation rollback stage collision')
            native_rename(path, stage)
        else:
            native_rename(path, stage, exchange=True)
        sync_dir(path.parent)
        require(fingerprint(path) == action['before'], 'Rollback readback failed: ' + str(path))
    journal['status'] = 'rollback-conflict' if conflicts else 'rolled-back'
    journal['conflicts'] = conflicts
    replace_record(state/'journal.json', journal)
    require(not conflicts, 'Recovery preserved externally modified paths; reconcile: ' + ', '.join(conflicts))
    return {'operation': 'recover', 'verified': True, 'status': 'rolled-back',
            'preserved_backups': [p for a in journal['actions'] for p in (a['stage'], a.get('backup'))
                                  if p is not None and fingerprint(Path(p)) is not None]}


def recover(state, *, for_install=False):
    path = state/'journal.json'
    if not path.exists():
        return {'operation': 'recover', 'verified': True, 'status': 'no-pending-transaction'}
    journal = load(path)
    require(isinstance(journal, dict) and journal.get('schema') == JOURNAL_SCHEMA
            and journal.get('status') in ('staging','activating','complete','rolled-back','rollback-conflict')
            and isinstance(journal.get('actions'), list) and isinstance(journal.get('receipt'), dict)
            and isinstance(journal.get('transaction'), str), 'Unsupported lifecycle journal')
    for index, action in enumerate(journal['actions']):
        require(isinstance(action, dict) and 'before' in action and 'after' in action
                and absolute(action['path']) == Path(action['path'])
                and Path(action['stage']) == Path(action['path']).parent/
                    ('.agent-memory-' + journal['transaction'] + '-' + str(index))
                and (action.get('backup') is None or Path(action['backup']) == Path(action['path']).parent/
                     ('.agent-memory-backup-' + journal['transaction'] + '-' + str(index))),
                'Malformed lifecycle journal target or staging path')
    if journal['status'] == 'complete':
        require(journal.get('receipt_identity') is not None
                and fingerprint(state/'receipt.json') == journal['receipt_identity'],
                'Completed installation receipt is missing or changed; preserve the journal and reconcile ownership')
        if for_install:
            # Installation will re-plan ownership, including allowed surrounding
            # edits and explicit development-source changes, before any activation.
            return {'operation': 'recover', 'verified': False, 'status': 'install-validation-pending'}
        verify_targets(journal['receipt']['targets'])
        return {'operation': 'recover', 'verified': True, 'status': 'complete'}
    if journal['status'] == 'rolled-back':
        if for_install:
            return {'operation': 'recover', 'verified': False, 'status': 'install-validation-pending'}
        for action in journal['actions']:
            require(fingerprint(Path(action['path'])) == action['before'],
                    'Rolled-back target changed; preserve and reconcile: ' + action['path'])
        return {'operation': 'recover', 'verified': True, 'status': 'rolled-back'}
    # Receipt activation is the commit point; a lost final response must not undo it.
    receipt = state/'receipt.json'
    if journal.get('receipt_identity') is not None and fingerprint(receipt) == journal['receipt_identity']:
        verify_targets(journal['receipt']['targets'])
        journal['status'] = 'complete'; replace_record(path, journal)
        return {'operation': 'recover', 'verified': True, 'status': 'complete'}
    return rollback(state, journal)


def install(plan):
    state = plan['state']; old = plan['old']; changed = [a for a in plan['actions'] if a['changed']]
    if (not changed and old and old['source'] == plan['source']
            and all(t['identity'] == next(a['after'] for a in plan['actions'] if a['path']==t['path'])
                    for t in old['targets'])):
        verify_targets(old['targets'])
        return {'operation': 'install', 'verified': True, 'changed': False, 'receipt': old,
                'restart_clients': [], 'receipt_path': str(state/'receipt.json')}
    transaction = uuid.uuid4().hex
    targets = []
    for action in plan['actions']:
        target = {'path': action['path'], 'kind': action['kind'], 'identity': action['after'],
                  'pre_change_identity': action['before']}
        if action['kind'] in ('copy','link'):
            target['artifact_digest'] = plan['source']['artifact_digest']
        if action['kind'] == 'activation':
            target['block_hash'] = sha(SHIM.encode())
            previous = next((t for t in (old['targets'] if old else []) if t['path']==action['path']), None)
            target['originally_absent'] = previous['originally_absent'] if previous else action['before'] is None
        targets.append(target)
    receipt = {'schema': RECEIPT_SCHEMA, 'installation_id': old['installation_id'] if old else uuid.uuid4().hex,
               'installed_at': datetime.now(timezone.utc).isoformat(), 'verified': True,
               'source': plan['source'], 'mode': plan['mode'], 'presets': plan['presets'],
               'custom_targets': plan['custom_targets'], 'no_guidance': plan['no_guidance'],
               'bin_dir': plan['bin_dir'], 'enabled_adapters': plan['adapters'], 'data_roots': [],
               'memory_data_location': 'repository-local memory-bank/; not installer-owned',
               'runtime_state': str(state.parent), 'targets': targets,
               'artifact_files': artifact.inspect(plan['pack_source'])['files'],
               'backups': list(old.get('backups', [])) if old else [], 'created_directories': [],
               'rollback_available': True, 'atomicity': 'per target; journaled rollback across targets'}
    journal = {'schema': JOURNAL_SCHEMA, 'transaction': transaction, 'status': 'staging',
               'actions': [], 'receipt': receipt, 'receipt_identity': None, 'created_directories': []}
    if (state/'journal.json').exists():
        write_file(state/('journal-' + transaction + '.json'), (state/'journal.json').read_bytes())
    replace_record(state/'journal.json', journal)
    try:
        for index, action in enumerate(changed):
            path = Path(action['path'])
            make_parents(path.parent, journal, state)
            stage = path.parent/('.agent-memory-' + transaction + '-' + str(index))
            require(not os.path.lexists(stage), 'Staging collision')
            entry = {k: action[k] for k in ('path','kind','before','after')}
            entry['stage'] = str(stage)
            entry['backup'] = str(path.parent/('.agent-memory-backup-' + transaction + '-' + str(index))) \
                if action['before'] is not None else None
            journal['actions'].append(entry)
            replace_record(state/'journal.json', journal)
            if action['kind'] == 'link':
                stage.symlink_to(plan['source']['pack'], target_is_directory=True)
            elif action['kind'] in ('copy','source'):
                copy_node(Path(action['source_path']), stage)
            else:
                write_file(stage, action['raw'], action['mode_bits'])
            sync_tree(stage); sync_dir(stage.parent)
            require(fingerprint(stage) == action['after'], 'Staged target differs from preflight source')
            if action['before'] is not None:
                backup = Path(entry['backup'])
                require(not os.path.lexists(backup) and fingerprint(path) == action['before'],
                        'Backup collision or target changed before backup: ' + str(path))
                copy_node(path, backup)
                require(fingerprint(backup) == action['before'] and fingerprint(path) == action['before'],
                        'Pre-activation backup readback failed: ' + str(path))
                receipt['backups'].append({'path': str(backup), 'original_path': str(path),
                                           'identity': action['before'], 'available': True,
                                           'kind': 'pre-activation-snapshot'})
                receipt['backups'].append({'path': str(stage), 'original_path': str(path),
                                           'identity': action['before'], 'available': True})
            replace_record(state/'journal.json', journal)
        # Exercise the actual parent filesystem before changing any target.
        for parent in {Path(a['path']).parent for a in journal['actions']}:
            with tempfile.TemporaryDirectory(prefix='.agent-memory-probe-', dir=parent) as probe:
                left, right = Path(probe)/'left', Path(probe)/'right'
                left.mkdir(); (left/'payload').write_bytes(b'left'); right.mkdir()
                native_rename(left, right, exchange=True)
                native_rename(right, Path(probe)/'third')
        for action in changed:
            require(fingerprint(Path(action['path'])) == action['before'],
                    'Target changed after preflight: ' + action['path'])
        journal['status'] = 'activating'; replace_record(state/'journal.json', journal)
        for index, action in enumerate(journal['actions']):
            path, stage = Path(action['path']), Path(action['stage'])
            unchanged_parent(path)
            fault('install-before-activate-' + str(index))
            require(fingerprint(path) == action['before'] and fingerprint(stage) == action['after'],
                    'Target/stage changed immediately before activation: ' + str(path))
            native_rename(stage, path, exchange=action['before'] is not None)
            sync_dir(path.parent)
            fault('install-after-activate-' + str(index))
            require(fingerprint(path) == action['after']
                    and fingerprint(stage) == action['before'], 'Activation identity readback failed')
            journal['activated'] = index + 1; replace_record(state/'journal.json', journal)
        verify_targets(targets)
        receipt['created_directories'] = list(old.get('created_directories', [])) if old else []
        receipt['created_directories'] += journal['created_directories']
        receipt_path = state/'receipt.json'
        if receipt_path.exists():
            backup = state/('receipt-' + transaction + '.json')
            write_file(backup, receipt_path.read_bytes())
            receipt['backups'].append({'path': str(backup), 'original_path': str(receipt_path),
                                       'identity': fingerprint(backup), 'available': True})
        journal['receipt'] = receipt
        journal['receipt_identity'] = {'kind':'file','hash':sha(encoded(receipt)),'mode':0o600}
        replace_record(state/'journal.json', journal)
        fault('install-before-receipt')
        replace_record(receipt_path, receipt)
        require(fingerprint(receipt_path) == journal['receipt_identity'], 'Receipt readback failed')
        fault('install-after-receipt')
        journal['status'] = 'complete'; replace_record(state/'journal.json', journal)
        return {'operation': 'install', 'verified': True, 'changed': True, 'receipt': receipt,
                'restart_clients': plan['restart'], 'receipt_path': str(receipt_path)}
    except Exception:
        # If the commit point landed, recovery verifies it; otherwise it restores old targets.
        recover(state)
        raise


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    create = commands.add_parser('install', help='Plan or install owned portable runtime targets')
    create.add_argument('--check', '--dry-run', action='store_true')
    create.add_argument('--development-source')
    create.add_argument('--remote')
    create.add_argument('--ref')
    create.add_argument('--mode', choices=('link','copy'))
    create.add_argument('--presets', help='all, none, or comma-separated agents,codex,claude,opencode')
    create.add_argument('--target-dir', action='append')
    create.add_argument('--bin-dir')
    create.add_argument('--no-guidance', action='store_true')
    commands.add_parser('recover', help='Verify a committed transaction or roll back interrupted activation')
    args = parser.parse_args(argv)
    try:
        require(sys.version_info >= (3,11) and shutil.which('git'), 'Python 3.11+ and Git are required')
        state, code = locations()
        preflight_layout(args, state, code, read_receipt(state))
        if args.command == 'recover' and not os.path.lexists(state/'journal.json'):
            result = {'operation':'recover', 'verified':True, 'status':'no-pending-transaction',
                      'writes_performed':False}
        elif args.command == 'install' and args.check:
            require(not (state/'journal.json').exists() or load(state/'journal.json')['status'] in ('complete','rolled-back'),
                    'An interrupted transaction requires recover before a new install plan')
            with prepare(args, state, code, read_receipt(state)) as plan:
                result = public_plan(plan)
        else:
            with locked(state):
                recovered = recover(state, for_install=args.command == 'install')
                if args.command == 'recover':
                    result = recovered
                else:
                    with prepare(args, state, code, read_receipt(state)) as plan:
                        result = install(plan)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, subprocess.SubprocessError) as exc:
        repair = ('Choose separate writable XDG state/data and client targets outside Memory Banks and source trees; '
                  'rerun install --check. Do not run recovery under the refused layout.' if isinstance(exc, LayoutError)
                  else 'Preserve reported paths; rerun this trusted helper with recover before retrying installation.')
        result = {'operation': args.command, 'verified': False, 'error': str(exc), 'repair': repair}
    print(json.dumps(result, indent=2))
    return 0 if result.get('verified') or result.get('schema') == 'agent-memory/install-plan-v1' else 1
