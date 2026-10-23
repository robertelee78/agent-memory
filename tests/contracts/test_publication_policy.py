"""Actual public policy/check refusal and linked-worktree index isolation."""
import json
import shutil
import unittest

from world import World
from publication_fixtures import PublicationFixture


class PublicationPolicyContract(PublicationFixture):
    def test_rule_inventory_reads_the_next_page_before_deciding_automation_is_allowed(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); policy = self.github(w, repo, bank, peer)
            (bank/'activeContext.md').write_text('# Current focus\n\nA durable independent update.\n')
            before = self.state(w, repo, bank)
            d = json.loads(policy.read_text()); d['rules'] = [{'type':'non_fast_forward'} for _ in range(100)]+[{'type':'required_deployments'}]
            policy.write_text(json.dumps(d))
            prepared = self.prepared(w, repo)
            self.assertEqual(prepared['sharing_state'], 'pending-policy')
            self.assertIn('required_deployments', prepared['unsupported_rules'])
            events = json.loads(policy.read_text())['events']
            self.assertTrue(any('rules/branches/main?per_page=100&page=2' == e['endpoint'].split('repos/fixture/memory/')[-1] for e in events))
            self.assertTrue(all(e['method']=='GET' for e in events)); self.assertEqual(before, self.state(w, repo, bank))

    def test_clean_rollup_cannot_substitute_for_missing_or_wrong_app_required_checks(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); policy = self.github(w, repo, bank, peer, protected=True)
            (bank/'activeContext.md').write_text('# Current focus\n\nA source-linked update requiring the configured CI.\n')
            d = json.loads(policy.read_text()); d.update(review='APPROVED', missing_required_check=True)
            policy.write_text(json.dumps(d)); request = self.prepared(w, repo)['request']; before = self.state(w, repo, bank)
            head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            missing = self.land(w, repo, request); self.assertFalse(missing['publication_verified'])
            self.assertIn('required status', missing['reason'].lower())
            d = json.loads(policy.read_text()); d['missing_required_check'] = False
            d['rules'] = [{'type':'required_status_checks','parameters':{'required_status_checks':[{'context':'fixture-ci','integration_id':2}]}}]
            policy.write_text(json.dumps(d)); wrong = self.land(w, repo, request)
            self.assertFalse(wrong['publication_verified']); self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            d = json.loads(policy.read_text()); d['check_app'] = 2; policy.write_text(json.dumps(d))
            landed = self.land(w, repo, request); self.assertTrue(landed['publication_verified'])
            self.assertEqual(before, self.state(w, repo, bank))

