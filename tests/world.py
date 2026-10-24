"""Disposable filesystem/Git worlds for public-interface acceptance tests.

Only the standard library, Python and Git are required. Never adopt a caller's
home, repository or credentials as a test root.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import secrets
import signal
import shutil
import subprocess
import sys
import tempfile


# The supervisor remains the process-group leader until it signals its own
# group. Worker exit/reaping therefore cannot release the group ID first.
_GROUP_SUPERVISOR = r'''
import json,os,signal,subprocess,sys,traceback
from pathlib import Path
directory=Path(sys.argv[1]);timeout=float(sys.argv[2]);argv=json.loads(sys.argv[3])
status={'supervisor_pid':os.getpid(),'worker_pid':None,'returncode':None,'timed_out':False,'error':None}
def record():
 with (directory/'status.json').open('w') as stream:
  json.dump(status,stream);stream.flush();os.fsync(stream.fileno())
try:
 if os.getpgrp()!=os.getpid():raise RuntimeError('Supervisor does not own its process group')
 with (directory/'stdout').open('xb') as out,(directory/'stderr').open('xb') as err:
  child=subprocess.Popen(argv,stdout=out,stderr=err,close_fds=True)
  status['worker_pid']=child.pid
  try:status['returncode']=child.wait(timeout=timeout)
  except subprocess.TimeoutExpired:status['timed_out']=True
except BaseException as exc:
 status['error']={'type':type(exc).__name__,'message':str(exc),'errno':getattr(exc,'errno',None)}
 traceback.print_exc()
finally:
 try:record()
 finally:
  if os.getpgrp()==os.getpid():
   try:os.killpg(os.getpid(),signal.SIGKILL)
   except OSError as exc:
    status['cleanup_error']={'type':type(exc).__name__,'message':str(exc),'errno':exc.errno};record()
    raise
'''


class World:
    def __init__(self):
        self._children = []
        # Ignore TMPDIR: it may point at a developer's project or live home.
        self._temp = tempfile.TemporaryDirectory(prefix="agent-memory-test-", dir="/tmp")
        self.root = Path(self._temp.name).resolve()
        self.root.chmod(0o700)
        self.token = secrets.token_hex(24)
        (self.root / ".test-world.json").write_text(json.dumps({"token": self.token}))
        self.paths = {name: self.root / name for name in (
            "home", "config", "data", "state", "cache", "backups", "tmp", "repo",
            "bank", "codex", "claude", "opencode", "skills", "credentials",
        )}
        for path in self.paths.values():
            path.mkdir(mode=0o700)
        git = shutil.which("git")
        if git is None:
            self.close()
            raise RuntimeError("The offline contract gate requires Git")
        # Allowlist rather than remove a handful of known secret variable names.
        path = os.pathsep.join(dict.fromkeys([
            str(Path(sys.executable).parent), str(Path(git).parent), "/usr/bin", "/bin",
        ]))
        self.env = {
            "PATH": path, "HOME": str(self.paths["home"]),
            "XDG_CONFIG_HOME": str(self.paths["config"]),
            "XDG_DATA_HOME": str(self.paths["data"]),
            "XDG_STATE_HOME": str(self.paths["state"]),
            "XDG_CACHE_HOME": str(self.paths["cache"]),
            "TMPDIR": str(self.paths["tmp"]),
            "CODEX_HOME": str(self.paths["codex"]),
            "CLAUDE_CONFIG_DIR": str(self.paths["claude"]),
            "OPENCODE_CONFIG_DIR": str(self.paths["opencode"]),
            "AGENT_MEMORY_CONFIG_HOME": str(self.paths["config"] / "agent-memory"),
            "AGENT_MEMORY_STATE_HOME": str(self.paths["state"] / "agent-memory"),
            "AGENT_MEMORY_BACKUP_HOME": str(self.paths["backups"]),
            "GH_CONFIG_DIR": str(self.paths["credentials"]),
            "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_SYSTEM": os.devnull,
            "GIT_CONFIG_GLOBAL": os.devnull, "GIT_TERMINAL_PROMPT": "0",
            "GIT_ASKPASS": "/usr/bin/false", "SSH_ASKPASS": "/usr/bin/false",
            "GIT_ALLOW_PROTOCOL": "file", "GIT_AUTHOR_NAME": "Test Developer",
            "GIT_AUTHOR_EMAIL": "test@example.invalid", "GIT_COMMITTER_NAME": "Test Developer",
            "GIT_COMMITTER_EMAIL": "test@example.invalid",
            "PYTHONDONTWRITEBYTECODE": "1", "PYTHONNOUSERSITE": "1",
            "LC_ALL": "C", "TZ": "UTC", "NO_COLOR": "1",
            "AGENT_MEMORY_TEST_ROOT": str(self.root),
            "AGENT_MEMORY_TEST_TOKEN": self.token,
        }

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def close(self):
        for child in self._children:
            if child.poll() is None:
                child.kill()
            child.communicate(timeout=10)
        self._temp.cleanup()

    def inside(self, path):
        """Refuse escape, including a symlink inside an otherwise safe world."""
        path = Path(path)
        if not path.is_absolute():
            path = self.root / path
        path = path.resolve()
        if not path.is_relative_to(self.root):
            raise ValueError("Test path escapes its disposable world")
        return path

    def run(self, args, *, cwd=None, env=None, check=True, timeout=60, group=False):
        if isinstance(args, (str, bytes)):
            raise ValueError("Commands require an argument list; shell execution is disabled")
        working = self.inside(cwd or self.paths["repo"])
        child_env = self.env.copy()
        if env:
            # Overrides may name a fixture failpoint, but cannot redirect roots or auth.
            if any(not key.startswith("AGENT_MEMORY_FAULT_") for key in env):
                raise ValueError("Only explicit fault-injection overrides are accepted")
            child_env.update(env)
        argv = [str(a) for a in args]
        if not group:
            return subprocess.run(argv, cwd=working, env=child_env,
                                  text=True, capture_output=True, check=check, timeout=timeout)
        # Keep the owned supervisor alive until descendant collection. Calling
        # killpg after communicate() reaped the leader could target a reused ID.
        with tempfile.TemporaryDirectory(prefix='owned-command-', dir=self.root) as control:
            directory = Path(control)
            process = subprocess.Popen([sys.executable, '-I', '-c', _GROUP_SUPERVISOR,
                                        control, str(timeout), json.dumps(argv)],
                                       cwd=working, env=child_env, text=True,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                       start_new_session=True)
            try:
                _control_out, control_error = process.communicate(timeout=timeout + 30)
            except subprocess.TimeoutExpired:
                # communicate has not reaped this still-owned leader. The
                # worker's own deadline is unchanged; this is a failed watchdog.
                if process.returncode is None:
                    os.killpg(process.pid, signal.SIGKILL)
                process.communicate(timeout=10)
                raise OSError('Owned command supervisor exceeded its cleanup watchdog') from None
            status_path = directory/'status.json'
            if not status_path.is_file():
                raise OSError('Owned command supervisor returned no completion record: ' + control_error)
            status = json.loads(status_path.read_text())
            if status.get('supervisor_pid') != process.pid or process.returncode != -signal.SIGKILL or status.get('cleanup_error'):
                raise OSError('Owned process-group collection failed: ' + json.dumps(status) + control_error)
            if status.get('error'):
                error = status['error']
                raise OSError(error.get('errno') or 5, error['message'])
            stdout = (directory/'stdout').read_text(encoding='utf-8')
            stderr = (directory/'stderr').read_text(encoding='utf-8')
            if status.get('timed_out') is True:
                raise subprocess.TimeoutExpired(argv, timeout, output=stdout, stderr=stderr)
            if type(status.get('returncode')) is not int or type(status.get('worker_pid')) is not int:
                raise OSError('Owned command completion record is incomplete')
            result = subprocess.CompletedProcess(argv, status['returncode'], stdout, stderr)
        if check:
            result.check_returncode()
        return result

    def git(self, *args, cwd=None, **kwargs):
        return self.run(["git", *args], cwd=cwd, **kwargs)

    def spawn(self, args, *, cwd=None):
        """Concurrent public commands with the same roots; kill/reap on cleanup."""
        if isinstance(args, (str, bytes)):
            raise ValueError("Commands require an argument list; shell execution is disabled")
        working = self.inside(cwd or self.paths["repo"])
        child = subprocess.Popen([str(a) for a in args], cwd=working, env=self.env.copy(),
                                 text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self._children.append(child)
        return child

    def repository(self, name="repo", *, bare=False):
        path = self.inside(name)
        path.mkdir(parents=True, exist_ok=True)
        self.git("init", "--initial-branch=main", *(["--bare"] if bare else []), str(path))
        return path

    def clone(self, remote, name):
        remote = self.inside(remote)
        target = self.inside(name)
        self.git("clone", "--", str(remote), str(target))
        return target


def require_world(env=None):
    """Validate the worker's test root before importing any acceptance suite."""
    env = os.environ if env is None else env
    root = Path(env.get("AGENT_MEMORY_TEST_ROOT", "/missing-test-world"))
    token = env.get("AGENT_MEMORY_TEST_TOKEN")
    if not root.is_absolute() or root.is_symlink():
        raise ValueError("Invalid test world")
    root = root.resolve()
    if root.parent != Path("/tmp").resolve() or not root.name.startswith("agent-memory-test-"):
        raise ValueError("Refusing a live path as a test world")
    marker = root / ".test-world.json"
    if marker.is_symlink() or not token or json.loads(marker.read_text()).get("token") != token:
        raise ValueError("Missing or mismatched test-world marker")
    for key in ("HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME",
                "TMPDIR", "CODEX_HOME", "CLAUDE_CONFIG_DIR", "OPENCODE_CONFIG_DIR", "GH_CONFIG_DIR",
                "AGENT_MEMORY_CONFIG_HOME", "AGENT_MEMORY_STATE_HOME", "AGENT_MEMORY_BACKUP_HOME"):
        value = Path(env.get(key, "/missing-test-path")).resolve()
        if not value.is_relative_to(root) or value == root:
            raise ValueError(f"Refusing non-isolated {key}")
    if Path.cwd().resolve().is_relative_to(root) is False:
        raise ValueError("Test worker must run inside its disposable world")
    return root
