"""Mechanical binding for meaning-based harness navigation, not a scripted semantic judge."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import unittest

from world import World
from contracts import test_admission as admission
from contracts import test_comparison as comparison


class DiscoveryContract(unittest.TestCase):
    def fixture(self,w):
        self.h=admission.AdmissionContract('test_user_correction_admits_small_record_and_history_automatically_without_git_changes')
        repo,bank=self.h.fixture(w);self.pack=self.h.pack
        return repo,bank

    def call(self,w,helper,*args):return self.h.call(w,helper,*args)
    def good(self,w,helper,*args):
        r,value=self.call(w,helper,*args);self.assertEqual(r.returncode,0,(value,r.stderr));return value
    def save(self,w,value,name='request.json'):return self.h.save(w,value,name)

    def trace(self,w,repo,bank,paths,intent='recall',nodes=()):
        value=self.good(w,'discover_memory.py','start','--repo',repo,'--intent',intent,
            '--task','Determine the permitted number of attempts after a temporary failure.')
        trace=value['discovery'];self.assertFalse(trace['complete']);self.assertFalse(value['usable_as_current_guidance'])
        trace['nodes'] += [{'path':path,'sha256':hashlib.sha256((bank/path).read_bytes()).hexdigest()} for path in nodes]
        trace['sources']=[{'path':path,'sha256':hashlib.sha256((bank/path).read_bytes()).hexdigest()} for path in paths]
        trace.update(complete=True,rationale='Selected plausible meaning-bearing routes; inspect all actual source conditions and evidence.')
        return trace

    def recall(self,trace,path):
        return {'project_id':trace['binding']['project_id'],'query':'How often may a temporary failure be attempted again?',
            'ids':[],'paths':[path],'context':{},'kinds':['semantic-fact'],'minimum_trust':'reviewed',
            'limit':8,'include_context':False,'discovery':trace}

    def compared(self,w,repo,captured,trace):
        request={'project_id':self.h.pid,'candidate':captured['request']['candidate'],
            'searches':['retry policy','attempt cap'],'read_paths':[row['path'] for row in trace['sources']],'discovery':trace}
        inspected=self.good(w,'compare_learning.py','inspect','--repo',repo,'--request',self.save(w,request))
        judgment={'observation':inspected['observation'],'decisions':[{'path':row['path'],'classification':'distinct-knowledge',
            'rationale':'This mechanical fixture uses an independently authored unrelated source.'} for row in inspected['matches']],
            'rationale':'Actual semantic interpretation belongs to the harness; this contract exercises version binding.'}
        proof=self.good(w,'compare_learning.py','review','--repo',repo,'--request',self.save(w,request),
            '--judgment',self.save(w,judgment,'judgment.json'))['proof']
        return request,proof

    def navigation(self):
        return {'learning':{'parent':'topics/request-failures.md','purpose':'Bounded retries after temporary request failures',
                'read-when':'When deciding how often a transient request may be repeated'},
            'history':{'parent':'topics/provenance.md','purpose':'Source and admission evidence for the request retry lesson',
                'read-when':'When auditing who observed this retry policy and why it was admitted'},
            'indexes':[{'path':'topics/request-failures.md','parent':'MEMORY.md','scope':'repository',
                'purpose':'Request failures, bounded retries and outcome preservation','read-when':'When recovering from a failed network request'},
                {'path':'topics/provenance.md','parent':'MEMORY.md','scope':'repository',
                'purpose':'Learning provenance and admission events','read-when':'When tracing the evidence and origins of stored lessons'}]}

    def preview(self,w,repo,captured,request,proof,navigation):
        return self.call(w,'admit_learning.py','preview','--repo',repo,'--capture',self.save(w,captured,'capture.json'),
            '--request',self.save(w,request),'--proof',self.save(w,proof,'proof.json'),
            '--navigation',self.save(w,navigation,'navigation.json'))

    def test_exact_source_after_semantic_navigation_has_no_literal_overlap_requirement(self):
        with World() as w:
            repo,bank=self.fixture(w);meta,path=self.h.add(bank,'retained-lesson',body='# Retry policy\n\nRetry transient requests no more than three times.\n')
            before=self.h.state(w,repo,bank);trace=self.trace(w,repo,bank,[path])
            value=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,self.recall(trace,path)))
            self.assertEqual([row['record']['id'] for row in value['results']],[meta['id']])
            self.assertEqual(value['discovery']['semantic_interpreter'],'invoking-harness')
            self.assertFalse(value['discovery']['semantic_completeness_verified']);self.assertEqual(before,self.h.state(w,repo,bank))

    def test_changed_unvisited_source_rejects_old_navigation_before_recall_or_comparison(self):
        with World() as w:
            repo,bank=self.fixture(w);meta,path=self.h.add(bank,'retained-lesson')
            trace=self.trace(w,repo,bank,[path]);comparison_trace=self.trace(w,repo,bank,[path],intent='comparison')
            original_root=(bank/'MEMORY.md').read_bytes();(bank/'projectbrief.md').write_text('# Project\n\nChanged without changing a visited router.\n')
            before=self.h.state(w,repo,bank)
            r,value=self.call(w,'retrieve.py','--repo',repo,'--request',self.save(w,self.recall(trace,path)))
            self.assertEqual(r.returncode,1);self.assertEqual(original_root,(bank/'MEMORY.md').read_bytes())
            request=self.h.request();captured=self.h.capture(w,repo,request)['capture']
            compared={'project_id':self.h.pid,'candidate':captured['request']['candidate'],
                'searches':['retry policy','attempt cap'],'read_paths':[path],'discovery':comparison_trace}
            r,value=self.call(w,'compare_learning.py','inspect','--repo',repo,'--request',self.save(w,compared))
            self.assertEqual(r.returncode,1);self.assertEqual(before,self.h.state(w,repo,bank))

    def test_meaningful_admission_creates_native_topics_preserves_payloads_and_replays(self):
        with World() as w:
            repo,bank=self.fixture(w);before=self.h.state(w,repo,bank)
            (bank/'MEMORY.md').write_text((bank/'MEMORY.md').read_text().replace('# Project memory','# Project memory\n\nRetain this custom map introduction.'))
            initial={p.relative_to(bank).as_posix():p.read_bytes() for p in bank.rglob('*.md')}
            request=self.h.request();captured=self.h.capture(w,repo,request)['capture'];trace=self.trace(w,repo,bank,[],intent='comparison')
            compared,proof=self.compared(w,repo,captured,trace)
            r,value=self.preview(w,repo,captured,compared,proof,self.navigation());self.assertEqual(r.returncode,0,(value,r.stderr))
            proposal=value['proposal'];r,done=self.h.apply(w,proposal);self.assertEqual(r.returncode,0,done);self.assertTrue(done['admission_verified'])
            learning='topics/'+request['candidate']['metadata']['id']+'.md'
            self.assertIn(learning,(bank/'topics/request-failures.md').read_text())
            self.assertIn('transient request may be repeated',(bank/'topics/request-failures.md').read_text())
            self.assertIn('Retain this custom map introduction.',(bank/'MEMORY.md').read_text())
            self.assertNotIn(learning,(bank/'MEMORY.md').read_text())
            for path,raw in initial.items():
                if path!='MEMORY.md':self.assertEqual((bank/path).read_bytes(),raw)
            self.good(w,'bank_contract.py','validate','--repo',repo)
            self.assertEqual(before[1:],self.h.state(w,repo,bank)[1:])
            r,replayed=self.h.apply(w,proposal);self.assertEqual(r.returncode,0,replayed);self.assertTrue(replayed['already_applied'])

    def test_navigation_without_trace_collision_or_substituted_proposal_refuses(self):
        with World() as w:
            repo,bank=self.fixture(w);captured=self.h.capture(w,repo,self.h.request())['capture']
            legacy,proof=self.h.compared(w,repo,captured);before=self.h.state(w,repo,bank)
            r,value=self.preview(w,repo,captured,legacy,proof,self.navigation());self.assertEqual(r.returncode,1)
            trace=self.trace(w,repo,bank,[],intent='comparison');request,proof=self.compared(w,repo,captured,trace)
            navigation=self.navigation();navigation['indexes'][0]['path']='projectbrief.md'
            r,value=self.preview(w,repo,captured,request,proof,navigation);self.assertEqual(r.returncode,1)
            r,value=self.preview(w,repo,captured,request,proof,self.navigation());self.assertEqual(r.returncode,0,value)
            altered=copy.deepcopy(value['proposal']);altered['admission_evidence']['navigation']['learning']['purpose']='An unreviewed different caption'
            r,value=self.h.apply(w,altered);self.assertEqual(r.returncode,1);self.assertEqual(before,self.h.state(w,repo,bank))

    def test_discovered_equivalent_cannot_be_omitted_from_comparison_to_admit_duplicate(self):
        with World() as w:
            repo,bank=self.fixture(w);meta,path=self.h.add(bank,'retained-lesson')
            captured=self.h.capture(w,repo,self.h.request())['capture']
            trace=self.trace(w,repo,bank,[path],intent='comparison')
            request={'project_id':self.h.pid,'candidate':captured['request']['candidate'],
                'searches':['unrelatedquasar','unrelatednebula'],'read_paths':[],'discovery':trace}
            before=self.h.state(w,repo,bank)
            view=self.good(w,'compare_learning.py','inspect','--repo',repo,'--request',self.save(w,request))
            self.assertEqual([row['path'] for row in view['matches']],[path])
            judgment={'observation':view['observation'],'decisions':[{'path':path,'classification':'equivalent',
                'rationale':'The fixture retains one bounded retry cap with the same actual conditions.'}],
                'rationale':'All discovered plausible learning must enter the duplicate decision despite unrelated queries.'}
            proof=self.good(w,'compare_learning.py','review','--repo',repo,'--request',self.save(w,request),
                '--judgment',self.save(w,judgment,'judgment.json'))['proof']
            r,value=self.preview(w,repo,captured,request,proof,self.navigation())
            self.assertEqual(r.returncode,0,value);self.assertEqual(value['disposition'],'update-existing')
            self.assertIsNone(value['proposal']);self.assertEqual(before,self.h.state(w,repo,bank))

    def test_traced_empty_selection_never_falls_back_to_literal_or_full_inventory(self):
        with World() as w:
            repo,bank=self.fixture(w);meta,path=self.h.add(bank,'retained-lesson')
            trace=self.trace(w,repo,bank,[]);request=self.recall(trace,path);request['paths']=[]
            for query in ('','attempt'):
                request['query']=query
                value=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
                self.assertEqual(value['results'],[])
            # With observed sources and no explicit selectors, use those exact
            # sources rather than interpreting a literal query as a new frontier.
            trace=self.trace(w,repo,bank,[path]);request['discovery']=trace;request['query']='unrelatedquasar'
            value=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertEqual([row['path'] for row in value['results']],[path])
            request.pop('discovery');request['query']=''
            value=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertEqual([row['path'] for row in value['results']],[path])

    def test_traced_stable_id_resolves_current_discovered_version_without_historical_trace(self):
        with World() as w:
            repo,bank=self.fixture(w);old,path=self.h.add(bank,'older-version',status='superseded',
                **{'state-reason':'The actual fixture correction retains this historical version.'})
            new,new_path=self.h.add(bank,'current-version',id=old['id'],version=2,
                body='# Revised retry policy\n\nThe corrected fixture allows four attempts.\n',
                corrects=[{'id':old['id'],'version':old['version']}])
            trace=self.trace(w,repo,bank,[new_path]);request=self.recall(trace,new_path)
            request['paths']=[];request['ids']=[old['id']]
            before=self.h.state(w,repo,bank)
            value=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertEqual([(row['path'],row['record']['version']) for row in value['results']],[(new_path,2)])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_source_version_uncertainty_and_missing_parent_never_become_novelty(self):
        with World() as w:
            repo,bank=self.fixture(w);meta,path=self.h.add(bank,'retained-lesson');trace=self.trace(w,repo,bank,[path])
            for change in ('source','uncertain','root'):
                edited=copy.deepcopy(trace)
                if change=='source':edited['sources'][0]['sha256']='0'*64
                elif change=='uncertain':edited['complete']=False
                else:edited['nodes']=[]
                before=self.h.state(w,repo,bank)
                r,value=self.call(w,'retrieve.py','--repo',repo,'--request',self.save(w,self.recall(edited,path)))
                self.assertEqual(r.returncode,1);self.assertEqual(before,self.h.state(w,repo,bank))


if __name__=='__main__':unittest.main()
