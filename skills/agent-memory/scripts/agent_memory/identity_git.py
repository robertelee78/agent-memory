"""Local read-only identity plumbing, without ambient Git redirection or hooks."""
import os
import re
import subprocess

from .frontmatter import ContractError


def _run(repo, *args, glob_pathspecs=False):
    env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
    env.update(GIT_NO_LAZY_FETCH='1', GIT_ALLOW_PROTOCOL='', GIT_OPTIONAL_LOCKS='0',
               GIT_TRACE='0', GIT_TRACE2='0', GIT_TRACE2_EVENT='0', GIT_TRACE2_PERF='0')
    return subprocess.run(['git', '--no-pager', '--no-replace-objects', '--glob-pathspecs' if glob_pathspecs else '--literal-pathspecs',
                           '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false',
                           '-C', str(repo), *args], env=env, capture_output=True, timeout=10)


def git(repo, *args, optional=False, glob_pathspecs=False):
    try:
        command = _run(repo, *args, glob_pathspecs=glob_pathspecs)
    except (FileNotFoundError, subprocess.TimeoutExpired) as exc:
        if optional:
            return None
        raise ContractError('Cannot inspect local repository identity evidence') from exc
    if command.returncode:
        if optional:
            return None
        raise ContractError('Cannot inspect repository evidence with Git; no identity change performed')
    # Remove the command terminator only. Spaces and newlines can be part of a path.
    return os.fsdecode(command.stdout).removesuffix('\n')


def root_head(repo):
    """Observe a complete root and optional immutable HEAD without trimming paths."""
    try:
        command = _run(repo, 'rev-parse', '--show-toplevel', '--verify', '--quiet', 'HEAD')
    except (OSError, subprocess.TimeoutExpired):
        return None, None
    if command.returncode:
        # Only a completed positive failure can retain rev-parse's root output.
        # A killed process has not completed even if it emitted a plausible root.
        if command.returncode > 0 and command.stdout.endswith(b'\n') and command.stdout != b'\n':
            return os.fsdecode(command.stdout[:-1]), None
        return None, None
    output = os.fsdecode(command.stdout)
    root, separator, head = output[:-1].rpartition('\n') if output.endswith('\n') else ('', '', '')
    if not root or not separator or not re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}', head):
        raise ContractError('Repository root/HEAD observation is incomplete; do not infer absence')
    return root, head
