"""Complete the complementary error/exit cells for reviewed resolution retry."""
import json
import unittest

from world import World
from contracts import test_layout_recovery_reconciliation as reconciliation
import layout_recovery_fixtures as recovery


class LayoutResolutionFaults(unittest.TestCase):
    def journey(self, stage, mode):
        with World() as world:
            command, _, repo, bank, _, inspected, applied = recovery.small_pending(world, reconciliation.SCRATCH)
            feature = repo / 'feature.txt'
            feature.write_text('Feature baseline\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
            feature.write_text('Staged feature\n')
            world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged feature\n')
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            canonical = reconciliation.bank_bytes(bank)
            index = (repo / '.git/index').read_bytes()
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            proposal = recovery.discard_proposal(world, command, repo, bank, inspected, applied['pending_unit_ids'][0])
            request = json.loads(proposal.read_text())['proposal']
            fault = world.run([command, 'bank', 'recover-layout', 'resolve', '--repo', repo, '--proposal', proposal],
                              check=False, timeout=30,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            receipt_path = directory / 'resolutions' / (request['resolution_id'] + '.json')
            receipt = receipt_path.read_bytes()
            recovery.call(world, command, 'resolve', '--repo', repo, '--proposal', proposal)
            history = recovery.call(world, command, 'resolve', '--repo', repo, '--proposal', proposal)
            self.assertTrue(history['already_applied'])
            self.assertFalse(history['writes_performed'])
            self.assertEqual(receipt_path.read_bytes(), receipt)
            self.assertEqual(reconciliation.bank_bytes(bank), canonical)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(feature.read_text(), 'Unstaged feature\n')
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
            self.assertTrue(status['semantic_recovery_complete'])
            self.assertEqual(status['pending_units'], 0)
            self.assertFalse((bank / '.agent-memory-transaction.md').exists())
            self.assertFalse((bank / '.agent-memory-recovery.md').exists())

    def test_receipt_error(self):
        self.journey('layout-recovery-resolution-after-receipt', 'error')

    def test_before_head_exit(self):
        self.journey('layout-recovery-ledger-generation-before-head', 'exit')

    def test_after_head_error(self):
        self.journey('layout-recovery-ledger-generation-after-head', 'error')

    def test_marker_removal_error(self):
        self.journey('layout-recovery-retirement-after-marker-removal', 'error')

    def test_sentinel_removal_exit(self):
        self.journey('layout-recovery-retirement-after-sentinel-removal', 'exit')
