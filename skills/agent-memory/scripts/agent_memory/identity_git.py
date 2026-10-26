"""Local read-only identity plumbing, without ambient Git redirection or hooks."""
import os
import subprocess

from .frontmatter import ContractError


def git(repo, *args, optional=False, glob_pathspecs=False):
    env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
    env.update(GIT_NO_LAZY_FETCH='1', GIT_ALLOW_PROTOCOL='', GIT_OPTIONAL_LOCKS='0',
               GIT_TRACE='0', GIT_TRACE2='0', GIT_TRACE2_EVENT='0', GIT_TRACE2_PERF='0')
    try:
        command = subprocess.run(['git', '--no-pager', '--no-replace-objects', '--glob-pathspecs' if glob_pathspecs else '--literal-pathspecs',
                                  '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false',
                                  '-C', str(repo), *args], env=env, capture_output=True, timeout=10)
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
