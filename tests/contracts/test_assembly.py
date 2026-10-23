"""Whole sources, diverse types, atomic disputes and exact context accounting."""
import copy
import hashlib
import json
from pathlib import Path
import unittest
import uuid

from world import World
from contracts import test_comparison as fixtures


class AssemblyContract(unittest.TestCase):
    call=fixtures.ComparisonContract.call
    good=fixtures.ComparisonContract.good
    save=fixtures.ComparisonContract.save
    fixture=fixtures.ComparisonContract.fixture
    state=fixtures.ComparisonContract.state
    add=fixtures.ComparisonContract.add

    def request(self,**updates):
        kinds=['semantic-fact','user-feedback','project-decision','episodic-attempt','result','reference','procedural-candidate']
        value={'recall':{'project_id':self.pid,'query':'','ids':[],'paths':[],'context':{},'kinds':kinds,
                        'minimum_trust':'reviewed','limit':64,'include_context':False},
               'max_bytes':65536,'token_budget':65536,'conflicts':[]}
        value.update(updates);return value

    def preview(self,w,repo,request):
        return self.call(w,'assemble_context.py','preview','--repo',repo,'--request',self.save(w,request))

    def counter(self,preview):
        # A real, explicitly disposable harness tokenizer: one Unicode codepoint
        # per token. This is not a claimed count for any commercial model.
        return {'schema':'agent-memory/token-count-v1','source_kind':'harness-tokenizer',
                'tokenizer':'disposable-codepoint-tokenizer-v1','context_sha256':preview['context_sha256'],
                'tokens':len(preview['context_text'])}

    def check(self,w,repo,request,counter):
        return self.call(w,'assemble_context.py','check','--repo',repo,'--request',self.save(w,request),
                         '--counter',self.save(w,counter,'counter.json'))

    def refs(self,bank,*paths):
        return [{'path':path,'sha256':hashlib.sha256((bank/path).read_bytes()).hexdigest()} for path in paths]

    def test_diverse_whole_units_and_exact_duplicates_preserve_every_source_origin_and_condition(self):
        with World() as w:
            repo,bank=self.fixture(w)
            for n in range(5):self.add(bank,'a-fact-'+str(n),body='# Useful fact '+str(n)+'\n\nDistinct durable knowledge.\n')
            first,p1=self.add(bank,'b-duplicate-one',body='# Exact duplicate\n\nKeep the same conditions.\n')
            second,p2=self.add(bank,'b-duplicate-two',body='# Exact duplicate\n\nKeep the same conditions.\n')
            second['origin']['developer-id']=str(uuid.uuid4());(bank/p2).write_text(fixtures.render(second,'# Exact duplicate\n\nKeep the same conditions.\n'))
            self.add(bank,'z-decision',body='# Confirmed decision\n\nUse the approved project constraint.\n',kind='project-decision')
            self.add(bank,'zz-feedback',body='# User correction\n\nPreserve the operator correction.\n',kind='user-feedback')
            before=self.state(w,repo,bank);request=self.request();r,all_rows=self.preview(w,repo,request);self.assertEqual(r.returncode,0,all_rows)
            payload=json.loads(all_rows['context_text']);units=payload['units']
            duplicate=next(unit for unit in units if len(unit['sources'])==2)
            self.assertEqual({s['record']['id'] for s in duplicate['sources']},{first['id'],second['id']})
            self.assertEqual({s['record']['origin']['developer-id'] for s in duplicate['sources']},{self.actor,second['origin']['developer-id']})
            self.assertEqual({s['lifecycle'] for s in duplicate['sources']},{'active'})
            self.assertEqual({s['trust'] for s in duplicate['sources']},{'reviewed'})
            small=copy.deepcopy(payload);small['units']=units[:3];small['omitted_units_for_budget']=len(units)-3
            request['max_bytes']=len((json.dumps(small,sort_keys=True,ensure_ascii=False,indent=2)+'\n').encode())
            r,limited=self.preview(w,repo,request);self.assertEqual(r.returncode,0,limited)
            selected=json.loads(limited['context_text'])['units']
            self.assertEqual({unit['memory_type'] for unit in selected},{'semantic-fact','project-decision','user-feedback'})
            self.assertLessEqual(limited['context_bytes'],request['max_bytes']);self.assertGreater(limited['omitted_units_for_budget'],0)
            self.assertEqual(before,self.state(w,repo,bank));self.assertFalse(limited['usable_context'])

    def test_harness_counter_is_bound_to_exact_current_text_and_token_cap(self):
        with World() as w:
            repo,bank=self.fixture(w);self.add(bank,'unicode',body='# Café\n\nA scoped café observation with 🧠 as inert data.\n')
            request=self.request();r,preview=self.preview(w,repo,request);self.assertEqual(r.returncode,0,preview)
            self.assertEqual(preview['context_bytes'],len(preview['context_text'].encode()))
            self.assertFalse(preview['token_budget_verified']);self.assertFalse(preview['usable_context'])
            counter=self.counter(preview);before=self.state(w,repo,bank)
            r,checked=self.check(w,repo,request,counter);self.assertEqual(r.returncode,0,checked);self.assertTrue(checked['usable_context'])
            self.assertEqual(checked['context_text'],preview['context_text']);self.assertFalse(checked['token_counter_assertions_verified'])
            request['token_budget']=1;r,over=self.check(w,repo,request,counter)
            self.assertEqual(r.returncode,0,over);self.assertFalse(over['usable_context']);self.assertIsNone(over['context_text'])
            request['token_budget']=65536
            for field,value in [('tokens',True),('tokens',0),('context_sha256','0'*64)]:
                bad=copy.deepcopy(counter);bad[field]=value;self.assertEqual(self.check(w,repo,request,bad)[0].returncode,1)
            self.assertEqual(before,self.state(w,repo,bank))
            self.add(bank,'arrived',body='# New current knowledge\n\nThe context is different now.\n')
            before=self.state(w,repo,bank);self.assertEqual(self.check(w,repo,request,counter)[0].returncode,1)
            self.assertEqual(before,self.state(w,repo,bank))

    def test_disputed_evidence_outside_query_stays_together_and_routes_to_detecting_developer(self):
        with World() as w:
            repo,bank=self.fixture(w);a,p1=self.add(bank,'yellow',body='# Wall paint\n\nThe wall should be yellow.\n')
            b,p2=self.add(bank,'blue',body='# Colour choice\n\nThe required colour is blue.\n');other=str(uuid.uuid4())
            b['origin']['developer-id']=other;(bank/p2).write_text(fixtures.render(b,'# Colour choice\n\nThe required colour is blue.\n'))
            request=self.request(conflicts=[{'sources':self.refs(bank,p1,p2),'rationale':'The same current wall has incompatible confirmed colour claims.'}])
            request['recall']['query']='wall yellow';before=self.state(w,repo,bank)
            r,v=self.preview(w,repo,request);self.assertEqual(r.returncode,0,v)
            group=json.loads(v['context_text'])['units'][0]
            self.assertEqual(group['kind'],'unresolved-conflict');self.assertFalse(group['use_as_settled_guidance'])
            self.assertEqual({s['path'] for s in group['sources']},{p1,p2});self.assertTrue(v['needs_clarification'])
            self.assertEqual(v['detecting_developer_id'],self.actor);self.assertEqual(v['other_developer_ids'],[other]);self.assertFalse(v['messages_sent'])
            request['max_bytes']=v['context_bytes']-1;r,small=self.preview(w,repo,request)
            self.assertEqual(r.returncode,1);self.assertNotIn('context_text',small);self.assertEqual(before,self.state(w,repo,bank))

    def test_conflict_reference_cannot_bypass_scope_visibility_lifecycle_or_source_hash(self):
        for change in ('other-branch','hidden','revoked','stale-hash','unsafe'):
            with self.subTest(change=change),World() as w:
                repo,bank=self.fixture(w);a,p1=self.add(bank,'one',body='# Wall\n\nA yellow wall.\n')
                b,p2=self.add(bank,'two',body='# Colour\n\nA blue wall.\n')
                refs=self.refs(bank,p1,p2)
                if change=='other-branch':b['conditions']={'branch':'another-branch'}
                elif change=='hidden':b['visibility']='hidden'
                elif change=='revoked':b['status']='revoked';b['state-reason']='The old claim was revoked.'
                if change in ('other-branch','hidden','revoked'):(bank/p2).write_text(fixtures.render(b,'# Colour\n\nA blue wall.\n'))
                if change=='revoked':
                    router=bank/'MEMORY.md';router.write_text(router.read_text().replace(
                        '| ['+p2+'](<'+p2+'>) | Reviewed fixture learning | repository | active |',
                        '| ['+p2+'](<'+p2+'>) | Reviewed fixture learning | repository | revoked |'))
                if change=='unsafe':(bank/p2).write_text(fixtures.render(b,'# Colour\n\npassword=ASSEMBLY_WITHHELD_SECRET\n'))
                if change!='stale-hash':refs=self.refs(bank,p1,p2)
                else:refs[1]['sha256']='0'*64
                request=self.request(conflicts=[{'sources':refs,'rationale':'Review the incompatible wall assertions without choosing a winner.'}]);before=self.state(w,repo,bank)
                r,v=self.preview(w,repo,request);self.assertEqual(r.returncode,1,v);self.assertNotIn('context_text',v)
                self.assertNotIn('ASSEMBLY_WITHHELD_SECRET',r.stdout+r.stderr);self.assertEqual(before,self.state(w,repo,bank))

    def test_overlapping_conflicts_form_one_indivisible_group(self):
        with World() as w:
            repo,bank=self.fixture(w);paths=[]
            for name in ('yellow','blue','green'):_,path=self.add(bank,name,body='# Wall colour\n\nThe wall is '+name+'.\n');paths.append(path)
            request=self.request(conflicts=[{'sources':self.refs(bank,*paths[:2]),'rationale':'Two claims disagree.'},
                                           {'sources':self.refs(bank,*paths[1:]),'rationale':'The overlapping third claim also disagrees.'}])
            r,v=self.preview(w,repo,request);self.assertEqual(r.returncode,0,v)
            units=json.loads(v['context_text'])['units'];self.assertEqual(len(units),1);self.assertEqual(len(units[0]['sources']),3)
            self.assertEqual(v['unresolved_conflict_groups'],1)
            request['max_bytes']=v['context_bytes']-1;self.assertEqual(self.preview(w,repo,request)[0].returncode,1)

    def test_same_body_with_different_conditions_stays_distinct_and_orientation_is_not_admitted(self):
        with World() as w:
            repo,bank=self.fixture(w);body='# Same wording\n\nRetain each original applicability.\n'
            self.add(bank,'general',body=body);self.add(bank,'branch-only',body=body,scope='branch:main')
            request=self.request();request['recall']['include_context']=True
            r,v=self.preview(w,repo,request);self.assertEqual(r.returncode,0,v);units=json.loads(v['context_text'])['units']
            typed=[u for u in units if u['kind']=='guidance'];self.assertEqual(len(typed),2)
            self.assertEqual({u['sources'][0]['record']['scope'] for u in typed},{'repository','branch:main'})
            orientation=next(u for u in units if u['kind']=='orientation-only')
            self.assertIsNone(orientation['sources'][0]['trust']);self.assertFalse(orientation['sources'][0]['admission_verified'])

    def test_small_budget_reports_omission_and_unknown_runtime_markers_never_produce_context(self):
        with World() as w:
            repo,bank=self.fixture(w);self.add(bank,'large',body='# Large useful lesson\n\n'+'x'*8000+'\n')
            request=self.request(max_bytes=1500);r,v=self.preview(w,repo,request);self.assertEqual(r.returncode,0,v)
            self.assertEqual(v['selected_paths'],[]);self.assertEqual(v['omitted_units_for_budget'],1)
            self.assertLessEqual(v['context_bytes'],request['max_bytes'])
            for name in ('.agent-memory.lock','.agent-memory-transaction.md'):
                marker=bank/name;marker.write_text('Unknown ownership; preserve.\n');before=self.state(w,repo,bank)
                r,v=self.preview(w,repo,request);self.assertEqual(r.returncode,1);self.assertNotIn('context_text',v)
                self.assertEqual(before,self.state(w,repo,bank));marker.unlink()
