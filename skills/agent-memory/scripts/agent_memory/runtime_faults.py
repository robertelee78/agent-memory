"""Logical lifecycle write boundaries, enabled only in explicit disposable tests.

A payload stage (including its file copies/fsyncs) is one logical write; Git's
private candidate clone is temporary input. Trace logging is test telemetry,
not another lifecycle mutation. Production does not create a trace.
"""
from contextlib import contextmanager
from contextvars import ContextVar
import json
import os
from pathlib import Path

from .bank_writes import fault
from .frontmatter import ContractError

_CURRENT = ContextVar('agent_memory_runtime_write', default=None)


def test_root():
    root = Path(os.environ.get('AGENT_MEMORY_TEST_ROOT', '/missing-test-world'))
    token = os.environ.get('AGENT_MEMORY_TEST_TOKEN')
    if (not token or not root.is_absolute() or root.is_symlink()
            or root.resolve().parent != Path('/tmp').resolve()
            or not root.name.startswith('agent-memory-test-')
            or json.loads((root/'.test-world.json').read_bytes()).get('token') != token):
        raise ContractError('Lifecycle fault tracing requires an explicit disposable World')
    return root.resolve()


@contextmanager
def operation(name, paths):
    testing = bool(os.environ.get('AGENT_MEMORY_FAULT_STAGE') or os.environ.get('AGENT_MEMORY_FAULT_TRACE'))
    root = test_root() if testing else None
    if testing and any(not Path(p).resolve().is_relative_to(root) or Path(p).resolve()==root for p in paths):
        raise ContractError('Lifecycle fault tests must not mutate paths outside their World')
    trace = os.environ.get('AGENT_MEMORY_FAULT_TRACE')
    if trace:
        trace = Path(trace)
        if not trace.is_absolute() or not trace.resolve().is_relative_to(root) or trace.is_symlink():
            raise ContractError('Lifecycle trace must remain inside its disposable World')
    token = _CURRENT.set({'name':name, 'step':0, 'testing':testing, 'trace':trace})
    try:
        yield
    finally:
        _CURRENT.reset(token)


@contextmanager
def temporary_probe():
    token = _CURRENT.set(None)
    try:
        yield
    finally:
        _CURRENT.reset(token)


def mutate(kind, action):
    current = _CURRENT.get()
    if not current or not current['testing']:
        return action()
    step = current['step']; current['step'] += 1
    def boundary(phase):
        point = f"{current['name']}-write-{step}-{phase}"
        if current['trace']:
            with current['trace'].open('a', encoding='utf-8') as stream:
                stream.write(json.dumps({'point':point, 'step':step, 'phase':phase, 'kind':kind})+'\n')
        fault(point)
    boundary('before')
    result = action()
    boundary('after')
    return result
