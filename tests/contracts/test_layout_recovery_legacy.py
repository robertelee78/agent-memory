"""Published legacy readers must preserve an actual pending layout hold."""
import json
from pathlib import Path
import sys
import unittest
import uuid

from world import World
from runtime_update_fixtures import snapshot
from contracts.test_admission import AdmissionContract
import layout_recovery_fixtures as recovery
import v014_runtime_fixture


class LegacyLayoutConsumers(unittest.TestCase):
    def test_published_old_consumers_and_doctor_preserve_pending_hold(self):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)
            self.assertTrue(receipt['verified'])
            self.assertEqual(receipt['receipt']['source']['version'], '0.0.14')
            self.assertEqual(receipt['receipt']['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['receipt']['bin_dir']) / 'am'
            command, _, repo, bank, _, inspected, applied = recovery.small_pending(world)
            self.assertFalse(applied['semantic_recovery_complete'])
            self.assertTrue((bank / '.agent-memory-transaction.md').is_file())
            helper = AdmissionContract()
            helper.pid = inspected['manifest']['project_id']
            helper.actor = inspected['manifest']['identity']['scope_context']['developer_id']
            capture = recovery.save(world, helper.request())
            search = recovery.save(world, {'project_id': helper.pid, 'query': '', 'ids': [],
                'paths': [], 'context': {}, 'kinds': [], 'minimum_trust': 'reviewed',
                'limit': 8, 'include_context': True})
            feature = repo / 'feature.txt'
            feature.write_text('Staged feature work\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Synthetic feature', cwd=repo)
            feature.write_text('Staged replacement\n')
            world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged replacement\n')
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            def state():
                return (snapshot(bank), snapshot(directory), (repo / '.git/index').read_bytes(),
                        world.git('rev-parse', 'HEAD', cwd=repo).stdout, feature.read_bytes())
            before = state()
            operations = [
                ['inspect', '--repo', repo],
                ['bank', 'validate', '--repo', repo],
                ['search', '--repo', repo, '--request', search],
                ['capture', '--repo', repo, '--request', capture],
                ['purge-data', '--repo', repo, '--path', bank, '--check'],
                ['writes', 'recover', '--repo', repo],
                ['migrate', 'recover', '--repo', repo, '--mode', 'rollback'],
            ]
            for args in operations:
                with self.subTest(operation=args[0]):
                    result = world.run([old, *args], check=False, timeout=60)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual(state(), before)
            for executable in (old, command):
                result = world.run([executable, 'doctor', '--fix', '--repo', repo, '--json'],
                                   check=False, timeout=60)
                self.assertFalse(json.loads(result.stdout)['repairs_performed'])
                self.assertEqual(state(), before)


            # A foreign lock and an unrelated legacy-schema journal are not
            # authority to complete, remove or reinterpret the typed hold.
            lock = bank / '.agent-memory.lock'
            lock.write_text(json.dumps({'operation_id': str(uuid.uuid4()), 'operation': 'bank-replace',
                                        'host': 'foreign-host', 'pid': 1, 'start': '2026-10-22T18:00:00Z'}))
            lock.chmod(0o600)
            decoy_id = str(uuid.uuid4())
            decoy = directory.parent.parent / 'transactions' / decoy_id
            decoy.mkdir(parents=True, mode=0o700)
            journal = decoy / 'journal.json'
            journal.write_text(json.dumps({'schema': 'agent-memory/transaction-journal-v1',
                                           'proposal': {'operation_id': decoy_id}, 'base_files': {}}))
            journal.chmod(0o600)
            guarded = state(), snapshot(decoy)
            for executable, args in ((old, ['writes', 'recover', '--repo', repo]),
                                     (command, ['writes', 'recover', '--repo', repo]),
                                     (old, ['migrate', 'recover', '--repo', repo, '--mode', 'finish'])):
                result = world.run([executable, *args], check=False, timeout=60)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual((state(), snapshot(decoy)), guarded)
            result = world.run([command, 'doctor', '--fix', '--repo', repo, '--json'],
                               check=False, timeout=60)
            self.assertFalse(json.loads(result.stdout)['repairs_performed'])
            self.assertEqual((state(), snapshot(decoy)), guarded)

if __name__ == '__main__':
    unittest.main()
