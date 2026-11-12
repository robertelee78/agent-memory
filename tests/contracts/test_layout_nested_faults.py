"""Real nested admission rollback interruption preserves its hold and sources."""
import json
from pathlib import Path
import unittest

from world import World
from contracts import test_layout_nested_reconciliation as nested
import layout_recovery_fixtures as recovery


class LayoutNestedFaults(unittest.TestCase):
    def feature(self, world, repo):
        path = repo / 'feature.txt'
        path.write_text('Feature baseline\n')
        world.git('add', 'feature.txt', cwd=repo)
        world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
        path.write_text('Staged feature\n')
        world.git('add', 'feature.txt', cwd=repo)
        path.write_text('Unstaged feature\n')
        return (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout

    def assert_feature(self, world, repo, saved):
        self.assertEqual((repo / '.git/index').read_bytes(), saved[0])
        self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, saved[1])
        self.assertEqual((repo / 'feature.txt').read_text(), 'Unstaged feature\n')

    def rollback_journey(self, stage, mode, *, changed_path='event'):
        helper = nested.NestedReconciliationContract()
        with World() as world:
            command, source, repo, bank, native, inspected, applied = recovery.small_pending(world)
            feature = self.feature(world, repo)
            operation = inspected['operation_id']
            directory = recovery.private_directory(world, bank, operation)
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            hold = (bank / '.agent-memory-transaction.md').read_bytes()
            sentinel = (bank / '.agent-memory-recovery.md').read_bytes()
            before = nested.canonical(bank)
            modes = {path: (bank / path).stat().st_mode & 0o7777 for path in before}
            proposal, transaction, target = recovery.admission_proposal(world, command, source, repo, bank, inspected, full=False)
            recovery.public(world, command, 'admission', 'apply', '--proposal', proposal, expected=1,
                            env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-nested-before-receipt'})
            helper.preview(world, command, repo, operation, transaction['operation_id'], 'finish', expected=1)
            preview, _ = helper.preview(world, command, repo, operation, transaction['operation_id'], 'rollback')
            if stage.endswith('after-progress'):
                paths = {'event': 'history/events/' + transaction['operation_id'] + '.md',
                         'target': target, 'router': 'MEMORY.md'}
                stage += ':' + paths[changed_path]
            fault = world.run([command, 'bank', 'recover-layout', 'recover-nested', '--repo', repo, '--proposal', preview],
                              check=False, timeout=30,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            nested_dir = directory / 'nested' / transaction['operation_id']
            self.assertTrue((nested_dir / 'rollback-plan.json').is_file())
            again, _ = helper.preview(world, command, repo, operation, transaction['operation_id'], 'rollback')
            recovery.call(world, command, 'recover-nested', '--repo', repo, '--proposal', again)
            self.assertEqual(nested.canonical(bank), before)
            self.assertEqual({path: (bank / path).read_text() for path in native}, native)
            self.assertEqual({path: (bank / path).stat().st_mode & 0o7777 for path in before}, modes)
            self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), hold)
            self.assertEqual((bank / '.agent-memory-recovery.md').read_bytes(), sentinel)
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
            self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assert_feature(world, repo, feature)

    def test_plan_error(self):
        self.rollback_journey('layout-recovery-nested-rollback-after-plan', 'error')

    def test_plan_exit(self):
        self.rollback_journey('layout-recovery-nested-rollback-after-plan', 'exit')

    def test_progress_error(self):
        self.rollback_journey('layout-recovery-nested-rollback-after-progress', 'error')

    def receipt_journey(self, stage, mode, *, partial=False):
        helper = nested.NestedReconciliationContract()
        with World() as world:
            command, source, repo, bank, _, inspected, applied = recovery.small_pending(world)
            feature = self.feature(world, repo)
            operation = inspected['operation_id']
            directory = recovery.private_directory(world, bank, operation)
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            hold = (bank / '.agent-memory-transaction.md').read_bytes()
            modes = {path: (bank / path).stat().st_mode & 0o7777 for path in nested.canonical(bank)}
            proposal, transaction, target = recovery.admission_proposal(world, command, source, repo, bank, inspected,
                                                                        full=not partial)
            fault = world.run([command, 'admission', 'apply', '--proposal', proposal], check=False, timeout=30,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            audit = next(Path(world.env['AGENT_MEMORY_STATE_HOME']).rglob(
                transaction['operation_id'] + '.capture.started.*.json'))
            audit_bytes = audit.read_bytes()
            self.assertEqual(json.loads(audit_bytes)['source_classes'],
                             [transaction['admission_evidence']['capture']['request']['source_class']])
            committed = nested.canonical(bank)
            self.assertEqual(committed[target], transaction['files'][target].encode())
            if not stage.endswith('after-hold-restore'):
                helper.preview(world, command, repo, operation, transaction['operation_id'], 'rollback', expected=1)
                preview, _ = helper.preview(world, command, repo, operation, transaction['operation_id'], 'finish')
                recovery.call(world, command, 'recover-nested', '--repo', repo, '--proposal', preview)
            self.assertEqual(nested.canonical(bank), committed)
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
            if partial:
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), hold)
                self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
                self.assertFalse(status['semantic_recovery_complete'])
            else:
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertTrue(status['semantic_recovery_complete'])
            historical = recovery.public(world, command, 'admission', 'apply', '--proposal', proposal)
            self.assertTrue(historical['already_applied'])
            self.assertEqual(audit.read_bytes(), audit_bytes)
            self.assertEqual(nested.canonical(bank), committed)
            self.assertEqual({path: (bank / path).stat().st_mode & 0o7777 for path in modes}, modes)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assert_feature(world, repo, feature)

    def test_receipt_exit(self):
        self.receipt_journey('layout-recovery-nested-after-receipt', 'exit')

    def test_before_resolution_exit(self):
        self.receipt_journey('layout-recovery-admission-receipt-before-resolution', 'exit')

    def test_hold_restore_before_error(self):
        self.receipt_journey('layout-recovery-before-hold-restore', 'error', partial=True)

    def test_hold_restore_before_exit(self):
        self.receipt_journey('layout-recovery-before-hold-restore', 'exit', partial=True)

    def test_hold_restore_after_error(self):
        self.receipt_journey('layout-recovery-after-hold-restore', 'error', partial=True)

    def test_hold_restore_after_exit(self):
        self.receipt_journey('layout-recovery-after-hold-restore', 'exit', partial=True)

    def test_progress_exit(self):
        self.rollback_journey('layout-recovery-nested-rollback-after-progress', 'exit')

    def test_retirement_binding_error(self):
        self.receipt_journey('layout-recovery-retirement-after-binding', 'error')

    def test_retirement_binding_exit(self):
        self.receipt_journey('layout-recovery-retirement-after-binding', 'exit')

    def early_journey(self, stage, mode, *, changed_path='event'):
        helper = nested.NestedReconciliationContract()
        with World() as world:
            command, source, repo, bank, _, inspected, applied = recovery.small_pending(world)
            feature = self.feature(world, repo)
            operation = inspected['operation_id']
            directory = recovery.private_directory(world, bank, operation)
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            hold = (bank / '.agent-memory-transaction.md').read_bytes()
            before = nested.canonical(bank)
            modes = {path: (bank / path).stat().st_mode & 0o7777 for path in before}
            proposal, transaction, target = recovery.admission_proposal(world, command, source, repo, bank, inspected, full=False)
            if stage.endswith('after-rename'):
                paths = {'event': 'history/events/' + transaction['operation_id'] + '.md',
                         'target': target, 'router': 'MEMORY.md'}
                stage += ':' + paths[changed_path]
            fault = world.run([command, 'admission', 'apply', '--proposal', proposal], check=False, timeout=30,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            if stage.endswith('before-nested-marker'):
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), hold)
                self.assertEqual(nested.canonical(bank), before)
                recovery.public(world, command, 'admission', 'apply', '--proposal', proposal)
                status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            else:
                helper.preview(world, command, repo, operation, transaction['operation_id'], 'finish', expected=1)
                preview, _ = helper.preview(world, command, repo, operation, transaction['operation_id'], 'rollback')
                recovery.call(world, command, 'recover-nested', '--repo', repo, '--proposal', preview)
                self.assertEqual(nested.canonical(bank), before)
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), hold)
                self.assertEqual({path: (bank / path).stat().st_mode & 0o7777 for path in before}, modes)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assert_feature(world, repo, feature)

    def test_marker_before_error(self):
        self.early_journey('layout-recovery-before-nested-marker', 'error')

    def test_marker_before_exit(self):
        self.early_journey('layout-recovery-before-nested-marker', 'exit')

    def test_marker_after_error(self):
        self.early_journey('layout-recovery-after-nested-marker', 'error')

    def test_marker_after_exit(self):
        self.early_journey('layout-recovery-after-nested-marker', 'exit')

    def test_rename_error(self):
        self.early_journey('layout-recovery-nested-after-rename', 'error')

    def test_rename_exit(self):
        self.early_journey('layout-recovery-nested-after-rename', 'exit')

    def test_rename_target_error(self):
        self.early_journey('layout-recovery-nested-after-rename', 'error', changed_path='target')

    def test_rename_target_exit(self):
        self.early_journey('layout-recovery-nested-after-rename', 'exit', changed_path='target')

    def test_rename_router_error(self):
        self.early_journey('layout-recovery-nested-after-rename', 'error', changed_path='router')

    def test_rename_router_exit(self):
        self.early_journey('layout-recovery-nested-after-rename', 'exit', changed_path='router')

    def test_progress_target_error(self):
        self.rollback_journey('layout-recovery-nested-rollback-after-progress', 'error', changed_path='target')

    def test_progress_target_exit(self):
        self.rollback_journey('layout-recovery-nested-rollback-after-progress', 'exit', changed_path='target')

    def test_progress_router_error(self):
        self.rollback_journey('layout-recovery-nested-rollback-after-progress', 'error', changed_path='router')

    def test_progress_router_exit(self):
        self.rollback_journey('layout-recovery-nested-rollback-after-progress', 'exit', changed_path='router')
