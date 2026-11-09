"""Finite private omp FIFO. Coverage metadata is never source completion authority."""
from __future__ import annotations

import argparse
from collections import deque
from contextlib import contextmanager
import hashlib
import importlib
import os
from pathlib import Path
import pkgutil
import re
import select
import sys
import threading
import time
import uuid

from . import bank_writes, trust_paths, turn_sources
from .developer_identity import load
from .frontmatter import ContractError

MAX_FRAME = 16 * 1024 * 1024
MAX_RETAINED = 64 * 1024 * 1024
MAX_FRAMES = 4096
MAX_HEADER = 256
READ_CHUNK = 64 * 1024
MAX_RESPONSE = 64 * 1024
MAX_RESPONSES = 4
MAX_IDENTITY = 1024 * 1024
MAX_IDS = 16384
MAX_METADATA = MAX_IDENTITY + 8192
BUSY_SECONDS = 30.0
BUSY_BACKOFF = 0.025
MARGIN_NS = 100_000_000
MAC_PROVISIONAL_RHO_NUMERATOR = 164
RATE_DENOMINATOR = 10000
WAKE_SUPPORT_QUALIFIED = False
SOURCE_TYPES = frozenset({'REQUEST', 'REFUSED-ATTEMPT', 'FINAL'})
CONTROL_TYPES = frozenset({'CLOCK', 'CLOCK-ACK', 'CANCEL'})
ACTIONS = frozenset({'hint', 'ingest', 'seal', 'wake'})
REFUSAL_CODES = frozenset({'capture-refused', 'identity-changed', 'unsafe-view', 'oversized-source',
    'queue-full', 'serialization-failed', 'startup-pending', 'invalid-request', 'binding-refused',
    'transition-final-view-unavailable', 'unsupported-view', 'missing-payload'})
_HEADER = re.compile(rb'AM249 ([A-Z-]+) (0|[1-9][0-9]{0,15}) (0|[1-9][0-9]{0,8}) (-|[0-9a-f-]{36})\n')
_HEX = re.compile(r'[0-9a-f]{64}')
_WINDOW_MINT = object()
_NONLEARNING_MINT = object()


def _exact(value, fields):
    if not isinstance(value, dict) or set(value) != set(fields):
        raise ContractError('Malformed finite stream metadata')
    return value


def _uuid(value):
    if not isinstance(value, str) or str(uuid.UUID(value)) != value:
        raise ContractError('Noncanonical finite stream identity')
    return value


def _installation_id(value):
    if not isinstance(value, str) or not re.fullmatch(r'[0-9a-f]{32}', value):
        raise ContractError('Invalid installed runtime identity')
    return value


def _integer(value):
    if type(value) is not int or not 0 <= value <= 2**53 - 1:
        raise ContractError('Invalid finite stream counter')
    return value


def _digest(value):
    return hashlib.sha256(turn_sources.encoded(value)).hexdigest()


def _sha(value):
    if not isinstance(value, str) or not _HEX.fullmatch(value):
        raise ContractError('Invalid finite stream metadata digest')
    return value


def _ids(value):
    if not isinstance(value, list) or len(value) > MAX_IDS or any(not isinstance(x, str) for x in value):
        raise ContractError('Unresolvable native identity inventory')
    if value != sorted(set(value)):
        raise ContractError('Noncanonical native identity inventory')
    for item in value:
        turn_sources.bounded(item)
    return value


def identity(value):
    """Closed metadata-only inventory; unknown identities are never truncated."""
    if value is None:
        return None
    _exact(value, {'view_id', 'leaf_id', 'native_ids', 'tool_ids'})
    _uuid(value['view_id'])
    if value['leaf_id'] is not None:
        turn_sources.bounded(value['leaf_id'])
    _ids(value['native_ids']); _ids(value['tool_ids'])
    if value['leaf_id'] is not None and value['leaf_id'] not in value['native_ids']:
        raise ContractError('Checked leaf absent from native inventory')
    if len(turn_sources.encoded(value)) > MAX_IDENTITY:
        raise ContractError('Unresolvable oversized native identity inventory')
    return value


def _hint_identity(value, *, session=None, repo=None):
    _exact(value, {'kind', 'session', 'header_id', 'cwd', 'leaf_id'})
    if value['kind'] != 'hint':
        raise ContractError('Unknown neutral hint identity')
    for name in ('session', 'header_id', 'cwd'):
        turn_sources.bounded(value[name], 4096 if name == 'cwd' else 512)
    if value['header_id'] != value['session'] or (session is not None and value['session'] != session):
        raise ContractError('Neutral hint session/header identity differs')
    cwd = Path(value['cwd'])
    if not cwd.is_absolute() or '..' in cwd.parts:
        raise ContractError('Invalid neutral hint scope')
    if repo is not None:
        try:
            scoped = cwd.resolve().is_relative_to(Path(repo).resolve())
        except (OSError, RuntimeError):
            scoped = False
        if not scoped:
            raise ContractError('Neutral hint belongs to another repository')
    if value['leaf_id'] is not None:
        turn_sources.bounded(value['leaf_id'])
    return value


def _is_hint(value):
    return isinstance(value, dict) and value.get('kind') == 'hint'


class _NonlearningTailEvidence:
    __slots__ = ('session', 'view_id', 'native_ids')

    def __init__(self, token, session, view_id, native_ids):
        if token is not _NONLEARNING_MINT:
            raise TypeError('Nonlearning recovery evidence is observer internal')
        self.session, self.view_id, self.native_ids = session, view_id, tuple(native_ids)

    def __reduce__(self):
        raise TypeError('Nonlearning recovery evidence cannot be serialized')


def _nonlearning_tail_evidence(session, view_id, native_ids):
    """Trusted ingest only, after complete committed old-file/parser qualification.

    The native projection must independently classify EVERY actual ID as
    NONLEARNING/payload-not-required and verify writer drain/original anchors.
    No observer JSON field or caller completeness flag can mint this evidence.
    """
    turn_sources.bounded(session); _uuid(view_id); _ids(native_ids)
    if not native_ids:
        raise ContractError('Empty source inventory cannot prove absent old tail')
    return _NonlearningTailEvidence(_NONLEARNING_MINT, session, view_id, native_ids)


def _retain(path, value, maximum=MAX_METADATA):
    turn_sources.private_mkdir(path.parent)
    try:
        turn_sources.exclusive(path, value, maximum)
    except FileExistsError:
        if turn_sources.read(path, maximum) != value:
            raise ContractError('Immutable finite stream metadata changed')
    return _digest(value)


def _base(repo):
    root, bank = bank_writes.bank_path(repo)
    operations, _ = bank_writes.state_paths(root, bank, str(uuid.uuid4()))
    return operations.parent


def _directory(base, session, instance):
    turn_sources.bounded(session); _uuid(instance)
    return Path(base) / 'omp-stream-v1' / hashlib.sha256(session.encode()).hexdigest() / instance


def _entries(path, pattern, *, directories=False):
    trust_paths.state(path)
    result = []
    with os.scandir(path) as scan:
        for entry in scan:
            if not re.fullmatch(pattern, entry.name) or (entry.is_dir(follow_symlinks=False) != directories):
                raise ContractError('Unknown finite stream inventory entry')
            if entry.is_symlink():
                raise ContractError('Redirected finite stream inventory')
            result.append(Path(entry.path))
    return sorted(result, key=lambda p: p.name)


def _session_instances(directory):
    trust_paths.state(directory)
    _sha(directory.name)
    result, marker = [], None
    with os.scandir(directory) as scan:
        for entry in scan:
            if entry.is_symlink():
                raise ContractError('Redirected finite session startup state')
            path = Path(entry.path)
            if entry.name == 'startup-incomplete.json':
                marker = turn_sources.read(path, 4096)
                if marker != {'schema': 'agent-memory/omp-session-startup-incomplete-v1',
                              'session_sha256': directory.name}:
                    raise ContractError('Unknown finite session startup qualification')
            else:
                _uuid(entry.name)
                if not entry.is_dir(follow_symlinks=False):
                    raise ContractError('Unknown finite session startup entry')
                result.append(path)
    return sorted(result), marker


@contextmanager
def _session_creation_lock(base, session_sha256):
    """Source barrier -> creation lock -> live/stream locks; never a runtime suffix."""
    directory = Path(base) / 'omp-stream-session-locks'
    turn_sources.private_mkdir(directory)
    path = directory / (_sha(session_sha256) + '.lock')
    try:
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    except FileExistsError:
        created = False
    else:
        os.close(fd)
        turn_sources.sync(directory)
        created = True
    with turn_sources.private_lock(path):
        turn_sources.read_bytes(path, 0)
        yield created


_IMPORT_GUARD = None
_PRELOADED_ARTIFACT = None


class _FrozenPackedImports:
    def find_spec(self, fullname, path=None, target=None):
        if fullname == __package__ or fullname.startswith(__package__ + '.'):
            raise ContractError('Late packed executable import is refused')
        return None


