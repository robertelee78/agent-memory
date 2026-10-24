"""Cross-developer contradiction holds disputed dependencies while useful work lands."""
import unittest

from world import World
from multi_machine_fixtures import MultiMachineFixture
from contracts import test_retrieval as retrieval


class MultiMachineConflicts(MultiMachineFixture):
    def test_detecting_developer_sees_both_origins_without_pausing_independent_memory_or_code(self):
        with World() as w:
            a,b,c,sibling,dev_a,dev_b,_baseline = self.setup_actors(w)
            yellow,p1 = self.fact(a/'memory-bank','yellow','# Wall\n\nThe same room wall should be yellow.\n',dev_a,'claude','A-session')
            dependent,child = self.fact(a/'memory-bank','paint-order','# Order\n\nThis order depends on the disputed wall color.\n',dev_a,'claude','A-session')
            blue,p2 = self.fact(b/'memory-bank','blue','# Wall\n\nThe same room wall should be blue.\n',dev_b,'codex','B-session')
            independent,path = self.fact(b/'memory-bank','independent','# Independent\n\nAn independent reviewed observation remains useful.\n',
                                         dev_b,'codex','B-session',{'component':'code.py'})
            (a/'memory-bank/activeContext.md').write_text('# Current focus\n\nIndependent local work continues while clarification is pending.\n')
            before = {str(repo):self.protected(w,repo) for repo in (a,b,c,sibling)}
            with self.actor(w,'profile-B'):
                published = self.publication(w,b); self.assertTrue(self.land(w,b,published)['publication_verified'])
            def decide(left,right):
                return 'contradiction' if {left['path'],right['path']}=={p1,p2} else 'related'
            recall = retrieval.RetrievalContract().request(yellow['project-id'],query='',paths=[p1,p2,child,path],
                                                          context={'component':'code.py'},limit=1)
            refreshed,_proposal = self.refresh(w,a,recall,decide,{child:[p1]})
            self.assertTrue(self.reviewed['recommend_other_developer_clarification'])
            self.assertIn(dev_b,self.reviewed['other_developer_ids']); self.assertFalse(self.reviewed['messages_sent'])
            self.assertTrue(self.reviewed['independent_updates_may_continue'])
            self.assertTrue(refreshed['conflicts_pending']); self.assertEqual(refreshed['sharing_state'],'refreshed-with-conflicts')
            self.assertEqual([r['path'] for r in refreshed['recall']['results']],[path])
            self.assertGreaterEqual(refreshed['recall']['excluded_counts']['pending-semantic-conflict'],2)
            self.assertTrue((a/'memory-bank'/p1).exists()); self.assertTrue((a/'memory-bank'/child).exists())
            self.assertFalse((a/'memory-bank'/p2).exists())
            # The same detecting session can still publish its independent focus.
            request = self.publication(w,a,decide,{child:[p1]}); landed = self.land(w,a,request)
            self.assertTrue(landed['publication_verified'])
            shared = request['candidate']
            ancestors = set(w.git('rev-list',shared,cwd=a).stdout.splitlines())
            for repo in (a,b,c,sibling):self.assertNotIn(before[str(repo)][0][2].strip(),ancestors)
            focus = w.git('show',shared+':memory-bank/activeContext.md',cwd=a).stdout
            self.assertIn('Independent local work continues',focus)
            paths = w.git('ls-tree','-r','--name-only',shared,'--','memory-bank',cwd=a).stdout.splitlines()
            self.assertIn('memory-bank/'+p2,paths); self.assertNotIn('memory-bank/'+p1,paths)
            self.assertNotIn('memory-bank/'+child,paths)
            for repo in (a,b,c,sibling): self.assertEqual(self.protected(w,repo),before[str(repo)])
            self.event('cross-developer-conflict',detecting_developer=dev_a,other_developer=dev_b,
                       clarification_recommended=True,messages_sent=False,independent_imports_and_publication=True,
                       disputed_memory_and_dependencies_held=True,actual_authority=landed['witnessed_commit'])


if __name__=='__main__':unittest.main()
