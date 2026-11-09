"""The published old runtime can clear its stale lock, not a typed recovery marker."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from runtime_update_fixtures import snapshot
from contracts import test_layout_recovery as existing
import layout_recovery_fixtures as recovery
import v014_runtime_fixture


class LayoutLegacyStaleLock(unittest.TestCase):
    def test_old_stale_lock_recovery_preserves_interrupted_apply(self):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['version'], '0.0.14')
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            helper = existing.LayoutRecoveryContract()
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            fault = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                              check=False, timeout=60, env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-after-marker',
                                                           'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(fault.returncode, 86, fault.stdout + fault.stderr)
            marker = bank / '.agent-memory-transaction.md'
            marker_bytes = marker.read_bytes()
            self.assertTrue((bank / '.agent-memory.lock').exists())
            preview = world.run([old, 'writes', 'recover', '--repo', repo], check=False, timeout=60)
            self.assertEqual(preview.returncode, 0, preview.stdout + preview.stderr)
            lock_request = json.loads(preview.stdout)
            self.assertTrue(lock_request.get('proposal'))
            unlocked = world.run([old, 'writes', 'recover', '--repo', repo, '--proposal', recovery.save(world, lock_request)],
                                 check=False, timeout=60)
            self.assertEqual(unlocked.returncode, 0, unlocked.stdout + unlocked.stderr)
            self.assertFalse((bank / '.agent-memory.lock').exists())
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            before = snapshot(bank), snapshot(directory), (repo / '.git/index').read_bytes()
            self.assertEqual(marker.read_bytes(), marker_bytes)
            for args in (['writes', 'recover', '--repo', repo],
                         ['migrate', 'recover', '--repo', repo, '--mode', 'finish'],
                         ['migrate', 'recover', '--repo', repo, '--mode', 'rollback']):
                refused = world.run([old, *args], check=False, timeout=60)
                self.assertNotEqual(refused.returncode, 0, refused.stdout + refused.stderr)
                self.assertEqual((snapshot(bank), snapshot(directory), (repo / '.git/index').read_bytes()), before)
                self.assertEqual(marker.read_bytes(), marker_bytes)
            doctor = world.run([old, 'doctor', '--fix', '--repo', repo, '--json'], check=False, timeout=60)
            self.assertFalse(json.loads(doctor.stdout)['repairs_performed'])
            self.assertEqual((snapshot(bank), snapshot(directory), (repo / '.git/index').read_bytes()), before)
            preview_path = world.inside('legacy-stale-aware-finish.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                          '--mode', 'finish', '--output', preview_path)
            result = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview_path)
            self.assertTrue(result['verified'])
            self.assertFalse(result['semantic_recovery_complete'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertTrue((bank / '.agent-memory-transaction.md').exists())
            helper.assert_fault_work_unchanged(world, repo, git)
