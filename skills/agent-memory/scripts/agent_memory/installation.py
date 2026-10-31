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
import shlex
import stat
import subprocess
import sys
import tempfile
import uuid

from . import artifact, runtime_faults, trust_paths
from .bank_writes import fault
from .frontmatter import ContractError, unique_object

RECEIPT_SCHEMA = 'agent-memory/install-receipt-v1'
JOURNAL_SCHEMA = 'agent-memory/install-journal-v1'
BEGIN = '<!-- agent-memory:activation:begin -->'
END = '<!-- agent-memory:activation:end -->'
SHIM = (artifact.PACK/'templates/activation-markdown.example.txt').read_text()
DEFAULT_REMOTE = 'https://github.com/robertelee78/agent-memory.git'
BOOTSTRAP_SOURCE = None  # In-process handoff from the already-verified bootstrap.
COMMAND_NAMES = ('am', 'agent-memory')

# Literal public routes also let pack validation inspect delegation without
# importing or executing a candidate. These are the existing governed helpers.
MEMORY_ROUTES = {
    'developer': ('developer_identity.py', ()),
    'project': ('project_identity.py', ()),
    'identity': ('scope_binding.py', ()),
    'init': ('initialize.py', ()),
    'inspect': ('bank_state.py', ('inspect',)),
    'bank': ('bank_contract.py', ()),
    'search': ('retrieve.py', ()),
    'discover': ('discover_memory.py', ()),
    'get': ('retrieve.py', ()),
    'explain': ('retrieve.py', ()),
    'context': ('assemble_context.py', ()),
    'capture': ('admit_learning.py', ('capture',)),
    'admission': ('admit_learning.py', ()),
    'claude-boundary': ('claude_boundary.py', ()),
    'codex-boundary': ('codex_boundary.py', ()),
    'private': ('private_workspace.py', ()),
    'compare': ('compare_learning.py', ()),
    'compact': ('consolidate_learning.py', ()),
    'checkpoint': ('checkpoint.py', ()),
    'govern': ('govern_memory.py', ()),
    'correct': ('govern_memory.py', ()),
    'supersede': ('govern_memory.py', ()),
    'revoke': ('govern_memory.py', ()),
    'forget': ('govern_memory.py', ()),
    'publish': ('publish_memory.py', ()),
    'refresh': ('refresh_memory.py', ()),
    'boundary': ('refresh_memory.py', ()),
    'semantic': ('reconcile_memory.py', ()),
    'sharing': ('sharing_boundary.py', ()),
    'exchange': ('sharing_recovery.py', ()),
    'projection': ('projection.py', ()),
    'writes': ('bank_writes.py', ()),
    'transaction': ('bank_transactions.py', ()),
    'migrate': ('migrate_bank.py', ()),
}


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
    trust_paths.record(path)
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


def _native_rename(source, target, *, exchange=False):
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


def native_rename(source, target, *, exchange=False):
    def perform():
        _native_rename(source, target, exchange=exchange)
        sync_dir(Path(target).parent)
        if Path(source).parent != Path(target).parent:
            sync_dir(Path(source).parent)
    return runtime_faults.mutate('atomic-node-switch', perform)


