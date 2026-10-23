"""Public single-file checkpoint, section preservation and separate omitted-context setup."""
import json
import os
import time
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



    def test_valid_atx_heading_forms_preserve_unknown_source_notes_and_replace_one_known_section(self):
        for unknown in ('  ## Source notes','##\tSource notes','## Source notes ###'):
            with self.subTest(unknown=unknown),World() as w:
                repo,bank=self.fixture(w)
                legacy=LEGACY.replace('## Source notes',unknown)
                body=BASE.replace('## Next step','  ##\tNext step ###').replace(LEGACY,legacy)
                (bank/'activeContext.md').write_text(body)
                p=self.preview(w,repo,self.payload(self.inspect(w,repo)))
                self.assertTrue(self.apply(w,p)[1]['verified'])
                saved=(bank/'activeContext.md').read_text()
                self.assertIn(legacy,saved);self.assertNotIn('Inspect the other clone.',saved)
                again=self.preview(w,repo,self.payload(self.inspect(w,repo)))
                self.assertEqual(again['changed_memory_paths'],[])
        with World() as w:
            repo,bank=self.fixture(w);(bank/'activeContext.md').write_text(BASE+'\n  ## Next step ##\n\nConflicting next step.\n')
            before=self.files(bank)
            r,value=self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,self.payload(self.inspect(w,repo))))
            self.assertEqual(r.returncode,1,value);self.assertIn('Duplicate checkpoint section',value['error']);self.assertEqual(self.files(bank),before)

    def test_multiline_fenced_list_items_remain_repeatably_checkpointable(self):
        with World() as w:
            repo,bank=self.fixture(w);payload=self.payload(self.inspect(w,repo))
            payload['recent_changes']=['```markdown\n## Current focus\nQuoted example, not a root section\n```',
                                       'A multiline result\nwith [evidence](../result.md).']
            payload['open_questions']=['~~~markdown\n## Next step\nAn unresolved example\n~~~']
            p=self.preview(w,repo,payload);self.assertTrue(self.apply(w,p)[1]['verified'])
            body=(bank/'activeContext.md').read_text();self.assertIn(LEGACY,body)
            self.assertIn('    ## Current focus\n',body);self.assertIn('    with [evidence]',body)
            payload['observed_active_sha256']=self.inspect(w,repo)['observed_active_sha256']
            again=self.preview(w,repo,payload);self.assertEqual(again['changed_memory_paths'],[])
            self.assertTrue(self.apply(w,again)[1]['verified'])
            payload['observed_active_sha256']=self.inspect(w,repo)['observed_active_sha256']
            payload['next_step']='Reproduce a consequential new result before broadening conditions.'
            self.assertTrue(self.apply(w,self.preview(w,repo,payload))[1]['verified'])
            self.assertIn(LEGACY,(bank/'activeContext.md').read_text())

    def test_normalized_fields_cannot_create_new_root_sections_or_unclosed_fences(self):
        with World() as w:
            repo,bank=self.fixture(w);before=self.files(bank)
            for field,value in (('focus','    ## Current focus'),('next_step','    ```markdown')):
                payload=self.payload(self.inspect(w,repo));payload[field]=value
                r,result=self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,payload))
                self.assertEqual(r.returncode,1,result);self.assertEqual(self.files(bank),before)
            payload=self.payload(self.inspect(w,repo));payload['recent_changes']=['    ## Current focus']
            self.assertEqual(self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,payload))[0].returncode,1)
            self.assertEqual(self.files(bank),before)

    def test_unsupported_setext_and_unicode_boundaries_stop_without_dropping_original_notes(self):
        for heading in ('Source notes\n---','Source notes\n===','## Source notes\u2028'):
            with self.subTest(heading=heading),World() as w:
                repo,bank=self.fixture(w);(bank/'activeContext.md').write_text(BASE.replace('## Source notes',heading))
                before=self.files(bank);payload=self.payload(self.inspect(w,repo))
                r,value=self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,payload))
                self.assertEqual(r.returncode,1,value);self.assertEqual(self.files(bank),before)

    def test_missing_context_preparation_refuses_identity_switch_during_resolution(self):
        with World() as w:
            repo,bank=self.fixture(w,False);a=self.good(w,'scope_binding','status','--repo',repo)['developer_id']
            b=self.good(w,'developer_identity','preview','--create','--label','Developer B')
            b=self.good(w,'developer_identity','apply','--proposal',self.save(w,b))['developer_id']
            select=self.good(w,'developer_identity','preview','--select',a)
            self.good(w,'developer_identity','apply','--proposal',self.save(w,select))
            before=self.files(bank);payload=self.save(w,self.payload(self.inspect(w,repo)),'input.json')
            real_git=shutil.which('git',path=w.env['PATH']);directory=w.inside('identity-rendezvous');directory.mkdir()
            ready,release=directory/'ready',directory/'release';bindir=w.inside('bin');bindir.mkdir();wrapper=bindir/'git'
            wrapper.write_text('#!'+sys.executable+'\nimport os,sys,time\nfrom pathlib import Path\nready=Path('+repr(str(ready))+'); release=Path('+repr(str(release))+')\nif "--show-toplevel" in sys.argv and not ready.exists():\n ready.touch()\n deadline=time.monotonic()+10\n while not release.exists() and time.monotonic()<deadline:time.sleep(.005)\nos.execv('+repr(real_git)+',['+repr(real_git)+',*sys.argv[1:]])\n')
            wrapper.chmod(0o700);w.env['PATH']=str(bindir)+os.pathsep+w.env['PATH']
            child=w.spawn([sys.executable,'-I',SCRIPTS/'checkpoint.py','prepare','--repo',repo,'--input',payload])
            deadline=time.monotonic()+5
            while not ready.exists() and child.poll() is None and time.monotonic()<deadline:time.sleep(.005)
            self.assertTrue(ready.exists(),child.poll())
            select=self.good(w,'developer_identity','preview','--select',b)
            self.good(w,'developer_identity','apply','--proposal',self.save(w,select,'select-b.json'))
            release.touch();out,err=child.communicate(timeout=20)
            self.assertFalse(err);self.assertEqual(child.returncode,1,out)
            self.assertIn('identity changed',json.loads(out)['error']);self.assertEqual(self.files(bank),before)



    def test_every_unknown_atx_heading_level_preserves_original_source_section_and_remains_idempotent(self):
        for level in (1,3,4,5,6):
            with self.subTest(level=level),World() as w:
                repo,bank=self.fixture(w);legacy=LEGACY.replace('## Source notes','#'*level+' Source notes')
                (bank/'activeContext.md').write_text(BASE.replace(LEGACY,legacy))
                p=self.preview(w,repo,self.payload(self.inspect(w,repo)))
                self.assertTrue(self.apply(w,p)[1]['verified']);self.assertIn(legacy,(bank/'activeContext.md').read_text())
                again=self.preview(w,repo,self.payload(self.inspect(w,repo)));self.assertEqual(again['changed_memory_paths'],[])
                self.assertIn(legacy,(bank/'activeContext.md').read_text())

    def test_multiline_underlines_and_raw_html_refuse_without_deleting_source_memory(self):
        boundaries=['Source notes\n'+indent+'continued heading\n'+underline for indent in ('    ','\t') for underline in ('---','===')]
        boundaries+=['<h1>Source notes</h1>','<pre>\n## Current focus\n</pre>',
                     '<!--\n## Current focus\n-->','text <!--\n## Current focus\n-->']
        for boundary in boundaries:
            with self.subTest(boundary=boundary),World() as w:
                repo,bank=self.fixture(w);body=BASE.replace('## Source notes',boundary)
                if '<' in boundary:body=body.replace('## Current focus\n\nOld focus.\n\n','',1)
                (bank/'activeContext.md').write_text(body);before=self.files(bank)
                r,value=self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,self.payload(self.inspect(w,repo))))
                self.assertEqual(r.returncode,1,value);self.assertEqual(self.files(bank),before)
                self.assertIn(b'original developer',before['activeContext.md'])

    def test_non_markdown_control_separators_cannot_manufacture_root_sections(self):
        for separator in ('\x0b','\x0c','\x1c','\x1d','\x1e'):
            with self.subTest(code=ord(separator)),World() as w:
                repo,bank=self.fixture(w)
                body=BASE.replace('## Source notes','An inline continuation'+separator+'## Checkpoint evidence')
                (bank/'activeContext.md').write_text(body);before=self.files(bank)
                r,value=self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,self.payload(self.inspect(w,repo))))
                self.assertEqual(r.returncode,1,value);self.assertIn('control',value['error']);self.assertEqual(self.files(bank),before)

    def test_field_headings_are_explicitly_fenced_and_html_underline_examples_repeat_safely(self):
        with World() as w:
            repo,bank=self.fixture(w);before=self.files(bank)
            for heading in ('# Field title','### Field title','###### Field title'):
                payload=self.payload(self.inspect(w,repo));payload['focus']=heading
                r,value=self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,payload))
                self.assertEqual(r.returncode,1,value);self.assertEqual(self.files(bank),before)
            payload=self.payload(self.inspect(w,repo));payload['recent_changes']=['```html\n<h1>Source notes</h1>\n## Current focus\n---\n```']
            p=self.preview(w,repo,payload);self.assertTrue(self.apply(w,p)[1]['verified'])
            self.assertIn(LEGACY,(bank/'activeContext.md').read_text())
            payload['observed_active_sha256']=self.inspect(w,repo)['observed_active_sha256']
            self.assertEqual(self.preview(w,repo,payload)['changed_memory_paths'],[])

    def test_ambiguous_nested_heading_or_wrong_level_reserved_name_refuse_preserving_original(self):
        for boundary in ('- An unrelated source note\n\n  ## Source notes', '# Current focus'):
            with self.subTest(boundary=boundary),World() as w:
                repo,bank=self.fixture(w);(bank/'activeContext.md').write_text(BASE.replace('## Source notes',boundary))
                before=self.files(bank)
                r,value=self.command(w,'checkpoint','preview','--repo',repo,'--input',self.save(w,self.payload(self.inspect(w,repo))))
                self.assertEqual(r.returncode,1,value);self.assertEqual(self.files(bank),before)


if __name__=='__main__':unittest.main()
