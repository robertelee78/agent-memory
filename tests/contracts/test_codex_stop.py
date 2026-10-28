"""Quiet proactive completion and bounded source-bound Codex fallback."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
import uuid
from unittest.mock import patch
from world import World
from contracts import test_admission
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'skills/agent-memory/scripts'))
from agent_memory import turn_sources as core

class CodexStopContract(unittest.TestCase):
    def call(self,w,pack,adapter,event):
        r=subprocess.run([sys.executable,'-I',str(pack/'hooks'/('codex_'+adapter+'.py'))],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=15)
        self.assertEqual(r.returncode,0,r.stderr);return json.loads(r.stdout)
    def test_proactive_complete_nullable_final_late_steering_and_durable_wake_cap(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            s,t=str(uuid.uuid4()),str(uuid.uuid4())
            event={'hook_event_name':'Stop','session_id':s,'turn_id':t,'stop_hook_active':False,'cwd':str(repo),'last_assistant_message':None}
            def prompt(text):return self.call(w,helper.pack,'prompt',{**event,'hook_event_name':'UserPromptSubmit','prompt':text})
            prompt('ordinary no-learning question')
            with patch.dict(os.environ,w.env,clear=True):
                source=core.inspect(repo,'codex',s,t)['pending'][0]
                request={'session':s,'prompt':t,'observation_id':source['observation_id'],'source_sha256':source['source_sha256'],'outcome':'no-learning','assessment':{'source_id':source['observation_id'],'source_review':'Reviewed actual question; no durable learning.','candidates':[]}}
                receipt=core.verify_assessment(repo,request,source['observation_id'])
                core.complete(repo,'codex',s,t,source['observation_id'],source['source_sha256'],receipt)
            self.assertEqual(self.call(w,helper.pack,'stop',event),{})
            prompt('late steering source')
            first=self.call(w,helper.pack,'stop',event);self.assertEqual(first['decision'],'block');self.assertLessEqual(len(first['reason'].encode()),2048)
            self.assertNotIn('decision',self.call(w,helper.pack,'stop',event))
            prompt(first['reason'])
            with patch.dict(os.environ,w.env,clear=True):
                self.assertTrue(any(x['control_origin_hint'] for x in core.inspect(repo,'codex',s,t)['sources']))
            second=self.call(w,helper.pack,'stop',event);self.assertEqual(second['decision'],'block')
            prompt('another actual steering source')
            self.assertNotIn('decision',self.call(w,helper.pack,'stop',{**event,'stop_hook_active':True}))
    def test_nonrepository_uninitialized_and_missing_source(self):
        with World() as w:
            hook=Path(__file__).resolve().parents[2]/'skills/agent-memory'
            for repo in [w.repository(),w.inside('outside')]:
                repo.mkdir(exist_ok=True)
                event={'hook_event_name':'Stop','session_id':str(uuid.uuid4()),'turn_id':str(uuid.uuid4()),'stop_hook_active':False,'cwd':str(repo),'last_assistant_message':None}
                self.assertEqual(self.call(w,hook,'stop',event),{})
    def test_legacy_completion_remains_quiet_without_rewriting_source(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            s,t=str(uuid.uuid4()),str(uuid.uuid4());digest=hashlib.sha256(b'actual legacy source').hexdigest()
            with patch.dict(os.environ,w.env,clear=True):
                from agent_memory import bank_writes,scope_binding
                operations,_=bank_writes.state_paths(repo,bank,str(uuid.uuid4()));directory=operations.parent/'codex-boundaries';directory.mkdir(parents=True,mode=0o700)
                key=hashlib.sha256((s+':'+t).encode()).hexdigest()
                pending={'schema':'agent-memory/codex-boundary-pending-v1','project_id':scope_binding.require(repo)['project_id'],'session':s,'turn':t,'source_uuid':t,'source_sha256':digest}
                path=directory/(key+'.pending');path.write_text(json.dumps(pending));path.chmod(0o600)
            request={'session':s,'prompt':t,'source_uuid':t,'source_sha256':digest,'outcome':'no-learning','assessment':{'source_id':t,'source_review':'Reviewed actual retained legacy source; no durable lesson. '+'😀'*380,'candidates':[]}}
            inp=w.inside('legacy-assessment.json');inp.write_text(json.dumps(request))
            r=subprocess.run([sys.executable,'-I',str(helper.pack/'scripts/installer.py'),'codex-boundary','--repo',str(repo),'--input',str(inp)],text=True,capture_output=True,env=w.env,timeout=15)
            self.assertEqual(r.returncode,0,r.stdout+r.stderr)
            event={'hook_event_name':'Stop','session_id':s,'turn_id':t,'stop_hook_active':False,'cwd':str(repo),'last_assistant_message':None}
            self.assertEqual(self.call(w,helper.pack,'stop',event),{})

    def test_quiet_completion_surfaces_earlier_pending_once(self):
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            s,t=str(uuid.uuid4()),str(uuid.uuid4());digest=hashlib.sha256(b'current').hexdigest()
            with patch.dict(os.environ,w.env,clear=True):
                obs=core.register(repo,'codex',s,t,digest)
                value={'observation_id':obs['observation_id'],'source_sha256':digest,'outcome':'no-learning','assessment':{'source_id':obs['observation_id'],'source_review':'Reviewed actual current fixture request; no durable learning.','candidates':[]}}
                receipt=core.verify_assessment(repo,value,obs['observation_id']);core.complete(repo,'codex',s,t,obs['observation_id'],digest,receipt)
                core.register(repo,'codex',str(uuid.uuid4()),str(uuid.uuid4()),hashlib.sha256(b'earlier interrupted source').hexdigest())
            event={'hook_event_name':'Stop','session_id':s,'turn_id':t,'stop_hook_active':False,'cwd':str(repo),'last_assistant_message':None}
            first=self.call(w,helper.pack,'stop',event)
            self.assertIn('earlier source assessments remain pending',first['systemMessage']);self.assertNotIn('decision',first)
            self.assertEqual(self.call(w,helper.pack,'stop',event),{})
