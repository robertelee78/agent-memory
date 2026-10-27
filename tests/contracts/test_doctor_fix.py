"""Public doctor repair uses owned state and preserves user data."""
import json
from pathlib import Path
import unittest

from world import World
from runtime_update_fixtures import fixture, snapshot, advance, call, bank as make_bank


class DoctorFixContract(unittest.TestCase):
    def test_noop_and_restore_missing_owned_hook(self):
        with World() as world:
            repo, _, receipt = fixture(world)
            cli = Path(receipt['bin_dir']) / 'am'
            result = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            value = json.loads(result.stdout)
            self.assertEqual(result.returncode, 0, value)
            self.assertFalse(value['repairs_performed'])
            hook = world.paths['codex'] / 'hooks.json'
            self.assertTrue(hook.is_file())
            bank = make_bank(world, repo)
            world.git('add', '.', cwd=repo)
            world.git('commit', '-m', 'Track fixture memory', cwd=repo)
            before = snapshot(bank)
            hook.unlink()
            repaired = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            data = json.loads(repaired.stdout)
            self.assertEqual(repaired.returncode, 0, data)
            self.assertTrue(data['repairs_performed'])
            self.assertTrue(hook.is_file())
            self.assertEqual(snapshot(bank), before)
            self.assertIn('codex', data['restart_clients'])

    def test_interrupted_bank_is_reported_and_preserved(self):
        with World() as world:
            repo, _, receipt = fixture(world)
            cli = Path(receipt['bin_dir']) / 'am'
            bank = make_bank(world, repo)
            marker = bank / '.agent-memory-transaction.md'
            marker.write_text('preserve interrupted transaction\n')
            before = snapshot(bank)
            result = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            data = json.loads(result.stdout)
            self.assertEqual(snapshot(bank), before)
            skipped = [entry for entry in data['repairs'] if entry['component'] == 'bank.transaction' and entry['state'] == 'skipped']
            self.assertEqual(skipped[0]['state'], 'skipped')
            self.assertIn('original operation', skipped[0]['repair'])

    def test_missing_owned_runtime_targets_are_recreated(self):
        with World() as world:
            repo, _, receipt = fixture(world)
            cli = Path(receipt['bin_dir']) / 'am'
            targets = [entry for entry in receipt['targets'] if entry['kind'] in {'copy', 'link', 'activation', 'wrapper'} and Path(entry['path']) != cli]
            self.assertTrue(targets)
            for entry in targets:
                path = Path(entry['path'])
                if path.is_dir() and not path.is_symlink():
                    import shutil
                    shutil.rmtree(path)
                else:
                    path.unlink()
            result = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            data = json.loads(result.stdout)
            self.assertEqual(result.returncode, 0, data)
            self.assertTrue(data['repairs_performed'])
            for entry in targets:
                self.assertTrue(Path(entry['path']).exists())

    def test_receipted_bank_transaction_cleanup(self):
        from contracts.test_bank_transactions import BankTransactionsContract
        helper = BankTransactionsContract()
        with World() as world:
            _, _, receipt = fixture(world)
            repo, bank, _, after, proposal = helper.fixture(world)
            helper.crash(world, proposal, 'transaction-after-receipt')
            helper.unlock(world, repo)
            cli = Path(receipt['bin_dir']) / 'am'
            result = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            data = json.loads(result.stdout)
            self.assertEqual(result.returncode, 0, data)
            self.assertFalse((bank / '.agent-memory-transaction.md').exists())
            self.assertEqual(helper.canonical(bank), after)
            self.assertTrue(any(a['component'] == 'bank.transaction' and a['state'] == 'repaired' for a in data['repairs']))

    def test_modified_owned_hook_is_preserved_and_not_replaced(self):
        with World() as world:
            repo, _, receipt = fixture(world)
            cli = Path(receipt['bin_dir']) / 'am'
            hook = world.paths['codex'] / 'hooks.json'
            hook.write_text('{"hooks": {"UserPromptSubmit": []}, "user": "preserve"}\n')
            before = hook.read_bytes()
            result = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(hook.read_bytes(), before)
            self.assertFalse(json.loads(result.stdout)['repairs_performed'])

    def test_foreign_hook_configuration_is_preserved_and_receipt_reconciled(self):
        with World() as world:
            repo, _, receipt = fixture(world)
            cli = Path(receipt['bin_dir']) / 'am'
            hook = world.paths['codex'] / 'hooks.json'
            value = json.loads(hook.read_text())
            value['user-setting'] = {'keep': True}
            hook.write_text(json.dumps(value, indent=2) + '\n')
            before = hook.read_bytes()
            result = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            data = json.loads(result.stdout)
            self.assertEqual(result.returncode, 0, data)
            self.assertTrue(data['repairs_performed'])
            self.assertEqual(hook.read_bytes(), before)
            self.assertEqual(data['restart_clients'], [])

    def test_recover_update_interrupted_before_receipt(self):
        with World() as world:
            repo, pack, receipt = fixture(world)
            advance(world, repo, pack)
            call(world, receipt, expected=86, env={
                'AGENT_MEMORY_FAULT_STAGE': 'update-before-receipt',
                'AGENT_MEMORY_FAULT_MODE': 'exit',
            })
            journal = world.paths['state'] / 'agent-memory/install/journal.json'
            cli = Path(receipt['bin_dir']) / 'am'
            result = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            data = json.loads(result.stdout)
            self.assertEqual(result.returncode, 0, data)
            self.assertTrue(data['repairs_performed'])
            self.assertEqual(json.loads(journal.read_text())['status'], 'rolled-back')

    def test_recover_committed_interrupted_update_from_recorded_journal(self):
        with World() as world:
            repo, pack, receipt = fixture(world)
            advance(world, repo, pack)
            call(world, receipt, expected=86, env={
                'AGENT_MEMORY_FAULT_STAGE': 'update-after-receipt',
                'AGENT_MEMORY_FAULT_MODE': 'exit',
            })
            journal = world.paths['state'] / 'agent-memory/install/journal.json'
            self.assertNotEqual(json.loads(journal.read_text())['status'], 'complete')
            cli = Path(receipt['bin_dir']) / 'am'
            result = world.run([cli, 'doctor', '--fix', '--repo', repo, '--json'], check=False)
            data = json.loads(result.stdout)
            self.assertEqual(result.returncode, 0, data)
            self.assertTrue(data['repairs_performed'])
            self.assertEqual(json.loads(journal.read_text())['status'], 'complete')
