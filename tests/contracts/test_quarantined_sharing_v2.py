"""Unsafe record bodies hold their own exchange, preserving useful independent work."""
from datetime import datetime, timezone
import json
import sys
import unittest

from world import World
from publication_fixtures import PublicationFixture
from contracts import test_semantic_sharing as semantic_fixtures
from contracts import test_comparison as comparison_fixtures


class QuarantinedSharingVersionTwoContract(PublicationFixture):
    def private(self,w,name,value):
        path=w.inside(name+'.json');path.write_text(json.dumps(value));return path

    def inspection(self,w,repo,name,request=None):
        path=w.inside(name+'.json');args=['inspect-v2','--repo',repo,'--output',path]
        if request is not None:args+=['--input',self.private(w,name+'-request',request)]
        output=self.good(w,'reconcile_memory.py',*args)
        return path,json.loads(path.read_text()),output

    def versioned_review(self,w,repo):
        _,first,_=self.inspection(w,repo,'first')
        # Authored fixture meanings: peer orientation/local focus are separate
        # subjects from the quarantined isolation claim. Do not manufacture a
        # dependency by selecting that unknown claim as their plausible basis.
        rows={row['key']:row for row in first['sources']}
        request=first['request'];request['selections']={}
        for key in first['changed']:
            row=rows[key];subject=row['target_path'] or row['path']
            request['selections'][key]={'searches':[subject,subject.removesuffix('.md')],
                'paths':[subject],'ids':[],'complete':True,'uncertain':False}
        path,view,stdout=self.inspection(w,repo,'selected',request)
        visible=[stdout,view]
        for page in view['pages']:
            visible.append(self.good(w,'reconcile_memory.py','page-v2','--repo',repo,'--input',path,'--page',str(page['index'])))
        judgment={'schema':'agent-memory/semantic-sharing-judgment-v2','request':request,'binding':view['binding'],
            'manifest_sha256':view['manifest_sha256'],'pages':[page['sha256'] for page in view['pages']],
            'pairs':[{'left':a,'right':b,'classification':'related','reason':'Authored independent fixture subjects; withheld text remains unresolved.'}
                for a,b in view['required_pairs']], 'dependencies':{},'consolidations':[],
            'rationale':'Exercise source isolation; a related label never clears withheld evidence.',
            'recorded_at':datetime.now(timezone.utc).isoformat()}
        result_path=w.inside('reviewed.json')
        visible.append(self.good(w,'reconcile_memory.py','review-v2','--repo',repo,
            '--input',self.private(w,'judgment',judgment),'--output',result_path))
        result=json.loads(result_path.read_text())
        visible.append(self.good(w,'reconcile_memory.py','check','--v2','--repo',repo,
            '--input',self.private(w,'proof',result['proof'])))
        self.assertNotIn(self.marker,json.dumps(visible))
        self.assertNotIn('refresh_files',visible[-1]);self.assertNotIn('publish_files',visible[-1])
        self.assertTrue(view['withheld_sources']);self.assertTrue(result['conflicts_pending'])
        return result

    def test_local_unsafe_body_stays_local_while_safe_peer_learning_is_imported_and_recalled(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);h=semantic_fixtures.SemanticSharingContract()
            self.marker='sk-'+('X'*24)
            _,held=h.add(bank,'isolated-local','# Isolated evidence\n\n'+self.marker+'\n')
            (peer/'memory-bank/projectbrief.md').write_text('# Project\n\nIndependent peer learning remains useful.\n');h.publish(w,peer)
            before=self.state(w,repo,bank);raw=(bank/held).read_bytes()
            result=self.versioned_review(w,repo)
            self.assertIn(held,result['blocked_paths']);self.assertNotIn(held,result['publish_files'])
            self.assertEqual(result['refresh_files'][held].encode(),raw)
            project=result['proof']['judgment']['binding']['project_id']
            request={'proof':result['proof'],'recall':{'project_id':project,'query':'','paths':['projectbrief.md'],
                'ids':[],'context':{},'kinds':[],'minimum_trust':'reviewed','limit':8,'include_context':True}}
            prepared_path=w.inside('refresh.json')
            self.good(w,'refresh_memory.py','prepare','--v2','--repo',repo,
                '--input',self.private(w,'refresh-request',request),'--output',prepared_path)
            output=w.inside('refreshed.json')
            self.good(w,'refresh_memory.py','apply','--v2','--input',prepared_path,'--output',output)
            applied=json.loads(output.read_text())
            self.assertTrue(applied['refresh_applied']);self.assertTrue(applied['conflicts_pending'])
            self.assertTrue(applied['remote_freshness_verified'])
            self.assertIn('Independent peer learning',(bank/'projectbrief.md').read_text())
            self.assertEqual(raw,(bank/held).read_bytes());self.assertEqual(before[1:],self.state(w,repo,bank)[1:])
            self.assertNotIn(self.marker,json.dumps(applied))

    def test_unsafe_authority_body_is_not_imported_and_independent_safe_local_delta_can_publish(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);self.github(w,repo,bank,peer);h=semantic_fixtures.SemanticSharingContract()
            self.marker='sk-'+('Y'*24)
            _,held=h.add(peer/'memory-bank','isolated-peer','# Isolated evidence\n\n'+self.marker+'\n');h.publish(w,peer)
            (bank/'activeContext.md').write_text('# Current focus\n\nIndependent local learning can be shared.\n')
            before=self.state(w,repo,bank);result=self.versioned_review(w,repo)
            self.assertNotIn(held,result['refresh_files']);self.assertIn(held,result['publish_files'])
            # Public preparation/landing preserve pre-existing authority bytes,
            # never introduce the quarantined payload from another side.
            prepared=self.good(w,'publish_memory.py','prepare','--v2','--repo',repo,
                '--input',self.private(w,'publish-proof',result['proof']))
            landed=self.land(w,repo,prepared['request'])
            self.assertTrue(landed['publication_verified']);self.assertEqual(before,self.state(w,repo,bank))

    def test_withheld_replacement_also_holds_its_older_claim_during_useful_refresh(self):
        for visibility in ('visible','hidden'):
            with self.subTest(visibility=visibility),World() as w:
                repo,bank,peer,base=self.fixture(w);h=semantic_fixtures.SemanticSharingContract()
                old,old_path=h.add(peer/'memory-bank','old-isolation','# Isolation policy\n\nThe former retention rule applies.\n')
                h.publish(w,peer);w.git('pull','--ff-only','origin','main',cwd=repo)
                self.marker='sk-'+('W'*24)
                new,new_path=h.add(peer/'memory-bank','replacement-isolation','# Isolation policy\n\n'+self.marker+'\n' if visibility=='visible' else '# Isolation policy\n\nThe clarified retention rule applies.\n')
                new['supersedes']=[{'id':old['id'],'version':old['version']}];new['visibility']=visibility
                body='# Isolation policy\n\n'+self.marker+'\n' if visibility=='visible' else '# Isolation policy\n\nThe clarified retention rule applies.\n'
                (peer/'memory-bank'/new_path).write_text(comparison_fixtures.render(new,body))
                (peer/'memory-bank/projectbrief.md').write_text('# Project\n\nIndependent peer orientation can proceed.\n');h.publish(w,peer)
                result=self.versioned_review(w,repo)
                self.assertTrue({old_path,new_path}.issubset(result['blocked_paths']))
                self.assertNotIn(new_path,result['refresh_files'])
                request={'proof':result['proof'],'recall':{'project_id':result['proof']['judgment']['binding']['project_id'],
                    'query':'','paths':[old_path,'projectbrief.md'],'ids':[],'context':{},'kinds':['semantic-fact'],
                    'minimum_trust':'reviewed','limit':8,'include_context':True}}
                prepared=w.inside('linked-refresh.json')
                self.good(w,'refresh_memory.py','prepare','--v2','--repo',repo,
                    '--input',self.private(w,'linked-refresh-request',request),'--output',prepared)
                output=w.inside('linked-refreshed.json')
                self.good(w,'refresh_memory.py','apply','--v2','--input',prepared,'--output',output)
                actual=json.loads(output.read_text());self.assertTrue(actual['refresh_applied'])
                self.assertNotIn('former retention',json.dumps(actual))
                self.assertIn('Independent peer orientation',json.dumps(actual))

    def test_unsafe_structural_caption_still_refuses_without_disclosing_it(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);self.marker='sk-'+('Z'*24)
            router=bank/'MEMORY.md';router.write_text(router.read_text().replace('Session focus',self.marker))
            before=self.state(w,repo,bank)
            result,value=self.call(w,'reconcile_memory.py','inspect-v2','--repo',repo,'--output',w.inside('must-not-exist.json'))
            self.assertEqual(result.returncode,1);self.assertNotIn(self.marker,json.dumps(value))
            self.assertFalse(w.inside('must-not-exist.json').exists());self.assertEqual(before,self.state(w,repo,bank))