def preload(receipt=None):
    """Load executable Python closure before OPEN, including function-local imports.

    The caller verifies the installed artifact before this operation. Importing all
    packaged modules avoids lazy source loads after native stdin has disappeared.
    """
    global _IMPORT_GUARD, _PRELOADED_ARTIFACT
    artifact = receipt['source']['artifact_digest'] if receipt is not None else None
    if _IMPORT_GUARD is not None:
        if artifact != _PRELOADED_ARTIFACT:
            raise ContractError('Finite worker cannot adopt another executable closure')
        return
    package = importlib.import_module(__package__)
    if receipt is not None:
        names = []
        for relative in receipt['artifact_files']:
            prefix = 'scripts/agent_memory/'
            if relative.startswith(prefix) and relative.endswith('.py'):
                name = relative[len(prefix):-3]
                if name.endswith('/__init__'):
                    name = name[:-9]
                if name and name != '__init__':
                    names.append(name.replace('/', '.'))
    else:
        names = [item.name for item in pkgutil.iter_modules(package.__path__) if not item.ispkg]
    for name in sorted(set(names)):
        importlib.import_module(__package__ + '.' + name)
    _PRELOADED_ARTIFACT = artifact
    _IMPORT_GUARD = _FrozenPackedImports()
    sys.meta_path.insert(0, _IMPORT_GUARD)


def encode_frame(kind, sequence, value, nonce=None):
    raw = turn_sources.encoded(value)
    if len(raw) > MAX_FRAME:
        raise ContractError('Finite stream frame exceeds bound')
    _integer(sequence)
    if nonce is not None:
        _uuid(nonce)
    header = f'AM249 {kind} {sequence} {len(raw)} {nonce or "-"}\n'.encode('ascii')
    if len(header) > MAX_HEADER:
        raise ContractError('Finite stream header exceeds bound')
    return header + raw


class _Budget:
    def __init__(self):
        self.bytes = 0
        self.frames = 0
        self.lock = threading.Lock()

    def reserve(self, count):
        with self.lock:
            if self.frames >= MAX_FRAMES or self.bytes + count > MAX_RETAINED:
                raise ContractError('Finite stream accepted queue exceeds bound')
            self.frames += 1
            self.bytes += count

    def release(self, count):
        with self.lock:
            self.bytes -= count
            self.frames -= 1


class FrameReader:
    """One bounded partial body; its reservation includes queued and executing bytes."""
    def __init__(self, stream, budget):
        self.stream, self.budget = stream, budget

    def read(self):
        header = bytearray()
        while True:
            byte = self.stream.read(1)
            if not byte:
                if header:
                    raise ContractError('Partial finite stream header')
                return None
            header.extend(byte)
            if len(header) > MAX_HEADER:
                raise ContractError('Finite stream header exceeds bound')
            if byte == b'\n':
                break
        match = _HEADER.fullmatch(header)
        if match is None:
            raise ContractError('Invalid finite stream header')
        kind = match[1].decode('ascii')
        sequence, size = int(match[2]), int(match[3])
        nonce = None if match[4] == b'-' else _uuid(match[4].decode('ascii'))
        if kind not in SOURCE_TYPES | CONTROL_TYPES or size > MAX_FRAME:
            raise ContractError('Unknown or oversized finite stream frame')
        if kind in CONTROL_TYPES and (sequence != 0 or nonce is not None or size > MAX_RESPONSE):
            raise ContractError('Invalid finite stream control frame')
        reserved = kind not in CONTROL_TYPES
        # Controls have their own one-frame 64KiB bound; a full source FIFO
        # must not prevent CANCEL from revoking a not-yet-started reservation.
        if reserved:
            self.budget.reserve(size)
        try:
            raw = bytearray()
            while len(raw) < size:
                chunk = self.stream.read(min(READ_CHUNK, size - len(raw)))
                if not chunk:
                    raise ContractError('Partial finite stream body')
                raw.extend(chunk)
            return kind, sequence, nonce, raw
        except BaseException:
            if reserved:
                self.budget.release(size)
            raise


class ResponseWriter:
    """Independent bounded writer: a blocked recipient cannot hold source locks."""
    def __init__(self, stream, instance_id):
        self.stream, self.instance_id = stream, instance_id
        self.condition = threading.Condition()
        self.queue = deque()
        self.live = True
        self.finishing = False
        self.inflight = False
        self.loss_pending = False
        self.thread = threading.Thread(target=self._write, name='omp-stream-response', daemon=True)
        self.thread.start()

    def send(self, kind, sequence=0, **value):
        with self.condition:
            if not self.live or self.finishing:
                return False
            if len(self.queue) + int(self.inflight) >= MAX_RESPONSES:
                disposable = next((item for item in self.queue if item[0] == 'RECEIVED'), None)
                if disposable is not None:
                    self.queue.remove(disposable)
                elif kind == 'RECEIVED':
                    return False
                else:
                    self.loss_pending = True
                    self.condition.notify_all()
                    return False
            try:
                raw = encode_frame(kind, sequence, {'instance_id': self.instance_id, **value})
            except (ValueError, TypeError, UnicodeError):
                raw = None
            if raw is None or len(raw) > MAX_RESPONSE:
                if kind == 'RECEIVED':
                    return False
                kind = 'OBSERVED' if kind == 'OBSERVED' else 'REFUSED'
                raw = encode_frame(kind, sequence, {'instance_id': self.instance_id,
                    'code': 'result-oversized' if kind == 'OBSERVED' else 'response-unavailable',
                    'outcome': 'unknown', 'status': 'pending'})
            self.queue.append((kind, raw))
            self.condition.notify_all()
            return True

    def _loss_notice(self):
        if self.loss_pending and len(self.queue) + int(self.inflight) < MAX_RESPONSES:
            raw = encode_frame('REFUSED', 0, {'instance_id': self.instance_id,
                'code': 'response-pressure', 'outcome': 'unknown', 'status': 'pending'})
            self.queue.append(('REFUSED', raw))
            self.loss_pending = False

    def _write(self):
        try:
            fd = self.stream.fileno()
        except (AttributeError, OSError):
            fd = None
        try:
            if fd is not None:
                os.set_blocking(fd, False)
            while True:
                with self.condition:
                    while self.live and not self.queue and not self.loss_pending and not self.finishing:
                        self.condition.wait()
                    self._loss_notice()
                    if not self.live or (not self.queue and self.finishing):
                        return
                    _, raw = self.queue.popleft()
                    self.inflight = True
                offset = 0
                while offset < len(raw):
                    with self.condition:
                        if not self.live:
                            return
                    if fd is None:
                        written = self.stream.write(raw[offset:])
                        offset += len(raw) - offset if written is None else written
                    else:
                        try:
                            offset += os.write(fd, memoryview(raw)[offset:])
                        except BlockingIOError:
                            select.select([], [fd], [], BUSY_BACKOFF)
                if fd is None:
                    self.stream.flush()
                with self.condition:
                    raw = None
                    self.inflight = False
                    self._loss_notice()
                    self.condition.notify_all()
        except (OSError, ValueError):
            with self.condition:
                self.live = False
                self.queue.clear()
                self.condition.notify_all()

    def finish(self):
        # A finite output grace is not a source execution timeout or helper kill.
        with self.condition:
            self.finishing = True
            self.condition.notify_all()
        self.thread.join(0.1)
        with self.condition:
            self.live = False
            self.queue.clear()
            self.condition.notify_all()
        self.thread.join(0.1)


def _stamp(value):
    if not isinstance(value, str) or not re.fullmatch(r'(?:0|[1-9][0-9]{0,23})', value):
        raise ContractError('Invalid monotonic clock timestamp')
    return int(value)


class VerifiedNativeDeliveryWindow:
    """Purpose-limited, internally minted calibration; never JSON authority."""
    __slots__ = ('L', 'j3', 'j_deadline', 'nonce', 'instance', 'cancelled', '_eligible', '_platform')

    def __init__(self, token, *, L, j3, j_deadline, nonce, instance, eligible, platform):
        if token is not _WINDOW_MINT:
            raise TypeError('Delivery windows are wire-executor internal')
        self.L, self.j3, self.j_deadline = L, j3, j_deadline
        self.nonce, self.instance, self.cancelled = nonce, instance, False
        self._eligible, self._platform = eligible, platform

    def __reduce__(self):
        raise TypeError('Delivery windows cannot be serialized')

    @property
    def support(self):
        return 'provisional-unqualified' if self._platform == 'darwin' else 'unqualified'

    def allow_reservation(self, recipient_eligible=None):
        if self.cancelled or self._platform != 'darwin' or not self._eligible():
            return False
        if recipient_eligible is not None and not recipient_eligible():
            return False
        remaining = self.j_deadline - self.j3
        if remaining < 0:
            return False
        # Ceiling subtraction keeps integer arithmetic conservative: exact R3 D.
        drift = (MAC_PROVISIONAL_RHO_NUMERATOR * remaining + RATE_DENOMINATOR - 1) // RATE_DENOMINATOR
        deadline = self.j_deadline + self.L - drift
        return time.monotonic_ns() + MARGIN_NS < deadline


