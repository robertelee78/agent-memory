"""Executable transport regressions, not evidence of native omp support."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import shutil
import unittest
import uuid
from unittest.mock import patch
from world import World
from contracts import test_admission

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import installation, omp_boundary as boundary, omp_installation, omp_projection, omp_transport as transport, turn_sources


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
                name = hashlib.sha256(control['id'].encode()).hexdigest() + '.json'
                self.assertFalse((segments[0][0] / 'captures' / name).exists())
                self.assertTrue((segments[-1][0] / 'captures' / name).exists())

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


class OwnedInstructionBoundary(unittest.TestCase):
    """Structural carrier fixtures exercise real installation ownership, not native support."""

    def installed(self, world, mode='copy'):
        source = Path(__file__).resolve().parents[2]
        root = world.inside('owned native root')
        root.mkdir()
        result = world.run([sys.executable, '-I', '-B',
                            source / 'skills/agent-memory/scripts/installer.py',
                            'install', '--development-source', source, '--presets', 'omp',
                            '--omp-agent-dir', root, '--omp-peers-upgraded', '--mode', mode,
                            '--bin-dir', world.inside('owned bin')], timeout=30)
        receipt = json.loads(result.stdout)['receipt']
        witness = omp_installation._owned_manual(receipt)
        self.assertIsNotNone(witness)
        return receipt, witness

    def pair(self, witness):
        text, path, _, _, _, _ = witness._verify()
        body = text.rstrip('\n')
        lines = len(body.split('\n'))
        invocation = {'type': 'message', 'id': 'manual-invocation', 'parentId': 'human',
                      'message': {'role': 'assistant', 'content': [
                          {'type': 'toolCall', 'id': 'manual-call', 'name': 'read',
                           'arguments': {'i': 'Reading installed guide', 'path': 'skill://agent-memory'}}]}}
        entry = {'type': 'message', 'id': 'manual-result', 'parentId': 'manual-invocation',
                 'message': {'role': 'toolResult', 'toolCallId': 'manual-call', 'toolName': 'read',
                             'isError': False, 'content': [{'type': 'text',
                                 'text': '[Skill file: ' + path + ']\n' + body}],
                             'details': {'contentType': 'text/markdown', 'resolvedPath': path,
                                 'rowPrefixes': False, 'totalLines': lines,
                                 'displayContent': {'lineNumbers': list(range(1, lines + 1)),
                                                    'startLine': 1, 'text': body},
                                 'meta': {'source': {'type': 'internal', 'value': 'skill://agent-memory'}}}}}
        return invocation, entry

    def test_exact_carrier_and_unforgeable_witness(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            invocation, entry = self.pair(witness)
            projected = omp_projection._project_owned_manual(entry, invocation, witness, witness._verify())
            self.assertIsNotNone(projected)
            snapshot = projected['snapshot']
            self.assertEqual(snapshot['content'], [])
            self.assertNotIn('details', snapshot)
            self.assertEqual(snapshot['instruction_context']['observed_receipt_sha256'], transport.digest(receipt))
            self.assertEqual(snapshot['instruction_context']['invocation_native_entry_id'], invocation['id'])
            self.assertTrue(omp_projection._valid_instruction_context(snapshot))
            self.assertIsNone(omp_projection._project_owned_manual(entry, invocation, receipt, witness._verify()))
            self.assertIsNone(omp_projection._project_owned_manual(entry, invocation, {'trusted': True}, witness._verify()))
            forged = self.entry_for_forgery(snapshot['instruction_context'])
            ordinary = omp_projection.project(forged)
            self.assertNotIn('gap', ordinary)
            self.assertNotIn('instruction_context', ordinary['snapshot'])
            with self.assertRaises(TypeError):
                json.dumps(witness)

    def entry_for_forgery(self, context):
        return {'type': 'message', 'id': 'forged-result', 'parentId': None,
                'message': {'role': 'toolResult', 'toolCallId': 'forged-call', 'toolName': 'read',
                            'content': [{'type': 'text', 'text': 'Ordinary benign result.'}],
                            'instruction_context': context}}

    def test_malformed_carriers_and_invocations_decline(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, witness = self.installed(world)
            invocation, entry = self.pair(witness)
            variants = []
            for mutate in (
                lambda m: m.update(isError=True),
                lambda m: m['content'].append({'type': 'text', 'text': 'extra'}),
                lambda m: m['content'][0].update(type='image'),
                lambda m: m['content'][0].update(text=m['content'][0]['text'][:-1]),
                lambda m: m['details'].update(extra='value'),
                lambda m: m['details'].update(resolvedPath='/foreign/SKILL.md'),
                lambda m: m['details'].update(totalLines=True),
                lambda m: m['details']['displayContent'].update(startLine=True),
                lambda m: m['details']['meta']['source'].update(value='skill://other'),
                lambda m: m.update(approved=True),
            ):
                candidate = copy.deepcopy(entry)
                mutate(candidate['message'])
                variants.append((invocation, candidate))
            for mutate in (
                lambda c: c['arguments'].update(path='skill://agent-memory:1-20'),
                lambda c: c['arguments'].update(path='/foreign/SKILL.md'),
                lambda c: c['arguments'].update(extra=True),
                lambda c: c.update(id='another-call'),
                lambda c: c.update(name='bash'),
            ):
                candidate = copy.deepcopy(invocation)
                mutate(candidate['message']['content'][0])
                variants.append((candidate, entry))
            duplicate = copy.deepcopy(invocation)
            duplicate['message']['content'].append(copy.deepcopy(duplicate['message']['content'][0]))
            variants.append((duplicate, entry))
            for call, result in variants:
                with self.subTest(call=call, result_keys=list(result['message'])):
                    self.assertIsNone(omp_projection._project_owned_manual(result, call, witness, witness._verify()))

    def test_witness_declines_changed_receipt_file_and_permissions(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            forged = copy.deepcopy(receipt)
            forged['installation_id'] = str(uuid.uuid4())
            self.assertIsNone(omp_installation._owned_manual(forged))
            manual = Path(receipt['omp_binding']['skill_target']) / 'SKILL.md'
            original = manual.read_bytes()
            mode = manual.stat().st_mode & 0o777
            manual.write_bytes(original + b'\nChanged installed bytes.\n')
            self.assertIsNone(witness._verify())
            self.assertIsNone(omp_installation._owned_manual(receipt))
            manual.write_bytes(original)
            manual.chmod(mode | 0o002)
            self.assertIsNone(witness._verify())
            self.assertIsNone(omp_installation._owned_manual(receipt))

    def test_link_witness_declines_retargeted_owned_link(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world, 'link')
            link = Path(receipt['omp_binding']['skill_target'])
            self.assertTrue(link.is_symlink())
            replacement = world.inside('foreign skill')
            replacement.mkdir()
            (replacement / 'SKILL.md').write_text('Foreign guide.\n')
            link.unlink()
            link.symlink_to(replacement, target_is_directory=True)
            self.assertIsNone(witness._verify())
            self.assertIsNone(omp_installation._owned_manual(receipt))

    def test_historical_annotation_is_closed_and_installation_independent(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            invocation, entry = self.pair(witness)
            snapshot = omp_projection._project_owned_manual(entry, invocation, witness, witness._verify())['snapshot']
            manual = Path(receipt['omp_binding']['skill_target']) / 'SKILL.md'
            manual.unlink()
            self.assertIsNone(witness._verify())
            self.assertTrue(omp_projection._valid_instruction_context(snapshot))
            self.assertTrue(omp_projection._valid_instruction_context({'role': 'user', 'content': []}))
            mutations = (
                lambda s: s.update(content=[{'type': 'text', 'text': 'not metadata'}]),
                lambda s: s.update(details={}),
                lambda s: s['instruction_context'].update(extra='value'),
                lambda s: s['instruction_context'].update(tool_call_id='different'),
                lambda s: s['instruction_context'].update(manual_file_sha256='z' * 64),
                lambda s: s['instruction_context'].update(renderer='another-renderer'),
                lambda s: s['instruction_context'].pop('installation_id'),
            )
            for mutate in mutations:
                changed = copy.deepcopy(snapshot)
                mutate(changed)
                self.assertFalse(omp_projection._valid_instruction_context(changed))

    def observe(self, repo, session, lifecycle, entries, receipt=None, *, seal=False):
        payload = {'entries': entries, 'branch_ids': [e['id'] for e in entries],
                   'event_ordinal': len(entries), 'terminal': False, 'trigger': 'human',
                   'inherited_ids': [], 'metadata': []}
        if seal:
            payload['request_wake'] = False
        return transport.observe(repo, {'schema': 'agent-memory/omp-observer-request-v1',
                                      'action': 'seal' if seal else 'ingest', 'session': session,
                                      'lifecycle': lifecycle, 'payload': payload}, receipt=receipt)

    def test_fresh_capture_preserves_surrounding_source_and_retries(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world, patch.dict(os.environ, world.env, clear=True):
                receipt, witness = self.installed(world, mode)
                helper = NativeSources()
                repo, session, lifecycle = helper.fixture(world)
                invocation, entry = self.pair(witness)
                human = helper.entry('human', 'user', 'Review this task', attribution='user')
                answer = helper.entry('answer', 'assistant', 'The task is complete.')
                entries = [human, invocation, entry, answer]
                result = self.observe(repo, session, lifecycle, entries, receipt, seal=True)
                directory, _ = transport.segments(repo, session)[0]
                name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
                path = directory / 'snapshots' / name
                original = path.read_bytes()
                snapshot = transport.read(path)
                self.assertEqual(snapshot['content'], [])
                self.assertEqual(snapshot['instruction_context']['tool_call_id'], 'manual-call')
                self.assertTrue(omp_projection._valid_instruction_context(snapshot))
                for native in ('human', 'manual-invocation', 'answer'):
                    self.assertTrue((directory / 'captures' /
                                     (hashlib.sha256(native.encode()).hexdigest() + '.json')).exists())
                manifest = transport.read(directory / 'segment.json')
                self.assertFalse(any(g['native_entry_id'] == 'tool:manual-call' for g in manifest['gaps']))
                self.observe(repo, session, result['lifecycle'], entries, receipt)
                self.assertEqual(path.read_bytes(), original)

    def test_nonread_result_never_mints_manual_witness(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, _ = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            invocation = helper.entry('call', 'assistant', [
                {'type': 'toolCall', 'id': 'bash-call', 'name': 'bash',
                 'arguments': {'command': 'printf 12'}}])
            result = helper.entry('result', 'toolResult', '12',
                                  toolCallId='bash-call', toolName='bash', isError=False)
            with patch.object(omp_installation, '_owned_manual',
                              side_effect=AssertionError('non-read minted manual witness')):
                self.observe(repo, session, lifecycle, [human, invocation, result], receipt, seal=True)
            directory, _ = transport.segments(repo, session)[0]
            name = hashlib.sha256(result['id'].encode()).hexdigest() + '.json'
            snapshot = transport.read(directory / 'snapshots' / name)
            self.assertEqual(snapshot['content'], [{'type': 'text', 'text': '12'}])
            self.assertEqual(snapshot['toolName'], 'bash')
            self.assertNotIn('instruction_context', snapshot)

    def test_screened_invocation_does_not_hide_following_answer(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            invocation['message']['content'][0]['arguments']['i'] = 'password=secretvalue'
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            answer = helper.entry('answer', 'assistant', 'The task is complete.')
            self.observe(repo, session, lifecycle, [human, invocation, entry, answer], receipt, seal=True)
            directory, _ = transport.segments(repo, session)[0]
            gaps = [transport.read(path) for path in (directory / 'gaps').glob('*.json')]
            self.assertTrue(any(g['native_entry_id'] == invocation['id'] for g in gaps))
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            result_snapshot = directory / 'snapshots' / name
            if result_snapshot.exists():
                self.assertNotIn('instruction_context', transport.read(result_snapshot))
            answer_name = hashlib.sha256(answer['id'].encode()).hexdigest() + '.json'
            answer_snapshot = transport.read(directory / 'snapshots' / answer_name)
            self.assertEqual(answer_snapshot['content'], [{'type': 'text', 'text': 'The task is complete.'}])
            self.assertTrue(transport.pending_inventory(repo)[0])

    def test_snapshot_publication_recovery_survives_uninstall_and_retry(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            self.observe(repo, session, lifecycle, [human, invocation], receipt)
            directory, _ = transport.segments(repo, session)[0]
            snapshot = omp_projection._project_owned_manual(entry, invocation, witness, witness._verify())['snapshot']
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            path = directory / 'snapshots' / name
            transport.retain(path, snapshot)
            original = path.read_bytes()
            (Path(receipt['omp_binding']['skill_target']) / 'SKILL.md').unlink()
            self.observe(repo, session, lifecycle, [human, invocation, entry])
            capture = transport.read(directory / 'captures' / name)
            self.assertEqual(capture['snapshot_sha256'], transport.digest(snapshot))
            self.assertEqual(path.read_bytes(), original)
            self.observe(repo, session, lifecycle, [human, invocation, entry])
            self.assertEqual(path.read_bytes(), original)
            self.assertFalse(list((directory / 'gaps').glob('*.json')))

    def test_mismatched_orphan_snapshot_is_integrity_failure(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            self.observe(repo, session, lifecycle, [human, invocation], receipt)
            directory, _ = transport.segments(repo, session)[0]
            snapshot = omp_projection._project_owned_manual(entry, invocation, witness, witness._verify())['snapshot']
            snapshot['native_entry_id'] = 'another-entry'
            self.assertTrue(omp_projection._valid_instruction_context(snapshot))
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            path = directory / 'snapshots' / name
            transport.retain(path, snapshot)
            original = path.read_bytes()
            with self.assertRaises(ValueError):
                self.observe(repo, session, lifecycle, [human, invocation, entry], receipt)
            self.assertEqual(path.read_bytes(), original)
            self.assertFalse((directory / 'captures' / name).exists())
            self.assertFalse(list((directory / 'gaps').glob('*.json')))

    def test_preexisting_native_gap_is_not_reclassified(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            unavailable = copy.deepcopy(entry)
            unavailable['message']['content'] = [{'type': 'image', 'data': 'opaque'}]
            self.observe(repo, session, lifecycle, [human, invocation, unavailable], receipt)
            directory, _ = transport.segments(repo, session)[0]
            gaps = {p.name: p.read_bytes() for p in (directory / 'gaps').glob('*.json')}
            self.assertTrue(gaps)
            self.observe(repo, session, lifecycle, [human, invocation, entry], receipt)
            self.assertEqual(gaps, {p.name: p.read_bytes() for p in (directory / 'gaps').glob('*.json')})
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            path = directory / 'snapshots' / name
            if path.exists():
                self.assertNotIn('instruction_context', transport.read(path))
            self.assertTrue(transport.pending_inventory(repo)[0])

    def test_sealed_tool_gap_does_not_authorize_late_classification(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            result = self.observe(repo, session, lifecycle, [human, invocation], receipt, seal=True)
            directory, _ = transport.segments(repo, session)[0]
            manifest_path = directory / 'segment.json'
            before = manifest_path.read_bytes()
            self.assertTrue(any(g['native_entry_id'] == 'tool:manual-call'
                                for g in transport.read(manifest_path)['gaps']))
            self.observe(repo, session, result['lifecycle'], [human, invocation, entry], receipt)
            self.assertEqual(manifest_path.read_bytes(), before)
            self.assertFalse((directory / 'resolutions').exists() and
                             list((directory / 'resolutions').glob('*.json')))
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            for segment, _ in transport.segments(repo, session):
                path = segment / 'snapshots' / name
                if path.exists():
                    self.assertNotIn('instruction_context', transport.read(path))

    def test_same_sweep_human_boundary_preserves_sealed_tool_gap(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            first = helper.entry('human', 'user', 'Review the first task', attribution='user')
            second = helper.entry('second-human', 'user', 'Review the next task', attribution='user')
            entries = [first, invocation, second, entry]
            result = self.observe(repo, session, lifecycle, entries, receipt)
            segments = transport.segments(repo, session)
            original = next(directory for directory, _ in segments
                            if (directory / 'segment.json').exists()
                            and any(g['native_entry_id'] == 'tool:manual-call'
                                    for g in transport.read(directory / 'segment.json')['gaps']))
            manifest = (original / 'segment.json').read_bytes()
            gap_value = next(g for g in transport.read(original / 'segment.json')['gaps']
                             if g['native_entry_id'] == 'tool:manual-call')
            gap_path = original / 'gaps' / (gap_value['gap_id'] + '.json')
            gap_bytes = gap_path.read_bytes()
            resolution = original / 'resolutions' / (gap_value['gap_id'] + '.json')
            self.assertFalse(resolution.exists())
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            for directory, _ in segments:
                path = directory / 'snapshots' / name
                if path.exists():
                    self.assertNotIn('instruction_context', transport.read(path))
            self.observe(repo, session, result['lifecycle'], entries, receipt)
            self.assertEqual((original / 'segment.json').read_bytes(), manifest)
            self.assertEqual(gap_path.read_bytes(), gap_bytes)
            self.assertFalse(resolution.exists())
            self.assertTrue(transport.pending_inventory(repo)[0])




class ExplicitNativeOwnership(unittest.TestCase):
    def test_explicit_native_install_applies_preview_for_copy_and_link(self):
        source = Path(__file__).resolve().parents[2]
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world:
                root = world.inside('explicit native root')
                root.mkdir()
                bin_dir = world.inside('owned bin')
                command = [sys.executable, '-I', '-B',
                           source / 'skills/agent-memory/scripts/installer.py',
                           'install', '--development-source', source, '--presets', 'omp',
                           '--omp-agent-dir', root, '--omp-peers-upgraded',
                           '--mode', mode, '--bin-dir', bin_dir]
                preview = json.loads(world.run(command + ['--check'], timeout=30).stdout)
                self.assertFalse(preview['installation_writes_performed'])
                self.assertFalse((root / 'extensions/agent-memory.ts').exists())
                applied = json.loads(world.run(command, timeout=30).stdout)
                self.assertTrue(applied['verified'])
                receipt = applied['receipt']
                self.assertEqual(receipt['omp_binding'], preview['omp']['binding'])
                self.assertEqual(receipt['schema'], 'agent-memory/install-receipt-v2')
                for target in preview['targets']:
                    self.assertTrue(Path(target['path']).exists(), target['path'])
                self.assertEqual(json.loads(world.run(
                    [bin_dir / 'am', '--json', 'version'], timeout=30).stdout)['executing_source']['version'],
                    receipt['source']['version'])

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


class NativeLifecyclePublic(unittest.TestCase):
    def managed(self, world, mode='copy'):
        from runtime_update_fixtures import PACK
        channel = world.repository('native lifecycle channel')
        pack = channel / 'skills/agent-memory'
        shutil.copytree(PACK, pack)
        world.git('add', '.', cwd=channel)
        world.git('commit', '-m', 'Controlled native lifecycle runtime', cwd=channel)
        root = world.inside('explicit native lifecycle root')
        root.mkdir()
        result = world.run([sys.executable, '-I', '-B', PACK / 'scripts/installer.py',
                            'install', '--remote', channel, '--ref', 'main', '--mode', mode,
                            '--presets', 'omp', '--omp-agent-dir', root, '--omp-peers-upgraded'], timeout=30)
        return channel, pack, json.loads(result.stdout)['receipt']

    def public(self, world, receipt, *args, expected=0):
        result = world.run([Path(receipt['bin_dir']) / 'am', '--json', *args], check=False, timeout=30)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def extension(self, receipt):
        path = Path(receipt['omp_binding']['extension_target'])
        owned = next(target for target in receipt['targets'] if target['path'] == str(path))
        self.assertEqual(path.stat().st_mode & 0o777, 0o644)
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), owned['identity']['hash'])
        return path

    def enroll_fixture(self, repo, session):
        # Controlled source-verifier/lifecycle setup, NOT native proof. Complete
        # actual retained enrollments without the observer's wall-clock budget;
        # native runs must enroll through their real callbacks independently.
        for directory, cursor in transport.segments(repo, session):
            for path in sorted((directory / 'enrollments').glob('*.json')):
                _, faults = transport.registration(repo, directory, cursor, json.loads(path.read_text()), perform=True)
                self.assertFalse(faults)

    def update_journey(self, world, receipt, advance):
        from runtime_update_fixtures import snapshot, receipt_path
        extension = self.extension(receipt)
        before = snapshot(extension), receipt_path(world).read_bytes()
        self.assertTrue(self.public(world, receipt, 'update', '--check')['check_complete'])
        unchanged = self.public(world, receipt, 'update')
        self.assertTrue(unchanged['verified'])
        self.assertFalse(unchanged['changed'])
        self.assertEqual((snapshot(extension), receipt_path(world).read_bytes()), before)
        advance()
        before = snapshot(extension), receipt_path(world).read_bytes()
        self.assertTrue(self.public(world, receipt, 'update', '--check')['update_available'])
        self.assertEqual((snapshot(extension), receipt_path(world).read_bytes()), before)
        changed = self.public(world, receipt, 'update')
        self.assertTrue(changed['verified'])
        self.assertTrue(changed['changed'])
        self.extension(changed['receipt'])
        self.assertFalse(self.public(world, changed['receipt'], 'update')['changed'])

    def test_managed_native_updates_read_back_only_owned_public_commands(self):
        from runtime_update_fixtures import advance
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world:
                channel, pack, receipt = self.managed(world, mode)
                self.update_journey(world, receipt, lambda: advance(world, channel, pack, '1.0.0'))

    def test_signed_native_updates_preserve_extension_integrity(self):
        from signed_release_fixtures import SignedRelease
        with World() as world, SignedRelease(world) as release:
            release.prepare()
            root = world.inside('signed native root')
            root.mkdir()
            installed = world.run(['bash', '-o', 'pipefail', '-c',
                'curl -fsSL "$1" | bash -s -- --json --mode copy --presets omp '
                '--omp-agent-dir "$2" --omp-peers-upgraded',
                'fixture', release.base + '/download/v0.0.3/install.sh', root],
                timeout=120, check=False)
            self.assertEqual(installed.returncode, 0, installed.stdout + installed.stderr)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['omp_binding']['agent_dir'], str(root))
            self.assertEqual(receipt['source']['kind'], 'release')
            self.update_journey(world, receipt, release.advance)

    def test_public_uninstall_requires_quiescence_and_actually_drains_completed_sources(self):
        from runtime_update_fixtures import snapshot
        with World() as world:
            _, _, receipt = self.managed(world)
            extension = self.extension(receipt)
            initial = snapshot(extension)
            preview = self.public(world, receipt, 'uninstall', '--check', '--omp-writers-quiesced')
            self.assertTrue(preview['check_complete'])
            self.assertEqual(snapshot(extension), initial)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                helper.observe(repo, session, lifecycle, [
                    helper.entry('human', 'user', 'Controlled native lifecycle work', attribution='user')])
                self.enroll_fixture(repo, session)
                for row in transport.pending_inventory(repo)[0]:
                    helper.acknowledge(repo, row)
                base = boundary._base(repo)
            self.extension(receipt)
            retained = snapshot(base / 'omp-source-v1')
            before = snapshot(extension), snapshot(base)
            denied = self.public(world, receipt, 'uninstall', '--check', expected=1)
            self.assertFalse(denied['verified'])
            self.assertEqual((snapshot(extension), snapshot(base)), before)
            # Check mode cannot publish a drain proof or clear the real marker;
            # the common capability union must still refuse reader removal.
            preview = self.public(world, receipt, 'uninstall', '--check', '--omp-writers-quiesced', expected=1)
            self.assertIn('Private-state runtime compatibility', preview['error'])
            self.assertEqual((snapshot(extension), snapshot(base)), before)
            result = self.public(world, receipt, 'uninstall', '--omp-writers-quiesced')
            self.assertTrue(result['verified'])
            self.assertFalse(extension.exists())
            self.assertFalse((base / 'turn-sources-compat-v1.json').exists())
            self.assertTrue(list((base / 'omp-drains').glob('*.json')))
            self.assertEqual(snapshot(base / 'omp-source-v1'), retained)
            with patch.dict(os.environ, world.env, clear=True):
                self.assertEqual(boundary.required_source_capabilities(repo), set())

    def test_public_uninstall_keeps_pending_sources_and_disabled_owned_binding(self):
        with World() as world:
            _, _, receipt = self.managed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                helper.observe(repo, session, lifecycle, [
                    helper.entry('human', 'user', 'Still unassessed controlled source', attribution='user')])
                base = boundary._base(repo)
            extension = self.extension(receipt)
            result = self.public(world, receipt, 'uninstall', '--omp-writers-quiesced', expected=1)
            self.assertFalse(result['verified'])
            self.assertFalse(extension.exists())
            self.assertTrue((base / 'turn-sources-compat-v1.json').exists())
            self.assertFalse(list((base / 'omp-drains').glob('*.json')))
            self.assertTrue(list(extension.parent.glob('.agent-memory-disabled-*.ts.disabled')))

    def test_held_native_preview_consumes_reviewed_envelope_without_publishing(self):
        import layout_recovery_fixtures as recovery
        from runtime_update_fixtures import snapshot
        with World() as world:
            command, source, repo, bank, native, inspected, _ = recovery.small_pending(world)
            helper = NativeSources()
            with patch.dict(os.environ, world.env, clear=True):
                helper.observe(repo, str(uuid.uuid4()), str(uuid.uuid4()), [
                    helper.entry('held-human', 'user', 'Controlled pending source under the same layout hold', attribution='user')])
            root = world.inside('held native root')
            root.mkdir()
            request = recovery.review_request(world, command, source, repo, inspected, native)
            attestation = request['quiescence_attestation']
            attestation['declared_invocation_routes'] += [str(root / path) for path in
                ('skills/agent-memory', 'extensions/agent-memory.ts', 'AGENTS.md')]
            attestation['declared_invocation_routes'].append(str(command.parent / 'agent-memory'))
            def envelope(attestation):
                return {'attestation': attestation, 'review': {
                    'schema': 'agent-memory/layout-quiescence-review-v1', 'review_id': attestation['review_id'],
                    'operation_id': attestation['operation_id'], 'attestation_sha256': recovery.sha(recovery.encoded(attestation)),
                    'reviewer_developer_id': attestation['operator_developer_id'],
                    'reviewer_harness': 'controlled-native-independent-reviewer',
                    'reviewer_session': 'native-renewal-' + str(uuid.uuid4()), 'review_time': attestation['issued_at'],
                    'decision': 'approve', 'rationale': 'Controlled independent review of exact bank, artifact and complete invocation routes.'}}
            good = envelope(attestation)
            arguments = [command, '--json', 'install', '--development-source', source, '--presets', 'omp',
                         '--omp-agent-dir', root, '--omp-peers-upgraded']
            variants = []
            wrong_artifact = copy.deepcopy(attestation)
            wrong_artifact['runtime_artifact_digest'] = 'a' * 64
            variants.append(envelope(wrong_artifact))
            wrong_bank = copy.deepcopy(attestation)
            wrong_bank['bank_path'] = str(repo / 'other-bank')
            variants.append(envelope(wrong_bank))
            missing_route = copy.deepcopy(attestation)
            missing_route['declared_invocation_routes'].remove(str(root / 'extensions/agent-memory.ts'))
            variants.append(envelope(missing_route))
            missing_other_route = copy.deepcopy(attestation)
            missing_other_route['declared_invocation_routes'].remove(str(root / 'skills/agent-memory'))
            variants.append(envelope(missing_other_route))
            unreviewed = copy.deepcopy(good)
            unreviewed['review']['reviewer_harness'] = request['author']['harness']
            unreviewed['review']['reviewer_session'] = request['author']['session']
            variants.append(unreviewed)
            before = snapshot(world.paths['state']), snapshot(bank), snapshot(root)
            for value in variants:
                result = world.run([*arguments, '--check', '--omp-layout-quiescence', recovery.save(world, value)],
                                   check=False, timeout=30)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual((snapshot(world.paths['state']), snapshot(bank), snapshot(root)), before)
            proof = recovery.save(world, good)
            preview = world.run([*arguments, '--check', '--omp-layout-quiescence', proof], timeout=30)
            checked = json.loads(preview.stdout)
            self.assertEqual(checked['schema'], 'agent-memory/install-plan-v1')
            self.assertFalse(checked['installation_writes_performed'])
            self.assertEqual(checked['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1'])
            self.assertEqual((snapshot(world.paths['state']), snapshot(bank), snapshot(root)), before)
            applied = json.loads(world.run([*arguments, '--omp-layout-quiescence', proof], timeout=30).stdout)
            self.assertTrue(applied['verified'])
            self.extension(applied['receipt'])


class NativeSmokeQualification(unittest.TestCase):
    """Controlled protocol fixtures only: never native/provider qualification."""
    def fixture(self, world):
        from runtime_update_fixtures import ROOT, PACK, receipt_path
        helper = NativeSources()
        repo, session, lifecycle = helper.fixture(world)
        root = world.inside('native smoke root')
        root.mkdir()
        installed = json.loads(world.run([sys.executable, '-I', '-B', PACK / 'scripts/installer.py',
            'install', '--development-source', ROOT, '--presets', 'omp', '--omp-agent-dir', root,
            '--omp-peers-upgraded'], timeout=30).stdout)
        with patch.dict(os.environ, world.env, clear=True):
            helper.observe(repo, session, lifecycle, [
                helper.entry('historical-human', 'user', 'Historical direct source, not current native proof', attribution='user')])
        return repo, installed['receipt'], receipt_path(world)

    def binary(self, world, receipt, *, version='18.16.0', mode='disabled'):
        """Fake only the native protocol; real installed CLI verifies sources."""
        path = world.inside('controlled-native-protocol')
        am = str(Path(receipt['bin_dir']) / 'am')
        path.write_text('#!' + sys.executable + '\n' + 'VERSION = ' + repr(version) + '\nMODE = ' + repr(mode)
            + '\nAM = ' + repr(am) + '\nARTIFACT = ' + repr(receipt['source']['artifact_digest'])
            + '\nAGENT_DIR = ' + repr(receipt['omp_binding']['agent_dir']) + '\n' + r'''
import hashlib, json, re, subprocess, sys, uuid
from pathlib import Path
if '--version' in sys.argv:
    print('omp/' + VERSION); raise SystemExit()
Path(__file__).with_suffix('.launched').write_text('Controlled protocol launch, never model proof.')
root = Path(sys.argv[sys.argv.index('--session-dir') + 1]); root.mkdir()
repo = sys.argv[sys.argv.index('--cwd') + 1]
session = str(uuid.uuid4()); lifecycle = str(uuid.uuid4()); entries = []
path = root / 'current.jsonl'
path.write_text(json.dumps({'type': 'session', 'id': session, 'cwd': repo}) + '\n')
print(json.dumps({'type': 'ready'}), flush=True)
for line in sys.stdin:
    value = json.loads(line)
    if value['type'] == 'prompt':
        entry = {'type': 'message', 'id': str(uuid.uuid4()),
                 'parentId': entries[-1]['id'] if entries else None,
                 'message': {'role': 'user', 'attribution': 'user', 'content': value['message']}}
        with path.open('a') as stream:
            stream.write(json.dumps(entry) + '\n')
        entries.append(entry)
        if MODE != 'disabled' and (MODE != 'first-only' or len(entries) == 1):
            retained = json.loads(json.dumps(entries))
            if len(entries) == 2 and MODE == 'wrong-text':
                retained[-1]['message']['content'] = 'Wrong retained human source text.'
            if len(entries) == 2 and MODE == 'wrong-id':
                retained[-1]['id'] = 'foreign-native-human'
            request = {'schema': 'agent-memory/omp-observer-request-v1', 'action': 'seal',
                       'session': session, 'lifecycle': lifecycle, 'payload': {
                'entries': retained, 'branch_ids': [entry['id'] for entry in retained],
                'event_ordinal': len(entries), 'terminal': False, 'trigger': 'human',
                'inherited_ids': [], 'metadata': [], 'request_wake': False}}
            observed = subprocess.run([AM, 'omp-boundary', '--repo', repo, '--observer', '--stdin',
                '--artifact', ARTIFACT, '--omp-agent-dir', AGENT_DIR], input=json.dumps(request),
                capture_output=True, text=True, check=True)
            lifecycle = json.loads(observed.stdout)['lifecycle']
        sentinel = re.search(r'OMP_SMOKE_(?:FIRST|SECOND)_[a-f0-9]+', value['message'])[0]
        print(json.dumps({'type': 'message_end', 'message': {'role': 'assistant', 'content': sentinel}}), flush=True)
        print(json.dumps({'type': 'session_settled'}), flush=True)
    elif value['type'] == 'get_state':
        if MODE == 'missing-session':
            path.unlink()
        late = value['id'].startswith(('observer-progress-', 'observer-final-'))
        if MODE == 'late-duplicate' and late:
            print(json.dumps({'type': 'message_end', 'message': {'role': 'assistant', 'content': sentinel}}), flush=True)
        print(json.dumps({'type': 'response', 'id': value['id'], 'data': {
            'sessionId': session, 'sessionFile': str(path),
            'isSettled': not (MODE == 'late-unsettled' and late)}}), flush=True)
''')
        path.chmod(0o700)
        return path

    def smoke(self, world, repo, receipt, receipt_path, binary, *, expected=1, timeout=5):
        from runtime_update_fixtures import ROOT
        output = world.inside('controlled-smoke')
        completed = world.run([sys.executable, '-I', '-B', ROOT / 'scripts/evaluations/omp_native_smoke.py',
            '--binary', binary, '--am', Path(receipt['bin_dir']) / 'am', '--receipt', receipt_path,
            '--repo', repo, '--output', output, '--model', 'controlled/no-provider',
            '--timeout', str(timeout)], check=False, timeout=max(30, timeout * 4 + 15))
        self.assertEqual(completed.returncode, expected, completed.stdout + completed.stderr)
        report = json.loads((output / 'report.json').read_text())
        self.assertEqual(report['qualified'], expected == 0)
        self.assertFalse(report['full_native_qualification'])
        return report

    def test_historical_observers_cannot_qualify_disabled_current_extension(self):
        with World() as world:
            repo, receipt, path = self.fixture(world)
            report = self.smoke(world, repo, receipt, path, self.binary(world, receipt))
            self.assertEqual(report['status_readback']['omp']['observer_evidence'], 'retained')
            self.assertEqual(report['native_requests'], ['human-1', 'human-2'])

    def test_both_exact_current_human_turns_and_persistent_session_are_required(self):
        for mode in ('first-only', 'wrong-text', 'wrong-id', 'missing-session'):
            with self.subTest(mode=mode), World() as world:
                repo, receipt, path = self.fixture(world)
                self.smoke(world, repo, receipt, path, self.binary(world, receipt, mode=mode))

    def test_unsupported_binary_refuses_before_native_launch_or_human_turns(self):
        with World() as world:
            repo, receipt, path = self.fixture(world)
            binary = self.binary(world, receipt, version='18.14.0')
            report = self.smoke(world, repo, receipt, path, binary)
            self.assertEqual(report['native_version'], 'omp/18.14.0')
            self.assertEqual(report['native_requests'], [])
            self.assertFalse(binary.with_suffix('.launched').exists())

    def test_source_verifier_binds_completed_human_ids_text_roots_and_digests(self):
        from runtime_update_fixtures import ROOT
        spec = importlib.util.spec_from_file_location('omp_native_smoke', ROOT / 'scripts/evaluations/omp_native_smoke.py')
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        with World() as world:
            repo, receipt, _ = self.fixture(world)
            root = world.inside('source-verifier-run')
            (root / 'sessions').mkdir(parents=True)
            session = str(uuid.uuid4())
            path = root / 'sessions/current.jsonl'
            prompts = ['Controlled first human text.', 'Controlled second human text.']
            helper = NativeSources()
            humans = [helper.entry('current-first', 'user', prompts[0], attribution='user'),
                      helper.entry('current-second', 'user', prompts[1], attribution='user', parent='current-first')]
            path.write_text('\n'.join(json.dumps(value) for value in [
                {'type': 'title', 'v': 1, 'title': 'Controlled native session title',
                 'pad': '', 'updatedAt': '2026-11-06T00:00:00.000Z'},
                {'type': 'session', 'id': session, 'cwd': str(repo)}, *humans]) + '\n')
            with patch.dict(os.environ, world.env, clear=True):
                # A manual shared registration in this exact session still has
                # no observer capture/root, and cannot qualify either human.
                turn_sources.register(repo, 'omp', session, str(uuid.uuid4()), 'a' * 64)
            am = str(Path(receipt['bin_dir']) / 'am')
            status = json.loads(world.run([am, '--json', 'status', '--repo', repo], timeout=30).stdout)
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)
            with patch.dict(os.environ, world.env, clear=True):
                first = helper.observe(repo, session, str(uuid.uuid4()), humans[:1])
                helper.observe(repo, session, first['lifecycle'], humans)
                NativeLifecyclePublic().enroll_fixture(repo, session)
                for row in transport.pending_inventory(repo)[0]:
                    if row.get('human_root') and row['session'] == session:
                        helper.acknowledge(repo, row)
            evidence = smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)
            self.assertEqual([row['native_entry_id'] for row in evidence], ['current-first', 'current-second'])
            self.assertEqual(len({row['human_root'] for row in evidence}), 2)
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, str(uuid.uuid4()), str(path), prompts, status, world.env)
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, session, str(path), prompts[:1], status, world.env)
            with patch.dict(os.environ, world.env, clear=True):
                directory, _ = transport.segments(repo, session)[-1]
            cursor = directory / 'cursor.json'
            original = cursor.read_bytes()
            value = json.loads(original)
            value['origin']['human_root'] = evidence[0]['human_root']
            cursor.write_text(json.dumps(value))
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)
            cursor.write_bytes(original)
            snapshot = directory / 'snapshots' / (hashlib.sha256(b'current-second').hexdigest() + '.json')
            snapshot.write_text(snapshot.read_text().replace(prompts[1], 'Tampered source digest.'))
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)

    def test_source_verifier_waits_for_fault_free_shared_registration(self):
        from runtime_update_fixtures import ROOT
        spec = importlib.util.spec_from_file_location('omp_native_smoke', ROOT / 'scripts/evaluations/omp_native_smoke.py')
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        with World() as world:
            repo, receipt, _ = self.fixture(world)
            root = world.inside('registration-verifier-run')
            (root / 'sessions').mkdir(parents=True)
            session, lifecycle = str(uuid.uuid4()), str(uuid.uuid4())
            prompt = 'Controlled ephemeral source awaiting registration.'
            human = NativeSources().entry('registration-human', 'user', prompt, attribution='user')
            path = root / 'sessions/current.jsonl'
            path.write_text('\n'.join(json.dumps(value) for value in [
                {'type': 'session', 'id': session, 'cwd': str(repo)}, human]) + '\n')
            am = str(Path(receipt['bin_dir']) / 'am')
            status = json.loads(world.run([am, '--json', 'status', '--repo', repo], timeout=30).stdout)
            with patch.dict(os.environ, world.env, clear=True):
                directory, cursor = transport.new_segment(repo, session, lifecycle, lifecycle, 'human', 1)
                captured = transport.capture(directory, cursor, human)
                transport.enroll_admission(directory, cursor, captured)
            with self.assertRaises(smoke.PendingObservations):
                smoke.current_observations(am, repo, root, session, str(path), [prompt], status, world.env)
            with patch.dict(os.environ, world.env, clear=True):
                NativeSources().observe(repo, session, lifecycle, [human])
            evidence = smoke.current_observations(am, repo, root, session, str(path), [prompt], status, world.env)
            self.assertEqual([row['native_entry_id'] for row in evidence], ['registration-human'])
            self.assertEqual([row['human_root'] for row in evidence], [lifecycle])

    def test_late_native_frames_cannot_reuse_initial_answer_or_settled_proof(self):
        for mode in ('complete', 'late-duplicate', 'late-unsettled'):
            with self.subTest(mode=mode), World() as world:
                repo, receipt, path = self.fixture(world)
                report = self.smoke(world, repo, receipt, path,
                    self.binary(world, receipt, mode=mode),
                    expected=0 if mode == 'complete' else 1, timeout=30)
                if mode == 'complete':
                    self.assertEqual(len(report['current_human_observations']), 2)
                    self.assertEqual(len({row['human_root'] for row in report['current_human_observations']}), 2)
                elif mode == 'late-duplicate':
                    self.assertTrue(any(count > 1 for count in report['task_sentinels'].values()))
                else:
                    self.assertFalse(report['native_is_settled'])