def sync_dir(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def unchanged_parent(path):
    require(path.parent.resolve() == path.parent,
            'Installation parent changed or became a symlink: ' + str(path.parent))


def _copy_payload(source, target, *, private_root=False):
    if source.is_symlink():
        target.symlink_to(os.readlink(source), target_is_directory=source.is_dir())
    elif source.is_dir():
        # Keep the new top-level staging entry private throughout the copy.
        # copytree's final copystat must not briefly expose group-write payloads.
        target.mkdir(mode=0o700)
        for child in source.iterdir():
            _copy_payload(child, target / child.name)
        if not private_root:
            shutil.copystat(source, target, follow_symlinks=False)
    else:
        _write_file(target, source.read_bytes(), stat.S_IMODE(source.lstat().st_mode))
        shutil.copystat(source, target, follow_symlinks=False)


def _copy_node(source, target, *, private_root=False):
    _copy_payload(source, target, private_root=private_root)
    sync_tree(target); sync_dir(target.parent)


def copy_node(source, target, *, private_root=False):
    return runtime_faults.mutate('payload-snapshot', lambda: _copy_node(source, target, private_root=private_root))


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


def _write_file(path, raw, mode=0o600):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    with os.fdopen(descriptor, 'wb') as stream:
        stream.write(raw); stream.flush(); os.fsync(stream.fileno())
    path.chmod(mode)


def write_file(path, raw, mode=0o600):
    return runtime_faults.mutate('file-stage', lambda: _write_file(path, raw, mode))


def cleanup_file(path):
    if os.path.lexists(path):
        def remove():
            path.unlink()
            sync_dir(path.parent)
        runtime_faults.mutate('stage-cleanup', remove)


def replace_record(path, value):
    stage = path.parent / ('.record-' + uuid.uuid4().hex)
    try:
        write_file(stage, encoded(value))
        def publish():
            os.replace(stage, path)
            sync_dir(path.parent)
        runtime_faults.mutate('record-publication', publish)
    finally:
        cleanup_file(stage)


def locations():
    home = Path.home()
    state = absolute(Path(os.environ.get('XDG_STATE_HOME', home/'.local/state'))/'agent-memory/install')
    code = absolute(Path(os.environ.get('XDG_DATA_HOME', home/'.local/share'))/'agent-memory/source')
    return state, code


def read_owner(state):
    """Read existing lock metadata without creating, replacing or stealing it."""
    path = state/'owner.json'
    if not os.path.lexists(path):
        return None
    owner = load(path)
    require(isinstance(owner, dict) and owner.get('schema') == 'agent-memory/lifecycle-owner-v1',
            'Unrelated lifecycle owner record; preserve it for reconciliation')
    return owner


def make_state_directory(path, *, leaf=True):
    if path.exists():
        require(path.is_dir() and not path.is_symlink(), 'State parent must be an ordinary directory')
        return
    make_state_directory(path.parent, leaf=False)
    def create():
        path.mkdir(mode=0o700)
        sync_dir(path.parent)
    runtime_faults.mutate('state-parent-directory', create)


@contextmanager
def locked(state):
    trust_paths.state(state)
    preflight_path(state)
    unchanged_parent(state)
    require(not state.is_symlink(), 'Installation state directory is a symlink')
    make_state_directory(state)
    path = state/'lock'
    require(not path.is_symlink(), 'Lifecycle lock is a symlink: ' + str(path))
    if not os.path.lexists(path):
        seed = state/('.lock-seed-' + uuid.uuid4().hex)
        try:
            write_file(seed, encoded({'schema':'agent-memory/lifecycle-lock-v1'}))
            try:
                native_rename(seed, path)
            except FileExistsError:
                pass
        finally:
            cleanup_file(seed)
    trust_paths.record(path)
    fd = os.open(path, os.O_RDWR | getattr(os, 'O_NOFOLLOW', 0))
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ContractError('Another lifecycle process holds the lock; retry after it finishes') from None
        require(stat.S_ISREG(os.fstat(fd).st_mode) and os.fstat(fd).st_nlink == 1,
                'Lifecycle lock is not a private regular file')
        prior = os.read(fd, 65537)
        value = json.loads(prior)
        require(len(prior) <= 65536 and isinstance(value, dict)
                and value.get('schema') == 'agent-memory/lifecycle-lock-v1',
                'Unrelated lifecycle lock file; preserve it for reconciliation')
        owner_path = state/'owner.json'
        read_owner(state)
        # Keep the flock inode stable. Diagnostic PID metadata is published
        # separately and atomically, so a killed writer cannot tear the lock.
        replace_record(owner_path, {'schema':'agent-memory/lifecycle-owner-v1',
                                   'lock_path':str(path), 'pid':os.getpid(),
                                   'started_at':datetime.now(timezone.utc).isoformat()})
        trust_paths.state(state)
        trust_paths.record(path)
        yield
    finally:
        os.close(fd)


def git(root, *args, check=True):
    if args and args[0] != 'ls-remote':
        from .managed_source import git as local_git
        return local_git(root, *args, check=check)
    result = subprocess.run(['git', '-C', str(root), *args], text=True, capture_output=True,
                            env={**os.environ, 'GIT_OPTIONAL_LOCKS': '0', 'GIT_TERMINAL_PROMPT': '0'},
                            timeout=120)
    if check:
        require(result.returncode == 0, 'Source Git operation failed: ' + result.stderr.strip())
    return result


def source_info(root, kind, remote=None, ref=None):
    root = root.resolve(strict=True)
    trust_paths.source(root)
    pack = root/'skills/agent-memory' if (root/'skills/agent-memory').is_dir() else root
    package = artifact.inspect(pack)
    if kind == 'release':
        from . import release_distribution as release
        manifest = release.installed(root)
        require(package['artifact_digest'] == manifest['artifact_digest'], 'Signed artifact inspection differs')
        return {'kind': kind, 'path': str(root), 'pack': str(pack), 'remote': DEFAULT_REMOTE,
                'ref': 'releases/latest', 'revision': manifest['source_commit'], 'dirty': False,
                'version': package['version'], 'artifact_digest': package['artifact_digest'],
                'compatibility': package['compatibility'],
                'release_manifest_sha256': release.digest((root/release.MANIFEST).read_bytes()),
                'signature_namespace': release.NAMESPACE, 'signature_principal': release.PRINCIPAL}
    result = git(root, 'rev-parse', '--show-toplevel', check=False)
    revision = 'unversioned'; dirty = None
    # An arbitrary copied package inside an unrelated Git parent is unversioned.
    if result.returncode == 0 and Path(result.stdout.strip()).resolve() == root:
        revision = git(root, 'rev-parse', 'HEAD').stdout.strip()
        if kind == 'managed':
            from .managed_source import verify_payload
            verify_payload(root, revision)
            dirty = False
        else:
            dirty = bool(git(root, 'status', '--porcelain', '--untracked-files=all').stdout.strip())
    require(kind != 'managed' or (revision != 'unversioned' and not dirty),
            'Managed source must be a clean versioned checkout')
    return {'kind': kind, 'path': str(root), 'pack': str(pack), 'remote': remote, 'ref': ref,
            'revision': revision, 'dirty': dirty, 'version': package['version'],
            'artifact_digest': package['artifact_digest'], 'compatibility': package['compatibility']}


def read_receipt(state):
    trust_paths.state(state)
    path = state/'receipt.json'
    if not os.path.lexists(path):
        return None
    value = load(path)
    require(isinstance(value, dict) and value.get('schema') == RECEIPT_SCHEMA
            and value.get('verified') is True and isinstance(value.get('installation_id'), str)
            and isinstance(value.get('targets'), list) and isinstance(value.get('source'), dict),
            'Invalid installation ownership receipt: ' + str(path))
    if value.get('uninstalled') is True:
        return None
    for item in value['targets']:
        require(isinstance(item, dict) and item.get('kind') in {'copy', 'link', 'wrapper', 'activation', 'source', 'hook-config', 'hook-link'}
                and absolute(item['path']) == Path(item['path']) and 'identity' in item,
                'Malformed target ownership receipt')
    return value


def activation(raw, expected=None, *, shim=SHIM):
    text = raw.decode('utf-8')
    require(text.count(BEGIN) == text.count(END) and text.count(BEGIN) <= 1,
            'Unbalanced/duplicate activation sentinels; preserve and reconcile manually')
    if BEGIN in text:
        start, finish = text.index(BEGIN), text.index(END) + len(END)
        require(text.index(BEGIN) < text.index(END) and expected is not None and sha(text[start:finish].encode()) == expected,
                'Activation block is unrelated or modified; preserve and reconcile manually')
        return (text[:start] + shim + text[finish:]).encode()
    require(expected is None, 'Owned activation block is missing; preserve and reconcile manually')
    return raw + (b'' if not raw or raw.endswith(b'\n') else b'\n') + shim.encode() + b'\n'


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


def hook_paths(name):
    """Client-owned configuration paths; exact entries remain independently owned."""
    home = Path.home()
    if name == 'codex':
        return Path(os.environ.get('CODEX_HOME', home/'.codex'))/'hooks.json'
    if name == 'claude':
        return Path(os.environ.get('CLAUDE_CONFIG_DIR', home/'.claude'))/'settings.json'
    if name == 'opencode':
        return Path(os.environ.get('XDG_CONFIG_HOME', home/'.config'))/'opencode/plugins/agent-memory.js'
    return None


def client_hook_entries(name, active_pack):
    """Return the exact owned prompt and Stop entries for this runtime."""
    from . import client_hooks
    launcher = '/bin/sh ' + shlex.quote(str(active_pack/'hooks'/'python3.sh')) + ' '
    prompt = launcher + shlex.quote(str(active_pack/'hooks'/('codex_prompt.py' if name == 'codex' else 'turn_hook.py')))
    stop = {'hooks': [{'type': 'command', 'command': launcher + shlex.quote(str(active_pack/'hooks'/('codex_stop.py' if name == 'codex' else 'claude_stop.py')))}]}
    if name == 'claude':
        stop['hooks'][0]['asyncRewake'] = True
    return client_hooks.entry(prompt), stop


def wrapper(pack):
    # Search every PATH component: macOS login shells can put an older system
    # Python before an otherwise available supported interpreter. Quote the
    # selected package path as shell data, never as an evaluated command.
    target = shlex.quote(str(pack / 'scripts/installer.py'))
    return ("#!/bin/sh\n"
            "old_ifs=$IFS\nIFS=:\nset -f\n"
            "for dir in $PATH; do\n"
            "  [ -n \"$dir\" ] || dir=.\n"
            "  candidate=$dir/python3\n"
            "  if [ -x \"$candidate\" ] && \"$candidate\" -I -c 'import sys; sys.exit(sys.version_info < (3, 11))' >/dev/null 2>&1; then\n"
            "    IFS=$old_ifs\n"
            "    exec \"$candidate\" -I -B -c 'import runpy,sys; sys.orig_argv=[sys.executable,sys.argv[1]]; sys.argv=sys.argv[1:]; runpy.run_path(sys.argv.pop(1),run_name=\"__main__\")' \"$0\" " + target + " \"$@\"\n"
            "  fi\n"
            "done\n"
            "IFS=$old_ifs\n"
            "printf '%s\\n' 'am: Python 3.11+ must be available as python3 on PATH.' >&2\n"
            "exit 1\n").encode()


def preflight_path(path):
    trust_paths.inspect(path, allow_final_link=True)
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
        for target in old['targets']:
            owned = Path(target['path'])
            if target['kind'] == 'source':
                trust_paths.source(owned)
            else:
                trust_paths.tree(owned)
        sources.add(absolute(old['source']['path']).resolve())
    journal_path = state/'journal.json'
    if os.path.lexists(journal_path):
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
        paths.extend(cfg['bin_dir']/name for name in COMMAND_NAMES)
        for name, target, guidance in presets(cfg['presets']):
            paths.append(absolute(target/'agent-memory'))
            if guidance and not cfg['no_guidance']:
                paths.append(absolute(guidance))
            hook = hook_paths(name)
            if hook and not cfg['no_guidance']:
                paths.append(absolute(hook))
        paths.extend(absolute(target)/'agent-memory' for target in cfg['custom_targets'])
        if args.development_source:
            sources.add(absolute(args.development_source).resolve(strict=True))
    elif args.command == 'update' and old:
        paths.extend(absolute(old['bin_dir'])/name for name in COMMAND_NAMES)
        if not old.get('no_guidance', False):
            for name, _, _ in presets(old['presets']):
                hook = hook_paths(name)
                if hook:
                    paths.append(absolute(hook))
    for path in paths[1:]:
        preflight_path(path)
        require_layout(not path.is_relative_to(state) and not state.is_relative_to(path),
                       'Installation target overlaps the external receipt directory')
    for path in paths:
        for source in sources:
            require_layout(not path.is_relative_to(source) and not source.is_relative_to(path),
                           'Installer must not mutate a source package or development source: ' + str(path))


def lifecycle_paths(args, state, code, old):
    """Validate the complete possible write set before enabling test faults."""
    paths = [state, code]
    if old:
        paths.extend(Path(t['path']) for t in old['targets'])
    journal = read_journal(state)
    if journal:
        for action in journal['actions']:
            paths.extend(Path(action[key]) for key in ('path', 'stage'))
    if args.command == 'install':
        cfg = configuration(args, old)
        paths.extend(cfg['bin_dir'] / name for name in COMMAND_NAMES)
        for name, target, guidance in presets(cfg['presets']):
            paths.append(absolute(target / 'agent-memory'))
            if guidance and not cfg['no_guidance']:
                paths.append(absolute(guidance))
            hook = hook_paths(name)
            if hook and not cfg['no_guidance']:
                paths.append(absolute(hook))
        paths.extend(absolute(target) / 'agent-memory' for target in cfg['custom_targets'])
    return paths


def public_plan(plan):
    return {'schema': 'agent-memory/install-plan-v1', 'operation': 'install', 'verified': False,
            'installation_writes_performed': False, 'source': plan['source'], 'mode': plan['mode'],
            'enabled_adapters': plan['adapters'], 'data_roots': [],
            'memory_data_location': 'repository-local memory-bank/; not installer-owned', 'receipt_path': str(plan['state']/'receipt.json'),
            'targets': [{k: v for k, v in item.items() if k in ('path', 'kind', 'before', 'after', 'changed')}
                        for item in plan['actions']],
            **({'cache_recovery': plan['cache_recovery']['summary']} if plan.get('cache_recovery') is not None else {}),
            'restart_clients': plan['restart'], 'atomicity': 'per target; journaled rollback across targets'}


@contextmanager
def prepare(args, state, code, old, *, candidate_root=None, candidate_kind=None, repair_missing_hooks=False, cache_recovery=None):
    with tempfile.TemporaryDirectory(prefix='agent-memory-install-source-') as scratch:
        scratch = Path(scratch)
        saved = old['source'] if old else None
        if cache_recovery is not None:
            from . import cache_recovery as cache
            require(old == cache_recovery['receipt'] and read_receipt(state) == old,
                    'Installation receipt changed before cache recovery planning')
            cache.recheck(cache_recovery)
        if candidate_root is not None:
            require(saved is not None and saved['kind'] in ('managed', 'release'), 'Updates require managed ownership')
            root = Path(candidate_root).resolve(strict=True)
            source = source_info(root, candidate_kind or saved['kind'], saved['remote'], saved['ref'])
        elif args.development_source:
            require(not args.remote and not args.ref, 'Choose development source or managed remote/ref, not both')
            root = absolute(args.development_source).resolve(strict=True)
            source = source_info(root, 'development')
        elif saved and saved['kind'] == 'development':
            require(not args.remote and not args.ref, 'Changing source channel requires a separate installation')
            root = Path(saved['path']); source = source_info(root, 'development')
        elif (not args.remote and not args.ref and
              (saved is None or saved['kind'] == 'release'
               or (saved['kind'] == 'managed' and saved['remote'] == DEFAULT_REMOTE and saved['ref'] == 'main'))):
            from . import release_distribution as release, updating
            if saved:
                if cache_recovery is None:
                    _, cache_recovery = updating.qualified_current(state, code)
                else:
                    updating.current(state, code, cache_recovery=cache_recovery)
            else:
                require(not os.path.lexists(code), 'Unrelated managed-source collision: ' + str(code))
            if saved and saved['kind'] == 'release' and BOOTSTRAP_SOURCE is None and cache_recovery is None:
                root = Path(saved['path'])
            elif BOOTSTRAP_SOURCE is not None:
                root = Path(BOOTSTRAP_SOURCE).resolve(strict=True)
            else:
                root = scratch/'source'
                release.acquire(root)
            source = source_info(root, 'release')
            if saved:
                updating.release_transition(saved, source, root)
                updating.compatible_surface(saved, source, cache_recovery=cache_recovery)
        elif saved and saved['kind'] == 'release':
            require(False, 'Git remote/ref options cannot change a signed release installation; preview an explicit uninstall/reinstall instead')
        elif saved:
            require((not args.remote or args.remote == saved['remote'])
                    and (not args.ref or args.ref == saved['ref']), 'Managed channel differs from ownership receipt')
            root = Path(saved['path'])
            trust_paths.source(root)
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
        # source_info canonicalizes the trusted source. Keep root in that same
        # namespace before relative paths/ownership checks (/var -> /private/var
        # and symlinked TMPDIR are normal macOS layouts).
        root = Path(source['path'])
        require(not state.is_relative_to(root) and not root.is_relative_to(state),
                'Installation state must not overlap the selected source')
        pack_source = Path(source['pack'])
        shim_path = pack_source/'templates/activation-markdown.example.txt'
        shim = shim_path.read_text() if shim_path.is_file() else SHIM
        require(shim.startswith(BEGIN+'\n') and shim.endswith(END)
                and shim.count(BEGIN)==shim.count(END)==1 and len(shim.encode())<=4096,
                'Candidate activation shim must be one bounded invocation block')
        active_root = root if source['kind'] == 'development' else code
        active_pack = active_root / pack_source.relative_to(root)
        source = {**source, 'path': str(active_root), 'pack': str(active_pack)}
        cfg = configuration(args, old)
        mode, selected, custom = cfg['mode'], cfg['presets'], cfg['custom_targets']
        no_guidance, bin_dir = cfg['no_guidance'], cfg['bin_dir']
        prior = {item['path']: item for item in old['targets']} if old else {}
        actions = []; adapters = []
        def add(path, kind, raw=None, source_path=None, mode_bits=0o644, block=False, hook_entry=None, stop_entry=None):
            path = absolute(path); preflight_path(path); before = fingerprint(path)
            previous = prior.get(str(path))
            if block:
                require(before is None or before['kind'] == 'file', 'Activation file collision: ' + str(path))
                raw = activation(path.read_bytes() if before else b'',
                                 previous.get('block_hash') if previous and previous['kind']=='activation' and not (repair_missing_hooks and before is None) else None, shim=shim)
                mode_bits = before['mode'] if before else 0o644
            elif kind == 'hook-config':
                from . import client_hooks
                require(before is None or before['kind'] == 'file', 'Client hook configuration collision: ' + str(path))
                require(previous is None or previous['kind'] == kind, 'Client hook ownership changed: ' + str(path))
                original_raw = path.read_bytes() if before else b''
                document = client_hooks.parse(original_raw)
                original_shape = previous.get('hook_original_shape', {}) if previous else {
                    'hooks_present': 'hooks' in document,
                    'event_present': 'UserPromptSubmit' in document.get('hooks', {}),
                }
                missing_owned = bool(repair_missing_hooks and previous and before is None)
                raw = client_hooks.merge(original_raw,
                                         previous.get('hook_entry') if previous and not missing_owned else None, hook_entry)
                stop_shape = previous.get('stop_original_shape', {}) if previous and previous.get('stop_entry') else {
                    'hooks_present': 'hooks' in document,
                    'event_present': 'Stop' in document.get('hooks', {}),
                }
                if stop_entry is not None:
                    raw = client_hooks.merge(raw, previous.get('stop_entry') if previous and not missing_owned else None, stop_entry, event='Stop')
                mode_bits = before['mode'] if before else 0o644
            elif before is not None:
                if previous and kind == 'source' and before != previous['identity']:
                    if cache_recovery is not None and path == code:
                        from . import cache_recovery as cache
                        cache.recheck(cache_recovery)
                        require(before == cache_recovery['source_identity'], 'Qualified signed source changed before planning')
                    else:
                        verify_targets([previous], source=saved)
                    require(fingerprint(path) == before, 'Managed source changed during ownership verification')
                else:
                    require(previous is not None and previous['identity'] == before,
                            'Unrelated or user-modified target; preserve: ' + str(path))
            if kind == 'hook-link':
                after = {'kind': 'link', 'target': str(source_path)}
            elif kind == 'link':
                after = {'kind': 'link', 'target': str(active_pack)}
            elif kind in ('copy', 'source'):
                after = fingerprint(source_path)
                # An unchanged, already-trusted legacy copy needs no permission
                # churn or new receipt. New/replaced copies activate privately.
                same_payload = before and before.get('kind') == 'directory' and before.get('hash') == after['hash']
                after = {**after, 'mode': before['mode'] if same_payload else 0o700}
            else:
                after = {'kind': 'file', 'hash': sha(raw), 'mode': mode_bits}
            actions.append({'path': str(path), 'kind': kind, 'before': before, 'after': after,
                            'raw': raw, 'source_path': str(source_path) if source_path else None,
                            'mode_bits': mode_bits, 'changed': before != after})
            if hook_entry is not None:
                actions[-1]['hook_entry'] = hook_entry
                actions[-1]['hook_original_shape'] = original_shape
                if stop_entry is not None:
                    actions[-1]['stop_entry'] = stop_entry
                    actions[-1]['stop_original_shape'] = stop_shape
        if source['kind'] in ('managed', 'release'):
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
            hook = hook_paths(name)
            if name in ('codex', 'claude') and hook and not no_guidance:
                prompt_entry, stop = client_hook_entries(name, active_pack)
                add(hook, 'hook-config', hook_entry=prompt_entry, stop_entry=stop)
                groups[name].append(str(absolute(hook)))
            elif name == 'opencode' and hook and not no_guidance:
                add(hook, 'hook-link', source_path=active_pack/'hooks'/'opencode.js')
                groups[name].append(str(absolute(hook)))
        for target in custom:
            add(absolute(target)/'agent-memory', mode, source_path=pack_source)
            adapter = 'custom:' + str(absolute(target))
            adapters.append(adapter); groups[adapter] = [str(absolute(target)/'agent-memory')]
        names = artifact.command_names(source['compatibility'])
        for name in names:
            # Do not rewrite a legacy runtime's launcher until its own updater
            # has installed a runtime that understands the current commands.
            if 'am' not in names:
                raw = ('#!/usr/bin/env python3\nimport runpy, sys, os, stat, tempfile\nsys.dont_write_bytecode = True\n'
                       + 'parent = os.path.realpath(\"/tmp\")\nnode = \"/\"\n'
                       + 'for part in parent.split(\"/\"):\n    node = os.path.join(node, part)\n    value = os.lstat(node)\n    if not stat.S_ISDIR(value.st_mode) or value.st_uid != 0 or (value.st_mode & 0o022 and not value.st_mode & stat.S_ISVTX): raise RuntimeError(\"Unsafe source-only startup temporary root\")\n'
                       + 'guard = tempfile.TemporaryDirectory(prefix=\"am-source-only-\", dir=parent)\nsys.pycache_prefix = guard.name\n'
                       + 'runpy.run_path(' + repr(str(active_pack/'scripts/installer.py')) + ", run_name='__main__')\n").encode()
            else:
                raw = wrapper(active_pack)
            add(bin_dir/name, 'wrapper', raw=raw, mode_bits=0o755)
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
                return previous is None or previous.get('block_hash') != sha(shim.encode())
            if item['kind'] in ('hook-config', 'hook-link'):
                return previous is None or item['changed'] or previous.get('artifact_digest') != source['artifact_digest']
            return False
        restart = [a for a in adapters if any(item['path'] in groups[a] and loaded_change(item)
                                             for item in actions)]
        yield {'state': state, 'source': source, 'mode': mode, 'presets': selected,
               'custom_targets': [str(absolute(t)) for t in custom], 'no_guidance': no_guidance,
               'bin_dir': str(bin_dir), 'adapters': adapters, 'actions': actions, 'restart': restart,
               'old': old, 'pack_source': pack_source, 'shim':shim, 'cache_recovery': cache_recovery}


def make_parents(parent, journal, state):
    if parent.exists():
        require(parent.is_dir(), 'Target parent is not a directory')
        return
    make_parents(parent.parent, journal, state)
    journal['created_directories'].append(str(parent))
    replace_record(state/'journal.json', journal)
    def create():
        parent.mkdir(mode=0o700); sync_dir(parent.parent)
    runtime_faults.mutate('target-parent-directory', create)


def verify_targets(targets, *, source=None):
    qualifications = []
    for target in targets:
        path = Path(target['path'])
        trust_paths.tree(path)
        observed = fingerprint(path)
        if target['kind'] == 'hook-config' and 'hook_entry' in target:
            from . import client_hooks
            require(observed is not None and observed['kind'] == 'file',
                    'Client hook configuration is unavailable: ' + str(path))
            document = client_hooks.parse(path.read_bytes())
            client_hooks.locate(document, target.get('hook_entry'))
            if target.get('stop_entry'):
                client_hooks.locate(document, target['stop_entry'], event='Stop')
            require(fingerprint(path) == observed, 'Client hook configuration changed during verification')
            continue
        if observed != target['identity']:
            require(target['kind'] == 'source' and source and source.get('kind') == 'managed'
                    and observed and target['identity']
                    and observed.get('kind') == target['identity'].get('kind') == 'directory'
                    and observed.get('mode') == target['identity'].get('mode'),
                    'Installed target readback failed: ' + str(path))
            from .managed_source import verify
            qualifications.append(verify(path, source))
            require(fingerprint(path) == observed, 'Managed source changed during ownership verification')
        if target['identity'] is not None and target['kind'] in ('copy', 'link'):
            require(artifact.inspect(path)['artifact_digest'] == target['artifact_digest'],
                    'Client-visible artifact readback failed: ' + str(path))
    return qualifications


def rollback_compatibility(state, journal):
    """No executable restoration until the old runtime can read current banks."""
    restoring = [a for a in journal['actions'] if a['kind'] in {'source', 'copy', 'link', 'wrapper', 'hook-link'}
                 and a['before'] is not None and fingerprint(Path(a['path'])) != a['before']]
    if not restoring:
        return
    compatibility = journal.get('previous_compatibility')
    if compatibility is None:
        # Legacy incomplete journals predate this field. Their uncommitted
        # receipt is still the prior installation, and must identify these
        # exact targets before it can supply the old readability declaration.
        previous = read_receipt(state)
        require(previous is not None and all(any(t['path'] == a['path'] and t['identity'] == a['before']
                for t in previous['targets']) for a in restoring),
                'Cannot establish prior runtime compatibility; preserve targets and use a trusted outside helper')
        compatibility = previous['source'].get('compatibility')
    require(isinstance(compatibility, dict)
            and artifact.strings(compatibility.get('readable_bank_schemas'))
            and artifact.strings(compatibility.get('migrator_source_schemas'), allow_empty=True),
            'Prior runtime readability metadata is missing; preserve the lifecycle transaction')
    repositories = journal.get('rollback_repositories', [])
    require(isinstance(repositories, list) and len(repositories) <= 1024
            and all(isinstance(p, str) and absolute(p) == Path(p) for p in repositories),
            'Malformed rollback repository inventory')
    from . import updating
    updating.banks(repositories, compatibility, require_readable=True)


def preserve_preexchange_cache_writer(state, journal):
    """End only a proved unactivated source-cache race, preserving every byte."""
    if (not journal.get('opaque_cache_recovery') or journal.get('activated', 0)
            or journal.get('receipt_identity') is not None or not journal['actions']):
        return
    action = journal['actions'][0]
    if action['kind'] != 'source' or action['before'] is None:
        return
    path, stage = Path(action['path']), Path(action['stage'])
    if fingerprint(path) == action['before']:
        return
    old = read_receipt(state)
    if (old is None or sha(encoded(old)) != journal['opaque_cache_recovery'].get('receipt_sha256')
            or journal['opaque_cache_recovery'].get('old_source_identity') != action['before']
            or path != Path(old['source']['path']) or not action.get('backup')
            or fingerprint(stage) != action['after']
            or fingerprint(Path(action['backup'])) != action['before']
            or any(fingerprint(Path(item['path'])) != item['before'] for item in journal['actions'][1:])):
        return
    from . import cache_recovery as cache
    try:
        qualification = cache.qualify(path, old)
        cache.recheck(qualification)
    except (ContractError, OSError, ValueError):
        return  # Unknown changes retain the existing strict conflict path.
    if (fingerprint(stage) != action['after'] or fingerprint(Path(action['backup'])) != action['before']
            or read_receipt(state) != old
            or any(fingerprint(Path(item['path'])) != item['before'] for item in journal['actions'][1:])):
        return
    if journal.get('preserved_opaque_source', {}).get('identity') == qualification['source_identity']:
        return
    # Keep the exact old journal before adding separately bound race evidence.
    prior = state/'journal.json'
    raw = prior.read_bytes()
    history = state/('journal-opaque-preexchange-' + journal['transaction'] + '-' + sha(raw) + '.json')
    if not os.path.lexists(history):
        write_file(history, raw)
    else:
        trust_paths.record(history)
        require(history.read_bytes() == raw, 'Preserved preexchange journal differs; retain recovery evidence')
    journal['preserved_opaque_source'] = {
        'path': str(path), 'identity': qualification['source_identity'],
        'qualification': qualification['summary'],
        'prior_journal': {'path': str(history), 'sha256': sha(raw)},
    }


def rollback(state, journal):
    trust_paths.state(state)
    rollback_compatibility(state, journal)
    preserve_preexchange_cache_writer(state, journal)
    conflicts = []
    for action in reversed(journal['actions']):
        path, stage = Path(action['path']), Path(action['stage'])
        unchanged_parent(path)
        trust_paths.tree(path)
        trust_paths.tree(stage)
        current, staged = fingerprint(path), fingerprint(stage)
        preserved = journal.get('preserved_opaque_source', {})
        if (current == action['before'] or (action['kind'] == 'source'
                and preserved.get('path') == action['path'] and current == preserved.get('identity'))):
            continue
        if current != action['after'] or (action['before'] is not None and staged != action['before']):
            conflicts.append(str(path)); continue
        if action['kind'] in {'source', 'copy', 'link', 'wrapper'} and action['before'] is not None:
            rollback_compatibility(state, journal)
        if action['after'] is None:
            native_rename(stage, path)
        elif action['before'] is None:
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
    return {'operation': 'recover', 'verified': not bool(journal.get('opaque_cache_recovery')),
            'status': 'opaque-cache-recovery-pending' if journal.get('opaque_cache_recovery') else 'rolled-back',
            **({'repair': 'The exact previous tree was restored without executing it. Its caches remain untrusted; retry from the trusted signed installer, preserving this journal and backup.'}
               if journal.get('opaque_cache_recovery') else {}),
            'preserved_backups': [p for a in journal['actions'] for p in (a['stage'], a.get('backup'))
                                  if p is not None and fingerprint(Path(p)) is not None]}


def read_journal(state):
    """Validate recovery metadata without changing runtime targets or records."""
    trust_paths.state(state)
    path = state/'journal.json'
    if not os.path.lexists(path):
        return None
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
    return journal


def recover(state, *, for_install=False):
    path = state/'journal.json'
    journal = read_journal(state)
    if journal is None:
        return {'operation': 'recover', 'verified': True, 'status': 'no-pending-transaction'}
    if journal['status'] == 'complete':
        require(journal.get('receipt_identity') is not None
                and fingerprint(state/'receipt.json') == journal['receipt_identity'],
                'Completed installation receipt is missing or changed; preserve the journal and reconcile ownership')
        if for_install:
            # Installation will re-plan ownership, including allowed surrounding
            # edits and explicit development-source changes, before any activation.
            return {'operation': 'recover', 'verified': False, 'status': 'install-validation-pending'}
        if not journal['receipt'].get('uninstalled'):
            verify_targets(journal['receipt']['targets'], source=journal['receipt']['source'])
        return {'operation': 'recover', 'verified': True, 'status': 'complete'}
    if journal['status'] == 'rolled-back':
        if for_install:
            return {'operation': 'recover', 'verified': False, 'status': 'install-validation-pending'}
        for action in journal['actions']:
            preserved = journal.get('preserved_opaque_source', {})
            expected = (preserved['identity'] if action['kind'] == 'source'
                        and preserved.get('path') == action['path'] else action['before'])
            require(fingerprint(Path(action['path'])) == expected,
                    'Rolled-back target changed; preserve and reconcile: ' + action['path'])
        return {'operation': 'recover', 'verified': not bool(journal.get('opaque_cache_recovery')),
                'status': 'opaque-cache-recovery-pending' if journal.get('opaque_cache_recovery') else 'rolled-back',
                **({'repair': 'The previous source is preserved but its opaque caches remain untrusted. Retry using the trusted signed installer; do not execute old runtime caches.'}
                   if journal.get('opaque_cache_recovery') else {})}
    # Receipt activation is the commit point; a lost final response must not undo it.
    receipt = state/'receipt.json'
    if journal.get('receipt_identity') is not None and fingerprint(receipt) == journal['receipt_identity']:
        verify_targets(journal['receipt']['targets'])
        journal['status'] = 'complete'; replace_record(path, journal)
        return {'operation': 'recover', 'verified': True, 'status': 'complete'}
    return rollback(state, journal)


def install(plan, *, before_activate=None, after_activate=None):
    operation = plan.get('operation', 'install')
    state = plan['state']; old = plan['old']; changed = [a for a in plan['actions'] if a['changed']]
    trust_paths.state(state)
    if plan.get('cache_recovery') is not None:
        from . import cache_recovery as cache
        require(old == plan['cache_recovery']['receipt'] and read_receipt(state) == old,
                'Installation receipt changed after cache qualification')
        cache.recheck(plan['cache_recovery'])
    compatibility = None
    if operation != 'uninstall':
        from . import updating
        compatibility = updating.banks(plan.get('rollback_repositories', []), plan['source']['compatibility'])
        plan['rollback_repositories'] = compatibility['observed_repositories']
    if (operation != 'uninstall' and not changed and old and old['source'] == plan['source']
            and all(t['identity'] == next(a['after'] for a in plan['actions'] if a['path']==t['path'])
                    for t in old['targets'])):
        verify_targets(old['targets'], source=old['source'])
        return {'operation': operation, 'verified': True, 'changed': False, 'receipt': old,
                'restart_clients': [], 'receipt_path': str(state/'receipt.json')}
    transaction = uuid.uuid4().hex
    targets = []
    for action in plan['actions']:
        target = {'path': action['path'], 'kind': action['kind'], 'identity': action['after'],
                  'pre_change_identity': action['before']}
        if action['kind'] in ('copy','link','hook-link','hook-config'):
            target['artifact_digest'] = plan['source']['artifact_digest']
        if action['kind'] == 'hook-config' and operation != 'uninstall':
            target['hook_entry'] = action['hook_entry']
            target['hook_original_shape'] = action['hook_original_shape']
            if action.get('stop_entry'):
                target['stop_entry'] = action['stop_entry']
                target['stop_original_shape'] = action['stop_original_shape']
            previous = next((t for t in (old['targets'] if old else []) if t['path']==action['path']), None)
            target['originally_absent'] = previous['originally_absent'] if previous else action['before'] is None
        if action['kind'] == 'activation' and operation != 'uninstall':
            target['block_hash'] = sha(plan['shim'].encode())
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
               'artifact_files': {} if operation == 'uninstall' else artifact.inspect(plan['pack_source'])['files'],
               'backups': list(old.get('backups', [])) if old else [], 'created_directories': [],
               'rollback_available': True, 'atomicity': 'per target; journaled rollback across targets'}
    if operation == 'uninstall':
        receipt.update(uninstalled=True, uninstalled_at=receipt['installed_at'],
                       preserved=plan['preserved'], reinstall=plan['reinstall'])
    journal = {'schema': JOURNAL_SCHEMA, 'operation': operation, 'transaction': transaction, 'status': 'staging',
               'actions': [], 'receipt': receipt, 'receipt_identity': None, 'created_directories': [],
               'previous_compatibility': old['source']['compatibility'] if old else None,
               'rollback_repositories': plan.get('rollback_repositories', [])}
    if plan.get('cache_recovery') is not None:
        journal['opaque_cache_recovery'] = plan['cache_recovery']['summary']
    if os.path.lexists(state/'journal.json'):
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
            if action['after'] is None:
                pass  # The old node moves to its rollback stage during removal.
            elif action['kind'] == 'hook-link':
                runtime_faults.mutate('link-stage', lambda: stage.symlink_to(action['after']['target']))
            elif action['kind'] == 'link':
                runtime_faults.mutate('link-stage', lambda: stage.symlink_to(plan['source']['pack'], target_is_directory=True))
            elif action['kind'] in ('copy','source'):
                copy_node(Path(action['source_path']), stage, private_root=True)
            else:
                write_file(stage, action['raw'], action['mode_bits'])
            if action['after'] is not None:
                sync_tree(stage)
            sync_dir(stage.parent)
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
        for parent in sorted({Path(a['path']).parent for a in journal['actions']}):
            with runtime_faults.temporary_probe(), tempfile.TemporaryDirectory(prefix='.agent-memory-probe-', dir=parent) as probe:
                left, right = Path(probe)/'left', Path(probe)/'right'
                left.mkdir(); (left/'payload').write_bytes(b'left'); right.mkdir()
                _native_rename(left, right, exchange=True)
                _native_rename(right, Path(probe)/'third')
        for action in changed:
            require(fingerprint(Path(action['path'])) == action['before'],
                    'Target changed after preflight: ' + action['path'])
        if before_activate:
            before_activate()
        if compatibility is not None:
            updating.banks(plan['rollback_repositories'], plan['source']['compatibility'])
        journal['status'] = 'activating'; replace_record(state/'journal.json', journal)
        for index, action in enumerate(journal['actions']):
            path, stage = Path(action['path']), Path(action['stage'])
            fault(operation + '-before-activate-' + str(index))
            unchanged_parent(path)
            trust_paths.state(state)
            trust_paths.tree(path)
            trust_paths.tree(stage)
            require(fingerprint(path) == action['before'] and fingerprint(stage) == action['after'],
                    'Target/stage changed immediately before activation: ' + str(path))
            if action['after'] is None:
                native_rename(path, stage)
            else:
                native_rename(stage, path, exchange=action['before'] is not None)
            sync_dir(path.parent)
            fault(operation + '-after-activate-' + str(index))
            require(fingerprint(path) == action['after']
                    and fingerprint(stage) == action['before'], 'Activation identity readback failed')
            journal['activated'] = index + 1; replace_record(state/'journal.json', journal)
        verify_targets(targets)
        if after_activate:
            after_activate()
        if compatibility is not None:
            updating.banks(plan['rollback_repositories'], plan['source']['compatibility'])
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
        fault(operation + '-before-receipt')
        replace_record(receipt_path, receipt)
        require(fingerprint(receipt_path) == journal['receipt_identity'], 'Receipt readback failed')
        fault(operation + '-after-receipt')
        journal['status'] = 'complete'; replace_record(state/'journal.json', journal)
        return {'operation': operation, 'verified': True, 'changed': True, 'receipt': receipt,
                **({'cache_recovery': {**plan['cache_recovery']['summary'], 'planned': False,
                                      'old_runtime_preserved': True,
                                      'preserved_source_backups': [b['path'] for b in receipt['backups']
                                          if b.get('original_path') == plan['cache_recovery']['root']]}}
                   if plan.get('cache_recovery') is not None else {}),
                'restart_clients': plan['restart'], 'receipt_path': str(receipt_path)}
    except Exception:
        # If the commit point landed, recovery verifies it; otherwise it restores old targets.
        recover(state)
        raise


