"""Child restore faults retain source evidence and recover only known states."""
import stat
import unittest

from world import World
from contracts import test_layout_recovery as existing
import layout_recovery_fixtures as recovery


class LayoutRestoreFaults(unittest.TestCase):
    def journey(self, stage, mode):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            parent = recovery.private_directory(world, bank, safe['operation_id'])
            parent_receipt = (parent / 'receipt.json').read_bytes()
            child_proposal = world.inside('child.json')
            child = recovery.call(world, command, 'rollback', '--repo', repo, '--operation', safe['operation_id'],
                                  '--output', child_proposal)
            directory = recovery.private_directory(world, bank, child['operation_id'])
            fault = world.run([command, 'bank', 'recover-layout', 'rollback', '--repo', repo, '--proposal', child_proposal],
                              check=False, timeout=60, env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            journal = (directory / 'journal.json').read_bytes()
            preview = world.inside('child-recover.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', child['operation_id'],
                          '--mode', 'finish', '--output', preview)
            result = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview)
            self.assertTrue(result['verified'])
            self.assertEqual((directory / 'journal.json').read_bytes(), journal)
            self.assertEqual((parent / 'receipt.json').read_bytes(), parent_receipt)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*')
                              if p.is_file() and p.name not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')}, originals)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in originals}, modes)
            self.assertEqual({row['path']: (parent / row['blob']).read_bytes() for row in inspected['manifest']['sources']}, originals)
            self.assertTrue(recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview)['receipt_is_historical'])
            helper.assert_fault_work_unchanged(world, repo, git)

    def test_after_receipt_error(self):
        self.journey('layout-recovery-after-receipt', 'error')

    def test_probe_journal_error(self):
        self.journey('layout-recovery-probe-journal', 'error')

    def test_probe_journal_exit(self):
        self.journey('layout-recovery-probe-journal', 'exit')

    def test_partial_state_and_validate_refuse_without_adoption(self):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            fault = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                              check=False, timeout=60, env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-after-marker',
                                                            'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(fault.returncode, 86, fault.stdout + fault.stderr)
            recovery.unlock(world, command, repo)
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            marker = (bank / '.agent-memory-transaction.md').read_bytes()
            journal = (directory / 'journal.json').read_bytes()
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            validation = world.run([command, 'bank', 'validate', '--repo', repo], check=False, timeout=60)
            self.assertNotEqual(validation.returncode, 0, validation.stdout + validation.stderr)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, before)
            occupant = bank / 'foreign-occupant.md'
            occupant.write_text('Unrelated external edit\n')
            changed = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            output = world.inside('impossible.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                          '--mode', 'finish', '--output', output, expected=1)
            self.assertFalse(output.exists())
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, changed)
            self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), marker)
            self.assertEqual((directory / 'journal.json').read_bytes(), journal)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}, originals)
            helper.assert_fault_work_unchanged(world, repo, git)
