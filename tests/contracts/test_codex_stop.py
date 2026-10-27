"""Codex Stop guidance stays in the current turn and preserves local state."""
import json
import subprocess
import sys
import unittest
import uuid
import hashlib
from pathlib import Path
from world import World
from contracts import test_admission

class CodexStopContract(unittest.TestCase):
    def test_bound_turn_continues_once_and_missing_identity_is_pending(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            hook=helper.pack/'hooks/codex_stop.py';before=helper.state(w,repo,bank)
            session,turn=str(uuid.uuid4()),str(uuid.uuid4())
            event={'hook_event_name':'Stop','session_id':session,'turn_id':turn,'stop_hook_active':False,'cwd':str(repo),'last_assistant_message':'fixture answer'}
            prompt={**event,'hook_event_name':'UserPromptSubmit','prompt':'Actual scoped user request'}
            p=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/codex_prompt.py')],input=json.dumps(prompt),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(p.returncode,0,p.stderr)
            altered={**prompt,'prompt':'Different user request'}
            bad=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/codex_prompt.py')],input=json.dumps(altered),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertIn('pending',json.loads(bad.stdout)['systemMessage'])
            def invoke(value):
                p=subprocess.run([sys.executable,'-I',str(hook)],input=json.dumps(value),text=True,capture_output=True,env=w.env,timeout=10)
                self.assertEqual(p.returncode,0,p.stderr);return json.loads(p.stdout)
            answer=invoke(event);self.assertEqual(answer['decision'],'block');self.assertIn(turn,answer['reason']);self.assertIn('permissions',answer['reason'])
            pending_file=next(w.paths['state'].rglob('codex-boundaries/*.pending'));original=pending_file.read_bytes()
            pending_file.write_text('[]');self.assertIn('pending',invoke(event)['systemMessage']);pending_file.write_bytes(original)
            cli=helper.pack/'scripts/installer.py'
            inventory=subprocess.run([sys.executable,'-I',str(cli),'codex-boundary','--repo',str(repo),'--list-pending'],text=True,capture_output=True,env=w.env,timeout=10)
            listed=json.loads(inventory.stdout);self.assertTrue(listed['verified']);self.assertEqual(listed['pending'][0]['turn'],turn);self.assertNotIn(prompt['prompt'],inventory.stdout)
            self.assertEqual(invoke({**event,'stop_hook_active':True}),{})
            request={'session':session,'prompt':turn,'source_uuid':turn,'source_sha256':hashlib.sha256(prompt['prompt'].encode()).hexdigest(),'outcome':'no-learning','assessment':{'source_id':turn,'source_review':'Reviewed this source; no durable project learning.','candidates':[]}}
            input_path=w.inside('codex-ack.json');input_path.write_text(json.dumps(request))
            cli=helper.pack/'scripts/installer.py'
            args=[sys.executable,'-I',str(cli),'codex-boundary','--repo',str(repo),'--input',str(input_path)]
            ack=subprocess.run(args,text=True,capture_output=True,env=w.env,timeout=10)
            self.assertTrue(json.loads(ack.stdout)['verified'],ack.stdout+ack.stderr)
            retry=subprocess.run(args,text=True,capture_output=True,env=w.env,timeout=10)
            self.assertTrue(json.loads(retry.stdout)['already_completed'],retry.stdout+retry.stderr)
            done=next(w.paths['state'].rglob('codex-boundaries/*.done'));done_original=done.read_bytes();done.write_text('[]')
            malformed=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/codex_prompt.py')],input=json.dumps(prompt),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertIn('pending',json.loads(malformed.stdout)['systemMessage']);done.write_bytes(done_original)
            repeat=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/codex_prompt.py')],input=json.dumps(prompt),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertEqual(json.loads(repeat.stdout),{})
            changed=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/codex_prompt.py')],input=json.dumps(altered),text=True,capture_output=True,env=w.env,timeout=10)
            self.assertIn('pending',json.loads(changed.stdout)['systemMessage'])
            self.assertIn('pending',invoke({**event,'turn_id':None})['systemMessage'])
            self.assertEqual(before,helper.state(w,repo,bank))
    def test_nonrepository_and_uninitialized_are_quiet(self):
        with World() as w:
            hook=Path(__file__).resolve().parents[2]/'skills/agent-memory/hooks/codex_stop.py'
            for repo in (w.repository(),w.inside('outside')):
                repo.mkdir(exist_ok=True)
                event={'hook_event_name':'Stop','session_id':str(uuid.uuid4()),'turn_id':str(uuid.uuid4()),'stop_hook_active':False,'cwd':str(repo),'last_assistant_message':'ok'}
                result=subprocess.run([sys.executable,'-I',str(hook)],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=10)
                self.assertEqual(json.loads(result.stdout),{})
