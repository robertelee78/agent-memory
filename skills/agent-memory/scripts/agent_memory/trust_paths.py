"""Read-only Unix ownership/mode checks for a trusted lifecycle verifier.

These checks cannot authenticate Python code that has already been imported.
An installation exposed to an attacker needs a trusted outside verifier/helper.
Repository Memory Banks intentionally have a different collaboration policy.
"""
from __future__ import annotations

import os
from pathlib import Path
import stat
import sys

from .frontmatter import ContractError


class TrustPathError(ContractError):
    """Preserve untrusted lifecycle paths instead of reading or repairing them."""


def refuse(path, reason):
    raise TrustPathError('Unsafe lifecycle trust path: ' + str(path) + ': ' + reason)


def inspect(path, *, private_directory=False, sensitive_record=False,
            allow_final_link=False):
    """Check root-to-leaf reachability without mkdir, chmod or following links.

    Callers canonicalize intentional platform aliases before entering here.
    A private barrier protects descendants, never its own replaceable entry.
    Root-owned sticky parents protect only trusted-owned direct entries.
    """
    path = Path(path)
    if not path.is_absolute() or '..' in path.parts:
        refuse(path, 'an absolute canonical path is required')
    current_uid = os.geteuid()
    protected = False
    leaf = None
    node = Path(path.anchor)
    for part in (None, *path.parts[1:]):
        if part is not None:
            node = node / part
        try:
            value = node.lstat()
        except FileNotFoundError:
            break
        final = node == path
        mode = value.st_mode
        if value.st_uid not in {current_uid, 0}:
            refuse(node, 'owner is neither the executing user nor root')
        if stat.S_ISLNK(mode):
            if not final or not allow_final_link:
                refuse(node, 'a trusted ordinary path must not be redirected by a symlink')
            leaf = value
            break  # Link mode bits do not describe permissions of its target.
        if not stat.S_ISDIR(mode) and not (final and stat.S_ISREG(mode)):
            refuse(node, 'an ordinary directory or regular file is required')
        if not protected and mode & 0o022:
            if not (stat.S_ISDIR(mode) and value.st_uid == 0 and mode & stat.S_ISVTX):
                refuse(node, 'group/other write access permits an untrusted principal to change it')
        # A hardlink outside a private barrier could expose a writable payload.
        if stat.S_ISREG(mode) and value.st_nlink != 1 and mode & 0o022:
            refuse(node, 'a writable payload has aliases outside its checked path')
        if stat.S_ISDIR(mode) and value.st_uid == current_uid and not mode & 0o077:
            protected = True
        if final:
            leaf = value
    if leaf is not None and private_directory:
        if not stat.S_ISDIR(leaf.st_mode) or leaf.st_uid != current_uid or leaf.st_mode & 0o077:
            refuse(path, 'installation state must be a private current-user-owned directory (0700)')
    if leaf is not None and sensitive_record:
        if (not stat.S_ISREG(leaf.st_mode) or leaf.st_uid != current_uid
                or leaf.st_mode & 0o022 or leaf.st_nlink != 1):
            refuse(path, 'lifecycle records must be current-user-owned, regular, single-link and not group/other writable')
    return {'path': str(path), 'present': leaf is not None,
            'unix_path_policy_verified': True, 'acl_audited': False,
            'executing_verifier_authenticated': False, 'writes_performed': False}


def tree(path, *, git_metadata=False):
    """Check owned runtime bytes before Git/config/activation consumes them."""
    path = Path(path)
    inspect(path, allow_final_link=True)
    if path.is_symlink() or not path.is_dir():
        return
    for parent, dirs, files in os.walk(path, followlinks=False,
                                       onerror=lambda exc: (_ for _ in ()).throw(exc)):
        for name in dirs + files:
            node = Path(parent) / name
            # Git metadata may affect executable transports and hooks. Do not
            # follow a redirected config/object directory before checking it.
            inspect(node, allow_final_link=not git_metadata and '.git' not in node.relative_to(path).parts)


def source(path):
    """Include linked-worktree Git config before invoking Git on a source."""
    path = Path(path)
    inspect(path)
    tree(path)
    marker = path / '.git'
    if marker.is_file():
        inspect(marker)
        if marker.stat().st_size > 4096:
            refuse(marker, 'Git directory pointer exceeds its bound')
        # Match Git's CR/LF handling, retaining real spaces in path components:
        # https://github.com/git/git/blob/v2.50.1/setup.c
        text = marker.read_text().rstrip('\r\n')
        if not text.startswith('gitdir: '):
            refuse(marker, 'unrecognized Git directory pointer')
        directory = git_pointer(path, text[8:])
    elif marker.is_dir():
        directory = marker
    elif (path / 'HEAD').is_file() and (path / 'objects').is_dir():
        directory = path
    else:
        return
    tree(directory, git_metadata=True)
    common = directory / 'commondir'
    if common.exists():
        inspect(common)
        if common.stat().st_size > 4096:
            refuse(common, 'Git common-directory pointer exceeds its bound')
        tree(git_pointer(directory, common.read_text().rstrip('\r\n')), git_metadata=True)


def git_pointer(base, text):
    """Validate the locator Git will follow, not just its resolved destination."""
    if not text or any(ord(c) < 32 or ord(c) == 127 for c in text):
        refuse(base, 'unsupported empty/control-containing Git directory pointer')
    supplied = Path(text)
    node = Path(supplied.anchor) if supplied.is_absolute() else Path(base)
    parts = supplied.parts[1:] if supplied.is_absolute() else supplied.parts
    for part in parts:
        if part == '..':
            inspect(node)
            node = node.parent
        elif part != '.':
            node = node / part
        # Only fixed, root-owned Darwin aliases are canonicalized. A mutable
        # arbitrary alias must not disappear before ownership/ancestor checks.
        aliases = {'/tmp': '/private/tmp', '/var': '/private/var', '/etc': '/private/etc'}
        if sys.platform == 'darwin' and str(node) in aliases and node.is_symlink():
            value = node.lstat()
            destination = Path(os.path.abspath(node.parent / os.readlink(node)))
            if value.st_uid != 0 or destination != Path(aliases[str(node)]):
                refuse(node, 'an unexpected platform alias cannot locate trusted Git metadata')
            node = destination
        inspect(node)
    if not node.is_dir():
        refuse(node, 'Git metadata directory is unavailable')
    return node


def state(path):
    return inspect(path, private_directory=True)


def record(path):
    return inspect(path, sensitive_record=True)
