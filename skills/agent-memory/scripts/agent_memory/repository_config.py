"""One repository bank location; missing local settings never establish absence."""
import os
from pathlib import Path, PurePosixPath
import re
import unicodedata

from .frontmatter import ContractError, read
from .identity_git import git

MAX_ROUTERS = 64
MAX_DISCOVERY_ENTRIES = 1000000
ROUTER_FIELDS = {'schema', 'project-id', 'router-updated', 'canonical-remote'}


def local_value(root, key):
    raw = git(root, 'config', '--local', '--null', '--get-all', key, optional=True)
    if raw is None:
        return None
    values = raw.split('\0')
    if values[-1] == '':
        values.pop()
    if len(values) != 1 or not values[0]:
        raise ContractError('Repository memory configuration needs one explicit value per key')
    return values[0]


def safe_label(value):
    # Import at operation time: the security module consumes the shared writer.
    from .security import scan
    if scan(value)['reasons']:
        raise ContractError('Repository boundary label requires safety review; preserve it without echoing its value')


def validate_root(relative):
    if not isinstance(relative, str):
        raise ContractError('Memory root requires an explicit repository-relative directory')
    path = PurePosixPath(relative)
    if (not relative or len(relative.encode()) > 256 or path.is_absolute() or str(path) != relative
            or any(part.startswith('.') for part in path.parts) or relative == '.'
            or any(ord(c) < 32 or c in '\\:\x7f\x85\u2028\u2029' for c in relative)):
        raise ContractError('Memory bank root must be a bounded canonical repository-relative directory')
    safe_label(relative)
    if unicodedata.normalize('NFC', relative).casefold() == 'memory-bank' and relative != 'memory-bank':
        raise ContractError('Default memory bank directory has a portable spelling alias')
    return relative


def validate_branch(branch):
    if (not isinstance(branch, str) or not branch or len(branch.encode()) > 256
            or branch.startswith(('-', '/')) or branch.endswith(('/', '.')) or branch == '@'
            or '..' in branch or '@{' in branch or '//' in branch
            or any(part.startswith('.') or part.endswith('.lock') for part in branch.split('/'))
            or any(c.isspace() or ord(c) < 32 or c in '~^:?*[\\\x7f' for c in branch)):
        raise ContractError('Authority branch must be an explicit safe Git branch name')
    safe_label(branch)
    return branch


def router_kind(raw):
    """Recognize a locator, never evaluate its body or establish source authority."""
    if not isinstance(raw, bytes):
        raise ContractError('Cannot inspect possible existing memory; do not infer absence')
    marker = re.search(br'(?m)^schema:[ \t]*[\"\x27]?agent-memory/', raw[:16384])
    if not marker:
        return None
    if raw.startswith(b'---\r\n'):
        raise ContractError('Frontmatter must start at byte zero with LF separators; inspect the existing router')
    try:
        if len(raw) > 16384:
            raise ContractError('Possible router exceeds its bound')
        metadata, _ = read(raw.decode('utf-8'), allowed=ROUTER_FIELDS)
        if metadata.get('schema') != 'agent-memory/v1':
            raise ContractError('Unknown existing memory schema')
    except (ContractError, UnicodeError):
        raise ContractError('Possible existing memory needs repair or explicit inspection, not another bank')
    return True


def router_bytes(root, relative):
    target = root / relative
    parts = [root.joinpath(*relative.parts[:n]) for n in range(1, len(relative.parts) + 1)]
    if any(part.is_symlink() for part in parts):
        raise ContractError('A possible memory locator crosses a symlink; inspect without adopting it')
    if target.is_file():
        with target.open('rb') as stream:
            return stream.read(16385)
    # A pending deletion cannot hide a tracked bank from location discovery.
    value = git(root, 'show', '--no-textconv', ':' + relative.as_posix(), optional=True)
    if value is None:
        raise ContractError('Tracked memory locator is unavailable; do not infer an empty repository')
    return value.encode('utf-8')


