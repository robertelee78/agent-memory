"""Compact checks retain independent freshness and exact source evidence."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import uuid
from unittest.mock import patch
from world import World
import refresh_fixtures

PACK=Path(__file__).resolve().parents[2]/'skills/agent-memory'
sys.path.insert(0,str(PACK/'scripts'))
from agent_memory import turn_check, turn_sources, codex_boundary, claude_boundary, memory_refresh

class CompactChecks(refresh_fixtures.RefreshFixture):
    def authored(self,repo,harness,session,prompt):
        digest=hashlib.sha256(b'Actual transient status question').hexdigest()
        source=turn_sources.register(repo,harness,session,prompt,digest, **({'native_source_id':prompt} if harness=='claude' else {}))
        oid=source['observation_id']
        return {'session':session,'prompt':prompt,'observation_id':oid,'source_sha256':digest,
                'outcome':'no-learning','assessment':{'source_id':oid,'source_review':'Reviewed the actual transient status question; no durable claim.','candidates':[]}}

    def test_actual_cli_one_call_quiet_and_exact_legacy_receipt_scope(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);before=self.state(w,repo,bank)
            with patch.dict(os.environ,w.env,clear=True):
                for harness in ('codex','claude'):
                    value=self.authored(repo,harness,str(uuid.uuid4()),str(uuid.uuid4()))
                    args=[sys.executable,'-I','-B',str(self.pack/'scripts/installer.py'),harness+'-boundary','--repo',str(repo),'--refresh','--quiet']
                    args+=['--stdin'] if harness=='codex' else ['--input','-']
                    result=subprocess.run(args,input=json.dumps(value),text=True,capture_output=True,env=w.env,timeout=20)
                    self.assertEqual((result.returncode,result.stdout,result.stderr),(0,'',''))
                    legacy=(codex_boundary if harness=='codex' else claude_boundary).complete(repo,value)
                    self.assertTrue(legacy['verified']);self.assertNotEqual(legacy.get('schema'),turn_check.SCHEMA)
                    self.assertNotIn('refresh_verified',legacy)
            self.assertEqual(self.state(w,repo,bank),before)
            self.assertFalse(list(Path(w.env['AGENT_MEMORY_STATE_HOME']).rglob('private-workspaces')))

    def test_actual_assessment_refresh_failure_dedup_and_recovery(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            failure={'sharing_state':'pending-inspection','remote_freshness_verified':False,'failure_component':'refresh.state','failure_code':'fixture-unavailable','reason':'Actual state unavailable.','repair':'Inspect the retained fixture state.'}
            with patch.dict(os.environ,w.env,clear=True):
                value=self.authored(repo,'codex',str(uuid.uuid4()),str(uuid.uuid4()))
                for attempt in range(2):
                    with patch.object(memory_refresh,'current_result',return_value=failure):
                        result=turn_check.check(repo,value,codex_boundary.complete,'codex')
                    self.assertFalse(result['verified']);self.assertTrue(result['assessment_verified']);self.assertFalse(result['refresh_verified'])
                    out,err=io.StringIO(),io.StringIO()
                    with contextlib.redirect_stdout(out),contextlib.redirect_stderr(err):code=turn_check.emit(result,True)
                    self.assertEqual(code,1);self.assertEqual(out.getvalue(),'')
                    self.assertEqual(bool(err.getvalue()),attempt==0)
                    self.assertNotIn('source_review',err.getvalue())
                with patch.object(memory_refresh,'current_result',return_value=dict(failure,observed_authority_head='changed-irrelevant-diagnostic')):
                    repeated=turn_check.check(repo,value,codex_boundary.complete,'codex')
                self.assertFalse(repeated['notice_required'])
                bad=dict(value,source_sha256='f'*64)
                with patch.object(memory_refresh,'current_result',return_value=failure):
                    first_bad=turn_check.check(repo,bad,codex_boundary.complete,'codex')
                    repeated_bad=turn_check.check(repo,bad,codex_boundary.complete,'codex')
                    different_bad=turn_check.check(repo,dict(bad,source_sha256='e'*64),codex_boundary.complete,'codex')
                self.assertTrue(first_bad['notice_required']);self.assertFalse(repeated_bad['notice_required']);self.assertTrue(different_bad['notice_required'])
                out,err=io.StringIO(),io.StringIO()
                with contextlib.redirect_stdout(out),contextlib.redirect_stderr(err):self.assertEqual(turn_check.emit(first_bad,True),1)
                self.assertIn('source assessment pending',err.getvalue());self.assertIn('memory refresh pending',err.getvalue())
                self.assertIn('Inspect the retained fixture state.',err.getvalue());self.assertIn('matching boundary pending inventory',err.getvalue())
                self.assertEqual(len(err.getvalue().splitlines()),1)
                recovered=turn_check.check(repo,value,codex_boundary.complete,'codex')
                self.assertTrue(recovered['verified'],recovered)
                bad=dict(value,source_sha256='f'*64)
                result=turn_check.check(repo,bad,codex_boundary.complete,'codex')
                self.assertTrue(result['refresh_verified']);self.assertFalse(result['assessment_verified']);self.assertEqual(turn_check.exit_code(result),1)

    def test_batch_distinct_observations_and_partial_failure(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            with patch.dict(os.environ,w.env,clear=True):
                session,prompt=str(uuid.uuid4()),str(uuid.uuid4())
                values=[self.authored(repo,'codex',session,prompt) for _ in range(2)]
                values[1]['source_sha256']='e'*64
                result=turn_check.check(repo,{'schema':'agent-memory/turn-check-request-v1','assessments':values},codex_boundary.complete,'codex')
                self.assertFalse(result['verified']);self.assertEqual(result['assessment_count'],2)
                self.assertTrue(result['assessment']['results'][0]['verified']);self.assertFalse(result['assessment']['results'][1]['verified'])
                with self.assertRaises(ValueError):turn_check.requests({'schema':'agent-memory/turn-check-request-v1','assessments':[values[0],values[0]]})

    def test_native_combined_refresh_uses_protected_writable_state_without_review_echo(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);root=Path(w.env['AGENT_MEMORY_STATE_HOME']);root.chmod(0o775)
            request={'root':str(root),'session':'native-session','source':'native-source','repo':str(repo),'action':'enroll'}
            def invoke(value):
                r=subprocess.run([sys.executable,'-I','-B',str(PACK/'hooks/boundary_ledger.py')],input=json.dumps(value),text=True,capture_output=True,env=w.env,timeout=20)
                self.assertEqual(r.returncode,0,r.stdout+r.stderr);return json.loads(r.stdout)
            self.assertEqual(invoke(request)['status'],'enrolled')
            result=invoke(dict(request,action='assessed-no-learning',refresh_current=True,assessment={'source_id':'native-source','source_review':'Actual transient status request reviewed.','candidates':[]}))
            self.assertTrue(result['verified'],result);self.assertTrue(result['assessment_verified']);self.assertTrue(result['refresh_verified'])
            self.assertNotIn('source_review',json.dumps(result))
            self.assertEqual(root.stat().st_mode&0o777,0o775)
