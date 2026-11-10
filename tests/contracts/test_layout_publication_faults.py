"""Publication push interruption uses real remote readback on exact retry."""
import json
import unittest

from world import World
from contracts import test_layout_recovery_publication as publication
import layout_recovery_fixtures as recovery


class LayoutPublicationFaults(unittest.TestCase):
    def journey(self, boundary, mode):
        helper = publication.LayoutPublicationContract()
        with World() as world:
            command, _, repo, bank, remote, operation_id, intended = helper.public_repair(world)
            local = helper.state(world, repo, bank)
            before = world.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            observation_path = world.inside('publication-fault-observation.json')
            recovery.call(world, command, 'publish', 'inspect', '--repo', repo, '--operation', operation_id,
                          '--output', observation_path)
            observation = json.loads(observation_path.read_text())['observation']
            request = {'schema': 'agent-memory/layout-publication-prepare-v1',
                       'observation_id': observation['observation_id'], 'intended_files': intended,
                       'review': helper.publication_review(observation, intended)}
            prepared = world.inside('publication-fault-prepared.json')
            recovery.call(world, command, 'publish', 'prepare', '--repo', repo, '--operation', operation_id,
                          '--input', recovery.save(world, request), '--output', prepared)
            exact = recovery.save(world, json.loads(prepared.read_text())['request'])
            interrupted = world.run([command, 'bank', 'recover-layout', 'publish', 'land', '--repo', repo,
                                     '--operation', operation_id, '--input', exact,
                                     '--output', world.inside('interrupted-publication.json')],
                                    check=False, timeout=60,
                                    env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-publication-' + boundary + '-push',
                                         'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(interrupted.returncode, 86 if mode == 'exit' else 1,
                             interrupted.stdout + interrupted.stderr)
            interrupted_head = world.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            if boundary == 'before':
                self.assertEqual(interrupted_head, before)
            else:
                self.assertNotEqual(interrupted_head, before)
            self.assertEqual(helper.state(world, repo, bank), local)
            output = world.inside('publication-fault-retry.json')
            recovery.call(world, command, 'publish', 'land', '--repo', repo, '--operation', operation_id,
                          '--input', exact, '--output', output)
            landed = json.loads(output.read_text())
            self.assertTrue(landed['publication_verified'])
            if boundary == 'after':
                self.assertEqual(landed['state'], 'already-landed')
                self.assertEqual(world.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout, interrupted_head)
            completed_head = world.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            self.assertNotEqual(completed_head, before)
            again = world.inside('publication-fault-historical.json')
            recovery.call(world, command, 'publish', 'land', '--repo', repo, '--operation', operation_id,
                          '--input', exact, '--output', again)
            self.assertEqual(json.loads(again.read_text())['state'], 'already-landed')
            self.assertEqual(world.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout, completed_head)
            self.assertEqual(helper.state(world, repo, bank), local)

    def test_before_push_error(self):
        self.journey('before', 'error')

    def test_before_push_exit(self):
        self.journey('before', 'exit')

    def test_after_push_error(self):
        self.journey('after', 'error')

    def test_after_push_exit(self):
        self.journey('after', 'exit')
