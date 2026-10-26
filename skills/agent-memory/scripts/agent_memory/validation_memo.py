"""Bounded, operation-local reuse of checks on exact immutable text inputs.

This never stores source observations, scope/time decisions or failed checks.
Loaded validators/schemas are fixed for an operation. No data is persisted.
"""
from collections import OrderedDict
from contextlib import contextmanager
from contextvars import ContextVar
from copy import deepcopy
import hashlib
import json

MAX_ENTRIES = 65544
MAX_MAPS = 8
MAX_BYTES = 64 * 1024 * 1024
_current = ContextVar('agent_memory_validation_memo', default=None)


class Memo:
    def __init__(self):
        self.values = OrderedDict()
        self.bytes = 0
        self.maps = 0

    def checked(self, namespace, key, produce, *, fresh_guard=None):
        identity = (namespace, key)
        if identity in self.values:
            if fresh_guard is not None:
                fresh_guard()
            value, weight = self.values[identity]
            self.values.move_to_end(identity)
            return deepcopy(value)
        value = produce()  # Never store a failure.
        try:
            weight = len(json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()) + 256
        except (TypeError, ValueError, UnicodeError):
            return value
        if weight > MAX_BYTES:
            return value
        is_map = namespace == 'bank-format'
        while self.values and (len(self.values) >= MAX_ENTRIES or self.bytes + weight > MAX_BYTES
                               or (is_map and self.maps >= MAX_MAPS)):
            (old_namespace, _), (_, old_weight) = self.values.popitem(last=False)
            self.bytes -= old_weight
            self.maps -= old_namespace == 'bank-format'
        self.values[identity] = (deepcopy(value), weight)
        self.bytes += weight
        self.maps += is_map
        return value


@contextmanager
def operation():
    """Nested calls share one memo; only the outer owner resets it, even on error."""
    existing = _current.get()
    if existing is not None:
        yield existing
        return
    token = _current.set(Memo())
    try:
        yield _current.get()
    finally:
        _current.reset(token)


def current():
    return _current.get()


def map_input(files):
    """Capture exact text, with finite eligibility limits and no authority claim."""
    if type(files) is not dict or len(files) > 65536:
        return None
    items = tuple(files.items())
    digest = hashlib.sha256()
    total = 0
    if any(type(path) is not str or type(body) is not str or len(path) > 4096
           or len(body) > 16384 for path, body in items):
        return None
    try:
        for path, body in sorted(items):
            name, raw = path.encode('utf-8'), body.encode('utf-8')
            total += len(name) + len(raw)
            if len(raw) > 16384 or total > MAX_BYTES:
                return None
            for part in (name, raw):
                digest.update(len(part).to_bytes(8, 'big'))
                digest.update(part)
    except UnicodeError:
        return None
    return digest.digest(), dict(items)


def text_key(content, maximum):
    if (type(content) is not str or len(content) > 65536 or type(maximum) is not int
            or maximum not in (16384, 65536)):
        return None
    try:
        raw = content.encode('utf-8')
    except UnicodeError:
        return None
    if len(raw) > 65536:
        return None
    return hashlib.sha256(raw).digest(), type(maximum).__name__, repr(maximum)