def git_roots(root):
    listed = git(root, 'ls-files', '--cached', '--others', '--exclude-standard', '-z', optional=True)
    if listed is None:
        raise ContractError('Repository memory inventory is unavailable; do not infer absence')
    paths = sorted({PurePosixPath(path) for path in listed.split('\0') if path
                    and PurePosixPath(path).name.casefold() == 'memory.md'
                    and not any(part.startswith('.') for part in PurePosixPath(path).parts)})
    if len(paths) > MAX_ROUTERS:
        raise ContractError('Memory locator inventory exceeds its bound; incomplete discovery cannot prove absence')
    roots = set()
    for path in paths:
        if path.is_absolute() or '..' in path.parts:
            raise ContractError('Invalid possible repository memory locator')
        if router_kind(router_bytes(root, path)):
            roots.add(path.parent.as_posix())
    return roots


def physical_others(root, selected):
    """Only when no selected router is present: include ignored/untracked banks."""
    examined, routers = 0, 0
    def unreadable(_):
        raise ContractError('Repository discovery is incomplete; do not create a bank from an unreadable inventory')
    for directory, names, files in os.walk(root, followlinks=False, onerror=unreadable):
        directory = Path(directory)
        # An embedded independent repository has its own one-bank namespace.
        if directory != root and (directory / '.git').exists():
            names[:] = []
            continue
        examined += len(names) + len(files)
        if examined > MAX_DISCOVERY_ENTRIES:
            raise ContractError('Physical memory discovery exceeds its bound; absence remains unknown')
        names[:] = sorted(name for name in names if not name.startswith('.')
                          and not (directory / name).is_symlink() and directory / name != selected)
        for name in files:
            if name.casefold() != 'memory.md':
                continue
            routers += 1
            if routers > MAX_ROUTERS:
                raise ContractError('Possible memory locator inventory exceeds its bound')
            target = directory / name
            if target.is_symlink():
                raise ContractError('A possible existing memory locator is a symlink; inspect it explicitly')
            with target.open('rb') as stream:
                raw = stream.read(16385)
            if router_kind(raw) and directory != selected:
                raise ContractError('Existing memory lies outside the selected bank; configure its location before initialization')


def settings(repo):
    root = Path(repo).resolve()
    top = git(root, 'rev-parse', '--show-toplevel', optional=True)
    if top is None and (root / '.git').exists():
        raise ContractError('Existing Git metadata/configuration cannot be inspected; do not guess the bank root')
    if top is not None and Path(top).resolve() != root:
        raise ContractError('Select the actual repository root for its one memory bank')
    explicit = local_value(root, 'agent-memory.bankRoot')
    if explicit is not None:
        validate_root(explicit)
    roots = git_roots(root) if top is not None else set()
    if len(roots) > 1 or (explicit is not None and any(value != explicit for value in roots)):
        raise ContractError('Competing memory bank locations require explicit reconciliation; preserve every source')
    relative = explicit or (next(iter(roots)) if roots else 'memory-bank')
    validate_root(relative)
    current = root
    for part in PurePosixPath(relative).parts:
        if current.is_dir():
            key = unicodedata.normalize('NFC', part).casefold()
            for entry in current.iterdir():
                if unicodedata.normalize('NFC', entry.name).casefold() == key and entry.name != part:
                    raise ContractError('Configured bank directory has a portable spelling alias')
        current = current / part
        if current.is_symlink() or (current.exists() and not current.is_dir()):
            raise ContractError('Configured bank directory cannot follow symlinks or occupied files')
    if not (current / 'MEMORY.md').is_file():
        physical_others(root, current)
    branch = local_value(root, 'agent-memory.authorityBranch')
    if branch is not None:
        validate_branch(branch)
    return {'bank_root': relative, 'authority_branch': branch}


def bank_path(repo):
    return Path(repo).resolve() / settings(repo)['bank_root']


def relative(repo):
    return settings(repo)['bank_root']
