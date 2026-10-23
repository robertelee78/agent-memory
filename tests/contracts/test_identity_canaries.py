"""Repository isolation and shared provenance across disposable machine roots."""
import copy
import unittest
import uuid

from world import World
from release_canary_fixtures import PortableCanaryFixture


class IdentityCanaries(PortableCanaryFixture, unittest.TestCase):
    def test_checkpoint_provenance_survives_fresh_machine_and_second_developer_without_personal_bank(self):
        with World() as world:
            repo, bank, project, first = self.fixture(world)
            before, code = self.bank_state(bank), self.code_state(world, repo)
            self.checkpoint(world, repo, first)
            self.assertEqual(self.code_state(world, repo), code)
            for path in before.keys() - {'activeContext.md'}:
                self.assertEqual(self.bank_state(bank)[path], before[path])
            active = (bank / 'activeContext.md').read_text()
            self.assertIn('Original evidence', active)
            self.assertIn('applicability elsewhere is unknown', active)
            world.git('add', 'memory-bank', cwd=repo)
            world.git('commit', '-m', 'Explicit fixture memory transfer', cwd=repo)
            clone = world.clone(repo, 'second machine clone')
            world.git('remote', 'remove', 'origin', cwd=clone)
            # A separate disposable machine has no registry/cache from machine one.
            machine = world.inside('second-machine')
            machine.mkdir()
            for key in ('HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'XDG_CACHE_HOME',
                        'AGENT_MEMORY_CONFIG_HOME', 'AGENT_MEMORY_STATE_HOME', 'GH_CONFIG_DIR'):
                target = machine / key.lower()
                target.mkdir()
                world.env[key] = str(target)
            checked = self.good(world, 'bank_contract.py', 'validate', '--repo', clone)
            self.assertEqual(checked['project_id'], project)
            self.assertEqual((clone / 'memory-bank/activeContext.md').read_text(), active)
            self.assertIn('independent reproduction', active)
            second = self.developer(world)
            self.assertNotEqual(first, second)
            self.bind(world, clone)
            clone_code = self.code_state(world, clone)
            self.checkpoint(world, clone, second)
            self.assertEqual(self.code_state(world, clone), clone_code)
            self.assertEqual(self.good(world, 'scope_binding.py', 'status', '--repo', clone)['project_id'], project)
            self.assertEqual((bank / 'activeContext.md').read_text(), active)
            self.assertFalse((clone / 'memory-bank/personal').exists())

    def test_replacing_bank_with_another_project_never_silently_rebinds_or_recalls_it(self):
        with World() as world:
            repo, bank, project, _ = self.fixture(world)
            code = self.code_state(world, repo)
            foreign = str(uuid.uuid4())
            router = bank / 'MEMORY.md'
            router.write_text(router.read_text().replace(project, foreign))
            active = bank / 'activeContext.md'
            active.write_text('# Focus\n\nFOREIGN_PROJECT_BODY_MUST_NOT_RETURN\n')
            changed = self.bank_state(bank)
            # Matching the newly copied header does not authorize its project.
            for requested in (project, foreign):
                result, value = self.policy(world, repo, requested, category='retrieval',
                    text='The requested lesson is in the selected repository.')
                self.assertEqual(result.returncode, 1, value)
                self.assertNotIn('FOREIGN_PROJECT_BODY', result.stdout)
                self.assertFalse(value['canonical_writes_performed'])
            status = self.good(world, 'scope_binding.py', 'status', '--repo', repo)
            self.assertFalse(status['resolved'])
            self.assertEqual(self.bank_state(bank), changed)
            self.assertEqual(self.code_state(world, repo), code)

    def test_changed_developer_selection_invalidates_old_checkpoint_without_reattribution(self):
        with World() as world:
            repo, bank, _, first = self.fixture(world)
            current = self.good(world, 'checkpoint.py', 'inspect', '--repo', repo)
            request = {'focus': 'First developer is investigating the retry discrepancy.',
                       'recent_changes': ['The source and tool disagree.'],
                       'open_questions': ['Which result reproduces?'],
                       'next_step': 'Run the independent reproduction.', 'conditions': {'component': 'retry'},
                       'evidence': ['../app.py'], 'harness': 'codex', 'session': 'first-session',
                       'observed_active_sha256': current['observed_active_sha256']}
            proposal = self.good(world, 'checkpoint.py', 'preview', '--repo', repo,
                                 '--input', self.save(world, request))
            retained = copy.deepcopy(proposal)
            second = self.developer(world)
            self.assertNotEqual(first, second)
            before, code = self.bank_state(bank), self.code_state(world, repo)
            result, value = self.helper(world, 'checkpoint.py', 'apply',
                                        '--proposal', self.save(world, retained))
            self.assertEqual(result.returncode, 1, value)
            self.assertEqual(self.bank_state(bank), before)
            self.assertEqual(self.code_state(world, repo), code)
            # A fresh proposal can use the current actor, with the same shared bank.
            self.checkpoint(world, repo, second)


if __name__ == '__main__':
    unittest.main()