class ClockWindows:
    def __init__(self, instance, eligible, platform=None):
        self.instance, self.eligible = instance, eligible
        self.platform = sys.platform if platform is None else platform
        self.lock = threading.Lock()
        self.pending = None
        self.verified = None
        self.ended = False

    def control(self, kind, value, writer):
        with self.lock:
            if kind == 'CLOCK':
                p1 = time.monotonic_ns()
                _exact(value, {'nonce', 'j0', 'j_deadline'})
                nonce = _uuid(value['nonce'])
                j0, deadline = _stamp(value['j0']), _stamp(value['j_deadline'])
                if deadline < j0 or deadline - j0 > 950_000_000 or self.ended:
                    raise ContractError('Invalid callback clock interval')
                if ((self.pending is not None and self.pending[0] == nonce)
                        or (self.verified is not None and self.verified.nonce == nonce)):
                    raise ContractError('Callback clock nonce was already used')
                if self.verified is not None:
                    self.verified.cancelled = True
                self.verified = None
                p2 = time.monotonic_ns()
                if p2 < p1:
                    raise ContractError('Local callback clock regressed')
                self.pending = (nonce, j0, deadline, p1, p2)
                writer.send('CLOCK-REPLY', nonce=nonce, p1=str(p1), p2=str(p2))
            elif kind == 'CLOCK-ACK':
                _exact(value, {'nonce', 'j0', 'j3'})
                nonce, j0, j3 = _uuid(value['nonce']), _stamp(value['j0']), _stamp(value['j3'])
                pending = self.pending
                self.pending = None
                if pending is None or pending[0] != nonce or pending[1] != j0 or j3 < j0 or self.ended:
                    raise ContractError('Unmatched callback clock acknowledgement')
                _, _, deadline, p1, p2 = pending
                L, U = p2 - j3, p1 - j0
                if L > U:
                    raise ContractError('Contradictory callback clock calibration')
                if j3 <= deadline:
                    self.verified = VerifiedNativeDeliveryWindow(_WINDOW_MINT, L=L, j3=j3,
                        j_deadline=deadline, nonce=nonce, instance=self.instance,
                        eligible=lambda: not self.ended and self.eligible(), platform=self.platform)
            else:
                _exact(value, {'nonce'})
                nonce = _uuid(value['nonce'])
                if self.pending is not None and self.pending[0] == nonce:
                    self.pending = None
                if self.verified is not None and self.verified.nonce == nonce:
                    self.verified.cancelled = True

    def window(self, nonce):
        with self.lock:
            value = self.verified
            return value if value is not None and nonce == value.nonce and not self.ended else None

    def revoke(self):
        with self.lock:
            self.pending = None
            if self.verified is not None:
                self.verified.cancelled = True

    def end(self):
        with self.lock:
            self.ended = True
            self.pending = None
            if self.verified is not None:
                self.verified.cancelled = True


def _binding(value, repo, session, instance, lifecycle):
    fields = {'project_id', 'repo_root', 'bank_path', 'checkout_key', 'artifact_digest',
              'installation_id', 'initial_lifecycle'}
    _exact(value, fields)
    for key in fields:
        turn_sources.bounded(value[key], 4096)
    _uuid(value['project_id']); _installation_id(value['installation_id']); _uuid(value['initial_lifecycle'])
    _sha(value['artifact_digest'])
    if value['initial_lifecycle'] != lifecycle or not Path(repo).resolve().is_relative_to(Path(value['repo_root'])):
        raise ContractError('Foreign finite stream startup binding')
    for key in ('repo_root', 'bank_path'):
        path = Path(value[key])
        if not path.is_absolute() or '..' in path.parts or path.resolve() != path:
            raise ContractError('Redirected finite stream startup scope')
    if not Path(value['bank_path']).is_relative_to(Path(value['repo_root'])):
        raise ContractError('Foreign finite stream bank')
    return value


def _attest_binding(binding, receipt):
    """One full executable attestation per OPEN or source frame, outside transport."""
    from . import omp_installation
    if (not isinstance(receipt, dict) or not isinstance(receipt.get('omp_binding'), dict)
            or receipt.get('installation_id') != binding['installation_id']
            or receipt.get('source', {}).get('artifact_digest') != binding['artifact_digest']):
        raise ContractError('Finite stream requires its original installed observer receipt')
    return omp_installation.verified_observer_binding(binding['artifact_digest'],
        receipt['omp_binding']['agent_dir'], receipt=receipt)


@contextmanager
def _verify_binding(repo, binding, receipt, proof):
    """Fast same-frame executable, marker and scope check under the source barrier."""
    from . import omp_installation
    with omp_installation._observe_observer_binding(repo, receipt, proof) as context:
        if (context.project_id != binding['project_id'] or str(context.root) != binding['repo_root']
                or str(context.bank) != binding['bank_path'] or context.scope['checkout_key'] != binding['checkout_key']):
            raise ContractError('Finite stream project/bank/checkout binding changed')
        yield context


class _InvalidAttempt(ContractError):
    """Pure metadata validation only, never a filesystem or binding failure."""


