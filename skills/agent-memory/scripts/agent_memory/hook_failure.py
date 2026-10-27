"""Private, bounded hook failure observations; absence never proves completion."""
from __future__ import annotations
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import uuid
from . import trust_paths
from .frontmatter import ContractError

SCHEMA = 'agent-memory/hook-failure-v1'
STAGES = {'event', 'repository', 'identity', 'boundary', 'guidance'}
REASONS = {'event.invalid', 'repository.unavailable', 'identity.unresolved',
           'boundary.missing', 'boundary.unsafe', 'boundary.source_mismatch',
           'state.unavailable', 'guidance.unavailable', 'internal.unexpected'}
ADAPTERS = (('codex', 'prompt'), ('codex', 'stop'))
MAX = 4096


class NoRepository(ContractError):
    pass


def paths(repo, harness, adapter):
    if (harness, adapter) not in ADAPTERS:
        raise ContractError('Unsupported hook adapter')
    repo = Path(repo).resolve(strict=True)
    try:
        found = subprocess.run(['git', '-C', str(repo), 'rev-parse', '--git-common-dir'],
                               capture_output=True, text=True, timeout=5)
    except FileNotFoundError:
        raise NoRepository('Git unavailable for hook diagnostics') from None
    if found.returncode:
        raise NoRepository('Repository unavailable for hook diagnostics')
    common = (repo / found.stdout.strip()).resolve(strict=True)
    raw = os.environ.get('AGENT_MEMORY_STATE_HOME')
    root = Path(raw or str(Path(os.environ.get('XDG_STATE_HOME', str(Path.home()/'.local/state'))) / 'agent-memory')).expanduser()
    if (not root.is_absolute() or root.resolve().is_relative_to(repo)
            or root.resolve().is_relative_to(common)
            or (common.name == '.git' and root.resolve().is_relative_to(common.parent))):
        raise ContractError('Hook diagnostics require private state outside the repository')
    directory = root / 'hook-failures'
    trust_paths.inspect(directory)
    key = hashlib.sha256((str(common)+'\0'+str(repo)+'\0'+harness+'\0'+adapter).encode()).hexdigest()
    return directory, key+'.json'


def validate(value, harness, adapter):
    if (not isinstance(value, dict) or set(value) != {'schema','harness','adapter','stage','reason','timestamp'}
            or not all(isinstance(value[k],str) for k in value)
            or value['schema'] != SCHEMA or value['harness'] != harness or value['adapter'] != adapter
            or value['stage'] not in STAGES or value['reason'] not in REASONS
            or not isinstance(value['timestamp'], str) or len(value['timestamp']) > 40):
        raise ContractError('Invalid hook failure metadata')
    try:
        date = datetime.datetime.fromisoformat(value['timestamp'])
        if date.tzinfo is None: raise ValueError()
    except ValueError:
        raise ContractError('Invalid hook failure timestamp') from None
    return value


def read_one(directory, name, harness, adapter, *, content=True):
    path = directory/name
    if not os.path.lexists(path): return None
    trust_paths.state(directory)
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    except FileNotFoundError:
        return None
    try:
        info = os.fstat(fd)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid()
                or info.st_nlink != 1 or info.st_mode & 0o077 or (content and info.st_size > MAX)):
            raise ContractError('Unsafe hook failure metadata')
        with os.fdopen(fd, 'rb', closefd=False) as stream: raw = stream.read(MAX+1)
    finally: os.close(fd)
    return validate(json.loads(raw), harness, adapter) if content else raw


def read(repo, *, errors=None):
    values = []
    for harness, adapter in ADAPTERS:
        try:
            directory, name = paths(repo, harness, adapter)
        except NoRepository:
            return []
        try:
            value = read_one(directory, name, harness, adapter)
        except (OSError, ValueError, TypeError, KeyError, UnicodeError):
            if errors is None: raise
            errors.append({'harness':harness,'adapter':adapter,'reason':'state.unavailable'})
            continue
        if value is not None: values.append(value)
    return values


def write(repo, harness, adapter, value):
    directory, name = paths(repo, harness, adapter)
    if value is None and not os.path.lexists(directory/name): return False
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    trust_paths.state(directory)
    lock = directory/(name+'.lock')
    fd = os.open(lock, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    try:
        info = os.fstat(fd)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid()
                or info.st_nlink != 1 or info.st_mode & 0o077):
            raise ContractError('Unsafe hook failure lock')
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        old = read_one(directory, name, harness, adapter, content=False)
        if value is None:
            if old is None: return False
            (directory/name).unlink()
        else:
            raw = json.dumps(validate(value,harness,adapter),sort_keys=True).encode()
            if len(raw)>MAX: raise ContractError('Hook failure metadata exceeds bound')
            temp = directory/('.'+uuid.uuid4().hex+'.tmp')
            out = os.open(temp, os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW, 0o600)
            try:
                with os.fdopen(out,'wb') as stream:
                    stream.write(raw);stream.flush();os.fsync(stream.fileno())
                os.replace(temp,directory/name)
            finally:
                if os.path.lexists(temp): temp.unlink()
        dfd = os.open(directory,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        try: os.fsync(dfd)
        finally: os.close(dfd)
        return True
    finally: os.close(fd)


def record(repo,harness,adapter,stage,reason):
    return write(repo,harness,adapter,{'schema':SCHEMA,'harness':harness,'adapter':adapter,
        'stage':stage,'reason':reason,'timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat()})


def clear(repo,harness,adapter):
    return write(repo,harness,adapter,None)