def failure_repair(exc):
    from .release_distribution import DistributionError
    if isinstance(exc, DistributionError):
        return ('Preserve the active runtime and receipts. Check the trusted release, curl and OpenSSH '
                'signature support from an operator-trusted installer; never fall back to unsigned bytes.')
    if isinstance(exc, trust_paths.TrustPathError):
        return ('Preserve this path and its contents. Use a trusted outside helper if the installed runtime '
                'may have been exposed. Review ownership and permissions yourself or choose private per-user '
                'XDG/client paths; the installer does not chmod or adopt an untrusted layout.')
    if isinstance(exc, LayoutError):
        return ('Choose separate writable XDG state/data and client targets outside Memory Banks and source trees; '
                'rerun install --check. Do not run recovery under the refused layout.')
    try:
        state, _ = locations()
        journal = read_journal(state)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError):
        return ('Preserve reported paths and installation records; inspect diagnostics and reconcile '
                'unreadable recovery metadata before retrying.')
    if journal and journal['status'] not in ('complete', 'rolled-back'):
        return 'Preserve reported paths; rerun this trusted helper with recover before retrying installation.'
    return ('Preserve reported paths; resolve the reported planning or prerequisite error, then rerun '
            'install --check before retrying. No pending activation requires recovery.')


def main(argv=None):
    global BOOTSTRAP_SOURCE
    from . import cli
    argv = cli.arguments(sys.argv[1:] if argv is None else argv)
    if argv and argv[0] in MEMORY_ROUTES:
        return cli.execute(argv[0], argv[1:], MEMORY_ROUTES[argv[0]])
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in MEMORY_ROUTES:
        commands.add_parser(name, help='Shared memory operation; use ' + name + ' --help')
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
    update = commands.add_parser('update', help='Check or activate a verified managed-channel update')
    update.add_argument('--check', action='store_true')
    update.add_argument('--repo', action='append', default=[])
    uninstall = commands.add_parser('uninstall', help='Remove only unchanged owned runtime targets; preserve project memory')
    uninstall.add_argument('--check', '--dry-run', action='store_true')
    purge = commands.add_parser('purge-data', help='Explicitly preview or confirm deletion of one exact local project bank')
    purge.add_argument('--repo', required=True)
    purge.add_argument('--path', required=True)
    consent = purge.add_mutually_exclusive_group(required=True)
    consent.add_argument('--check', action='store_true')
    consent.add_argument('--confirm')
    commands.add_parser('recover', help='Verify a committed transaction or roll back interrupted activation')
    for name in ('version', 'status', 'doctor'):
        diagnostic = commands.add_parser(name, help='Read-only runtime and bank diagnostics')
        diagnostic.add_argument('--json', action='store_true')
        if name != 'version':
            diagnostic.add_argument('--repo', default='.')
        if name == 'doctor':
            diagnostic.add_argument('--projection')
            diagnostic.add_argument('--fix', action='store_true', help='Repair safely identified owned runtime faults and recheck')
    args = parser.parse_args(argv)
    if args.command in ('version', 'status', 'doctor'):
        from .diagnostics import execute
        return execute(args)
    if args.command == 'update':
        from .updating import execute
        return execute(args)
    if args.command == 'uninstall':
        from .uninstallation import execute
        return execute(args)
    if args.command == 'purge-data':
        from .purging import execute
        return execute(args)
    try:
        require(sys.version_info >= (3,11) and shutil.which('git'), 'Python 3.11+ and Git are required')
        state, code = locations()
        old = read_receipt(state)
        preflight_layout(args, state, code, old)
        default_release = (args.command == 'install' and not args.development_source and not args.remote and not args.ref
                           and (old is None or old['source']['kind'] == 'release'
                                or (old['source']['kind'] == 'managed' and old['source']['remote'] == DEFAULT_REMOTE
                                    and old['source']['ref'] == 'main')))
        if default_release and BOOTSTRAP_SOURCE is None:
            # Prove the actual verification prerequisite before even creating
            # lifecycle lock/owner records; revalidate this staged source under lock.
            reuse = False
            if old and old['source']['kind'] == 'release':
                from . import updating
                _, qualification = updating.qualified_current(state, code)
                reuse = qualification is None
            if not reuse:
                from . import release_distribution as release
                with tempfile.TemporaryDirectory(prefix='am-preverified-install-') as temporary:
                    root = Path(temporary)/'source'; release.acquire(root)
                    BOOTSTRAP_SOURCE = root
                    try:
                        return main(argv)
                    finally:
                        BOOTSTRAP_SOURCE = None
        if args.command == 'recover' and not os.path.lexists(state/'journal.json'):
            result = {'operation':'recover', 'verified':True, 'status':'no-pending-transaction',
                      'writes_performed':False}
        elif args.command == 'install' and args.check:
            require(not os.path.lexists(state/'journal.json') or load(state/'journal.json')['status'] in ('complete','rolled-back'),
                    'An interrupted transaction requires recover before a new install plan')
            with prepare(args, state, code, read_receipt(state)) as plan:
                result = public_plan(plan)
                from . import updating
                result['compatibility'] = updating.banks([], plan['source']['compatibility'])
        else:
            with runtime_faults.operation(args.command, lifecycle_paths(args, state, code, read_receipt(state))), locked(state):
                recovered = recover(state, for_install=args.command == 'install')
                if args.command == 'recover':
                    result = recovered
                else:
                    with prepare(args, state, code, read_receipt(state)) as plan:
                        result = install(plan)
        runtime_faults.response(args.command, result)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, subprocess.SubprocessError) as exc:
        result = {'operation': args.command, 'verified': False, 'error': str(exc), 'repair': failure_repair(exc)}
    print(json.dumps(result, indent=2))
    return 0 if result.get('verified') or result.get('schema') == 'agent-memory/install-plan-v1' else 1