class Coverage:
    def __init__(self, repo, session, instance, lifecycle, binding, *, receipt=None):
        self.repo, self.session, self.instance = repo, session, instance
        self.receipt, self.binding = receipt, binding
        self.base = _base(repo)
        self.directory = _directory(self.base, session, instance)
        self.lock_path = self.directory / 'stream.lock'
        self._live_guard = None
        self.previous_identity = None
        self.last_checked_identity = None
        self.last_sequence = 0
        self.refused_count = 0
        self.last_ordinal = 0
        self.open = {'schema': 'agent-memory/omp-stream-open-v1', **binding,
                     'session': session, 'instance_id': instance, 'opened_at': str(time.time_ns())}
        self.attest_frame()
        try:
            with turn_sources.source_barrier(repo), _verify_binding(repo, binding, receipt, self.frame_proof):
                with _session_creation_lock(self.base, self.directory.parent.name) as fresh_lock:
                    session_dir = self.directory.parent
                    existed = os.path.lexists(session_dir)
                    instances, _ = _session_instances(session_dir) if existed else ([], None)
                    turn_sources.private_mkdir(session_dir)
                    if (existed and not instances) or (not existed and not fresh_lock):
                        _retain(session_dir / 'startup-incomplete.json',
                            {'schema': 'agent-memory/omp-session-startup-incomplete-v1',
                             'session_sha256': session_dir.name}, 4096)
                    turn_sources.private_mkdir(self.directory)
                    guard = turn_sources.private_lock(self.directory / 'live.lock', shared=True)
                    guard.__enter__()
                    self._live_guard = guard
                    with turn_sources.private_lock(self.lock_path):
                        turn_sources.exclusive(self.directory / 'open.json', self.open, MAX_METADATA)
        except BaseException:
            self.close_live()
            raise
        self.open_digest = _digest(self.open)

    def attest_frame(self):
        self.frame_proof = _attest_binding(self.binding, self.receipt)

    def close_live(self):
        guard, self._live_guard = self._live_guard, None
        if guard is not None:
            guard.__exit__(None, None, None)

    @contextmanager
    def control(self):
        with turn_sources.source_barrier(self.repo), _verify_binding(self.repo, self.binding, self.receipt, self.frame_proof), turn_sources.private_lock(self.lock_path):
            if turn_sources.read(self.directory / 'open.json', MAX_METADATA) != self.open:
                raise ContractError('Finite stream OPEN binding changed')
            yield

    def attempt(self, sequence, action, ordinal, disposition, reason, known, source_kind=None, frame_kind='REQUEST'):
        try:
            is_hint = _is_hint(known)
            if is_hint:
                _hint_identity(known, session=self.session, repo=self.repo)
                if action != 'hint' or disposition != 'accepted' or source_kind is not None:
                    raise ContractError('Hint metadata is not a full stored view')
            else:
                identity(known)
                if action == 'hint' and disposition == 'accepted':
                    raise ContractError('Neutral hint requires its closed metadata identity')
            _integer(ordinal)
            if action not in ACTIONS or disposition not in {'accepted', 'refused'}:
                raise ContractError('Invalid finite source attempt')
            if source_kind not in {None, 'session-manager', 'retired-file'}:
                raise ContractError('Unknown checked source-view kind')
            if disposition == 'refused' and source_kind is not None:
                raise ContractError('Refused projection is not a checked stored view')
            if sequence != self.last_sequence + 1 or ordinal < self.last_ordinal:
                raise ContractError('Noncontiguous finite source attempts')
            if frame_kind not in {'REQUEST', 'REFUSED-ATTEMPT'} or (frame_kind == 'REFUSED-ATTEMPT' and disposition != 'refused'):
                raise ContractError('Invalid source attempt wire type')
            if (disposition == 'accepted' and reason is not None) or (disposition == 'refused' and reason not in REFUSAL_CODES):
                raise ContractError('Invalid finite source refusal code')
            previous = self.previous_identity
            if is_hint:
                inventory = dict(known)
            elif known is None:
                inventory = {'kind': 'unresolvable'}
            elif previous is None:
                inventory = {'kind': 'baseline', **known}
            else:
                inventory = {'kind': 'delta', 'view_id': known['view_id'], 'leaf_id': known['leaf_id'],
                    'native_added': sorted(set(known['native_ids']) - set(previous['native_ids'])),
                    'native_removed': sorted(set(previous['native_ids']) - set(known['native_ids'])),
                    'tool_added': sorted(set(known['tool_ids']) - set(previous['tool_ids'])),
                    'tool_removed': sorted(set(previous['tool_ids']) - set(known['tool_ids']))}
            value = {'schema': 'agent-memory/omp-stream-attempt-v1', 'open_sha256': self.open_digest,
                'instance_id': self.instance, 'sequence': sequence, 'action': action,
                'event_ordinal': ordinal, 'disposition': disposition, 'reason': reason, 'identity': inventory,
                'source_kind': source_kind, 'frame_kind': frame_kind}
            if len(turn_sources.encoded(value)) > MAX_IDENTITY:
                raise ContractError('Finite attempt identity record exceeds bound')
        except (ValueError, TypeError, KeyError):
            raise _InvalidAttempt('Invalid finite source attempt metadata') from None
        with self.control():
            if os.path.lexists(self.directory / 'retirement.json'):
                raise ContractError('Finite stream is retired')
            digest = _retain(self.directory / 'attempts' / f'{sequence}.json', value, MAX_IDENTITY)
        if not is_hint:
            self.previous_identity = known
        if source_kind == 'session-manager':
            self.last_checked_identity = known
        self.last_sequence, self.last_ordinal = sequence, ordinal
        self.refused_count += int(frame_kind == 'REFUSED-ATTEMPT')
        return digest

    def refuse_invalid(self, sequence, frame_kind, action, ordinal):
        # Never replace an accepted/partial immutable slot after a real I/O fault.
        with self.control():
            if os.path.lexists(self.directory / 'attempts' / f'{sequence}.json'):
                raise ContractError('Original finite attempt exists; preserve its unknown outcome')
        return self.attempt(sequence, action, ordinal, 'refused', 'invalid-request', None, None, frame_kind)

    def started(self, sequence, attempt_digest):
        # Called inside source barrier -> omp-transport lock, never acquires transport.
        with _verify_binding(self.repo, self.binding, self.receipt, self.frame_proof), turn_sources.private_lock(self.lock_path):
            if os.path.lexists(self.directory / 'retirement.json'):
                raise ContractError('Finite stream retirement fence prevents execution')
            _retain(self.directory / 'started' / f'{sequence}.json', {
                'schema': 'agent-memory/omp-stream-started-v1', 'open_sha256': self.open_digest,
                'attempt_sha256': attempt_digest, 'sequence': sequence}, 4096)

    def outcome(self, sequence, attempt_digest, outcome, code, references=None):
        if outcome not in {'verified', 'durable-refusal', 'unknown'}:
            raise ContractError('Invalid finite stream outcome')
        value = {'schema': 'agent-memory/omp-stream-outcome-v1', 'open_sha256': self.open_digest,
            'attempt_sha256': attempt_digest, 'sequence': sequence, 'outcome': outcome,
            'code': code, 'source_references': _source_refs(references or [])}
        with self.control():
            _retain(self.directory / 'outcomes' / f'{sequence}.json', value)

    def transition(self, value):
        _exact(value, {'transition_id', 'target_session', 'target_instance', 'last_view', 'reason'})
        _uuid(value['transition_id']); _uuid(value['target_instance']); turn_sources.bounded(value['target_session'])
        if ((value['target_session'] == self.session and value['target_instance'] == self.instance)
                or value['reason'] != 'transition-final-view-unavailable'):
            raise ContractError('Invalid committed source transition')
        identity(value['last_view'])
        if value['last_view'] != self.last_checked_identity:
            raise ContractError('Transition does not bind last checked old view')
        record = {'schema': 'agent-memory/omp-transition-tail-v1', 'open_sha256': self.open_digest,
            'project_id': self.open['project_id'], 'checkout_key': self.open['checkout_key'],
            'session': self.session, 'instance_id': self.instance, **value}
        with self.control():
            _retain(self.directory / 'transitions' / (value['transition_id'] + '.json'), record)

    def close(self, final):
        _exact(final, {'last_attempt_seq', 'refused_count', 'last_event_ordinal', 'transition'})
        for field in ('last_attempt_seq', 'refused_count', 'last_event_ordinal'):
            _integer(final[field])
        if (final['last_attempt_seq'], final['refused_count'], final['last_event_ordinal']) != (
                self.last_sequence, self.refused_count, self.last_ordinal):
            raise ContractError('FINAL does not account all source attempts')
        if final['transition'] is not None:
            self.transition(final['transition'])
        with self.control():
            row, pins = _inspect(self.directory, self.open, include_close=False)
            pins = [item for item in pins if item[0] != 'retirement.json'
                    and not item[0].startswith(('resolutions/', 'transition-resolutions/'))]
            if any(item['code'] in {'missing-outcome', 'unknown-after-start', 'busy-exhausted', 'unknown-outcome'} for item in row['pending']):
                return False
            _retain(self.directory / 'closed.json', {'schema': 'agent-memory/omp-stream-close-v1',
                'open_sha256': self.open_digest, 'last_attempt_seq': self.last_sequence,
                'refused_count': self.refused_count, 'last_event_ordinal': self.last_ordinal,
                'inventory_sha256': _digest(pins)}, 4096)
        return True


def _source_refs(values):
    # Actual source references may include existing observation/source digest bindings.
    # No request digest, exception message, or arbitrary result fields are retained.
    if not isinstance(values, list) or len(values) > MAX_IDS:
        raise ContractError('Invalid original source reference inventory')
    result = []
    for item in values:
        _exact(item, {'session', 'lifecycle', 'observation_id', 'source_sha256'})
        turn_sources.bounded(item['session']); _uuid(item['lifecycle']); _uuid(item['observation_id']); _sha(item['source_sha256'])
        result.append(dict(item))
    if len(turn_sources.encoded(result)) > MAX_IDENTITY:
        raise ContractError('Original source references exceed bound')
    return result


def _returned_refs(result):
    refs = result.get('source_references', [])
    try:
        return _source_refs(refs)
    except (ValueError, TypeError, KeyError):
        return []

def _request_identity(request, known):
    known = identity(known)
    if known is None:
        return None
    payload = request['payload']
    if not isinstance(payload, dict) or not isinstance(payload.get('view'), dict):
        raise ContractError('Checked identity lacks stored source view')
    view = payload['view']
    parents = view.get('native_parents')
    if not isinstance(parents, list) or any(not isinstance(item, dict) or not isinstance(item.get('id'), str) for item in parents):
        raise ContractError('Checked identity lacks native graph')
    native = [item['id'] for item in parents]
    if len(set(native)) != len(native) or sorted(native) != known['native_ids'] or view.get('leaf_id') != known['leaf_id']:
        raise ContractError('Envelope native identities differ from stored view')
    tools = set()
    entries = payload.get('entries')
    if not isinstance(entries, list):
        raise ContractError('Checked identity lacks source entries')
    for entry in entries:
        message = entry.get('message') if isinstance(entry, dict) else None
        if not isinstance(message, dict):
            continue
        if message.get('role') == 'toolResult' and isinstance(message.get('toolCallId'), str):
            tools.add(message['toolCallId'])
        content = message.get('content')
        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and block.get('type') == 'toolCall' and isinstance(block.get('id'), str):
                    tools.add(block['id'])
    if sorted(tools) != known['tool_ids']:
        raise ContractError('Envelope tool identities differ from stored source')
    return known



