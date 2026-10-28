"""Explicit push-user policy via copied public helpers and real guarded Git refs."""
import json
from world import World
from publication_fixtures import PublicationFixture

USER = {'id': 7, 'login': 'allowed-user', 'type': 'User'}


class RestrictedActorContract(PublicationFixture):
    def setup_policy(self, w):
        repo, bank, peer, _ = self.fixture(w)
        policy = self.github(w, repo, bank, peer, protected=True)
        (bank/'activeContext.md').write_text('# Current focus\n\nIndependent reviewed learning.\n')
        d = json.loads(policy.read_text())
        d.update(restrictions={'users': [USER], 'teams': [], 'apps': []}, allowed_users=[USER])
        policy.write_text(json.dumps(d))
        return repo, bank, policy

    def edit(self, policy, **values):
        d = json.loads(policy.read_text()); d.update(values); policy.write_text(json.dumps(d))

    def test_allowed_user_still_waits_for_reviews_and_checks_then_lands(self):
        with World() as w:
            repo, bank, policy = self.setup_policy(w); before = self.state(w, repo, bank)
            prepared = self.prepared(w, repo)
            self.assertTrue(prepared['repository_policy']['push_actor']['eligible'])
            request = prepared['request']
            pending = self.land(w, repo, request)
            self.assertEqual(pending['sharing_state'], 'pending-repository-rules')
            self.edit(policy, review='APPROVED', checks='PENDING')
            self.assertFalse(self.land(w, repo, request)['publication_verified'])
            self.edit(policy, checks='SUCCESS')
            landed = self.land(w, repo, request)
            self.assertTrue(landed['publication_verified'])
            self.assertEqual(before, self.state(w, repo, bank))

    def test_all_negative_identity_and_inventory_cases_refuse_prepare(self):
        cases = [
            {'allowed_users': []},  # admin/write is not an explicit grant
            {'actor': {**USER, 'id': 8}},
            {'actor': {**USER, 'login': 'different-user'}},
            {'actor_unavailable': True},
            {'allowed_users': [USER, USER]},
            {'allowed_users': [{**USER, 'id': True}]},
            {'allowed_users': {'users': [USER]}},
            {'restrictions': {'users': [USER]}},
            {'allowed_users': [], 'restrictions': {'users': [], 'teams': [{'id': 7}], 'apps': []}},
            {'allowed_users': [], 'restrictions': {'users': [], 'teams': [], 'apps': [{'id': 7}]}},
            {'allowlist_failed_page': 1},
        ]
        with World() as w:
            repo, bank, policy = self.setup_policy(w); baseline = json.loads(policy.read_text())
            proof = self.proof(w, repo); before = self.state(w, repo, bank)
            for values in cases:
                with self.subTest(values=values):
                    policy.write_text(json.dumps({**baseline, **values}))
                    result = self.good(w, 'publish_memory.py', 'prepare', '--repo', repo,
                                       '--input', self.save(w, proof))
                    self.assertEqual(result['sharing_state'], 'pending-policy')
                    self.assertFalse(result['publication_verified']); self.assertNotIn('request', result)
                    self.assertIn('restricted-push-actor-unverified', result['unsupported_rules'])
                    self.assertIn('repair', result['policy_failure'])
                    self.assertTrue(all(e['method'] == 'GET' for e in json.loads(policy.read_text())['events']))
                    self.assertEqual(before, self.state(w, repo, bank))

    def test_complete_later_page_succeeds_but_incomplete_chains_refuse(self):
        with World() as w:
            repo, bank, policy = self.setup_policy(w)
            users = [{'id': n+100, 'login': 'user-'+str(n), 'type': 'User'} for n in range(400)]
            self.edit(policy, allowed_users=users[:100]+[{**USER, 'login': 'Allowed-User'}])
            prepared = self.prepared(w, repo)
            self.assertEqual(prepared['sharing_state'], 'prepared')
            proof = prepared['request']['proof']; before = self.state(w, repo, bank)
            baseline = json.loads(policy.read_text())
            for change in [{'allowlist_failed_page': 2}, {'broken_next': True},
                           {'allowed_users': users+[USER]},
                           {'allowed_users': users[:100]+[users[0]]}]:
                with self.subTest(change=change):
                    policy.write_text(json.dumps({**baseline, **change}))
                    result = self.good(w, 'publish_memory.py', 'prepare', '--repo', repo,
                                       '--input', self.save(w, proof))
                    self.assertEqual(result['sharing_state'], 'pending-policy')
                    self.assertFalse(result['publication_verified'])
                    self.assertEqual(before, self.state(w, repo, bank))

    def test_actor_revoked_after_prepare_and_transport_rejection_stay_unpublished(self):
        with World() as w:
            repo, bank, policy = self.setup_policy(w)
            request = self.prepared(w, repo)['request']; before = self.state(w, repo, bank)
            head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            self.edit(policy, allowed_users=[])
            self.assertEqual(self.land(w, repo, request)['sharing_state'], 'pending-policy')
            self.edit(policy, allowed_users=[USER], review='APPROVED', transport_actor_id=8)
            rejected = self.land(w, repo, request)
            self.assertFalse(rejected['publication_verified'])
            self.assertIn('different credentials', rejected['reason'])
            self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            self.assertEqual(before, self.state(w, repo, bank))

    def test_immediate_final_recheck_refuses_actor_allowlist_or_restriction_change(self):
        for field in ['change_actor_at', 'change_allowlist_at', 'remove_restrictions_at']:
            with self.subTest(field=field), World() as w:
                repo, bank, policy = self.setup_policy(w)
                request = self.prepared(w, repo)['request']; before = self.state(w, repo, bank)
                head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
                d = json.loads(policy.read_text())
                counter = 'actor_reads' if field == 'change_actor_at' else 'protection_reads'
                self.edit(policy, review='APPROVED', **{field: d[counter]+2})
                process, result = self.call(w, 'publish_memory.py', 'land', '--repo', repo,
                                            '--input', self.save(w, request))
                self.assertEqual(process.returncode, 1)
                self.assertFalse(result['publication_verified'])
                self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
                self.assertEqual(before, self.state(w, repo, bank))
