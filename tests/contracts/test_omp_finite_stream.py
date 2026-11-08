"""Finite FIFO loss/phase/coverage boundaries; no native-runtime qualification."""
from contextlib import nullcontext
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import omp_stream as stream, turn_sources
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