class FiniteExecutor:
    def __init__(self, coverage, writer, clocks, receipt=None, observer=None):
        self.coverage, self.writer, self.clocks, self.receipt = coverage, writer, clocks, receipt
        if observer is None:
            from .omp_transport import observe
            observer = observe
        self.observer = observer

    @staticmethod
    def metadata(function, *args):
        deadline = None
        while True:
            try:
                return function(*args)
            except turn_sources.SourceTransportBusy:
                if deadline is None:
                    deadline = time.monotonic() + BUSY_SECONDS
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise
                # Every private/source lock has unwound before the wait.
                time.sleep(min(BUSY_BACKOFF, remaining))

    def refuse_invalid(self, kind, sequence, value):
        action, ordinal = 'hint', self.coverage.last_ordinal
        if isinstance(value, dict):
            candidate = value.get('request') if kind == 'REQUEST' else value
            if isinstance(candidate, dict) and isinstance(candidate.get('action'), str) and candidate['action'] in ACTIONS:
                action = candidate['action']
            candidate_ordinal = value.get('event_ordinal')
            if type(candidate_ordinal) is int and 0 <= candidate_ordinal <= 2**53 - 1:
                ordinal = max(ordinal, candidate_ordinal)
        digest = self.metadata(self.coverage.refuse_invalid, sequence, kind, action, ordinal)
        self.metadata(self.coverage.outcome, sequence, digest, 'durable-refusal', 'invalid-request')
        self.writer.send('REFUSED', sequence, code='invalid-request', outcome='durable-refusal')

    def execute(self, kind, sequence, nonce, raw):
        if kind not in {'REQUEST', 'REFUSED-ATTEMPT'}:
            raise ContractError('Invalid source frame type')
        value = None
        try:
            value = load(raw.decode('utf-8', 'strict'))
            if kind == 'REFUSED-ATTEMPT':
                if nonce is not None:
                    raise ContractError('Refused source attempt cannot carry wake authority')
                _exact(value, {'action', 'event_ordinal', 'reason', 'identity'})
                action, disposition, reason = value['action'], 'refused', value['reason']
                known, source_kind = identity(value['identity']), None
            else:
                _exact(value, {'request', 'event_ordinal', 'identity'})
                request = value['request']
                _exact(request, {'schema', 'action', 'session', 'lifecycle', 'payload'})
                if request['schema'] != 'agent-memory/omp-observer-request-v2' or request['session'] != self.coverage.session:
                    raise ContractError('Foreign finite observer request')
                _uuid(request['lifecycle'])
                if request['action'] not in ACTIONS or (nonce is not None and request['action'] != 'seal'):
                    raise ContractError('Invalid finite observer action/window')
                if request['action'] == 'hint':
                    known = _hint_identity(value['identity'], session=self.coverage.session, repo=self.coverage.repo)
                    _exact(request['payload'], {'session', 'header_id', 'cwd', 'leaf_id'})
                    if request['payload'] != {key: item for key, item in known.items() if key != 'kind'}:
                        raise ContractError('Neutral hint payload differs from its identity')
                    source_kind = None
                else:
                    known = _request_identity(request, value['identity'])
                    view = request['payload'].get('view') if isinstance(request['payload'], dict) else None
                    source_kind = view.get('source_kind') if isinstance(view, dict) else None
                action, disposition, reason = request['action'], 'accepted', None
            ordinal = max(_integer(value['event_ordinal']), self.coverage.last_ordinal)
        except (ValueError, TypeError, KeyError, RecursionError):
            self.coverage.attest_frame()
            self.refuse_invalid(kind, sequence, value)
            return
        if action != 'hint' or (disposition == 'refused' and known is not None):
            self.coverage.attest_frame()
        try:
            digest = self.metadata(self.coverage.attempt, sequence, action, ordinal,
                disposition, reason, known, source_kind, kind)
        except _InvalidAttempt:
            self.refuse_invalid(kind, sequence, value)
            return
        if disposition == 'refused':
            self.metadata(self.coverage.outcome, sequence, digest, 'durable-refusal', reason)
            self.writer.send('REFUSED', sequence, code=reason, outcome='durable-refusal')
            return
        if action == 'hint':
            self.metadata(self.coverage.outcome, sequence, digest, 'verified', 'hint-recorded')
            self.writer.send('OBSERVED', sequence, result={'verified': True, 'status': 'hint-recorded',
                'canonical_writes_performed': False})
            return
        self.writer.send('RECEIVED', sequence)
        started = False

        def on_started():
            nonlocal started
            if started:
                raise ContractError('Finite source attempt started twice')
            self.coverage.started(sequence, digest)
            started = True
        on_started.instance_id = self.coverage.instance
        on_started.identity = identity(value['identity'])
        deadline = time.monotonic() + BUSY_SECONDS
        result = None
        while True:
            try:
                result = self.observer(self.coverage.repo, request, receipt=self.receipt,
                    on_started=on_started, delivery_window=self.clocks.window(nonce) if nonce else None,
                    verified_binding=self.coverage.frame_proof)
            except turn_sources.SourceTransportBusy:
                if started:
                    outcome, code = 'unknown', 'unknown-after-start'
                elif time.monotonic() >= deadline:
                    outcome, code = 'unknown', 'busy-exhausted'
                else:
                    time.sleep(min(BUSY_BACKOFF, max(0, deadline - time.monotonic())))
                    continue
            except ContractError:
                outcome, code = ('unknown', 'unknown-after-start') if started else ('durable-refusal', 'observer-refused')
            except Exception:
                outcome, code = 'unknown', 'unknown-after-start' if started else 'unknown-outcome'
            else:
                if isinstance(result, dict) and result.get('verified') is True and started:
                    outcome, code = 'verified', 'verified'
                elif isinstance(result, dict) and result.get('verified') is True:
                    outcome, code = 'unknown', 'unknown-outcome'
                else:
                    outcome, code = ('unknown', 'unknown-after-start') if started else ('durable-refusal', 'observer-refused')
            break
        # Persistence/output failures cannot re-enter or replay source execution.
        refs = _returned_refs(result) if outcome == 'verified' else []
        self.metadata(self.coverage.outcome, sequence, digest, outcome, code, refs)
        if outcome == 'verified':
            self.writer.send('OBSERVED', sequence, result=result)
        else:
            self.writer.send('OBSERVED', sequence, outcome=outcome, code=code)


def _open_value(value, directory):
    fields = {'schema', 'project_id', 'repo_root', 'bank_path', 'checkout_key', 'session', 'instance_id',
              'initial_lifecycle', 'artifact_digest', 'installation_id', 'opened_at'}
    _exact(value, fields)
    if value['schema'] != 'agent-memory/omp-stream-open-v1':
        raise ContractError('Unknown finite stream OPEN schema')
    for key in fields - {'schema'}:
        turn_sources.bounded(value[key], 4096)
    _uuid(value['project_id']); _uuid(value['instance_id']); _installation_id(value['installation_id']); _uuid(value['initial_lifecycle'])
    _sha(value['artifact_digest']); _stamp(value['opened_at'])
    if directory.name != value['instance_id'] or directory.parent.name != hashlib.sha256(value['session'].encode()).hexdigest():
        raise ContractError('Finite stream OPEN namespace differs')
    return value


def _apply_identity(value, previous):
    if value == {'kind': 'unresolvable'}:
        return None
    if not isinstance(value, dict):
        raise ContractError('Malformed finite identity record')
    if _is_hint(value):
        _hint_identity(value)
        return previous
    if value.get('kind') == 'baseline':
        _exact(value, {'kind', 'view_id', 'leaf_id', 'native_ids', 'tool_ids'})
        return identity({key: item for key, item in value.items() if key != 'kind'})
    _exact(value, {'kind', 'view_id', 'leaf_id', 'native_added', 'native_removed', 'tool_added', 'tool_removed'})
    if value['kind'] != 'delta' or previous is None:
        raise ContractError('Finite identity delta has no known baseline')
    result = {'view_id': value['view_id'], 'leaf_id': value['leaf_id']}
    for prefix in ('native', 'tool'):
        added, removed = _ids(value[prefix + '_added']), _ids(value[prefix + '_removed'])
        old = set(previous[prefix + '_ids'])
        if old.intersection(added) or not set(removed).issubset(old) or set(added).intersection(removed):
            raise ContractError('Invalid finite identity delta')
        result[prefix + '_ids'] = sorted((old - set(removed)) | set(added))
    return identity(result)


