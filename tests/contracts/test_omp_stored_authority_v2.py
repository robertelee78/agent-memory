"""Controlled stored-source authority contracts; not native/provider qualification."""
import copy
import hashlib
import os
import unittest
import uuid
from unittest.mock import Mock, patch

from world import World
from contracts import test_omp_boundary as fixtures

transport = fixtures.transport
projection = fixtures.omp_projection
boundary = fixtures.boundary


class NativeStoredAuthorityV2(unittest.TestCase):
    def request(self, repo, session, lifecycle, entries, *, action='ingest', terminal=False):
        payload = fixtures.observer_payload(repo, session, entries, terminal=terminal)
        if action == 'seal':
            payload['request_wake'] = False
        return {'schema': 'agent-memory/omp-observer-request-v2', 'action': action,
                'session': session, 'lifecycle': lifecycle, 'payload': payload}

    def neutral_hint(self, world, repo, session, lifecycle, *, leaf=None, receipt=None):
        receipt = receipt or fixtures.OwnedInstructionBoundary().installed(world)[0]
        value = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'hint',
                 'session': session, 'lifecycle': lifecycle,
                 'payload': {'session': session, 'header_id': session, 'cwd': str(repo), 'leaf_id': leaf}}
        result = fixtures.observe_installed(world, fixtures.Path(receipt['bin_dir']) / 'am', repo, receipt, value)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return fixtures.json.loads(result.stdout), receipt

    def test_notification_without_stored_human_has_no_origin_or_source_gap(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            value = self.request(repo, session, lifecycle, [])
            result, _ = self.neutral_hint(world, repo, session, lifecycle)
            self.assertTrue(result['verified'])
            self.assertEqual(transport.segments(repo, session), [])
            self.assertEqual(transport.pending_inventory(repo), ([], []))
            value.update(schema='agent-memory/omp-observer-request-v1', action='admit', payload={
                'schema': 'agent-memory/omp-admission-v1', 'session': session, 'lifecycle': lifecycle,
                'capture_id': str(uuid.uuid4()), 'event_ordinal': 1, 'role': 'user',
                'attribution': 'user', 'tool_call_ids': []})
            with self.assertRaises(ValueError):
                helper.observe_request(repo, value)
            self.assertEqual(transport.segments(repo, session), [])

    def test_checked_stored_user_is_sole_genuine_root_and_mapping_is_not_required(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            entries = [helper.entry('actual-human', 'user', 'Only ephemeral arithmetic.', attribution='user'),
                       helper.entry('actual-answer', 'assistant', 'The ephemeral answer.')]
            helper.observe_request(repo, self.request(repo, session, lifecycle, entries, action='seal'))
            current = transport.segments(repo, session)
            self.assertEqual(len(current), 1)
            directory, cursor = current[0]
            expected = str(uuid.uuid5(uuid.NAMESPACE_URL, str(repo.resolve()) + ':omp:' + session + ':actual-human'))
            self.assertEqual(cursor['origin']['human_root'], expected)
            self.assertEqual(cursor['control_interval']['human_entry'], 'actual-human')
            self.assertFalse((directory / 'admissions').exists())
            self.assertEqual(transport.read(directory / 'segment.json')['gaps'], [])
            rows, faults = transport.pending_inventory(repo)
            self.assertEqual(faults, [])
            for row in rows:
                self.assertTrue(helper.acknowledge(repo, row)['verified'])
            self.assertEqual(transport.list_pending(repo)['pending'], [])

    def test_delayed_old_checked_view_refuses_new_session_and_real_new_human_has_one_root(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, old, lifecycle = helper.fixture(world)
            human = helper.entry('old-native-human', 'user', 'Old real source.', attribution='user')
            helper.observe(repo, old, lifecycle, [human])
            directory, _ = transport.segments(repo, old)[0]
            old_metadata = {'schema': 'agent-memory/omp-admission-v1', 'session': old, 'lifecycle': lifecycle,
                'capture_id': str(uuid.uuid4()), 'event_ordinal': 1, 'role': 'user', 'attribution': 'user', 'tool_call_ids': []}
            metadata_path = directory / 'admissions' / (old_metadata['capture_id'] + '.json')
            transport.retain(metadata_path, {**old_metadata, 'lifecycle': directory.name}, 4096)
            metadata_bytes = metadata_path.read_bytes(); manifest = (directory / 'segment.json').read_bytes()
            new = str(uuid.uuid4()); new_lifecycle = str(uuid.uuid4())
            delayed = self.request(repo, old, lifecycle, [human])
            delayed['session'] = new
            delayed['lifecycle'] = new_lifecycle
            started = []
            with self.assertRaises(ValueError):
                helper.observe_request(repo, delayed, on_started=lambda: started.append(True))
            self.assertEqual(started, [])
            self.assertEqual(transport.segments(repo, new), [])
            helper.observe_request(repo, self.request(repo, new, new_lifecycle, []))
            self.assertEqual(transport.segments(repo, new), [])
            self.assertEqual(metadata_path.read_bytes(), metadata_bytes)
            self.assertEqual((directory / 'segment.json').read_bytes(), manifest)
            actual = helper.entry('new-native-human', 'user', 'Actual new session source.', attribution='user')
            helper.observe(repo, new, new_lifecycle, [actual])
            roots = [cursor for _, cursor in transport.segments(repo, new)
                     if cursor['origin']['kind'] == 'genuine-user']
            self.assertEqual(len(roots), 1)
            self.assertEqual(roots[0]['control_interval']['human_entry'], 'new-native-human')
            self.assertEqual((directory / 'segment.json').read_bytes(), manifest)

    def test_changed_header_branch_and_parent_graph_refuse_before_transaction(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            original = self.request(repo, session, lifecycle, [helper.entry('human', 'user', 'Actual source.', attribution='user')])
            mutations = [lambda v: v['payload']['view']['header'].update(id='another-session'),
                         lambda v: v['payload'].update(branch_ids=[]),
                         lambda v: v['payload']['view']['native_parents'][0].update(parent_id='missing')]
            for mutate in mutations:
                value = copy.deepcopy(original); mutate(value); started = []
                with self.assertRaises(ValueError):
                    helper.observe_request(repo, value, on_started=lambda: started.append(True))
                self.assertEqual(started, [])
            self.assertEqual(transport.segments(repo, session), [])

    def test_legacy_notification_only_cursor_is_neither_reused_mutated_nor_sealed(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, phantom = helper.fixture(world)
            directory, cursor = transport.new_segment(repo, session, phantom, phantom, 'human', 50)
            original = (directory / 'cursor.json').read_bytes()
            actual = helper.entry('actual-native-human', 'user', 'Actual stored source.', attribution='user')
            helper.observe(repo, session, phantom, [actual], terminal=True)
            self.assertEqual((directory / 'cursor.json').read_bytes(), original)
            self.assertFalse((directory / 'segment.json').exists())
            chosen = transport.active_segment(transport.segments(repo, session), ['actual-native-human'])
            self.assertNotEqual(chosen[1]['lifecycle'], phantom)
            self.assertTrue(any(row['status'] == 'legacy-unverified-metadata' for row in transport.pending_inventory(repo)[0]))

    def test_weak_historical_event_resolution_does_not_authorize_new_acknowledgement(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            helper.observe(repo, session, lifecycle, [helper.entry('human', 'user', 'Actual source.', attribution='user'),
                helper.entry('answer', 'assistant', 'Actual answer.')], seal=False)
            directory, cursor = transport.segments(repo, session)[0]
            event = str(uuid.uuid4())
            cursor['control_interval']['metadata_matches'] = {event: 'answer'}
            transport.replace_cursor(directory / 'cursor.json', cursor)
            missing = transport.gap(directory, cursor, 'event:' + event, 'missing-stored-source')
            manifest = transport.seal(directory, cursor)
            reference = 'snapshots/' + hashlib.sha256(b'answer').hexdigest() + '.json'
            snapshot = transport.read(directory / reference)
            transport.retain(directory / 'resolutions' / (missing['gap_id'] + '.json'), {
                'schema': 'agent-memory/omp-gap-resolution-v1', 'manifest_sha256': transport.digest(manifest),
                'gap_id': missing['gap_id'], 'recovered_ref': reference,
                'recovered_sha256': transport.digest(snapshot), 'reason': 'verified-native-reassociation'}, 4096)
            self.assertTrue(transport._coverage(directory, manifest, historical=True))
            self.assertFalse(transport._coverage(directory, manifest))
            helper.observe(repo, session, lifecycle, [], seal=False)
            rows = [row for row in transport.pending_inventory(repo)[0] if 'observation_id' in row]
            self.assertEqual(len(rows), 2)
            self.assertTrue(all(row['status'] == 'pending-coverage-gap' for row in rows))
            for row in rows:
                with self.assertRaisesRegex(ValueError, 'unresolved coverage gaps'):
                    helper.acknowledge(repo, row)

    def test_neutral_hint_keeps_sealed_parent_immutable(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Actual source.', attribution='user')
            helper.observe(repo, session, lifecycle, [human])
            directory, _ = transport.segments(repo, session)[0]
            original = (directory / 'segment.json').read_bytes()
            helper.observe_request(repo, self.request(repo, session, lifecycle,
                [human, helper.entry('answer', 'assistant', 'Later actual stored answer.')]))
            self.assertEqual((directory / 'segment.json').read_bytes(), original)
            self.assertEqual(len(transport.segments(repo, session)), 2)

    def test_expired_internal_delivery_window_never_reserves_root_wake(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            result = helper.observe(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual source.', attribution='user')])
            value = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake',
                     'session': session, 'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}
            refused = helper.observe_request(repo, value, delivery_window=fixtures.LiveDeliveryWindow(expired=True))
            self.assertTrue(refused['verified']); self.assertFalse(refused['continue'])
            self.assertFalse((transport.session_dir(repo, session) / 'root-wakes').exists())
            self.assertTrue(helper.observe_request(repo, value, delivery_window=fixtures.LiveDeliveryWindow())['continue'])
            self.assertFalse(helper.observe_request(repo, value, delivery_window=fixtures.LiveDeliveryWindow())['continue'])

    def test_cancel_before_reservation_refuses_and_after_start_preserves_result(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            result = helper.observe(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual source.', attribution='user')])
            value = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake',
                     'session': session, 'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}
            cancelled = fixtures.LiveDeliveryWindow()
            cancelled._owner.revoke()
            refused = helper.observe_request(repo, value, delivery_window=cancelled)
            self.assertTrue(refused['verified'])
            self.assertFalse(refused['continue'])
            self.assertEqual(refused['status'], 'pending-undeliverable-window')
            self.assertFalse((transport.session_dir(repo, session) / 'root-wakes').exists())

            window = fixtures.LiveDeliveryWindow()
            retain = transport.retain
            def cancel_after_start(path, payload, bound):
                if path.parent.name == 'root-wakes':
                    window._owner.control('CANCEL', {'nonce': window.nonce}, None)
                return retain(path, payload, bound)
            with patch.object(transport, 'retain', side_effect=cancel_after_start):
                accepted = helper.observe_request(repo, value, delivery_window=window)
            self.assertTrue(accepted['verified'])
            self.assertTrue(accepted['continue'])
            wakes = transport.session_dir(repo, session) / 'root-wakes'
            self.assertEqual(len(list(wakes.glob('*.json'))), 1)
            repeated = helper.observe_request(repo, value, delivery_window=fixtures.LiveDeliveryWindow())
            self.assertTrue(repeated['verified'])
            self.assertFalse(repeated['continue'])
            self.assertEqual(len(list(wakes.glob('*.json'))), 1)

    def test_replacement_and_end_after_start_preserve_result(self):
        for control in ('replacement', 'end'):
            with self.subTest(control=control), World() as world, patch.dict(os.environ, world.env, clear=True):
                helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
                result = helper.observe(repo, session, lifecycle,
                    [helper.entry('human', 'user', 'Actual source.', attribution='user')])
                value = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake',
                         'session': session, 'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}
                window = fixtures.LiveDeliveryWindow()
                retain = transport.retain
                applied = []
                def control_after_start(path, payload, bound):
                    if path.parent.name == 'root-wakes':
                        applied.append(control)
                        if control == 'end':
                            window._owner.end()
                        else:
                            window._owner.control('CLOCK', {'nonce': str(uuid.uuid4()),
                                'j0': '100', 'j_deadline': '950000100'}, Mock())
                    return retain(path, payload, bound)
                with patch.object(transport, 'retain', side_effect=control_after_start):
                    accepted = helper.observe_request(repo, value, delivery_window=window)
                self.assertEqual(applied, [control])
                self.assertTrue(accepted['verified'])
                self.assertTrue(accepted['continue'])
                wakes = transport.session_dir(repo, session) / 'root-wakes'
                self.assertEqual(len(list(wakes.glob('*.json'))), 1)
                repeated = helper.observe_request(repo, value, delivery_window=fixtures.LiveDeliveryWindow())
                self.assertTrue(repeated['verified'])
                self.assertFalse(repeated['continue'])
                self.assertEqual(len(list(wakes.glob('*.json'))), 1)

    def test_reparented_linux_window_cannot_reserve(self):
        import threading
        from agent_memory import omp_stream
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            result = helper.observe(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual source.', attribution='user')])
            value = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake',
                     'session': session, 'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}
            window = fixtures.LiveDeliveryWindow()
            image = omp_stream._LinuxNativeImage.__new__(omp_stream._LinuxNativeImage)
            image.lock = threading.Lock()
            image.closed = False
            image.ready = (123, 124, os.getppid() + 1, 1, (), ())
            window._platform = 'linux'
            window._owner.native_image = image
            with patch.object(omp_stream.select, 'select', return_value=([], [], [])):
                refused = helper.observe_request(repo, value, delivery_window=window)
            self.assertTrue(refused['verified'])
            self.assertFalse(refused['continue'])
            self.assertEqual(refused['status'], 'pending-undeliverable-window')
            self.assertFalse((transport.session_dir(repo, session) / 'root-wakes').exists())

    def test_on_started_runs_once_after_preflight_before_source_write(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            calls = []
            def started():
                calls.append(True)
                self.assertFalse(transport.session_dir(repo, session).exists())
            result = helper.observe_request(repo, self.request(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual source.', attribution='user')]), on_started=started)
            self.assertTrue(result['verified']); self.assertEqual(calls, [True])

    def test_active_unavailable_tail_is_real_gap_not_a_notification_gap(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            value = self.request(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual source.', attribution='user')], action='ingest', terminal=True)
            value['payload']['view']['tail_status'] = 'active-unavailable'
            helper.observe_request(repo, value)
            directory, _ = transport.segments(repo, session)[0]
            gaps = transport.read(directory / 'segment.json')['gaps']
            self.assertTrue(any(gap['native_entry_id'] is None and gap['reason'] == 'ambiguous-lifecycle' for gap in gaps))
            self.assertFalse(any(str(gap['native_entry_id']).startswith('event:') for gap in gaps))

    def test_retired_persistence_truncation_is_not_literal_complete_learning(self):
        entry = fixtures.NativeSources().entry('native', 'assistant',
            'Synthetic visible prefix.' + projection.PERSISTENCE_TRUNCATION_NOTICE)
        self.assertEqual(projection.project(entry), {'gap': 'oversized-source'})
        self.assertEqual(projection.project(entry, persisted=True), {'gap': 'oversized-source'})

    def test_retired_orphan_snapshot_reuses_original_safe_payload_by_native_identity(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Actual stored task.', attribution='user')
            helper.observe(repo, session, lifecycle, [human], seal=False)
            directory, cursor = transport.segments(repo, session)[0]
            original = helper.entry('answer', 'assistant', 'Original complete safe stored answer.', parent='human')
            snapshot = projection.project(original)['snapshot']
            reference = 'snapshots/' + hashlib.sha256(b'answer').hexdigest() + '.json'
            transport.retain(directory / reference, snapshot)
            before = (directory / reference).read_bytes()
            persisted = copy.deepcopy(original)
            persisted['message']['content'] = 'Persisted prefix.' + projection.PERSISTENCE_TRUNCATION_NOTICE
            captured = transport.capture(directory, cursor, persisted, persisted=True)
            self.assertEqual(captured['snapshot_sha256'], transport.digest(snapshot))
            self.assertEqual((directory / reference).read_bytes(), before)

    def test_retired_new_truncated_entry_keeps_gap_and_prevents_complete_source_claim(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Actual stored task.', attribution='user')
            helper.observe(repo, session, lifecycle, [human], seal=False)
            tail = helper.entry('tail', 'assistant', 'Persisted prefix.' + projection.PERSISTENCE_TRUNCATION_NOTICE)
            value = self.request(repo, session, lifecycle, [human, tail], action='ingest', terminal=True)
            value['payload']['view']['source_kind'] = 'retired-file'
            value['payload']['view']['file'] = str(repo / 'explicit-retired-fixture.jsonl')
            result = helper.observe_request(repo, value)
            self.assertTrue(result['verified'])
            self.assertFalse(result['source_coverage_complete'])
            self.assertTrue(any(row['status'] == 'pending-coverage-gap' for row in transport.pending_inventory(repo)[0]))

    def test_missing_or_stale_receipt_is_not_a_production_fallback(self):
        with self.assertRaises(ValueError):
            transport._verify_observer_receipt(None)

    def test_random_legacy_root_with_real_reconciled_human_still_cannot_acknowledge(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, random_root = helper.fixture(world)
            directory, cursor = transport.new_segment(repo, session, random_root, random_root, 'human', 1)
            human = helper.entry('real-later-human', 'user', 'Real content later matched to old metadata.', attribution='user')
            captured = transport.capture(directory, cursor, human)
            event = str(uuid.uuid4())
            cursor['control_interval'] = {'human_entry': human['id'], 'capture_order': [human['id']],
                                          'metadata_matches': {event: human['id']}}
            transport.replace_cursor(directory / 'cursor.json', cursor)
            manifest = {'schema': 'agent-memory/omp-segment-v1', 'session': session, 'lifecycle': random_root,
                        'origin': cursor['origin'], 'seal_id': str(uuid.uuid4()), 'admission_ref': captured['snapshot_ref'],
                        'captures': [{'ref': captured['snapshot_ref'], 'sha256': captured['snapshot_sha256']}],
                        'gaps': [], 'exclusions': list(projection.EXCLUSIONS), 'previous_digest': None}
            transport.retain(directory / 'segment.json', manifest)
            transport._enroll(directory, cursor, 'omp:capture:' + captured['capture_id'],
                              transport.read(directory / captured['snapshot_ref']), 'admission', human['id'])
            transport._enroll(directory, cursor, 'omp:seal:' + manifest['seal_id'], manifest, 'segment', manifest['seal_id'])
            registered = []
            for path in transport._files(directory, 'enrollments', r'[0-9a-f]{64}\.json'):
                enrollment = transport.read(path, 4096)
                registered.append(fixtures.turn_sources.register(repo, 'omp', session, random_root,
                    enrollment['source_sha256'], native_source_id=enrollment['native_source_id'],
                    source_reference=enrollment['source_reference']))
            self.assertEqual(len(registered), 2)
            child_id = str(uuid.uuid4()); child_directory = directory.parent / child_id
            child = {'schema': 'agent-memory/omp-cursor-v1', 'session': session, 'lifecycle': child_id,
                     'origin': {'kind': 'foreign-continuation', 'human_root': random_root, 'native_trigger': 'foreign-hidden'},
                     'last_native_entry': None, 'event_ordinal': 2, 'control_interval': None, 'tool_lifecycles': {}}
            transport.retain(child_directory / 'cursor.json', child)
            assistant = helper.entry('legacy-child-answer', 'assistant', 'Real child content under an old phantom family.')
            transport.capture(child_directory, child, assistant)
            child_before = {str(path.relative_to(child_directory)): path.read_bytes() for path in child_directory.rglob('*.json')}
            self.assertFalse(transport.source_bound_origin(child_directory, child))
            before = {str(path.relative_to(directory)): path.read_bytes() for path in directory.rglob('*.json')}
            self.assertFalse(transport.source_bound_origin(directory, cursor))
            rows, faults = transport.pending_inventory(repo)
            self.assertEqual(faults, [])
            self.assertTrue(any(row['status'] == 'legacy-unverified-metadata' for row in rows))
            for observation in registered:
                with self.assertRaisesRegex(ValueError, 'independently stored root origin'):
                    helper.acknowledge(repo, observation)
                with self.assertRaisesRegex(ValueError, 'independently stored root origin'):
                    transport.source(repo, session, random_root, observation['observation_id'])
            with self.assertRaisesRegex(ValueError, 'unbound native root family'):
                transport.new_segment(repo, session, str(uuid.uuid4()), random_root, 'foreign-hidden', 2)
            self.assertEqual(before, {str(path.relative_to(directory)): path.read_bytes() for path in directory.rglob('*.json')})
            self.assertEqual(transport.native_disposition_ids(repo, session), set())
            helper.observe(repo, session, str(uuid.uuid4()), [human, assistant], terminal=True)
            self.assertEqual(child_before, {str(path.relative_to(child_directory)): path.read_bytes() for path in child_directory.rglob('*.json')})
            self.assertEqual(before, {str(path.relative_to(directory)): path.read_bytes() for path in directory.rglob('*.json')})
            chosen = transport.active_segment(transport.segments(repo, session), [human['id'], assistant['id']])
            self.assertNotEqual(chosen[1]['origin']['human_root'], random_root)

    def test_completed_historical_origin_exact_replay_is_read_only_and_changed_request_refuses(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            helper.observe(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual old source.', attribution='user')])
            rows, faults = transport.pending_inventory(repo)
            self.assertEqual(faults, []); self.assertEqual(len(rows), 2)
            for row in rows:
                helper.acknowledge(repo, row)
            directory, cursor = transport.segments(repo, session)[0]
            cursor['control_interval'].pop('human_entry')
            transport.replace_cursor(directory / 'cursor.json', cursor)
            row = rows[0]
            value = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
            value.update(outcome='no-learning', assessment={'source_id': row['observation_id'],
                'source_review': 'Reviewed the actual ephemeral transport fixture; no durable project learning.', 'candidates': []})
            base = boundary._base(repo)
            before = {str(path.relative_to(base)): path.read_bytes() for path in base.rglob('*') if path.is_file()}
            result = transport.complete(repo, value)
            self.assertTrue(result['already_completed'])
            self.assertEqual(result['origin_qualification'], 'historical-unverified-native-origin')
            changed = copy.deepcopy(value); changed['assessment']['source_review'] = 'A different new assertion.'
            with self.assertRaisesRegex(ValueError, 'independently stored root origin'):
                transport.complete(repo, changed)
            self.assertEqual(before, {str(path.relative_to(base)): path.read_bytes() for path in base.rglob('*') if path.is_file()})

    def test_same_role_position_decoy_never_resolves_real_event_gap(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Actual request.', attribution='user')
            helper.observe(repo, session, lifecycle, [human], seal=False)
            directory, cursor = transport.segments(repo, session)[0]
            event = str(uuid.uuid4())
            transport.retain(directory / 'admissions' / (event + '.json'), {
                'schema': 'agent-memory/omp-admission-v1', 'session': session, 'lifecycle': directory.name,
                'capture_id': event, 'event_ordinal': 2, 'role': 'assistant', 'attribution': 'agent', 'tool_call_ids': []}, 4096)
            missing = transport.gap(directory, cursor, 'event:' + event, 'missing-stored-source')
            transport.seal(directory, cursor)
            manifest = (directory / 'segment.json').read_bytes()
            helper.observe(repo, session, lifecycle,
                [human, helper.entry('position-decoy', 'assistant', 'Same role and adjacent position are not event identity.')],
                terminal=True)
            self.assertFalse((directory / 'resolutions' / (missing['gap_id'] + '.json')).exists())
            self.assertEqual((directory / 'segment.json').read_bytes(), manifest)
            rows = [row for row in transport.pending_inventory(repo)[0]
                    if row['lifecycle'] == directory.name and 'observation_id' in row]
            self.assertEqual(len(rows), 2)
            self.assertTrue(all(row['status'] == 'pending-coverage-gap' for row in rows))

    def test_retired_view_closed_identity_negatives_refuse_before_source_writes(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            original = self.request(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual request.', attribution='user')])
            original['payload']['view'].update(source_kind='retired-file', file=str(repo / 'owned-native.jsonl'))
            mutations = {
                'duplicate': lambda v: v['view']['native_parents'].append(copy.deepcopy(v['view']['native_parents'][0])),
                'cycle': lambda v: (v['view']['native_parents'][0].update(parent_id='human'), v['entries'][0].update(parentId='human')),
                'leaf': lambda v: v['view'].update(leaf_id='missing'),
                'cwd': lambda v: v['view'].update(cwd=str(repo.parent)),
                'version': lambda v: v['view']['header'].update(version=99),
                'header': lambda v: v['view']['header'].update(id='wrong-session'),
                'file': lambda v: v['view'].update(file=None),
                'tail': lambda v: v['view'].update(tail_status='active-unavailable'),
                'message': lambda v: v['entries'][0].update(message=None),
                'reserved-native': lambda v: (v['view']['native_parents'][0].update(id='tool:collision'),
                    v['entries'][0].update(id='tool:collision'), v['view'].update(leaf_id='tool:collision'),
                    v.update(branch_ids=['tool:collision'])),
            }
            for name, mutation in mutations.items():
                with self.subTest(name=name):
                    value = copy.deepcopy(original); mutation(value['payload']); started = []
                    with self.assertRaises(ValueError):
                        helper.observe_request(repo, value, on_started=lambda: started.append(True))
                    self.assertEqual(started, [])
            self.assertEqual(transport.segments(repo, session), [])

    def test_resumed_fork_postfork_human_is_not_blanket_inherited(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            entries = [helper.entry('inherited-human', 'user', 'Old fork context.', attribution='user'),
                       helper.entry('postfork-human', 'user', 'New actual task after fork.', attribution='user')]
            value = self.request(repo, session, lifecycle, entries, action='seal')
            view = value['payload']['view']
            view['header'].update(parent_session='actual-parent', timestamp='2026-11-08T01:00:00.000Z')
            view['native_parents'][0]['timestamp'] = '2026-11-08T00:59:00.000Z'
            view['native_parents'][1]['timestamp'] = '2026-11-08T01:01:00.000Z'
            view['fork_status'] = 'verified'; value['payload']['inherited_ids'] = ['inherited-human']
            helper.observe_request(repo, value)
            roots = [cursor for _, cursor in transport.segments(repo, session) if cursor['origin']['kind'] == 'genuine-user']
            self.assertEqual(len(roots), 1)
            self.assertEqual(roots[0]['control_interval']['human_entry'], 'postfork-human')

    def test_fork_without_time_evidence_retains_nonhuman_source_with_explicit_gap(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            value = self.request(repo, session, lifecycle,
                [helper.entry('uncertain-human', 'user', 'Real available stored text.', attribution='user')], terminal=True)
            value['payload']['view']['header']['parent_session'] = 'actual-parent'
            value['payload']['view']['fork_status'] = 'unavailable'
            helper.observe_request(repo, value)
            directory, cursor = transport.segments(repo, session)[0]
            self.assertEqual(cursor['origin']['kind'], 'foreign-continuation')
            self.assertTrue((directory / 'captures' / (hashlib.sha256(b'uncertain-human').hexdigest() + '.json')).exists())
            self.assertTrue(any(gap['reason'] == 'ambiguous-lifecycle' for gap in transport.read(directory / 'segment.json')['gaps']))

    def test_resolved_sealed_tool_gap_does_not_leave_stale_tool_pending_or_block_drain(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Actual short request.', attribution='user')
            call = helper.entry('call', 'assistant', [{'type': 'toolCall', 'id': 'unique-call', 'name': 'read', 'arguments': {'path': 'synthetic.txt'}}])
            helper.observe(repo, session, lifecycle, [human, call])
            original, _ = transport.segments(repo, session)[0]
            sealed = (original / 'segment.json').read_bytes(); cursor = (original / 'cursor.json').read_bytes()
            result = helper.entry('result', 'toolResult', 'Actual safe result.', toolCallId='unique-call', toolName='read', isError=False)
            helper.observe(repo, session, lifecycle, [human, call, result], seal=False, terminal=True)
            self.assertEqual((original / 'segment.json').read_bytes(), sealed)
            self.assertEqual((original / 'cursor.json').read_bytes(), cursor)
            rows, faults = transport.pending_inventory(repo)
            self.assertEqual(faults, [])
            self.assertFalse(any(row['status'] in {'pending-native-tool', 'pending-coverage-gap'} for row in rows))
            self.assertGreaterEqual(len([row for row in rows if 'observation_id' in row]), 2)
            for row in rows:
                helper.acknowledge(repo, row)
            self.assertEqual(transport.list_pending(repo)['pending'], [])
            self.assertTrue(boundary.drain_source_capabilities(repo, quiesced=True)['drainable'])

    def test_foreign_tool_call_id_cannot_resolve_actual_sealed_tool_gap(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Actual request.', attribution='user')
            call = helper.entry('call', 'assistant', [{'type': 'toolCall', 'id': 'owned-call',
                'name': 'read', 'arguments': {'path': 'synthetic.txt'}}])
            helper.observe(repo, session, lifecycle, [human, call])
            original, _ = transport.segments(repo, session)[0]
            manifest = (original / 'segment.json').read_bytes()
            missing = next(item for item in transport.read(original / 'segment.json')['gaps']
                           if item['native_entry_id'] == 'tool:owned-call')
            foreign = helper.entry('foreign-result', 'toolResult', 'Different invocation result.',
                                   toolCallId='foreign-call', toolName='read', isError=False)
            helper.observe(repo, session, lifecycle, [human, call, foreign], seal=False, terminal=True)
            self.assertEqual((original / 'segment.json').read_bytes(), manifest)
            self.assertFalse((original / 'resolutions' / (missing['gap_id'] + '.json')).exists())
            rows, faults = transport.pending_inventory(repo)
            self.assertEqual(faults, [])
            self.assertTrue(any(row['status'] == 'pending-coverage-gap' for row in rows))
            self.assertFalse(boundary.drain_source_capabilities(repo, quiesced=True)['drainable'])

    def test_unsafe_exact_recovery_does_not_append_gap_to_sealed_source(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Actual task.', attribution='user')
            opaque = helper.entry('missing', 'assistant', [{'type': 'image'}])
            helper.observe(repo, session, lifecycle, [human, opaque])
            directory, _ = transport.segments(repo, session)[0]
            before = {str(path.relative_to(directory)): path.read_bytes() for path in directory.rglob('*.json')}
            unsafe = helper.entry('missing', 'assistant', 'password=synthetic-secret-should-not-retain')
            helper.observe(repo, session, lifecycle, [human, unsafe], seal=False, terminal=True)
            self.assertEqual(before, {str(path.relative_to(directory)): path.read_bytes() for path in directory.rglob('*.json')})
            self.assertTrue(any(row['status'] == 'pending-coverage-gap' for row in transport.pending_inventory(repo)[0]))

    def test_manager_subdirectory_same_bank_accepts_and_native_root_identity_stays_canonical(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            subdirectory = repo / 'synthetic-subdirectory'; subdirectory.mkdir()
            human = helper.entry('human', 'user', 'Actual task from a subdirectory.', attribution='user')
            value = self.request(repo, session, lifecycle, [human], action='seal')
            value['payload']['view']['cwd'] = str(subdirectory)
            value['payload']['view']['header']['cwd'] = str(subdirectory)
            result = helper.observe_request(repo, value)
            expected = str(uuid.uuid5(uuid.NAMESPACE_URL, str(repo.resolve()) + ':omp:' + session + ':human'))
            self.assertEqual(result['lifecycle'], expected)
            directory, cursor = transport.segments(repo, session)[0]
            self.assertTrue(transport.source_bound_origin(directory, cursor))
            self.assertEqual(transport.read(directory / 'snapshots' / (hashlib.sha256(b'human').hexdigest() + '.json'))['native_entry_id'], 'human')

    def test_nested_foreign_repository_cwd_cannot_borrow_parent_bank_scope(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            nested = world.repository(repo / 'foreign-repository')
            value = self.request(repo, session, lifecycle,
                [helper.entry('human', 'user', 'A different repository task.', attribution='user')])
            value['payload']['view']['cwd'] = str(nested)
            value['payload']['view']['header']['cwd'] = str(nested)
            started = []
            with self.assertRaisesRegex(ValueError, 'another bank scope'):
                helper.observe_request(repo, value, on_started=lambda: started.append(True))
            self.assertEqual(started, [])
            self.assertEqual(transport.segments(repo, session), [])

    def test_unavailable_or_nongit_manager_cwd_refuses_before_source_start(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            nongit = world.inside('not-a-repository'); nongit.mkdir()
            invalid = repo / 'unavailable-nested-git'; invalid.mkdir(); (invalid / '.git').write_text('invalid Git locator\n')
            for cwd in (nongit, invalid, world.inside('missing-directory')):
                with self.subTest(cwd=str(cwd)):
                    value = self.request(repo, session, lifecycle,
                        [helper.entry('human', 'user', 'Unqualified location.', attribution='user')])
                    value['payload']['view']['cwd'] = str(cwd); started = []
                    with self.assertRaises((ValueError, OSError)):
                        helper.observe_request(repo, value, on_started=lambda: started.append(True))
                    self.assertEqual(started, [])
            self.assertEqual(transport.segments(repo, session), [])

    def test_legacy_random_root_wake_follows_exact_native_user_without_spending_new_human_budget(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, legacy = helper.fixture(world)
            transport.new_segment(repo, session, legacy, legacy, 'human', 0)
            wakes = transport.session_dir(repo, session) / 'root-wakes'
            transport.retain(wakes / (legacy + '.json'), {'schema': 'agent-memory/omp-root-wake-v1',
                'session': session, 'human_root': legacy, 'native_user_entry': 'same-human',
                'request_id': str(uuid.uuid4()), 'status': 'requested'}, 4096)
            human = helper.entry('same-human', 'user', 'Actual previously requested human.', attribution='user')
            result = helper.observe(repo, session, str(uuid.uuid4()), [human])
            request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake', 'session': session,
                       'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}
            repeated = helper.observe_request(repo, request, delivery_window=fixtures.LiveDeliveryWindow())
            self.assertFalse(repeated['continue'])
            self.assertEqual(repeated['status'], 'pending-already-requested-native-user')
            self.assertEqual(len(list(wakes.glob('*.json'))), 1)
            other = helper.entry('different-human', 'user', 'A real new human task.', attribution='user')
            result = helper.observe(repo, session, str(uuid.uuid4()), [human, other])
            request.update(lifecycle=result['lifecycle'], payload=result['wake_payload'])
            self.assertTrue(helper.observe_request(repo, request, delivery_window=fixtures.LiveDeliveryWindow())['continue'])
            self.assertEqual(len(list(wakes.glob('*.json'))), 2)

    def test_malformed_wake_state_suppresses_reservation_but_retains_and_enrolls_real_source(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            wakes = transport.session_dir(repo, session) / 'root-wakes'
            transport.retain(wakes / 'unknown.json', {'unexpected': 'preserve'}, 4096)
            human = helper.entry('human', 'user', 'Actual source despite broken wake metadata.', attribution='user')
            result = helper.observe(repo, session, lifecycle, [human])
            self.assertTrue(result['verified']); self.assertTrue(result['faults'])
            rows, faults = transport.pending_inventory(repo)
            self.assertTrue(faults)
            self.assertEqual(len([row for row in rows if 'observation_id' in row]), 2)
            request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake', 'session': session,
                       'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}
            refused = helper.observe_request(repo, request, delivery_window=fixtures.LiveDeliveryWindow())
            self.assertFalse(refused['continue'])
            self.assertEqual(refused['status'], 'pending-wake-state-unverifiable')
            self.assertEqual(transport.read(wakes / 'unknown.json'), {'unexpected': 'preserve'})

    def test_no_active_resume_paths_never_append_to_previously_sealed_instance_segment(self):
        for kind in ('carrier', 'message', 'tail'):
            with self.subTest(kind=kind), World() as world, patch.dict(os.environ, world.env, clear=True):
                helper = fixtures.NativeSources(); repo, session, _ = helper.fixture(world)
                lifecycle = str(uuid.uuid5(uuid.NAMESPACE_URL, str(repo.resolve()) + ':omp:' + session + ':old-human'))
                helper.observe(repo, session, lifecycle,
                    [helper.entry('old-human', 'user', 'Earlier branch human.', attribution='user')], terminal=True)
                original = transport.session_dir(repo, session) / lifecycle
                before = {str(path.relative_to(original)): path.read_bytes() for path in original.rglob('*.json')}
                entries = ([{'type': 'unknown-native-carrier', 'id': 'later', 'parentId': None}]
                           if kind == 'carrier' else
                           [helper.entry('later', 'assistant', 'A different stored branch.')] if kind == 'message' else [])
                value = self.request(repo, session, lifecycle, entries, terminal=True)
                if kind == 'tail':
                    value['payload']['view']['tail_status'] = 'active-unavailable'
                result = helper.observe_request(repo, value)
                self.assertTrue(result['verified'])
                self.assertEqual(before, {str(path.relative_to(original)): path.read_bytes() for path in original.rglob('*.json')})
                successors = [item for item in transport.segments(repo, session) if item[0] != original]
                self.assertEqual(len(successors), 1)
                self.assertTrue(transport.read(successors[0][0] / 'segment.json')['gaps'])
                self.assertEqual(transport.pending_inventory(repo)[1], [])

    def test_wake_failure_after_reservation_preserves_verified_seal_and_single_budget(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            entries = [helper.entry('human', 'user', 'Actual source.', attribution='user')]
            value = self.request(repo, session, lifecycle, entries, action='seal')
            value['payload']['request_wake'] = True
            replace = transport.replace_cursor
            def fail_after_reservation(path, cursor):
                if (cursor.get('control_interval') or {}).get('generated_guidance'):
                    raise OSError('Controlled failure after durable reservation')
                return replace(path, cursor)
            with patch.object(transport, 'replace_cursor', side_effect=fail_after_reservation):
                result = helper.observe_request(repo, value, delivery_window=fixtures.LiveDeliveryWindow())
            self.assertTrue(result['verified'])
            self.assertEqual(result['status'], 'pending-wake-unknown-after-reservation')
            self.assertFalse(result['continue'])
            wakes = transport.session_dir(repo, session) / 'root-wakes'
            self.assertEqual(len(list(wakes.glob('*.json'))), 1)
            repeated = helper.observe_request(repo, value, delivery_window=fixtures.LiveDeliveryWindow())
            self.assertTrue(repeated['verified']); self.assertFalse(repeated['continue'])
            self.assertEqual(len(list(wakes.glob('*.json'))), 1)
            self.assertEqual(len([row for row in transport.pending_inventory(repo)[0] if 'observation_id' in row]), 2)

    def test_pre_reservation_wake_exception_does_not_unverify_committed_source(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            from agent_memory.omp_stream import VerifiedNativeDeliveryWindow
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            value = self.request(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual source.', attribution='user')], action='seal')
            value['payload']['request_wake'] = True
            with patch.object(VerifiedNativeDeliveryWindow, 'reserve', side_effect=OSError('Controlled read-only clock failure')):
                result = helper.observe_request(repo, value, delivery_window=fixtures.LiveDeliveryWindow())
            self.assertTrue(result['verified']); self.assertFalse(result['continue'])
            self.assertEqual(result['status'], 'pending-wake-unavailable')
            self.assertFalse((transport.session_dir(repo, session) / 'root-wakes').exists())
            self.assertEqual(len([row for row in transport.pending_inventory(repo)[0] if 'observation_id' in row]), 2)

    def test_removed_public_observer_flag_refuses_before_any_source_write(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, _ = fixtures.OwnedInstructionBoundary().installed(world)
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            base = boundary._base(repo)
            before = {str(path.relative_to(base)): path.read_bytes() for path in base.rglob('*') if path.is_file()}
            value = self.request(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Must not be written by removed public flag.', attribution='user')])
            result = fixtures.subprocess.run([fixtures.Path(receipt['bin_dir']) / 'am', 'omp-boundary', '--repo', repo,
                '--observer', '--stdin'], input=fixtures.json.dumps(value), cwd=repo, env=world.env,
                capture_output=True, text=True, timeout=30)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(before, {str(path.relative_to(base)): path.read_bytes() for path in base.rglob('*') if path.is_file()})
            self.assertEqual(transport.segments(repo, session), [])


if __name__ == '__main__':
    unittest.main()
