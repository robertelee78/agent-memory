"""A prior real nested receipt cannot authorize old recovery of a held bank."""
import json
from pathlib import Path
import sys
import unittest
import uuid

from world import World
from runtime_update_fixtures import snapshot
import layout_recovery_fixtures as recovery
import v014_runtime_fixture


class LayoutLegacyHoldReceipt(unittest.TestCase):
    def journey(self, decoy):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            command, source, repo, bank, _, inspected, applied = recovery.small_pending(world)
            feature = repo / 'feature.txt'; feature.write_text('Feature baseline\n')
            world.git('add', 'feature.txt', cwd=repo); world.git('commit', '-m', 'Feature baseline', cwd=repo)
            feature.write_text('Staged feature\n'); world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged feature\n')
            index = (repo / '.git/index').read_bytes()
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            first, _, _ = recovery.admission_proposal(world, command, source, repo, bank, inspected, full=False)
            admitted = recovery.public(world, command, 'admission', 'apply', '--proposal', first)
            actual_receipt = Path(admitted['receipt_path'])
            receipt_bytes = actual_receipt.read_bytes()
            self.assertTrue(json.loads(receipt_bytes)['verified'])
            second, _, _ = recovery.admission_proposal(world, command, source, repo, bank, inspected, full=False)
            fault = world.run([command, 'admission', 'apply', '--proposal', second], check=False, timeout=30,
                              env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-nested-marker',
                                   'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(fault.returncode, 86, fault.stdout + fault.stderr)
            marker = bank / '.agent-memory-transaction.md'; marker_bytes = marker.read_bytes()
            self.assertEqual(json.loads(marker_bytes)['schema'], 'agent-memory/layout-frontier-hold-v1')
            decoy_path = None
            if decoy:
                identity = str(uuid.uuid4())
                decoy_path = directory.parent.parent / 'transactions' / identity
                decoy_path.mkdir(parents=True, mode=0o700)
                journal = decoy_path / 'journal.json'
                journal.write_text(json.dumps({'schema': 'agent-memory/transaction-journal-v1',
                                               'proposal': {'operation_id': identity}, 'base_files': {}}))
                journal.chmod(0o600)
            preview = world.run([old, 'writes', 'recover', '--repo', repo], check=False, timeout=60)
            self.assertEqual(preview.returncode, 0, preview.stdout + preview.stderr)
            unlocked = world.run([old, 'writes', 'recover', '--repo', repo,
                                  '--proposal', recovery.save(world, json.loads(preview.stdout))], check=False, timeout=60)
            self.assertEqual(unlocked.returncode, 0, unlocked.stdout + unlocked.stderr)
            before = snapshot(bank), snapshot(directory), snapshot(decoy_path) if decoy_path else None
            for args in (['transaction', 'recover', '--repo', repo],
                         ['migrate', 'recover', '--repo', repo, '--mode', 'finish'],
                         ['migrate', 'recover', '--repo', repo, '--mode', 'rollback']):
                refused = world.run([old, *args], check=False, timeout=60)
                self.assertNotEqual(refused.returncode, 0, refused.stdout + refused.stderr)
                self.assertEqual((snapshot(bank), snapshot(directory), snapshot(decoy_path) if decoy_path else None), before)
                self.assertEqual(marker.read_bytes(), marker_bytes)
                self.assertEqual(actual_receipt.read_bytes(), receipt_bytes)
            doctor = world.run([old, 'doctor', '--fix', '--repo', repo, '--json'], check=False, timeout=60)
            self.assertFalse(json.loads(doctor.stdout)['repairs_performed'])
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(decoy_path) if decoy_path else None), before)
            recovery.public(world, command, 'admission', 'apply', '--proposal', second)
            self.assertEqual(marker.read_bytes(), marker_bytes)
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
            self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertEqual(actual_receipt.read_bytes(), receipt_bytes)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(feature.read_text(), 'Unstaged feature\n')

    def test_existing_receipt(self):
        self.journey(False)

    def test_existing_receipt_with_decoy(self):
        self.journey(True)