def _inspect(directory, opened, *, include_close=True):
    opened = _open_value(opened, directory)
    open_digest = _digest(opened)
    allowed = {'open.json', 'stream.lock', 'live.lock', 'attempts', 'outcomes', 'started', 'closed.json', 'retirement.json',
               'resolutions', 'transitions', 'transition-resolutions'}
    trust_paths.state(directory)
    with os.scandir(directory) as scan:
        for entry in scan:
            if entry.name not in allowed or entry.is_symlink():
                raise ContractError('Unknown finite stream state')
    turn_sources.read_bytes(directory / 'live.lock', 0)
    pins, pending = [['open.json', open_digest]], []
    attempts, outcomes, starts, resolutions, known = {}, {}, {}, {}, None
    transitions, transition_resolutions = set(), set()
    last_checked = None
    hints, verified_native_ids = [], set()
    refusal_count, ordinal = 0, 0
    for namespace in ('attempts', 'outcomes', 'started', 'resolutions', 'transitions', 'transition-resolutions'):
        path = directory / namespace
        if not os.path.lexists(path):
            continue
        pattern = r'[0-9a-f-]{36}\.json' if 'transition' in namespace else r'[1-9][0-9]*\.json'
        for child in _entries(path, pattern):
            value = turn_sources.read(child, MAX_METADATA)
            pins.append([str(child.relative_to(directory)), _digest(value)])
            if namespace == 'attempts':
                attempts[int(child.stem)] = value
            elif namespace == 'outcomes':
                outcomes[int(child.stem)] = value
            elif namespace == 'started':
                starts[int(child.stem)] = value
            elif namespace == 'resolutions':
                resolutions[int(child.stem)] = value
            elif namespace == 'transitions':
                transitions.add(child.stem)
            elif namespace == 'transition-resolutions':
                transition_resolutions.add(child.stem)
    if transition_resolutions - transitions:
        raise ContractError('Orphan finite transition recovery')
    if (set(outcomes) | set(starts) | set(resolutions)) - set(attempts):
        raise ContractError('Orphan finite stream attempt metadata')
    for expected_sequence, sequence in enumerate(sorted(attempts), 1):
        if sequence != expected_sequence:
            raise ContractError('Missing finite source attempt')
        attempt = attempts[sequence]
        _exact(attempt, {'schema', 'open_sha256', 'instance_id', 'sequence', 'action', 'event_ordinal', 'disposition', 'reason', 'identity', 'source_kind', 'frame_kind'})
        if (attempt['schema'] != 'agent-memory/omp-stream-attempt-v1' or attempt['open_sha256'] != open_digest
                or attempt['instance_id'] != opened['instance_id'] or attempt['sequence'] != sequence
                or attempt['action'] not in ACTIONS or attempt['disposition'] not in {'accepted', 'refused'}):
            raise ContractError('Foreign finite source attempt')
        if (attempt['source_kind'] not in {None, 'session-manager', 'retired-file'}
                or (attempt['disposition'] == 'refused' and attempt['source_kind'] is not None)):
            raise ContractError('Invalid checked source-view kind')
        if (attempt['frame_kind'] not in {'REQUEST', 'REFUSED-ATTEMPT'}
                or (attempt['frame_kind'] == 'REFUSED-ATTEMPT' and attempt['disposition'] != 'refused')):
            raise ContractError('Invalid source attempt wire type')
        if sequence in starts:
            start = starts[sequence]
            _exact(start, {'schema', 'open_sha256', 'attempt_sha256', 'sequence'})
            if start != {'schema': 'agent-memory/omp-stream-started-v1', 'open_sha256': open_digest,
                         'attempt_sha256': _digest(attempt), 'sequence': sequence}:
                raise ContractError('Foreign finite stream start record')
        _integer(attempt['event_ordinal'])
        if attempt['event_ordinal'] < ordinal:
            raise ContractError('Regressed finite event ordinal')
        ordinal = attempt['event_ordinal']
        if attempt['disposition'] == 'refused':
            if attempt['reason'] not in REFUSAL_CODES:
                raise ContractError('Unknown finite refusal code')
        elif attempt['reason'] is not None:
            raise ContractError('Accepted finite source has refusal reason')
        refusal_count += int(attempt['frame_kind'] == 'REFUSED-ATTEMPT')
        is_hint = _is_hint(attempt['identity'])
        if is_hint:
            _hint_identity(attempt['identity'], session=opened['session'], repo=opened['repo_root'])
            if (attempt['action'] != 'hint' or attempt['disposition'] != 'accepted'
                    or attempt['source_kind'] is not None or sequence in starts):
                raise ContractError('Neutral hint acquired full source execution authority')
        known = _apply_identity(attempt['identity'], known)
        if attempt['source_kind'] == 'session-manager':
            last_checked = known
        outcome = outcomes.get(sequence)
        code = None
        if outcome is None:
            code = 'missing-outcome'
        else:
            _exact(outcome, {'schema', 'open_sha256', 'attempt_sha256', 'sequence', 'outcome', 'code', 'source_references'})
            if (outcome['schema'] != 'agent-memory/omp-stream-outcome-v1' or outcome['open_sha256'] != open_digest
                    or outcome['attempt_sha256'] != _digest(attempt) or outcome['sequence'] != sequence
                    or outcome['outcome'] not in {'verified', 'durable-refusal', 'unknown'}):
                raise ContractError('Foreign finite stream outcome')
            if outcome['outcome'] == 'verified':
                if attempt['disposition'] != 'accepted' or (not is_hint and sequence not in starts):
                    raise ContractError('Verified finite outcome lacks executed accepted attempt')
                if is_hint and (outcome['code'] != 'hint-recorded' or outcome['source_references']):
                    raise ContractError('Neutral hint outcome claims source authority')
                if not is_hint and outcome['code'] != 'verified':
                    raise ContractError('Full source outcome has a non-execution code')
            if outcome['code'] == 'hint-recorded' and (not is_hint or outcome['outcome'] != 'verified'):
                raise ContractError('Hint outcome code differs from neutral metadata execution')
            _source_refs(outcome['source_references'])
            if outcome['code'] not in REFUSAL_CODES | {'observer-refused', 'verified', 'hint-recorded', 'unknown-after-start', 'unknown-outcome', 'busy-exhausted'}:
                raise ContractError('Unknown finite outcome code')
            if outcome['outcome'] != 'verified':
                code = outcome['code']
        if is_hint:
            hints.append((sequence, attempt['identity']['leaf_id']))
        elif outcome is not None and outcome['outcome'] == 'verified' and known is not None and attempt['source_kind'] in {'session-manager', 'retired-file'}:
            verified_native_ids.update(known['native_ids'])
        resolution = directory / 'resolutions' / f'{sequence}.json'
        if os.path.lexists(resolution):
            if is_hint:
                raise ContractError('Neutral hint cannot be resolved as a full source attempt')
            recovered = turn_sources.read(resolution, MAX_METADATA)
            _exact(recovered, {'schema', 'open_sha256', 'attempt_sha256', 'retirement_sha256', 'sequence', 'view_id', 'native_ids', 'tool_ids', 'source_references'})
            if (recovered['schema'] != 'agent-memory/omp-stream-recovery-v1' or recovered['open_sha256'] != open_digest
                    or recovered['attempt_sha256'] != _digest(attempt) or recovered['sequence'] != sequence
                    or known is None or not (known['native_ids'] or known['tool_ids'])
                    or not set(known['native_ids']).issubset(_ids(recovered['native_ids']))
                    or not set(known['tool_ids']).issubset(_ids(recovered['tool_ids']))):
                raise ContractError('Unbound exact native stream recovery')
            _uuid(recovered['view_id'])
            if not _source_refs(recovered['source_references']):
                raise ContractError('Attempt recovery lacks actual source references')
            fence = turn_sources.read(directory / 'retirement.json', MAX_METADATA)
            if recovered['retirement_sha256'] != _digest(fence):
                raise ContractError('Stream recovery lacks retirement fence')
            code = None
        if code:
            pending.append({'code': code, 'sequence': sequence})
        if known is None and not is_hint:
            pending.append({'code': 'unresolvable-identity', 'sequence': sequence})
    retirement_path = directory / 'retirement.json'
    retired = os.path.lexists(retirement_path)
    if retired:
        fence = turn_sources.read(retirement_path, MAX_METADATA)
        _exact(fence, {'schema', 'open_sha256', 'instance_id', 'session', 'fence_id'})
        if fence != {'schema': 'agent-memory/omp-stream-retirement-v1', 'open_sha256': open_digest,
                     'instance_id': opened['instance_id'], 'session': opened['session'], 'fence_id': fence['fence_id']}:
            raise ContractError('Foreign finite stream retirement')
        _uuid(fence['fence_id']); pins.append(['retirement.json', _digest(fence)])
    transition_dir = directory / 'transitions'
    if os.path.lexists(transition_dir):
        for path in _entries(transition_dir, r'[0-9a-f-]{36}\.json'):
            value = turn_sources.read(path, MAX_METADATA)
            _exact(value, {'schema', 'open_sha256', 'project_id', 'checkout_key', 'session', 'instance_id', 'transition_id', 'target_session', 'target_instance', 'last_view', 'reason'})
            if (value['schema'] != 'agent-memory/omp-transition-tail-v1' or value['open_sha256'] != open_digest
                    or any(value[key] != opened[key] for key in ('project_id', 'checkout_key', 'session', 'instance_id'))
                    or value['transition_id'] != path.stem or value['reason'] != 'transition-final-view-unavailable'):
                raise ContractError('Foreign finite transition tail')
            identity(value['last_view']); _uuid(value['transition_id']); _uuid(value['target_instance']); turn_sources.bounded(value['target_session'])
            if value['last_view'] != last_checked or (value['target_session'] == opened['session']
                    and value['target_instance'] == opened['instance_id']):
                raise ContractError('Transition does not bind last precommit checked old view')
            resolution = directory / 'transition-resolutions' / path.name
            if os.path.lexists(resolution):
                resolved = turn_sources.read(resolution, MAX_METADATA)
                _exact(resolved, {'schema', 'transition_sha256', 'retirement_sha256', 'view_id', 'native_ids', 'tool_ids', 'source_references', 'nonlearning_native_ids'})
                anchors = value['last_view']
                if (resolved['schema'] != 'agent-memory/omp-transition-recovery-v1' or resolved['transition_sha256'] != _digest(value)
                        or not retired or resolved['retirement_sha256'] != _digest(fence) or anchors is None
                        or not set(anchors['native_ids']).issubset(_ids(resolved['native_ids']))
                        or not set(anchors['tool_ids']).issubset(_ids(resolved['tool_ids']))):
                    raise ContractError('Unbound transition source recovery')
                _uuid(resolved['view_id']); _source_refs(resolved['source_references'])
                if not resolved['source_references']:
                    if (not anchors['native_ids'] or not resolved['native_ids']
                            or _ids(resolved['nonlearning_native_ids']) != resolved['native_ids']):
                        raise ContractError('No-learning recovery lacks nonempty anchored exact graph')
                elif resolved['nonlearning_native_ids'] != []:
                    raise ContractError('Mixed source/nonlearning recovery proof')
            else:
                pending.append({'code': value['reason'], 'transition_id': value['transition_id']})
    pins.sort()
    closed = False
    if include_close and os.path.lexists(directory / 'closed.json'):
        value = turn_sources.read(directory / 'closed.json', 4096)
        _exact(value, {'schema', 'open_sha256', 'last_attempt_seq', 'refused_count', 'last_event_ordinal', 'inventory_sha256'})
        # Close pins contain immutable attempt/outcome/started/transition inventory,
        # not later retirement/recovery records.
        close_pins = [item for item in pins if item[0] != 'retirement.json' and not item[0].startswith(('resolutions/', 'transition-resolutions/'))]
        expected = {'schema': 'agent-memory/omp-stream-close-v1', 'open_sha256': open_digest,
            'last_attempt_seq': len(attempts), 'refused_count': refusal_count, 'last_event_ordinal': ordinal,
            'inventory_sha256': _digest(close_pins)}
        if value != expected:
            raise ContractError('Finite stream close inventory changed')
        closed = True
        pins.append(['closed.json', _digest(value)])
    if include_close and not closed:
        pending.append({'code': 'open-stream'})
    return {'session': opened['session'], 'instance_id': opened['instance_id'], 'open_sha256': open_digest,
            'closed': closed, 'retired': retired, 'pending': pending,
            '_hints': hints, '_verified_native_ids': verified_native_ids}, sorted(pins)


