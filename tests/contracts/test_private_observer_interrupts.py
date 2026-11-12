"""Private absence fences refuse races without replacing cancellation or fatal exits."""
import asyncio
from contextlib import contextmanager
import os
from pathlib import Path
import sys
import unittest
import uuid
from unittest.mock import patch

from world import World
from contracts import test_comparison

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'skills/agent-memory/scripts'))
from agent_memory import bank_writes, omp_boundary, private_state, recovery_frontier, turn_sources
from agent_memory.frontmatter import ContractError


class PrivateObserverInterrupts(unittest.TestCase):
    fatal_types = (KeyboardInterrupt, SystemExit, GeneratorExit, asyncio.CancelledError, MemoryError)

    def fixture(self, world):
        repo = world.repository()
        bank = repo / 'memory-bank'
        bank.mkdir()
        (bank / 'my-note.md').write_text('Retain the ordinary bank and all raced private evidence.\n')
        operations, _ = bank_writes.state_paths(repo, bank, '00000000-0000-4000-8000-000000000000')
        base = operations.parent
        turn_sources.private_mkdir(base)
        return repo, bank, base

    def evidence(self, world):
        # Include bytes, entry identity and permissions, not just an invocation counter.
        return {str(path.relative_to(world.root)):
                (path.lstat().st_dev, path.lstat().st_ino, path.lstat().st_mode,
                 os.readlink(path) if path.is_symlink() else path.read_bytes() if path.is_file() else None)
                for path in world.root.rglob('*')}

    def create_private_file(self, path):
        turn_sources.private_mkdir(path.parent)
        with path.open('xb') as stream:
            stream.write(b'RACED_PRIVATE_EVIDENCE_MUST_SURVIVE\n')
        path.chmod(0o600)

    @contextmanager
    def guard(self, family, repo, base):
        if family == 'transport':
            with omp_boundary._read_only_transport_lock(base):
                yield base / 'omp-transport.lock'
        elif family == 'native':
            with private_state.observation_barrier(repo):
                yield base / 'source-barrier.lock'
        else:
            directory = base / 'turn-sources-v2' / 'turns' / ('a' * 64)
            turn_sources.private_mkdir(directory)
            retained = directory / 'retained-source.json'
            if not retained.exists():
                self.create_private_file(retained)
            with turn_sources.source_barrier(repo, exclusive=True), turn_sources.read_only_locks():
                with turn_sources.locked(directory):
                    yield directory / 'completion.lock'

    def raced_guard(self, family, failures):
        for failure_type in failures:
            with self.subTest(family=family, failure=failure_type), World() as world, \
                    patch.dict(os.environ, world.env, clear=True):
                repo, _, base = self.fixture(world)
                failure = failure_type('original guarded-body exit') if failure_type else None
                saved = None
                expected = failure_type if failure_type in self.fatal_types else ContractError
                with self.assertRaises(expected) as caught:
                    with self.guard(family, repo, base) as raced:
                        self.create_private_file(raced)
                        saved = self.evidence(world)
                        if failure is not None:
                            raise failure
                if failure_type in self.fatal_types:
                    self.assertIs(caught.exception, failure)
                elif failure is not None:
                    self.assertIsNot(caught.exception, failure)
                    self.assertIs(caught.exception.__context__, failure)
                self.assertIsNotNone(saved, 'The guarded body must physically create its evidence')
                self.assertEqual(self.evidence(world), saved)
                self.assertFalse(turn_sources._read_only_locks.get())
                self.assertEqual(turn_sources._barriers.get(), {})
                if family == 'completion':
                    # Reopening the real lock proves teardown released it, independently
                    # of the ContextVar state used to recognize nested barriers.
                    with turn_sources.private_lock(base / 'source-barrier.lock', existing_only=True):
                        pass

    def test_transport_absence_rechecks_normal_and_ordinary_failures(self):
        self.raced_guard('transport', (None, ContractError, RuntimeError))

    def test_transport_absence_preserves_interrupts_and_fatal_failures(self):
        self.raced_guard('transport', self.fatal_types)

    def test_native_absence_rechecks_normal_and_ordinary_failures(self):
        self.raced_guard('native', (None, ContractError, RuntimeError))

    def test_native_absence_preserves_interrupts_and_fatal_failures(self):
        self.raced_guard('native', self.fatal_types)

    def test_completion_absence_rechecks_normal_and_ordinary_failures(self):
        self.raced_guard('completion', (None, ContractError, RuntimeError))

    def test_completion_absence_preserves_interrupts_and_fatal_failures(self):
        self.raced_guard('completion', self.fatal_types)

    def test_unchanged_absence_allows_normal_exit_and_preserves_ordinary_error(self):
        for family in ('transport', 'native', 'completion'):
            with self.subTest(family=family), World() as world, \
                    patch.dict(os.environ, world.env, clear=True):
                repo, _, base = self.fixture(world)
                with self.guard(family, repo, base) as absent:
                    self.assertFalse(absent.exists())
                self.assertFalse(absent.exists())
                failure = RuntimeError('ordinary error without a race')
                with self.assertRaises(RuntimeError) as caught:
                    with self.guard(family, repo, base):
                        raise failure
                self.assertIs(caught.exception, failure)
                self.assertFalse(absent.exists())

    @contextmanager
    def race_at_call(self, function, race, failure):
        # Inject a real filesystem race at a body boundary without replacing any
        # guard, inventory result, scope decision or stock runtime implementation.
        previous = sys.gettrace()
        fired = []
        def trace(frame, event, arg):
            if event == 'call' and frame.f_code is function.__code__ and not fired:
                fired.append(True)
                race()
                if failure is not None:
                    raise failure
            return None
        sys.settrace(trace)
        try:
            yield fired
        finally:
            sys.settrace(previous)

    def compatibility_race(self, failures):
        for failure_type in failures:
            with self.subTest(failure=failure_type), World() as world, \
                    patch.dict(os.environ, world.env, clear=True):
                repo, _, base = self.fixture(world)
                saved = []
                def race():
                    self.create_private_file(base / 'source-barrier.lock')
                    saved.append(self.evidence(world))
                failure = failure_type('original lifecycle exit') if failure_type else None
                with patch.object(Path, 'cwd', return_value=repo), \
                        self.race_at_call(recovery_frontier._observe_bank, race, failure) as fired:
                    if failure_type in self.fatal_types:
                        with self.assertRaises(failure_type) as caught:
                            private_state.require_compatible(None, {'private_state_capabilities': []}, 'install')
                        self.assertIs(caught.exception, failure)
                    else:
                        result = private_state.required_private_state_capabilities(
                            None, {'private_state_capabilities': []}, 'install')
                        self.assertEqual(result['decision'], 'pending')
                        self.assertTrue(result['unknown_state'])
                        self.assertTrue(result['private_evidence_preserved'])
                self.assertEqual(fired, [True])
                self.assertEqual(len(saved), 1)
                self.assertEqual(self.evidence(world), saved[0])

    def test_public_compatibility_preserves_interrupts_and_fatal_failures(self):
        self.compatibility_race(self.fatal_types)

    def test_public_compatibility_race_remains_pending_for_normal_and_ordinary_failure(self):
        self.compatibility_race((None, ContractError, RuntimeError))

    def test_public_native_drain_preserves_interrupts_and_releases_source_barrier(self):
        for failure_type in (None, ContractError, *self.fatal_types):
            with self.subTest(failure=failure_type), World() as world, \
                    patch.dict(os.environ, world.env, clear=True):
                repo, _ = test_comparison.ComparisonContract().fixture(world)
                base = omp_boundary._base(repo)
                with turn_sources.source_barrier(repo, exclusive=True):
                    pass
                saved = []
                def race():
                    self.create_private_file(base / 'omp-transport.lock')
                    saved.append(self.evidence(world))
                failure = failure_type('original native drain exit') if failure_type else None
                expected = failure_type if failure_type in self.fatal_types else ContractError
                with self.race_at_call(omp_boundary._drain_inventory, race, failure) as fired:
                    with self.assertRaises(expected) as caught:
                        omp_boundary.drain_source_capabilities(repo, quiesced=True, check=True)
                if failure_type in self.fatal_types:
                    self.assertIs(caught.exception, failure)
                elif failure is not None:
                    self.assertIsNot(caught.exception, failure)
                    self.assertIs(caught.exception.__context__, failure)
                self.assertEqual(fired, [True])
                self.assertEqual(len(saved), 1)
                self.assertEqual(self.evidence(world), saved[0])
                self.assertEqual(turn_sources._barriers.get(), {})
                self.assertFalse(turn_sources._read_only_locks.get())
                with turn_sources.private_lock(base / 'source-barrier.lock', existing_only=True):
                    pass

    def test_public_source_inspection_preserves_interrupts_with_raced_completion_lock(self):
        for failure_type in (None, ContractError, *self.fatal_types):
            with self.subTest(failure=failure_type), World() as world, \
                    patch.dict(os.environ, world.env, clear=True):
                repo, _ = test_comparison.ComparisonContract().fixture(world)
                session, lifecycle = str(uuid.uuid4()), str(uuid.uuid4())
                turn_sources.register(repo, 'codex', session, lifecycle, '0' * 64)
                _, _, _, directory, _ = turn_sources.private_namespace(repo, 'codex', session, lifecycle)
                completion = directory / 'completion.lock'
                self.assertFalse(completion.exists())
                saved = []
                def race():
                    self.create_private_file(completion)
                    saved.append(self.evidence(world))
                failure = failure_type('original source inspection exit') if failure_type else None
                expected = failure_type if failure_type in self.fatal_types else ContractError
                with turn_sources.source_barrier(repo, exclusive=True), turn_sources.read_only_locks():
                    with self.race_at_call(turn_sources.private_inventory, race, failure) as fired:
                        with self.assertRaises(expected) as caught:
                            turn_sources.inspect(repo, 'codex', session, lifecycle)
                if failure_type in self.fatal_types:
                    self.assertIs(caught.exception, failure)
                elif failure is not None:
                    self.assertIsNot(caught.exception, failure)
                    self.assertIs(caught.exception.__context__, failure)
                self.assertEqual(fired, [True])
                self.assertEqual(len(saved), 1)
                self.assertEqual(self.evidence(world), saved[0])
                self.assertEqual(turn_sources._barriers.get(), {})
                self.assertFalse(turn_sources._read_only_locks.get())
                base = directory.parents[2]
                with turn_sources.private_lock(base / 'source-barrier.lock', existing_only=True):
                    pass



if __name__ == '__main__':
    unittest.main()
