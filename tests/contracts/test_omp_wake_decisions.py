"""Optional finite wake-decision consumers; no native or timing qualification."""
import copy
import json
import os
from pathlib import Path
import stat
import sys
import unittest
from unittest.mock import patch

from world import World
from contracts import test_omp_source_phase as phase
from contracts import test_omp_boundary as fixtures
from agent_memory import diagnostics, omp_installation, omp_stream as stream
from agent_memory import omp_transport as transport, turn_sources

uid = phase.uid


class WakeDecisions(unittest.TestCase):
    # Reuse the real installed finite/source-phase fixture without inheriting its tests.
    setUp = phase.ArmedSourcePhase.setUp
    body = phase.ArmedSourcePhase.body
    execute = phase.ArmedSourcePhase.execute
    calibrate = phase.ArmedSourcePhase.calibrate
    outcome = phase.ArmedSourcePhase.outcome
    result = phase.ArmedSourcePhase.result
    roots = phase.ArmedSourcePhase.roots
    assert_source = phase.ArmedSourcePhase.assert_source

    def decision_path(self, sequence=1):
        return stream._wake_decision_directory(self.coverage.directory) / f'{sequence}.json'

    def decision(self, sequence=1):
        return turn_sources.read(self.decision_path(sequence), 4096)

    def readback(self):
        return omp_installation.diagnostics(self.repo, self.receipt)['wake_decisions']

    def offered(self):
        reserve = stream._LateNativeDeliveryWindow.reserve
        def calibrated(window):
            self.calibrate()
            return reserve(window)
        with patch.object(stream._LateNativeDeliveryWindow, 'reserve', calibrated), patch.object(stream.time, 'monotonic_ns', return_value=1_000_000):
            self.execute()
        self.assertTrue(self.result()['continue'])

    def immutable_bytes(self):
        paths = list(self.coverage.directory.rglob('*.json'))
        source = transport.session_dir(self.repo, self.session)
        paths.extend(path for path in source.rglob('*.json') if path.name != 'cursor.json')
        return {path: path.read_bytes() for path in paths}

    def test_offered_binds_actual_root_after_response_without_changing_old_evidence(self):
        retain = stream._retain_wake_decision
        phases, evidence = [], {}
        def observed(value):
            self.assertFalse(self.decision_path().exists())
            phases.append('response')
        def retain_after_response(*args):
            self.assertEqual(phases, ['response'])
            self.assertTrue(any(kind == 'OBSERVED' for kind, _, _ in self.responses.values))
            evidence.update(self.immutable_bytes())
            value = retain(*args)
            phases.append('diagnostic')
            return value
        self.responses.on_observed = observed
        with patch.object(stream, '_retain_wake_decision', side_effect=retain_after_response):
            self.offered()
        self.assertEqual(phases, ['response', 'diagnostic'])
        decision = self.decision()
        actual = turn_sources.read(self.roots()[0], 4096)
        self.assertEqual(actual['request_id'], self.result()['request_id'])
        self.assertEqual(set(decision), {'schema', 'open_sha256', 'attempt_sha256', 'outcome_sha256',
                                       'sequence', 'status', 'continuation_offered', 'human_root', 'root_wake_sha256'})
        self.assertEqual(decision['human_root'], actual['human_root'])
        self.assertEqual(decision['root_wake_sha256'], stream._digest(actual))
        self.assertEqual(decision['open_sha256'], stream._digest(self.coverage.open))
        self.assertEqual(decision['attempt_sha256'], stream._digest(turn_sources.read(self.coverage.directory / 'attempts/1.json', stream.MAX_METADATA)))
        self.assertEqual(decision['outcome_sha256'], stream._digest(self.outcome()))
        self.assertEqual(decision['status'], 'offered')
        self.assertTrue(decision['continuation_offered'])
        self.assertEqual(omp_installation.diagnostics(self.repo, self.receipt)['answer_first'],
                         {'activation': 'receipt-owned', 'automation': 'unqualified', 'native_loading': 'not-observed'})
        self.assertLessEqual(self.decision_path().stat().st_size, 4096)
        self.assertEqual(stat.S_IMODE(self.decision_path().stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(self.decision_path().parent.stat().st_mode), 0o700)
        self.assertTrue(self.coverage.close({'last_attempt_seq': 1, 'refused_count': 0,
                                            'last_event_ordinal': 1, 'transition': None}))
        closed = (self.coverage.directory / 'closed.json').read_bytes()
        inventory = stream.inventory(self.repo)
        pending = transport.list_pending(self.repo)
        summary = self.readback()
        self.assertEqual(summary['counts']['offered'], 1)
        self.assertEqual(summary['rows'], [{'instance_id': self.instance, 'sequence': 1,
                                           'status': 'offered', 'continuation_offered': True}])
        self.assertEqual(summary['delivery'], 'not-observed')
        self.assertFalse(summary['assessment_authority'])
        self.assertFalse(summary['completion_inferred'])
        self.assertEqual(transport.list_pending(self.repo), pending)
        self.assertEqual(stream.inventory(self.repo), inventory)
        self.assertEqual((self.coverage.directory / 'closed.json').read_bytes(), closed)
        self.assertTrue(all(path.read_bytes() == raw for path, raw in evidence.items()))
        hidden = self.decision_path().parent.with_name('hidden-optional-records')
        self.decision_path().parent.rename(hidden)
        self.assertEqual(stream.inventory(self.repo), inventory)
        self.assertEqual(self.readback()['counts']['unrecorded'], 1)
        hidden.rename(self.decision_path().parent)
        self.assert_source()

    def test_not_armed_is_distinct_from_armed_without_nonce(self):
        self.execute(self.body(armed=False), nonce=False)
        self.execute(self.body(armed=True, ordinal=2), nonce=False, sequence=2)
        first, second = self.decision(), self.decision(2)
        self.assertEqual(first['status'], 'not-armed')
        self.assertEqual(second['status'], 'pending-undeliverable-window')
        for decision in (first, second):
            self.assertFalse(decision['continuation_offered'])
            self.assertIsNone(decision['human_root'])
            self.assertIsNone(decision['root_wake_sha256'])
        self.assertEqual(self.roots(), [])
        self.assertEqual(self.readback()['counts']['not-armed'], 1)
        self.assertEqual(self.readback()['counts']['pending-undeliverable-window'], 1)

    def test_existing_root_retains_legacy_bare_pending_without_inferred_cause(self):
        self.offered()
        root_bytes = self.roots()[0].read_bytes()
        self.nonce = uid()
        self.execute(self.body(ordinal=2), sequence=2)
        result = next(value['result'] for kind, sequence, value in self.responses.values
                      if kind == 'OBSERVED' and sequence == 2)
        self.assertEqual(result['status'], 'pending')
        self.assertFalse(result['continue'])
        self.assertEqual(self.decision(2)['status'], 'pending')
        self.assertIsNone(self.decision(2)['human_root'])
        self.assertEqual(self.roots()[0].read_bytes(), root_bytes)
        self.assertEqual(self.readback()['counts']['pending'], 1)

    def test_empty_verified_seal_reports_wake_not_evaluated(self):
        self.entries = []
        self.execute(nonce=False)
        self.assertEqual(self.outcome()['outcome'], 'verified')
        self.assertNotIn('continue', self.result())
        self.assertEqual(self.decision()['status'], 'wake-not-evaluated')
        self.assertEqual(self.readback()['counts']['wake-not-evaluated'], 1)
        self.assertEqual(self.roots(), [])

    def test_retained_root_after_cursor_failure_stays_unknown_and_rootless(self):
        replace = transport.replace_cursor
        reserve = stream._LateNativeDeliveryWindow.reserve
        def calibrated(window):
            self.calibrate()
            return reserve(window)
        def fail_after_replace(path, cursor):
            replace(path, cursor)
            if (cursor.get('control_interval') or {}).get('generated_guidance'):
                raise OSError('controlled cursor replacement exit fault')
        with patch.object(stream._LateNativeDeliveryWindow, 'reserve', calibrated), patch.object(transport, 'replace_cursor', side_effect=fail_after_replace), patch.object(stream.time, 'monotonic_ns', return_value=1_000_000):
            self.execute()
        self.assert_source()
        self.assertEqual(len(self.roots()), 1)
        self.assertEqual(self.result()['status'], 'pending-wake-unknown-after-reservation')
        decision = self.decision()
        self.assertEqual(decision['status'], 'pending-wake-unknown-after-reservation')
        self.assertIsNone(decision['human_root'])
        self.assertIsNone(decision['root_wake_sha256'])
        self.assertFalse(decision['continuation_offered'])
        self.assertEqual(self.readback()['counts']['offered'], 0)

    def test_offered_writer_checks_actual_request_without_cursor_or_root_scan(self):
        retain = stream._retain_wake_decision
        read = turn_sources.read
        reads = []
        def immutable_read(path, maximum):
            path = Path(path)
            self.assertNotEqual(path.name, 'cursor.json')
            reads.append(path)
            return read(path, maximum)
        def retain_checked(*args):
            with patch.object(turn_sources, 'read', side_effect=immutable_read), patch.object(transport, 'validate_root_wakes', side_effect=AssertionError('diagnostic must not scan roots')):
                return retain(*args)
        with patch.object(stream, '_retain_wake_decision', side_effect=retain_checked):
            self.offered()
        self.assertIn(self.roots()[0], reads)
        self.assertEqual(self.decision()['status'], 'offered')
        self.nonce = uid()
        self.entries.extend([self.helper.entry('other-human', 'user', 'A different root.', attribution='user'),
                             self.helper.entry('other-answer', 'assistant', 'A different answer.')])
        observe = self.executor.observer
        def mismatched(*args, **kwargs):
            result = observe(*args, **kwargs)
            if result.get('continue') is True:
                result['request_id'] = uid()
            return result
        reserve = stream._LateNativeDeliveryWindow.reserve
        def calibrated(window):
            self.calibrate()
            return reserve(window)
        with patch.object(self.executor, 'observer', side_effect=mismatched), patch.object(stream._LateNativeDeliveryWindow, 'reserve', calibrated), patch.object(stream.time, 'monotonic_ns', return_value=1_000_000):
            self.execute(self.body(ordinal=2), sequence=2)
        self.assertEqual(len(self.roots()), 2)
        self.assertFalse(self.decision_path(2).exists())
        self.assertEqual(self.readback()['counts']['unrecorded'], 1)

    def test_spoofed_record_keys_types_digests_and_status_are_unbound_without_contents(self):
        self.offered()
        actual = self.decision()
        inventory = stream.inventory(self.repo)
        variants = [dict(actual, private_payload='do-not-disclose'),
                    dict(actual, status='do-not-disclose'), dict(actual, continuation_offered=1),
                    dict(actual, sequence=True), dict(actual, open_sha256='0' * 64),
                    dict(actual, attempt_sha256='0' * 64), dict(actual, outcome_sha256='0' * 64),
                    dict(actual, root_wake_sha256='0' * 64), dict(actual, human_root=uid()),
                    dict(actual, status='pending', continuation_offered=False)]
        missing = dict(actual)
        del missing['schema']
        variants.append(missing)
        for value in variants:
            with self.subTest(fields=sorted(value), status=value.get('status')):
                self.decision_path().write_bytes(turn_sources.encoded(value))
                summary = self.readback()
                self.assertEqual(summary['counts']['unbound'], 1)
                self.assertEqual(summary['rows'][0]['status'], 'unbound')
                self.assertFalse(summary['rows'][0]['continuation_offered'])
                self.assertNotIn('do-not-disclose', json.dumps(summary))
                self.assertEqual(stream.inventory(self.repo), inventory)
        self.decision_path().write_bytes(json.dumps(actual, indent=2).encode())
        self.assertEqual(self.readback()['counts']['offered'], 1)

    def test_canonical_parsed_evidence_digests_survive_noncanonical_json_encoding(self):
        self.offered()
        inventory = stream.inventory(self.repo)
        paths = [self.coverage.directory / name for name in
                 ('open.json', 'attempts/1.json', 'started/1.json', 'outcomes/1.json')]
        paths.extend(self.roots())
        for path in paths:
            value = turn_sources.read(path, stream.MAX_METADATA)
            path.write_bytes(json.dumps(value, indent=2).encode())
        self.assertEqual(stream.inventory(self.repo), inventory)
        self.assertEqual(self.readback()['counts']['offered'], 1)

    def test_malformed_real_result_shapes_are_unknown_without_inventing_a_cause(self):
        observe = self.executor.observer
        shapes = [{'continue': 'yes', 'status': 'pending'},
                  {'continue': False, 'status': 'do-not-disclose'},
                  {'continue': True, 'request_id': None},
                  {'continue': False, 'status': ['do-not-disclose']}]
        responses = []
        def malformed(*args, **kwargs):
            result = observe(*args, **kwargs)
            result.update(shapes[len(responses)])
            responses.append(copy.deepcopy(result))
            return result
        with patch.object(self.executor, 'observer', side_effect=malformed):
            for sequence in range(1, len(shapes) + 1):
                self.execute(self.body(ordinal=sequence), nonce=False, sequence=sequence)
                self.assertEqual(self.decision(sequence)['status'], 'unknown')
                self.assertIsNone(self.decision(sequence)['human_root'])
        actual = [value['result'] for kind, _, value in self.responses.values if kind == 'OBSERVED']
        self.assertEqual(actual, responses)
        summary = self.readback()
        self.assertEqual(summary['counts']['unknown'], len(shapes))
        self.assertNotIn('do-not-disclose', json.dumps(summary))
        self.assertEqual(self.roots(), [])

    def test_nonseal_and_failed_source_attempts_do_not_acquire_diagnostic_rows(self):
        self.execute(self.body(action='ingest'), nonce=False)
        capture = transport.capture
        def failed(*args, **kwargs):
            capture(*args, **kwargs)
            raise OSError('controlled source failure before verification')
        self.entries.append(self.helper.entry('incomplete-answer', 'assistant', 'An incomplete source.'))
        with patch.object(transport, 'capture', side_effect=failed):
            self.execute(self.body(ordinal=2), nonce=False, sequence=2)
        self.assertFalse(self.decision_path().exists())
        self.assertFalse(self.decision_path(2).exists())
        self.assertEqual(turn_sources.read(self.coverage.directory / 'outcomes/2.json', stream.MAX_METADATA)['outcome'], 'unknown')
        summary = self.readback()
        self.assertEqual(summary['rows'], [])
        self.assertEqual(sum(summary['counts'].values()), 0)

    def test_conflicting_existing_record_is_never_rewritten_or_replayed(self):
        path = self.decision_path()
        turn_sources.private_mkdir(path.parent)
        conflict = b'{"arbitrary":"do-not-disclose"}'
        path.write_bytes(conflict)
        path.chmod(0o600)
        retain = stream._retain_wake_decision
        results = []
        def retained(*args):
            result = retain(*args)
            results.append(result)
            return result
        with patch.object(stream, '_retain_wake_decision', side_effect=retained):
            self.offered()
        self.assertEqual(results, ['unavailable'])
        self.assertEqual(path.read_bytes(), conflict)
        self.assertEqual(len(self.roots()), 1)
        self.assert_source()
        summary = self.readback()
        self.assertEqual(summary['counts']['unbound'], 1)
        self.assertNotIn('do-not-disclose', json.dumps(summary))

    def test_optional_writer_failure_after_response_preserves_source_root_and_drain(self):
        exclusive = turn_sources.exclusive
        evidence, calls = {}, []
        def failed(path, value, maximum):
            if 'omp-wake-decisions-v1' in path.parts:
                self.assertTrue(any(kind == 'OBSERVED' for kind, _, _ in self.responses.values))
                evidence.update(self.immutable_bytes())
                calls.append(path)
                raise PermissionError('controlled optional evidence failure')
            return exclusive(path, value, maximum)
        with patch.object(turn_sources, 'exclusive', side_effect=failed):
            self.offered()
        self.assertEqual(len(calls), 1)
        self.assertEqual(len(self.roots()), 1)
        self.assertFalse(self.decision_path().exists())
        self.assertTrue(all(path.read_bytes() == raw for path, raw in evidence.items()))
        self.assert_source()
        self.assertEqual(self.readback()['counts']['unrecorded'], 1)

    def test_failed_observed_send_leaves_decision_unrecorded_and_still_drains(self):
        def failed(value):
            raise BrokenPipeError('controlled OBSERVED send failure')
        self.responses.on_observed = failed
        with self.assertRaises(BrokenPipeError):
            self.execute()
        self.assertFalse(self.decision_path().exists())
        self.assertEqual(self.outcome()['outcome'], 'verified')
        rows, faults = transport.pending_inventory(self.repo)
        self.assertEqual(faults, [])
        self.assertTrue(all('observation_id' in row for row in rows))
        self.assertEqual(self.readback()['counts']['unrecorded'], 1)

    def test_substituted_oversize_response_never_records_an_offered_continuation(self):
        from contracts.test_omp_finite_stream import Framing
        writer = Framing().response_writer()
        writer.instance_id = self.instance
        self.executor.writer = writer
        observe = self.executor.observer
        def oversized(*args, **kwargs):
            result = observe(*args, **kwargs)
            result['faults'] = ['x' * stream.MAX_RESPONSE]
            return result
        reserve = stream._LateNativeDeliveryWindow.reserve
        def calibrated(window):
            self.calibrate()
            return reserve(window)
        with patch.object(self.executor, 'observer', side_effect=oversized), patch.object(stream._LateNativeDeliveryWindow, 'reserve', calibrated), patch.object(stream.time, 'monotonic_ns', return_value=1_000_000):
            self.execute()
        notice = stream.load(writer.queue[-1][1].split(b'\n', 1)[1])
        self.assertEqual(notice['code'], 'result-oversized')
        self.assertNotIn('result', notice)
        self.assertNotIn('continue', notice)
        self.assertFalse(self.decision_path().exists())
        self.assertEqual(len(self.roots()), 1)
        self.assertEqual(self.outcome()['outcome'], 'verified')
        rows, faults = transport.pending_inventory(self.repo)
        self.assertEqual(faults, [])
        self.assertEqual(len(rows), 2)
        self.assertTrue(all('observation_id' in row for row in rows))
        self.assertEqual(self.readback()['counts']['unrecorded'], 1)

    def test_false_observed_send_leaves_decision_unrecorded(self):
        send = self.responses.send
        def rejected(kind, sequence=0, **value):
            accepted = send(kind, sequence, **value)
            return False if kind == 'OBSERVED' else accepted
        with patch.object(self.responses, 'send', side_effect=rejected):
            self.execute()
        self.assertFalse(self.decision_path().exists())
        self.assert_source()
        self.assertEqual(self.readback()['counts']['unrecorded'], 1)

    def _fatal_optional_failure(self, error):
        completions = []
        def failed(*args):
            completions.append(args[-1])
            raise error
        with patch.object(stream, '_retain_wake_decision', side_effect=failed):
            with self.assertRaises(type(error)):
                self.offered()
        self.assert_source()
        self.assertEqual(len(self.roots()), 1)
        self.assertEqual(len(completions), 1)
        self.assertFalse(completions[0]._live)
        self.assertFalse(self.decision_path().exists())

    def test_optional_writer_memory_error_keeps_fatal_policy(self):
        self._fatal_optional_failure(MemoryError('controlled diagnostic allocation fault'))

    def test_optional_writer_recursion_error_keeps_fatal_policy(self):
        self._fatal_optional_failure(RecursionError('controlled diagnostic recursion fault'))

    def test_bounded_summary_orphans_and_unreadable_namespace_do_not_change_doctor_health(self):
        for sequence in range(1, 67):
            self.execute(self.body(armed=False, ordinal=sequence), nonce=False, sequence=sequence)
        self.assertTrue(self.coverage.close({'last_attempt_seq': 66, 'refused_count': 0,
                                            'last_event_ordinal': 66, 'transition': None}))
        inventory = stream.inventory(self.repo)
        baseline = diagnostics.run('doctor', self.repo)
        self.assertTrue(baseline['ok'], baseline['failures'])
        summary = baseline['omp']['wake_decisions']
        self.assertEqual(len(summary['rows']), 64)
        self.assertTrue(summary['truncated'])
        self.assertEqual(summary['counts']['not-armed'], 66)
        orphan = stream._wake_decision_directory(self.coverage.directory).parent / uid()
        turn_sources.private_mkdir(orphan)
        (orphan / 'not-a-diagnostic').write_text('do-not-disclose')
        self.assertEqual(self.readback(), summary)
        optional = self.decision_path().parent
        hidden = optional.with_name('hidden-owned-diagnostics')
        optional.rename(hidden)
        optional.write_bytes(b'do-not-disclose')
        optional.chmod(0o600)
        unavailable = diagnostics.run('doctor', self.repo)
        self.assertTrue(unavailable['ok'], unavailable['failures'])
        self.assertEqual(unavailable['omp']['wake_decisions']['availability'], 'unavailable-or-unverifiable')
        self.assertEqual(unavailable['omp']['wake_decisions']['rows'], [])
        self.assertNotIn('do-not-disclose', json.dumps(unavailable))
        self.assertEqual(stream.inventory(self.repo), inventory)
        self.assertEqual(unavailable['omp']['transport_qualification'], baseline['omp']['transport_qualification'])


class AnswerFirstQualification(unittest.TestCase):
    def test_no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status(self):
        root = Path(__file__).resolve().parents[2]
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, _, _ = fixtures.NativeSources().fixture(world)
            agent = world.inside('no-guidance-native-agent')
            agent.mkdir()
            receipt = json.loads(world.run([
                sys.executable, '-I', '-B', root / 'skills/agent-memory/scripts/installer.py',
                'install', '--development-source', root, '--presets', 'omp', '--omp-agent-dir', agent,
                '--omp-peers-upgraded', '--no-guidance'], timeout=60).stdout)['receipt']
            self.assertIsNone(receipt['omp_binding']['activation_target'])
            for operation in ('status', 'doctor'):
                result = diagnostics.run(operation, repo)
                self.assertTrue(result['ok'], result['failures'])
                self.assertEqual(result['omp']['owned_disk_integrity'], 'verified')
                self.assertEqual(result['omp']['answer_first'], {'activation': 'no_guidance',
                    'automation': 'unqualified', 'native_loading': 'not-observed'})
            self.assertFalse((agent / 'AGENTS.md').exists())


if __name__ == '__main__':
    unittest.main()
