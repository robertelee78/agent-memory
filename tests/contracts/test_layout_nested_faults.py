"""Real nested admission rollback interruption preserves its hold and sources."""
import unittest

from world import World
from contracts import test_layout_nested_reconciliation as nested
import layout_recovery_fixtures as recovery


class LayoutNestedFaults(unittest.TestCase):
    def rollback_journey(self, stage, mode):
        helper = nested.NestedReconciliationContract()
        with World() as world:
            command, source, repo, bank, native, inspected, applied = recovery.small_pending(world)
            operation = inspected['operation_id']
            directory = recovery.private_directory(world, bank, operation)
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            hold = (bank / '.agent-memory-transaction.md').read_bytes()
            sentinel = (bank / '.agent-memory-recovery.md').read_bytes()
            before = nested.canonical(bank)
            proposal, transaction, target = recovery.admission_proposal(world, command, source, repo, bank, inspected, full=False)
            recovery.public(world, command, 'admission', 'apply', '--proposal', proposal, expected=1,
                            env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-nested-before-receipt'})
            helper.preview(world, command, repo, operation, transaction['operation_id'], 'finish', expected=1)
            preview, _ = helper.preview(world, command, repo, operation, transaction['operation_id'], 'rollback')
            if stage.endswith('after-progress'):
                event = 'history/events/' + transaction['operation_id'] + '.md'
                stage += ':' + event
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
            self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), hold)
            self.assertEqual((bank / '.agent-memory-recovery.md').read_bytes(), sentinel)
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
            self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)

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
            operation = inspected['operation_id']
            directory = recovery.private_directory(world, bank, operation)
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            hold = (bank / '.agent-memory-transaction.md').read_bytes()
            proposal, transaction, target = recovery.admission_proposal(world, command, source, repo, bank, inspected,
                                                                        full=not partial)
            fault = world.run([command, 'admission', 'apply', '--proposal', proposal], check=False, timeout=30,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
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
            self.assertEqual(nested.canonical(bank), committed)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)

    def test_receipt_error(self):
        self.receipt_journey('layout-recovery-nested-after-receipt', 'error')

    def test_receipt_exit(self):
        self.receipt_journey('layout-recovery-nested-after-receipt', 'exit')

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
