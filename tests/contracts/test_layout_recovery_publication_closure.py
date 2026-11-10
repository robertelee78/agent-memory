"""Public recovery publication policy and ordinary pending-frontier boundaries."""
import json
import unittest

from world import World
from contracts import test_layout_recovery_publication as publication_fixture
from layout_recovery_fixtures import call, inspect, installed, prepare, private_directory, save
from runtime_update_fixtures import snapshot


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
            directory = private_directory(w, bank, operation)
            private_before = snapshot(directory)
            candidate = w.git('cat-file', 'commit', request['candidate'], cwd=repo).stdout
            head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            policy = w.inside('github-policy.json')
            baseline = json.loads(policy.read_text())
            for name, changes in (
                    ('policy', {'rules': [{'type': 'required_deployments'}]}),
                    ('required-review', {'rules': [], 'protected': True, 'review': 'REVIEW_REQUIRED'})):
                with self.subTest(policy=name):
                    policy.write_text(json.dumps({**baseline, **changes}))
                    # The reviewed observation pins repository policy. A later
                    # policy change invalidates that request before landing;
                    # the public contract is refusal without publication or
                    # replacement, not a particular internal policy label.
                    pending = call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation,
                                   '--input', request_path, '--output', w.inside('closure-' + name + '.json'),
                                   expected=1)
                    self.assertEqual(pending['state'], 'pending')
                    self.assertFalse(pending['verified'])
                    for field in ('publication_verified', 'canonical_writes_performed',
                                  'canonical_mutation_may_have_occurred', 'original_index_writes_performed',
                                  'head_writes_performed', 'feature_writes_performed', 'history_writes_performed'):
                        self.assertFalse(pending[field], field)
                    self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
                    self.assertEqual(before, helper.state(w, repo, bank))
                    self.assertEqual(private_before, snapshot(directory))
                    self.assertEqual(candidate, w.git('cat-file', 'commit', request['candidate'], cwd=repo).stdout)
                    self.assertEqual(prepared_bytes, prepared_path.read_bytes())
                    self.assertEqual(request, json.loads(request_path.read_text()))

    def test_policy_at_observation_refuses_prepare_or_required_review_land(self):
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
                directory = private_directory(w, bank, operation)
                private_before = snapshot(directory)
                head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
                prepared = call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', operation,
                                '--input', input_path, '--output', prepared_path,
                                expected=1 if expected == 'pending-policy' else 0)
                if expected == 'pending-policy':
                    # Unsupported rules refuse before a candidate can exist.
                    pending = prepared
                    self.assertNotIn('request', json.loads(prepared_path.read_text()))
                else:
                    self.assertEqual(prepared['state'], 'prepared')
                    prepared_bytes = prepared_path.read_bytes()
                    request = json.loads(prepared_bytes)['request']
                    request_path = save(w, request)
                    private_before = snapshot(directory)
                    candidate = w.git('cat-file', 'commit', request['candidate'], cwd=repo).stdout
                    changed = json.loads(policy.read_text())
                    changed['rules'] = [{'type': 'required_deployments'}]
                    policy.write_text(json.dumps(changed))
                    stale = call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation,
                                 '--input', request_path, '--output', w.inside('closure-stale-rules.json'),
                                 expected=1)
                    self.assertFalse(stale['publication_verified'])
                    self.assertEqual(before, helper.state(w, repo, bank))
                    self.assertEqual(private_before, snapshot(directory))
                    self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
                    self.assertEqual(candidate, w.git('cat-file', 'commit', request['candidate'], cwd=repo).stdout)
                    self.assertEqual(prepared_bytes, prepared_path.read_bytes())
                    changed['rules'] = []
                    policy.write_text(json.dumps(changed))
                    pending = call(w, command, 'publish', 'land', '--repo', repo, '--operation', operation,
                                   '--input', request_path, '--output', w.inside('closure-rules.json'),
                                   expected=1)
                    self.assertEqual(candidate, w.git('cat-file', 'commit', request['candidate'], cwd=repo).stdout)
                    self.assertEqual(prepared_bytes, prepared_path.read_bytes())
                    self.assertEqual(request, json.loads(request_path.read_text()))
                self.assertEqual(pending['state'], expected)
                self.assertFalse(pending['publication_verified'])
                self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
                self.assertEqual(before, helper.state(w, repo, bank))
                self.assertEqual(private_before, snapshot(directory))
                self.assertEqual(observed_bytes, observed_path.read_bytes())
                self.assertEqual(input_bytes, input_path.read_bytes())

    def test_ordinary_publish_refuses_a_previously_reviewed_proof_after_pending_handoff(self):
        helper = publication_fixture.LayoutPublicationContract()
        with World() as w:
            command, source = installed(w)
            repo, bank, peer, _ = helper.fixture(w)
            helper.github(w, repo, bank, peer)
            context = bank / 'activeContext.md'
            # Preserve the existing orientation as ordinary context, not the
            # legacy fixture's incomplete reserved checkpoint heading, and
            # create a real local memory delta before reviewing publication.
            context.write_text(context.read_text().replace('# Current focus\n', '# Active context\n', 1)
                               + '\nUnder the controlled fixture condition, preserve the source before acting.\n')
            proof = helper.proof(w, repo)
            proof_path = save(w, proof)
            proof_bytes = proof_path.read_bytes()
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
            directory = private_directory(w, bank, observed['operation_id'])
            private_before = snapshot(directory)
            head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            process = w.run([command, 'publish', 'prepare', '--repo', repo, '--input', proof_path],
                            check=False, timeout=60)
            self.assertNotEqual(process.returncode, 0)
            self.assertEqual(before, helper.state(w, repo, bank))
            self.assertEqual(private_before, snapshot(directory))
            self.assertEqual(proof_bytes, proof_path.read_bytes())
            self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            self.assertNotIn('Unknown provenance requires independent review.', process.stdout + process.stderr)
