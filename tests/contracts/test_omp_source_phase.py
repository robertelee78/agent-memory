"""R5 source/wake/enrollment phase faults; no native or timing qualification."""
from contextlib import contextmanager
import io
import json
import os
from pathlib import Path
import pickle
import subprocess
import sys
import threading
import time
import unittest
from unittest.mock import patch
import uuid

from world import World
from contracts import test_omp_boundary as fixtures
from agent_memory import omp_boundary, omp_projection, omp_stream as stream, omp_transport as transport
from agent_memory import layout_recovery_state, local_registry, scope_binding, turn_sources
from agent_memory.frontmatter import ContractError


def uid():
    return str(uuid.uuid4())


class PhaseResponses:
    def __init__(self):
        self.live = True
        self.values = []
        self.on_observed = None

    def send(self, kind, sequence=0, **value):
        if kind == 'OBSERVED' and self.on_observed is not None:
            self.on_observed(value)
        self.values.append((kind, sequence, value))
        return self.live


class ArmedSourcePhase(unittest.TestCase):
    """Real installed bindings, metadata, transport and shared registration."""
    def setUp(self):
        self.world = World()
        self.world.__enter__()
        self.addCleanup(self.world.__exit__, None, None, None)
        self.env = patch.dict(os.environ, self.world.env, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.helper = fixtures.NativeSources()
        self.repo, self.session, self.lifecycle = self.helper.fixture(self.world)
        root = Path(__file__).resolve().parents[2]
        agent = self.world.inside('source-phase-agent')
        agent.mkdir()
        self.receipt = json.loads(self.world.run([
            sys.executable, '-I', '-B', root / 'skills/agent-memory/scripts/installer.py',
            'install', '--development-source', root, '--presets', 'omp', '--omp-agent-dir', agent,
            '--omp-peers-upgraded'], timeout=60).stdout)['receipt']
        self.instance, self.nonce = uid(), uid()
        binding = omp_boundary.stream_startup(self.repo, self.session, self.instance, self.lifecycle,
                                               receipt=self.receipt)
        self.coverage = stream.Coverage(self.repo, self.session, self.instance, self.lifecycle,
                                       binding, receipt=self.receipt)
        self.addCleanup(self.coverage.close_live)
        self.responses = PhaseResponses()
        self.clocks = stream.ClockWindows(self.instance, lambda: self.responses.live, platform='darwin')
        self.addCleanup(self.clocks.end)
        self.executor = stream.FiniteExecutor(self.coverage, self.responses, self.clocks, receipt=self.receipt)
        self.entries = [self.helper.entry('phase-human', 'user', 'Ephemeral source phase request', attribution='user'),
                        self.helper.entry('phase-answer', 'assistant', 'The substantive answer is delivered.')]

    def body(self, *, armed=True, action='seal', ordinal=1):
        payload = fixtures.observer_payload(self.repo, self.session, self.entries)
        if action == 'seal':
            payload['request_wake'] = armed
        known = {'view_id': uid(), 'leaf_id': payload['view']['leaf_id'],
                 'native_ids': sorted(entry['id'] for entry in payload['view']['native_parents']), 'tool_ids': []}
        return {'request': {'schema': 'agent-memory/omp-observer-request-v2', 'action': action,
                           'session': self.session, 'lifecycle': self.lifecycle, 'payload': payload},
                'event_ordinal': ordinal, 'identity': known}

    def execute(self, body=None, *, nonce=True, sequence=1):
        self.executor.execute('REQUEST', sequence, self.nonce if nonce else None,
                              turn_sources.encoded(self.body() if body is None else body))

    def calibrate(self, nonce=None):
        nonce = self.nonce if nonce is None else nonce
        now = time.monotonic_ns()
        self.clocks.control('CLOCK', {'nonce': nonce, 'j0': str(now),
                                     'j_deadline': str(now + 950_000_000)}, self.responses)
        self.clocks.control('CLOCK-ACK', {'nonce': nonce, 'j0': str(now),
                                         'j3': str(time.monotonic_ns())}, self.responses)

    def outcome(self):
        return turn_sources.read(self.coverage.directory / 'outcomes/1.json', stream.MAX_METADATA)

    def result(self):
        return next(value['result'] for kind, _, value in self.responses.values if kind == 'OBSERVED')

    def roots(self):
        base = transport.session_dir(self.repo, self.session) / 'root-wakes'
        return sorted(base.glob('*.json')) if base.exists() else []

    def assert_source(self):
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertTrue(self.result()['verified'])
        self.assertFalse(self.result()['source_coverage_complete'])
        rows, faults = transport.pending_inventory(self.repo)
        self.assertEqual(faults, [])
        rows = [row for row in rows if row['session'] == self.session]
        self.assertEqual(len(rows), 2)
        self.assertTrue(all('observation_id' in row for row in rows))
        self.assertEqual(len(list((self.coverage.directory / 'outcomes').glob('*.json'))), 1)
        return rows

    def test_missing_clock_commits_pending_refs_then_registers_after_response(self):
        seen = []
        def observed(value):
            result = value['result']
            seen.append(result)
            rows, faults = transport.pending_inventory(self.repo)
            self.assertEqual(faults, [])
            self.assertEqual(len(rows), 2)
            self.assertTrue(all('observation_id' not in row for row in rows))
            self.assertTrue(all(row['source_reference'].startswith(('omp:capture:', 'omp:seal:')) for row in rows))
            self.assertEqual(result['source_references'], [])
            self.assertEqual(self.outcome()['source_references'], [])
            self.assertEqual(len(result['wake_payload']['enrollment_refs']), 2)
        self.responses.on_observed = observed
        self.execute()
        self.assert_source()
        self.assertEqual(len(seen), 1)
        self.assertEqual(self.result()['status'], 'pending-undeliverable-window')
        self.assertEqual(self.roots(), [])

    def test_ack_after_source_started_resolves_only_at_reservation(self):
        original = transport.capture
        called = []
        def capture(*args, **kwargs):
            if not called:
                called.append(True)
                self.assertIsNone(self.clocks.window(self.nonce))
                self.calibrate()
            return original(*args, **kwargs)
        with patch.object(transport, 'capture', side_effect=capture), patch.object(stream.time, 'monotonic_ns', return_value=1_000_000):
            self.execute()
        self.assert_source()
        self.assertTrue(self.result()['continue'])
        self.assertEqual(len(self.roots()), 1)

    def test_busy_outcome_beforewrite_or_exit_retries_only_identical_source_after_unlock(self):
        for after_write in (False, True):
            with self.subTest(after_write=after_write):
                # A separate real finite instance/root prevents immutable-slot reuse.
                if self.coverage.last_sequence:
                    self.coverage.close_live()
                    self.session, self.lifecycle, self.instance, self.nonce = uid(), uid(), uid(), uid()
                    binding = omp_boundary.stream_startup(self.repo, self.session, self.instance, self.lifecycle,
                                                           receipt=self.receipt)
                    self.coverage = stream.Coverage(self.repo, self.session, self.instance, self.lifecycle,
                                                   binding, receipt=self.receipt)
                    self.addCleanup(self.coverage.close_live)
                    self.responses = PhaseResponses()
                    self.clocks = stream.ClockWindows(self.instance, lambda: self.responses.live, platform='darwin')
                    self.addCleanup(self.clocks.end)
                    self.executor = stream.FiniteExecutor(self.coverage, self.responses, self.clocks, receipt=self.receipt)
                original = self.coverage.outcome
                writes = []
                captures = []
                real_capture = transport.capture
                def capture(*args, **kwargs):
                    captures.append(1)
                    return real_capture(*args, **kwargs)
                def outcome(*args):
                    writes.append(turn_sources.encoded(args))
                    if len(writes) == 1:
                        if after_write:
                            original(*args)
                        raise turn_sources.SourceTransportBusy('injected outcome retention/exit boundary')
                    # A real exclusive second descriptor proves transport has unwound.
                    with turn_sources.private_lock(omp_boundary._base(self.repo) / 'omp-transport.lock'):
                        pass
                    return original(*args)
                with patch.object(self.coverage, 'outcome', side_effect=outcome), patch.object(transport, 'capture', side_effect=capture):
                    self.execute()
                self.assertEqual(len(writes), 2)
                self.assertEqual(writes[0], writes[1])
                self.assertEqual(len(captures), 2)
                self.assertEqual(self.result()['status'], 'pending-wake-busy')
                self.assertEqual(self.roots(), [])
                self.assert_source()

    def test_nonbusy_outcome_failure_never_writes_second_outcome_or_verified_response(self):
        original = self.coverage.outcome
        calls = []
        def outcome(*args):
            calls.append(args)
            original(*args)
            raise ContractError('injected outcome exit drift')
        with patch.object(self.coverage, 'outcome', side_effect=outcome):
            with self.assertRaisesRegex(ContractError, 'outcome exit drift'):
                self.execute()
        self.assertEqual(len(calls), 1)
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertFalse(any(kind == 'OBSERVED' for kind, _, _ in self.responses.values))
        self.assertEqual(self.roots(), [])
        rows, _ = transport.pending_inventory(self.repo)
        self.assertTrue(all('observation_id' not in row for row in rows))

    def _cancelled_uncommitted_outcome(self, error):
        completions, writes, drains = [], [], []
        commit = stream._SourceCompletion.commit_source
        def commit_source(completion, context, result):
            completions.append(completion)
            return commit(completion, context, result)
        def outcome(*args):
            writes.append(args)
            self.clocks.control('CANCEL', {'nonce': self.nonce}, self.responses)
            raise error
        with patch.object(stream._SourceCompletion, 'commit_source', commit_source), \
                patch.object(self.coverage, 'outcome', side_effect=outcome), \
                patch.object(transport, 'drain_enrollments', side_effect=lambda *a, **kw: drains.append(True)):
            with self.assertRaises(type(error)) as refused:
                self.execute()
        self.assertIs(refused.exception, error)
        self.assertEqual(len(completions), 1)
        self.assertTrue(completions[0].attempted)
        self.assertFalse(completions[0].source_committed)
        self.assertFalse(completions[0]._live)
        self.assertFalse(completions[0].window.reserve())
        self.assertEqual(len(writes), 1)
        self.assertEqual(drains, [])
        self.assertTrue((self.coverage.directory / 'started/1.json').exists())
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        self.assertFalse(any(kind == 'OBSERVED' for kind, _, _ in self.responses.values))
        self.assertEqual(self.roots(), [])
        rows, faults = transport.pending_inventory(self.repo)
        self.assertEqual(faults, [])
        self.assertEqual(len(rows), 2)
        self.assertTrue(all('observation_id' not in row for row in rows))
        self.assertFalse(self.coverage.close({'last_attempt_seq': 1, 'refused_count': 0,
                                             'last_event_ordinal': 1, 'transition': None}))
        self.assertFalse((self.coverage.directory / 'closed.json').exists())

    def test_cancelled_uncommitted_outcome_refusal_never_verifies_partial_seal(self):
        self._cancelled_uncommitted_outcome(ContractError('controlled outcome refusal before write'))

    def test_cancelled_uncommitted_outcome_io_failure_never_verifies_partial_seal(self):
        self._cancelled_uncommitted_outcome(OSError('controlled outcome failure before write'))

    def test_wake_and_observer_unwind_faults_preserve_source_and_actual_reservation_fact(self):
        original_guard = transport._verify_observer_receipt
        reserved = []
        original_reserve = stream._LateNativeDeliveryWindow.reserve
        def reserve(window):
            # Calibrate immediately at the real reservation boundary; no timing claim.
            self.calibrate()
            result = original_reserve(window)
            reserved.append(result)
            return result
        @contextmanager
        def guard(*args, **kwargs):
            with original_guard(*args, **kwargs) as context:
                yield context
                if reserved:
                    raise ContractError('injected observer successful-exit fault')
        with patch.object(stream._LateNativeDeliveryWindow, 'reserve', reserve), patch.object(transport, '_verify_observer_receipt', guard):
            self.execute()
        self.assertEqual(reserved, [True])
        self.assert_source()
        self.assertFalse(self.result()['continue'])
        self.assertEqual(self.result()['status'], 'pending-wake-unknown-after-reservation')
        self.assertEqual(len(self.roots()), 1)

    def test_postcommit_pre_reservation_fault_has_no_second_outcome(self):
        original = stream._SourceCompletion.before_reserve
        def before_reserve(completion, context):
            original(completion, context)
            raise ContractError('injected pre-reservation drift')
        with patch.object(stream._SourceCompletion, 'before_reserve', before_reserve):
            self.execute()
        self.assert_source()
        self.assertEqual(self.result()['status'], 'pending-wake-unavailable')
        self.assertFalse(self.result()['continue'])
        self.assertEqual(self.roots(), [])

    def test_response_loss_still_drains_once_before_next_fifo_and_final(self):
        original = transport.drain_enrollments
        phases = []
        def drain(*args, **kwargs):
            phases.append('drain')
            self.assertTrue(self.outcome()['outcome'] == 'verified')
            return original(*args, **kwargs)
        def lost(value):
            phases.append('response')
            self.responses.on_observed = None
            raise BrokenPipeError('injected response loss')
        self.responses.on_observed = lost
        with patch.object(transport, 'drain_enrollments', side_effect=drain):
            with self.assertRaises(BrokenPipeError):
                self.execute()
        self.assertEqual(phases, ['response', 'drain'])
        rows, faults = transport.pending_inventory(self.repo)
        self.assertEqual(faults, [])
        self.assertTrue(all('observation_id' in row for row in rows))
        self.execute(self.body(armed=False, ordinal=2), nonce=False, sequence=2)
        self.assertTrue(self.coverage.close({'last_attempt_seq': 2, 'refused_count': 0,
                                           'last_event_ordinal': 2, 'transition': None}))
        self.assertTrue((self.coverage.directory / 'closed.json').exists())

    def test_nonarmed_and_direct_observation_keep_synchronous_registration(self):
        observed_rows = []
        self.responses.on_observed = lambda value: observed_rows.extend(transport.pending_inventory(self.repo)[0])
        self.execute(self.body(armed=False), nonce=False)
        self.assertEqual(len(observed_rows), 2)
        self.assertTrue(all('observation_id' in row for row in observed_rows))
        self.assert_source()
        direct_session, direct_lifecycle = uid(), uid()
        payload = fixtures.observer_payload(self.repo, direct_session, self.entries)
        payload['request_wake'] = True
        direct = transport.observe(self.repo, {
            'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
            'session': direct_session, 'lifecycle': direct_lifecycle, 'payload': payload},
            receipt=self.receipt, verified_binding=self.coverage.frame_proof)
        self.assertTrue(direct['verified'])
        self.assertEqual(len(direct['source_references']), 2)
        direct_rows = [row for row in transport.pending_inventory(self.repo)[0] if row['session'] == direct_session]
        self.assertEqual(len(direct_rows), 2)
        self.assertTrue(all('observation_id' in row for row in direct_rows))

    def _checkpoint_lock_swap(self, name):
        original = transport.seal
        base = omp_boundary._base(self.repo)
        path, old = base / name, base / (name + '.held')
        def seal(*args, **kwargs):
            result = original(*args, **kwargs)
            path.rename(old)
            path.touch(mode=0o600)
            return result
        try:
            with patch.object(transport, 'seal', side_effect=seal):
                if name == 'source-barrier.lock':
                    with self.assertRaises(ContractError):
                        self.execute()
                else:
                    self.execute()
            if (self.coverage.directory / 'outcomes/1.json').exists():
                self.assertNotEqual(self.outcome()['outcome'], 'verified')
            self.assertFalse(any(kind == 'OBSERVED' and value.get('result', {}).get('verified')
                                 for kind, _, value in self.responses.values))
            self.assertEqual(self.roots(), [])
        finally:
            if old.exists():
                path.unlink()
                old.rename(path)

    def test_source_checkpoint_rejects_actual_executor_barrier_fd_swap(self):
        self._checkpoint_lock_swap('source-barrier.lock')

    def test_source_checkpoint_rejects_actual_transport_fd_swap(self):
        self._checkpoint_lock_swap('omp-transport.lock')

    def _assert_no_started_or_retirement(self):
        self.assertFalse((self.coverage.directory / 'started/1.json').exists())
        self.assertFalse(list(self.coverage.base.rglob('retirement.json')))
        self.assertFalse(any(kind == 'STARTED' for kind, _, _ in self.responses.values))
        self.assertFalse(any(kind == 'OBSERVED' and value.get('result', {}).get('verified')
                             for kind, _, value in self.responses.values))

    def _assert_invalid_borrowed_observation(self, held):
        from runtime_update_fixtures import snapshot
        before = snapshot(self.coverage.base)
        started = []
        with self.assertRaises(ContractError) as refused:
            transport.observe(self.repo, self.body(armed=False)['request'], receipt=self.receipt,
                              verified_binding=self.coverage.frame_proof, held_source=held,
                              on_started=lambda: started.append(True))
        self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        self.assertEqual(started, [])
        self.assertEqual(snapshot(self.coverage.base), before)
        self._assert_no_started_or_retirement()

    def test_borrowed_consumer_refuses_released_source_observation_without_fallback(self):
        with turn_sources.source_barrier(self.repo) as held:
            held.recheck()
        self._assert_invalid_borrowed_observation(held)

    def test_borrowed_consumer_refuses_foreign_thread_source_observation_without_fallback(self):
        ready, release = threading.Event(), threading.Event()
        observations, errors = [], []
        def own_source():
            try:
                with turn_sources.source_barrier(self.repo) as held:
                    observations.append(held)
                    ready.set()
                    if not release.wait(10):
                        raise RuntimeError('Foreign source owner was not released')
                    held.recheck()
            except BaseException as exc:
                errors.append(exc)
                ready.set()
        owner = threading.Thread(target=own_source, daemon=True)
        owner.start()
        try:
            self.assertTrue(ready.wait(10))
            self.assertEqual(errors, [])
            self.assertEqual(len(observations), 1)
            self._assert_invalid_borrowed_observation(observations[0])
            # A live foreign owner still excludes an independent exclusive FD.
            with self.assertRaises(turn_sources.SourceTransportBusy):
                with turn_sources.private_lock(self.coverage.base / 'source-barrier.lock',
                                               existing_only=True):
                    pass
        finally:
            release.set()
            owner.join(10)
        self.assertFalse(owner.is_alive())
        self.assertEqual(errors, [])

    def test_borrowed_consumer_refuses_transport_observation_without_fallback(self):
        with turn_sources.private_lock(self.coverage.base / 'omp-transport.lock') as held:
            self._assert_invalid_borrowed_observation(held)
            held.recheck()

    def test_borrowed_consumer_refuses_stream_observation_without_fallback(self):
        with turn_sources.private_lock(self.coverage.lock_path, existing_only=True) as held:
            self._assert_invalid_borrowed_observation(held)
            held.recheck()

    def test_borrowed_consumer_refuses_live_observation_without_fallback(self):
        with turn_sources.private_lock(self.coverage.directory / 'live.lock',
                                       shared=True, existing_only=True) as held:
            self._assert_invalid_borrowed_observation(held)
            held.recheck()

    def _executor_borrowed_inode_swap(self, edge):
        borrowed = turn_sources._borrowed_source_barrier
        path = self.coverage.base / 'source-barrier.lock'
        old = self.coverage.base / 'source-barrier.borrowed-held'
        swapped = False
        def swap():
            nonlocal swapped
            path.rename(old)
            path.touch(mode=0o600)
            swapped = True
        @contextmanager
        def fault(held):
            if swapped:
                with borrowed(held) as observation:
                    yield observation
                return
            try:
                if edge == 'entry':
                    swap()
                with borrowed(held) as observation:
                    yield observation
                    if edge == 'exit':
                        swap()
            finally:
                # Restore before the executor owner unwinds: a failure here
                # must come from the borrowed check, not final owner cleanup.
                if old.exists():
                    path.unlink()
                    old.rename(path)
        with patch.object(turn_sources, '_borrowed_source_barrier', fault):
            with self.assertRaises(ContractError) as refused:
                self.execute()
        self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        self._assert_no_started_or_retirement()
        if edge == 'entry':
            self.assertFalse((self.coverage.directory / 'attempts/1.json').exists())
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        self.assertFalse((self.coverage.base / 'omp-transport.lock').exists())
        self.assertFalse((self.coverage.base / 'omp-source-v1').exists())
        with turn_sources.private_lock(path, existing_only=True) as owner:
            owner.recheck()

    def test_executor_borrowed_entry_refuses_actual_source_inode_swap(self):
        self._executor_borrowed_inode_swap('entry')

    def test_executor_borrowed_exit_refuses_actual_source_inode_swap(self):
        self._executor_borrowed_inode_swap('exit')

    @contextmanager
    def _configured_bank_change(self):
        """A genuine same-project bank move, never native activation of B."""
        from runtime_update_fixtures import snapshot
        config = self.repo / '.git/config'
        original_config = config.read_bytes()
        bank_a = self.repo / 'memory-bank'
        bank_b = self.repo / 'alternate-bank'
        def change():
            bank_a.rename(bank_b)
            self.world.git('config', '--local', 'agent-memory.bankroot', 'alternate-bank',
                           cwd=self.repo)
        def restore():
            config.write_bytes(original_config)
            if bank_b.exists():
                bank_b.rename(bank_a)
        try:
            change()
            context = scope_binding.private_enrollment_context(self.repo)
            self.assertEqual(context.bank, bank_b)
            self.assertEqual(context.project_id, self.coverage.binding['project_id'])
            base_b = layout_recovery_state._paths_for_bank(self.repo, bank_b)[2].parent
            before = snapshot(base_b)
            self.assertIsNone(before)
            restore()
            yield bank_b, base_b, before, change, restore
        finally:
            restore()

    def test_configured_bank_change_at_metadata_refuses_without_attempt_or_new_base(self):
        from runtime_update_fixtures import snapshot
        original = stream._request_identity
        with self._configured_bank_change() as (_, base_b, before, change, _):
            def identity(*args, **kwargs):
                result = original(*args, **kwargs)
                change()
                return result
            instance = uid()
            directory = stream._directory(self.coverage.base, self.session, instance)
            wire = stream.encode_frame('REQUEST', 1, self.body(armed=False)) + stream.encode_frame(
                'FINAL', 2, {'last_attempt_seq': 1, 'refused_count': 0,
                             'last_event_ordinal': 1, 'transition': None})
            def startup(repo, session, requested_instance, lifecycle):
                return omp_boundary.stream_startup(repo, session, requested_instance, lifecycle,
                                                   receipt=self.receipt)
            with patch.object(stream, '_request_identity', side_effect=identity):
                result = stream.run(self.repo, self.session, instance, self.lifecycle,
                                    startup=startup, receipt=self.receipt,
                                    input=io.BytesIO(wire), output=io.BytesIO())
            self.assertEqual(result, 1)
            self.assertFalse((directory / 'attempts/1.json').exists())
            self.assertFalse((directory / 'started/1.json').exists())
            self.assertFalse((directory / 'closed.json').exists())
            self.assertFalse(list(self.coverage.base.rglob('retirement.json')))
            self.assertEqual(snapshot(base_b), before)

    def _configured_bank_observer_refusal(self, action, *, canonical_writer=False):
        from runtime_update_fixtures import snapshot
        checked_view = omp_projection.checked_view
        with self._configured_bank_change() as (bank_b, base_b, before, change, restore):
            writer = None
            writer_owner = None
            deferred = False
            def checked(*args, **kwargs):
                nonlocal writer, writer_owner
                result = checked_view(*args, **kwargs)
                change()
                if canonical_writer:
                    writer = local_registry.exclusive(bank_b / '.agent-memory.lock',
                                                       uid(), 'bank-capture')
                    writer_owner = writer.__enter__()
                return result
            def observe(*args, **kwargs):
                try:
                    transport.observe(*args, **kwargs)
                except BaseException as error:
                    if isinstance(error, turn_sources.SourceTransportBusy):
                        self.assertIsInstance(error.__cause__, local_registry.CanonicalWriterBusy)
                        self.assertEqual(snapshot(base_b), before)
                        self._assert_no_started_or_retirement()
                        raise
                    # The real guard has unwound. Preserve its exact exception
                    # while restoring the transient race for outcome's mint.
                    self.assertIsInstance(error, ContractError)
                    self.assertEqual(snapshot(base_b), before)
                    self._assert_no_started_or_retirement()
                    restore()
                    raise
                try:
                    self.fail('Changed-bank observation unexpectedly succeeded')
                finally:
                    restore()
            def release(seconds):
                nonlocal writer, deferred
                self.assertEqual(seconds, stream.BUSY_BACKOFF)
                self.assertEqual((bank_b / '.agent-memory.lock').read_bytes(), writer_owner)
                self.assertEqual(snapshot(base_b), before)
                self._assert_no_started_or_retirement()
                deferred = True
                writer.__exit__(None, None, None)
                writer = None
            self.executor.observer = observe
            try:
                # One post-checked-view injection; real busy retries keep B
                # selected and reach the binding refusal after writer release.
                first = True
                def checked_once(*args, **kwargs):
                    nonlocal first
                    if first:
                        first = False
                        return checked(*args, **kwargs)
                    return checked_view(*args, **kwargs)
                with patch.object(omp_projection, 'checked_view', side_effect=checked_once), \
                        patch.object(stream, 'time', wraps=stream.time) as clock:
                    clock.monotonic.return_value = 0.0
                    clock.sleep.side_effect = release if canonical_writer else (
                        lambda _: self.fail('Bank mismatch must refuse without a busy wait'))
                    self.execute(self.body(armed=False, action=action), nonce=False)
            finally:
                if writer is not None:
                    writer.__exit__(None, None, None)
                restore()
            self.assertEqual(deferred, canonical_writer)
            self.assertEqual((self.outcome()['outcome'], self.outcome()['code']),
                             ('durable-refusal', 'observer-refused'))
            self._assert_no_started_or_retirement()
            self.assertEqual(snapshot(base_b), before)
            self.assertFalse((base_b / 'source-barrier.lock').exists())
            self.assertFalse((base_b / 'omp-transport.lock').exists())
            self._assert_refusal_then_valid_close(action)

    def _assert_refusal_then_valid_close(self, action):
        self.assertIn(('OBSERVED', 1, {'outcome': 'durable-refusal', 'code': 'observer-refused'}),
                      self.responses.values)
        self.assertFalse((self.coverage.base / 'omp-source-v1').exists())
        self.assertFalse(list(self.coverage.base.rglob('enrollments')))
        self.assertFalse(list(self.coverage.base.rglob('omp-drains')))
        self.assertFalse(list(self.coverage.base.rglob('omp-stream-drains')))
        self.executor.observer = transport.observe
        self.execute(self.body(armed=False, action=action, ordinal=2), nonce=False, sequence=2)
        self.assertTrue(next(value['result']['verified'] for kind, sequence, value
                             in self.responses.values if kind == 'OBSERVED' and sequence == 2))
        self.assertTrue(self.coverage.close({'last_attempt_seq': 2, 'refused_count': 0,
                                            'last_event_ordinal': 2, 'transition': None}))
        self.assertTrue((self.coverage.directory / 'closed.json').exists())

    def test_configured_bank_change_at_seal_refuses_without_new_private_state(self):
        self._configured_bank_observer_refusal('seal')

    def test_configured_bank_change_at_ingest_refuses_without_retirement_or_new_private_state(self):
        self._configured_bank_observer_refusal('ingest')

    def test_configured_bank_seal_canonical_writer_is_busy_before_binding_refusal(self):
        self._configured_bank_observer_refusal('seal', canonical_writer=True)

    def test_configured_bank_ingest_canonical_writer_is_busy_before_binding_refusal(self):
        self._configured_bank_observer_refusal('ingest', canonical_writer=True)

    def _activated_root_retarget_refusal(self, action, *, exclusive=False):
        from contracts.test_scope_binding import ScopeBindingContract
        from identity_fixtures import bind
        self.assertNotEqual(os.geteuid(), 0, 'The unreadable native-binding canary requires non-root')
        repo_a = self.repo
        repo_b = self.world.repository('second-activated-root')
        (repo_b / 'source.txt').write_text('Independent activated checkout fixture.\n')
        self.world.git('add', 'source.txt', cwd=repo_b)
        self.world.git('commit', '-m', 'Independent fixture checkout', cwd=repo_b)
        ScopeBindingContract().bank(self.world, repo_b)
        bind(self.world, repo_b)
        omp_boundary._activate_marker(repo_b)
        base_b = omp_boundary._base(repo_b)
        self.assertFalse((base_b / 'omp-transport.lock').exists())
        link = self.world.inside('retargeted-checkout')
        link.symlink_to(repo_a, target_is_directory=True)
        binding = self.coverage.binding
        self.coverage.close_live()
        self.instance = uid()
        self.coverage = stream.Coverage(link, self.session, self.instance, self.lifecycle,
                                       binding, receipt=self.receipt)
        self.addCleanup(self.coverage.close_live)
        self.executor = stream.FiniteExecutor(self.coverage, self.responses, self.clocks,
                                             receipt=self.receipt)
        native_binding = base_b / 'omp-bank-binding-v1.json'
        def physical_snapshot():
            return {str(path.relative_to(base_b)): (
                path.lstat().st_dev, path.lstat().st_ino, path.lstat().st_mode,
                path.read_bytes() if path.is_file() else None)
                for path in [base_b, *sorted(base_b.rglob('*'))]}
        before = physical_snapshot()
        original_mode = native_binding.stat().st_mode & 0o777
        checked_view = omp_projection.checked_view
        def retarget(target):
            fresh = self.world.inside('replacement-checkout-link')
            fresh.symlink_to(target, target_is_directory=True)
            os.replace(fresh, link)
        armed = True
        def checked(*args, **kwargs):
            nonlocal armed
            result = checked_view(*args, **kwargs)
            if armed:
                armed = False
                retarget(repo_b)
            return result
        def observe(*args, **kwargs):
            try:
                transport.observe(*args, **kwargs)
            except BaseException as error:
                # Snapshot before restoring the actual pathname race; only
                # the canary's mode is temporarily restored to read its bytes.
                self.assertIsInstance(error, ContractError)
                self.assertNotIsInstance(error, turn_sources.SourceTransportBusy)
                self._assert_no_started_or_retirement()
                self.assertFalse(list(base_b.rglob('retirement.json')))
                if holder is not None:
                    holder.recheck()
                    with self.assertRaises(turn_sources.SourceTransportBusy):
                        with turn_sources.private_lock(base_b / 'source-barrier.lock',
                                                       existing_only=True):
                            pass
                native_binding.chmod(original_mode)
                self.assertEqual(physical_snapshot(), before)
                retarget(repo_a)
                raise
            try:
                self.fail('Retargeted observation unexpectedly succeeded')
            finally:
                native_binding.chmod(original_mode)
                retarget(repo_a)
        self.executor.observer = observe
        gate = turn_sources.private_lock(base_b / 'source-barrier.lock', existing_only=True)
        holder = None
        try:
            if exclusive:
                holder = gate.__enter__()
            native_binding.chmod(0)
            with patch.object(omp_projection, 'checked_view', side_effect=checked), \
                    patch.object(stream, 'time', wraps=stream.time) as clock:
                clock.monotonic.return_value = 0.0
                clock.sleep.side_effect = lambda _: self.fail('Foreign source barrier must not be acquired')
                self.execute(self.body(armed=False, action=action), nonce=False)
            self.assertEqual((self.outcome()['outcome'], self.outcome()['code']),
                             ('durable-refusal', 'observer-refused'))
            self._assert_no_started_or_retirement()
            self.assertFalse((base_b / 'omp-transport.lock').exists())
            self.assertFalse(list(base_b.rglob('retirement.json')))
            if holder is not None:
                holder.recheck()
                with self.assertRaises(turn_sources.SourceTransportBusy):
                    with turn_sources.private_lock(base_b / 'source-barrier.lock',
                                                   existing_only=True):
                        pass
        finally:
            native_binding.chmod(original_mode)
            retarget(repo_a)
            if holder is not None:
                gate.__exit__(None, None, None)
        self.assertEqual(physical_snapshot(), before)
        self._assert_refusal_then_valid_close(action)

    def test_activated_root_retarget_at_seal_refuses_before_unreadable_native_binding(self):
        self._activated_root_retarget_refusal('seal')

    def test_activated_root_retarget_at_ingest_refuses_before_unreadable_native_binding(self):
        self._activated_root_retarget_refusal('ingest')

    def test_activated_root_retarget_at_seal_never_acquires_foreign_exclusive_barrier(self):
        self._activated_root_retarget_refusal('seal', exclusive=True)

    def test_activated_root_retarget_at_ingest_never_acquires_foreign_exclusive_barrier(self):
        self._activated_root_retarget_refusal('ingest', exclusive=True)

    def test_oldest8_after_response_and_cli_collision_leave_canonical_state_unchanged(self):
        from refresh_fixtures import RefreshFixture
        # A real synchronized authority makes --refresh's post-unlock success
        # discriminate contention from bad input or independently stale memory.
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, bank, _, _ = RefreshFixture().fixture(world)
            omp_boundary._activate_marker(repo)
            root = Path(__file__).resolve().parents[2]
            agent = world.inside('collision-agent')
            agent.mkdir()
            receipt = json.loads(world.run([
                sys.executable, '-I', '-B', root / 'skills/agent-memory/scripts/installer.py',
                'install', '--development-source', root, '--presets', 'omp', '--omp-agent-dir', agent,
                '--omp-peers-upgraded'], timeout=60).stdout)['receipt']
            am = Path(receipt['bin_dir']) / 'am'
            old_session, old_lifecycle = uid(), uid()
            old_entries = [
                self.helper.entry('collision-reviewed-human', 'user', 'Compute ephemeral arithmetic only.', attribution='user'),
                self.helper.entry('collision-reviewed-answer', 'assistant', 'The ephemeral arithmetic answer is delivered.')]
            old_payload = fixtures.observer_payload(repo, old_session, old_entries, terminal=True)
            old_payload['request_wake'] = False
            transport.observe(repo, {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                'session': old_session, 'lifecycle': old_lifecycle, 'payload': old_payload}, receipt=receipt)
            historical, faults = transport.pending_inventory(repo)
            self.assertEqual(faults, [])
            row = next(item for item in historical if item['session'] == old_session
                       and item['source_reference'].startswith('omp:seal:'))
            source = transport.source(repo, row['session'], row['lifecycle'], row['observation_id'])
            self.assertTrue(source['verified'])
            self.assertTrue(source['coverage_complete'])
            self.assertEqual(source['enrollment']['source_sha256'], row['source_sha256'])
            shared_before = turn_sources.inspect(repo, 'omp', row['session'], row['lifecycle'])
            assessment = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
            assessment.update(outcome='no-learning', assessment={
                'source_id': row['observation_id'],
                'source_review': 'Read the retained arithmetic-only human source and delivered answer in this exact sealed manifest; no durable project fact or instruction.',
                'candidates': []})
            variants = [
                (['--list-pending'], None),
                (['--source', row['observation_id'], '--session', row['session'], '--lifecycle', row['lifecycle']], None),
                (['--stdin', '--quiet', '--refresh'], json.dumps(assessment))]
            def public(arguments, data):
                return subprocess.run([str(am), '--json', 'omp-boundary', '--repo', str(repo), *arguments],
                    input=data, text=True, capture_output=True, cwd=repo, env=world.env, timeout=60)
            session, instance, lifecycle, nonce = uid(), uid(), uid(), uid()
            binding = omp_boundary.stream_startup(repo, session, instance, lifecycle, receipt=receipt)
            coverage = stream.Coverage(repo, session, instance, lifecycle, binding, receipt=receipt)
            responses = PhaseResponses()
            clocks = stream.ClockWindows(instance, lambda: responses.live, platform='darwin')
            entries = [self.helper.entry('phase-human-' + str(i), 'user', 'Ephemeral ' + str(i), attribution='user')
                       for i in range(6)]
            payload = fixtures.observer_payload(repo, session, entries)
            payload['request_wake'] = True
            body = {'request': {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                               'session': session, 'lifecycle': lifecycle, 'payload': payload},
                    'event_ordinal': 1, 'identity': {'view_id': uid(), 'leaf_id': payload['view']['leaf_id'],
                        'native_ids': sorted(entry['id'] for entry in payload['view']['native_parents']), 'tool_ids': []}}
            original_bank = {str(p.relative_to(bank)): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            transport_base = omp_boundary._base(repo) / 'omp-source-v1'
            original = transport.drain_enrollments
            collisions = []
            def drain(*args, **kwargs):
                original_source = {str(p.relative_to(transport_base)): p.read_bytes()
                                   for p in transport_base.rglob('*') if p.is_file()}
                for arguments, data in variants:
                    completed = public(arguments, data)
                    self.assertEqual(completed.returncode, 1, completed.stdout + completed.stderr)
                    if '--quiet' in arguments:
                        self.assertEqual(completed.stdout, '')
                        self.assertIn('source assessment pending', completed.stderr)
                    else:
                        value = json.loads(completed.stdout)
                        self.assertIs(value['verified'], False)
                        self.assertEqual(value['status'], 'pending')
                    collisions.append(completed)
                self.assertEqual(original_source, {str(p.relative_to(transport_base)): p.read_bytes()
                                                   for p in transport_base.rglob('*') if p.is_file()})
                self.assertEqual(shared_before, turn_sources.inspect(repo, 'omp', row['session'], row['lifecycle']))
                self.assertEqual(original_bank, {str(p.relative_to(bank)): p.read_bytes() for p in bank.rglob('*') if p.is_file()})
                return original(*args, **kwargs)
            try:
                executor = stream.FiniteExecutor(coverage, responses, clocks, receipt=receipt)
                with patch.object(transport, 'drain_enrollments', side_effect=drain):
                    executor.execute('REQUEST', 1, nonce, turn_sources.encoded(body))
                self.assertEqual(len(collisions), 3)
                rows, faults = transport.pending_inventory(repo)
                self.assertEqual(faults, [])
                current = [item for item in rows if item['session'] == session]
                self.assertEqual(len(current), 12)
                self.assertEqual(sum('observation_id' in item for item in current), 8)
                self.assertEqual(len(list((coverage.directory / 'outcomes').glob('*.json'))), 1)
                for arguments, data in variants:
                    completed = public(arguments, data)  # Exact argv and request, now unlocked.
                    self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
                    if '--quiet' in arguments:
                        self.assertEqual((completed.stdout, completed.stderr), ('', ''))
                    else:
                        self.assertTrue(json.loads(completed.stdout)['verified'])
                completed_source = transport.source(repo, row['session'], row['lifecycle'], row['observation_id'])
                self.assertEqual(completed_source['source'], source['source'])
                self.assertTrue(completed_source['observation']['completed'])
                self.assertEqual(original_bank, {str(p.relative_to(bank)): p.read_bytes() for p in bank.rglob('*') if p.is_file()})
            finally:
                clocks.end()
                coverage.close_live()

    def test_clock_ack_behind_accepted_16mib_frame_reaches_late_reservation(self):
        body = turn_sources.encoded(self.body())
        raw = body + b' ' * (stream.MAX_FRAME - len(body))
        now = time.monotonic_ns()
        wire = (f'AM249 REQUEST 1 {len(raw)} {self.nonce}\n'.encode() + raw +
                stream.encode_frame('CLOCK', 0, {'nonce': self.nonce, 'j0': str(now),
                    'j_deadline': str(now + 950_000_000)}) +
                stream.encode_frame('CLOCK-ACK', 0, {'nonce': self.nonce, 'j0': str(now), 'j3': str(now + 1)}))
        reader = stream.FrameReader(io.BytesIO(wire), stream._Budget())
        source = reader.read()
        self.assertEqual(len(source[3]), stream.MAX_FRAME)
        started, acknowledged = threading.Event(), threading.Event()
        failures = []
        original = transport.capture
        def capture(*args, **kwargs):
            if not started.is_set():
                self.assertIsNone(self.clocks.window(self.nonce))
                started.set()
                self.assertTrue(acknowledged.wait(20), 'independent control reader did not acknowledge')
            return original(*args, **kwargs)
        def controls():
            try:
                if not started.wait(20):
                    raise AssertionError('source did not begin')
                for _ in range(2):
                    kind, _, _, data = reader.read()
                    self.clocks.control(kind, stream.load(data.decode()), self.responses)
            except BaseException as error:
                failures.append(error)
            finally:
                acknowledged.set()
        thread = threading.Thread(target=controls)
        thread.start()
        try:
            # Freeze only reservation arithmetic after real frame/control parsing.
            with patch.object(transport, 'capture', side_effect=capture), patch.object(stream.time, 'monotonic_ns', return_value=now + 2):
                self.executor.execute(*source)
        finally:
            thread.join(25)
        self.assertFalse(thread.is_alive())
        self.assertEqual(failures, [])
        self.assert_source()
        self.assertTrue(self.result()['continue'])
        self.assertEqual(len(self.roots()), 1)


    def test_real_transport_lock_exit_swap_maps_committed_reservation_to_unknown(self):
        base = omp_boundary._base(self.repo)
        path = base / 'omp-transport.lock'
        old = base / 'transport-held-at-wake.lock'
        original = transport.replace_cursor
        swapped = []
        reserve = stream._LateNativeDeliveryWindow.reserve
        def reserve_now(window):
            self.calibrate()
            return reserve(window)
        def replace_cursor(target, cursor):
            result = original(target, cursor)
            if (cursor.get('control_interval') or {}).get('generated_guidance') and not swapped:
                path.rename(old)
                path.touch(mode=0o600)
                swapped.append(True)
            return result
        try:
            with patch.object(stream._LateNativeDeliveryWindow, 'reserve', reserve_now), patch.object(transport, 'replace_cursor', side_effect=replace_cursor):
                self.execute()
            self.assertEqual(swapped, [True])
            self.assert_source()
            self.assertEqual(self.result()['status'], 'pending-wake-unknown-after-reservation')
            self.assertFalse(self.result()['continue'])
            self.assertEqual(len(self.roots()), 1)
        finally:
            if old.exists():
                path.unlink()
                old.rename(path)

    def test_root_write_failure_after_actual_reservation_is_unknown_not_replayed(self):
        original = transport.retain
        reserve = stream._LateNativeDeliveryWindow.reserve
        roots = []
        def reserve_now(window):
            self.calibrate()
            return reserve(window)
        def retain(path, value, *args, **kwargs):
            result = original(path, value, *args, **kwargs)
            if value.get('schema') == 'agent-memory/omp-root-wake-v1':
                roots.append(path.read_bytes())
                raise OSError('injected lost root retention acknowledgement')
            return result
        with patch.object(stream._LateNativeDeliveryWindow, 'reserve', reserve_now), patch.object(transport, 'retain', side_effect=retain):
            self.execute()
        self.assertEqual(len(roots), 1)
        self.assert_source()
        self.assertEqual(self.result()['status'], 'pending-wake-unknown-after-reservation')
        self.assertFalse(self.result()['continue'])
        self.assertEqual(len(self.roots()), 1)

    def test_postresponse_busy_is_one_batch_and_leaves_discoverable_enrollments(self):
        calls = []
        def busy(*args, **kwargs):
            calls.append(True)
            raise turn_sources.SourceTransportBusy('injected materialization contention')
        with patch.object(transport, 'drain_enrollments', side_effect=busy):
            self.execute()
        self.assertEqual(calls, [True])
        self.assertTrue(self.result()['verified'])
        self.assertEqual(self.outcome()['outcome'], 'verified')
        rows, faults = transport.pending_inventory(self.repo)
        self.assertEqual(faults, [])
        self.assertEqual(len(rows), 2)
        self.assertTrue(all('observation_id' not in row for row in rows))
        self.assertFalse(list((transport.session_dir(self.repo, self.session)).rglob('registration-attempts/*.json')))
        # Later ordinary composed observation, not the failed batch, makes progress.
        self.execute(self.body(armed=False, ordinal=2), nonce=False, sequence=2)
        self.assertTrue(all('observation_id' in row for row in transport.pending_inventory(self.repo)[0]))

    def test_postresponse_control_flow_is_not_masked_or_closed(self):
        class Terminated(BaseException):
            pass
        with patch.object(transport, 'drain_enrollments', side_effect=Terminated('injected helper termination')):
            with self.assertRaises(Terminated):
                self.execute()
        self.assertTrue(self.result()['verified'])
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertFalse((self.coverage.directory / 'closed.json').exists())
        rows, _ = transport.pending_inventory(self.repo)
        self.assertTrue(all('observation_id' not in row for row in rows))

    def test_postcommit_resource_exhaustion_is_not_ordinary_wake_success(self):
        with patch.object(stream._SourceCompletion, 'before_reserve', side_effect=MemoryError('injected allocation failure')):
            with self.assertRaises(MemoryError):
                self.execute()
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertFalse(any(kind == 'OBSERVED' for kind, _, _ in self.responses.values))
        self.assertEqual(self.roots(), [])
        self.assertFalse((self.coverage.directory / 'closed.json').exists())

    def test_completion_guard_discovers_fresh_locations_and_restores_live_locator(self):
        original = self.coverage.outcome
        source_contexts = []
        fresh_locations = []
        discover = scope_binding._observe_enrollment_location
        commit = stream._SourceCompletion.commit_source
        def outcome(*args):
            # Observe the real nested lookup, not an echoed supplied context.
            fresh_locations.append(discover(self.repo))
            return original(*args)
        def commit_source(completion, context, result):
            source_contexts.append(context)
            self.assertIsNotNone(discover(self.repo))
            value = commit(completion, context, result)
            self.assertIs(scope_binding._enrollment_read_context(self.repo, context), context)
            self.assertIsNotNone(discover(self.repo))
            return value
        with patch.object(self.coverage, 'outcome', side_effect=outcome), patch.object(stream._SourceCompletion, 'commit_source', commit_source):
            self.execute()
        self.assertEqual(len(source_contexts), 1)
        self.assertEqual(fresh_locations, [None])
        self.assert_source()

    def test_plain_prereserve_open_read_refuses_drift_after_verified_source(self):
        original = stream._SourceCompletion.before_reserve
        path = self.coverage.directory / 'open.json'
        raw = path.read_bytes()
        def before_reserve(completion, context):
            value = turn_sources.read(path, stream.MAX_METADATA)
            value['checkout_key'] = 'foreign-checkout'
            path.write_bytes(turn_sources.encoded(value))
            try:
                return original(completion, context)
            finally:
                path.write_bytes(raw)
        with patch.object(stream._SourceCompletion, 'before_reserve', before_reserve):
            self.execute()
        self.assert_source()
        self.assertEqual(self.result()['status'], 'pending-wake-unavailable')
        self.assertEqual(self.roots(), [])


    def test_captured_source_checkpoint_failure_does_not_verify_or_schedule_drain(self):
        original = scope_binding.recheck_private_enrollment
        failed = []
        session_directory = transport.session_dir(self.repo, self.session)
        def recheck(context):
            value = original(context)
            if not failed and list(session_directory.glob('*/segment.json')):
                failed.append(True)
                raise ContractError('injected completed-source checkpoint drift')
            return value
        drain_calls = []
        drain = transport.drain_enrollments
        def drain_enrollments(*args, **kwargs):
            drain_calls.append(True)
            return drain(*args, **kwargs)
        with patch.object(scope_binding, 'recheck_private_enrollment', side_effect=recheck), patch.object(transport, 'drain_enrollments', side_effect=drain_enrollments):
            self.execute()
        self.assertEqual(failed, [True])
        self.assertEqual(drain_calls, [])
        self.assertNotEqual(self.outcome()['outcome'], 'verified')
        self.assertFalse(any(kind == 'OBSERVED' and value.get('result', {}).get('verified')
                             for kind, _, value in self.responses.values))
        self.assertEqual(self.roots(), [])

    def test_busy_outcome_exit_exhaustion_keeps_retained_record_without_response_or_drain(self):
        original = self.coverage.outcome
        retained = []
        def outcome(*args):
            original(*args)
            retained.append((self.coverage.directory / 'outcomes/1.json').read_bytes())
            raise turn_sources.SourceTransportBusy('injected persistent outcome exit deferral')
        with patch.object(self.coverage, 'outcome', side_effect=outcome), patch.object(stream, 'BUSY_SECONDS', 0):
            with self.assertRaises(turn_sources.SourceTransportBusy):
                self.execute()
        self.assertEqual(len(retained), 2)  # one in-lock attempt, one outer metadata attempt
        self.assertEqual(retained[0], retained[1])
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertFalse(any(kind == 'OBSERVED' for kind, _, _ in self.responses.values))
        rows, faults = transport.pending_inventory(self.repo)
        self.assertEqual(faults, [])
        self.assertTrue(all('observation_id' not in row for row in rows))
        self.assertEqual(self.roots(), [])


    def _ordinary_started_resource_fault(self, error, *, armed=False):
        original = transport.capture
        retained = []
        def capture(*args, **kwargs):
            result = original(*args, **kwargs)
            retained.append(result)
            raise error
        with patch.object(transport, 'capture', side_effect=capture):
            self.execute(self.body(armed=armed), nonce=armed)
        self.assertEqual(len(retained), 1)
        self.assertTrue((self.coverage.directory / 'started/1.json').exists())
        self.assertEqual(self.outcome()['outcome'], 'unknown')
        self.assertEqual(self.outcome()['code'], 'unknown-after-start')
        responses = [(kind, sequence, value) for kind, sequence, value in self.responses.values if kind == 'OBSERVED']
        self.assertEqual(responses, [('OBSERVED', 1, {'outcome': 'unknown', 'code': 'unknown-after-start'})])
        self.assertTrue(list(transport.session_dir(self.repo, self.session).glob('*/snapshots/*.json')))
        self.assertFalse((self.coverage.directory / 'closed.json').exists())

    def test_ordinary_started_memory_error_retains_original_unknown_response(self):
        self._ordinary_started_resource_fault(MemoryError('ordinary source allocation fault'))

    def test_ordinary_started_recursion_error_retains_original_unknown_response(self):
        self._ordinary_started_resource_fault(RecursionError('ordinary source recursion fault'))

    def test_armed_started_precommit_memory_error_retains_original_unknown_response(self):
        self._ordinary_started_resource_fault(MemoryError('armed source fault before completion attempt'), armed=True)

    def test_reused_header_nonce_decline_and_exit_fault_do_not_claim_own_reservation(self):
        reserve = stream._LateNativeDeliveryWindow.reserve
        guard = transport._verify_observer_receipt
        reservations, failed = [], []
        def reserve_window(window):
            if not reservations:
                self.calibrate()
            result = reserve(window)
            reservations.append(result)
            return result
        @contextmanager
        def observe_guard(*args, **kwargs):
            with guard(*args, **kwargs) as context:
                yield context
                if len(reservations) == 2 and not failed:
                    failed.append(True)
                    raise ContractError('second attempt exit fault after declined nonce reuse')
        with patch.object(stream._LateNativeDeliveryWindow, 'reserve', reserve_window), patch.object(transport, '_verify_observer_receipt', observe_guard), patch.object(stream.time, 'monotonic_ns', return_value=1_000_000):
            self.execute()
            self.entries.extend([
                self.helper.entry('second-human', 'user', 'A distinct second human root.', attribution='user'),
                self.helper.entry('second-answer', 'assistant', 'A distinct delivered answer.')])
            self.execute(self.body(ordinal=2), sequence=2)
        self.assertEqual(reservations, [True, False])
        self.assertEqual(failed, [True])
        results = {sequence: value['result'] for kind, sequence, value in self.responses.values if kind == 'OBSERVED'}
        self.assertTrue(results[1]['continue'])
        self.assertTrue(results[2]['verified'])
        self.assertFalse(results[2]['continue'])
        self.assertEqual(results[2]['status'], 'pending-wake-unavailable')
        self.assertEqual(len(self.roots()), 1)
        second = turn_sources.read(self.coverage.directory / 'outcomes/2.json', stream.MAX_METADATA)
        self.assertEqual(second['outcome'], 'verified')
        self.assertEqual(len(list((self.coverage.directory / 'outcomes').glob('*.json'))), 2)

    def test_closed_pipe_writer_false_drains_before_next_fifo_and_final_in_full_run(self):
        self.coverage.close_live()
        instance = uid()
        directory = stream._directory(stream._base(self.repo), self.session, instance)
        first = self.body()
        second = self.body(armed=False, ordinal=2)
        wire = (stream.encode_frame('REQUEST', 1, first, self.nonce) +
                stream.encode_frame('REQUEST', 2, second) +
                stream.encode_frame('FINAL', 3, {'last_attempt_seq': 2, 'refused_count': 0,
                    'last_event_ordinal': 2, 'transition': None}))
        read_fd, write_fd = os.pipe()
        os.close(read_fd)  # Actual EPIPE in ResponseWriter's independent writer.
        output = os.fdopen(write_fd, 'wb', buffering=0)
        self.addCleanup(output.close)
        phases, writers, pending_refs, retained_source = [], [], [], []
        send = stream.ResponseWriter.send
        capture = transport.capture
        drain = transport.drain_enrollments
        attempt = stream.Coverage.attempt
        close = stream.Coverage.close
        drained = []
        def send_response(writer, kind, sequence=0, **value):
            result = send(writer, kind, sequence, **value)
            if not writers:
                writers.append(writer)
            if kind == 'OBSERVED':
                phases.append(('response', sequence, result))
            return result
        def capture_source(*args, **kwargs):
            writer = writers[0]
            with writer.condition:
                self.assertTrue(writer.condition.wait_for(lambda: not writer.live, timeout=10),
                                'actual closed pipe did not revoke ResponseWriter')
            return capture(*args, **kwargs)
        def drain_sources(*args, **kwargs):
            if drained:
                return drain(*args, **kwargs)
            self.assertEqual(phases, [('response', 1, False)])
            self.assertFalse((directory / 'attempts/2.json').exists())
            self.assertFalse((directory / 'closed.json').exists())
            record = turn_sources.read(directory / 'outcomes/1.json', stream.MAX_METADATA)
            self.assertEqual(record['outcome'], 'verified')
            rows, faults = transport.pending_inventory(self.repo, enrollment_context=kwargs['enrollment_context'])
            self.assertEqual(faults, [])
            self.assertEqual(len(rows), 2)
            self.assertTrue(all('observation_id' not in row for row in rows))
            pending_refs.extend(row['source_reference'] for row in rows)
            base = transport.session_dir(self.repo, self.session, enrollment_context=kwargs['enrollment_context'])
            retained_source.extend((path, path.read_bytes()) for path in base.glob('*/snapshots/*.json'))
            phases.append(('drain-enter', 1))
            value = drain(*args, **kwargs)
            enrolled, faults = transport.pending_inventory(self.repo, enrollment_context=kwargs['enrollment_context'])
            self.assertEqual(faults, [])
            self.assertEqual({row['source_reference'] for row in enrolled}, set(pending_refs))
            self.assertTrue(all('observation_id' in row for row in enrolled))
            self.assertTrue(all(path.read_bytes() == raw for path, raw in retained_source))
            drained.append(True)
            phases.append(('drain-exit', 1))
            return value
        def attempt_source(coverage, sequence, *args, **kwargs):
            if sequence == 2:
                self.assertEqual(phases, [('response', 1, False), ('drain-enter', 1), ('drain-exit', 1)])
                self.assertFalse((directory / 'closed.json').exists())
                phases.append(('next-fifo', 2))
            return attempt(coverage, sequence, *args, **kwargs)
        def close_source(coverage, final):
            self.assertEqual(phases[-2:], [('next-fifo', 2), ('response', 2, False)])
            self.assertTrue(drained)
            phases.append(('final', 3))
            return close(coverage, final)
        def startup(repo, session, requested_instance, lifecycle):
            return omp_boundary.stream_startup(repo, session, requested_instance, lifecycle, receipt=self.receipt)
        with patch.object(stream.ResponseWriter, 'send', send_response), patch.object(transport, 'capture', side_effect=capture_source), patch.object(transport, 'drain_enrollments', side_effect=drain_sources), patch.object(stream.Coverage, 'attempt', attempt_source), patch.object(stream.Coverage, 'close', close_source):
            result = stream.run(self.repo, self.session, instance, self.lifecycle, startup=startup,
                                receipt=self.receipt, input=io.BytesIO(wire), output=output)
        self.assertEqual(result, 0)
        self.assertEqual(phases, [('response', 1, False), ('drain-enter', 1), ('drain-exit', 1),
                                  ('next-fifo', 2), ('response', 2, False), ('final', 3)])
        self.assertTrue((directory / 'closed.json').exists())
        self.assertTrue(all(turn_sources.read(directory / f'outcomes/{sequence}.json', stream.MAX_METADATA)['outcome'] == 'verified'
                            for sequence in (1, 2)))
        self.assertTrue(retained_source)
        self.assertTrue(all(path.read_bytes() == raw for path, raw in retained_source))


    def _postreserve_control_and_exit_fault(self, action):
        reserve = stream._LateNativeDeliveryWindow.reserve
        replace = transport.replace_cursor
        guard = transport._verify_observer_receipt
        windows, controls, failures = [], [], []
        def reserve_window(window):
            self.calibrate()
            value = reserve(window)
            self.assertTrue(value)
            windows.append(window)
            return value
        def replace_cursor(path, cursor):
            value = replace(path, cursor)
            if (cursor.get('control_interval') or {}).get('generated_guidance') and not controls:
                self.assertEqual(len(windows), 1)
                self.assertTrue(windows[0].reservation_started)
                self.assertEqual(len(self.roots()), 1)
                if action == 'cancel':
                    self.clocks.control('CANCEL', {'nonce': self.nonce}, self.responses)
                elif action == 'replacement':
                    self.clocks.control('CLOCK', {'nonce': uid(), 'j0': '1000000',
                        'j_deadline': '951000000'}, self.responses)
                elif action == 'eof':
                    self.clocks.end()
                elif action == 'output-loss':
                    self.responses.live = False
                else:
                    raise AssertionError('Unknown controlled post-reservation transition')
                controls.append(action)
                self.assertTrue(windows[0].reservation_started)
            return value
        @contextmanager
        def observe_guard(*args, **kwargs):
            with guard(*args, **kwargs) as context:
                yield context
                if controls and not failures:
                    failures.append(True)
                    raise ContractError('observer exit fault after actual reservation and ' + action)
        with patch.object(stream._LateNativeDeliveryWindow, 'reserve', reserve_window), patch.object(transport, 'replace_cursor', side_effect=replace_cursor), patch.object(transport, '_verify_observer_receipt', observe_guard), patch.object(stream.time, 'monotonic_ns', return_value=1_000_000):
            self.execute()
        self.assertEqual(controls, [action])
        self.assertEqual(failures, [True])
        self.assertEqual(len(windows), 1)
        self.assertTrue(windows[0].reservation_started)  # Survives completion.close().
        self.assertFalse(windows[0].reserve())
        self.assert_source()
        self.assertFalse(self.result()['continue'])
        self.assertEqual(self.result()['status'], 'pending-wake-unknown-after-reservation')
        self.assertEqual(len(self.roots()), 1)

    def test_cancel_after_actual_reserve_then_exit_fault_retains_one_unknown_root(self):
        self._postreserve_control_and_exit_fault('cancel')

    def test_replacement_after_actual_reserve_then_exit_fault_retains_one_unknown_root(self):
        self._postreserve_control_and_exit_fault('replacement')

    def test_eof_after_actual_reserve_then_exit_fault_retains_one_unknown_root(self):
        self._postreserve_control_and_exit_fault('eof')

    def test_output_loss_after_actual_reserve_then_exit_fault_retains_one_unknown_root(self):
        self._postreserve_control_and_exit_fault('output-loss')


class HeldLockObservations(unittest.TestCase):
    def test_actual_descriptor_identity_and_closed_observation(self):
        with World() as world:
            parent = world.inside('held-locks')
            parent.mkdir(mode=0o700)
            path = parent / 'lock'
            with turn_sources.private_lock(path) as held:
                held.recheck()
                with self.assertRaises(TypeError):
                    pickle.dumps(held)
                path.rename(parent / 'old-lock')
                path.touch(mode=0o600)
                with self.assertRaises(ContractError):
                    held.recheck()
                path.unlink()
                (parent / 'old-lock').rename(path)
            with self.assertRaises(ContractError):
                held.recheck()

    def test_parent_checks_only_existing_private_lock_mode(self):
        for existing in (False, True):
            with self.subTest(existing=existing), World() as world:
                parent = world.inside('parent')
                parent.mkdir(mode=0o700)
                path = parent / 'lock'
                path.touch(mode=0o600)
                with turn_sources.private_lock(path, existing_only=existing) as held:
                    parent.rename(world.inside('old-parent'))
                    parent.mkdir(mode=0o700)
                    # Hard-link the *actual* held inode into a replacement parent.
                    os.link(world.inside('old-parent/lock'), path)
                    if existing:
                        with self.assertRaisesRegex(ContractError, 'parent changed'):
                            held.recheck()
                    else:
                        held.recheck()
                    path.unlink()
                    parent.rmdir()
                    world.inside('old-parent').rename(parent)

    def test_nested_barrier_exposes_actual_executor_held_observation(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, _, _ = fixtures.NativeSources().fixture(world)
            with turn_sources.source_barrier(repo) as outer:
                with turn_sources.source_barrier(repo) as inner:
                    self.assertIs(inner, outer)
                    inner.recheck()
                outer.recheck()
            with self.assertRaises(ContractError):
                outer.recheck()


class LateWindowControls(unittest.TestCase):
    def test_nonce_cancel_replacement_eof_output_loss_and_one_shot_lookup(self):
        for mode in ('early', 'late', 'cancel', 'replace', 'eof', 'output-loss', 'closed', 'wrong-nonce'):
            with self.subTest(mode=mode):
                responses = PhaseResponses()
                owner = stream.ClockWindows(uid(), lambda: responses.live, platform='darwin')
                nonce = uid()
                late = stream._LateNativeDeliveryWindow(stream._WINDOW_MINT, owner, nonce)
                if mode == 'early':
                    self.assertFalse(late.reserve())
                with patch.object(stream.time, 'monotonic_ns', side_effect=[1000, 1010]):
                    owner.control('CLOCK', {'nonce': nonce, 'j0': '100', 'j_deadline': '950000100'}, responses)
                owner.control('CLOCK-ACK', {'nonce': nonce, 'j0': '100', 'j3': '200'}, responses)
                if mode == 'cancel':
                    owner.control('CANCEL', {'nonce': nonce}, responses)
                elif mode == 'replace':
                    owner.control('CLOCK', {'nonce': uid(), 'j0': '100', 'j_deadline': '950000100'}, responses)
                elif mode == 'eof':
                    owner.end()
                elif mode == 'output-loss':
                    responses.live = False
                elif mode == 'closed':
                    late.close()
                elif mode == 'wrong-nonce':
                    late = stream._LateNativeDeliveryWindow(stream._WINDOW_MINT, owner, uid())
                with patch.object(stream.time, 'monotonic_ns', return_value=0):
                    self.assertEqual(late.reserve(), mode == 'late')
                    self.assertFalse(late.reserve())
                self.assertEqual(late.reservation_started, mode == 'late')
                late.close()
                with self.assertRaises(TypeError):
                    pickle.dumps(late)
                owner.end()

    def test_cancel_before_clock_cannot_revive_source_nonce(self):
        responses = PhaseResponses()
        owner = stream.ClockWindows(uid(), lambda: True, platform='darwin')
        nonce = uid()
        owner.control('CANCEL', {'nonce': nonce}, responses)
        with self.assertRaises(ContractError):
            owner.control('CLOCK', {'nonce': nonce, 'j0': '100', 'j_deadline': '950000100'}, responses)
        self.assertIsNone(owner.window(nonce))
        owner.end()
