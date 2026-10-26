"""Versioned refresh scans its complete changed delta, beyond a small capture batch."""
from datetime import datetime,timezone
import copy
import hashlib
import json
import sys
import unittest

from world import World
from publication_fixtures import PublicationFixture
from record_fixtures import project_id,record_text


class LargeRefreshSecurityContract(PublicationFixture):
    def private(self,w,name,value):
        path=w.inside(name+'.json');path.write_text(json.dumps(value));return path

    def view(self,w,repo,name,request=None):
        path=w.inside(name+'.json');args=['inspect-v2','--repo',repo,'--output',path]
        if request is not None:args+=['--input',self.private(w,name+'-request',request)]
        self.good(w,'reconcile_memory.py',*args);return path,json.loads(path.read_text())

    def test_complete_v2_delta_above_sixty_four_sources_imports_and_qualifies_peer_learning(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);other=peer/'memory-bank';pid=project_id((bank/'MEMORY.md').read_text())
            # Seventy independently authored records in two bounded existing
            # contract indexes, not an oversized root or one oversized source.
            paths=[]
            for group in range(2):
                routes=[]
                for n in range(group*35,(group+1)*35):
                    path='topics/peer-'+str(n)+'.md';paths.append(path)
                    (other/path).parent.mkdir(exist_ok=True)
                    (other/path).write_text(record_text('# Peer observation '+str(n)+'\n\nRetain the independent authored partition '+str(n)+' observation.\n',pid))
                    routes.append({'path':path,'purpose':'Partition '+str(n)+' observation','scope':'repository','lifecycle':'active','read-when':'When this partition applies'})
                index='topics/peer-group-'+str(group)+'.md'
                request={'path':index,'project-id':pid,'updated':'2026-10-25T00:00:00Z','routes':routes}
                content=self.good(w,'bank_layout.py','render-index','--input',self.private(w,'index-'+str(group),request))['content']
                (other/index).write_text(content)
                router=other/'MEMORY.md';router.write_text(router.read_text().replace('\n## Omitted contexts\n',
                    '| ['+index+'](<'+index+'>) | Independent peer partitions | repository | active | When inspecting peer observations |\n\n## Omitted contexts\n'))
            w.git('add','memory-bank',cwd=peer);w.git('commit','-m','Authored complete peer delta',cwd=peer);w.git('push','origin','HEAD:main',cwd=peer)
            before=self.state(w,repo,bank)
            _,initial=self.view(w,repo,'initial');rows={row['key']:row for row in initial['sources']}
            request=initial['request'];request['selections']={}
            for key in initial['changed']:
                row=rows[key];subject=row['target_path'] or row['path']
                request['selections'][key]={'searches':[subject,subject.removesuffix('.md')],'paths':[subject],'ids':[],'complete':True,'uncertain':False}
            path,view=self.view(w,repo,'selected',request)
            for page in view['pages']:self.good(w,'reconcile_memory.py','page-v2','--repo',repo,'--input',path,'--page',str(page['index']))
            judgment={'schema':'agent-memory/semantic-sharing-judgment-v2','request':request,'binding':view['binding'],
                'manifest_sha256':view['manifest_sha256'],'pages':[page['sha256'] for page in view['pages']],
                'pairs':[{'left':a,'right':b,'classification':'related','reason':'Independent authored partition observations and their navigation preserve distinct sources.'} for a,b in view['required_pairs']],
                'dependencies':{},'consolidations':[],'rationale':'Bounded mechanical complete-delta control, not semantic truth verification.',
                'recorded_at':datetime.now(timezone.utc).isoformat()}
            reviewed=w.inside('reviewed.json');self.good(w,'reconcile_memory.py','review-v2','--repo',repo,
                '--input',self.private(w,'judgment',judgment),'--output',reviewed)
            proof=json.loads(reviewed.read_text())['proof']
            recall={'project_id':pid,'query':'','ids':[],'paths':[paths[-1]],'context':{},'kinds':['semantic-fact'],'minimum_trust':'reviewed','limit':8,'include_context':False}
            prepared=w.inside('prepared.json');self.good(w,'refresh_memory.py','prepare','--v2','--repo',repo,
                '--input',self.private(w,'refresh',{'proof':proof,'recall':recall}),'--output',prepared)
            proposal=json.loads(prepared.read_text())['proposal']
            delta=[path for path,body in proposal['files'].items() if proposal['observed_hashes'].get(path)!=hashlib.sha256(body.encode()).hexdigest()]
            self.assertGreater(len(delta),64);self.assertGreater(sum(len(proposal['files'][path].encode()) for path in delta),65536)
            tampered=copy.deepcopy(proposal);tampered['files'][paths[-1]]+='\npassword=WITHHELD_LATE_DELTA_FIXTURE\n'
            failed,value=self.call(w,'refresh_memory.py','apply','--v2','--input',self.private(w,'tampered',tampered),'--output',w.inside('must-not-exist.json'))
            self.assertEqual(failed.returncode,1,value);self.assertNotIn('WITHHELD_LATE_DELTA_FIXTURE',failed.stdout+failed.stderr)
            self.assertEqual(before,self.state(w,repo,bank));self.assertFalse(w.inside('must-not-exist.json').exists())
            applied=w.inside('applied.json');self.good(w,'refresh_memory.py','apply','--v2','--input',prepared,'--output',applied)
            result=json.loads(applied.read_text());self.assertTrue(result['refresh_applied']);self.assertTrue(result['remote_freshness_verified'])
            self.assertEqual([row['path'] for row in result['recall']['results']],[paths[-1]])
            self.assertEqual(before[1:],self.state(w,repo,bank)[1:])
            for path in paths:self.assertEqual((bank/path).read_bytes(),(other/path).read_bytes())

    def test_complete_refresh_security_primitive_scans_late_sources_and_keeps_other_action_bounds(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);before=self.state(w,repo,bank)
            # This is a security primitive control, separate from the public
            # complete-delta workflow above; it grants no publication authority.
            program='''import sys,uuid
sys.path.insert(0,sys.argv[1])
from agent_memory import security,scope_binding
from agent_memory.frontmatter import ContractError
context=scope_binding.require(sys.argv[2])
contents={'topics/'+str(n)+'.md':'# Source\\n\\nA safe authored source.\\n' for n in range(70)}
contents['topics/69.md']='# Withheld\\n\\npassword=WITHHELD_LATE_PRIMITIVE_FIXTURE\\n'
try:security.guarded_start(sys.argv[2],str(uuid.uuid4()),context,contents,['external'],action='refresh',complete_refresh=True)
except ContractError as exc:assert 'credential' in str(exc)
else:raise AssertionError('Late unsafe source escaped complete refresh scanning')
for action,complete in (('capture',True),('capture',False)):
 try:security.guarded_start(sys.argv[2],str(uuid.uuid4()),context,contents,['external'],action=action,complete_refresh=complete)
 except ContractError:pass
 else:raise AssertionError('Another action inherited complete-refresh capacity')
body='x'*16384
try:security.complete_refresh_reasons({'topics/'+str(n)+'.md':body for n in range(4097)})
except ContractError:pass
else:raise AssertionError('Complete refresh escaped the actual bank byte bound')
print('complete-source scanning and action/aggregate bounds checked')
'''
            result=w.run([sys.executable,'-I','-c',program,self.pack/'scripts',repo],check=False)
            self.assertEqual(result.returncode,0,result.stderr);self.assertNotIn('WITHHELD_LATE_PRIMITIVE_FIXTURE',result.stdout+result.stderr)
            self.assertEqual(before,self.state(w,repo,bank))
