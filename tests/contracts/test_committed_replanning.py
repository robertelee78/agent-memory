"""Committed receipt recovery stays strict while explicit ownership replans remain usable."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from runtime_update_fixtures import ROOT, fixture, advance, call, snapshot, receipt_path


class CommittedReplanningContract(unittest.TestCase):
    def test_outside_edit_after_lost_commit_response_requires_explicit_replan(self):
        with World() as world:
            channel, pack, receipt = fixture(world)
            revision = advance(world, channel, pack)
            call(world, receipt, expected=86, env={
                'AGENT_MEMORY_FAULT_STAGE': 'update-after-receipt',
                'AGENT_MEMORY_FAULT_MODE': 'exit'})
            path = world.paths['codex']/'AGENTS.md'
            outside = 'Keep this user text outside AM ownership.\n'
            path.write_text(path.read_text() + outside)
            state = receipt_path(world).parent
            pending = (state/'journal.json').read_bytes()
            helper = ROOT/'scripts/installer.py'
            for _ in range(2):
                refused = world.run([sys.executable, '-I', helper, 'recover'], check=False)
                self.assertEqual(refused.returncode, 1, refused.stdout + refused.stderr)
                value = json.loads(refused.stdout)
                self.assertFalse(value['verified'])
                self.assertEqual((state/'journal.json').read_bytes(), pending)
                self.assertTrue(path.read_text().endswith(outside))
            before = snapshot(world.root)
            self.assertTrue(call(world, receipt, '--check')[1]['eligible'])
            planned = world.run([sys.executable, '-I', helper, 'install', '--check',
                                 '--remote', channel, '--ref', 'main', '--presets', 'codex'])
            self.assertEqual(json.loads(planned.stdout)['schema'], 'agent-memory/install-plan-v1')
            removal = world.run([sys.executable, '-I', ROOT/'scripts/uninstall_runtime.py',
                                 'uninstall', '--check'])
            self.assertTrue(json.loads(removal.stdout)['check_complete'])
            self.assertEqual(snapshot(world.root), before)
            _, result = call(world, receipt)
            self.assertTrue(result['verified'])
            self.assertEqual(result['receipt']['source']['revision'], revision)
            self.assertTrue(path.read_text().endswith(outside))
            self.assertEqual(path.read_text().count('<!-- agent-memory:activation:begin -->'), 1)
            self.assertTrue(any(p.read_bytes() == pending for p in state.glob('journal-*.json')))
            recovered = world.run([sys.executable, '-I', helper, 'recover'])
            self.assertTrue(json.loads(recovered.stdout)['verified'])

    def test_uncommitted_or_conflicting_journals_never_authorize_readonly_plans(self):
        with World() as world:
            channel, _, receipt = fixture(world)
            journal = receipt_path(world).parent/'journal.json'
            original = json.loads(journal.read_bytes())
            commands = [
                [sys.executable, '-I', ROOT/'scripts/installer.py', 'install', '--check',
                 '--remote', channel, '--ref', 'main', '--presets', 'codex'],
                [sys.executable, '-I', ROOT/'scripts/update_runtime.py', 'update', '--check'],
                [sys.executable, '-I', ROOT/'scripts/uninstall_runtime.py', 'uninstall', '--check'],
            ]
            for case in ('staging', 'rollback-conflict', 'mismatched-receipt', 'mismatched-complete'):
                with self.subTest(case=case):
                    value = dict(original)
                    value['status'] = ('complete' if case == 'mismatched-complete' else
                                       'activating' if case == 'mismatched-receipt' else case)
                    if case.startswith('mismatched-'):
                        value['receipt_identity'] = {**original['receipt_identity'], 'hash': '0' * 64}
                    journal.write_text(json.dumps(value))
                    before = snapshot(world.root)
                    for command in commands:
                        refused = world.run(command, check=False)
                        self.assertEqual(refused.returncode, 1, refused.stdout + refused.stderr)
                        self.assertFalse(json.loads(refused.stdout)['verified'])
                        self.assertEqual(snapshot(world.root), before)

    def test_redirected_target_parent_refuses_at_preflight_without_writes(self):
        with World() as world:
            channel, pack, receipt = fixture(world)
            advance(world, channel, pack)
            call(world, receipt, expected=86, env={
                'AGENT_MEMORY_FAULT_STAGE': 'update-after-receipt',
                'AGENT_MEMORY_FAULT_MODE': 'exit'})
            retained = world.root/'retained-codex'
            world.paths['codex'].rename(retained)
            world.paths['codex'].symlink_to(retained, target_is_directory=True)
            before = snapshot(world.root)
            for args in (('recover',), ('install', '--remote', channel, '--ref', 'main', '--presets', 'codex')):
                refused = world.run([sys.executable, '-I', ROOT/'scripts/installer.py', *args], check=False)
                self.assertEqual(refused.returncode, 1, refused.stdout + refused.stderr)
                value = json.loads(refused.stdout)
                self.assertFalse(value['verified'])
                self.assertEqual(snapshot(world.root), before)

    def test_committed_managed_index_refresh_verifies_without_changing_payload_or_receipt(self):
        with World() as world:
            channel, pack, receipt = fixture(world)
            advance(world, channel, pack)
            call(world, receipt, expected=86, env={
                'AGENT_MEMORY_FAULT_STAGE': 'update-after-receipt',
                'AGENT_MEMORY_FAULT_MODE': 'exit'})
            receipt_bytes = receipt_path(world).read_bytes()
            code = Path(json.loads(receipt_bytes)['source']['path'])
            original = snapshot(code)
            clean = world.git('status', '--porcelain', '--untracked-files=all', cwd=code)
            self.assertEqual(clean.stdout, '')
            current = snapshot(code)
            self.assertEqual([name for name, value in current.items() if original.get(name) != value],
                             ['.git/index'])
            for _ in range(2):
                recovered = world.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'recover'])
                self.assertTrue(json.loads(recovered.stdout)['verified'])
                self.assertEqual(receipt_path(world).read_bytes(), receipt_bytes)
                self.assertEqual(snapshot(code), current)
