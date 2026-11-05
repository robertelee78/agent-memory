"""Installed public admission -> nested recovery, with real receipts and hard exits."""
import json
from pathlib import Path
import time
import unittest
import uuid

from world import World
import layout_recovery_fixtures as f


def canonical(bank):
    return {path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*')
            if path.is_file() and not path.name.startswith('.agent-memory')}


def retained(directory):
    return {path.relative_to(directory).as_posix(): path.read_bytes() for path in directory.rglob('*') if path.is_file()}


class NestedReconciliationContract(unittest.TestCase):
    def preview(self, world, command, repo, operation, nested, mode, expected=0):
        output = world.inside('nested-' + str(uuid.uuid4()) + '.json')
        value = f.call(world, command, 'recover-nested', '--repo', repo, '--operation', operation,
            '--nested', nested, '--mode', mode, '--output', output, expected=expected)
        return output, value

    def test_durable_canonical_receipt_reconciles_only_its_exact_journaled_head_gap(self):
        for fault_mode in ('error', 'exit'):
            with self.subTest(mode=fault_mode), World() as world:
                command, source, repo, bank, _, inspected, _ = f.small_pending(world)
                operation = inspected['operation_id']
                directory = f.private_directory(world, bank, operation)
                original_blobs = {row['source_id']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
                proposal, transaction, target = f.admission_proposal(world, command, source, repo, bank, inspected, full=True)
                fault = world.run([command, 'admission', 'apply', '--proposal', proposal], check=False, timeout=30,
                    env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-ledger-generation-after-head', 'AGENT_MEMORY_FAULT_MODE': fault_mode})
                self.assertEqual(fault.returncode, 86 if fault_mode == 'exit' else 1, fault.stdout + fault.stderr)
                if fault_mode == 'exit':
                    f.unlock(world, command, repo)
                head = json.loads((directory / 'ledger-head.json').read_text())
                index_bytes = (directory.parent / 'index.json').read_bytes()
                indexed = json.loads(index_bytes)['operations'][operation]
                self.assertEqual(head['generation'], 1)
                self.assertNotEqual(indexed['ledger_head_sha256'], f.sha(f.encoded(head)))
                marker = (bank / '.agent-memory-transaction.md').read_bytes()
                self.assertEqual(json.loads(marker)['nested_id'], transaction['operation_id'])
                bank_before = canonical(bank)
                self.assertEqual(bank_before[target], transaction['files'][target].encode())
                f.call(world, command, 'status', '--repo', repo, '--operation', operation, expected=1)
                transition_path = directory / 'ledger-transitions/00000001.json'
                transition_bytes = transition_path.read_bytes()
                corrupt = json.loads(transition_bytes)
                corrupt['units_sha256'] = '0' * 64
                transition_path.write_bytes(f.encoded(corrupt))
                self.preview(world, command, repo, operation, transaction['operation_id'], 'finish', expected=1)
                self.assertEqual(canonical(bank), bank_before)
                self.assertEqual((directory.parent / 'index.json').read_bytes(), index_bytes)
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), marker)
                transition_path.write_bytes(transition_bytes)
                private_before = retained(directory)
                recovery, preview = self.preview(world, command, repo, operation, transaction['operation_id'], 'finish')
                self.assertTrue(preview['temporary_lock_writes_performed'])
                self.assertFalse(preview['canonical_writes_performed'])
                self.assertEqual(canonical(bank), bank_before)
                self.assertEqual(retained(directory), private_before)
                self.assertEqual((directory.parent / 'index.json').read_bytes(), index_bytes)
                self.preview(world, command, repo, operation, transaction['operation_id'], 'rollback', expected=1)
                if fault_mode == 'error':
                    rendezvous = world.inside('recovery-contention')
                    world.env.update(AGENT_MEMORY_FAULT_STAGE='layout-recovery-admission-receipt-before-resolution',
                        AGENT_MEMORY_FAULT_MODE='pause', AGENT_MEMORY_FAULT_DIR=str(rendezvous))
                    child = world.spawn([command, 'bank', 'recover-layout', 'recover-nested', '--repo', repo, '--proposal', recovery])
                    for key in ('AGENT_MEMORY_FAULT_STAGE', 'AGENT_MEMORY_FAULT_MODE', 'AGENT_MEMORY_FAULT_DIR'):
                        world.env.pop(key)
                    ready = rendezvous / 'layout-recovery-admission-receipt-before-resolution.ready'
                    deadline = time.monotonic() + 10
                    while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                        time.sleep(.01)
                    self.assertTrue(ready.exists())
                    self.preview(world, command, repo, operation, transaction['operation_id'], 'finish', expected=1)
                    self.assertEqual(canonical(bank), bank_before)
                    (rendezvous / 'layout-recovery-admission-receipt-before-resolution.release').touch()
                    stdout, stderr = child.communicate(timeout=30)
                    self.assertEqual(child.returncode, 0, stdout + stderr)
                else:
                    f.call(world, command, 'recover-nested', '--repo', repo, '--proposal', recovery)
                self.assertEqual(canonical(bank), bank_before)
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertFalse((bank / '.agent-memory-recovery.md').exists())
                after = f.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertTrue(after['semantic_recovery_complete'])
                ledger = json.loads((directory / 'ledger/00000001.json').read_text())
                self.assertEqual(ledger['units'][0]['state'], 'admitted')
                link = ledger['units'][0]['terminal_receipts'][0]
                terminal = json.loads((directory / 'resolutions' / (link['resolution_id'] + '.json')).read_text())
                self.assertEqual(terminal['canonical_operation_id'], transaction['operation_id'])
                self.assertEqual(link['sha256'], f.sha(f.encoded(terminal)))
                self.assertEqual(len(ledger['units'][0]['terminal_receipts']), 1)
                for row in inspected['manifest']['sources']:
                    self.assertEqual((directory / row['blob']).read_bytes(), original_blobs[row['source_id']])

    def test_interrupted_pre_receipt_rollback_resumes_ordered_plan_not_forward_prefix(self):
        for fault_mode in ('error', 'exit'):
            with self.subTest(mode=fault_mode), World() as world:
                command, source, repo, bank, native, inspected, applied = f.small_pending(world)
                operation = inspected['operation_id']
                directory = f.private_directory(world, bank, operation)
                hold = (bank / '.agent-memory-transaction.md').read_bytes()
                sentinel = (bank / '.agent-memory-recovery.md').read_bytes()
                base = canonical(bank)
                proposal, transaction, target = f.admission_proposal(world, command, source, repo, bank, inspected, full=False)
                f.public(world, command, 'admission', 'apply', '--proposal', proposal, expected=1,
                    env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-nested-before-receipt'})
                self.preview(world, command, repo, operation, transaction['operation_id'], 'finish', expected=1)
                recovery, _ = self.preview(world, command, repo, operation, transaction['operation_id'], 'rollback')
                event = 'history/events/' + transaction['operation_id'] + '.md'
                stage = ('transaction-before-recheck:' + target if fault_mode == 'error' else
                         'layout-recovery-nested-rollback-after-rename:' + event)
                fault = world.run([command, 'bank', 'recover-layout', 'recover-nested', '--repo', repo, '--proposal', recovery],
                    check=False, timeout=30, env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': fault_mode})
                self.assertEqual(fault.returncode, 86 if fault_mode == 'exit' else 1, fault.stdout + fault.stderr)
                if fault_mode == 'exit':
                    f.unlock(world, command, repo)
                nested_directory = directory / 'nested' / transaction['operation_id']
                plan = (nested_directory / 'rollback-plan.json').read_bytes()
                self.assertFalse((bank / event).exists())
                self.assertTrue((bank / target).exists())
                interrupted = canonical(bank)
                f.call(world, command, 'recover-nested', '--repo', repo, '--proposal', recovery, expected=1)
                self.assertEqual(canonical(bank), interrupted)
                resume, _ = self.preview(world, command, repo, operation, transaction['operation_id'], 'rollback')
                context = bank / 'projectbrief.md'
                before = context.read_bytes()
                context.write_bytes(before + b'\nExternal unrelated edit\n')
                f.call(world, command, 'recover-nested', '--repo', repo, '--proposal', resume, expected=1)
                self.assertEqual(context.read_bytes(), before + b'\nExternal unrelated edit\n')
                context.write_bytes(before)
                f.call(world, command, 'recover-nested', '--repo', repo, '--proposal', resume)
                self.assertEqual((nested_directory / 'rollback-plan.json').read_bytes(), plan)
                self.assertEqual(canonical(bank), base)
                self.assertEqual({path: (bank / path).read_text() for path in native}, native)
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), hold)
                self.assertEqual((bank / '.agent-memory-recovery.md').read_bytes(), sentinel)
                status = f.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
                self.assertFalse(status['semantic_recovery_complete'])
                progress = list((nested_directory / 'rollback-progress').glob('*.json'))
                self.assertEqual(len(progress), 3 if fault_mode == 'error' else 2)


if __name__ == '__main__':
    unittest.main()
