"""A retained recovery backup is not independent repository evidence."""
import copy
import json
import unittest

from contracts import test_admission
import layout_recovery_fixtures as f
from world import World


def snapshot(directory):
    return {p.relative_to(directory).as_posix(): p.read_bytes()
            for p in directory.rglob('*') if p.is_file()}


class RecoveryBackupCaptureContract(unittest.TestCase):
    def test_private_backup_cannot_substitute_independent_repository_capture(self):
        with World() as world:
            command, _, repo, bank, _, inspected, applied = f.small_pending(world)
            directory = f.private_directory(world, bank, inspected['operation_id'])
            source = next(row for row in inspected['manifest']['sources']
                          if row['path'] == 'handoff-prompt.md')
            backup = directory / source['blob']
            original = backup.read_bytes()
            self.assertEqual(original, (repo / 'synthetic-policy.md').read_bytes())
            helper = test_admission.AdmissionContract('runTest')
            helper.pid = inspected['manifest']['project_id']
            helper.actor = inspected['manifest']['identity']['scope_context']['developer_id']
            request = helper.request('verified-repository', 'durable-discovery')
            request['candidate']['body'] = f.policy_body(0)
            request['candidate']['metadata']['evidence'] = ['synthetic-policy.md']
            request['repository_evidence'] = {'path': 'synthetic-policy.md', 'sha256': f.sha(original)}
            accepted = world.run([command, 'capture', '--repo', repo, '--request', f.save(world, request)],
                                 check=False, timeout=60)
            self.assertEqual(accepted.returncode, 0, accepted.stdout + accepted.stderr)
            self.assertEqual(json.loads(accepted.stdout)['decision'], 'eligible')
            alias = repo / 'backup-evidence.md'
            alias.symlink_to(backup)
            before_bank, before_private = snapshot(bank), snapshot(directory)
            index_path = repo / '.git/index'
            self.assertFalse(index_path.exists())
            for path in (str(backup), 'backup-evidence.md'):
                with self.subTest(path=path):
                    bad = copy.deepcopy(request)
                    bad['repository_evidence']['path'] = path
                    bad['candidate']['metadata']['evidence'] = [path]
                    refused = world.run([command, 'capture', '--repo', repo, '--request', f.save(world, bad)],
                                        check=False, timeout=60)
                    self.assertNotEqual(refused.returncode, 0)
                    self.assertEqual(snapshot(bank), before_bank)
                    self.assertEqual(snapshot(directory), before_private)
                    self.assertFalse(index_path.exists())
                    self.assertEqual(backup.read_bytes(), original)
                    self.assertTrue(alias.is_symlink())
            status = f.call(world, command, 'status', '--repo', repo,
                            '--operation', inspected['operation_id'])
            self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            self.assertFalse(status['semantic_recovery_complete'])
