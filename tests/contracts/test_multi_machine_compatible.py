"""Compatible publication from separate branches, then another machine's boundary."""
import unittest

from world import World
from multi_machine_fixtures import MultiMachineFixture
from contracts import test_retrieval as retrieval


class MultiMachineCompatible(MultiMachineFixture):
    def test_two_developers_three_clones_and_a_worktree_reconcile_without_importing_feature_code(self):
        with World() as w:
            a,b,c,sibling,dev_a,dev_b,baseline = self.setup_actors(w)
            (a/'memory-bank/projectbrief.md').write_text(baseline.replace('Use the isolated build.', 'Use the verified isolated build.'))
            (b/'memory-bank/projectbrief.md').write_text(baseline.replace('Review scoped sources.', 'Review scoped sources and their evidence.'))
            first,p1 = self.fact(a/'memory-bank','A-lesson','# Build lesson\n\nKeep the reviewed build observation conditional.\n',
                                  dev_a,'claude','A-session',{'component':'code.py'})
            second,p2 = self.fact(b/'memory-bank','B-lesson','# Review lesson\n\nKeep the independent review observation conditional.\n',
                                   dev_b,'codex','B-session',{'component':'code.py'})
            before = {str(repo):self.protected(w,repo) for repo in (a,b,c,sibling)}
            banks = {str(repo):self.state(w,repo,repo/'memory-bank')[0] for repo in (a,b,sibling)}
            candidate_a = self.publication(w,a)
            with self.actor(w,'profile-B'): candidate_b = self.publication(w,b)
            self.assertTrue(self.land(w,a,candidate_a)['publication_verified'])
            def compatible(left,right):
                return 'compatible' if left['path']==right['path']=='projectbrief.md' else 'related'
            with self.actor(w,'profile-B'):
                stale = self.land(w,b,candidate_b)
                self.assertFalse(stale['publication_verified']); self.assertEqual(stale['sharing_state'],'pending-reconciliation')
                fresh = self.publication(w,b,compatible); self.assertNotEqual(fresh['base'],candidate_b['base'])
                landed = self.land(w,b,fresh); self.assertTrue(landed['publication_verified'])
            shared = fresh['candidate']
            ancestors = set(w.git('rev-list',shared,cwd=b).stdout.splitlines())
            for repo in (a,b,c,sibling):self.assertNotIn(before[str(repo)][0][2].strip(),ancestors)
            self.assertEqual(w.git('show',shared+':code.py',cwd=b).stdout,'VALUE = 1\n')
            brief = w.git('show',shared+':memory-bank/projectbrief.md',cwd=b).stdout
            self.assertIn('verified isolated build',brief); self.assertIn('sources and their evidence',brief)
            project = first['project-id']
            recall = retrieval.RetrievalContract().request(project,query='',ids=[first['id'],second['id']],context={'component':'code.py'})
            with self.actor(w,'profile-C'):
                imported,proposal = self.refresh(w,c,recall)
                self.assertTrue(imported['remote_freshness_verified']); self.assertFalse(imported['feature_code_freshness_verified'])
                self.assertEqual(imported['recall']['scope_context']['developer_id'],dev_a)
                rows = {r['record']['id']:r['record'] for r in imported['recall']['results']}
                self.assertEqual(set(rows),{first['id'],second['id']})
                self.assertEqual(rows[second['id']]['origin']['developer-id'],dev_b)
                self.assertEqual(rows[second['id']]['origin']['harness'],'codex')
                self.assertEqual(rows[first['id']]['conditions'],{'component':'code.py'})
                wrong = retrieval.RetrievalContract().request(project,query='',ids=[first['id'],second['id']],context={'component':'other.py'})
                self.assertEqual(self.good(w,'retrieve.py','--repo',c,'--request',self.save(w,wrong))['results'],[])
                repeat = self.apply_refresh(w,proposal)
                self.assertTrue(repeat['remote_freshness_verified']); self.assertFalse(repeat['canonical_writes_performed'])
            self.assertEqual(len(list((c/'memory-bank/topics').glob('*.md'))),2)
            for repo in (a,b,c,sibling): self.assertEqual(self.protected(w,repo),before[str(repo)])
            for repo in (a,b,sibling): self.assertEqual(self.state(w,repo,repo/'memory-bank')[0],banks[str(repo)])
            self.event('compatible-multi-machine',participating_clones=3,linked_worktrees=1,developer_ids=2,
                       fixture_harnesses=['claude','codex','opencode','custom'],stale_candidate_refused=True,
                       actual_authority=landed['witnessed_commit'],qualified_imported_ids=2,feature_state_preserved=True)


if __name__=='__main__':unittest.main()
