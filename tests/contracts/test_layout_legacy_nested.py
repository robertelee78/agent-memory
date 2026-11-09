"""Old recovery tools cannot reinterpret a real interrupted nested admission."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from runtime_update_fixtures import snapshot
from contracts import test_layout_nested_reconciliation as nested
import layout_recovery_fixtures as recovery
import v014_runtime_fixture


class LayoutLegacyNested(unittest.TestCase):
    def journey(self, receipted):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            command, source, repo, bank, _, inspected, applied = recovery.small_pending(world)
            feature = repo / 'feature.txt'
            feature.write_text('Feature baseline\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
            feature.write_text('Staged feature\n')
            world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged feature\n')
            operation = inspected['operation_id']
            directory = recovery.private_directory(world, bank, operation)
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            hold = (bank / '.agent-memory-transaction.md').read_bytes()
            original_bank = nested.canonical(bank)
            proposal, transaction, _ = recovery.admission_proposal(world, command, source, repo, bank, inspected,
                                                                   full=receipted)
            stage = 'layout-recovery-nested-after-receipt' if receipted else 'layout-recovery-after-nested-marker'
            fault = world.run([command, 'admission', 'apply', '--proposal', proposal], check=False, timeout=30,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(fault.returncode, 86, fault.stdout + fault.stderr)
            marker = bank / '.agent-memory-transaction.md'
            marker_bytes = marker.read_bytes()
            self.assertEqual(json.loads(marker_bytes)['schema'], 'agent-memory/layout-frontier-transaction-v1')
            preview = world.run([old, 'writes', 'recover', '--repo', repo], check=False, timeout=60)
            self.assertEqual(preview.returncode, 0, preview.stdout + preview.stderr)
            unlock = world.run([old, 'writes', 'recover', '--repo', repo,
                                '--proposal', recovery.save(world, json.loads(preview.stdout))], check=False, timeout=60)
            self.assertEqual(unlock.returncode, 0, unlock.stdout + unlock.stderr)
            self.assertFalse((bank / '.agent-memory.lock').exists())
            bank_before = snapshot(bank)
            private_before = snapshot(directory)
            index = (repo / '.git/index').read_bytes()
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            for args in (['transaction', 'recover', '--repo', repo],
                         ['migrate', 'recover', '--repo', repo, '--mode', 'finish'],
                         ['migrate', 'recover', '--repo', repo, '--mode', 'rollback']):
                result = world.run([old, *args], check=False, timeout=60)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual((snapshot(bank), snapshot(directory)), (bank_before, private_before))
                self.assertEqual(marker.read_bytes(), marker_bytes)
            doctor = world.run([old, 'doctor', '--fix', '--repo', repo, '--json'], check=False, timeout=60)
            self.assertFalse(json.loads(doctor.stdout)['repairs_performed'])
            self.assertEqual((snapshot(bank), snapshot(directory)), (bank_before, private_before))
            helper = nested.NestedReconciliationContract()
            mode = 'finish' if receipted else 'rollback'
            helper.preview(world, command, repo, operation, transaction['operation_id'],
                           'rollback' if receipted else 'finish', expected=1)
            known, _ = helper.preview(world, command, repo, operation, transaction['operation_id'], mode)
            result = recovery.call(world, command, 'recover-nested', '--repo', repo, '--proposal', known)
            self.assertTrue(result['verified'])
            if receipted:
                self.assertFalse(marker.exists())
                status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertTrue(status['semantic_recovery_complete'])
            else:
                self.assertEqual(marker.read_bytes(), hold)
                self.assertEqual(nested.canonical(bank), original_bank)
                status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(feature.read_text(), 'Unstaged feature\n')

    def test_without_nested_receipt(self):
        self.journey(False)

    def test_with_nested_receipt(self):
        self.journey(True)
