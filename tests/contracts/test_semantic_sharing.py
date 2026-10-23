"""Public source-bound meaning review across actual independent Git clones."""
from datetime import datetime, timezone
import itertools
import hashlib
import json
from pathlib import Path
import sys
import unittest
import uuid

from world import World
from record_fixtures import record_text, project_id
from contracts import test_sharing_recovery as recovery
from contracts import test_comparison as comparison

ROOT=Path(__file__).resolve().parents[2]


class SemanticSharingContract(unittest.TestCase):
    save=recovery.SharingRecoveryContract.save
    state=recovery.SharingRecoveryContract.state

    def fixture(self,w):
        helper=recovery.SharingRecoveryContract();repo,bank,peer,base=helper.fixture(w);self.pack=helper.pack
        return repo,bank,peer,base

    def call(self,w,*args):
        r=w.run([sys.executable,'-I',self.pack/'scripts/reconcile_memory.py',*args],check=False)
        return r,json.loads(r.stdout)

    def good(self,w,*args):
        r,v=self.call(w,*args);self.assertEqual(r.returncode,0,(v,r.stderr));return v

    def judgment(self,inspection,decide=None,**changes):
        rows={r['key']:r for r in inspection['sources']}
        value={'binding':inspection['binding'],'pairs':[
            {'left':a,'right':b,'classification':decide(rows[a],rows[b]) if decide else 'related',
             'reason':'Actual complete source meaning reviewed for this isolated case.'} for a,b in inspection['required_pairs']],
            'dependencies':{},'consolidations':[],'rationale':'Preserve actual source conditions and every origin; no landing-order truth.',
            'recorded_at':datetime.now(timezone.utc).isoformat()}
        value.update(changes);return value

    def review(self,w,repo,value):return self.call(w,'review','--repo',repo,'--input',self.save(w,value))

    def add(self,bank,name,body,*,conditions=None,developer=None):
        text=record_text(body,project_id((bank/'MEMORY.md').read_text()))
        meta,_=comparison.unpack(text)
        if conditions is not None:meta['conditions']=conditions
        if developer:meta['origin']['developer-id']=developer
        path='topics/'+name+'.md';(bank/'topics').mkdir(exist_ok=True)
        (bank/path).write_text(comparison.render(meta,body))
        row='| ['+path+'](<'+path+'>) | Scoped reviewed lesson | repository | active | When conditions match |\n'
        router=bank/'MEMORY.md';router.write_text(router.read_text().replace('\n## Omitted contexts\n',row+'\n## Omitted contexts\n'))
        return meta,path

    def publish(self,w,peer):
        w.git('add','memory-bank',cwd=peer);w.git('commit','-m','Isolated shared reviewed learning',cwd=peer);w.git('push','origin','HEAD:main',cwd=peer)

    def apply(self,w,repo,files):
        r=w.run([sys.executable,'-I',self.pack/'scripts/bank_transactions.py','preview','--repo',repo,'--files',self.save(w,files),'--operation','refresh'])
        return json.loads(w.run([sys.executable,'-I',self.pack/'scripts/bank_transactions.py','apply','--proposal',self.save(w,json.loads(r.stdout),'apply.json')]).stdout)

    def test_textually_clean_cross_file_yellow_blue_conflict_shows_both_origins_and_keeps_independent_work(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);other=str(uuid.uuid4())
            yellow,p1=self.add(bank,'wall-local','# Wall\n\nThe same room wall should be yellow.\n')
            blue,p2=self.add(peer/'memory-bank','wall-landed','# Wall\n\nThe same room wall should be blue.\n',developer=other)
            (bank/'activeContext.md').write_text('# Current focus\n\nIndependent local work may continue.\n')
            (peer/'memory-bank/projectbrief.md').write_text('# Project\n\nIndependent shared project orientation.\n');self.publish(w,peer)
            before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
            def classify(a,b):return 'contradiction' if {a['path'],b['path']}=={p1,p2} else 'related'
            value=self.judgment(view,classify);r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
            self.assertEqual(plan['blocked_paths'],sorted([p1,p2]));self.assertEqual(len(plan['conflicts']),1)
            self.assertIn(other,plan['other_developer_ids']);self.assertTrue(plan['recommend_other_developer_clarification'])
            self.assertFalse(plan['messages_sent']);self.assertTrue(plan['independent_updates_may_continue'])
            self.assertEqual({c['path'] for c in plan['conflicts'][0]['claims']},{p1,p2})
            self.assertIn(p1,plan['refresh_files']);self.assertNotIn(p2,plan['refresh_files'])
            self.assertIn(p2,plan['publish_files']);self.assertNotIn(p1,plan['publish_files'])
            self.assertIn('Independent shared',plan['refresh_files']['projectbrief.md'])
            self.assertIn('Independent local',plan['publish_files']['activeContext.md'])
            self.assertEqual(before,self.state(w,repo,bank))
            proof=self.good(w,'check','--repo',repo,'--input',self.save(w,plan['proof']))
            self.assertTrue(proof['source_observation_current']);self.assertFalse(proof['publication_verified'])
            applied=self.apply(w,repo,plan['refresh_files']);self.assertTrue(applied['verified'])
            self.assertEqual(before[1:],self.state(w,repo,bank)[1:])

    def test_same_file_nonoverlapping_compatible_changes_combine_without_landing_order(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            original='# Project\n\n## Build\nUse the isolated build.\n\n## Review\nReview scoped sources.\n'
            (bank/'projectbrief.md').write_text(original);w.git('reset','--','code.py',cwd=repo);w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Shared independent sections',cwd=repo);w.git('push','origin','HEAD:main',cwd=repo);w.git('add','code.py',cwd=repo)
            w.git('pull','--ff-only',cwd=peer)
            (bank/'projectbrief.md').write_text(original.replace('Use the isolated build.','Use the verified isolated build.'))
            (peer/'memory-bank/projectbrief.md').write_text(original.replace('Review scoped sources.','Review scoped sources and their evidence.'));self.publish(w,peer)
            before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
            value=self.judgment(view,lambda a,b:'compatible' if a['path']==b['path']=='projectbrief.md' else 'related')
            r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
            for files in (plan['refresh_files'],plan['publish_files']):
                self.assertIn('verified isolated build',files['projectbrief.md']);self.assertIn('sources and their evidence',files['projectbrief.md'])
            self.assertEqual(plan['blocked_paths'],[]);self.assertEqual(before,self.state(w,repo,bank))

    def test_differently_worded_duplicate_combines_stable_identity_all_evidence_and_origins(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            first,p1=self.add(bank,'retry-local','# Retry cap\n\nRetry at most three times.\n')
            second,p2=self.add(peer/'memory-bank','retry-peer','# Attempt limit\n\nUse no more than three retries.\n')
            self.publish(w,peer);before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
            selected=[row for row in view['sources'] if row['path'] in {p1,p2}]
            self.assertEqual(len(selected),2);primary=next(row for row in selected if row['path']==p1)
            value=self.judgment(view,lambda a,b:'equivalent' if {a['path'],b['path']}=={p1,p2} else 'related',
                consolidations=[{'sources':[row['key'] for row in selected],'primary':primary['key'],
                    'body':'# Retry cap\n\nRetry at most three times.\n','reason':'Both actual claims establish the same cap under identical conditions.'}])
            r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
            for files in (plan['refresh_files'],plan['publish_files']):
                current,body=comparison.unpack(files[p1]);historical,oldbody=comparison.unpack(files[p2])
                self.assertEqual(current['id'],first['id']);self.assertEqual(current['version'],2)
                self.assertEqual(historical['status'],'superseded');self.assertEqual(historical['origin'],second['origin'])
                old,oldbody=comparison.unpack(files['history/versions/'+first['id']+'/v1.md'])
                self.assertEqual(old['origin'],first['origin']);self.assertIn('Retry at most three',oldbody)
                self.assertIn({'id':second['id'],'version':1},current['derived-from'])
            self.assertEqual(before,self.state(w,repo,bank))

    def test_disjoint_conditions_stay_distinct_unknown_conditions_cannot_claim_disjointness(self):
        for condition in ('known','unknown'):
            with self.subTest(condition=condition),World() as w:
                repo,bank,peer,base=self.fixture(w)
                self.add(bank,'wall-old','# Wall\n\nUse yellow in the older release.\n',conditions={'branch':'older'})
                self.add(peer/'memory-bank','wall-main','# Wall\n\nUse blue on main.\n',conditions={'branch':'main' if condition=='known' else None})
                self.publish(w,peer);before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
                value=self.judgment(view,lambda a,b:'disjoint-conditions' if a['path'].startswith('topics/wall-') and b['path'].startswith('topics/wall-') else 'related')
                r,plan=self.review(w,repo,value)
                if condition=='known':
                    self.assertEqual(r.returncode,0,plan);self.assertIn('topics/wall-old.md',plan['refresh_files']);self.assertIn('topics/wall-main.md',plan['refresh_files'])
                else:self.assertEqual(r.returncode,1,plan)
                self.assertEqual(before,self.state(w,repo,bank))

    def test_new_disputed_context_keeps_omission_explanation_and_independent_plan_valid(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            path='productContext.md';(bank/path).write_text('# Product intent\n\nThe wall should be yellow.\n')
            router=bank/'MEMORY.md';text=''.join(line for line in router.read_text().splitlines(keepends=True) if not line.startswith('- `'+path+'`:'))
            text=text.replace('\n## Omitted contexts\n','| ['+path+'](<'+path+'>) | Product intent | repository | active | Before painting |\n\n## Omitted contexts\n');router.write_text(text)
            _,p2=self.add(peer/'memory-bank','wall-blue','# Wall\n\nThe wall should be blue.\n');self.publish(w,peer)
            before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
            value=self.judgment(view,lambda a,b:'contradiction' if {a['path'],b['path']}=={path,p2} else 'related')
            r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
            self.assertIn(path,plan['refresh_files']);self.assertNotIn(path,plan['publish_files'])
            self.assertIn('- `productContext.md`: No additional product context has been supplied',plan['publish_files']['MEMORY.md'])
            self.assertEqual(before,self.state(w,repo,bank))

    def test_hidden_other_kind_superseder_still_prevents_duplicate_reactivation(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            first,p1=self.add(bank,'old-cap','# Retry cap\n\nUse at most three retries.\n')
            second,p2=self.add(peer/'memory-bank','same-cap','# Same cap\n\nThree retries are the limit.\n')
            invalidator,p3=self.add(peer/'memory-bank','withdrawal','# Withdrawal\n\nThis exact old cap was invalidated.\n')
            invalidator.update(kind='procedural-candidate',visibility='hidden',supersedes=[{'id':first['id'],'version':first['version']}])
            (peer/'memory-bank'/p3).write_text(comparison.render(invalidator,'# Withdrawal\n\nThis exact old cap was invalidated.\n'))
            self.publish(w,peer);view=self.good(w,'inspect','--repo',repo)
            selected=[r for r in view['sources'] if r['path'] in {p1,p2}];primary=next(r for r in selected if r['path']==p1)
            value=self.judgment(view,lambda a,b:'equivalent' if {a['path'],b['path']}=={p1,p2} else 'related',
                consolidations=[{'sources':[r['key'] for r in selected],'primary':primary['key'],
                'body':'# Retry cap\n\nUse at most three retries.\n','reason':'Read both exact claims without reviving their invalidated source.'}])
            before=self.state(w,repo,bank);r,refused=self.review(w,repo,value)
            self.assertEqual(r.returncode,1);self.assertEqual(before,self.state(w,repo,bank))

    def test_explicit_clarifying_decision_corrects_then_consolidates_with_both_origins_and_history(self):
        from contracts import test_admission as admission
        from contracts import test_governance as governance
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            old,p1=self.add(bank,'wall-local','# Wall\n\nThe wall should be yellow.\n')
            landed,p2=self.add(peer/'memory-bank','wall-landed','# Wall\n\nThe wall should be blue.\n')
            self.publish(w,peer)
            scope=json.loads(w.run([sys.executable,'-I',self.pack/'scripts/scope_binding.py','status','--repo',repo]).stdout)
            helper=admission.AdmissionContract();helper.pack=self.pack;helper.pid=scope['project_id'];helper.actor=scope['developer_id']
            request=helper.request();candidate=request['candidate'];candidate['metadata'].update(id=old['id'])
            candidate['metadata']['evidence']=['user:explicit-wall-blue-clarification'];candidate['body']='# Wall\n\nThe wall should be blue.\n'
            captured=helper.capture(w,repo,request)['capture'];compared,proof=helper.compared(w,repo,captured,classification='contradiction')
            gov=governance.GovernanceContract();gov.pack=self.pack;gov.pid=scope['project_id'];gov.actor=scope['developer_id']
            value=gov.request(bank,old,p1,'correct');value['authority'].update(evidence=['user:explicit-wall-blue-clarification'],reason='The current detecting developer explicitly clarified that this wall is blue.')
            value['capture']=captured;value['comparison']={'request':compared,'proof':proof}
            r,prepared=gov.preview(w,repo,value);self.assertEqual(r.returncode,0,prepared)
            r,done=gov.apply(w,prepared['proposal']);self.assertEqual(r.returncode,0,done)
            corrected,body=comparison.unpack((bank/p1).read_text());self.assertEqual(corrected['version'],2)
            self.assertIn('user:explicit-wall-blue-clarification',corrected['evidence'])
            view=self.good(w,'inspect','--repo',repo);selected=[r for r in view['sources'] if r['path'] in {p1,p2}]
            primary=next(r for r in selected if r['path']==p1)
            value=self.judgment(view,lambda a,b:'equivalent' if {a['path'],b['path']}=={p1,p2} else 'related',
                consolidations=[{'sources':[r['key'] for r in selected],'primary':primary['key'],
                    'body':'# Wall\n\nThe wall should be blue.\n','reason':'The actual recorded developer decision resolves the prior yellow claim; retain both blue origins.'}])
            before=self.state(w,repo,bank);r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
            self.assertEqual(plan['conflicts'],[])
            result=plan['refresh_files'];current,_=comparison.unpack(result[p1]);previous,_=comparison.unpack(result[p2])
            self.assertEqual(current['id'],old['id']);self.assertEqual(current['version'],3)
            self.assertIn('user:explicit-wall-blue-clarification',current['evidence'])
            self.assertEqual(previous['origin'],landed['origin']);self.assertEqual(previous['status'],'superseded')
            archived,_=comparison.unpack(result['history/versions/'+old['id']+'/v1.md']);self.assertEqual(archived['origin'],old['origin'])
            self.assertIn('history/versions/'+old['id']+'/v2.md',result);self.assertEqual(before,self.state(w,repo,bank))
            done=self.apply(w,repo,result);self.assertTrue(done['verified']);self.assertEqual(before[1:],self.state(w,repo,bank)[1:])

    def test_missing_pair_changed_sources_and_tampered_output_proof_refuse(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);view=self.good(w,'inspect','--repo',repo);value=self.judgment(view)
            bad=json.loads(json.dumps(value));bad['pairs'].pop();before=self.state(w,repo,bank)
            r,refused=self.review(w,repo,bad);self.assertEqual(r.returncode,1);self.assertEqual(before,self.state(w,repo,bank))
            r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
            plan['proof']['publish_hashes']['projectbrief.md']='0'*64
            r,refused=self.call(w,'check','--repo',repo,'--input',self.save(w,plan['proof']));self.assertEqual(r.returncode,1)
            (bank/'activeContext.md').write_text('# Current focus\n\nA newer local observation.\n');before=self.state(w,repo,bank)
            r,refused=self.review(w,repo,value);self.assertEqual(r.returncode,1);self.assertEqual(before,self.state(w,repo,bank))

    def test_only_explicit_dependent_updates_pause_with_conflict(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            self.add(bank,'wall','# Wall\n\nPaint yellow.\n');self.add(peer/'memory-bank','wall-peer','# Wall\n\nPaint blue.\n')
            (bank/'activeContext.md').write_text('# Current focus\n\nDependent paint purchase.\n')
            (bank/'projectbrief.md').write_text('# Project\n\nIndependent architecture learning.\n');self.publish(w,peer)
            view=self.good(w,'inspect','--repo',repo)
            value=self.judgment(view,lambda a,b:'contradiction' if {a['path'],b['path']}=={'topics/wall.md','topics/wall-peer.md'} else 'related',
                dependencies={'activeContext.md':['topics/wall.md']})
            r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
            self.assertIn('activeContext.md',plan['blocked_paths']);self.assertNotIn('projectbrief.md',plan['blocked_paths'])
            self.assertNotIn('Dependent paint purchase',plan['publish_files']['activeContext.md'])
            self.assertIn('Independent architecture',plan['publish_files']['projectbrief.md'])


if __name__=='__main__':unittest.main()
