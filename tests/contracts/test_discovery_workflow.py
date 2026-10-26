"""Public bounded map/read flow, qualification privacy and actionable failures."""
import hashlib
import json
import unittest

from world import World
from contracts import test_discovery as fixtures


class DiscoveryWorkflowContract(unittest.TestCase):
    fixture=fixtures.DiscoveryContract.fixture
    call=fixtures.DiscoveryContract.call
    good=fixtures.DiscoveryContract.good
    save=fixtures.DiscoveryContract.save
    recall=fixtures.DiscoveryContract.recall

    def start(self,w,repo,intent='recall'):
        return self.good(w,'discover_memory.py','start','--repo',repo,'--intent',intent,
            '--task','Determine how often a temporary network failure may be attempted again.')

    def read(self,w,repo,trace,request):
        return self.call(w,'discover_memory.py','read','--repo',repo,'--input',self.save(w,trace,'trace.json'),
            '--request',self.save(w,request),'--rationale','Selected plausible request-failure sources by meaning from their actual parent maps.')

    def test_topic_navigation_qualifies_actual_sources_and_returns_only_safe_recall_state(self):
        with World() as w:
            repo,bank=self.fixture(w);meta,path=self.h.add(bank,'attempt-policy')
            index='topics/network.md'
            spec={'path':index,'project-id':self.h.pid,'updated':'2026-10-25T00:00:00Z',
                'routes':[{'path':path,'purpose':'Bounded repetitions after temporary request failures','scope':'repository',
                    'lifecycle':'active','read-when':'When deciding how often a request can be repeated'}]}
            content=self.good(w,'bank_layout.py','render-index','--input',self.save(w,spec))['content']
            (bank/index).write_text(content)
            root=bank/'MEMORY.md';root.write_text(root.read_text().replace(path,index))
            before=self.h.state(w,repo,bank)
            start=self.start(w,repo)
            self.assertFalse(start['full_bank_format_validated']);self.assertFalse(start['usable_as_current_guidance'])
            node=self.good(w,'discover_memory.py','node','--repo',repo,'--input',self.save(w,start,'start.json'),'--path',index)
            request=self.recall(node['discovery'],path);request.pop('discovery')
            result,value=self.read(w,repo,node['discovery'],request)
            self.assertEqual(result.returncode,0,value)
            self.assertEqual(value['qualification']['results'][0]['record'],meta)
            self.assertTrue(value['usable_as_current_guidance']);self.assertFalse(value['semantic_completeness_verified'])
            self.assertEqual(value['discovery']['sources'],[]);self.assertFalse(value['discovery']['complete'])
            self.assertNotIn('request',value);self.assertEqual(before,self.h.state(w,repo,bank))
            warm=self.start(w,repo);self.assertTrue(warm['optional_indexes_used'])

    def test_withheld_source_hashes_and_labels_are_not_echoed_by_qualified_read(self):
        with World() as w:
            repo,bank=self.fixture(w)
            hidden,path=self.h.add(bank,'hidden-claim',visibility='hidden',body='# Private assertion\n\nWithheld material stays withheld.\n')
            start=self.start(w,repo);request=self.recall(start['discovery'],path);request.pop('discovery')
            r,value=self.read(w,repo,start['discovery'],request);self.assertEqual(r.returncode,0,value)
            self.assertEqual(value['qualification']['results'],[])
            rendered=json.dumps(value)
            self.assertNotIn('Withheld material stays withheld',rendered)
            self.assertNotIn(hashlib.sha256((bank/path).read_bytes()).hexdigest(),rendered)
            self.assertEqual(value['discovery']['sources'],[])

    def test_changed_map_explains_the_problem_and_restart_repairs_navigation(self):
        with World() as w:
            repo,bank=self.fixture(w);_,path=self.h.add(bank,'attempt-policy')
            start=self.start(w,repo);request=self.recall(start['discovery'],path);request.pop('discovery')
            (bank/'MEMORY.md').write_text((bank/'MEMORY.md').read_text().replace('# Project memory','# Project memory\n\nNew map orientation.'))
            before=self.h.state(w,repo,bank)
            r,failed=self.read(w,repo,start['discovery'],request)
            self.assertEqual(r.returncode,1);self.assertEqual(failed['failure_code'],'source-context-changed')
            self.assertIn('discover start',failed['repair']);self.assertEqual(before,self.h.state(w,repo,bank))
            restarted=self.start(w,repo);r,done=self.read(w,repo,restarted['discovery'],request)
            self.assertEqual(r.returncode,0,done);self.assertEqual(len(done['qualification']['results']),1)

    def test_limited_start_cannot_certify_a_bank_with_a_broken_unrelated_record(self):
        with World() as w:
            repo,bank=self.fixture(w);_,path=self.h.add(bank,'attempt-policy');_,bad=self.h.add(bank,'unrelated')
            original=(bank/bad).read_bytes();(bank/bad).write_text('# Broken optional record\n')
            start=self.start(w,repo);self.assertFalse(start['full_bank_format_validated'])
            request=self.recall(start['discovery'],path);request.pop('discovery')
            r,failed=self.read(w,repo,start['discovery'],request)
            self.assertEqual(r.returncode,1);self.assertEqual(failed['failure_component'],'canonical-qualification')
            self.assertIn('am doctor',failed['repair']);self.assertNotIn('qualification',failed)
            (bank/bad).write_bytes(original)
            restarted=self.start(w,repo);r,done=self.read(w,repo,restarted['discovery'],request)
            self.assertEqual(r.returncode,0,done);self.assertEqual(len(done['qualification']['results']),1)

    def test_comparison_read_retains_discovered_matches_despite_unrelated_literal_searches(self):
        with World() as w:
            repo,bank=self.fixture(w);_,path=self.h.add(bank,'attempt-policy')
            captured=self.h.capture(w,repo,self.h.request())['capture']
            start=self.start(w,repo,'comparison')
            request={'project_id':self.h.pid,'candidate':captured['request']['candidate'],
                'searches':['unrelatedquasar','unrelatednebula'],'read_paths':[path]}
            r,value=self.read(w,repo,start['discovery'],request)
            self.assertEqual(r.returncode,0,value)
            self.assertEqual([row['path'] for row in value['inspection']['matches']],[path])
            self.assertEqual(value['request']['discovery'],value['discovery'])
            self.assertTrue(value['discovery']['complete']);self.assertFalse(value['usable_as_current_guidance'])

    def test_comparison_read_refuses_hidden_source_without_returning_request_or_source_trace(self):
        with World() as w:
            repo,bank=self.fixture(w);_,path=self.h.add(bank,'hidden-claim',visibility='hidden')
            captured=self.h.capture(w,repo,self.h.request())['capture'];start=self.start(w,repo,'comparison')
            request={'project_id':self.h.pid,'candidate':captured['request']['candidate'],
                'searches':['unrelatedquasar','unrelatednebula'],'read_paths':[path]}
            r,value=self.read(w,repo,start['discovery'],request)
            self.assertEqual(r.returncode,1,value)
            self.assertNotIn('request',value);self.assertNotIn('discovery',value)
            self.assertNotIn(hashlib.sha256((bank/path).read_bytes()).hexdigest(),json.dumps(value))
