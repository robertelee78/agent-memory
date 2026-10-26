"""The explicit large-bank refresh entry point uses its versioned review path."""
from world import World
from refresh_fixtures import RefreshFixture
from contracts import test_semantic_sharing as semantic_fixtures


class RefreshVersionTwoContract(RefreshFixture):
    def test_legacy_semantic_inspect_and_check_keep_their_required_input_contract(self):
        with World() as w:
            h=semantic_fixtures.SemanticSharingContract();repo,bank,peer,base=h.fixture(w)
            view=h.good(w,'inspect','--repo',repo)
            self.assertEqual(view['schema'],'agent-memory/semantic-sharing-inspection-v1')
            result,reviewed=h.review(w,repo,h.judgment(view));self.assertEqual(result.returncode,0,reviewed)
            checked=h.good(w,'check','--repo',repo,'--input',h.save(w,reviewed['proof']))
            self.assertTrue(checked['source_observation_current'])

    def test_explicit_v2_inspection_uses_changed_learning_frontier_and_retains_the_actual_base(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            before=self.state(w,repo,bank)
            v2=self.good(w,'refresh_memory.py','inspect','--v2','--repo',repo,'--base',base)
            self.assertEqual(v2['inspection']['schema'],'agent-memory/semantic-sharing-inspection-v2')
            self.assertEqual(v2['inspection']['request']['base'],base)
            self.assertEqual(v2['inspection']['binding']['base'],base)
            self.assertEqual(v2['baseline_source'],'explicit')
            self.assertFalse(v2['refresh_applied']);self.assertFalse(v2['canonical_writes_performed'])
            legacy=self.good(w,'refresh_memory.py','inspect','--repo',repo,'--base',base)
            self.assertEqual(legacy['inspection']['schema'],'agent-memory/semantic-sharing-inspection-v1')
            self.assertEqual(before,self.state(w,repo,bank))
