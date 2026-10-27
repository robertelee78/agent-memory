"""Codex Stop guidance stays in the current turn and preserves local state."""
import json
import subprocess
import sys
import unittest
from pathlib import Path
from world import World
from contracts import test_admission

class CodexStopContract(unittest.TestCase):
    def test_bound_turn_continues_once_and_missing_identity_is_pending(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            hook=helper.pack/'hooks/codex_stop.py';before=helper.state(w,repo,bank)
            event={'hook_event_name':'Stop','session_id':'fixture-session','turn_id':'fixture-turn','stop_hook_active':False,'cwd':str(repo),'last_assistant_message':'fixture answer'}
            def invoke(value):
                p=subprocess.run([sys.executable,'-I',str(hook)],input=json.dumps(value),text=True,capture_output=True,env=w.env,timeout=10)
                self.assertEqual(p.returncode,0,p.stderr);return json.loads(p.stdout)
            answer=invoke(event);self.assertEqual(answer['decision'],'block');self.assertIn('fixture-turn',answer['reason']);self.assertIn('permissions',answer['reason'])
            self.assertEqual(invoke({**event,'stop_hook_active':True}),{})
            self.assertIn('pending',invoke({**event,'turn_id':None})['systemMessage'])
            self.assertEqual(before,helper.state(w,repo,bank))
    def test_nonrepository_and_uninitialized_are_quiet(self):
        with World() as w:
            hook=Path(__file__).resolve().parents[2]/'skills/agent-memory/hooks/codex_stop.py'
            for repo in (w.repository(),w.inside('outside')):
                repo.mkdir(exist_ok=True)
                event={'hook_event_name':'Stop','session_id':'session','turn_id':'turn','stop_hook_active':False,'cwd':str(repo),'last_assistant_message':'ok'}
                result=subprocess.run([sys.executable,'-I',str(hook)],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=10)
                self.assertEqual(json.loads(result.stdout),{})
