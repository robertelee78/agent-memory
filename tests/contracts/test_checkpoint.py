"""Public single-file checkpoint, section preservation and separate omitted-context setup."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World
from identity_fixtures import bind

ROOT=Path(__file__).resolve().parents[2]
SCRIPTS=ROOT/'scripts'
CONTEXTS=['projectbrief.md','productContext.md','activeContext.md','systemPatterns.md','techContext.md','progress.md']
LEGACY='## Source notes\n\nKeep branch:legacy, original developer and [evidence](../legacy-result.md).\n\n```markdown\n## Current focus\nThis is an example, not a duplicate checkpoint section.\n```\n'
BASE='# Active context\n\n## Current focus\n\nOld focus.\n\n## Recent changes\n\nEarlier meaningful result with its source.\n\n## Open questions\n\nDoes this hold beyond the retry component?\n\n## Next step\n\nInspect the other clone.\n\n'+LEGACY


class CheckpointContract(unittest.TestCase):
    def command(self,w,name,*args,env=None):
        helper=Path(name)
        if not helper.is_absolute():helper=SCRIPTS/(name+'.py')
        r=w.run([sys.executable,'-I',helper,*args],check=False,env=env)
        return r,json.loads(r.stdout) if r.stdout else None

    def good(self,w,name,*args,**kwargs):
        r,v=self.command(w,name,*args,**kwargs);self.assertEqual(r.returncode,0,v or r.stderr);return v

    def save(self,w,value,name='proposal.json'):
        p=w.inside(name);p.write_text(json.dumps(value));return p

    def fixture(self,w,active=True):
        repo=w.repository();pid=str(uuid.uuid4());present={'projectbrief.md':'# Brief\n\nStable project requirements.\n','progress.md':'# Progress\n\nPlanned work is not release proof.\n'}
        if active:present['activeContext.md']=BASE
        p={'project-id':pid,'router-updated':'2026-10-22T18:00:00Z','contexts':{
            path:{'body':body,'purpose':'Fixture context','scope':'repository','lifecycle':'active','read-when':'When resuming'} for path,body in present.items()},
            'omitted':{path:'No distinct useful evidence' for path in CONTEXTS if path not in present}}
        files=self.good(w,'bank_contract','render','--input',self.save(w,p))['files'];bank=repo/'memory-bank';bank.mkdir()
        for path,body in files.items():(bank/path).write_text(body)
        bind(w,repo);return repo,bank

    def inspect(self,w,repo):return self.good(w,'checkpoint','inspect','--repo',repo)

    def payload(self,current):
        return {'focus':'Verify the retry lesson against the current branch; [task](../task.md).',
                'recent_changes':['The isolated retry result was inspected; preserve its conditions and evidence.'],
                'open_questions':['Applicability outside this component remains unknown.'],
                'next_step':'Inspect the second clone before broadening the lesson.',
                'evidence':['../task.md','../retry-result.md'],'conditions':{'branch':'main','component':'src/retry'},
                'harness':'custom','session':'session-1','observed_active_sha256':current['observed_active_sha256']}

    def preview(self,w,repo,payload):return self.good(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,payload,'input.json'))
    def apply(self,w,p,**kwargs):return self.command(w,'checkpoint','apply','--proposal',self.save(w,p),**kwargs)
    def files(self,bank):return {p.relative_to(bank).as_posix():p.read_bytes() for p in bank.rglob('*.md') if not p.name.startswith('.agent-memory')}

    def test_checkpoint_captures_four_fields_with_provenance_and_preserves_every_other_file_and_unrelated_sections(self):
        with World() as w:
            repo,bank=self.fixture(w);(repo/'feature.py').write_text('unfinished = True\n');w.git('add','feature.py',cwd=repo)
            index,head=(repo/'.git/index').read_bytes(),(repo/'.git/HEAD').read_bytes();before=self.files(bank)
            p=self.preview(w,repo,self.payload(self.inspect(w,repo)));self.assertEqual(self.files(bank),before)
            r,done=self.apply(w,p);self.assertEqual(r.returncode,0,done);self.assertTrue(done['verified'])
            after=self.files(bank);self.assertEqual({k:v for k,v in after.items() if k!='activeContext.md'},{k:v for k,v in before.items() if k!='activeContext.md'})
            body=after['activeContext.md'].decode();self.assertIn(LEGACY,body)
            for heading in ('Current focus','Recent changes','Open questions','Next step','Checkpoint provenance','Checkpoint conditions'):
                self.assertIn('## '+heading,body)
            self.assertIn('"harness": "custom"',body);self.assertIn('"branch": "main"',body)
            self.assertEqual(done['changed_memory_paths'],['memory-bank/activeContext.md']);self.assertFalse(done['publication_verified'])
            self.assertEqual((repo/'.git/index').read_bytes(),index);self.assertEqual((repo/'.git/HEAD').read_bytes(),head)
            self.assertEqual((repo/'feature.py').read_text(),'unfinished = True\n')
            again=self.preview(w,repo,self.payload(self.inspect(w,repo)));self.assertEqual(again['changed_memory_paths'],[])

    def test_stale_active_or_other_bank_edits_are_preserved_and_fresh_checkpoint_carries_manual_evidence(self):
        with World() as w:
            repo,bank=self.fixture(w);current=self.inspect(w,repo);payload=self.payload(current);p=self.preview(w,repo,payload)
            (bank/'activeContext.md').write_text(BASE+'\n## Manual evidence\n\nRetain [manual result](../manual.md) under branch:legacy.\n')
            changed=self.files(bank);self.assertEqual(self.apply(w,p)[0].returncode,1);self.assertEqual(self.files(bank),changed)
            self.assertEqual(self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,payload))[0].returncode,1)
            p=self.preview(w,repo,self.payload(self.inspect(w,repo)));r,done=self.apply(w,p);self.assertEqual(r.returncode,0,done)
            self.assertIn('Retain [manual result]',(bank/'activeContext.md').read_text())
            p=self.preview(w,repo,self.payload(self.inspect(w,repo)));(bank/'progress.md').write_text('# Progress\n\nNew external work evidence.\n')
            changed=self.files(bank);self.assertEqual(self.apply(w,p)[0].returncode,1);self.assertEqual(self.files(bank),changed)

    def test_missing_active_context_requires_a_separate_observed_route_transaction_then_only_active_is_checkpointed(self):
        with World() as w:
            repo,bank=self.fixture(w,False);current=self.inspect(w,repo);self.assertTrue(current['preparation_required']);payload=self.payload(current);before=self.files(bank)
            self.assertEqual(self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,payload))[0].returncode,1)
            prepare=self.good(w,'checkpoint','prepare','--repo',repo,'--input',self.save(w,payload));self.assertTrue(prepare['separate_router_operation'])
            self.assertEqual(self.files(bank),before)
            done=self.good(w,'bank_transactions','apply','--proposal',self.save(w,prepare['transaction']));self.assertTrue(done['verified'])
            self.assertEqual(set(done['changed_memory_paths']),{'memory-bank/MEMORY.md','memory-bank/activeContext.md'})
            payload=self.payload(self.inspect(w,repo));payload['next_step']='Inspect a newly reported meaningful result.'
            before=self.files(bank);p=self.preview(w,repo,payload);r,done=self.apply(w,p);self.assertEqual(r.returncode,0,done)
            self.assertEqual(done['changed_memory_paths'],['memory-bank/activeContext.md'])
            for path,body in before.items():
                if path!='activeContext.md':self.assertEqual((bank/path).read_bytes(),body)

    def test_ambiguous_headings_oversized_input_and_substituted_body_or_target_refuse(self):
        with World() as w:
            repo,bank=self.fixture(w);payload=self.payload(self.inspect(w,repo));before=self.files(bank)
            bad=dict(payload);bad['focus']='x'*20000
            self.assertEqual(self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,bad))[0].returncode,1)
            p=self.preview(w,repo,payload)
            for field,value in (('path','progress.md'),('content','# Forged\n\nDropped existing sources.\n')):
                bad=json.loads(json.dumps(p));bad['proposal']['write'][field]=value
                self.assertEqual(self.apply(w,bad)[0].returncode,1);self.assertEqual(self.files(bank),before)
            (bank/'activeContext.md').write_text(BASE+'\n## Current focus\n\nA second ambiguous focus.\n')
            payload=self.payload(self.inspect(w,repo))
            self.assertEqual(self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,payload))[0].returncode,1)

    def test_interrupted_checkpoint_replays_after_owned_lock_recovery_without_duplicate_or_later_overwrite(self):
        with World() as w:
            repo,bank=self.fixture(w);p=self.preview(w,repo,self.payload(self.inspect(w,repo)))
            r,_=self.apply(w,p,env={'AGENT_MEMORY_FAULT_STAGE':'after-replace','AGENT_MEMORY_FAULT_MODE':'exit'});self.assertEqual(r.returncode,86)
            unlock=self.good(w,'bank_writes','recover','--repo',repo);self.good(w,'bank_writes','recover','--repo',repo,'--proposal',self.save(w,unlock))
            r,done=self.apply(w,p);self.assertEqual(r.returncode,0,done);self.assertTrue(done['verified']);self.assertFalse(done['writes_performed'])
            (bank/'activeContext.md').write_text('# Later correction\n\nPreserve this newer evidence.\n');before=self.files(bank)
            r,replay=self.apply(w,p);self.assertEqual(r.returncode,0,replay);self.assertFalse(replay['verified']);self.assertEqual(self.files(bank),before)

    def test_copied_artifact_uses_common_identity_and_untrusted_existing_markdown_is_never_executed(self):
        with World() as w:
            repo,bank=self.fixture(w);sentinel=w.inside('must-not-execute')
            (bank/'activeContext.md').write_text(BASE+f'\n## Untrusted example\n\n`touch {sentinel}` and $(touch {sentinel}) are data.\n')
            pack=w.inside('portable pack');shutil.copytree(ROOT/'skills/agent-memory',pack);helper=pack/'scripts/checkpoint.py'
            current=self.good(w,helper,'inspect','--repo',repo);p=self.good(w,helper,'preview','--repo',repo,'--input',self.save(w,self.payload(current)))
            done=self.good(w,helper,'apply','--proposal',self.save(w,p));self.assertTrue(done['verified']);self.assertFalse(sentinel.exists())
            self.assertIn('are data.',(bank/'activeContext.md').read_text())
            p=self.preview(w,repo,self.payload(self.inspect(w,repo)))
            d=self.good(w,'developer_identity','preview','--create','--label','Another selected developer');self.good(w,'developer_identity','apply','--proposal',self.save(w,d))
            before=self.files(bank);self.assertEqual(self.apply(w,p)[0].returncode,1);self.assertEqual(self.files(bank),before)


if __name__=='__main__':unittest.main()
