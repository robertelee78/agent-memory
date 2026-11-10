"""Public recovery publication policy and ordinary pending-frontier boundaries."""
import json
import unittest

from world import World
from contracts import test_layout_recovery_publication as publication_fixture
from layout_recovery_fixtures import call, inspect, installed, prepare, save


class RecoveryPublicationClosure(unittest.TestCase):
    def test_policy_and_required_review_leave_candidate_and_private_evidence_intact(self):
        for expected, policy_changes in (
                ('pending-policy', {'rules': [{'type': 'required_deployments'}]}),
                ('pending-repository-rules', {'rules': [], 'protected': True, 'review': 'REVIEW_REQUIRED'})):
            with self.subTest(expected=expected), World() as w:
                helper = publication_fixture.LayoutPublicationContract()
                command, _, repo, bank, _, operation, intended = helper.public_repair(w)
                policy = w.inside('github-policy.json')
                data = json.loads(policy.read_text())
                data.update(policy_changes)
                policy.write_text(json.dumps(data))
                observed_path = w.inside('closure-publication-observation.json')
                call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', operation,
                     '--output', observed_path)
                observed_bytes = observed_path.read_bytes()
                observation = json.loads(observed_bytes)['observation']
                request_input = {'schema': 'agent-memory/layout-publication-prepare-v1',
                                 'observation_id': observation['observation_id'], 'intended_files': intended,
                                 'review': helper.publication_review(observation, intended)}
                input_path = save(w, request_input)
                input_bytes = input_path.read_bytes()
                prepared_path = w.inside('closure-publication-prepared.json')
                before = helper.state(w, repo, bank)
                head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
                prepared = call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', operation,
                                '--input', input_path, '--output', prepared_path)
                if expected == 'pending-policy':
                    # Unsupported rules are refused at prepare, before a candidate can exist.
                    pending = prepared
                    self.assertNotIn('request', json.loads(prepared_path.read_text()))
                else:
                    self.assertEqual(prepared['state'], 'prepared')
                    prepared_bytes = prepared_path.read_bytes()
                    request = json.loads(prepared_bytes)['request']
                    request_path = save(w, request)
                    changed = json.loads(policy.read_text())
                    changed['rules'] = [{'type': 'required_deployments'}]
                    policy.write_text(json.dumps(changed))
                    stale = call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation,
                                 '--input', request_path, '--output', w.inside('closure-stale-rules.json'),
                                 expected=1)
                    self.assertFalse(stale['publication_verified'])
                    self.assertEqual(before, helper.state(w, repo, bank))
                    self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
                    self.assertEqual(prepared_bytes, prepared_path.read_bytes())
                    changed = json.loads(policy.read_text())
                    changed['rules'] = []
                    policy.write_text(json.dumps(changed))
                    pending = call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation,
                                   '--input', request_path, '--output', w.inside('closure-rules.json'))
                    self.assertEqual(prepared_bytes, prepared_path.read_bytes())
                    self.assertEqual(request, json.loads(request_path.read_text()))
                self.assertEqual(pending['state'], expected)
                self.assertFalse(pending['publication_verified'])
                self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
                self.assertEqual(before, helper.state(w, repo, bank))
                self.assertEqual(observed_bytes, observed_path.read_bytes())
                self.assertEqual(input_bytes, input_path.read_bytes())

    def test_ordinary_publish_refuses_a_previously_reviewed_proof_after_pending_handoff(self):
        helper = publication_fixture.LayoutPublicationContract()
        with World() as w:
            command, source = installed(w)
            repo, bank, peer, _ = helper.fixture(w)
            helper.github(w, repo, bank, peer)
            (bank/'activeContext.md').write_text(
                '# Current focus\n\nKeep independently useful learning conditional on unfinished work.\n')
            proof = helper.proof(w, repo)
            proof_path = save(w, proof)
            control = w.run([command, 'publish', 'prepare', '--repo', repo, '--input', proof_path],
                            check=False, timeout=60)
            self.assertEqual(control.returncode, 0, control.stdout + control.stderr)
            self.assertEqual(json.loads(control.stdout)['sharing_state'], 'prepared')
            intended = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')
                        if not p.name.startswith('.agent-memory')}
            (bank/'handoff-prompt.md').write_text('# Unconfirmed synthetic handoff\n\nUnknown provenance requires independent review.\n')
            _, observed = inspect(w, command, repo)
            proposal, _ = prepare(w, command, source, repo, observed, intended)
            applied = call(w, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            before = helper.state(w, repo, bank)
            head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            process = w.run([command, 'publish', 'prepare', '--repo', repo, '--input', proof_path],
                            check=False, timeout=60)
            self.assertNotEqual(process.returncode, 0)
            self.assertEqual(before, helper.state(w, repo, bank))
            self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            self.assertNotIn('Unknown provenance requires independent review.', process.stdout + process.stderr)
