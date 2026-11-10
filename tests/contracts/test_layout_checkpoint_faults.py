"""Held public checkpoints recover at their actual write-receipt boundaries."""
import stat
import unittest

from world import World
from contracts import test_checkpoint
from contracts import test_layout_nested_reconciliation as nested
import layout_recovery_fixtures as recovery


class LayoutCheckpointFaults(unittest.TestCase):
    def journey(self, after_receipt, mode):
        with World() as world:
            command, source = recovery.installed(world)
            repo, bank, native = recovery.memory(world)
            active = test_checkpoint.BASE
            native['activeContext.md'] = active
            prefix, tail = native['MEMORY.md'].split('\n## Omitted contexts\n', 1)
            route = '| [activeContext.md](<activeContext.md>) | Active context | repository | active | When resuming |'
            native['MEMORY.md'] = prefix.rstrip() + '\n' + route + '\n\n## Omitted contexts\n' + tail.replace(
                '- `activeContext.md`: No applicable evidence yet\n', '')
            (bank / 'MEMORY.md').write_text(native['MEMORY.md'])
            (bank / 'activeContext.md').write_text(active)
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0))
            _, inspected = recovery.inspect(world, command, repo)
            layout, _ = recovery.prepare(world, command, source, repo, inspected, native)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', layout)
            feature = repo / 'feature.txt'
            feature.write_text('Feature baseline\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Feature baseline', cwd=repo)
            feature.write_text('Staged feature\n')
            world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged feature\n')
            index = (repo / '.git/index').read_bytes()
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            operation = inspected['operation_id']
            directory = recovery.private_directory(world, bank, operation)
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            hold = (bank / '.agent-memory-transaction.md').read_bytes()
            sentinel = (bank / '.agent-memory-recovery.md').read_bytes()
            before = nested.canonical(bank)
            modes = {path: stat.S_IMODE((bank / path).stat().st_mode) for path in before}
            current = recovery.public(world, command, 'checkpoint', 'inspect', '--repo', repo)
            payload = test_checkpoint.CheckpointContract().payload(current)
            proposal = recovery.public(world, command, 'checkpoint', 'preview', '--repo', repo,
                                       '--input', recovery.save(world, payload))
            proposal_file = recovery.save(world, proposal)
            nested_id = proposal['proposal']['write']['operation_id']
            stage = 'layout-recovery-nested-' + ('after' if after_receipt else 'before') + '-receipt'
            fault = world.run([command, 'checkpoint', 'apply', '--proposal', proposal_file], check=False, timeout=60,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            journal = directory / 'nested' / nested_id / 'journal.json'
            journal_bytes = journal.read_bytes()
            changed = nested.canonical(bank)
            self.assertNotEqual(changed['activeContext.md'], before['activeContext.md'])
            helper = nested.NestedReconciliationContract()
            wanted_mode = 'finish' if after_receipt else 'rollback'
            helper.preview(world, command, repo, operation, nested_id, 'rollback' if after_receipt else 'finish', expected=1)
            preview, _ = helper.preview(world, command, repo, operation, nested_id, wanted_mode)
            result = recovery.call(world, command, 'recover-nested', '--repo', repo, '--proposal', preview)
            self.assertTrue(result['verified'])
            self.assertEqual(nested.canonical(bank), changed if after_receipt else before)
            if after_receipt:
                historical = recovery.public(world, command, 'checkpoint', 'apply', '--proposal', proposal_file)
                self.assertTrue(historical['write']['already_applied'])
                self.assertFalse(historical['writes_performed'])
            self.assertEqual(journal.read_bytes(), journal_bytes)
            self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), hold)
            self.assertEqual((bank / '.agent-memory-recovery.md').read_bytes(), sentinel)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in before}, modes)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}, originals)
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
            self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(feature.read_text(), 'Unstaged feature\n')

    def test_before_receipt_error(self):
        self.journey(False, 'error')

    def test_before_receipt_exit(self):
        self.journey(False, 'exit')

    def test_after_receipt_error(self):
        self.journey(True, 'error')

    def test_after_receipt_exit(self):
        self.journey(True, 'exit')
