"""Bank-wide source coordination for the native omp boundary."""
from contextlib import contextmanager
import hashlib
import re
import fcntl
import os
import stat
import uuid

from . import bank_writes, trust_paths, turn_sources
from .developer_identity import load
from .frontmatter import ContractError

CAPABILITY = 'omp-harness-v1'
MARKER = {'schema': 'agent-memory/turn-sources-compat-v1',
          'minimum_reader': CAPABILITY, 'harnesses': ['omp']}


def _base(repo):
    root, bank = bank_writes.bank_path(repo)
    operations, _ = bank_writes.state_paths(root, bank, str(uuid.uuid4()))
    return operations.parent


@contextmanager
def source_barrier(repo, *, exclusive=False):
    """Serialize lifecycle changes against source work without blocking a turn."""
    base = _base(repo)
    turn_sources.private_mkdir(base)
    fd = os.open(base / 'source-barrier.lock',
                 os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    try:
        info = os.fstat(fd)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid()
                or info.st_nlink != 1 or info.st_mode & 0o077):
            raise ContractError('Unsafe source barrier')
        try:
            fcntl.flock(fd, (fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH) | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ContractError('Source barrier busy; preserve pending work') from None
        yield
    finally:
        os.close(fd)


def required_source_capabilities(repo):
    """Read known requirements conservatively while holding the source barrier."""
    base = _base(repo)
    marker = base / 'turn-sources-compat-v1.json'
    missing = False
    try:
        value = load(turn_sources.read_bytes(marker, turn_sources.MAX_SOURCE))
    except FileNotFoundError:
        missing = True
        value = None
    if not missing and value != MARKER:
        raise ContractError('Unknown or malformed source compatibility marker')
    source = base / 'omp-source-v1'
    if os.path.lexists(source):
        trust_paths.state(source)
        if missing:
            raise ContractError('omp source inventory lacks compatibility marker; preserve evidence')
    pending = base / 'turn-sources-v2' / 'pending'
    if os.path.lexists(pending):
        trust_paths.state(pending)
        with os.scandir(pending) as entries:
            for entry in entries:
                if (not re.fullmatch(r'(?:0|[1-9][0-9]*)\.json', entry.name)
                        or int(entry.name[:-5]) >= turn_sources.MAX_TURNS):
                    raise ContractError('Unknown source index entry')
                try:
                    record = load(turn_sources.read_bytes(pending / entry.name, turn_sources.MAX_SOURCE))
                except (OSError, ValueError, TypeError):
                    raise ContractError('Unverifiable source index') from None
                if (not isinstance(record, dict) or record.get('schema') != 'agent-memory/turn-reference-v2'
                        or set(record) != {'schema', 'harness', 'session', 'lifecycle', 'project_id', 'turn'}
                        or any(not isinstance(v, str) for v in record.values())
                        or record['harness'] not in {'codex', 'claude', 'opencode', 'omp'}):
                    raise ContractError('Unverifiable source index')
                turn_sources.bounded(record['session'])
                turn_sources.bounded(record['lifecycle'])
                root, _ = bank_writes.bank_path(repo)
                expected = hashlib.sha256(turn_sources.encoded(
                    [str(root), record['harness'], record['session'], record['lifecycle']])).hexdigest()
                if record['turn'] != expected:
                    raise ContractError('Foreign source index namespace')
                if record['harness'] == 'omp' and missing:
                    raise ContractError('omp source index lacks compatibility marker')
    return set() if missing else {CAPABILITY}