def _join_hints(repo, rows, pins):
    """One fresh linear per-session join; notifications never create source origin."""
    full, needed, row_hints = {}, {}, []
    for row in rows:
        session = row['session']
        full.setdefault(session, set()).update(row.pop('_verified_native_ids', set()))
        hints = row.pop('_hints', [])
        row_hints.append((row, hints))
        for _, leaf in hints:
            if leaf is not None:
                needed.setdefault(session, set()).add(leaf)
    dispositions = {}
    if needed:
        from .omp_transport import native_disposition_ids
        for session, leaves in needed.items():
            missing = leaves - full.get(session, set())
            if not missing:
                continue
            actual = native_disposition_ids(repo, session)
            if not isinstance(actual, set):
                raise ContractError('Native disposition inventory is not a verified set')
            for native in actual:
                turn_sources.bounded(native)
            dispositions[session] = missing.intersection(actual)
    for session, native_ids in dispositions.items():
        full.setdefault(session, set()).update(native_ids)
    coverage = []
    for row, hints in row_hints:
        session = row['session']
        covered = full.get(session, set())
        for sequence, leaf in hints:
            status = 'neutral' if leaf is None else 'covered' if leaf in covered else 'uncovered'
            coverage.append([session, row['instance_id'], sequence, leaf, status])
            if status == 'uncovered':
                row['pending'].append({'code': 'hint-uncovered', 'sequence': sequence, 'native_entry_id': leaf})
    if coverage:
        pins.append(['hint-coverage', coverage])


def _incomplete_startup(directory):
    """Account only an unpublished startup, never waive missing source records."""
    _uuid(directory.name)
    trust_paths.state(directory)
    pins = [['startup-directory', directory.name]]
    with os.scandir(directory) as scan:
        entries = sorted(list(scan), key=lambda entry: entry.name)
    for entry in entries:
        if entry.name not in {'live.lock', 'stream.lock', 'open.json'} or entry.is_symlink():
            raise ContractError('Unpublished stream has unknown or source-bearing state')
        raw = turn_sources.read_bytes(Path(entry.path), MAX_METADATA if entry.name == 'open.json' else 0)
        # OPEN contains only safe binding metadata, never a source/request body.
        pins.append([entry.name, hashlib.sha256(raw).hexdigest()])
    return {'session': None, 'session_sha256': directory.parent.name, 'instance_id': directory.name,
        'open_sha256': None, 'closed': False, 'retired': False,
        'pending': [{'code': 'startup-incomplete'}]}, pins


def _inventory(repo, base=None):
    """Complete sibling inventory. Lifecycle callers retain an EXCLUSIVE barrier."""
    with turn_sources.source_barrier(repo):
        base = _base(repo) if base is None else Path(base)
        if not trust_paths.state(base)['present']:
            raise ContractError('Finite stream private state is offline')
        namespace = base / 'omp-stream-v1'
        rows, pins = [], []
        lock_namespace = base / 'omp-stream-session-locks'
        if os.path.lexists(lock_namespace):
            for path in _entries(lock_namespace, r'[0-9a-f]{64}\.lock'):
                turn_sources.read_bytes(path, 0)
                if not os.path.lexists(namespace / path.stem):
                    rows.append({'session': None, 'session_sha256': path.stem, 'instance_id': None,
                        'open_sha256': None, 'closed': False, 'retired': False,
                        'pending': [{'code': 'startup-incomplete'}]})
                    pins.append([str(path.relative_to(base)), [['orphan-session-startup-lock', path.stem]]])
        if os.path.lexists(namespace):
            for session_dir in _entries(namespace, r'[0-9a-f]{64}', directories=True):
                directories, startup_marker = _session_instances(session_dir)
                if startup_marker is not None or not directories:
                    rows.append({'session': None, 'session_sha256': session_dir.name, 'instance_id': None,
                        'open_sha256': None, 'closed': False, 'retired': False,
                        'pending': [{'code': 'startup-incomplete'}]})
                    state_pin = ['startup-incomplete.json', _digest(startup_marker)] if startup_marker is not None else ['empty-startup-directory', session_dir.name]
                    pins.append([str(session_dir.relative_to(base)), [state_pin]])
                for directory in directories:
                    try:
                        opened = turn_sources.read(directory / 'open.json', MAX_METADATA)
                    except (FileNotFoundError, ValueError):
                        row, stream_pins = _incomplete_startup(directory)
                    else:
                        opened = _open_value(opened, directory)
                        if opened.get('repo_root') != str(Path(repo).resolve()):
                            raise ContractError('Foreign repository finite stream')
                        with turn_sources.private_lock(directory / 'stream.lock', shared=True):
                            row, stream_pins = _inspect(directory, opened)
                    rows.append(row)
                    pins.append([str(directory.relative_to(base)), stream_pins])
        _join_hints(repo, rows, pins)
        pending = [{'session': row['session'], 'instance_id': row['instance_id'],
                    **({'session_sha256': row['session_sha256']} if 'session_sha256' in row else {}), **item}
                   for row in rows for item in row['pending']]
        return {'schema': 'agent-memory/omp-stream-inventory-v1', 'streams': rows, 'pending': pending,
                'inventory_sha256': _digest(pins)}


def inventory(repo, base=None):
    try:
        return _inventory(repo, base)
    except (OSError, ValueError, TypeError, KeyError) as exc:
        if isinstance(exc, ContractError):
            raise
        raise ContractError('Finite stream inventory is missing, offline or malformed') from None


def drain_state(repo, base=None):
    return inventory(repo, base)


def validate_drain(repo, proof, base=None):
    current = inventory(repo, base)
    if current['pending'] or proof != current:
        raise ContractError('Finite source stream coverage remains pending or changed')
    return current


def retire_for_recovery(repo, session, *, exclude_instance=None):
    """Ingest only: source barrier -> transport, before on_started; skip live owners."""
    base = _base(repo)
    namespace = base / 'omp-stream-v1' / hashlib.sha256(turn_sources.bounded(session).encode()).hexdigest()
    handles = []
    with turn_sources.source_barrier(repo):
        if not os.path.lexists(namespace):
            return handles
        directories, _ = _session_instances(namespace)
        for directory in directories:
            if directory.name == exclude_instance:
                continue
            try:
                turn_sources.read(directory / 'open.json', MAX_METADATA)
            except (FileNotFoundError, ValueError):
                _incomplete_startup(directory)
                continue
            try:
                turn_sources.read_bytes(directory / 'live.lock', 0)
            except FileNotFoundError:
                if os.path.lexists(directory / 'open.json'):
                    raise ContractError('Published finite stream lacks its live-owner lock') from None
                # A concurrent startup has not published OPEN. It is not recoverable;
                # a persistent incomplete directory still refuses lifecycle inventory.
                continue
            probe = turn_sources.private_lock(directory / 'live.lock')
            try:
                probe.__enter__()
            except turn_sources.SourceTransportBusy:
                continue
            try:
                with turn_sources.private_lock(directory / 'stream.lock'):
                    if not os.path.lexists(directory / 'open.json'):
                        continue
                    opened = _open_value(turn_sources.read(directory / 'open.json', MAX_METADATA), directory)
                    if opened['session'] != session:
                        raise ContractError('Foreign recovery session')
                    path = directory / 'retirement.json'
                    if os.path.lexists(path):
                        fence = turn_sources.read(path, MAX_METADATA)
                    else:
                        fence = {'schema': 'agent-memory/omp-stream-retirement-v1', 'open_sha256': _digest(opened),
                            'instance_id': directory.name, 'session': session, 'fence_id': str(uuid.uuid4())}
                        _retain(path, fence)
                    _inspect(directory, opened)
                    handles.append({'directory': str(directory), 'fence': fence})
            finally:
                probe.__exit__(*sys.exc_info())
    return handles


