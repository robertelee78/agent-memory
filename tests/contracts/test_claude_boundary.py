"""Source-bound Claude wake refuses ambiguous or repeated events."""
import json
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import unittest
import uuid
from world import World
from contracts import test_admission
from publication_fixtures import PublicationFixture

HOOK = Path(__file__).resolve().parents[2] / 'skills/agent-memory/hooks/claude_stop.py'

class ClaudeBoundaryContract(unittest.TestCase):
    def test_uninitialized_and_nonrepository_are_quiet(self):
        with World() as w:
            repo=w.repository()
            outside=w.inside('no-repo');outside.mkdir()
            for directory in (repo,outside):
                event={'hook_event_name':'Stop','stop_hook_active':False,'cwd':str(directory)}
                result=subprocess.run([sys.executable,'-B',str(HOOK)],input=json.dumps(event),text=True,
                                      capture_output=True,cwd=directory,env=w.env)
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertEqual(result.stdout,'')
                self.assertEqual(result.stderr,'')

    def test_published_ack_requires_real_authority_readback(self):
        with World() as w:
            fixture=PublicationFixture();repo,bank,_=fixture.bootstrap(w);fixture.github(w,repo,bank)
            session,prompt,message=map(str,(uuid.uuid4(),uuid.uuid4(),uuid.uuid4()))
            transcript=w.inside('transcript-publish.jsonl');transcript.write_text(json.dumps({'type':'user','sessionId':session,'promptId':prompt,'uuid':message,'message':{'role':'user','content':'Synthetic decision'}})+'\n')
            event={'hook_event_name':'Stop','stop_hook_active':False,'session_id':session,'prompt_id':prompt,
                   'cwd':str(repo),'transcript_path':str(transcript),'last_assistant_message':'Synthetic answer'}
            hook=subprocess.run([sys.executable,'-B',str(HOOK)],input=json.dumps(event),text=True,capture_output=True,cwd=repo,env=w.env)
            self.assertEqual(hook.returncode,2,hook.stdout+hook.stderr)
            request=fixture.prepared(w,repo)['request']
            value={'session':session,'prompt':prompt,'source_uuid':message,
                   'source_sha256':hashlib.sha256(b'Synthetic decision').hexdigest(),
                   'outcome':'published','assessment':'The reviewed candidate reflects the source.',
                   'publication_request':request}
            path=w.inside('publish-ack.json');path.write_text(json.dumps(value));cli=HOOK.parent.parent/'scripts/installer.py'
            before=w.run([sys.executable,'-I',cli,'claude-boundary','--repo',repo,'--input',path],check=False)
            self.assertNotEqual(before.returncode,0)
            landed=fixture.land(w,repo,request);self.assertTrue(landed['publication_verified'])
            after=w.run([sys.executable,'-I',cli,'claude-boundary','--repo',repo,'--input',path],check=False)
            self.assertEqual(after.returncode,0,after.stdout+after.stderr)
            self.assertTrue(json.loads(after.stdout)['publication_verified'])
            self.assertTrue(json.loads(after.stdout)['assessment_is_agent_assertion'])

    def test_bound_source_and_duplicate_are_fail_closed(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            session,prompt,message=map(str,(uuid.uuid4(),uuid.uuid4(),uuid.uuid4()))
            transcript=w.inside('transcript.jsonl');transcript.write_text(json.dumps({'type':'user','sessionId':session,'promptId':prompt,'uuid':message,'message':{'role':'user','content':'Synthetic decision'}})+'\n')
            event={'hook_event_name':'Stop','stop_hook_active':False,'session_id':session,'prompt_id':prompt,
                   'cwd':str(repo),'transcript_path':str(transcript),'last_assistant_message':'Synthetic answer'}
            before=helper.state(w,repo,bank)
            def call(e):return subprocess.run([sys.executable,'-B',str(HOOK)],input=json.dumps(e),text=True,capture_output=True,cwd=repo,env=w.env)
            first=call(event);self.assertEqual(first.returncode,2,first.stdout+first.stderr)
            self.assertIn(message,first.stderr);self.assertIn('Verify that exact source',first.stderr)
            self.assertNotIn('Synthetic decision',first.stderr)
            second=call(event);self.assertEqual(second.returncode,0);self.assertIn('pending',second.stdout)
            request={'session':session,'prompt':prompt,'source_uuid':message,
                     'source_sha256':hashlib.sha256(b'Synthetic decision').hexdigest(),
                     'outcome':'no-learning','assessment':'The source contains no reusable lesson.'}
            input_path=w.inside('assessment.json');input_path.write_text(json.dumps(request))
            cli=HOOK.parent.parent/'scripts/installer.py'
            bad={**request,'source_sha256':'0'*64};input_path.write_text(json.dumps(bad))
            refused=w.run([sys.executable,'-I',cli,'claude-boundary','--repo',repo,'--input',input_path],check=False)
            self.assertNotEqual(refused.returncode,0)
            self.assertIn('pending',call(event).stdout)
            input_path.write_text(json.dumps(request))
            pending = next((w.paths['state']/'agent-memory').rglob('*.pending'))
            pending_bytes = pending.read_bytes()
            completed=w.run([sys.executable,'-I',cli,'claude-boundary','--repo',repo,'--input',input_path],check=False)
            self.assertEqual(completed.returncode,0,completed.stdout+completed.stderr)
            self.assertTrue(json.loads(completed.stdout)['assessment_is_agent_assertion'])
            self.assertFalse(json.loads(completed.stdout)['publication_verified'])
            self.assertFalse(json.loads(completed.stdout)['source_publication_link_verified'])
            pending.write_bytes(pending_bytes);pending.chmod(0o600)
            recovered=w.run([sys.executable,'-I',cli,'claude-boundary','--repo',repo,'--input',input_path],check=False)
            self.assertEqual(recovered.returncode,0,recovered.stdout+recovered.stderr)
            self.assertFalse(pending.exists())
            self.assertEqual(call(event).stdout,'')
            guarded=call({**event,'stop_hook_active':True});self.assertEqual(guarded.returncode,0)
            missing=call({**event,'prompt_id':str(uuid.uuid4())});self.assertEqual(missing.returncode,0);self.assertIn('pending',missing.stdout)
            self.assertEqual(before,helper.state(w,repo,bank))
