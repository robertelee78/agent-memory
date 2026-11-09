"""Finite FIFO loss/phase/coverage boundaries; no native-runtime qualification."""
from contextlib import ExitStack, contextmanager, nullcontext
import io
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest import mock
import uuid

from world import World
from contracts import test_checkpoint
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import omp_stream as stream, turn_sources
from agent_memory import bank_transactions, bank_writes, local_registry, omp_boundary, omp_installation, scope_binding
from agent_memory.frontmatter import ContractError


def uid():
    return str(uuid.uuid4())


class Responses:
    def __init__(self):
        self.live = True
        self.values = []

    def send(self, kind, sequence=0, **value):
        self.values.append((kind, sequence, value))
        return self.live


class TinyReads(io.BytesIO):
    def read(self, size=-1):
        return super().read(min(size, 1) if size >= 0 else 1)


class Framing(unittest.TestCase):
    def test_multibyte_utf8_crosses_every_read_boundary(self):
        value = {'text': '\u03bb\U0001f30d'}
        budget = stream._Budget()
        reader = stream.FrameReader(TinyReads(stream.encode_frame('REQUEST', 1, value)), budget)
        kind, sequence, nonce, raw = reader.read()
        self.assertEqual((kind, sequence, nonce), ('REQUEST', 1, None))
        self.assertEqual(stream.load(raw.decode('utf-8', 'strict')), value)
        self.assertEqual(budget.bytes, len(raw))
        budget.release(len(raw))
        self.assertIsNone(reader.read())
        self.assertEqual((budget.frames, budget.bytes), (0, 0))

    def test_partial_header_and_body_never_parse_prefix(self):
        for raw in (b'AM249 REQ', b'AM249 REQUEST 1 3 -\n{}'):
            budget = stream._Budget()
            with self.assertRaises(ContractError):
                stream.FrameReader(io.BytesIO(raw), budget).read()
            self.assertEqual((budget.frames, budget.bytes), (0, 0))

    def test_noncanonical_header_and_oversize_refuse(self):
        for raw in (b'AM249 REQUEST 01 2 -\n{}', b'AM249 REQUEST 1 16777217 -\n',
                    b'AM249 UNKNOWN 1 0 -\n', b'AM249 CLOCK 1 0 -\n', b'x' * 257):
            with self.assertRaises(ContractError):
                stream.FrameReader(io.BytesIO(raw), stream._Budget()).read()

    def test_budget_counts_inflight_partial_and_queue(self):
        budget = stream._Budget()
        with mock.patch.object(stream, 'MAX_RETAINED', 8), mock.patch.object(stream, 'MAX_FRAMES', 2):
            budget.reserve(4)
            budget.reserve(4)
            with self.assertRaises(ContractError):
                budget.reserve(0)
            budget.release(4)
            with self.assertRaises(ContractError):
                budget.reserve(5)
            budget.reserve(4)
            self.assertEqual((budget.bytes, budget.frames), (8, 2))

    def test_cancel_control_has_capacity_even_when_source_fifo_is_full(self):
        budget = stream._Budget()
        budget.frames, budget.bytes = stream.MAX_FRAMES, stream.MAX_RETAINED
        raw = stream.encode_frame('CANCEL', 0, {'nonce': uid()})
        frame = stream.FrameReader(TinyReads(raw), budget).read()
        self.assertEqual(frame[0], 'CANCEL')
        self.assertEqual((budget.frames, budget.bytes), (stream.MAX_FRAMES, stream.MAX_RETAINED))

    def response_writer(self):
        writer = object.__new__(stream.ResponseWriter)
        import threading
        from collections import deque
        writer.instance_id = uid()
        writer.condition = threading.Condition()
        writer.queue = deque()
        writer.inflight = False
        writer.live = True
        writer.finishing = False
        writer.loss_pending = False
        return writer

    def test_response_pressure_discards_received_before_priority(self):
        writer = self.response_writer()
        writer.queue.extend([('RECEIVED', b'ack'), ('OBSERVED', b'one'),
                             ('CLOCK-REPLY', b'two'), ('OBSERVED', b'three')])
        self.assertTrue(writer.send('OBSERVED', 5, result={'verified': True}))
        self.assertTrue(writer.live)
        self.assertEqual(len(writer.queue), stream.MAX_RESPONSES)
        self.assertFalse(any(kind == 'RECEIVED' for kind, _ in writer.queue))
        self.assertFalse(writer.loss_pending)
        self.assertTrue(writer.queue[-1][1].startswith(b'AM249 OBSERVED 5 '))

    def test_priority_pressure_coalesces_bounded_loss_notice_without_disconnect(self):
        writer = self.response_writer()
        writer.queue.extend([('OBSERVED', b'priority')] * stream.MAX_RESPONSES)
        self.assertFalse(writer.send('OBSERVED', 7, result={'verified': True}))
        self.assertFalse(writer.send('OBSERVED', 8, result={'verified': True}))
        self.assertTrue(writer.live)
        self.assertTrue(writer.loss_pending)
        self.assertEqual(len(writer.queue), stream.MAX_RESPONSES)
        writer.queue.popleft()
        with writer.condition:
            writer._loss_notice()
        self.assertEqual(len(writer.queue), stream.MAX_RESPONSES)
        self.assertFalse(writer.loss_pending)
        notice = stream.load(writer.queue[-1][1].split(b'\n', 1)[1])
        self.assertEqual(notice['code'], 'response-pressure')
        self.assertNotIn('result', notice)
        writer.queue.clear()
        self.assertTrue(writer.send('OBSERVED', 9, result={'verified': True}))
        self.assertTrue(writer.live)

    def test_oversize_response_becomes_fixed_notice_and_later_delivery_stays_live(self):
        writer = self.response_writer()
        self.assertTrue(writer.send('OBSERVED', 1, result={'text': 'x' * stream.MAX_RESPONSE}))
        self.assertTrue(writer.live)
        raw = writer.queue[-1][1]
        self.assertLessEqual(len(raw), stream.MAX_RESPONSE)
        self.assertTrue(raw.startswith(b'AM249 OBSERVED 1 '))
        notice = stream.load(raw.split(b'\n', 1)[1])
        self.assertEqual(notice['code'], 'result-oversized')
        self.assertNotIn('result', notice)
        self.assertTrue(writer.send('OBSERVED', 2, result={'verified': True}))

    def test_actual_descriptor_failure_revokes_recipient(self):
        writer = self.response_writer()
        writer.stream = mock.Mock()
        writer.stream.fileno.return_value = 42
        with mock.patch.object(stream.os, 'set_blocking', side_effect=OSError('closed fd')):
            writer._write()
        self.assertFalse(writer.live)
        self.assertFalse(writer.send('OBSERVED', 1, result={'verified': True}))


class CoverageFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.base = self.root / 'private'
        self.base.mkdir(mode=0o700)
        self.bank = self.root / 'memory-bank'
        self.bank.mkdir(mode=0o700)
        self.session, self.instance, self.lifecycle = uid(), uid(), uid()
        self.binding = {'project_id': uid(), 'repo_root': str(self.root), 'bank_path': str(self.bank),
            'checkout_key': 'checkout', 'artifact_digest': 'a' * 64, 'installation_id': uuid.uuid4().hex,
            'initial_lifecycle': self.lifecycle}
        self.addCleanup(mock.patch.stopall)
        mock.patch.object(stream, '_base', return_value=self.base).start()
        mock.patch.object(turn_sources, 'source_barrier', side_effect=lambda *a, **kw: nullcontext()).start()
        self.binding_verifier = mock.patch.object(stream, '_verify_binding', return_value=nullcontext()).start()
        self.full_binding_verifier = mock.patch.object(stream, '_attest_binding', return_value=object()).start()
        self.coverage = stream.Coverage(str(self.root), self.session, self.instance, self.lifecycle, self.binding)
        self.addCleanup(self.coverage.close_live)
        self.responses = Responses()
        self.clocks = stream.ClockWindows(self.instance, lambda: self.responses.live, platform='darwin')

    def known(self, native=None):
        native = ['native-a'] if native is None else native
        return {'view_id': uid(), 'leaf_id': native[-1] if native else None,
                'native_ids': sorted(native), 'tool_ids': []}

    def request(self, action='ingest', known=None):
        known = self.known() if known is None else known
        return {'request': {'schema': 'agent-memory/omp-observer-request-v2', 'action': action,
            'session': self.session, 'lifecycle': self.lifecycle,
            'payload': {'entries': [], 'view': {'leaf_id': known['leaf_id'], 'source_kind': 'session-manager',
                'native_parents': [{'id': native, 'parent_id': None} for native in known['native_ids']]}}},
            'event_ordinal': 1, 'identity': known}

    def hint(self, leaf=None, *, ordinal=1):
        known = {'kind': 'hint', 'session': self.session, 'header_id': self.session,
                 'cwd': str(self.root), 'leaf_id': leaf}
        return {'request': {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'hint',
            'session': self.session, 'lifecycle': self.lifecycle,
            'payload': {key: value for key, value in known.items() if key != 'kind'}},
            'event_ordinal': ordinal, 'identity': known}

    def other_coverage(self, session=None):
        value = stream.Coverage(str(self.root), session or self.session, uid(), self.lifecycle, self.binding)
        self.addCleanup(value.close_live)
        return value

    def execute(self, observer, value=None, sequence=1, raw=None, *, coverage=None):
        current = self.coverage if coverage is None else coverage
        clocks = self.clocks if coverage is None else stream.ClockWindows(current.instance, lambda: self.responses.live)
        executor = stream.FiniteExecutor(current, self.responses, clocks, observer=observer)
        executor.execute('REQUEST', sequence, None,
                         bytearray(stream.turn_sources.encoded(self.request() if value is None else value)) if raw is None else raw)

    def final(self, count=1, refused=0, ordinal=1, transition=None):
        return {'last_attempt_seq': count, 'refused_count': refused, 'last_event_ordinal': ordinal,
                'transition': transition}

    def outcome(self, sequence=1):
        return turn_sources.read(self.coverage.directory / 'outcomes' / f'{sequence}.json', stream.MAX_METADATA)

    def pending(self):
        return stream.inventory(str(self.root), self.base)['pending']

    @staticmethod
    def verified(repo, request, *, on_started, **kwargs):
        on_started()
        return {'verified': True, 'status': 'pending'}

    def test_verified_pending_is_terminal_transport_not_learning_completion(self):
        self.execute(self.verified)
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertTrue(self.coverage.close(self.final()))
        inventory = stream.inventory(str(self.root), self.base)
        self.assertEqual(inventory['pending'], [])
        self.assertTrue(inventory['streams'][0]['closed'])
        self.assertNotIn('completed', str(inventory))

    def test_busy_before_started_retries_same_attempt_only(self):
        calls = []
        def observe(repo, request, *, on_started, **kwargs):
            calls.append(request)
            if len(calls) == 1:
                raise turn_sources.SourceTransportBusy('public diagnostic')
            on_started()
            return {'verified': True}
        with mock.patch.object(stream.time, 'sleep') as sleep:
            self.execute(observe)
        self.assertEqual(len(calls), 2)
        self.assertIs(calls[0], calls[1])
        self.assertEqual(sleep.call_args.args[0], stream.BUSY_BACKOFF)
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertEqual(self.coverage.last_sequence, 1)

    def test_busy_after_started_never_replays(self):
        calls = []
        def observe(repo, request, *, on_started, **kwargs):
            calls.append(1)
            on_started()
            raise turn_sources.SourceTransportBusy('nested lock busy')
        self.execute(observe)
        self.assertEqual(calls, [1])
        self.assertEqual(self.outcome()['code'], 'unknown-after-start')
        self.assertFalse(self.coverage.close(self.final()))
        self.assertTrue(any(item['code'] == 'open-stream' for item in self.pending()))

    def test_busy_exhaustion_is_open_not_durable_refusal(self):
        with mock.patch.object(stream.time, 'monotonic', side_effect=[0.0, 30.0]):
            self.execute(mock.Mock(side_effect=turn_sources.SourceTransportBusy('busy')))
        self.assertEqual(self.outcome()['outcome'], 'unknown')
        self.assertEqual(self.outcome()['code'], 'busy-exhausted')
        self.assertFalse(self.coverage.close(self.final()))

    def test_metadata_busy_after_verified_never_replays_observer(self):
        observe = mock.Mock(side_effect=self.verified)
        original = self.coverage.outcome
        writes = []
        def outcome(*args):
            writes.append(args)
            if len(writes) == 1:
                raise turn_sources.SourceTransportBusy('metadata busy')
            return original(*args)
        with mock.patch.object(self.coverage, 'outcome', side_effect=outcome), mock.patch.object(stream.time, 'sleep'):
            self.execute(observe)
        self.assertEqual(observe.call_count, 1)
        self.assertEqual(len(writes), 2)
        self.assertEqual(self.outcome()['outcome'], 'verified')

    def test_arbitrary_exception_is_not_verified_receipt_or_refusal(self):
        self.execute(mock.Mock(side_effect=RuntimeError('sensitive body must not persist')))
        value = self.outcome()
        self.assertEqual((value['outcome'], value['code']), ('unknown', 'unknown-outcome'))
        self.assertNotIn('sensitive', str(value))
        self.assertFalse(any('result' in value for _, _, value in self.responses.values))

    def test_fixed_prestart_refusal_closes_but_blocks_drain(self):
        self.execute(mock.Mock(side_effect=ContractError('arbitrary diagnostic not retained')))
        self.assertEqual(self.outcome()['outcome'], 'durable-refusal')
        self.assertTrue(self.coverage.close(self.final()))
        inventory = stream.inventory(str(self.root), self.base)
        self.assertTrue(inventory['streams'][0]['closed'])
        with self.assertRaises(ContractError):
            stream.validate_drain(str(self.root), inventory, self.base)

    def test_removed_marker_refuses_open_and_later_metadata_producers(self):
        instance = uid()
        with mock.patch.object(stream, '_verify_binding', side_effect=ContractError('activation required')):
            with self.assertRaises(ContractError):
                stream.Coverage(str(self.root), self.session, instance, self.lifecycle, self.binding)
            with self.assertRaises(ContractError):
                self.coverage.attempt(1, 'ingest', 1, 'accepted', None, self.known())
        self.assertFalse(stream._directory(self.base, self.session, instance).exists())
        self.assertFalse((self.coverage.directory / 'attempts').exists())

    def test_verified_without_on_started_is_not_accepted(self):
        self.execute(mock.Mock(return_value={'verified': True}))
        self.assertEqual(self.outcome()['outcome'], 'unknown')
        self.assertFalse(self.coverage.close(self.final()))

    def test_bad_schema_session_and_identity_are_counted_without_dropping_later_source(self):
        observe = mock.Mock(side_effect=self.verified)
        for sequence, mutate in enumerate(('schema', 'session', 'identity'), 1):
            value = self.request()
            if mutate == 'schema':
                value['request']['schema'] = 'agent-memory/omp-observer-request-v1'
            elif mutate == 'session':
                value['request']['session'] = uid()
            else:
                value['identity']['native_ids'] = ['other']
                value['identity']['leaf_id'] = 'other'
            self.execute(observe, value, sequence=sequence)
            self.assertEqual(self.outcome(sequence)['code'], 'invalid-request')
            self.assertEqual(self.outcome(sequence)['outcome'], 'durable-refusal')
        observe.assert_not_called()
        self.execute(observe, sequence=4)
        self.assertEqual(observe.call_count, 1)
        self.assertEqual(self.outcome(4)['outcome'], 'verified')
        self.assertEqual(self.coverage.refused_count, 0)
        self.assertTrue(self.coverage.close(self.final(count=4)))
        self.assertTrue(any(item['code'] == 'invalid-request' for item in self.pending()))

    def test_invalid_utf8_and_partial_json_are_refused_without_losing_following_frame(self):
        observe = mock.Mock(side_effect=self.verified)
        cases = (bytearray(b'{"text":"\xff"}'), bytearray('{"text":"ok"}'.encode('utf-16')), bytearray(b'{'))
        for sequence, raw in enumerate(cases, 1):
            self.execute(observe, sequence=sequence, raw=raw)
            self.assertEqual(self.outcome(sequence)['code'], 'invalid-request')
        observe.assert_not_called()
        self.execute(observe, sequence=4)
        self.assertEqual(self.outcome(4)['outcome'], 'verified')
        self.assertTrue(self.coverage.close(self.final(count=4)))

    def test_matching_preexisting_invalid_attempt_is_not_a_live_retry_witness(self):
        path = self.coverage.directory / 'attempts/1.json'
        value = {'schema': 'agent-memory/omp-stream-attempt-v1', 'open_sha256': self.coverage.open_digest,
            'instance_id': self.instance, 'sequence': 1, 'action': 'hint', 'event_ordinal': 0,
            'disposition': 'refused', 'reason': 'invalid-request', 'identity': {'kind': 'unresolvable'},
            'source_kind': None, 'frame_kind': 'REQUEST'}
        stream._retain(path, value, stream.MAX_IDENTITY)
        original = path.read_bytes()
        with self.assertRaises(ContractError):
            self.coverage.refuse_invalid(1, 'REQUEST', 'hint', 0)
        self.assertEqual(path.read_bytes(), original)
        self.assertEqual((self.coverage.last_sequence, self.coverage.refused_count), (0, 0))
        self.assertIsNone(self.coverage._invalid_attempt_retry)
        path.unlink()
        control = self.coverage.control
        @contextmanager
        def busy_after_precheck():
            with control():
                yield
            raise turn_sources.SourceTransportBusy('Synthetic precheck exit deferral')
        with mock.patch.object(self.coverage, 'control', side_effect=busy_after_precheck):
            with self.assertRaises(turn_sources.SourceTransportBusy):
                self.coverage.refuse_invalid(1, 'REQUEST', 'hint', 0)
        stream._retain(path, value, stream.MAX_IDENTITY)
        with self.assertRaises(ContractError) as refused:
            self.coverage.refuse_invalid(1, 'REQUEST', 'hint', 0)
        self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        self.assertEqual(path.read_bytes(), original)
        self.assertEqual((self.coverage.last_sequence, self.coverage.refused_count), (0, 0))

    def test_partial_immutable_attempt_is_never_replaced_by_refusal(self):
        path = self.coverage.directory / 'attempts/1.json'
        turn_sources.private_mkdir(path.parent)
        path.write_bytes(b'{')
        path.chmod(0o600)
        with self.assertRaises(ValueError):
            self.execute(mock.Mock())
        self.assertEqual(turn_sources.read_bytes(path, stream.MAX_METADATA), b'{')
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        self.assertFalse(any(kind == 'REFUSED' for kind, _, _ in self.responses.values))

    def test_real_io_failure_does_not_manufacture_invalid_request_refusal(self):
        retain = stream._retain
        def fail_after_write(path, value, maximum=stream.MAX_METADATA):
            result = retain(path, value, maximum)
            if path.parent.name == 'attempts':
                raise OSError('simulated persistence failure')
            return result
        with mock.patch.object(stream, '_retain', side_effect=fail_after_write):
            with self.assertRaises(OSError):
                self.execute(mock.Mock())
        attempt = turn_sources.read(self.coverage.directory / 'attempts/1.json', stream.MAX_METADATA)
        self.assertEqual(attempt['disposition'], 'accepted')
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        self.assertFalse(any(kind == 'REFUSED' for kind, _, _ in self.responses.values))

    def test_source_kind_validation_failure_accounts_slot_and_continues(self):
        malformed = self.request()
        malformed['request']['payload']['view']['source_kind'] = 'unrecognized'
        self.execute(mock.Mock(), malformed)
        self.assertEqual(self.outcome()['code'], 'invalid-request')
        self.execute(self.verified, sequence=2)
        self.assertEqual(self.outcome(2)['outcome'], 'verified')

    def test_unknown_identity_breaks_delta_chain(self):
        first = self.known()
        self.coverage.attempt(1, 'ingest', 1, 'accepted', None, first)
        self.coverage.attempt(2, 'ingest', 2, 'refused', 'capture-refused', None)
        self.coverage.attempt(3, 'ingest', 3, 'accepted', None, self.known(['native-a', 'native-b']))
        value = turn_sources.read(self.coverage.directory / 'attempts/3.json', stream.MAX_METADATA)
        self.assertEqual(value['identity']['kind'], 'baseline')
        self.assertNotIn('native_added', value['identity'])

    def test_lost_refusal_notice_cannot_match_final(self):
        self.execute(self.verified)
        with self.assertRaises(ContractError):
            self.coverage.close(self.final(count=2, refused=1))
        self.assertFalse((self.coverage.directory / 'closed.json').exists())

    def test_retirement_fence_prevents_old_start(self):
        digest = self.coverage.attempt(1, 'ingest', 1, 'accepted', None, self.known())
        self.coverage.close_live()
        fences = stream.retire_for_recovery(str(self.root), self.session)
        self.assertEqual(len(fences), 1)
        with self.assertRaises(ContractError):
            self.coverage.started(1, digest)
        self.assertFalse((self.coverage.directory / 'started/1.json').exists())
        self.assertEqual(stream.retire_for_recovery(str(self.root), self.session), fences)

    def test_retirement_skips_live_worker_even_while_stream_metadata_is_locked(self):
        with turn_sources.private_lock(self.coverage.lock_path):
            self.assertEqual(stream.retire_for_recovery(str(self.root), self.session), [])
        self.assertFalse((self.coverage.directory / 'retirement.json').exists())
        self.execute(self.verified)
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertTrue(self.coverage.close(self.final()))
        self.assertEqual(stream.retire_for_recovery(str(self.root), self.session), [])
        self.coverage.close_live()
        self.assertEqual(len(stream.retire_for_recovery(str(self.root), self.session)), 1)

    def test_exact_recovery_does_not_clear_open_unknown_tail(self):
        self.execute(mock.Mock(side_effect=ContractError('refused')))
        self.coverage.close_live()
        fences = stream.retire_for_recovery(str(self.root), self.session)
        refs = [{'session': self.session, 'lifecycle': self.lifecycle,
                 'observation_id': uid(), 'source_sha256': 'b' * 64}]
        recovered = stream.resolve_recovery(str(self.root), fences, view_id=uid(),
            native_ids=['native-a'], tool_ids=[], payload_native_ids=['native-a'], source_references=refs)
        self.assertEqual(len(recovered), 1)
        self.assertEqual([item['code'] for item in self.pending()], ['open-stream'])

    def test_pre_id_refusal_cannot_be_resolved_by_counts(self):
        digest = self.coverage.attempt(1, 'ingest', 1, 'refused', 'capture-refused', None)
        self.coverage.outcome(1, digest, 'durable-refusal', 'capture-refused')
        self.coverage.close_live()
        fences = stream.retire_for_recovery(str(self.root), self.session)
        recovered = stream.resolve_recovery(str(self.root), fences, view_id=uid(), native_ids=['native-a'],
            tool_ids=[], payload_native_ids=['native-a'], source_references=[], complete=True)
        self.assertEqual(recovered, [])
        self.assertTrue(any(item['code'] == 'unresolvable-identity' for item in self.pending()))

    def test_transition_binds_last_old_view_and_remains_pending_after_close(self):
        value = self.request()
        self.execute(self.verified, value)
        transition = {'transition_id': uid(), 'target_session': uid(), 'target_instance': uid(), 'last_view': value['identity'],
                      'reason': 'transition-final-view-unavailable'}
        self.assertTrue(self.coverage.close(self.final(transition=transition)))
        self.assertEqual([item['code'] for item in self.pending()], ['transition-final-view-unavailable'])
        self.assertTrue(stream.inventory(str(self.root), self.base)['streams'][0]['closed'])

    def test_failed_retired_view_does_not_replace_precommit_transition_anchor(self):
        original = self.request()
        self.execute(self.verified, original)
        recovery = self.request(known=self.known(['native-a', 'native-b']))
        recovery['request']['payload']['view']['source_kind'] = 'retired-file'
        recovery['event_ordinal'] = 2
        self.execute(self.verified, recovery, sequence=2)
        transition = {'transition_id': uid(), 'target_session': uid(), 'target_instance': uid(), 'last_view': original['identity'],
                      'reason': 'transition-final-view-unavailable'}
        self.assertTrue(self.coverage.close(self.final(count=2, ordinal=2, transition=transition)))
        self.assertEqual([item['code'] for item in self.pending()], ['transition-final-view-unavailable'])

    def test_null_leaf_hint_is_fast_neutral_metadata_and_never_observes(self):
        observe = mock.Mock(side_effect=AssertionError('hint reached source executor'))
        initial = self.full_binding_verifier.call_count
        self.execute(observe, self.hint())
        observe.assert_not_called()
        self.assertEqual(self.full_binding_verifier.call_count, initial)
        self.assertEqual(self.outcome()['code'], 'hint-recorded')
        self.assertFalse((self.coverage.directory / 'started').exists())
        self.assertFalse((self.base / 'omp-source-v1').exists())
        self.assertTrue(self.coverage.close(self.final()))
        with mock.patch('agent_memory.omp_transport.native_disposition_ids') as source_ids:
            self.assertEqual(self.pending(), [])
        source_ids.assert_not_called()

    def test_failed_hint_identity_copy_is_fast_counted_refusal_not_neutral_null_leaf(self):
        observe = mock.Mock()
        initial = self.full_binding_verifier.call_count
        executor = stream.FiniteExecutor(self.coverage, self.responses, self.clocks, observer=observe)
        value = {'action': 'hint', 'event_ordinal': 1, 'reason': 'capture-refused', 'identity': None}
        executor.execute('REFUSED-ATTEMPT', 1, None, bytearray(turn_sources.encoded(value)))
        observe.assert_not_called()
        self.assertEqual(self.full_binding_verifier.call_count, initial)
        self.assertEqual(self.coverage.refused_count, 1)
        self.assertEqual(self.outcome()['outcome'], 'durable-refusal')
        self.assertTrue(self.coverage.close(self.final(refused=1)))
        self.assertTrue(any(item['code'] == 'capture-refused' for item in self.pending()))

    def test_hint_cannot_carry_source_fields_or_create_human_authority(self):
        value = self.hint()
        value['request']['payload']['role'] = 'user'
        observe = mock.Mock()
        self.execute(observe, value)
        observe.assert_not_called()
        self.assertEqual(self.outcome()['code'], 'invalid-request')
        self.assertFalse((self.base / 'omp-source-v1').exists())

    def test_hints_preserve_full_delta_baseline_and_transition_anchor(self):
        first = self.request()
        self.execute(self.verified, first)
        before = self.coverage.previous_identity
        observe = mock.Mock()
        self.execute(observe, self.hint('native-a'), sequence=2)
        self.assertEqual(self.coverage.previous_identity, before)
        self.assertEqual(self.coverage.last_checked_identity, first['identity'])
        second = self.request(known=self.known(['native-a', 'native-b']))
        self.execute(self.verified, second, sequence=3)
        recorded = turn_sources.read(self.coverage.directory / 'attempts/3.json', stream.MAX_METADATA)
        self.assertEqual(recorded['identity']['kind'], 'delta')
        self.assertEqual(recorded['identity']['native_added'], ['native-b'])
        self.execute(observe, self.hint('native-b'), sequence=4)
        transition = {'transition_id': uid(), 'target_session': uid(), 'target_instance': uid(),
            'last_view': second['identity'], 'reason': 'transition-final-view-unavailable'}
        self.assertTrue(self.coverage.close(self.final(count=4, transition=transition)))
        with mock.patch('agent_memory.omp_transport.native_disposition_ids') as source_ids:
            self.assertEqual([item['code'] for item in self.pending()], ['transition-final-view-unavailable'])
        source_ids.assert_not_called()
        observe.assert_not_called()

    def test_unknown_hint_leaf_resolves_from_fresh_exact_native_disposition_join_once(self):
        observe = mock.Mock()
        initial = self.full_binding_verifier.call_count
        for sequence in range(1, 9):
            self.execute(observe, self.hint('native-a', ordinal=sequence), sequence=sequence)
        self.assertEqual(self.full_binding_verifier.call_count, initial)
        self.assertTrue(self.coverage.close(self.final(count=8, ordinal=8)))
        original = turn_sources.read_bytes(self.coverage.directory / 'attempts/1.json', stream.MAX_METADATA)
        with mock.patch('agent_memory.omp_transport.native_disposition_ids', return_value=set()) as source_ids:
            before = stream.inventory(str(self.root), self.base)
            self.assertEqual(source_ids.call_count, 1)
            self.assertEqual([item['code'] for item in before['pending']], ['hint-uncovered'] * 8)
            source_ids.return_value = {'native-a'}
            after = stream.inventory(str(self.root), self.base)
            self.assertEqual(source_ids.call_count, 2)
            self.assertEqual(after['pending'], [])
            self.assertNotEqual(before['inventory_sha256'], after['inventory_sha256'])
        self.assertEqual(turn_sources.read_bytes(self.coverage.directory / 'attempts/1.json', stream.MAX_METADATA), original)
        observe.assert_not_called()

    def test_earlier_instance_verified_full_view_covers_later_instance_hint(self):
        self.execute(self.verified)
        self.assertTrue(self.coverage.close(self.final()))
        later = self.other_coverage()
        observe = mock.Mock()
        self.execute(observe, self.hint('native-a'), coverage=later)
        self.assertTrue(later.close(self.final()))
        with mock.patch('agent_memory.omp_transport.native_disposition_ids') as source_ids:
            self.assertEqual(self.pending(), [])
        source_ids.assert_not_called()
        observe.assert_not_called()

    def test_later_instance_full_view_resolves_earlier_hint(self):
        self.execute(mock.Mock(), self.hint('native-a'))
        self.assertTrue(self.coverage.close(self.final()))
        with mock.patch('agent_memory.omp_transport.native_disposition_ids', return_value=set()):
            self.assertEqual([item['code'] for item in self.pending()], ['hint-uncovered'])
        later = self.other_coverage()
        self.execute(self.verified, self.request(), coverage=later)
        self.assertTrue(later.close(self.final()))
        with mock.patch('agent_memory.omp_transport.native_disposition_ids') as source_ids:
            self.assertEqual(self.pending(), [])
        source_ids.assert_not_called()

    def test_refused_and_unknown_full_frames_do_not_cover_hint_merely_by_native_ids(self):
        self.execute(mock.Mock(), self.hint('native-a'))
        self.assertTrue(self.coverage.close(self.final()))
        for started in (False, True):
            later = self.other_coverage()
            def observe(repo, request, *, on_started, **kwargs):
                if started:
                    on_started()
                    raise RuntimeError('unknown after write boundary')
                raise ContractError('prestart refusal')
            self.execute(observe, self.request(), coverage=later)
        with mock.patch('agent_memory.omp_transport.native_disposition_ids', return_value=set()) as source_ids:
            self.assertTrue(any(item['code'] == 'hint-uncovered' for item in self.pending()))
            self.assertEqual(source_ids.call_count, 1)

    def test_another_session_full_view_does_not_cover_hint_and_source_errors_refuse(self):
        self.execute(mock.Mock(), self.hint('native-a'))
        other = self.other_coverage(session=uid())
        value = self.request()
        value['request']['session'] = other.session
        self.execute(self.verified, value, coverage=other)
        with mock.patch('agent_memory.omp_transport.native_disposition_ids', return_value=set()):
            self.assertTrue(any(item['code'] == 'hint-uncovered' for item in self.pending()))
        with mock.patch('agent_memory.omp_transport.native_disposition_ids', side_effect=ContractError('unverifiable source state')):
            with self.assertRaises(ContractError):
                self.pending()

    def test_same_session_different_instance_transition_is_valid_but_self_target_refuses(self):
        value = self.request()
        self.execute(self.verified, value)
        transition = {'transition_id': uid(), 'target_session': self.session, 'target_instance': self.instance,
            'last_view': value['identity'], 'reason': 'transition-final-view-unavailable'}
        with self.assertRaises(ContractError):
            self.coverage.close(self.final(transition=transition))
        transition['target_instance'] = uid()
        self.assertTrue(self.coverage.close(self.final(transition=transition)))
        self.assertEqual([item['code'] for item in self.pending()], ['transition-final-view-unavailable'])

    def test_empty_refs_cannot_resolve_learning_attempt_or_transition(self):
        value = self.request()
        self.execute(self.verified, value)
        transition = {'transition_id': uid(), 'target_session': uid(), 'target_instance': uid(), 'last_view': value['identity'],
                      'reason': 'transition-final-view-unavailable'}
        self.coverage.close(self.final(transition=transition))
        self.coverage.close_live()
        fences = stream.retire_for_recovery(str(self.root), self.session)
        self.assertEqual(stream.resolve_recovery(str(self.root), fences, view_id=uid(),
            native_ids=['native-a'], tool_ids=[], payload_native_ids=['native-a'],
            source_references=[], complete=True), [])
        self.assertEqual([item['code'] for item in self.pending()], ['transition-final-view-unavailable'])

    def test_internal_nonlearning_evidence_resolves_only_anchored_closed_transition(self):
        value = self.request()
        self.execute(self.verified, value)
        transition = {'transition_id': uid(), 'target_session': uid(), 'target_instance': uid(), 'last_view': value['identity'],
                      'reason': 'transition-final-view-unavailable'}
        self.coverage.close(self.final(transition=transition))
        self.coverage.close_live()
        fences = stream.retire_for_recovery(str(self.root), self.session)
        view_id = uid()
        evidence = stream._nonlearning_tail_evidence(self.session, view_id, ['native-a'])
        self.assertEqual(stream.resolve_recovery(str(self.root), fences, view_id=view_id,
            native_ids=['native-a'], tool_ids=[], payload_native_ids=['native-a'], source_references=[],
            complete=True, leaf_id='native-a', nonlearning_evidence=evidence), [])
        self.assertEqual(self.pending(), [])
        self.assertFalse((self.coverage.directory / 'resolutions').exists())
        with self.assertRaises(TypeError):
            evidence.__reduce__()
        with self.assertRaises(ContractError):
            stream._nonlearning_tail_evidence(self.session, uid(), [])
        with self.assertRaises(ContractError):
            stream.resolve_recovery(str(self.root), fences, view_id=view_id, native_ids=['native-a'],
                tool_ids=[], payload_native_ids=['native-a'], source_references=[], complete=True,
                nonlearning_evidence={'trusted': True})

    def test_inventory_refuses_unknown_private_state(self):
        turn_sources.exclusive(self.coverage.directory / 'outcomes-bogus.json', {'x': 1}, 4096)
        with self.assertRaises(ContractError):
            stream.inventory(str(self.root), self.base)

    def test_inventory_accepts_symlinked_repository_alias(self):
        alias = self.root.parent / (self.root.name + '-alias-' + uid())
        alias.symlink_to(self.root, target_is_directory=True)
        self.addCleanup(alias.unlink)
        value = stream.inventory(str(alias), self.base)
        self.assertEqual(value['streams'][0]['session'], self.session)

    def test_unpublished_instance_stays_pending_while_later_valid_source_closes(self):
        abandoned = stream._directory(self.base, self.session, uid())
        turn_sources.private_mkdir(abandoned)
        with turn_sources.private_lock(abandoned / 'live.lock'):
            pass
        value = stream.inventory(str(self.root), self.base)
        row = next(row for row in value['streams'] if row['instance_id'] == abandoned.name)
        self.assertIsNone(row['session'])
        self.assertEqual(row['session_sha256'], abandoned.parent.name)
        self.assertEqual(row['pending'], [{'code': 'startup-incomplete'}])
        before = value['inventory_sha256']
        def observe(repo, request, *, on_started, **kwargs):
            self.assertEqual(stream.retire_for_recovery(repo, request['session'],
                exclude_instance=on_started.instance_id), [])
            on_started()
            return {'verified': True}
        self.execute(observe)
        self.assertTrue(self.coverage.close(self.final()))
        after = stream.inventory(str(self.root), self.base)
        self.assertNotEqual(before, after['inventory_sha256'])
        row = next(row for row in after['streams'] if row['instance_id'] == abandoned.name)
        self.assertEqual(row['pending'], [{'code': 'startup-incomplete'}])
        with self.assertRaises(ContractError):
            stream.validate_drain(str(self.root), after, self.base)
        with self.assertRaises(ContractError):
            stream.inventory(str(self.root), self.base / 'offline')

    def test_partial_open_is_pinned_as_unpublished_without_trusting_its_binding(self):
        abandoned = stream._directory(self.base, self.session, uid())
        turn_sources.private_mkdir(abandoned)
        path = abandoned / 'open.json'
        path.write_bytes(b'{"schema":"agent-memory/omp-stream-open-v1","project_id":')
        path.chmod(0o600)
        value = stream.inventory(str(self.root), self.base)
        row = next(row for row in value['streams'] if row['instance_id'] == abandoned.name)
        self.assertIsNone(row['session'])
        self.assertIsNone(row['open_sha256'])
        self.assertEqual(row['pending'], [{'code': 'startup-incomplete'}])
        self.assertEqual(stream.retire_for_recovery(str(self.root), self.session), [])
        self.assertEqual(turn_sources.read_bytes(path, stream.MAX_METADATA),
                         b'{"schema":"agent-memory/omp-stream-open-v1","project_id":')

    def test_session_creation_contention_is_typed_busy_without_publishing_instance(self):
        instance = uid()
        path = self.base / 'omp-stream-session-locks' / (self.coverage.directory.parent.name + '.lock')
        with turn_sources.private_lock(path):
            with self.assertRaises(turn_sources.SourceTransportBusy):
                stream.Coverage(str(self.root), self.session, instance, self.lifecycle, self.binding)
        self.assertFalse(stream._directory(self.base, self.session, instance).exists())

    def test_missing_open_with_source_records_is_not_an_unpublished_startup(self):
        abandoned = stream._directory(self.base, self.session, uid())
        turn_sources.private_mkdir(abandoned / 'attempts')
        turn_sources.exclusive(abandoned / 'attempts/1.json', {'unknown': True}, 4096)
        with self.assertRaises(ContractError):
            stream.inventory(str(self.root), self.base)

    def test_empty_session_or_orphan_lock_qualification_survives_later_same_session_start(self):
        for initial in ('empty-session-directory', 'orphan-creation-lock'):
            session = uid()
            directory = stream._directory(self.base, session, uid()).parent
            if initial == 'empty-session-directory':
                turn_sources.private_mkdir(directory)
            else:
                with turn_sources.private_lock(self.base / 'omp-stream-session-locks' / (directory.name + '.lock')):
                    pass
            before = stream.inventory(str(self.root), self.base)
            self.assertTrue(any(row.get('session_sha256') == directory.name
                and row['pending'] == [{'code': 'startup-incomplete'}] for row in before['streams']))
            instance = uid()
            value = self.request()
            value['request']['session'] = session
            data = stream.encode_frame('REQUEST', 1, value) + stream.encode_frame('FINAL', 2, self.final())
            with mock.patch.object(stream, 'preload'), mock.patch('agent_memory.omp_transport.observe', self.verified):
                result = stream.run(str(self.root), session, instance, self.lifecycle,
                    startup=lambda *args: self.binding, input=io.BytesIO(data), output=io.BytesIO())
            self.assertEqual(result, 0)
            marker = turn_sources.read(directory / 'startup-incomplete.json', 4096)
            self.assertEqual(marker, {'schema': 'agent-memory/omp-session-startup-incomplete-v1',
                                      'session_sha256': directory.name})
            after = stream.inventory(str(self.root), self.base)
            healthy = next(row for row in after['streams'] if row['instance_id'] == instance)
            self.assertTrue(healthy['closed'])
            self.assertEqual(healthy['pending'], [])
            self.assertTrue(any(row.get('session_sha256') == directory.name
                and row['instance_id'] is None and row['pending'] == [{'code': 'startup-incomplete'}]
                for row in after['streams']))
            self.assertTrue(any(item.get('session_sha256') == directory.name for item in after['pending']))
            with self.assertRaises(ContractError):
                stream.validate_drain(str(self.root), after, self.base)

    def test_startup_accepts_real_32_hex_installation_id_and_refuses_uuid_form(self):
        self.assertIs(stream._binding(self.binding, str(self.root), self.session,
            self.instance, self.lifecycle), self.binding)
        invalid = {**self.binding, 'installation_id': uid()}
        with self.assertRaises(ContractError):
            stream._binding(invalid, str(self.root), self.session, self.instance, self.lifecycle)
        with self.assertRaises(ContractError):
            stream._open_value({**self.coverage.open, 'installation_id': invalid['installation_id']},
                               self.coverage.directory)
        instance = uid()
        with mock.patch.object(stream, 'preload'):
            result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                startup=lambda *args: invalid, input=io.BytesIO(), output=io.BytesIO())
        self.assertEqual(result, 1)
        self.assertFalse(stream._directory(self.base, self.session, instance).exists())

    def test_inventory_refuses_orphan_outcome(self):
        path = self.coverage.directory / 'outcomes'
        turn_sources.private_mkdir(path)
        turn_sources.exclusive(path / '1.json', {'unbound': True}, 4096)
        with self.assertRaises(ContractError):
            stream.inventory(str(self.root), self.base)

    def test_malformed_request_then_valid_source_and_sender_refusal_all_close_accounted(self):
        valid = self.request()
        refused = {'action': 'ingest', 'event_ordinal': 1, 'reason': 'capture-refused', 'identity': self.known()}
        invalid = b'AM249 REQUEST 1 1 -\n{'
        data = (invalid + stream.encode_frame('REQUEST', 2, valid)
            + stream.encode_frame('REFUSED-ATTEMPT', 3, refused)
            + stream.encode_frame('FINAL', 4, self.final(count=3, refused=1)))
        instance = uid()
        observe = mock.Mock(side_effect=self.verified)
        initial = self.full_binding_verifier.call_count
        with mock.patch.object(stream, 'preload'), mock.patch('agent_memory.omp_transport.observe', observe):
            result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                startup=lambda *args: self.binding, input=io.BytesIO(data), output=io.BytesIO())
        directory = stream._directory(self.base, self.session, instance)
        self.assertEqual(result, 0)
        self.assertEqual(observe.call_count, 1)
        self.assertEqual(self.full_binding_verifier.call_count, initial + 5)  # OPEN + 3 source frames + FINAL.
        first = turn_sources.read(directory / 'attempts/1.json', stream.MAX_METADATA)
        third = turn_sources.read(directory / 'attempts/3.json', stream.MAX_METADATA)
        self.assertEqual((first['disposition'], first['frame_kind']), ('refused', 'REQUEST'))
        self.assertEqual((third['disposition'], third['frame_kind']), ('refused', 'REFUSED-ATTEMPT'))
        self.assertEqual(turn_sources.read(directory / 'outcomes/2.json', stream.MAX_METADATA)['outcome'], 'verified')
        self.assertEqual(turn_sources.read(directory / 'closed.json', 4096)['refused_count'], 1)
        rows = stream.inventory(str(self.root), self.base)['streams']
        pending = next(row['pending'] for row in rows if row['instance_id'] == instance)
        self.assertTrue(any(item['code'] == 'invalid-request' for item in pending))
        self.assertTrue(any(item['code'] == 'capture-refused' for item in pending))
        with turn_sources.private_lock(directory / 'live.lock'):
            pass  # EOF/drain exit released the lifetime owner lock.


    def test_run_drains_accepted_frames_only_after_valid_final_and_eof(self):
        value = self.request()
        data = stream.encode_frame('REQUEST', 1, value) + stream.encode_frame('FINAL', 2, self.final())
        instance = uid()
        with mock.patch.object(stream, 'preload'), mock.patch('agent_memory.omp_transport.observe', self.verified):
            result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                startup=lambda *args: self.binding, input=io.BytesIO(data), output=io.BytesIO())
        self.assertEqual(result, 0)
        self.assertTrue((stream._directory(self.base, self.session, instance) / 'closed.json').exists())

    def test_startup_typed_busy_retries_before_open_and_ready(self):
        events = []
        instance = uid()
        def startup(*args):
            events.append('startup')
            if events.count('startup') == 1:
                raise turn_sources.SourceTransportBusy('startup busy')
            self.assertFalse((stream._directory(self.base, self.session, instance) / 'open.json').exists())
            return self.binding
        data = stream.encode_frame('FINAL', 1, self.final(count=0, ordinal=0))
        with mock.patch.object(stream, 'preload', side_effect=lambda *args: events.append('preload')), mock.patch.object(stream.time, 'sleep') as sleep:
            result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                startup=startup, input=io.BytesIO(data), output=io.BytesIO())
        self.assertEqual(result, 0)
        self.assertEqual(events, ['preload', 'startup', 'startup'])
        self.assertEqual(sleep.call_args.args[0], stream.BUSY_BACKOFF)

    def test_startup_busy_exhaustion_is_pending_without_open_or_close(self):
        instance = uid()
        responses = Responses()
        responses.finish = mock.Mock()
        with mock.patch.object(stream, 'preload'), mock.patch.object(stream, 'ResponseWriter', return_value=responses), mock.patch.object(stream.time, 'monotonic', side_effect=[0.0, 30.0]):
            result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                startup=mock.Mock(side_effect=turn_sources.SourceTransportBusy('startup busy')),
                input=io.BytesIO(), output=io.BytesIO())
        self.assertEqual(result, 1)
        self.assertEqual(responses.values, [('REFUSED', 0, {'code': 'busy-exhausted', 'outcome': 'unknown', 'status': 'pending'})])
        self.assertFalse(stream._directory(self.base, self.session, instance).exists())

    def test_run_eof_partial_or_after_final_cannot_close(self):
        value = self.request()
        request = stream.encode_frame('REQUEST', 1, value)
        final = stream.encode_frame('FINAL', 2, self.final())
        for data in (request, request + final[:-1], request + final + request,
                     stream.encode_frame('REQUEST', 2, value) + final):
            instance = uid()
            with mock.patch.object(stream, 'preload'), mock.patch('agent_memory.omp_transport.observe', self.verified):
                result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                    startup=lambda *args: self.binding, input=io.BytesIO(data), output=io.BytesIO())
            self.assertEqual(result, 1)
            self.assertFalse((stream._directory(self.base, self.session, instance) / 'closed.json').exists())

    def test_broken_output_does_not_prevent_accepted_source_drain(self):
        class BrokenOutput(io.BytesIO):
            def write(self, raw):
                raise BrokenPipeError()
        data = stream.encode_frame('REQUEST', 1, self.request()) + stream.encode_frame('FINAL', 2, self.final())
        instance = uid()
        with mock.patch.object(stream, 'preload'), mock.patch('agent_memory.omp_transport.observe', self.verified):
            result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                startup=lambda *args: self.binding, input=io.BytesIO(data), output=BrokenOutput())
        self.assertEqual(result, 0)
        self.assertTrue((stream._directory(self.base, self.session, instance) / 'closed.json').exists())

    def test_invalid_clock_control_revokes_wake_but_keeps_reading_source(self):
        control = stream.encode_frame('CLOCK-ACK', 0, {'nonce': uid(), 'j0': '1', 'j3': '2'})
        data = control + stream.encode_frame('REQUEST', 1, self.request()) + stream.encode_frame('FINAL', 2, self.final())
        instance = uid()
        with mock.patch.object(stream, 'preload'), mock.patch('agent_memory.omp_transport.observe', self.verified):
            result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                startup=lambda *args: self.binding, input=io.BytesIO(data), output=io.BytesIO())
        directory = stream._directory(self.base, self.session, instance)
        self.assertEqual(result, 0)
        self.assertEqual(turn_sources.read(directory / 'outcomes/1.json', stream.MAX_METADATA)['outcome'], 'verified')
        self.assertTrue((directory / 'closed.json').exists())

    def test_cancelled_superseded_and_invalid_clock_acks_never_pin_source_close(self):
        first, second = uid(), uid()
        controls = [
            stream.encode_frame('CLOCK', 0, {'nonce': first, 'j0': '100', 'j_deadline': '950000100'}),
            stream.encode_frame('CANCEL', 0, {'nonce': first}),
            stream.encode_frame('CLOCK-ACK', 0, {'nonce': first, 'j0': '100', 'j3': '200'}),
            stream.encode_frame('CLOCK', 0, {'nonce': first, 'j0': '100', 'j_deadline': '950000100'}),
            stream.encode_frame('CLOCK', 0, {'nonce': second, 'j0': '100', 'j_deadline': '950000100'}),
            stream.encode_frame('CLOCK-ACK', 0, {'nonce': first, 'j0': '100', 'j3': '200'}),
            stream.encode_frame('CLOCK-ACK', 0, {'nonce': second, 'j0': '100', 'j3': '99'}),
        ]
        data = b''.join(controls) + stream.encode_frame('REQUEST', 1, self.request()) + stream.encode_frame('FINAL', 2, self.final())
        instance = uid()
        observe = mock.Mock(side_effect=self.verified)
        with mock.patch.object(stream, 'preload'), mock.patch('agent_memory.omp_transport.observe', observe):
            result = stream.run(str(self.root), self.session, instance, self.lifecycle,
                startup=lambda *args: self.binding, input=io.BytesIO(data), output=io.BytesIO())
        directory = stream._directory(self.base, self.session, instance)
        self.assertEqual(result, 0)
        self.assertEqual(observe.call_count, 1)
        self.assertIsNone(observe.call_args.kwargs['delivery_window'])
        self.assertTrue((directory / 'closed.json').exists())


