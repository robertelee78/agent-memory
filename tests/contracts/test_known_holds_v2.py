"""Actual detected disagreements survive wording changes and private-state loss."""
from datetime import datetime,timezone
import json
import hashlib
import shutil
import sys
import unittest
import uuid

from world import World
from contracts import test_semantic_sharing as fixtures


class KnownHoldsVersionTwoContract(unittest.TestCase):
    def invoke(self,w,*args,expected=0):
        result=w.run([sys.executable,'-I',self.h.pack/'scripts/reconcile_memory.py',*args],check=False,timeout=120)
        value=json.loads(result.stdout);self.assertEqual(result.returncode,expected,(value,result.stderr));return value

    def inspect(self,w,repo,name,request=None):
        output=w.inside(name+'.json');args=['inspect-v2','--repo',repo,'--output',output]
        if request is not None:
            path=w.inside(name+'-request.json');path.write_text(json.dumps(request));args+=['--input',path]
        self.invoke(w,*args);return output,json.loads(output.read_text())

    def review(self,w,repo,view_path,view,contradiction):
        for page in view['pages']:self.invoke(w,'page-v2','--repo',repo,'--input',view_path,'--page',str(page['index']))
        rows={row['key']:row for row in view['sources']}
        judgment={'schema':'agent-memory/semantic-sharing-judgment-v2','request':view['request'],'binding':view['binding'],
            'manifest_sha256':view['manifest_sha256'],'pages':[page['sha256'] for page in view['pages']],
            'pairs':[{'left':a,'right':b,'classification':'contradiction' if contradiction and {rows[a]['path'],rows[b]['path']}==self.claims else 'related',
                'reason':'This authored fixture retains the same unresolved room color claims.'} for a,b in view['required_pairs']],
            'dependencies':{},'consolidations':[],'rationale':'Exercise retained dispute mechanics, not a model semantic-quality verdict.',
            'recorded_at':datetime.now(timezone.utc).isoformat()}
        path=w.inside('judgment.json');path.write_text(json.dumps(judgment));output=w.inside('reviewed.json')
        if output.exists():output.unlink()
        self.invoke(w,'review-v2','--repo',repo,'--input',path,'--output',output)
        return json.loads(output.read_text())

    def select(self,view):
        request=view['request'];request['selections']={key:{'searches':['wall','atrium'],'paths':sorted(self.claims),
            'ids':[],'complete':True,'uncertain':False} for key in view['changed']}
        return request

    def test_known_dispute_survives_rewording_changed_framing_and_registry_recovery(self):
        with World() as w:
            self.h=fixtures.SemanticSharingContract();repo,bank,peer,base=self.h.fixture(w)
            _,local=self.h.add(bank,'wall-local','# Atrium wall\n\nThe atrium wall should be yellow.\n')
            _,landed=self.h.add(peer/'memory-bank','wall-peer','# Atrium wall\n\nThe atrium wall should be blue.\n')
            self.claims={local,landed};self.h.publish(w,peer);code=self.h.state(w,repo,bank)[1:]
            _,initial=self.inspect(w,repo,'initial');path,view=self.inspect(w,repo,'selected',self.select(initial))
            first=self.review(w,repo,path,view,True);self.assertTrue(self.claims.issubset(first['blocked_paths']))
            proof=w.inside('first-proof.json');proof.write_text(json.dumps(first['proof']))
            checked=self.invoke(w,'check','--v2','--repo',repo,'--input',proof)
            self.assertTrue(checked['source_observation_current'])
            # These are still the same contradictory claims; only their exact
            # source/semantic variant and inherited route wording have changed.
            (bank/local).write_text((bank/local).read_text().replace('should be yellow','should use a yellow finish'))
            router=bank/'MEMORY.md';router.write_text(router.read_text().replace('Scoped reviewed lesson','Room configuration claim'))
            _,current=self.inspect(w,repo,'changed')
            self.assertTrue(current['known_causes'])
            self.assertTrue(all(not cause['missing_sources'] for cause in current['known_causes']))
            path,current=self.inspect(w,repo,'changed-selected',self.select(current))
            retained=self.review(w,repo,path,current,False)
            self.assertTrue(self.claims.issubset(retained['blocked_paths']));self.assertFalse(retained['messages_sent'])
            registries=list(w.paths['state'].rglob('semantic-holds/*/registry.json'));self.assertEqual(len(registries),1)
            registry=registries[0];before=registry.read_bytes();registry.unlink()
            pending=w.inside('must-not-report-empty.json')
            failed=self.invoke(w,'inspect-v2','--repo',repo,'--output',pending,expected=1);self.assertFalse(pending.exists())
            self.assertEqual(failed['failure_code'],'known-holds-registry-missing')
            self.assertEqual(failed['failure_component'],'known-dispute-state');self.assertIn('recover-holds',failed['repair'])
            command=w.run([sys.executable,'-I',self.h.pack/'scripts/refresh_memory.py','inspect','--v2','--repo',repo],check=False)
            refresh=json.loads(command.stdout);self.assertEqual(command.returncode,1,refresh)
            self.assertEqual(refresh['failure_code'],'known-holds-registry-missing');self.assertIn('recover-holds',refresh['repair'])
            recovery=self.invoke(w,'recover-holds','--repo',repo,'--project-id',current['binding']['project_id'])
            self.assertTrue(recovery['verified']);self.assertEqual(registry.read_bytes(),before)
            _,healed=self.inspect(w,repo,'healed');self.assertTrue(healed['known_causes'])
            command=w.run([sys.executable,'-I',self.h.pack/'scripts/refresh_memory.py','inspect','--v2','--repo',repo],check=False)
            self.assertEqual(command.returncode,0,command.stdout);self.assertTrue(json.loads(command.stdout)['inspection']['known_causes'])
            # Materialize the actual predecessor per-cause format. It lacks
            # stable record references, but its original logical references and
            # first-detection evidence must survive supported recovery/import.
            native=json.loads(registry.read_text());legacy={}
            for key,record in native['records'].items():
                value={'schema':'agent-memory/known-semantic-hold-v1','repo_root':str(repo),
                    'project_id':current['binding']['project_id'],**{k:v for k,v in record.items() if k!='resolution'}}
                value['references']={k:{n:v for n,v in ref.items() if n!='record'} for k,ref in value['references'].items()}
                value['checksum']=hashlib.sha256((json.dumps(value,sort_keys=True,indent=2,ensure_ascii=False)+'\n').encode()).hexdigest()
                target=registry.parent.parent/(key+'.json');target.write_text(json.dumps(value));target.chmod(0o600);legacy[target]=target.read_bytes()
            (registry.parent.parent/(current['binding']['project_id']+'.present.json')).unlink();shutil.rmtree(registry.parent)
            failed=self.invoke(w,'inspect-v2','--repo',repo,'--output',w.inside('legacy-pending.json'),expected=1)
            self.assertEqual(failed['failure_code'],'known-holds-legacy-state');self.assertIn('recover-holds',failed['repair'])
            recovery=self.invoke(w,'recover-holds','--repo',repo,'--project-id',current['binding']['project_id'])
            self.assertTrue(recovery['verified']);self.assertEqual(legacy,{p:p.read_bytes() for p in legacy})
            _,migrated=self.inspect(w,repo,'legacy-imported');self.assertTrue(migrated['known_causes'])
            self.assertEqual(code,self.h.state(w,repo,bank)[1:])

    def test_wrong_project_recovery_preserves_private_state_and_stale_bank_marker(self):
        with World() as w:
            self.h=fixtures.SemanticSharingContract();repo,bank,peer,base=self.h.fixture(w)
            project=json.loads((bank/'MEMORY.md').read_text().split('project-id: ',1)[1].splitlines()[0])
            program='''import os,sys,uuid
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from agent_memory import local_registry as storage
with storage.exclusive(Path(sys.argv[2])/'.agent-memory.lock',str(uuid.uuid4()),'semantic-review-holds'):
 os._exit(86)
'''
            result=w.run([sys.executable,'-I','-c',program,self.h.pack/'scripts',bank],check=False)
            self.assertEqual(result.returncode,86)
            def snapshot():return {str(p.relative_to(w.root)):(p.read_bytes(),p.stat().st_mode) for p in w.root.rglob('*') if p.is_file()}
            before=snapshot()
            self.invoke(w,'recover-holds','--repo',repo,'--project-id',str(uuid.uuid4()),expected=1)
            self.assertEqual(before,snapshot());self.assertTrue((bank/'.agent-memory.lock').exists())
            recovered=self.invoke(w,'recover-holds','--repo',repo,'--project-id',project)
            self.assertTrue(recovered['verified']);self.assertFalse((bank/'.agent-memory.lock').exists())
