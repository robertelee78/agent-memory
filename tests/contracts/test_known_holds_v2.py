"""Actual detected disagreements survive wording changes and private-state loss."""
from datetime import datetime,timezone
import json
import sys
import unittest

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
            recovery=self.invoke(w,'recover-holds','--repo',repo,'--project-id',current['binding']['project_id'])
            self.assertTrue(recovery['verified']);self.assertEqual(registry.read_bytes(),before)
            _,healed=self.inspect(w,repo,'healed');self.assertTrue(healed['known_causes'])
            self.assertEqual(code,self.h.state(w,repo,bank)[1:])
