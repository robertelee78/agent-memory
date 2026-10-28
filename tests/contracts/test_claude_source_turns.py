"""Native Claude input survives interruption and finishes before Stop quietly."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import unittest
import uuid
from world import World
from contracts import test_admission

class ClaudeSourceTurns(unittest.TestCase):
    def test_enroll_before_transcript_source_assertion_then_quiet_nullable_stop(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            session,prompt,message=map(str,(uuid.uuid4(),uuid.uuid4(),uuid.uuid4()))
            transcript=w.inside('not-yet-written.jsonl')
            event={'hook_event_name':'UserPromptSubmit','session_id':session,'prompt_id':prompt,
                   'cwd':str(repo),'transcript_path':str(transcript),'prompt':'Fixture status question'}
            def hook(name,value):
                return subprocess.run([sys.executable,'-I',str(helper.pack/'hooks'/name)],input=json.dumps(value),text=True,capture_output=True,env=w.env,timeout=10)
            enrolled=hook('turn_hook.py',event)
            self.assertEqual(enrolled.returncode,0,enrolled.stderr)
            self.assertIn('observation_id=',json.loads(enrolled.stdout)['hookSpecificOutput']['additionalContext'])
            self.assertFalse(transcript.exists())
            cli=helper.pack/'scripts/installer.py'
            listed=w.run([sys.executable,'-I',cli,'claude-boundary','--repo',repo,'--list-pending','--session',session,'--prompt',prompt])
            pending=json.loads(listed.stdout)['pending'];self.assertEqual(len(pending),1)
            observation=pending[0]['observation_id']
            request={'session':session,'prompt':prompt,'observation_id':observation,'source_sha256':hashlib.sha256(event['prompt'].encode()).hexdigest(),
                     'outcome':'no-learning','assessment':{'source_id':observation,'source_review':'Reviewed the fixture status question; no durable learning.','candidates':[]}}
            args=[sys.executable,'-I',cli,'claude-boundary','--repo',repo,'--input','-','--quiet']
            malformed={**request,'observation_id':observation+' invalid-shell-word'}
            invalid=subprocess.run(args,input=json.dumps(malformed),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertNotEqual(invalid.returncode,0)
            self.assertIn('observation_id',json.loads(invalid.stdout)['reason'])
            before=subprocess.run(args,input=json.dumps(request),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(before.returncode,0,before.stdout+before.stderr)
            self.assertFalse(transcript.exists())
            transcript.write_text(json.dumps({'type':'user','sessionId':session,'promptId':prompt,'uuid':message,'message':{'role':'user','content':event['prompt']}})+'\n')
            completed=subprocess.run(args,input=json.dumps(request),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(completed.returncode,0,completed.stdout+completed.stderr);self.assertEqual(completed.stdout,'')
            stop=hook('claude_stop.py',{**event,'hook_event_name':'Stop','stop_hook_active':False,'last_assistant_message':None})
            self.assertEqual(stop.returncode,0,stop.stdout+stop.stderr);self.assertEqual(stop.stdout+stop.stderr,'')
            transcript.unlink()  # Exact retry uses the already verified receipt.
            retry=subprocess.run(args,input=json.dumps(request),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(retry.returncode,0,retry.stdout+retry.stderr)

    def test_interrupted_prompt_is_discoverable_and_stdin_cannot_claim_publication(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            event={'hook_event_name':'UserPromptSubmit','session_id':str(uuid.uuid4()),'prompt_id':str(uuid.uuid4()),'cwd':str(repo),
                   'transcript_path':str(w.inside('interrupted.jsonl')),'prompt':'Interrupted fixture input'}
            result=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/claude_prompt.py')],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertIn('hookSpecificOutput',json.loads(result.stdout))
            cli=helper.pack/'scripts/installer.py'
            later=w.run([sys.executable,'-I',cli,'claude-boundary','--repo',repo,'--list-pending'])
            self.assertTrue(json.loads(later.stdout)['turns'])
            bad=subprocess.run([sys.executable,'-I',str(cli),'claude-boundary','--repo',str(repo),'--input','-'],input=json.dumps({'outcome':'published','verified':True}),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertNotEqual(bad.returncode,0)

    def test_legacy_unicode_done_is_bounded_and_corruption_is_not_quiet(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            session,prompt,message=map(str,(uuid.uuid4(),uuid.uuid4(),uuid.uuid4()))
            transcript=w.inside('legacy.jsonl');text='Fixture arithmetic question'
            transcript.write_text(json.dumps({'type':'user','sessionId':session,'promptId':prompt,'uuid':message,'message':{'role':'user','content':text}})+'\n')
            event={'hook_event_name':'Stop','session_id':session,'prompt_id':prompt,'cwd':str(repo),'transcript_path':str(transcript),'stop_hook_active':False,'last_assistant_message':None}
            def stop():return subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/claude_stop.py')],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=10)
            first=stop();self.assertEqual(first.returncode,2,first.stdout+first.stderr)
            request={'session':session,'prompt':prompt,'source_uuid':message,'source_sha256':hashlib.sha256(text.encode()).hexdigest(),'outcome':'no-learning','assessment':{'source_id':message,'source_review':'Reviewed transient fixture question. '+('🌻'*380),'candidates':[]}}
            path=w.inside('legacy-assessment.json');path.write_text(json.dumps(request))
            result=w.run([sys.executable,'-I',helper.pack/'scripts/installer.py','claude-boundary','--repo',repo,'--input',path],check=False)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            self.assertEqual(stop().stdout+stop().stderr,'')
            completed=next(w.paths['state'].rglob('*.done'));record=json.loads(completed.read_text());record['outcome']='invented';completed.write_text(json.dumps(record,ensure_ascii=False))
            corrupt=stop();self.assertIn('pending',corrupt.stdout);self.assertEqual(corrupt.returncode,0)
            transcript.unlink();import os;os.mkfifo(transcript)
            fifo=stop();self.assertEqual(fifo.returncode,0);self.assertIn('pending',fifo.stdout)

    def test_interrupted_old_unicode_completion_retries_without_rewriting_evidence(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            session,prompt,message=map(str,(uuid.uuid4(),uuid.uuid4(),uuid.uuid4()))
            transcript=w.inside('legacy-interrupted.jsonl');text='Transient fixture question'
            transcript.write_text(json.dumps({'type':'user','sessionId':session,'promptId':prompt,'uuid':message,'message':{'role':'user','content':text}})+'\n')
            event={'hook_event_name':'Stop','session_id':session,'prompt_id':prompt,'cwd':str(repo),'transcript_path':str(transcript),'stop_hook_active':False,'last_assistant_message':None}
            enrolled=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/claude_stop.py')],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(enrolled.returncode,2,enrolled.stdout+enrolled.stderr)
            pending=next(w.paths['state'].rglob('*.pending'));pending_raw=pending.read_bytes()
            request={'session':session,'prompt':prompt,'source_uuid':message,'source_sha256':hashlib.sha256(text.encode()).hexdigest(),'outcome':'no-learning','assessment':{'source_id':message,'source_review':'Reviewed transient fixture question. '+('🌻'*50),'candidates':[]}}
            path=w.inside('legacy-retry.json');path.write_text(json.dumps(request))
            args=[sys.executable,'-I',helper.pack/'scripts/installer.py','claude-boundary','--repo',repo,'--input',path]
            completed=w.run(args,check=False)
            self.assertEqual(completed.returncode,0,completed.stdout+completed.stderr)
            done=next(w.paths['state'].rglob('*.done'))
            # An old v1 writer used ASCII escapes. Simulate interruption after
            # durable completion publication but before pending was removed.
            old_record=json.loads(done.read_bytes())
            old_raw=json.dumps(old_record,sort_keys=True,ensure_ascii=True).encode()
            self.assertLessEqual(len(old_raw),4096)
            done.write_bytes(old_raw);pending.write_bytes(pending_raw);pending.chmod(0o600)
            retry=w.run(args,check=False)
            self.assertEqual(retry.returncode,0,retry.stdout+retry.stderr)
            self.assertTrue(json.loads(retry.stdout)['verified'])
            self.assertFalse(pending.exists())
            self.assertEqual(done.read_bytes(),old_raw)

    def test_shared_native_prompt_tracks_three_distinct_observations_without_transcript_projection(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            session,prompt=map(str,(uuid.uuid4(),uuid.uuid4()))
            event={'hook_event_name':'UserPromptSubmit','session_id':session,'prompt_id':prompt,'cwd':str(repo),'transcript_path':str(w.inside('native-queued.jsonl'))}
            observed=[]
            for text in ['Fixture question one','Fixture question two','Fixture question two']:
                result=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/claude_prompt.py')],input=json.dumps({**event,'prompt':text}),text=True,capture_output=True,env=w.env,timeout=10)
                self.assertIn('hookSpecificOutput',json.loads(result.stdout))
                listed=w.run([sys.executable,'-I',helper.pack/'scripts/installer.py','claude-boundary','--repo',repo,'--list-pending','--session',session,'--prompt',prompt])
                entries=json.loads(listed.stdout)['pending'];new=next(x for x in entries if x['observation_id'] not in [v[0] for v in observed]);observed.append((new['observation_id'],text))
            self.assertEqual(len({x[0] for x in observed}),3)
            for index,(observation,text) in enumerate(observed):
                request={'session':session,'prompt':prompt,'observation_id':observation,'source_sha256':hashlib.sha256(text.encode()).hexdigest(),'outcome':'no-learning','assessment':{'source_id':observation,'source_review':'Reviewed this exact disposable fixture question; no durable project learning.','candidates':[]}}
                args=[sys.executable,'-I',str(helper.pack/'scripts/installer.py'),'claude-boundary','--repo',str(repo),'--input','-','--quiet']
                wrong={**request,'source_sha256':'0'*64}
                refused=subprocess.run(args,input=json.dumps(wrong),text=True,capture_output=True,env=w.env,timeout=10);self.assertNotEqual(refused.returncode,0)
                result=subprocess.run(args,input=json.dumps(request),text=True,capture_output=True,env=w.env,timeout=10);self.assertEqual(result.returncode,0,result.stdout+result.stderr)
                listed=w.run([sys.executable,'-I',helper.pack/'scripts/installer.py','claude-boundary','--repo',repo,'--list-pending','--session',session,'--prompt',prompt]);self.assertEqual(len(json.loads(listed.stdout)['pending']),2-index)
            stop=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/claude_stop.py')],input=json.dumps({**event,'hook_event_name':'Stop','stop_hook_active':False,'last_assistant_message':None}),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(stop.returncode,0);self.assertEqual(stop.stdout+stop.stderr,'')
