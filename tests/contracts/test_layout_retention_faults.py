"""Public retention interruption and exact reviewed resume on disposable banks."""
import unittest

from world import World
from contracts import test_layout_recovery_retention as retention
import layout_recovery_fixtures as recovery


class LayoutRetentionFaults(unittest.TestCase):
    def journey(self, stage, mode):
        helper = retention.LayoutRetentionContract()
        with World() as world:
            command, _, repo, bank, observed, layout_path, layout_proposal, applied = helper.public_repair(world)
            self.assertTrue(applied['semantic_recovery_complete'])
            feature = repo / 'feature.txt'
            feature.write_text('Committed feature\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Retain feature baseline', cwd=repo)
            feature.write_text('Staged feature\n')
            world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged feature\n')
            index = (repo / '.git/index').read_bytes()
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            canonical = {p.relative_to(bank).as_posix(): (p.read_bytes(), p.stat().st_mode & 0o7777)
                         for p in bank.rglob('*') if p.is_file()}
            reviewed, proposal = helper.reviewed_cleanup(world, command, repo, observed, layout_proposal)
            directory = recovery.private_directory(world, bank, observed['operation_id'])
            receipt = (directory / 'receipt.json').read_bytes()
            ordered = sorted(proposal['delete'])
            self.assertTrue(ordered)
            before = {name: (directory / name).read_bytes() for name in ordered}
            interrupted = world.run([command, 'bank', 'recover-layout', 'retention', 'apply', '--repo', repo,
                                     '--operation', observed['operation_id'], '--proposal', reviewed],
                                    check=False, timeout=60,
                                    env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(interrupted.returncode, 86 if mode == 'exit' else 1,
                             interrupted.stdout + interrupted.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            self.assertEqual((directory / 'receipt.json').read_bytes(), receipt)
            if stage.endswith('retention-intent'):
                self.assertEqual({name: (directory / name).read_bytes() for name in ordered}, before)
            elif stage.endswith('retention-after-progress'):
                self.assertFalse((directory / ordered[0]).exists())
                self.assertEqual({name: (directory / name).read_bytes() for name in ordered[1:]},
                                 {name: before[name] for name in ordered[1:]})
            else:
                self.assertTrue(all(not (directory / name).exists() for name in ordered))
            recovery.call(world, command, 'retention', 'apply', '--repo', repo,
                          '--operation', observed['operation_id'], '--proposal', reviewed)
            again = recovery.call(world, command, 'retention', 'apply', '--repo', repo,
                                  '--operation', observed['operation_id'], '--proposal', reviewed)
            self.assertTrue(again['receipt_is_historical'])
            history = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', layout_path)
            self.assertTrue(history['receipt_is_historical'])
            self.assertEqual((directory / 'receipt.json').read_bytes(), receipt)
            self.assertTrue(all(not (directory / name).exists() for name in ordered))
            self.assertEqual(canonical, {p.relative_to(bank).as_posix(): (p.read_bytes(), p.stat().st_mode & 0o7777)
                                         for p in bank.rglob('*') if p.is_file()})
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(feature.read_text(), 'Unstaged feature\n')

    def test_intent_error(self):
        self.journey('layout-recovery-retention-intent', 'error')

    def test_intent_exit(self):
        self.journey('layout-recovery-retention-intent', 'exit')

    def test_progress_error(self):
        self.journey('layout-recovery-retention-after-progress', 'error')

    def test_progress_exit(self):
        self.journey('layout-recovery-retention-after-progress', 'exit')

    def test_tombstone_error(self):
        self.journey('layout-recovery-retention-final-tombstone', 'error')

    def test_tombstone_exit(self):
        self.journey('layout-recovery-retention-final-tombstone', 'exit')
