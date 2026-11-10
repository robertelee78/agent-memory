"""Reviewed unit revisions survive interrupted ledger publication."""
import json
import stat
import unittest

from world import World
from contracts import test_layout_recovery_reconciliation as reconciliation
import layout_recovery_fixtures as recovery


class LayoutRevisionFaults(unittest.TestCase):
    def journey(self, stage, mode):
        with World() as world:
            command, _, repo, bank, _, inspected, applied = recovery.small_pending(world, reconciliation.SCRATCH)
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
            canonical = reconciliation.bank_bytes(bank)
            modes = {path: stat.S_IMODE((bank / path).stat().st_mode) for path in canonical}
            markers = {name: (bank / name).read_bytes() for name in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')}
            parent = applied['pending_unit_ids'][0]
            request = reconciliation.revision_request(world, bank, inspected, parent)
            children = {row['unit_id'] for row in request['replacements'][0]['new_units']}
            proposal = world.inside('revision.json')
            recovery.call(world, command, 'revise-units', '--repo', repo, '--operation', operation,
                          '--input', recovery.save(world, request), '--output', proposal)
            head_before = (directory / 'ledger-head.json').read_bytes()
            generation = json.loads(head_before)['generation'] + 1
            next_name = str(generation).zfill(8) + '.json'
            index_path = directory.parent / 'index.json'
            index_before = index_path.read_bytes()
            fault = world.run([command, 'bank', 'recover-layout', 'revise-units', '--repo', repo, '--proposal', proposal],
                              check=False, timeout=60, env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            if stage == 'layout-recovery-ledger-generation-before-head':
                # The outer revision hook fires before advance_ledger writes.
                self.assertEqual((directory / 'ledger-head.json').read_bytes(), head_before)
                self.assertFalse((directory / 'ledger' / next_name).exists())
                self.assertFalse((directory / 'ledger-transitions' / next_name).exists())
            else:
                # The inner after-head hook precedes the index update.
                head_after = json.loads((directory / 'ledger-head.json').read_bytes())
                self.assertEqual(head_after['generation'], generation)
                self.assertNotEqual((directory / 'ledger-head.json').read_bytes(), head_before)
                self.assertEqual(index_path.read_bytes(), index_before)
            result = recovery.call(world, command, 'revise-units', '--repo', repo, '--proposal', proposal)
            self.assertTrue(result['verified'])
            retry = recovery.call(world, command, 'revise-units', '--repo', repo, '--proposal', proposal)
            self.assertTrue(retry['already_applied'])
            self.assertTrue(retry['receipt_is_historical'])
            ledger_head = json.loads((directory / 'ledger-head.json').read_bytes())
            ledger = json.loads((directory / 'ledger' / (str(ledger_head['generation']).zfill(8) + '.json')).read_bytes())
            old = next(unit for unit in ledger['units'] if unit['unit_id'] == parent)
            self.assertEqual(old['state'], 'superseded-by-revision')
            self.assertEqual(set(old['child_unit_ids']), children)
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
            self.assertEqual(set(status['pending_unit_ids']), (set(applied['pending_unit_ids']) - {parent}) | children)
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertEqual(reconciliation.bank_bytes(bank), canonical)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in canonical}, modes)
            self.assertEqual({name: (bank / name).read_bytes() for name in markers}, markers)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}, originals)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(feature.read_text(), 'Unstaged feature\n')

    def test_before_head_error(self):
        self.journey('layout-recovery-ledger-generation-before-head', 'error')

    def test_before_head_exit(self):
        self.journey('layout-recovery-ledger-generation-before-head', 'exit')

    def test_after_head_error(self):
        self.journey('layout-recovery-ledger-generation-after-head', 'error')

    def test_after_head_exit(self):
        self.journey('layout-recovery-ledger-generation-after-head', 'exit')
