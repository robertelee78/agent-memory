"""Public recovery boundaries at nested and legacy lifecycle transitions."""
import json
import os
import shutil
import sys
import unittest
from pathlib import Path

from world import World
import layout_recovery_fixtures as f
import v014_runtime_fixture
from runtime_update_fixtures import PACK, ROOT, owned, receipt_path, snapshot
from contracts.test_layout_recovery_reconciliation import SCRATCH


class NestedRecoveryClosure(unittest.TestCase):
    def test_receipted_nested_finish_reconciles_resolution_receipt_window(self):
        for stage in ('layout-recovery-nested-after-receipt',
                      'layout-recovery-admission-receipt-before-resolution'):
            with self.subTest(stage=stage), World() as world:
                command, source, repo, bank, _, inspected, applied = f.small_pending(world)
                operation = inspected['operation_id']
                directory = f.private_directory(world, bank, operation)
                proposal, transaction, target = f.admission_proposal(
                    world, command, source, repo, bank, inspected, full=True)
                f.public(world, command, 'admission', 'apply', '--proposal', proposal, expected=1,
                         env={'AGENT_MEMORY_FAULT_STAGE': stage})
                nested = transaction['operation_id']
                marker = json.loads((bank / '.agent-memory-transaction.md').read_text())
                self.assertEqual(marker['operation_id'], operation)
                self.assertEqual(marker['nested_id'], nested)
                self.assertTrue((bank / '.agent-memory-recovery.md').is_file())
                canonical = {path.relative_to(bank).as_posix(): path.read_bytes()
                             for path in bank.rglob('*')
                             if path.is_file() and not path.name.startswith('.agent-memory')}
                self.assertEqual(canonical[target], transaction['files'][target].encode())
                receipts = directory / 'resolutions'
                existing = {path.name: path.read_bytes() for path in receipts.glob('*.json')}
                self.assertEqual(len(existing), 1 if stage.endswith('before-resolution') else 0)
                self.assertEqual(json.loads((directory / 'ledger-head.json').read_text())['generation'], 0)
                f.call(world, command, 'recover-nested', '--repo', repo, '--operation', operation,
                       '--nested', nested, '--mode', 'rollback', '--output',
                       world.inside('nested-refused.json'), expected=1)
                finish = world.inside('nested-finish.json')
                f.call(world, command, 'recover-nested', '--repo', repo, '--operation', operation,
                       '--nested', nested, '--mode', 'finish', '--output', finish)
                f.call(world, command, 'recover-nested', '--repo', repo, '--proposal', finish)
                self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes()
                                  for path in bank.rglob('*')
                                  if path.is_file() and not path.name.startswith('.agent-memory')}, canonical)
                ledger = json.loads((directory / 'ledger/00000001.json').read_text())
                unit = next(row for row in ledger['units'] if row['unit_id'] == applied['pending_unit_ids'][0])
                self.assertEqual(unit['state'], 'admitted')
                self.assertEqual(len(unit['terminal_receipts']), 1)
                link = unit['terminal_receipts'][0]
                receipt_path = receipts / (link['resolution_id'] + '.json')
                resolution = json.loads(receipt_path.read_text())
                self.assertEqual(resolution['canonical_operation_id'], nested)
                self.assertEqual(link['sha256'], f.sha(f.encoded(resolution)))
                for name, data in existing.items():
                    self.assertEqual((receipts / name).read_bytes(), data)
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertFalse((bank / '.agent-memory-recovery.md').exists())
                status = f.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertTrue(status['semantic_recovery_complete'])
                before = snapshot(bank), (directory / 'ledger-head.json').read_bytes(), receipt_path.read_bytes()
                retry = world.inside('nested-finish-retry.json')
                f.call(world, command, 'recover-nested', '--repo', repo, '--operation', operation,
                       '--nested', nested, '--mode', 'finish', '--output', retry)
                f.call(world, command, 'recover-nested', '--repo', repo, '--proposal', retry)
                self.assertEqual((snapshot(bank), (directory / 'ledger-head.json').read_bytes(),
                                  receipt_path.read_bytes()), before)

    def test_unfinished_resolution_excludes_nested_child_finish(self):
        with World() as world:
            command, _, repo, bank, _, inspected, applied = f.small_pending(world, SCRATCH)
            parent = inspected['operation_id']
            child_file = world.inside('child.json')
            child = f.call(world, command, 'rollback', '--repo', repo, '--operation', parent, '--output', child_file)
            child_id = child['operation_id']
            resolution = f.discard_proposal(world, command, repo, bank, inspected, applied['pending_unit_ids'][0])
            # The child journal is durable, but the parent hold still owns the
            # marker, so the standalone resolution can reach its receipt window.
            f.call(world, command, 'rollback', '--repo', repo, '--proposal', child_file, expected=1,
                   env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-marker-swap'})
            marker = bank / '.agent-memory-transaction.md'
            self.assertEqual(json.loads(marker.read_text())['operation_id'], parent)
            finish = world.inside('finish-child.json')
            f.call(world, command, 'recover', '--repo', repo, '--operation', child_id,
                   '--mode', 'finish', '--output', finish)
            f.call(world, command, 'resolve', '--repo', repo, '--proposal', resolution, expected=1,
                   env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-resolution-after-receipt'})
            base = f.private_directory(world, bank, parent).parent
            before = snapshot(bank), snapshot(base), snapshot(repo / '.git/index')
            f.call(world, command, 'recover', '--repo', repo, '--proposal', finish, expected=1)
            self.assertEqual((snapshot(bank), snapshot(base), snapshot(repo / '.git/index')), before)
            done = f.call(world, command, 'resolve', '--repo', repo, '--proposal', resolution)
            self.assertTrue(done['semantic_recovery_complete'])
            # The old child preview is pinned to the former hold/frontier.
            f.call(world, command, 'recover', '--repo', repo, '--proposal', finish, expected=1)

    def test_changed_review_pin_refuses_interrupted_finish_without_replacement(self):
        with World() as world:
            command, source = f.installed(world)
            repo, bank, native = f.memory(world)
            (bank / 'handoff-prompt.md').write_text(SCRATCH)
            _, inspected = f.inspect(world, command, repo)
            proposal, envelope = f.prepare(world, command, source, repo, inspected, native)
            f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                   env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-receipt'})
            finish = world.inside('finish.json')
            f.call(world, command, 'recover', '--repo', repo, '--operation', inspected['operation_id'],
                   '--mode', 'finish', '--output', finish)
            directory = f.private_directory(world, bank, inspected['operation_id'])
            review = directory / 'reviews' / (envelope['proposal']['review_id'] + '.json')
            original = review.read_bytes()
            altered = json.loads(original)
            altered['evidence'] += ' Changed after review.'
            review.write_bytes(f.encoded(altered))
            before = snapshot(bank), snapshot(directory), snapshot(repo / '.git/index')
            f.call(world, command, 'recover', '--repo', repo, '--proposal', finish, expected=1)
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(repo / '.git/index')), before)
            review.write_bytes(original)
            result = f.call(world, command, 'recover', '--repo', repo, '--proposal', finish)
            self.assertTrue(result['verified'])

    def test_child_transaction_marker_binds_child_operation_id(self):
        with World() as world:
            command, _, repo, bank, _, inspected, _ = f.small_pending(world, SCRATCH)
            output = world.inside('restore.json')
            child = f.call(world, command, 'rollback', '--repo', repo, '--operation', inspected['operation_id'], '--output', output)
            f.call(world, command, 'rollback', '--repo', repo, '--proposal', output, expected=1,
                   env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-after-marker-swap'})
            marker = bank / '.agent-memory-transaction.md'
            actual = json.loads(marker.read_text())
            self.assertNotEqual(child['operation_id'], inspected['operation_id'])
            self.assertEqual(actual['operation_id'], child['operation_id'])
            self.assertEqual(actual['transaction_kind'], 'rollback')
            plan = world.inside('child-finish.json')
            f.call(world, command, 'recover', '--repo', repo, '--operation', child['operation_id'],
                   '--mode', 'finish', '--output', plan)
            result = f.call(world, command, 'recover', '--repo', repo, '--proposal', plan)
            self.assertTrue(result['verified'])
            self.assertEqual(json.loads(marker.read_text())['operation_id'], inspected['operation_id'])

    def test_legacy_reader_does_not_recognize_runtime_only_sentinel(self):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', '-B', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            old_receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(old_receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(old_receipt['bin_dir']) / 'am'
            _, _, repo, bank, _, inspected, _ = f.small_pending(world, SCRATCH)
            marker = bank / '.agent-memory-transaction.md'
            sentinel = bank / '.agent-memory-recovery.md'
            self.assertTrue(sentinel.is_file())
            # Deliberately model an unprotected legacy invocation route. The
            # sentinel alone is not a substitute for quiescing old consumers.
            marker.unlink()
            directory = f.private_directory(world, bank, inspected['operation_id'])
            before = snapshot(bank), snapshot(directory), snapshot(repo / '.git/index')
            read = world.run([old, '--json', 'bank', 'validate', '--repo', repo], check=False, timeout=60)
            self.assertEqual(read.returncode, 0, read.stdout + read.stderr)
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(repo / '.git/index')), before)

    def test_interrupted_activation_refuses_incapable_only_rollback_target(self):
        with World() as world, World() as helper:
            # Match the recovery fixture's realizable group before creating
            # the bank; the independently installed helper has its own World.
            for directory in [world.root, *(path for path in world.root.rglob('*') if path.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            old = v014_runtime_fixture.extract(world.inside('published-old'))
            channel = world.repository('legacy channel')
            pack = channel / 'skills/agent-memory'
            shutil.copytree(old, pack)
            world.git('add', '.', cwd=channel)
            world.git('commit', '-m', 'Published old runtime', cwd=channel)
            installed = world.run([sys.executable, '-I', '-B', old / 'scripts/installer.py', 'install',
                '--remote', channel, '--ref', 'main', '--mode', 'copy', '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['version'], '0.0.14')
            self.assertNotIn('layout-frontier-v1',
                             receipt['source']['compatibility'].get('private_state_capabilities', []))
            repo, bank, native = f.memory(world)
            shutil.rmtree(pack)
            shutil.copytree(PACK, pack)
            world.git('add', '-A', cwd=channel)
            world.git('commit', '-m', 'Capable runtime', cwd=channel)
            interrupted = world.run([sys.executable, '-I', '-B', ROOT / 'scripts/update_runtime.py', 'update'],
                check=False, timeout=60, env={'AGENT_MEMORY_FAULT_STAGE': 'update-after-activate-0',
                                              'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(interrupted.returncode, 86, interrupted.stdout + interrupted.stderr)
            command, source = f.installed(helper)
            (bank / 'handoff-prompt.md').write_text(SCRATCH)
            _, inspected = f.inspect(world, command, repo)
            proposal, _ = f.prepare(world, command, source, repo, inspected, native)
            f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            directory = f.private_directory(world, bank, inspected['operation_id'])
            state = receipt_path(world).parent
            before = snapshot(bank), snapshot(directory), owned(receipt), snapshot(state), snapshot(repo / '.git/index')
            recovered = world.run([sys.executable, '-I', '-B', ROOT / 'scripts/installer.py', 'recover'],
                                  check=False, timeout=60)
            self.assertNotEqual(recovered.returncode, 0, recovered.stdout + recovered.stderr)
            refusal = json.loads(recovered.stdout)
            self.assertFalse(refusal['verified'])
            self.assertIn('Private-state runtime compatibility is pending', refusal['error'])
            self.assertEqual((snapshot(bank), snapshot(directory), owned(receipt), snapshot(state),
                              snapshot(repo / '.git/index')), before)


if __name__ == '__main__':
    unittest.main()
