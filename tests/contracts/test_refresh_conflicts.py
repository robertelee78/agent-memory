"""Disputed learning stays pending while independent imports and code work continue."""
import unittest

from world import World
from refresh_fixtures import RefreshFixture
from contracts import test_semantic_sharing as semantic_helpers
from contracts import test_retrieval as retrieval_helpers


class RefreshConflictsContract(RefreshFixture):
    def test_conflict_and_real_dependency_are_preserved_but_not_returned_as_current_guidance(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); helper = semantic_helpers.SemanticSharingContract()
            yellow, local = helper.add(bank, 'yellow', '# Wall\n\nThe room wall should be yellow.\n')
            dependent, child = helper.add(bank, 'paint-order', '# Order\n\nThis order depends on the local wall color.\n')
            blue, landed = helper.add(peer/'memory-bank', 'blue', '# Wall\n\nThe same room wall should be blue.\n')
            (peer/'memory-bank/projectbrief.md').write_text('# Project\n\nIndependent landed orientation still arrives.\n')
            w.git('add', 'memory-bank', cwd=peer); w.git('commit', '-m', 'Actual contradictory and independent shared learning', cwd=peer)
            w.git('push', 'origin', 'HEAD:main', cwd=peer)
            before = self.state(w, repo, bank)
            def decide(a, b):
                return 'contradiction' if {a['path'], b['path']} == {local, landed} else 'related'
            recall = retrieval_helpers.RetrievalContract().request(yellow['project-id'], query='', include_context=True)
            proposal = self.prepared_refresh(w, repo, recall=recall, decide=decide, dependencies={child:[local]})
            self.assertEqual(len(self.last_preparation['conflicts']), 1)
            claims = self.last_preparation['conflicts'][0]['claims']
            self.assertEqual({c['path'] for c in claims}, {local, landed})
            done = self.apply_refresh(w, proposal)
            self.assertEqual(done['sharing_state'], 'refreshed-with-conflicts'); self.assertTrue(done['conflicts_pending'])
            self.assertTrue((bank/local).exists()); self.assertTrue((bank/child).exists()); self.assertFalse((bank/landed).exists())
            self.assertIn('Independent landed', (bank/'projectbrief.md').read_text())
            self.assertFalse({r['path'] for r in done['recall']['results']} & {local, child, landed})
            self.assertGreaterEqual(done['recall']['excluded_counts']['pending-semantic-conflict'], 2)
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_held_claims_do_not_consume_a_small_budget_and_starve_an_independent_import(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); helper = semantic_helpers.SemanticSharingContract()
            yellow, local = helper.add(bank, 'yellow', '# Wall\n\nThe wall should be yellow.\n')
            dependent, child = helper.add(bank, 'paint-order', '# Order\n\nThis depends on the wall decision.\n')
            blue, landed = helper.add(peer/'memory-bank', 'blue', '# Wall\n\nThe same wall should be blue.\n')
            independent, path = helper.add(peer/'memory-bank', 'z-independent', '# Independent\n\nAn independent observation remains usable.\n')
            w.git('add','memory-bank',cwd=peer); w.git('commit','-m','Disputed and independent actual imports',cwd=peer)
            w.git('push','origin','HEAD:main',cwd=peer)
            def decide(a,b):
                return 'contradiction' if {a['path'],b['path']} == {local,landed} else 'related'
            recall = retrieval_helpers.RetrievalContract().request(yellow['project-id'],query='',paths=[local,child,path],limit=1)
            before = self.state(w,repo,bank)
            done = self.apply_refresh(w,self.prepared_refresh(w,repo,recall=recall,decide=decide,dependencies={child:[local]}))
            self.assertTrue(done['conflicts_pending']); self.assertEqual([r['path'] for r in done['recall']['results']],[path])
            self.assertEqual(done['recall']['metrics']['selected'],1)
            self.assertEqual(before[1:],self.state(w,repo,bank)[1:])
