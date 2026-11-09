"""Interrupted public layout apply must finish from its exact reviewed state."""
import stat
import json
from pathlib import Path
import unittest

from world import World
from contracts import test_layout_recovery as existing
import layout_recovery_fixtures as recovery


class LayoutApplyFaults(unittest.TestCase):
    def journey(self, stage, mode):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            interrupted = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                                    check=False, timeout=60,
                                    env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(interrupted.returncode, 86 if mode == 'exit' else 1,
                             interrupted.stdout + interrupted.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            helper.assert_fault_work_unchanged(world, repo, git)
            preview = world.inside('known-finish.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                          '--mode', 'finish', '--output', preview)
            outcome = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview)
            self.assertTrue(outcome['verified'])
            self.assertFalse(outcome['semantic_recovery_complete'])
            self.assertGreater(outcome['pending_units'], 0)
            wanted = {path: body.encode() for path, body in native.items()}
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*')
                              if p.is_file() and p.name not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')}, wanted)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in wanted},
                             {path: modes[path] for path in wanted})
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertTrue((bank / '.agent-memory-transaction.md').exists())
            self.assertTrue((bank / '.agent-memory-recovery.md').exists())
            historical = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview)
            self.assertTrue(historical['receipt_is_historical'])
            helper.assert_fault_work_unchanged(world, repo, git)

    def test_readback_error(self):
        self.journey('layout-recovery-before-final-readback', 'error')

    def test_readback_exit(self):
        self.journey('layout-recovery-before-final-readback', 'exit')

    def test_index_error(self):
        self.journey('layout-recovery-final-index', 'error')

    def test_index_exit(self):
        self.journey('layout-recovery-final-index', 'exit')

    def test_before_sentinel_error(self):
        self.journey('layout-recovery-before-sentinel', 'error')

    def test_before_sentinel_exit(self):
        self.journey('layout-recovery-before-sentinel', 'exit')

    def test_after_sentinel_error(self):
        self.journey('layout-recovery-after-sentinel', 'error')

    def test_after_sentinel_exit(self):
        self.journey('layout-recovery-after-sentinel', 'exit')

    def test_before_marker_error(self):
        self.journey('layout-recovery-before-marker', 'error')

    def test_before_marker_exit(self):
        self.journey('layout-recovery-before-marker', 'exit')

    def test_after_hold_error(self):
        self.journey('layout-recovery-after-hold', 'error')

    def test_after_hold_exit(self):
        self.journey('layout-recovery-after-hold', 'exit')

    def test_before_swap_error(self):
        self.journey('layout-recovery-before-marker-swap', 'error')

    def test_before_swap_exit(self):
        self.journey('layout-recovery-before-marker-swap', 'exit')

    def test_after_swap_error(self):
        self.journey('layout-recovery-after-marker-swap', 'error')

    def test_after_swap_exit(self):
        self.journey('layout-recovery-after-marker-swap', 'exit')

    def test_journal_exit(self):
        self.journey('layout-recovery-after-journal', 'exit')

    def test_marker_exit(self):
        self.journey('layout-recovery-after-marker', 'exit')

    def test_router_exit(self):
        self.journey('layout-recovery-after-router', 'exit')

    def test_before_receipt_exit(self):
        self.journey('layout-recovery-before-receipt', 'exit')

    def test_receipt_exit(self):
        self.journey('layout-recovery-after-receipt', 'exit')

    def test_before_router_error(self):
        self.journey('layout-recovery-before-router', 'error')

    def test_before_router_exit(self):
        self.journey('layout-recovery-before-router', 'exit')

    def test_index_intent_error(self):
        self.journey('layout-recovery-index-intent', 'error')

    def test_index_intent_exit(self):
        self.journey('layout-recovery-index-intent', 'exit')

    def before_journal(self, mode):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            interrupted = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                                    check=False, timeout=60,
                                    env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-journal',
                                         'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(interrupted.returncode, 86 if mode == 'exit' else 1,
                             interrupted.stdout + interrupted.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            self.assertFalse((directory / 'journal.json').exists())
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()},
                             originals)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in originals}, modes)
            helper.assert_fault_work_unchanged(world, repo, git)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertTrue(applied['verified'])
            self.assertFalse(applied['semantic_recovery_complete'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertTrue(recovery.call(world, command, 'apply', '--repo', repo,
                                          '--proposal', proposal)['receipt_is_historical'])
            helper.assert_fault_work_unchanged(world, repo, git)

    def test_before_journal_error(self):
        self.before_journal('error')

    def test_before_journal_exit(self):
        self.before_journal('exit')

    def originals_completed(self, mode):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, _, repo, bank, _, originals, modes, git = helper.setup_fault_repair(world)
            interrupted = world.run([command, 'bank', 'recover-layout', 'inspect', '--repo', repo,
                                     '--output', world.inside('interrupted-inspect.json')],
                                    check=False, timeout=60,
                                    env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-originals-completed',
                                         'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(interrupted.returncode, 86 if mode == 'exit' else 1,
                             interrupted.stdout + interrupted.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            root = (Path(world.env['AGENT_MEMORY_STATE_HOME']) / 'writes'
                    / recovery.sha(str(bank).encode()) / 'layout-recovery')
            manifests = list(root.glob('*/manifest.json'))
            self.assertEqual(len(manifests), 1)
            manifest = json.loads(manifests[0].read_bytes())
            directory = manifests[0].parent
            saved = {row['path']: (directory / row['blob']).read_bytes() for row in manifest['sources']}
            self.assertEqual(saved, originals)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes()
                              for p in bank.rglob('*') if p.is_file()}, originals)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in originals}, modes)
            safe, inspected = recovery.inspect(world, command, repo)
            self.assertNotEqual(safe['operation_id'], manifest['operation_id'])
            fresh = recovery.private_directory(world, bank, safe['operation_id'])
            self.assertEqual({row['path']: (fresh / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in manifest['sources']}, saved)
            helper.assert_fault_work_unchanged(world, repo, git)

    def test_originals_completed_error(self):
        self.originals_completed('error')

    def test_originals_completed_exit(self):
        self.originals_completed('exit')

    def test_partial_staging_exit(self):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, _, repo, bank, _, originals, modes, git = helper.setup_fault_repair(world)
            failed = world.run([command, 'bank', 'recover-layout', 'inspect', '--repo', repo,
                                '--output', world.inside('partial-exit.json')], check=False, timeout=60,
                               env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-partial-original-staging',
                                    'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(failed.returncode, 86, failed.stdout + failed.stderr)
            recovery.unlock(world, command, repo)
            root = (Path(world.env['AGENT_MEMORY_STATE_HOME']) / 'writes'
                    / recovery.sha(str(bank).encode()) / 'layout-recovery')
            staging = list(root.glob('*/staging.json'))
            self.assertEqual(len(staging), 1)
            directory = staging[0].parent
            self.assertFalse(json.loads(staging[0].read_bytes())['observation_complete'])
            blobs = {p.name: p.read_bytes() for p in (directory / 'originals').glob('*.bin')}
            self.assertEqual(len(blobs), 1)
            self.assertIn(next(iter(blobs.values())), originals.values())
            self.assertFalse((directory / 'manifest.json').exists())
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', directory.name)
            self.assertEqual(status['state'], 'observation-incomplete')
            safe, inspected = recovery.inspect(world, command, repo)
            self.assertNotEqual(safe['operation_id'], directory.name)
            fresh = recovery.private_directory(world, bank, safe['operation_id'])
            self.assertEqual({row['path']: (fresh / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual({p.name: p.read_bytes() for p in (directory / 'originals').glob('*.bin')}, blobs)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes()
                              for p in bank.rglob('*') if p.is_file()}, originals)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in originals}, modes)
            helper.assert_fault_work_unchanged(world, repo, git)

    def terminal_journey(self, stage, mode):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, source, repo, bank, native, originals = helper.setup_repair(world)
            modes = {path: stat.S_IMODE((bank / path).stat().st_mode) for path in originals}
            feature = repo / 'feature.txt'
            feature.write_text('Committed feature\\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
            feature.write_text('Staged feature\\n')
            world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged feature\\n')
            index = (repo / '.git/index').read_bytes()
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            interrupted = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                                    check=False, timeout=60,
                                    env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(interrupted.returncode, 86 if mode == 'exit' else 1,
                             interrupted.stdout + interrupted.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            preview = world.inside('terminal-finish.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                          '--mode', 'finish', '--output', preview)
            completed = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview)
            self.assertTrue(completed['verified'])
            self.assertTrue(completed['semantic_recovery_complete'])
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()},
                             {path: body.encode() for path, body in native.items()})
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in native},
                             {path: modes[path] for path in native})
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertTrue(recovery.call(world, command, 'recover', '--repo', repo,
                                          '--proposal', preview)['receipt_is_historical'])
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(feature.read_text(), 'Unstaged feature\\n')

    def test_marker_removal_error(self):
        self.terminal_journey('layout-recovery-marker-removal', 'error')

    def test_marker_removal_exit(self):
        self.terminal_journey('layout-recovery-marker-removal', 'exit')

    def test_sentinel_retirement_before_error(self):
        self.terminal_journey('layout-recovery-before-sentinel-retirement', 'error')

    def test_sentinel_retirement_before_exit(self):
        self.terminal_journey('layout-recovery-before-sentinel-retirement', 'exit')

    def test_sentinel_retirement_after_error(self):
        self.terminal_journey('layout-recovery-sentinel-retirement', 'error')

    def test_sentinel_retirement_after_exit(self):
        self.terminal_journey('layout-recovery-sentinel-retirement', 'exit')

    def hold_swap_journey(self, boundary, mode):
        helper = existing.LayoutRecoveryContract()
        with World() as world:
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                          env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-after-receipt'})
            preview = world.inside('hold-swap-finish.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                          '--mode', 'finish', '--output', preview)
            fault = world.run([command, 'bank', 'recover-layout', 'recover', '--repo', repo, '--proposal', preview],
                              check=False, timeout=60,
                              env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-' + boundary + '-marker-swap',
                                   'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            marker = bank / '.agent-memory-transaction.md'
            self.assertTrue(marker.is_file())
            next_preview = world.inside('hold-swap-resume.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                          '--mode', 'finish', '--output', next_preview)
            outcome = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', next_preview)
            self.assertTrue(outcome['verified'])
            self.assertFalse(outcome['semantic_recovery_complete'])
            self.assertEqual(json.loads(marker.read_bytes())['schema'], 'agent-memory/layout-frontier-hold-v1')
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*')
                              if p.is_file() and p.name not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')},
                             {path: body.encode() for path, body in native.items()})
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in native},
                             {path: modes[path] for path in native})
            helper.assert_fault_work_unchanged(world, repo, git)

    def test_hold_swap_before_error(self):
        self.hold_swap_journey('before', 'error')

    def test_hold_swap_before_exit(self):
        self.hold_swap_journey('before', 'exit')

    def test_hold_swap_after_error(self):
        self.hold_swap_journey('after', 'error')

    def test_hold_swap_after_exit(self):
        self.hold_swap_journey('after', 'exit')

# Each path in the controlled proposal has its own worker case: temporary
# writes exist for the two replacements, while the handoff is a deletion.
def _path_case(stage, path, mode):
    def run(self):
        self.journey('layout-recovery-' + stage + ':' + path, mode)
    return run


for _stage, _label, _paths in (
        ('temporary-write', 'temp', ('activeContext.md', 'MEMORY.md')),
        ('before-replace', 'replace', ('activeContext.md', 'handoff-prompt.md', 'MEMORY.md')),
        ('after-rename', 'rename', ('activeContext.md', 'handoff-prompt.md', 'MEMORY.md')),
        ('after-progress', 'progress', ('activeContext.md', 'handoff-prompt.md', 'MEMORY.md'))):
    for _path in _paths:
        _path_label = {'activeContext.md': 'active', 'handoff-prompt.md': 'handoff',
                       'MEMORY.md': 'router'}[_path]
        for _mode in ('error', 'exit'):
            if _stage == 'after-rename' and _path == 'activeContext.md' and _mode == 'error':
                continue  # The original first-replacement contract already exercises this exact cell.
            setattr(LayoutApplyFaults, 'test_' + _label + '_' + _path_label + '_' + _mode,
                    _path_case(_stage, _path, _mode))
