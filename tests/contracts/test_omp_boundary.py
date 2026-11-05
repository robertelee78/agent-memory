"""Executable transport regressions, not evidence of native omp support."""
import json
import os
from pathlib import Path
import sys
import unittest
import uuid
from unittest.mock import patch
from world import World
from contracts import test_admission

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import installation, omp_boundary as boundary, omp_installation, omp_transport as transport, turn_sources


class NativeSources(unittest.TestCase):
    def fixture(self, world):
        repo, _ = test_admission.AdmissionContract().fixture(world)
        return repo, str(uuid.uuid4()), str(uuid.uuid4())

    def entry(self, identifier, role, text, *, attribution='agent', parent=None, **extra):
        return {'type': 'message', 'id': identifier, 'parentId': parent,
                'message': {'role': role, 'attribution': attribution, 'content': text, **extra}}

    def observe(self, repo, session, lifecycle, entries, *, seal=True, terminal=False, trigger='human'):
        payload = {'entries': entries, 'branch_ids': [e['id'] for e in entries], 'event_ordinal': len(entries),
                   'terminal': terminal, 'trigger': trigger, 'inherited_ids': [], 'metadata': []}
        if seal:
            payload['request_wake'] = False
        return transport.observe(repo, {'schema': 'agent-memory/omp-observer-request-v1',
                                       'action': 'seal' if seal else 'ingest', 'session': session,
                                       'lifecycle': lifecycle, 'payload': payload})

    def acknowledge(self, repo, row):
        value = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
        value.update(outcome='no-learning', assessment={'source_id': row['observation_id'],
                     'source_review': 'Reviewed the actual ephemeral transport fixture; no durable project learning.', 'candidates': []})
        return transport.complete(repo, value)

    def test_long_segment_uses_two_shared_observations_and_ingest_is_idempotent(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            entries = [self.entry('human', 'user', 'An ephemeral arithmetic question', attribution='user')]
            entries += [self.entry('assistant-' + str(i), 'assistant', 'Ephemeral computation') for i in range(150)]
            with patch.dict(os.environ, world.env, clear=True):
                result = self.observe(repo, session, lifecycle, entries)
                rows, faults = transport.pending_inventory(repo)
                self.assertFalse(faults)
                self.assertEqual(len(rows), 2)
                ids = {r['observation_id'] for r in rows}
                self.observe(repo, session, result['lifecycle'], entries)
                self.assertEqual(ids, {r['observation_id'] for r in transport.pending_inventory(repo)[0]})
                for row in rows:
                    self.assertTrue(self.acknowledge(repo, row)['verified'])
                self.assertEqual(transport.list_pending(repo)['pending'], [])

    def test_admission_requires_sealed_registered_segment(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            entries = [self.entry('human', 'user', 'Actual short source', attribution='user')]
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, entries, seal=False)
                rows = transport.pending_inventory(repo)[0]
                admission = next(r for r in rows if 'observation_id' in r)
                with self.assertRaises(ValueError):
                    self.acknowledge(repo, admission)

    def test_gap_refuses_both_assessment_outcomes_and_preserves_marker(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            entries = [self.entry('human', 'user', 'Source with unavailable payload', attribution='user'),
                       self.entry('image', 'assistant', [{'type': 'image', 'data': 'must-not-retain'}])]
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, entries)
                rows, _ = transport.pending_inventory(repo)
                self.assertTrue(all(row['status'] == 'pending-coverage-gap' for row in rows))
                for row in rows:
                    with self.assertRaises(ValueError):
                        self.acknowledge(repo, row)
                result = boundary.drain_source_capabilities(repo, quiesced=True)
                self.assertFalse(result['drainable'])
                self.assertEqual(boundary.required_source_capabilities(repo), {boundary.CAPABILITY})
                for path in (boundary._base(repo) / 'omp-source-v1').rglob('*.json'):
                    self.assertNotIn('must-not-retain', path.read_text())

    def test_capacity_retains_arrival_then_reuses_reference_after_capacity_frees(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True), patch.object(turn_sources, 'MAX_TURNS', 1):
                other = turn_sources.register(repo, 'codex', 'other', 'other', 'a' * 64)
                self.observe(repo, session, lifecycle, [self.entry('human', 'user', 'Actual queued source', attribution='user')])
                queued = transport.list_pending(repo)['pending']
                references = {row['source_reference'] for row in queued}
                self.assertTrue(all('observation_id' not in row for row in queued))
                value = {'observation_id': other['observation_id'], 'source_sha256': other['source_sha256'],
                         'outcome': 'no-learning', 'assessment': {'source_id': other['observation_id'], 'source_review': 'Reviewed actual other fixture.', 'candidates': []}}
                receipt = turn_sources.verify_assessment(repo, value, other['observation_id'])
                turn_sources.complete(repo, 'codex', 'other', 'other', other['observation_id'], other['source_sha256'], receipt)
                self.observe(repo, session, lifecycle, [], seal=False)
                enrolled = transport.list_pending(repo)['pending']
                self.assertEqual(references, {row['source_reference'] for row in enrolled})
                self.assertTrue(all('observation_id' in row for row in enrolled))

    def test_root_wake_is_once_and_later_human_carries_unassessed_old_source(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                first = self.entry('h1', 'user', 'First actual short question', attribution='user')
                result = self.observe(repo, session, lifecycle, [first])
                request = {'schema': 'agent-memory/omp-observer-request-v1', 'action': 'wake', 'session': session,
                           'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}
                wake = transport.observe(repo, request)
                self.assertTrue(wake['continue'])
                self.assertFalse(transport.observe(repo, request)['continue'])
                old_ids = set(result['wake_payload']['observation_ids'])
                second = self.entry('h2', 'user', 'Second actual short question', attribution='user')
                result = self.observe(repo, session, result['lifecycle'], [first, second])
                request.update(lifecycle=result['lifecycle'], payload=result['wake_payload'])
                wake = transport.observe(repo, request)
                self.assertTrue(wake['continue'])
                self.assertTrue(all(identifier in wake['additionalContext'] for identifier in old_ids))
                self.assertLessEqual(len(wake['additionalContext'].encode()), 8192)

    def test_completed_snapshots_require_bound_drain_proof_after_marker_removal(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, [self.entry('h1', 'user', 'Actual reviewed short question', attribution='user')])
                for row in transport.pending_inventory(repo)[0]:
                    self.acknowledge(repo, row)
                result = boundary.drain_source_capabilities(repo, quiesced=True)
                self.assertTrue(result['marker_cleared'])
                self.assertEqual(boundary.required_source_capabilities(repo), set())
                self.assertTrue((boundary._base(repo) / 'omp-source-v1').exists())
                for proof in (boundary._base(repo) / 'omp-drains').glob('*.json'):
                    proof.unlink()
                with self.assertRaises(ValueError):
                    boundary.required_source_capabilities(repo)

    def test_manual_marker_delete_cannot_hide_unregistered_source(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, [self.entry('human', 'user', 'Actual source', attribution='user')], seal=False)
                (boundary._base(repo) / 'turn-sources-compat-v1.json').unlink()
                with self.assertRaises(ValueError):
                    boundary.required_source_capabilities(repo)


    def test_exact_own_hidden_batch_does_not_open_a_source_successor(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                human = self.entry('human', 'user', 'Actual short request', attribution='user')
                result = self.observe(repo, session, lifecycle, [human])
                wake = transport.observe(repo, {'schema': 'agent-memory/omp-observer-request-v1', 'action': 'wake',
                                               'session': session, 'lifecycle': result['lifecycle'], 'payload': result['wake_payload']})
                control = {'type': 'custom_message', 'id': 'native-control', 'parentId': 'human',
                           'customType': 'session-stop-continuation', 'attribution': 'agent',
                           'content': wake['additionalContext']}
                self.observe(repo, session, result['lifecycle'], [human, control], seal=False)
                self.assertEqual(len(transport.segments(repo, session)), 1)
                substantive = self.entry('late-work', 'assistant', 'New substantive foreign work')
                self.observe(repo, session, result['lifecycle'], [human, control, substantive], seal=False, terminal=True,
                             trigger='own-hidden-ambiguous')
                segments = transport.segments(repo, session)
                self.assertEqual(len(segments), 2)
                self.assertEqual(segments[-1][1]['origin']['human_root'], segments[0][1]['origin']['human_root'])
                self.assertEqual(segments[-1][1]['origin']['native_trigger'], 'own-hidden-ambiguous')

    def test_late_native_tool_result_recovers_original_sealed_gap_without_reopening(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                human = self.entry('human', 'user', 'Actual approved tool request', attribution='user')
                assistant = self.entry('assistant-call', 'assistant',
                                       [{'type': 'toolCall', 'id': 'native-call', 'name': 'bash',
                                         'arguments': {'command': 'printf 12'}}])
                first = self.observe(repo, session, lifecycle, [human, assistant])
                original_dir, original_cursor = transport.segments(repo, session)[0]
                original_manifest = transport.read(original_dir / 'segment.json')
                self.assertTrue(any(gap['native_entry_id'] == 'tool:native-call' for gap in original_manifest['gaps']))
                result = self.entry('tool-result', 'toolResult', '12', toolCallId='native-call', toolName='bash')
                self.observe(repo, session, first['lifecycle'], [human, assistant, result], terminal=True)
                self.assertEqual(transport.read(original_dir / 'segment.json'), original_manifest)
                self.assertTrue(transport._coverage(original_dir, original_manifest))
                self.assertFalse(transport.read(original_dir / 'cursor.json')['tool_lifecycles'])
                self.assertFalse(transport.pending_inventory(repo)[1])
                self.assertEqual(len(transport.segments(repo, session)), 2)

    def test_protected_private_root_cannot_silently_move_or_lose_binding(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, [self.entry('human', 'user', 'Actual source', attribution='user')])
                base = boundary._base(repo)
                self.assertIn(Path(repo).resolve(), boundary.source_capability_repositories())
                binding = base / 'omp-bank-binding-v1.json'
                binding.unlink()
                with self.assertRaises(ValueError):
                    boundary.required_source_capabilities(repo)
                with self.assertRaises(ValueError):
                    boundary.source_capability_repositories()



class StoredBatchControls(unittest.TestCase):
    def test_foreign_companions_and_human_input_never_become_own_maintenance(self):
        from agent_memory import omp_control
        own = {'type': 'custom_message', 'id': 'own', 'parentId': None, 'customType': 'session-stop-continuation',
               'attribution': 'agent', 'content': 'exact reserved guidance'}
        self.assertIsNotNone(omp_control.pure_batch([own], 'exact reserved guidance'))
        for custom_type in ('goal-mode-context', 'plan-mode', 'vibe-mode-context', 'todo-error-reminder',
                            'advisor', 'MEMORY_RECALL_CHANGES', 'foreign-before-agent', 'unknown'):
            foreign = {**own, 'id': 'foreign', 'customType': custom_type, 'content': 'Foreign contextual work'}
            self.assertIsNone(omp_control.pure_batch([own, foreign], 'exact reserved guidance'))
        human = {'type': 'message', 'id': 'human', 'parentId': None,
                 'message': {'role': 'user', 'attribution': 'user', 'content': 'Actual new human request'}}
        self.assertIsNone(omp_control.pure_batch([human, own], 'exact reserved guidance'))
        self.assertIsNone(omp_control.pure_batch([{**own, 'content': 'exact reserved guidance\\njoined foreign stop text'}],
                                               'exact reserved guidance'))


class ExplicitNativeOwnership(unittest.TestCase):
    def test_all_never_enables_omp_and_root_is_required(self):
        self.assertNotIn('omp', {name for name, _, _ in installation.presets('all')})
        with self.assertRaises(ValueError):
            installation.presets('omp')

    def test_explicit_root_and_peer_attestation_never_come_from_environment(self):
        from argparse import Namespace
        args = Namespace(omp_agent_dir=None, omp_peers_upgraded=False)
        with self.assertRaises(ValueError):
            omp_installation.configuration(args, None, 'omp', False)
        with self.assertRaises(ValueError):
            omp_installation.binding('relative/root')

    def test_saved_root_changes_and_implicit_all_removal_refuse(self):
        from argparse import Namespace
        saved = omp_installation.binding('/private/native-owned')
        old = {'omp_binding': saved}
        with self.assertRaises(ValueError):
            omp_installation.configuration(Namespace(omp_agent_dir='/private/different', omp_peers_upgraded=True), old, 'omp', False)
        with self.assertRaises(ValueError):
            omp_installation.configuration(Namespace(omp_agent_dir=None, omp_peers_upgraded=False), old, 'all', False)