def resolve_recovery(repo, fences, *, view_id, native_ids, tool_ids, payload_native_ids,
                     source_references, complete=False, leaf_id=None, nonlearning_evidence=None):
    """Bind exact already-ingested native evidence; never infer unknown identities.

    Only the trusted observer-ingest path calls this after its independently checked
    source/payload writes. Plain model/observer JSON cannot select this operation.
    """
    _uuid(view_id); _ids(native_ids); _ids(tool_ids); _ids(payload_native_ids)
    # NONLEARNING IDs may be included in payload_native_ids only after the
    # observer independently verifies their payload-not-required metadata shape.
    refs = _source_refs(source_references)
    if type(complete) is not bool:
        raise ContractError('Invalid checked inventory completeness')
    if nonlearning_evidence is not None and type(nonlearning_evidence) is not _NonlearningTailEvidence:
        raise ContractError('Nonlearning tail evidence is not observer-internal')
    if not refs and nonlearning_evidence is None:
        return []
    recovered = []
    with turn_sources.source_barrier(repo):
        for handle in fences:
            _exact(handle, {'directory', 'fence'})
            fence = handle['fence']
            if any(reference['session'] != fence['session'] for reference in refs):
                raise ContractError('Recovery references name a different native session')
            directory = _directory(_base(repo), fence['session'], fence['instance_id'])
            if str(directory) != handle['directory']:
                raise ContractError('Foreign recovery fence location')
            with turn_sources.private_lock(directory / 'stream.lock'):
                if turn_sources.read(directory / 'retirement.json', MAX_METADATA) != fence:
                    raise ContractError('Recovery retirement fence changed')
                opened = turn_sources.read(directory / 'open.json', MAX_METADATA)
                row, _ = _inspect(directory, opened)
                known = None
                all_known = True
                attempts = directory / 'attempts'
                paths = _entries(attempts, r'[1-9][0-9]*\.json') if os.path.lexists(attempts) else []
                for path in sorted(paths, key=lambda p: int(p.stem)):
                    attempt = turn_sources.read(path, MAX_METADATA)
                    known = _apply_identity(attempt['identity'], known)
                    all_known = all_known and (known is not None or _is_hint(attempt['identity']))
                    if _is_hint(attempt['identity']):
                        continue
                    if (not refs or known is None or not (known['native_ids'] or known['tool_ids'])
                            or not set(known['native_ids']).issubset(native_ids)
                            or not set(known['native_ids']).issubset(payload_native_ids)
                            or not set(known['tool_ids']).issubset(tool_ids)):
                        continue
                    target = directory / 'resolutions' / path.name
                    if os.path.lexists(target):
                        continue
                    _retain(target, {'schema': 'agent-memory/omp-stream-recovery-v1', 'open_sha256': _digest(opened),
                        'attempt_sha256': _digest(attempt), 'retirement_sha256': _digest(fence),
                        'sequence': attempt['sequence'], 'view_id': view_id, 'native_ids': native_ids,
                        'tool_ids': tool_ids, 'source_references': refs})
                    recovered.append({'instance_id': opened['instance_id'], 'sequence': attempt['sequence']})
                no_learning = (not refs and nonlearning_evidence is not None and complete
                    and row['closed'] and all_known and bool(paths) and bool(native_ids)
                    and nonlearning_evidence.session == fence['session']
                    and nonlearning_evidence.view_id == view_id
                    and nonlearning_evidence.native_ids == tuple(native_ids))
                transitions = directory / 'transitions'
                if complete and (refs or no_learning) and os.path.lexists(transitions):
                    for path in _entries(transitions, r'[0-9a-f-]{36}\.json'):
                        transition = turn_sources.read(path, MAX_METADATA)
                        anchors = transition['last_view']
                        if (anchors is None or (no_learning and not anchors['native_ids'])
                                or not set(anchors['native_ids']).issubset(native_ids)
                                or not set(native_ids).issubset(payload_native_ids)
                                or not set(anchors['tool_ids']).issubset(tool_ids)
                                or (leaf_id is not None and leaf_id not in native_ids)):
                            continue
                        target = directory / 'transition-resolutions' / path.name
                        if not os.path.lexists(target):
                            _retain(target, {'schema': 'agent-memory/omp-transition-recovery-v1',
                                'transition_sha256': _digest(transition), 'retirement_sha256': _digest(fence),
                                'view_id': view_id, 'native_ids': native_ids, 'tool_ids': tool_ids,
                                'source_references': refs, 'nonlearning_native_ids': native_ids if no_learning else []})
    return recovered


def run(repo, session, instance_id, lifecycle, *, startup, receipt=None, input=None, output=None):
    """Drain a detached finite stdin, with exactly one serial source executor."""
    turn_sources.bounded(session); _uuid(instance_id); _uuid(lifecycle)
    writer = ResponseWriter(sys.stdout.buffer if output is None else output, instance_id)
    try:
        preload(receipt)
        deadline = time.monotonic() + BUSY_SECONDS
        while True:
            try:
                binding = startup(repo, session, instance_id, lifecycle)
                if binding is None:
                    writer.send('INACTIVE', code='inactive-no-bank')
                    writer.finish()
                    return 0
                _binding(binding, repo, session, instance_id, lifecycle)
                normalized = binding['repo_root']
                coverage = Coverage(normalized, session, instance_id, lifecycle, binding, receipt=receipt)
                repo = normalized
                break
            except turn_sources.SourceTransportBusy:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    writer.send('REFUSED', code='busy-exhausted', outcome='unknown', status='pending')
                    writer.finish()
                    return 1
                time.sleep(min(BUSY_BACKOFF, remaining))
    except Exception:
        writer.send('REFUSED', code='binding-refused')
        writer.finish()
        return 1
    clocks = ClockWindows(instance_id, lambda: writer.live)
    budget = _Budget()
    reader = FrameReader(sys.stdin.buffer.raw if input is None else input, budget)
    condition = threading.Condition()
    queue = deque()
    state = {'eof': False, 'error': False, 'final': None, 'next': 1}
    executor = FiniteExecutor(coverage, writer, clocks, receipt)

    def read_frames():
        try:
            while True:
                frame = reader.read()
                if frame is None:
                    clocks.end()
                    return
                kind, sequence, nonce, raw = frame
                if kind in CONTROL_TYPES:
                    try:
                        clocks.control(kind, load(raw.decode('utf-8', 'strict')), writer)
                    except (ValueError, TypeError, KeyError, UnicodeError):
                        # A faulty calibration revokes guidance, never source capture.
                        clocks.revoke()
                    frame = raw = None
                    continue
                with condition:
                    if state['final'] is not None or sequence != state['next']:
                        budget.release(len(raw))
                        raise ContractError('Finite source sequence gap or frame after FINAL')
                    if kind == 'FINAL':
                        if nonce is not None:
                            budget.release(len(raw))
                            raise ContractError('FINAL cannot carry wake authority')
                        state['final'] = load(raw.decode('utf-8', 'strict'))
                        budget.release(len(raw))
                    else:
                        state['next'] += 1
                        queue.append(frame)
                    frame = raw = None
                    condition.notify_all()
        except Exception:
            with condition:
                state['error'] = True
        finally:
            clocks.end()
            with condition:
                state['eof'] = True
                condition.notify_all()

    thread = threading.Thread(target=read_frames, name='omp-stream-reader', daemon=True)
    thread.start()
    writer.send('READY', wake_support='unqualified', max_frame=MAX_FRAME,
                max_retained=MAX_RETAINED, max_frames=MAX_FRAMES)
    try:
        while True:
            with condition:
                while not queue and not state['eof']:
                    condition.wait()
                if not queue:
                    break
                frame = queue.popleft()
            try:
                executor.execute(*frame)
            except Exception:
                # Malformed metadata cannot become an arbitrary verified receipt.
                state['error'] = True
            finally:
                retained_bytes = len(frame[3])
                frame = None
                budget.release(retained_bytes)
        thread.join()
        if state['error'] or state['final'] is None:
            return 1
        try:
            coverage.attest_frame()
            return 0 if executor.metadata(coverage.close, state['final']) else 1
        except Exception:
            return 1
    finally:
        clocks.end()
        writer.finish()
        coverage.close_live()


def main(argv=None, *, startup=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True)
    parser.add_argument('--session', required=True)
    parser.add_argument('--instance', required=True)
    parser.add_argument('--lifecycle', required=True)
    parser.add_argument('--artifact', required=True)
    parser.add_argument('--omp-agent-dir', '--agent-dir', dest='omp_agent_dir', required=True)
    args = parser.parse_args(argv)
    from .omp_installation import observer_binding
    receipt = observer_binding(args.artifact, args.omp_agent_dir)
    if startup is None:
        from .omp_boundary import stream_startup
        def startup(repo, session, instance_id, lifecycle):
            return stream_startup(repo, session, instance_id, lifecycle, receipt=receipt)
    return run(args.repo, args.session, args.instance, args.lifecycle, startup=startup, receipt=receipt)
