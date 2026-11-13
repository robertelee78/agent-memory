"""Read-coherent native enrollment joins; not native or latency qualification."""
from contextlib import contextmanager
import os
import unittest
from unittest.mock import patch

from world import World
from contracts import test_omp_boundary as fixtures
from agent_memory import scope_binding

transport = fixtures.transport
turn_sources = fixtures.turn_sources


class SegmentInventoryBatch(unittest.TestCase):
    @contextmanager
    def active(self, repo):
        context = scope_binding.private_enrollment_context(repo)
        with turn_sources.source_barrier(repo), scope_binding._private_enrollment_reads(context):
            with scope_binding._observe_enrollment_locations(context):
                yield context

    def segment(self, world, *, registered=False, gap=False, held=False):
        helper = fixtures.NativeSources()
        if held:
            from contracts.test_held_source_enrollment import HeldSourceEnrollment
            _, repo, _, _, _, _, _ = HeldSourceEnrollment().fixture(world)
            fixtures.boundary._activate_marker(repo)
            session = str(fixtures.uuid.uuid4())
        else:
            repo, session, _ = helper.fixture(world)
        human = helper.entry('human', 'user', 'Retained ephemeral arithmetic.', attribution='user')
        lifecycle = str(fixtures.uuid.uuid5(fixtures.uuid.NAMESPACE_URL,
            str(repo.resolve()) + ':omp:' + session + ':' + human['id']))
        directory, cursor = transport.new_segment(repo, session, lifecycle, lifecycle, 'human', 1)
        captured = transport.capture(directory, cursor, human)
        cursor['last_native_entry'] = human['id']
        cursor['control_interval'] = {'human_entry': human['id'], 'capture_order': [human['id']]}
        transport.replace_cursor(directory / 'cursor.json', cursor)
        transport.enroll_admission(directory, cursor, captured)
        if gap:
            transport.gap(directory, cursor, 'missing-assistant', 'missing-stored-source')
        transport.seal(directory, cursor)
        enrollments = [transport.read(path, 4096) for path in
                       transport._files(directory, 'enrollments', r'[0-9a-f]{64}\.json')]
        if registered:
            for enrollment in enrollments:
                observation, faults = transport.registration(repo, directory, cursor, enrollment, perform=True)
                self.assertIsNotNone(observation)
                self.assertEqual(faults, [])
        return helper, repo, directory, cursor, enrollments

    def rows(self, repo, directory, cursor, context):
        return transport.segment_inventory(repo, directory, cursor, enrollment_context=context)

    def attempt_path(self, directory, enrollment):
        return directory / 'registration-attempts' / (transport.digest(enrollment['source_reference']) + '.json')

    def complete_shared(self, repo, cursor, row):
        value = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
        value.update(outcome='no-learning', assessment={'source_id': row['observation_id'],
            'source_review': 'Reviewed retained ephemeral arithmetic and its exact source manifest.', 'candidates': []})
        receipt = turn_sources.verify_assessment(repo, value, row['observation_id'])
        return turn_sources.complete(repo, 'omp', cursor['session'], cursor['lifecycle'],
                                     row['observation_id'], row['source_sha256'], receipt)

    def test_registration_between_row_lookups_cannot_invent_retained_attempt_fault(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, directory, cursor, enrollments = self.segment(world)
            lookup = transport._registration_lookup
            registered = []
            inside = False
            with self.active(repo) as context:
                def register_after_first_lookup(state, enrollment):
                    nonlocal inside
                    result = lookup(state, enrollment)
                    if not inside and not registered:
                        inside = True
                        try:
                            observation, faults = transport.registration(repo, directory, cursor, enrollments[1],
                                perform=True, enrollment_context=context)
                            self.assertEqual(faults, [])
                            registered.append(observation)
                        finally:
                            inside = False
                    return result
                with patch.object(transport, '_registration_lookup', side_effect=register_after_first_lookup):
                    rows = self.rows(repo, directory, cursor, context)
                self.assertEqual({row['status'] for row in rows}, {'enrollment-pending'})
                self.assertTrue(all('faults' not in row and 'observation_id' not in row for row in rows))
                later = {row['source_reference']: row for row in self.rows(repo, directory, cursor, context)}
                self.assertEqual(later[enrollments[1]['source_reference']]['observation_id'], registered[0]['observation_id'])
                self.assertEqual(later[enrollments[1]['source_reference']]['status'], 'pending-assessment')
                self.assertEqual(later[enrollments[0]['source_reference']]['status'], 'enrollment-pending')
            self.assertTrue(self.attempt_path(directory, enrollments[1]).is_file())
            actual = turn_sources.inspect(repo, 'omp', cursor['session'], cursor['lifecycle'])['sources']
            self.assertEqual([source['observation_id'] for source in actual], [registered[0]['observation_id']])

    def test_sampled_attempt_changed_or_deleted_refuses_even_matching_registered_source(self):
        for mutation in ('changed', 'deleted'):
            with self.subTest(mutation=mutation), World() as world, patch.dict(os.environ, world.env, clear=True):
                _, repo, directory, cursor, enrollments = self.segment(world, registered=True)
                target = enrollments[1]
                path = self.attempt_path(directory, target)
                shared_inventory = transport._shared_inventory
                def mutate_after_observation(*args):
                    state = shared_inventory(*args)
                    if mutation == 'deleted':
                        path.unlink()
                    else:
                        value = transport.read(path, 4096)
                        value['source_sha256'] = 'a' * 64
                        path.write_bytes(turn_sources.encoded(value))
                    return state
                with self.active(repo) as context:
                    with patch.object(transport, '_shared_inventory', side_effect=mutate_after_observation):
                        rows = {row['source_reference']: row for row in self.rows(repo, directory, cursor, context)}
                self.assertEqual(rows[target['source_reference']]['faults'], ['source-unavailable'])
                self.assertEqual(rows[target['source_reference']]['status'], 'registration-fault')
                self.assertNotIn('observation_id', rows[target['source_reference']])
                self.assertEqual(rows[enrollments[0]['source_reference']]['status'], 'pending-assessment')
                self.assertEqual(len(turn_sources.inspect(repo, 'omp', cursor['session'], cursor['lifecycle'])['sources']), 2)
                self.assertEqual(path.exists(), mutation != 'deleted')

    def test_namespace_change_between_both_locked_passes_refuses_every_row(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, directory, cursor, _ = self.segment(world, registered=True)
            shared = transport._shared(repo, cursor)[3]
            private_inventory = turn_sources.private_inventory
            changed = False
            def change_between_passes(*args):
                nonlocal changed
                state = private_inventory(*args)
                if not changed:
                    changed = True
                    transport.retain(shared / 'fault.json', {'reason': 'physical-namespace-change'}, 4096)
                return state
            with self.active(repo) as context:
                with patch.object(turn_sources, 'private_inventory', side_effect=change_between_passes):
                    rows = self.rows(repo, directory, cursor, context)
            self.assertEqual(len(rows), 2)
            self.assertTrue(all(row['status'] == 'registration-fault' and
                                row['faults'] == ['source-unavailable'] for row in rows))
            self.assertEqual(transport.read(shared / 'fault.json', 4096)['reason'], 'physical-namespace-change')
            self.assertIn('registration-fault-retained', turn_sources.inspect(repo, 'omp', cursor['session'], cursor['lifecycle'])['faults'])

    def test_observation_failure_is_local_memoized_and_fault_lists_are_independent(self):
        for failure in (OSError('controlled unavailable read'), ValueError('controlled invalid inventory')):
            with self.subTest(failure=type(failure).__name__), World() as world, patch.dict(os.environ, world.env, clear=True):
                _, repo, directory, cursor, _ = self.segment(world, registered=True)
                shared_inventory = transport._shared_inventory
                failed = False
                def fail_once(*args):
                    nonlocal failed
                    if not failed:
                        failed = True
                        raise failure
                    return shared_inventory(*args)
                with self.active(repo) as context:
                    with patch.object(transport, '_shared_inventory', side_effect=fail_once):
                        rows = self.rows(repo, directory, cursor, context)
                    self.assertTrue(all(row['faults'] == ['source-unavailable'] and
                                        row['status'] == 'registration-fault' for row in rows))
                    rows[0]['faults'].append('consumer-local-mutation')
                    self.assertEqual(rows[1]['faults'], ['source-unavailable'])
                    self.assertTrue(all(row['status'] == 'pending-assessment'
                                        for row in self.rows(repo, directory, cursor, context)))

    def test_type_and_key_errors_still_reach_whole_segment_fault_handling(self):
        for failure in (TypeError('controlled invalid state'), KeyError('controlled missing field')):
            with self.subTest(failure=type(failure).__name__), World() as world, patch.dict(os.environ, world.env, clear=True):
                _, repo, _, cursor, _ = self.segment(world)
                with self.active(repo) as context:
                    with patch.object(transport, '_shared_inventory', side_effect=failure):
                        rows, faults = transport.pending_inventory(repo, enrollment_context=context)
                self.assertEqual(faults, ['native-enrollment-unverifiable'])
                self.assertEqual([(row['source_reference'], row['status']) for row in rows],
                                 [('omp:fault:' + cursor['lifecycle'], 'registration-fault')])

    def test_real_completion_between_result_and_wake_requires_new_inventory(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            entries = [helper.entry('human', 'user', 'Ephemeral request.', attribution='user')]
            payload = fixtures.observer_payload(repo, session, entries, terminal=True)
            payload['request_wake'] = True
            wake = transport.wake
            completed_ids = []
            def complete_before_wake(repo, session, lifecycle, wake_payload, **kwargs):
                directory = transport.session_dir(repo, session, enrollment_context=kwargs['enrollment_context']) / lifecycle
                cursor = transport.read(directory / 'cursor.json')
                rows = self.rows(repo, directory, cursor, kwargs['enrollment_context'])
                self.assertEqual(sorted(row['observation_id'] for row in rows), wake_payload['observation_ids'])
                for row in rows:
                    self.assertTrue(self.complete_shared(repo, cursor, row)['verified'])
                    completed_ids.append(row['observation_id'])
                return wake(repo, session, lifecycle, wake_payload, **kwargs)
            with patch.object(transport, 'wake', side_effect=complete_before_wake):
                result = helper.observe_request(repo, {'schema': 'agent-memory/omp-observer-request-v2',
                    'action': 'seal', 'session': session, 'lifecycle': lifecycle, 'payload': payload})
            self.assertTrue(result['verified'])
            self.assertFalse(result['continue'])
            self.assertEqual(result['status'], 'pending-wake-unavailable')
            state = turn_sources.inspect(repo, 'omp', session, result['lifecycle'])
            self.assertEqual(set(completed_ids), {source['observation_id'] for source in state['completed']})
            self.assertTrue(state['complete'])
            self.assertEqual(transport.list_pending(repo)['pending'], [])
            self.assertFalse((transport.session_dir(repo, session) / 'root-wakes').exists())

    def test_empty_segment_does_not_observe_shared_namespace(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            directory, cursor = transport.new_segment(repo, session, lifecycle, lifecycle, 'resume', 0)
            with self.active(repo) as context:
                with patch.object(transport, '_shared_inventory', side_effect=AssertionError('unexpected namespace observation')):
                    rows = self.rows(repo, directory, cursor, context)
                self.assertEqual([row['status'] for row in rows], ['open-segment'])

    def test_all_enrollments_are_validated_before_shared_namespace_observation(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, directory, cursor, enrollments = self.segment(world)
            path = directory / 'enrollments' / (transport.digest(enrollments[-1]['source_reference']) + '.json')
            changed = {**enrollments[-1], 'source_sha256': 'b' * 64}
            path.write_bytes(turn_sources.encoded(changed))
            with self.active(repo) as context:
                with patch.object(transport, '_shared_inventory', side_effect=AssertionError('invalid enrollment observed')):
                    with self.assertRaises(ValueError):
                        self.rows(repo, directory, cursor, context)
            self.assertEqual(transport.read(path, 4096), changed)

    def test_no_context_preserves_fresh_per_reference_observations(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, directory, cursor, enrollments = self.segment(world, registered=True)
            shared = transport._shared(repo, cursor)[3]
            lookup = transport._registration_lookup
            changed = False
            def fault_after_first_lookup(state, enrollment):
                nonlocal changed
                result = lookup(state, enrollment)
                if not changed:
                    changed = True
                    transport.retain(shared / 'fault.json', {'reason': 'late-source-fault'}, 4096)
                return result
            with patch.object(transport, '_registration_lookup', side_effect=fault_after_first_lookup):
                rows = {row['source_reference']: row for row in self.rows(repo, directory, cursor, None)}
            self.assertEqual(rows[enrollments[0]['source_reference']]['status'], 'pending-assessment')
            self.assertEqual(rows[enrollments[1]['source_reference']]['faults'], ['registration-fault-retained'])
            self.assertEqual(rows[enrollments[1]['source_reference']]['status'], 'registration-fault')
            self.assertEqual(transport.read(shared / 'fault.json', 4096)['reason'], 'late-source-fault')

    def test_historical_completed_gate_observation_is_not_reused_for_lookup(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper, repo, directory, cursor, _ = self.segment(world, registered=True)
            rows = self.rows(repo, directory, cursor, None)
            for row in rows:
                self.assertTrue(helper.acknowledge(repo, row)['verified'])
            cursor['control_interval'].pop('human_entry')
            transport.replace_cursor(directory / 'cursor.json', cursor)
            self.assertFalse(transport.source_bound_origin(directory, cursor))
            shared = transport._shared(repo, cursor)[3]
            completion = shared / (rows[0]['observation_id'] + '.done')
            shared_inventory = transport._shared_inventory
            changed = False
            def corrupt_completion_after_gate(*args):
                nonlocal changed
                state = shared_inventory(*args)
                if not changed:
                    changed = True
                    completion.write_bytes(b'{"invalid":"completion"}\n')
                return state
            with self.active(repo) as context:
                with patch.object(transport, '_shared_inventory', side_effect=corrupt_completion_after_gate):
                    joined = self.rows(repo, directory, cursor, context)
                self.assertEqual(len(joined), 2)
                self.assertTrue(all(row['status'] == 'registration-fault' and
                                    'completion-unverifiable' in row['faults'] for row in joined))
                joined[0]['faults'].append('consumer-local-mutation')
                self.assertNotIn('consumer-local-mutation', joined[1]['faults'])
                refused = self.rows(repo, directory, cursor, context)
            self.assertEqual([row['status'] for row in refused], ['legacy-unverified-metadata'])
            self.assertEqual(transport.read(completion), {'invalid': 'completion'})

    def test_distinct_sources_and_orphan_registration_retain_exact_identities(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, directory, cursor, enrollments = self.segment(world, registered=True)
            original = {source['source_reference']: source for source in
                        turn_sources.inspect(repo, 'omp', cursor['session'], cursor['lifecycle'])['sources']}
            orphan = turn_sources.register(repo, 'omp', cursor['session'], cursor['lifecycle'], 'c' * 64,
                native_source_id='retained-orphan', source_reference='omp:controlled-orphan')
            with self.active(repo) as context:
                rows, faults = transport.pending_inventory(repo, enrollment_context=context)
            self.assertEqual(faults, [])
            joined = {row['source_reference']: row for row in rows}
            self.assertEqual(len({item['source_sha256'] for item in enrollments}), 2)
            self.assertEqual(len({source['observation_id'] for source in original.values()}), 2)
            for enrollment in enrollments:
                row = joined[enrollment['source_reference']]
                self.assertEqual(row['source_sha256'], enrollment['source_sha256'])
                self.assertEqual(row['observation_id'], original[enrollment['source_reference']]['observation_id'])
                self.assertEqual(row['status'], 'pending-assessment')
            self.assertEqual(joined['omp:controlled-orphan']['observation_id'], orphan['observation_id'])
            self.assertEqual(joined['omp:controlled-orphan']['source_sha256'], 'c' * 64)
            self.assertEqual(joined['omp:controlled-orphan']['status'], 'orphan-registration')

    def test_duplicate_or_changed_digest_refuses_only_affected_lookup(self):
        for mutation in ('duplicate', 'digest'):
            with self.subTest(mutation=mutation), World() as world, patch.dict(os.environ, world.env, clear=True):
                _, repo, directory, cursor, enrollments = self.segment(world, registered=True)
                shared = transport._shared(repo, cursor)[3]
                target = enrollments[0]
                paths = sorted(shared.glob('source-*.json'))
                source_path = next(path for path in paths if transport.read(path, 4096)['source_reference'] == target['source_reference'])
                value = transport.read(source_path, 4096)
                if mutation == 'duplicate':
                    turn_sources.exclusive(shared / 'source-2.json', value, turn_sources.MAX_SOURCE)
                else:
                    value['source_sha256'] = 'd' * 64
                    source_path.write_bytes(turn_sources.encoded(value))
                with self.active(repo) as context:
                    rows = {row['source_reference']: row for row in self.rows(repo, directory, cursor, context)}
                self.assertEqual(rows[target['source_reference']]['faults'], ['source-unavailable'])
                self.assertEqual(rows[target['source_reference']]['status'], 'registration-fault')
                self.assertEqual(rows[enrollments[1]['source_reference']]['status'], 'pending-assessment')
                self.assertEqual(transport.read(source_path, 4096), value)

    def test_missing_corrupt_source_and_completion_faults_remain_visible(self):
        for mutation in ('missing-source', 'corrupt-source', 'corrupt-completion'):
            with self.subTest(mutation=mutation), World() as world, patch.dict(os.environ, world.env, clear=True):
                _, repo, directory, cursor, enrollments = self.segment(world, registered=True,
                    held=mutation == 'missing-source')
                shared = transport._shared(repo, cursor)[3]
                target = enrollments[0]
                source_path = next(path for path in shared.glob('source-*.json')
                                   if transport.read(path, 4096)['source_reference'] == target['source_reference'])
                source = transport.read(source_path, 4096)
                if mutation == 'missing-source':
                    source_path.unlink()
                    expected_fault = 'enrollment-source-missing'
                elif mutation == 'corrupt-source':
                    source_path.write_bytes(b'{"invalid":"source"}\n')
                    expected_fault = 'source-reservation-unverifiable'
                else:
                    transport.retain(shared / (source['observation_id'] + '.done'), {'invalid': 'completion'})
                    expected_fault = 'completion-unverifiable'
                with self.active(repo) as context:
                    rows = self.rows(repo, directory, cursor, context)
                self.assertEqual(len(rows), 2)
                self.assertTrue(all(row['status'] == 'registration-fault' and expected_fault in row['faults']
                                    for row in rows))
                rows[0]['faults'].append('consumer-local-mutation')
                self.assertNotIn('consumer-local-mutation', rows[1]['faults'])
                current = turn_sources.inspect(repo, 'omp', cursor['session'], cursor['lifecycle'])
                self.assertIn(expected_fault, current['faults'])
                self.assertFalse(current['complete'])

    def test_ordinary_missing_source_retains_only_its_own_registration_fault(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, directory, cursor, enrollments = self.segment(world, registered=True)
            shared = transport._shared(repo, cursor)[3]
            target = enrollments[0]
            source_path = next(path for path in shared.glob('source-*.json')
                               if transport.read(path, 4096)['source_reference'] == target['source_reference'])
            source_path.unlink()
            with self.active(repo) as context:
                rows = {row['source_reference']: row for row in self.rows(repo, directory, cursor, context)}
            self.assertEqual(rows[target['source_reference']]['status'], 'registration-fault')
            self.assertEqual(rows[target['source_reference']]['faults'], ['registration-fault-retained'])
            other = rows[enrollments[1]['source_reference']]
            self.assertEqual(other['status'], 'pending-assessment')
            self.assertNotIn('faults', other)
            current = turn_sources.inspect(repo, 'omp', cursor['session'], cursor['lifecycle'])
            self.assertEqual(current['faults'], [])
            self.assertFalse(current['complete'])
            self.assertEqual([value['observation_id'] for value in current['sources']],
                             [other['observation_id']])

    def test_missing_and_corrupt_attempts_are_not_confused_with_new_attempts(self):
        for mutation in ('unknown', 'corrupt', 'foreign-reference'):
            with self.subTest(mutation=mutation), World() as world, patch.dict(os.environ, world.env, clear=True):
                _, repo, directory, cursor, enrollments = self.segment(world)
                target = enrollments[0]
                path = self.attempt_path(directory, target)
                value = {'schema': 'agent-memory/omp-registration-attempt-v1', 'session': cursor['session'],
                    'lifecycle': cursor['lifecycle'], 'source_reference': target['source_reference'],
                    'source_sha256': target['source_sha256']}
                if mutation == 'foreign-reference':
                    value['source_reference'] = 'omp:foreign-source'
                transport.retain(path, value, 4096)
                if mutation == 'corrupt':
                    path.write_bytes(b'{invalid-json\n')
                with self.active(repo) as context:
                    if mutation == 'unknown':
                        rows = {row['source_reference']: row for row in self.rows(repo, directory, cursor, context)}
                        self.assertEqual(rows[target['source_reference']]['faults'], ['registration-fault-retained'])
                        self.assertEqual(rows[enrollments[1]['source_reference']]['status'], 'enrollment-pending')
                    else:
                        with self.assertRaises(ValueError):
                            self.rows(repo, directory, cursor, context)
                self.assertTrue(path.is_file())
                self.assertFalse(turn_sources.inspect(repo, 'omp', cursor['session'], cursor['lifecycle'])['sources'])

    def test_held_and_gapped_segments_do_not_gain_completion_authority(self):
        for held, gap in ((True, False), (False, True)):
            with self.subTest(held=held, gap=gap), World() as world, patch.dict(os.environ, world.env, clear=True):
                helper, repo, directory, cursor, enrollments = self.segment(world, registered=True, held=held, gap=gap)
                shared = transport._shared(repo, cursor)[3]
                before = {path.name: path.read_bytes() for path in shared.glob('source-*.json')}
                with self.active(repo) as context:
                    self.assertEqual(context.held, held)
                    rows = self.rows(repo, directory, cursor, context)
                self.assertEqual(len(rows), len(enrollments))
                self.assertTrue(all(row['status'] == ('pending-coverage-gap' if gap else 'pending-assessment') for row in rows))
                for row in rows:
                    with self.assertRaises(ValueError):
                        helper.acknowledge(repo, row)
                self.assertEqual({path.name: path.read_bytes() for path in shared.glob('source-*.json')}, before)
                self.assertEqual(list(shared.glob('*.done')), [])


if __name__ == '__main__':
    unittest.main()
