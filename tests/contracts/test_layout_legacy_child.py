"""Old tools cannot take ownership of a real interrupted child restoration."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from contracts import test_layout_recovery as existing
import layout_recovery_fixtures as recovery
import legacy_recovery_fixtures as legacy_guard
import v014_runtime_fixture


class LayoutLegacyChild(unittest.TestCase):
    def journey(self, stage):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            helper = existing.LayoutRecoveryContract()
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            parent = recovery.private_directory(world, bank, safe['operation_id'])
            parent_receipt = (parent / 'receipt.json').read_bytes()
            child_path = world.inside('child-preview.json')
            child = recovery.call(world, command, 'rollback', '--repo', repo, '--operation', safe['operation_id'],
                                  '--output', child_path)
            child_id = child['operation_id']
            self.assertNotEqual(child_id, safe['operation_id'])
            fault = world.run([command, 'bank', 'recover-layout', 'rollback', '--repo', repo, '--proposal', child_path],
                              check=False, timeout=60,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(fault.returncode, 86, fault.stdout + fault.stderr)
            marker = json.loads((bank / '.agent-memory-transaction.md').read_bytes())
            self.assertEqual(marker['operation_id'], child_id)
            directory = recovery.private_directory(world, bank, child_id)
            journal = json.loads((directory / 'journal.json').read_bytes())
            self.assertEqual(journal['parent_operation_id'], safe['operation_id'])
            self.assertEqual(journal['parent_receipt_sha256'], recovery.sha(parent_receipt))
            legacy_guard.guard(self, world, old, pack, repo, bank, directory, inspected, native)
            preview = world.inside('child-known-finish.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', child_id,
                          '--mode', 'finish', '--output', preview)
            finished = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview)
            self.assertTrue(finished['verified'])
            self.assertEqual((parent / 'receipt.json').read_bytes(), parent_receipt)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*')
                              if p.is_file() and p.name not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')}, originals)
            self.assertEqual({path: (bank / path).stat().st_mode & 0o7777 for path in originals}, modes)
            helper.assert_fault_work_unchanged(world, repo, git)

    def test_handoff(self):
        self.journey('layout-recovery-after-marker-swap')

    def test_receipt(self):
        self.journey('layout-recovery-after-receipt')
