"""Old recovery tools cannot reinterpret a real interrupted nested admission."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from contracts import test_layout_nested_reconciliation as nested
import layout_recovery_fixtures as recovery
import legacy_recovery_fixtures as legacy_guard
import v014_runtime_fixture


class LayoutLegacyNested(unittest.TestCase):
    def journey(self, receipted, decoy=False, *, boundary=None, partial_receipt=False):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            command, source, repo, bank, native, inspected, applied = recovery.small_pending(world)
            feature = repo / 'feature.txt'
            feature.write_text('Feature baseline\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
            feature.write_text('Staged feature\n')
            world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged feature\n')
            operation = inspected['operation_id']
            directory = recovery.private_directory(world, bank, operation)
            originals = {row['path']: (directory / row['blob']).read_bytes() for row in inspected['manifest']['sources']}
            hold = (bank / '.agent-memory-transaction.md').read_bytes()
            original_bank = nested.canonical(bank)
            proposal, transaction, target = recovery.admission_proposal(world, command, source, repo, bank, inspected,
                                                                   full=receipted and not partial_receipt)
            stage = 'layout-recovery-nested-after-receipt' if receipted else 'layout-recovery-after-nested-marker'
            if boundary and boundary.startswith('rename'):
                final = {path: recovery.sha(body.encode()) for path, body in transaction['files'].items()}
                paths = sorted((path for path in set(final) | set(transaction['observed_hashes'])
                                if final.get(path) != transaction['observed_hashes'].get(path)),
                               key=lambda path: (path == 'MEMORY.md', path))
                self.assertEqual(len(paths), 3)
                number = int(boundary.removeprefix('rename'))
                self.assertLess(number, len(paths))
                stage = 'layout-recovery-nested-after-rename:' + paths[number]
            elif boundary == 'unlinked':
                stage = 'layout-recovery-admission-receipt-before-resolution'
            elif boundary == 'hold-restore':
                stage = 'layout-recovery-before-hold-restore'
            fault = world.run([command, 'admission', 'apply', '--proposal', proposal], check=False, timeout=30,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(fault.returncode, 86, fault.stdout + fault.stderr)
            marker = bank / '.agent-memory-transaction.md'
            marker_bytes = marker.read_bytes()
            self.assertEqual(json.loads(marker_bytes)['schema'], 'agent-memory/layout-frontier-transaction-v1')
            legacy_guard.guard(self, world, old, pack, repo, bank, directory, inspected, native, decoy=decoy)
            index = (repo / '.git/index').read_bytes()
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            if receipted and not partial_receipt:
                request = recovery.save(world, transaction['admission_evidence']['comparison_request'])
                output = world.inside('blocked-comparison.json')
                before = nested.canonical(bank)
                recovery.public(world, command, 'compare', 'inspect', '--repo', repo, '--request', request,
                                '--private-output', output, expected=1)
                self.assertEqual(nested.canonical(bank), before)
            helper = nested.NestedReconciliationContract()
            mode = 'finish' if receipted else 'rollback'
            helper.preview(world, command, repo, operation, transaction['operation_id'],
                           'rollback' if receipted else 'finish', expected=1)
            known, _ = helper.preview(world, command, repo, operation, transaction['operation_id'], mode)
            result = recovery.call(world, command, 'recover-nested', '--repo', repo, '--proposal', known)
            self.assertTrue(result['verified'])
            if receipted and not partial_receipt:
                self.assertFalse(marker.exists())
                status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertTrue(status['semantic_recovery_complete'])
                recovery.public(world, command, 'compare', 'inspect', '--repo', repo, '--request', request,
                                '--private-output', output)
                compared = json.loads(output.read_text())
                self.assertEqual(compared['observation']['recovery_frontier']['unit_ids'], [])
                self.assertIn(target, {row['path'] for row in compared['matches']})
            else:
                self.assertEqual(marker.read_bytes(), hold)
                if not receipted:
                    self.assertEqual(nested.canonical(bank), original_bank)
                status = recovery.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(feature.read_text(), 'Unstaged feature\n')

    def test_without_nested_receipt(self):
        self.journey(False)

    def test_with_nested_receipt(self):
        self.journey(True)

    def test_without_nested_receipt_with_decoy(self):
        self.journey(False, decoy=True)

    def test_with_nested_receipt_with_decoy(self):
        self.journey(True, decoy=True)

    def test_boundary_rename_event(self):
        self.journey(False, boundary='rename0')

    def test_boundary_rename_target(self):
        self.journey(False, boundary='rename1')

    def test_boundary_rename_router(self):
        self.journey(False, boundary='rename2')

    def test_boundary_credited_unlinked(self):
        self.journey(True, boundary='unlinked')

    def test_boundary_hold_restore(self):
        self.journey(True, boundary='hold-restore', partial_receipt=True)
