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
    print(VERSION); raise SystemExit()
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
        print(json.dumps({'type': 'response', 'id': value['id'], 'data': {
            'sessionId': session, 'sessionFile': str(path), 'isSettled': True}}), flush=True)
''')
        path.chmod(0o700)
        return path

    def smoke(self, world, repo, receipt, receipt_path, binary):
        from runtime_update_fixtures import ROOT
        output = world.inside('controlled-smoke')
        completed = world.run([sys.executable, '-I', '-B', ROOT / 'scripts/evaluations/omp_native_smoke.py',
            '--binary', binary, '--am', Path(receipt['bin_dir']) / 'am', '--receipt', receipt_path,
            '--repo', repo, '--output', output, '--model', 'controlled/no-provider',
            '--timeout', '5'], check=False, timeout=30)
        self.assertEqual(completed.returncode, 1, completed.stdout + completed.stderr)
        report = json.loads((output / 'report.json').read_text())
        self.assertFalse(report['qualified'])
        self.assertFalse(report['full_native_qualification'])
        self.assertTrue(report['failures'])
        return report

    def test_historical_observers_cannot_qualify_disabled_current_extension(self):
        with World() as world:
            repo, receipt, path = self.fixture(world)
            report = self.smoke(world, repo, receipt, path, self.binary(world, receipt))
            self.assertEqual(report['status_readback']['omp']['observer_evidence'], 'retained')
            self.assertIn('Current native session has no retained observer evidence', report['failures'][0])

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
            self.assertEqual(report['native_version'], '18.14.0')
            self.assertEqual(report['native_requests'], [])
            self.assertFalse(binary.with_suffix('.launched').exists())
            self.assertIn('require exactly 18.16.0', report['failures'][0])

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
