"""Disposable public lifecycle boundaries; synthetic sources are not native-client proof."""
import copy
import json
from pathlib import Path
import unittest
import uuid

from world import World
import layout_recovery_fixtures as recovery
from runtime_update_fixtures import snapshot
from contracts import test_omp_boundary


class MixedPrivateCapabilities(unittest.TestCase):
    def test_layout_first_native_activation_requires_reviewed_new_routes_without_omp_preseed(self):
        with World() as world:
            command, source, repo, bank, native, inspected, applied = recovery.small_pending(world)
            self.assertFalse(applied['semantic_recovery_complete'])
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            originals = {row['source_id']: (directory / row['blob']).read_bytes()
                         for row in inspected['manifest']['sources']}
            root = world.inside('explicit omp root'); root.mkdir()
            request = recovery.review_request(world, command, source, repo, inspected, native)
            attestation = request['quiescence_attestation']
            attestation['declared_invocation_routes'] += [str(root / path) for path in
                ('skills/agent-memory', 'extensions/agent-memory.ts', 'AGENTS.md')]
            attestation['declared_invocation_routes'].append(str(command.parent / 'agent-memory'))

            def envelope(att):
                return {'attestation': att, 'review': {
                    'schema': 'agent-memory/layout-quiescence-review-v1', 'review_id': att['review_id'],
                    'operation_id': att['operation_id'], 'attestation_sha256': recovery.sha(recovery.encoded(att)),
                    'reviewer_developer_id': att['operator_developer_id'], 'reviewer_harness': 'fixture-independent-reviewer',
                    'reviewer_session': 'mixed-' + str(uuid.uuid4()), 'review_time': att['issued_at'],
                    'decision': 'approve', 'rationale': 'Disposable independent review of the exact bank and new invocation routes.'}}

            args = [command, '--json', 'install', '--development-source', source, '--presets', 'omp',
                    '--omp-agent-dir', root, '--omp-peers-upgraded']
            def preserved():
                return (snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                        (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout,
                        {row['source_id']: (directory / row['blob']).read_bytes()
                         for row in inspected['manifest']['sources']})
            before = preserved()
            baseline = snapshot(world.paths['state']), snapshot(root)
            wrong = copy.deepcopy(attestation)
            wrong['declared_invocation_routes'].remove(str(root / 'extensions/agent-memory.ts'))
            denied = world.run([*args, '--check', '--omp-layout-quiescence', recovery.save(world, envelope(wrong))],
                               check=False, timeout=60)
            self.assertNotEqual(denied.returncode, 0, denied.stdout + denied.stderr)
            self.assertEqual(preserved(), before)
            self.assertEqual((snapshot(world.paths['state']), snapshot(root)), baseline)
            proof = recovery.save(world, envelope(attestation))
            preview = json.loads(world.run([*args, '--check', '--omp-layout-quiescence', proof], timeout=60).stdout)
            self.assertEqual(preview['private_compatibility']['required_capabilities'], ['layout-frontier-v1'])
            self.assertEqual(preserved(), before)
            self.assertEqual((snapshot(world.paths['state']), snapshot(root)), baseline)
            result = json.loads(world.run([*args, '--omp-layout-quiescence', proof], timeout=60).stdout)
            self.assertTrue(result['verified'])
            test_omp_boundary.NativeLifecyclePublic().extension(result['receipt'])
            self.assertEqual(snapshot(bank), before[0])
            self.assertEqual(snapshot(world.paths['config']), before[2])
            self.assertEqual((repo / '.git/index').read_bytes(), before[3])
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, before[4])
            self.assertEqual({row['source_id']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
