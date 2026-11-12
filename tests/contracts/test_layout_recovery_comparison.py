"""Pending private units cannot be omitted from an admission comparison."""
import copy
import json
import unittest

from world import World
from contracts import test_admission
import layout_recovery_fixtures as f


class RecoveryComparisonContract(unittest.TestCase):
    def test_lexical_miss_omission_and_substituted_frontier_never_prove_novelty(self):
        with World() as w:
            command, source, repo, bank, _, inspected, applied = f.small_pending(w)
            helper = test_admission.AdmissionContract('runTest')
            helper.pid = inspected['manifest']['project_id']
            helper.actor = inspected['manifest']['identity']['scope_context']['developer_id']
            capture = helper.request('verified-repository', 'durable-discovery')
            capture['repository_evidence'] = {'path': 'synthetic-policy.md',
                'sha256': f.sha((repo / 'synthetic-policy.md').read_bytes())}
            capture['candidate']['metadata']['evidence'] = ['synthetic-policy.md']
            capture['assessment']['confirmed'] = False
            result = f.public(w, command, 'capture', '--repo', repo, '--request', f.save(w, capture))
            self.assertNotEqual(result['decision'], 'eligible')
            status = f.call(w, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
            self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
            proposal, transaction, _ = f.admission_proposal(w, command, source, repo, bank, inspected, full=True)
            evidence = transaction['admission_evidence']
            request, proof = evidence['comparison_request'], evidence['comparison_proof']
            request_file = f.save(w, request)
            private = w.inside('comparison-control.json')
            accepted = w.run([command, 'compare', 'check', '--repo', repo, '--request', request_file,
                '--proof', f.save(w, proof), '--private-output', private], check=False, timeout=60)
            self.assertEqual(accepted.returncode, 0, accepted.stdout + accepted.stderr)
            view = json.loads(private.read_text())
            self.assertEqual({row['unit_id'] for row in view['pending_matches']}, set(applied['pending_unit_ids']))
            lexical_request = copy.deepcopy(request)
            lexical_request['searches'] = ['unrelated lexical sentinel', 'distinct unrelated search']
            lexical_path = w.inside('lexical-miss.json')
            f.public(w, command, 'compare', 'inspect', '--repo', repo, '--request',
                f.save(w, lexical_request), '--private-output', lexical_path)
            lexical = json.loads(lexical_path.read_text())
            self.assertEqual({row['unit_id'] for row in lexical['pending_matches']}, set(applied['pending_unit_ids']))
            for change in ('omitted', 'substituted'):
                altered = copy.deepcopy(proof)
                if change == 'omitted':
                    altered['judgment']['pending_decisions'] = []
                else:
                    altered['judgment']['observation']['recovery_frontier']['ledger_heads'] = []
                refused = w.run([command, 'compare', 'check', '--repo', repo, '--request', request_file,
                    '--proof', f.save(w, altered), '--private-output', w.inside(change + '.json')],
                    check=False, timeout=60)
                self.assertNotEqual(refused.returncode, 0)
                self.assertFalse(json.loads(refused.stdout)['comparison_read_complete'])
            self.assertTrue(proposal.exists())
            self.assertEqual(f.call(w, command, 'status', '--repo', repo,
                '--operation', inspected['operation_id'])['pending_unit_ids'], applied['pending_unit_ids'])


if __name__ == '__main__':
    unittest.main()
