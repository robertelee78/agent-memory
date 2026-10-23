"""One explicit local repository bank configuration; no client namespace."""
from pathlib import Path, PurePosixPath
import unicodedata

from .frontmatter import ContractError
from .identity_git import git


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


def settings(repo):
    root = Path(repo).resolve()
    top = git(root, 'rev-parse', '--show-toplevel', optional=True)
    if top is None and (root / '.git').exists():
        raise ContractError('Existing Git metadata/configuration cannot be inspected; do not guess the bank root')
    if top is not None and Path(top).resolve() != root:
        raise ContractError('Select the actual repository root for its one memory bank')
    relative = local_value(root, 'agent-memory.bankRoot') or 'memory-bank'
    path = PurePosixPath(relative)
    if (not relative or len(relative.encode()) > 256 or path.is_absolute() or str(path) != relative
            or any(part.startswith('.') for part in path.parts) or relative == '.'
            or any(ord(c) < 32 or c in '\\:\x7f\x85\u2028\u2029' for c in relative)):
        raise ContractError('Memory bank root must be a bounded canonical repository-relative directory')
    current = root
    for part in path.parts:
        if current.is_dir():
            key = unicodedata.normalize('NFC', part).casefold()
            for entry in current.iterdir():
                if unicodedata.normalize('NFC', entry.name).casefold() == key and entry.name != part:
                    raise ContractError('Configured bank directory has a portable spelling alias')
        current = current / part
        if current.is_symlink() or (current.exists() and not current.is_dir()):
            raise ContractError('Configured bank directory cannot follow symlinks or occupied files')
    if relative != 'memory-bank' and (root / 'memory-bank/MEMORY.md').exists():
        raise ContractError('A second default bank remains; explicitly reconcile the repository bank location')
    branch = local_value(root, 'agent-memory.authorityBranch')
    if branch is not None:
        validate_branch(branch)
    return {'bank_root': relative, 'authority_branch': branch}


def validate_branch(branch):
    if (not isinstance(branch, str) or not branch or len(branch.encode()) > 256
            or branch.startswith(('-', '/')) or branch.endswith(('/', '.')) or branch == '@'
            or '..' in branch or '@{' in branch or '//' in branch
            or any(part.startswith('.') or part.endswith('.lock') for part in branch.split('/'))
            or any(c.isspace() or ord(c) < 32 or c in '~^:?*[\\\x7f' for c in branch)):
        raise ContractError('Authority branch must be an explicit safe Git branch name')
    return branch


def bank_path(repo):
    return Path(repo).resolve() / settings(repo)['bank_root']


def relative(repo):
    return settings(repo)['bank_root']