class FrozenImports(unittest.TestCase):
    def test_unpreloaded_late_packed_module_is_refused_without_blocking_stdlib(self):
        guard = stream._FrozenPackedImports()
        with self.assertRaises(ContractError):
            guard.find_spec('agent_memory.unpreloaded_new_module')
        self.assertIsNone(guard.find_spec('unrelated_external_package'))


class CanonicalWriterContention(unittest.TestCase):
    """Real bank/source authority and writer locks; not native qualification."""

    def setUp(self):
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.world = self.stack.enter_context(World())
        self.repo, self.bank = test_checkpoint.CheckpointContract().fixture(self.world)
        self.stack.enter_context(mock.patch.dict(os.environ, self.world.env, clear=True))
        omp_boundary._activate_marker(self.repo, fresh_only=True)
        context = omp_boundary.verify_marker(self.repo)
        self.session, self.instance, self.lifecycle = uid(), uid(), uid()
        self.binding = {'project_id': context.project_id, 'repo_root': str(self.repo),
            'bank_path': str(self.bank), 'checkout_key': context.scope['checkout_key'],
            'artifact_digest': 'a' * 64, 'installation_id': uuid.uuid4().hex,
            'initial_lifecycle': self.lifecycle}
        # Executable installation is outside this regression. Keep the actual
        # bank marker, source barrier, scope mint/rechecks and metadata locks.
        self.stack.enter_context(mock.patch.object(stream, '_attest_binding', return_value=object()))
        self.stack.enter_context(mock.patch.object(omp_installation, '_observe_observer_binding',
            side_effect=lambda repo, receipt, proof: omp_boundary._observe_marker(repo)))
        self.coverage = stream.Coverage(str(self.repo), self.session, self.instance,
                                       self.lifecycle, self.binding)
        self.stack.callback(self.coverage.close_live)
        self.responses = Responses()
        self.clocks = stream.ClockWindows(self.instance, lambda: self.responses.live, platform='darwin')
        self.lock = self.bank / '.agent-memory.lock'
        self.retained = None
        self.observer_calls = 0
        self.release_writer = None
        self.transactional = False
        self.writer_window = 'protected'
        self.empty_shared_probes = 0
        self.observer_result = None
        self.observer_error = None

    def request(self):
        return {'request': {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'ingest',
            'session': self.session, 'lifecycle': self.lifecycle,
            'payload': {'entries': [], 'view': {'leaf_id': 'native-a', 'source_kind': 'session-manager',
                'native_parents': [{'id': 'native-a', 'parent_id': None}]}}},
            'event_ordinal': 1, 'identity': {'view_id': uid(), 'leaf_id': 'native-a',
                                           'native_ids': ['native-a'], 'tool_ids': []}}

    def start_writer(self):
        if self.transactional:
            files = bank_transactions.files_at(self.bank)
            files['activeContext.md'] += '\nCanonical transaction fixture update.\n'
            proposal = bank_transactions.preview(self.repo, files, 'replace')
            apply = bank_transactions.apply
            pause_stage = 'transaction-after-marker'
        else:
            proposal = bank_writes.preview(self.repo, 'activeContext.md',
                (self.bank / 'activeContext.md').read_text() + '\nCanonical writer fixture update.\n')
            apply = bank_writes.apply
            pause_stage = 'locked-before-inventory'
        ready, release = threading.Event(), threading.Event()
        results, errors = [], []
        def pause():
            ready.set()
            if not release.wait(10):
                raise RuntimeError('Fixture did not release its canonical writer')
        def fault(stage):
            if stage == pause_stage and self.writer_window == 'protected':
                pause()
        def is_marker(fd):
            try:
                opened, named = os.fstat(fd), self.lock.lstat()
            except FileNotFoundError:
                return False
            return (opened.st_dev, opened.st_ino) == (named.st_dev, named.st_ino)
        flock, fsync = local_registry.fcntl.flock, os.fsync
        def publication_flock(fd, flags):
            if is_marker(fd):
                if flags & local_registry.fcntl.LOCK_SH:
                    self.empty_shared_probes += int(os.fstat(fd).st_size == 0)
                if flags & local_registry.fcntl.LOCK_EX and self.writer_window == 'pre-ex':
                    pause()
            return flock(fd, flags)
        def publication_fsync(fd):
            if self.writer_window == 'owner-fsync' and is_marker(fd):
                pause()
            return fsync(fd)
        def run():
            try:
                results.append(apply(proposal))
            except BaseException as exc:
                errors.append(exc)
                ready.set()
        self.stack.enter_context(mock.patch.object(bank_writes, 'fault', side_effect=fault))
        self.stack.enter_context(mock.patch.object(local_registry.fcntl, 'flock', side_effect=publication_flock))
        self.stack.enter_context(mock.patch.object(local_registry.os, 'fsync', side_effect=publication_fsync))
        writer = threading.Thread(target=run, daemon=True)
        def finish():
            release.set()
            writer.join(10)
            self.assertFalse(writer.is_alive(), 'Fixture writer failed to terminate')
            self.assertEqual(errors, [])
            self.assertEqual(len(results), 1)
            self.assertTrue(results[0]['verified'])
        self.stack.callback(finish)
        writer.start()
        self.assertTrue(ready.wait(10), 'Canonical writer did not reach its actual held-lock boundary')
        self.assertEqual(errors, [])
        self.release_writer = finish

    def observe(self, repo, request, *, on_started, **kwargs):
        self.observer_calls += 1
        try:
            on_started()
            self.retained = turn_sources.register(repo, 'omp', self.session, self.lifecycle,
                'b' * 64, native_source_id='native-a')
            self.start_writer()
            self.observer_result = {'verified': True, 'source_references': [{'session': self.session,
                'lifecycle': self.lifecycle, 'observation_id': self.retained['observation_id'],
                'source_sha256': self.retained['source_sha256']}]}
            return self.observer_result
        except BaseException as exc:
            self.observer_error = exc
            raise

    def execute(self):
        try:
            stream.FiniteExecutor(self.coverage, self.responses, self.clocks,
                observer=self.observe).execute('REQUEST', 1, None, bytearray(turn_sources.encoded(self.request())))
        finally:
            if self.observer_calls:
                self.assertIsNone(self.observer_error,
                    f'Fixture observer failed before metadata edge: {self.observer_error!r}')
                self.assertIsNotNone(self.observer_result, 'Fixture observer did not return its real source result')

    def final(self):
        return {'last_attempt_seq': 1, 'refused_count': 0, 'last_event_ordinal': 1, 'transition': None}

    def assert_retained(self):
        inventory = turn_sources.inspect(self.repo, 'omp', self.session, self.lifecycle)
        self.assertEqual(len(inventory['sources']), 1)
        self.assertEqual(inventory['sources'][0]['observation_id'], self.retained['observation_id'])
        self.assertEqual(inventory['sources'][0]['source_sha256'], self.retained['source_sha256'])
        self.assertEqual(inventory['sources'][0]['native_source_id'], 'native-a')
        self.assertFalse(inventory['complete'])
        self.assertEqual(self.observer_calls, 1)
        self.assertEqual(self.coverage.last_sequence, 1)

    def assert_outcome_after_writer_release(self):
        def release(seconds):
            self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
            self.assertTrue((self.coverage.directory / 'started/1.json').exists())
            owner = self.lock.read_bytes()
            expected = (local_registry.CanonicalWriterInitializing if self.writer_window == 'pre-ex'
                        else local_registry.CanonicalWriterBusy)
            with self.assertRaises(expected):
                scope_binding.require(self.repo)
            self.assertEqual(self.lock.read_bytes(), owner)
            if self.writer_window == 'pre-ex':
                self.assertEqual(owner, b'')
            if self.transactional:
                marker = stream.load((self.bank / '.agent-memory-transaction.md').read_bytes())
                self.assertEqual(marker['schema'], 'agent-memory/transaction-marker-v1')
                self.assertEqual(marker['transaction_id'], stream.load(owner)['operation_id'])
            # Retry must not wait while holding either source or stream locks.
            with turn_sources.source_barrier(self.repo, exclusive=True):
                with turn_sources.private_lock(self.coverage.lock_path):
                    pass
            self.release_writer()
        # Isolate the stream clock rather than changing time.sleep used by Git
        # subprocess internals or the real canonical writer thread.
        with mock.patch.object(stream, 'time', wraps=stream.time) as clock:
            clock.sleep.side_effect = release
            self.execute()
        clock.sleep.assert_called_once_with(stream.BUSY_BACKOFF)
        self.assertEqual(self.empty_shared_probes, 0)
        self.assert_retained()
        outcome = turn_sources.read(self.coverage.directory / 'outcomes/1.json', stream.MAX_METADATA)
        self.assertEqual(outcome['outcome'], 'verified')
        self.assertEqual(outcome['source_references'][0]['observation_id'], self.retained['observation_id'])
        self.assertTrue(self.coverage.close(self.final()))
        inventory = stream.inventory(str(self.repo))
        self.assertEqual(inventory['pending'], [])
        self.assertTrue(inventory['streams'][0]['closed'])

    def test_actual_writer_at_outcome_unwinds_then_closes_without_replaying_source(self):
        self.assert_outcome_after_writer_release()

    def test_actual_transaction_at_outcome_waits_without_adopting_its_marker(self):
        self.transactional = True
        self.assert_outcome_after_writer_release()

    def test_real_writer_pre_ex_empty_publication_defers_without_stealing_its_lock(self):
        self.writer_window = 'pre-ex'
        self.assert_outcome_after_writer_release()

    def test_real_writer_full_owner_fsync_window_has_ex_ownership_before_probe(self):
        self.writer_window = 'owner-fsync'
        self.assert_outcome_after_writer_release()

    def test_transaction_left_after_writer_release_remains_interruption(self):
        self.transactional = True
        marker = self.bank / '.agent-memory-transaction.md'
        preserved = []
        def release(seconds):
            preserved.append(marker.read_bytes())
            self.release_writer()
            marker.write_bytes(preserved[0])
            marker.chmod(0o600)
        with mock.patch.object(stream, 'time', wraps=stream.time) as clock:
            clock.sleep.side_effect = release
            with self.assertRaises(ContractError) as refused:
                self.execute()
        self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        self.assertEqual(clock.sleep.call_count, 1)
        self.assertEqual(marker.read_bytes(), preserved[0])
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        marker.unlink()
        self.assert_retained()
        self.assertFalse(self.coverage.close(self.final()))

    def assert_writer_metadata_exhaustion(self):
        ticks = iter((0.0, 0.0, 30.0))
        with mock.patch.object(stream, 'time', wraps=stream.time) as clock:
            clock.monotonic.side_effect = lambda: next(ticks, 30.0)
            with self.assertRaises(turn_sources.SourceTransportBusy):
                self.execute()
        clock.sleep.assert_not_called()
        self.assertTrue(self.lock.exists())
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        self.assertEqual(self.observer_calls, 1)
        self.release_writer()
        self.assert_retained()
        self.assertFalse(self.coverage.close(self.final()))
        pending = stream.inventory(str(self.repo))['pending']
        self.assertTrue(any(item['code'] == 'missing-outcome' for item in pending))
        self.assertFalse(any(kind == 'OBSERVED' for kind, _, _ in self.responses.values))

    def test_actual_writer_metadata_exhaustion_keeps_source_and_open_unknown(self):
        self.assert_writer_metadata_exhaustion()

    def test_real_initializing_writer_exhaustion_cannot_verify_or_close(self):
        self.writer_window = 'pre-ex'
        self.assert_writer_metadata_exhaustion()

    def test_unheld_empty_publication_exhaustion_retains_source_without_outcome(self):
        def unpublished():
            fd = os.open(self.lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
            os.close(fd)
        ticks = iter((0.0, 0.0, 30.0))
        with mock.patch.object(self, 'start_writer', side_effect=unpublished), \
                mock.patch.object(stream, 'time', wraps=stream.time) as clock:
            clock.monotonic.side_effect = lambda: next(ticks, 30.0)
            with self.assertRaises(turn_sources.SourceTransportBusy) as refused:
                self.execute()
        self.assertIsInstance(refused.exception.__cause__, local_registry.CanonicalWriterInitializing)
        self.assertEqual(self.lock.read_bytes(), b'')
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        self.assertFalse(any(kind == 'OBSERVED' for kind, _, _ in self.responses.values))
        clock.sleep.assert_not_called()
        self.lock.unlink()
        self.assert_retained()
        self.assertFalse(self.coverage.close(self.final()))

    def test_released_writer_with_unknown_transaction_is_hard_refusal_not_retry(self):
        marker = self.bank / '.agent-memory-transaction.md'
        def release(seconds):
            self.release_writer()
            marker.write_bytes(b'{"schema":"unknown-transaction"}')
            marker.chmod(0o600)
        with mock.patch.object(stream, 'time', wraps=stream.time) as clock:
            clock.sleep.side_effect = release
            with self.assertRaises(ContractError) as refused:
                self.execute()
        self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        self.assertEqual(clock.sleep.call_count, 1)
        self.assertEqual(marker.read_bytes(), b'{"schema":"unknown-transaction"}')
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        marker.unlink()  # Own deliberately invalid fixture marker, not recovery.
        self.assert_retained()
        self.assertFalse(self.coverage.close(self.final()))

    def test_writer_release_rediscovers_identity_instead_of_reusing_frame_scope(self):
        router = self.bank / 'MEMORY.md'
        original = router.read_bytes()
        def release(seconds):
            self.release_writer()
            router.write_bytes(original.replace(self.binding['project_id'].encode(), uid().encode()))
        with mock.patch.object(stream, 'time', wraps=stream.time) as clock:
            clock.sleep.side_effect = release
            with self.assertRaises(ContractError) as refused:
                self.execute()
        self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        self.assertEqual(clock.sleep.call_count, 1)
        self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
        router.write_bytes(original)
        self.assert_retained()

    def test_unheld_local_owner_is_interruption_not_live_pid_authority(self):
        with local_registry.exclusive(self.lock, uid(), 'bank-capture') as owner:
            with self.assertRaises(turn_sources.SourceTransportBusy):
                scope_binding.private_enrollment_context(self.repo)
        self.lock.write_bytes(owner)
        self.lock.chmod(0o600)
        with self.assertRaises(ContractError) as refused:
            scope_binding.private_enrollment_context(self.repo)
        self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        self.assertEqual(self.lock.read_bytes(), owner)

    def test_writer_arriving_after_preflight_is_classified_at_actual_identity_guard(self):
        from agent_memory import recovery_frontier
        original = recovery_frontier._verified_hold_at_bank
        with ExitStack() as held:
            def arrive(*args, **kwargs):
                value = original(*args, **kwargs)
                held.enter_context(local_registry.exclusive(self.lock, uid(), 'bank-capture'))
                return value
            with mock.patch.object(recovery_frontier, '_verified_hold_at_bank', side_effect=arrive):
                with self.assertRaises(turn_sources.SourceTransportBusy):
                    scope_binding.private_enrollment_context(self.repo)
            owner = self.lock.read_bytes()
            with self.assertRaises(local_registry.CanonicalWriterBusy):
                scope_binding.require(self.repo)
            self.assertEqual(self.lock.read_bytes(), owner)
        self.assertEqual(scope_binding.private_enrollment_context(self.repo).project_id, self.binding['project_id'])

    def test_rebound_inode_after_flock_observation_is_not_classified_as_busy(self):
        displaced = self.world.inside('displaced-own-writer-marker')
        flock = local_registry.fcntl.flock
        with local_registry.exclusive(self.lock, uid(), 'bank-capture') as owner:
            def rebind(fd, flags):
                try:
                    return flock(fd, flags)
                except BlockingIOError:
                    self.lock.rename(displaced)
                    self.lock.write_bytes(owner)
                    self.lock.chmod(0o600)
                    raise
            with mock.patch.object(local_registry.fcntl, 'flock', side_effect=rebind):
                with self.assertRaises(ContractError) as refused:
                    scope_binding.private_enrollment_context(self.repo)
            self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
            self.assertEqual(self.lock.read_bytes(), owner)
            self.assertEqual(displaced.read_bytes(), owner)
        self.lock.unlink()

    def assert_invalid_attempt_exit_retry(self, kind, *, corruption=None):
        path = self.coverage.directory / 'attempts/1.json'
        retain = stream._retain
        saved = []
        observer = mock.Mock(side_effect=CoverageFixture.verified)
        def persist(target, *args, **kwargs):
            result = retain(target, *args, **kwargs)
            if target == path and not saved:
                saved.append((path.read_bytes(), path.stat().st_ino, path.stat().st_mtime_ns))
                self.start_writer()
            return result
        def release(seconds):
            self.assertEqual(seconds, stream.BUSY_BACKOFF)
            self.assertEqual((self.coverage.last_sequence, self.coverage.refused_count), (0, 0))
            key = self.coverage._invalid_attempt_retry
            self.assertEqual(key, (self.coverage.open_digest, self.instance, 1, kind, 'hint', 0))
            with self.assertRaises(ContractError) as refused:
                self.coverage.refuse_invalid(1, kind, 'seal', 0)
            self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
            self.assertEqual(self.coverage._invalid_attempt_retry, key)
            self.assertEqual(path.read_bytes(), saved[0][0])
            observer.assert_not_called()
            with turn_sources.source_barrier(self.repo, exclusive=True), turn_sources.private_lock(self.coverage.lock_path):
                pass
            self.release_writer()
            if corruption == 'partial':
                path.write_bytes(b'{')
            elif corruption == 'different':
                value = stream.load(saved[0][0])
                value['event_ordinal'] = 1
                path.write_bytes(turn_sources.encoded(value))
        with mock.patch.object(stream, '_retain', side_effect=persist), \
                mock.patch.object(stream, 'time', wraps=stream.time) as clock:
            clock.sleep.side_effect = release
            executor = stream.FiniteExecutor(self.coverage, self.responses, self.clocks, observer=observer)
            if corruption is None:
                executor.execute(kind, 1, None, bytearray(b'{'))
            else:
                with self.assertRaises((ContractError, ValueError)) as refused:
                    executor.execute(kind, 1, None, bytearray(b'{'))
                self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        clock.sleep.assert_called_once_with(stream.BUSY_BACKOFF)
        observer.assert_not_called()
        self.assertIsNone(self.coverage._invalid_attempt_retry)
        if corruption is not None:
            expected = b'{' if corruption == 'partial' else turn_sources.encoded(
                {**stream.load(saved[0][0]), 'event_ordinal': 1})
            self.assertEqual(path.read_bytes(), expected)
            self.assertEqual((self.coverage.last_sequence, self.coverage.refused_count), (0, 0))
            self.assertFalse((self.coverage.directory / 'outcomes/1.json').exists())
            return
        self.assertEqual((path.read_bytes(), path.stat().st_ino, path.stat().st_mtime_ns), saved[0])
        outcome = turn_sources.read(self.coverage.directory / 'outcomes/1.json', stream.MAX_METADATA)
        self.assertEqual((outcome['outcome'], outcome['code']), ('durable-refusal', 'invalid-request'))
        refused_count = int(kind == 'REFUSED-ATTEMPT')
        self.assertEqual((self.coverage.last_sequence, self.coverage.refused_count), (1, refused_count))
        self.assertEqual([(frame, sequence) for frame, sequence, _ in self.responses.values], [('REFUSED', 1)])
        executor.execute('REQUEST', 2, None, bytearray(turn_sources.encoded(self.request())))
        self.assertEqual(observer.call_count, 1)
        self.assertTrue(self.coverage.close({'last_attempt_seq': 2, 'refused_count': refused_count,
                                            'last_event_ordinal': 1, 'transition': None}))

    def test_invalid_request_durable_attempt_exit_busy_resumes_exactly_once(self):
        self.assert_invalid_attempt_exit_retry('REQUEST')

    def test_invalid_refused_frame_durable_attempt_exit_busy_counts_once(self):
        self.assert_invalid_attempt_exit_retry('REFUSED-ATTEMPT')

    def test_invalid_attempt_exit_busy_never_adopts_partial_retry_record(self):
        self.assert_invalid_attempt_exit_retry('REQUEST', corruption='partial')

    def test_invalid_attempt_exit_busy_never_adopts_different_retry_record(self):
        self.assert_invalid_attempt_exit_retry('REQUEST', corruption='different')

    def test_empty_held_and_unheld_slots_never_mint_scope_or_claim_known_writer(self):
        for held in (False, True):
            with self.subTest(held=held):
                fd = os.open(self.lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
                try:
                    if held:
                        local_registry.fcntl.flock(fd, local_registry.fcntl.LOCK_EX | local_registry.fcntl.LOCK_NB)
                    with mock.patch.object(local_registry.fcntl, 'flock', wraps=local_registry.fcntl.flock) as probe:
                        with self.assertRaises(local_registry.CanonicalWriterInitializing):
                            local_registry.check_canonical_writer(self.lock, bank_writes.OPERATIONS)
                    probe.assert_not_called()
                    with self.assertRaises(local_registry.CanonicalWriterInitializing):
                        scope_binding.require(self.repo)
                    with self.assertRaises(turn_sources.SourceTransportBusy) as refused:
                        scope_binding.private_enrollment_context(self.repo)
                    self.assertIsInstance(refused.exception.__cause__, local_registry.CanonicalWriterInitializing)
                    self.assertNotIsInstance(refused.exception.__cause__, local_registry.CanonicalWriterBusy)
                    self.assertEqual(self.lock.read_bytes(), b'')
                finally:
                    os.close(fd)
                    self.lock.unlink()

    def assert_late_transaction_deferral(self, boundary):
        from agent_memory import recovery_frontier
        self.transactional = True
        fired = False
        original = recovery_frontier._marker_bytes
        original_hold = recovery_frontier._verified_hold_at_bank
        def arrive():
            nonlocal fired
            fired = True
            self.start_writer()
        def observe(root, bank, *args, **kwargs):
            if not fired:
                arrive()
            return original_hold(root, bank, *args, **kwargs)
        def read(bank, name, *, expected):
            selected = bank == self.bank and name == '.agent-memory-transaction.md' and not fired
            if selected and boundary == 'during-read':
                arrive()
                # An actual contended writer, not this arbitrary error, is the
                # only reason the wrapper may reclassify the failed read.
                raise ContractError('Synthetic bounded marker read interrupted')
            raw = original(bank, name, expected=expected)
            if selected and boundary == 'after-read':
                arrive()
            return raw
        target = '_verified_hold_at_bank' if boundary == 'entry' else '_marker_bytes'
        callback = observe if boundary == 'entry' else read
        with mock.patch.object(recovery_frontier, target, side_effect=callback):
            with self.assertRaises(turn_sources.SourceTransportBusy) as refused:
                scope_binding.private_enrollment_context(self.repo)
        self.assertTrue(fired)
        self.assertIsInstance(refused.exception.__cause__, local_registry.CanonicalWriterBusy)
        self.release_writer()
        self.assertEqual(scope_binding.private_enrollment_context(self.repo).project_id, self.binding['project_id'])

    def test_late_transaction_before_frontier_entry_is_exact_writer_deferral(self):
        self.assert_late_transaction_deferral('entry')

    def test_late_transaction_during_failed_marker_read_is_exact_writer_deferral(self):
        self.assert_late_transaction_deferral('during-read')

    def test_late_transaction_after_marker_read_is_exact_writer_deferral(self):
        self.assert_late_transaction_deferral('after-read')

    def test_arbitrary_marker_read_failure_without_writer_is_not_retryable(self):
        from agent_memory import recovery_frontier
        with mock.patch.object(recovery_frontier, '_marker_bytes',
                side_effect=ContractError('Synthetic bounded marker read interrupted')):
            with self.assertRaises(ContractError) as refused:
                scope_binding.private_enrollment_context(self.repo)
        self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
        self.assertFalse(self.lock.exists())

    def test_failed_marker_read_with_unproven_empty_slot_keeps_original_refusal(self):
        from agent_memory import recovery_frontier
        failure = ContractError('Synthetic bounded marker read interrupted')
        def read(*args, **kwargs):
            fd = os.open(self.lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
            os.close(fd)
            raise failure
        with mock.patch.object(recovery_frontier, '_marker_bytes', side_effect=read):
            with self.assertRaises(ContractError) as refused:
                scope_binding.private_enrollment_context(self.repo)
        self.assertIs(refused.exception, failure)
        self.assertEqual(self.lock.read_bytes(), b'')

    def test_held_unknown_foreign_malformed_and_unsafe_owners_never_become_busy(self):
        for mutation in ('unknown-operation', 'foreign-host', 'invalid-pid', 'malformed', 'oversize', 'noncanonical', 'symlink', 'hardlink', 'mode'):
            with self.subTest(mutation=mutation):
                with local_registry.exclusive(self.lock, uid(), 'bank-capture') as owner:
                    value = stream.load(owner)
                    if mutation == 'unknown-operation':
                        value['operation'] = 'bank-future-operation'
                    elif mutation == 'foreign-host':
                        value['host'] = 'foreign.fixture.invalid'
                    elif mutation == 'invalid-pid':
                        value['pid'] = 0
                    raw = (b'{' if mutation == 'malformed' else b'x' * 4097 if mutation == 'oversize'
                           else turn_sources.encoded(value) if mutation == 'noncanonical' else local_registry.encoded(value))
                    if mutation == 'symlink':
                        target = self.world.inside('foreign-owner.json')
                        target.write_bytes(owner)
                        self.lock.unlink()
                        self.lock.symlink_to(target)
                    else:
                        self.lock.write_bytes(raw)
                    if mutation == 'hardlink':
                        os.link(self.lock, self.world.inside('unexpected-owner-alias'))
                    elif mutation == 'mode':
                        self.lock.chmod(0o640)
                    with self.assertRaises(ContractError) as refused:
                        scope_binding.private_enrollment_context(self.repo)
                    self.assertNotIsInstance(refused.exception, turn_sources.SourceTransportBusy)
                    if mutation == 'symlink':
                        self.assertEqual(target.read_bytes(), owner)
                    else:
                        self.assertEqual(self.lock.read_bytes(), raw)
                self.lock.unlink(missing_ok=True)


class CanonicalWriterObservationBoundaries(unittest.TestCase):
    """Real kernel locks with deterministic acquisition/release interleavings."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.bank = self.root / 'bank'
        self.bank.mkdir(mode=0o700)
        self.lock = self.bank / '.agent-memory.lock'

    def interleave(self, boundary, *, replace=False):
        writer = local_registry.exclusive(self.lock, uid(), 'bank-capture')
        owner = writer.__enter__()
        initial = self.lock.lstat()
        fired = False
        released = False
        fstats = stats = 0
        real_lstat, real_open, real_fstat, real_stat = Path.lstat, os.open, os.fstat, os.stat
        real_flock = local_registry.fcntl.flock
        def change(point):
            nonlocal fired, released
            if fired or point != boundary:
                return
            fired = True
            if replace:
                self.lock.rename(self.bank / 'displaced-own-marker')
                self.lock.write_bytes(owner)
                self.lock.chmod(0o600)
            else:
                writer.__exit__(None, None, None)
                released = True
        def lstat(path, *args, **kwargs):
            info = real_lstat(path, *args, **kwargs)
            if path == self.lock:
                change('after-lstat')
            return info
        def opening(path, flags, *args, **kwargs):
            selected = path == self.lock.name and kwargs.get('dir_fd') is not None
            if selected:
                change('before-open')
            fd = real_open(path, flags, *args, **kwargs)
            if selected:
                change('after-open')
            return fd
        def fstat(fd):
            nonlocal fstats
            info = real_fstat(fd)
            if (info.st_dev, info.st_ino) == (initial.st_dev, initial.st_ino):
                fstats += 1
                change({1: 'after-initial-fstat', 2: 'after-read-fstat', 3: 'after-final-fstat'}.get(fstats))
            return info
        def stat(path, *args, **kwargs):
            nonlocal stats
            info = real_stat(path, *args, **kwargs)
            if path == self.lock.name and kwargs.get('dir_fd') is not None:
                stats += 1
                change({1: 'after-first-name-stat', 2: 'after-read-name-stat', 3: 'after-final-name-stat'}.get(stats))
            return info
        def flock(fd, flags):
            if flags & local_registry.fcntl.LOCK_SH:
                change('before-flock')
            try:
                return real_flock(fd, flags)
            except BlockingIOError:
                change('after-busy-flock')
                raise
        try:
            with mock.patch.object(Path, 'lstat', new=lstat), \
                    mock.patch.object(local_registry.os, 'open', side_effect=opening), \
                    mock.patch.object(local_registry.os, 'fstat', side_effect=fstat), \
                    mock.patch.object(local_registry.os, 'stat', side_effect=stat), \
                    mock.patch.object(local_registry.fcntl, 'flock', side_effect=flock):
                if replace:
                    with self.assertRaises(ContractError) as refused:
                        local_registry.check_canonical_writer(self.lock, {'capture'})
                    self.assertNotIsInstance(refused.exception,
                        (local_registry.CanonicalWriterBusy, local_registry.CanonicalWriterInitializing))
                else:
                    try:
                        local_registry.check_canonical_writer(self.lock, {'capture'})
                    except local_registry.CanonicalWriterBusy:
                        # Release just after the last name observation may
                        # retain the already-proved busy result, never authority.
                        self.assertEqual(boundary, 'after-final-name-stat')
            self.assertTrue(fired, f'Interleaving was not reached: {boundary}')
            if replace:
                self.assertEqual(self.lock.read_bytes(), owner)
                self.assertEqual((self.bank / 'displaced-own-marker').read_bytes(), owner)
            else:
                self.assertFalse(self.lock.exists())
                self.assertIsNone(local_registry.check_canonical_writer(self.lock, {'capture'}))
        finally:
            if not released:
                writer.__exit__(None, None, None)
            self.lock.unlink(missing_ok=True)
            (self.bank / 'displaced-own-marker').unlink(missing_ok=True)

    def test_clean_release_at_each_observation_boundary_is_not_recovery(self):
        for boundary in ('after-lstat', 'before-open', 'after-open', 'after-initial-fstat',
                         'after-first-name-stat', 'after-read-fstat', 'after-read-name-stat',
                         'before-flock', 'after-busy-flock', 'after-final-fstat', 'after-final-name-stat'):
            with self.subTest(boundary=boundary):
                self.interleave(boundary)

    def test_replacement_at_checked_boundaries_remains_hard_refusal(self):
        for boundary in ('before-open', 'after-initial-fstat', 'after-read-fstat',
                         'before-flock', 'after-busy-flock', 'after-final-fstat'):
            with self.subTest(boundary=boundary):
                self.interleave(boundary, replace=True)

    def test_zero_initialization_requires_exact_safe_slot_properties(self):
        for mutation in ('mode', 'hardlink', 'uid', 'symlink'):
            with self.subTest(mutation=mutation):
                self.lock.touch(mode=0o600)
                alias = self.bank / 'unsafe-alias'
                if mutation == 'mode':
                    self.lock.chmod(0o640)
                elif mutation == 'hardlink':
                    os.link(self.lock, alias)
                elif mutation == 'symlink':
                    self.lock.rename(alias)
                    self.lock.symlink_to(alias)
                with ExitStack() as patches:
                    if mutation == 'uid':
                        actual_uid = os.geteuid()
                        patches.enter_context(mock.patch.object(local_registry.os, 'geteuid', return_value=actual_uid + 1))
                    with self.assertRaises(ContractError) as refused:
                        local_registry.check_canonical_writer(self.lock, {'capture'})
                self.assertNotIsInstance(refused.exception,
                    (local_registry.CanonicalWriterBusy, local_registry.CanonicalWriterInitializing))
                self.lock.unlink()
                alias.unlink(missing_ok=True)

    def test_zero_initialization_cannot_rebind_its_pinned_bank(self):
        self.lock.touch(mode=0o600)
        real_open = os.open
        moved = self.root / 'displaced-bank'
        def opening(path, flags, *args, **kwargs):
            fd = real_open(path, flags, *args, **kwargs)
            if path == self.lock.name and kwargs.get('dir_fd') is not None:
                self.bank.rename(moved)
                self.bank.mkdir(mode=0o700)
            return fd
        with mock.patch.object(local_registry.os, 'open', side_effect=opening):
            with self.assertRaises(ContractError) as refused:
                local_registry.check_canonical_writer(self.lock, {'capture'})
        self.assertNotIsInstance(refused.exception,
            (local_registry.CanonicalWriterBusy, local_registry.CanonicalWriterInitializing))
        self.assertEqual((moved / self.lock.name).read_bytes(), b'')

    def test_real_owner_publication_between_empty_samples_is_only_initialization(self):
        for sample in (1, 2):
            with self.subTest(sample=sample):
                waiting, publish, published, release = (threading.Event() for _ in range(4))
                owners, errors = [], []
                write, fstat = os.write, os.fstat
                def publication(fd, payload):
                    waiting.set()
                    if not publish.wait(10):
                        raise RuntimeError('Fixture did not permit owner publication')
                    return write(fd, payload)
                def run():
                    try:
                        with local_registry.exclusive(self.lock, uid(), 'bank-capture') as owner:
                            owners.append(owner)
                            published.set()
                            if not release.wait(10):
                                raise RuntimeError('Fixture did not release published owner')
                    except BaseException as exc:
                        errors.append(exc)
                        waiting.set()
                        published.set()
                with mock.patch.object(local_registry.os, 'write', side_effect=publication):
                    writer = threading.Thread(target=run, daemon=True)
                    writer.start()
                    try:
                        self.assertTrue(waiting.wait(10))
                        self.assertEqual(errors, [])
                        inode = self.lock.lstat().st_ino
                        samples = 0
                        def observe(fd):
                            nonlocal samples
                            info = fstat(fd)
                            if info.st_ino == inode and info.st_size == 0:
                                samples += 1
                                if samples == sample:
                                    publish.set()
                                    self.assertTrue(published.wait(10))
                                    self.assertEqual(errors, [])
                            return info
                        with mock.patch.object(local_registry.os, 'fstat', side_effect=observe), \
                                mock.patch.object(local_registry.fcntl, 'flock', wraps=local_registry.fcntl.flock) as probe:
                            with self.assertRaises(local_registry.CanonicalWriterInitializing):
                                local_registry.check_canonical_writer(self.lock, {'capture'})
                        probe.assert_not_called()
                        self.assertEqual(samples, sample)
                        self.assertEqual(self.lock.read_bytes(), owners[0])
                    finally:
                        publish.set()
                        release.set()
                        writer.join(10)
                        self.assertFalse(writer.is_alive())
                    self.assertEqual(errors, [])
                self.assertFalse(self.lock.exists())

    def test_nonempty_short_owner_write_remains_interruption_not_initialization(self):
        write = os.write
        def short(fd, payload):
            return write(fd, payload[:1])
        with mock.patch.object(local_registry.os, 'write', side_effect=short):
            with self.assertRaises(ContractError):
                with local_registry.exclusive(self.lock, uid(), 'bank-capture'):
                    self.fail('A partial owner publication entered protected work')
        self.assertEqual(self.lock.read_bytes(), b'{')
        with self.assertRaises(ContractError) as refused:
            local_registry.check_canonical_writer(self.lock, {'capture'})
        self.assertNotIsInstance(refused.exception,
            (local_registry.CanonicalWriterBusy, local_registry.CanonicalWriterInitializing))


    def test_real_recovery_wins_first_ex_without_stranding_creators_empty_slot(self):
        creator = threading.get_ident()
        recovery_held, release_recovery = threading.Event(), threading.Event()
        errors, recovered = [], []
        flock = local_registry.fcntl.flock
        def recover():
            try:
                recovered.append(local_registry.recover(self.bank / 'registry.json', self.lock, {'bank-capture'}))
            except BaseException as exc:
                errors.append(exc)
                recovery_held.set()
        recovery = threading.Thread(target=recover, daemon=True)
        def contend(fd, flags):
            if threading.get_ident() == creator:
                recovery.start()
                self.assertTrue(recovery_held.wait(10))
                self.assertEqual(errors, [])
                return flock(fd, flags)
            result = flock(fd, flags)
            recovery_held.set()
            if not release_recovery.wait(10):
                raise RuntimeError('Fixture did not release its actual recovery lock')
            return result
        try:
            with mock.patch.object(local_registry.fcntl, 'flock', side_effect=contend):
                with self.assertRaises(ContractError) as refused:
                    with local_registry.exclusive(self.lock, uid(), 'bank-capture'):
                        self.fail('Writer entered protected work without its first EX lock')
                self.assertIsInstance(refused.exception.__cause__, BlockingIOError)
                self.assertFalse(self.lock.exists())
        finally:
            release_recovery.set()
            if recovery.ident is not None:
                recovery.join(10)
                self.assertFalse(recovery.is_alive())
        self.assertEqual(recovered, [])
        self.assertEqual(len(errors), 1)
        self.assertIsInstance(errors[0], FileNotFoundError)

    def test_noncontention_first_ex_error_removes_only_creators_empty_slot(self):
        import errno
        for code in (errno.ENOLCK, errno.ENOTSUP):
            with self.subTest(errno=code):
                failure = OSError(code, 'Synthetic first ownership lock failure')
                with mock.patch.object(local_registry.fcntl, 'flock', side_effect=failure):
                    with self.assertRaises(ContractError) as refused:
                        with local_registry.exclusive(self.lock, uid(), 'bank-capture'):
                            self.fail('Failed acquisition entered protected work')
                self.assertIs(refused.exception.__cause__, failure)
                self.assertFalse(self.lock.exists())
                with local_registry.exclusive(self.lock, uid(), 'bank-capture'):
                    self.assertTrue(self.lock.is_file())
                self.assertFalse(self.lock.exists())

    def test_interrupt_during_marker_read_is_not_reclassified_as_writer_retry(self):
        from agent_memory import recovery_frontier
        for failure in (KeyboardInterrupt(), SystemExit(7), MemoryError()):
            with self.subTest(error=type(failure).__name__), ExitStack() as owners:
                def read(*args, **kwargs):
                    owners.enter_context(local_registry.exclusive(self.lock, uid(), 'bank-capture'))
                    raise failure
                with recovery_frontier._observe_source_markers(), \
                        mock.patch.object(recovery_frontier, '_marker_bytes', side_effect=read):
                    with self.assertRaises(type(failure)) as refused:
                        recovery_frontier.marker_bytes(self.bank)
                self.assertIs(refused.exception, failure)

    def test_failed_first_ex_preserves_replacement_or_changed_empty_slot(self):
        for mutation in ('replacement', 'mode', 'hardlink', 'uid', 'nonempty'):
            with self.subTest(mutation=mutation), ExitStack() as patches:
                holders = []
                flock = local_registry.fcntl.flock
                def contend(fd, flags):
                    holder = os.open(self.lock, os.O_RDONLY | os.O_NOFOLLOW)
                    holders.append(holder)
                    flock(holder, local_registry.fcntl.LOCK_EX | local_registry.fcntl.LOCK_NB)
                    if mutation == 'replacement':
                        self.lock.rename(self.bank / 'displaced-empty-slot')
                        self.lock.write_bytes(b'foreign replacement')
                        self.lock.chmod(0o600)
                    elif mutation == 'mode':
                        self.lock.chmod(0o640)
                    elif mutation == 'hardlink':
                        os.link(self.lock, self.bank / 'extra-empty-alias')
                    elif mutation == 'uid':
                        actual_uid = os.geteuid()
                        patches.enter_context(mock.patch.object(local_registry.os, 'geteuid', return_value=actual_uid + 1))
                    else:
                        os.write(fd, b'{')
                    return flock(fd, flags)
                try:
                    with mock.patch.object(local_registry.fcntl, 'flock', side_effect=contend):
                        with self.assertRaises(ContractError):
                            with local_registry.exclusive(self.lock, uid(), 'bank-capture'):
                                self.fail('Contended writer entered protected work')
                    expected = b'foreign replacement' if mutation == 'replacement' else b'{' if mutation == 'nonempty' else b''
                    self.assertEqual(self.lock.read_bytes(), expected)
                    self.assertEqual(len(holders), 1)
                    self.assertGreater(os.fstat(holders[0]).st_ino, 0)
                finally:
                    for holder in holders:
                        os.close(holder)
                    self.lock.unlink(missing_ok=True)
                    (self.bank / 'displaced-empty-slot').unlink(missing_ok=True)
                    (self.bank / 'extra-empty-alias').unlink(missing_ok=True)

    def test_preexisting_empty_slot_is_never_creator_cleanup_authority(self):
        self.lock.touch(mode=0o600)
        original = self.lock.lstat()
        with mock.patch.object(local_registry.os, 'unlink', wraps=os.unlink) as unlink:
            with self.assertRaises(ContractError):
                with local_registry.exclusive(self.lock, uid(), 'bank-capture'):
                    self.fail('A preexisting empty slot was adopted')
        unlink.assert_not_called()
        self.assertEqual(self.lock.read_bytes(), b'')
        self.assertEqual(self.lock.lstat().st_ino, original.st_ino)

    def test_owner_write_oserror_preserves_publication_evidence_and_closes_creator(self):
        opened = []
        opening = os.open
        failure = OSError('Synthetic owner publication I/O error')
        def record(path, flags, *args, **kwargs):
            fd = opening(path, flags, *args, **kwargs)
            opened.append(fd)
            return fd
        with mock.patch.object(local_registry.os, 'open', side_effect=record), \
                mock.patch.object(local_registry.os, 'write', side_effect=failure):
            with self.assertRaises(ContractError) as refused:
                with local_registry.exclusive(self.lock, uid(), 'bank-capture'):
                    self.fail('A failed owner write entered protected work')
        self.assertIs(refused.exception.__cause__, failure)
        self.assertEqual(self.lock.read_bytes(), b'')
        self.assertEqual(len(opened), 1)
        with self.assertRaises(OSError):
            os.fstat(opened[0])
        with self.assertRaises(local_registry.CanonicalWriterInitializing):
            local_registry.check_canonical_writer(self.lock, {'capture'})
        self.assertEqual(self.lock.read_bytes(), b'')

    def test_initial_lstat_io_error_is_typed_without_opening_descriptors(self):
        lstat = Path.lstat
        failure = PermissionError('Synthetic marker inspection denial')
        def deny(path, *args, **kwargs):
            if path == self.lock:
                raise failure
            return lstat(path, *args, **kwargs)
        with mock.patch.object(Path, 'lstat', new=deny), \
                mock.patch.object(local_registry.os, 'open', wraps=os.open) as opening:
            with self.assertRaises(ContractError) as refused:
                local_registry.check_canonical_writer(self.lock, {'capture'})
        self.assertIs(refused.exception.__cause__, failure)
        opening.assert_not_called()

    def test_bank_replaced_by_symlink_before_open_is_typed_without_closing_foreign_fds(self):
        self.lock.touch(mode=0o600)
        lstat = Path.lstat
        moved = self.root / 'replaced-before-pin'
        replaced = False
        def rebind(path, *args, **kwargs):
            nonlocal replaced
            info = lstat(path, *args, **kwargs)
            if path == self.lock and not replaced:
                replaced = True
                self.bank.rename(moved)
                self.bank.symlink_to(moved, target_is_directory=True)
            return info
        with mock.patch.object(Path, 'lstat', new=rebind), \
                mock.patch.object(local_registry.os, 'close', wraps=os.close) as close:
            with self.assertRaises(ContractError) as refused:
                local_registry.check_canonical_writer(self.lock, {'capture'})
        self.assertIsInstance(refused.exception.__cause__, OSError)
        close.assert_not_called()
        self.assertEqual((moved / self.lock.name).read_bytes(), b'')


class DeadlineWindows(unittest.TestCase):
    def setUp(self):
        self.instance, self.nonce = uid(), uid()
        self.responses = Responses()
        self.clocks = stream.ClockWindows(self.instance, lambda: self.responses.live, platform='darwin')

    def calibrated(self, j3='200', deadline='950000100'):
        with mock.patch.object(stream.time, 'monotonic_ns', side_effect=[1000, 1010]):
            self.clocks.control('CLOCK', {'nonce': self.nonce, 'j0': '100', 'j_deadline': deadline}, self.responses)
        self.clocks.control('CLOCK-ACK', {'nonce': self.nonce, 'j0': '100', 'j3': j3}, self.responses)
        return self.clocks.window(self.nonce)

    def test_different_epochs_use_lower_offset_and_exact_rate_subtraction(self):
        window = self.calibrated()
        self.assertEqual(window.L, 810)
        remaining = window.j_deadline - window.j3
        drift = (164 * remaining + 9999) // 10000
        deadline = window.j_deadline + window.L - drift
        with mock.patch.object(stream.time, 'monotonic_ns', return_value=deadline - stream.MARGIN_NS - 1):
            self.assertTrue(window.allow_reservation())
        with mock.patch.object(stream.time, 'monotonic_ns', return_value=deadline - stream.MARGIN_NS):
            self.assertFalse(window.allow_reservation())
        self.assertEqual(window.support, 'provisional-unqualified')

    def test_cancel_eof_broken_output_and_recipient_revoke_window(self):
        window = self.calibrated()
        with mock.patch.object(stream.time, 'monotonic_ns', return_value=0):
            self.assertFalse(window.allow_reservation(lambda: False))
            self.responses.live = False
            self.assertFalse(window.allow_reservation())
            self.responses.live = True
            self.clocks.control('CANCEL', {'nonce': self.nonce}, self.responses)
            self.assertFalse(window.allow_reservation())
        self.clocks.end()
        self.assertIsNone(self.clocks.window(self.nonce))

    def test_late_calibration_and_nonce_mismatch_cannot_mint(self):
        self.assertIsNone(self.calibrated(j3='950000101'))
        with self.assertRaises(ContractError):
            self.clocks.control('CLOCK-ACK', {'nonce': uid(), 'j0': '100', 'j3': '200'}, self.responses)

    def test_linux_has_no_rate_bound_and_cannot_reserve(self):
        window = self.calibrated()
        window._platform = 'linux'
        with mock.patch.object(stream.time, 'monotonic_ns', return_value=0):
            self.assertFalse(window.allow_reservation())

    def test_windows_cannot_be_minted_or_serialized_by_json(self):
        with self.assertRaises(TypeError):
            stream.VerifiedNativeDeliveryWindow(None, L=0, j3=0, j_deadline=1,
                nonce=self.nonce, instance=self.instance, eligible=lambda: True, platform='darwin')
        window = self.calibrated()
        with self.assertRaises(TypeError):
            window.__reduce__()
        self.assertIsNone(self.clocks.window(uid()))


if __name__ == '__main__':
    unittest.main()
