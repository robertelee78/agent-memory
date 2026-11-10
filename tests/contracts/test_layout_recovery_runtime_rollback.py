"""Real capable runtime rollback with a public pending layout hold."""
import json
import os
from pathlib import Path
import sys
import unittest
import uuid

from world import World
import layout_recovery_fixtures as recovery
from runtime_update_fixtures import ROOT, advance, call, fixture, owned, receipt_path, snapshot


class HeldRuntimeRollbackContract(unittest.TestCase):
    def test_interrupted_capable_update_rolls_back_exact_runtime_and_keeps_hold(self):
        with World() as world:
            for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            channel, pack, receipt = fixture(world)
            command = Path(receipt['bin_dir']) / 'am'
            repo, bank, native = recovery.memory(world)
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            _, inspected = recovery.inspect(world, command, repo)
            request = recovery.review_request(world, command, pack, repo, inspected, native)
            request['quiescence_attestation']['declared_invocation_routes'] = sorted(set(
                request['quiescence_attestation']['declared_invocation_routes']
                + [target['path'] for target in receipt['targets']]))
            proposal = world.inside('held-rollback-layout.json')
            recovery.call(world, command, 'prepare', '--repo', repo,
                          '--operation', inspected['operation_id'], '--input', recovery.save(world, request),
                          '--output', proposal)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            previous = owned(receipt), receipt_path(world).read_bytes()
            advance(world, channel, pack)
            attestation = recovery.review_request(world, command, pack, repo, inspected, native)['quiescence_attestation']
            attestation['declared_invocation_routes'] = sorted(set(attestation['declared_invocation_routes']
                                                                     + [target['path'] for target in receipt['targets']]))
            envelope = {'attestation': attestation, 'review': {
                'schema': 'agent-memory/layout-quiescence-review-v1', 'review_id': attestation['review_id'],
                'operation_id': attestation['operation_id'], 'attestation_sha256': recovery.sha(recovery.encoded(attestation)),
                'reviewer_developer_id': attestation['operator_developer_id'], 'reviewer_harness': 'fixture-independent-reviewer',
                'reviewer_session': 'rollback-' + str(uuid.uuid4()), 'review_time': attestation['issued_at'],
                'decision': 'approve', 'rationale': 'Review the exact disposable capable candidate and declared routes.'}}
            renewal = recovery.save(world, envelope)
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            before_bank = snapshot(bank)
            call(world, receipt, '--layout-quiescence', renewal, expected=86,
                 env={'AGENT_MEMORY_FAULT_STAGE': 'update-after-activate-0', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            before_private = snapshot(directory)
            result = world.run([sys.executable, '-I', ROOT / 'scripts/installer.py', 'recover'],
                               check=False, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue(json.loads(result.stdout)['verified'])
            self.assertEqual((owned(receipt), receipt_path(world).read_bytes()), previous)
            self.assertEqual(snapshot(bank), before_bank)
            self.assertEqual(snapshot(directory), before_private)
            status = recovery.call(world, command, 'status', '--repo', repo,
                                   '--operation', inspected['operation_id'])
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertEqual(set(status['pending_unit_ids']), set(applied['pending_unit_ids']))
