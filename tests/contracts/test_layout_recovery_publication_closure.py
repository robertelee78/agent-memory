"""Public recovery publication policy and ordinary pending-frontier boundaries."""
import json
import unittest

from world import World
from contracts import test_layout_recovery_publication as publication_fixture
from layout_recovery_fixtures import call, inspect, installed, prepare, save


class RecoveryPublicationClosure(unittest.TestCase):
    def test_policy_and_required_review_leave_candidate_and_private_evidence_intact(self):
        helper = publication_fixture.LayoutPublicationContract()
        with World() as w:
            command, _, repo, bank, _, operation, intended = helper.public_repair(w)
            observed_path = w.inside('closure-publication-observation.json')
            call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', operation,
                 '--output', observed_path)
            observation = json.loads(observed_path.read_text())['observation']
            request_input = {'schema': 'agent-memory/layout-publication-prepare-v1',
                             'observation_id': observation['observation_id'], 'intended_files': intended,
                             'review': helper.publication_review(observation, intended)}
            prepared_path = w.inside('closure-publication-prepared.json')
            call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', operation,
                 '--input', save(w, request_input), '--output', prepared_path)
            prepared_bytes = prepared_path.read_bytes()
            request = json.loads(prepared_bytes)['request']
            request_path = save(w, request)
            before = helper.state(w, repo, bank)
            head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            policy = w.inside('github-policy.json')
            data = json.loads(policy.read_text())
            data['rules'] = [{'type': 'required_deployments'}]
            policy.write_text(json.dumps(data))
            pending = call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation,
                           '--input', request_path, '--output', w.inside('closure-policy.json'))
            self.assertEqual(pending['state'], 'pending-policy')
            self.assertFalse(pending['publication_verified'])
            self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            self.assertEqual(before, helper.state(w, repo, bank))
            self.assertEqual(prepared_bytes, prepared_path.read_bytes())
            self.assertEqual(request, json.loads(request_path.read_text()))
            data = json.loads(policy.read_text())
            data.update(rules=[], protected=True, review='REVIEW_REQUIRED')
            policy.write_text(json.dumps(data))
            pending = call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation,
                           '--input', request_path, '--output', w.inside('closure-rules.json'))
            self.assertEqual(pending['state'], 'pending-repository-rules')
            self.assertFalse(pending['publication_verified'])
            self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            self.assertEqual(before, helper.state(w, repo, bank))
            self.assertEqual(prepared_bytes, prepared_path.read_bytes())
            self.assertEqual(request, json.loads(request_path.read_text()))

    def test_ordinary_publish_refuses_a_previously_reviewed_proof_after_pending_handoff(self):
        helper = publication_fixture.LayoutPublicationContract()
        with World() as w:
            command, source = installed(w)
            repo, bank, peer, _ = helper.fixture(w)
            helper.github(w, repo, bank, peer)
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
