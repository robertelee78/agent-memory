"""Append-only sources and truthful per-observation completion."""
import hashlib
import json
import os
from pathlib import Path
import sys
import unittest
import uuid
from unittest.mock import patch
from world import World
from contracts import test_admission
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'skills/agent-memory/scripts'))
from agent_memory import turn_sources as core

class Sources(unittest.TestCase):
    def setup(self,w):
        helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
        return repo,str(uuid.uuid4()),str(uuid.uuid4())
    def register(self,repo,session,turn,text='actual request'):
        return core.register(repo,'codex',session,turn,hashlib.sha256(text.encode()).hexdigest())
    def ack(self,repo,s,t,o):
        value={'observation_id':o['observation_id'],'source_sha256':o['source_sha256'],'outcome':'no-learning','assessment':{'source_id':o['observation_id'],'source_review':'Reviewed the actual short fixture source; no durable project learning.','candidates':[]}}
        receipt=core.verify_assessment(repo,value,o['observation_id'])
        return core.complete(repo,'codex',s,t,o['observation_id'],o['source_sha256'],receipt)
    def test_distinct_and_identical_submissions_and_idempotent_ack(self):
        with World() as w:
            repo,s,t=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True):
                a=self.register(repo,s,t);b=self.register(repo,s,t,'steering');c=self.register(repo,s,t)
                self.assertEqual(len({x['observation_id'] for x in [a,b,c]}),3)
                self.ack(repo,s,t,a);self.assertEqual(len(core.inspect(repo,'codex',s,t)['pending']),2)
                self.assertTrue(self.ack(repo,s,t,a)['already_completed'])
                self.ack(repo,s,t,b);self.ack(repo,s,t,c)
                self.assertTrue(core.inspect(repo,'codex',s,t)['complete'])
                self.assertEqual(core.backlog(repo)['turns'],[])
                d=self.register(repo,s,t,'late steering')
                self.assertFalse(core.inspect(repo,'codex',s,t)['complete'])
                self.assertEqual(core.backlog(repo)['turns'][0]['pending'][0]['observation_id'],d['observation_id'])
    def test_registration_ignores_completion_lock_and_partial_slot_is_pending(self):
        with World() as w:
            repo,s,t=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True):
                a=self.register(repo,s,t)
                _,_,_,directory,_=core.namespace(repo,'codex',s,t)
                with core.locked(directory):
                    b=self.register(repo,s,t,'while completion held')
                    with self.assertRaises(ValueError):core.inspect(repo,'codex',s,t)
                (directory/'source-2.json').write_text('{partial');(directory/'source-2.json').chmod(0o600)
                self.ack(repo,s,t,a);self.ack(repo,s,t,b)
                result=core.inspect(repo,'codex',s,t)
                self.assertFalse(result['complete']);self.assertIn('source-reservation-unverifiable',result['faults'])
    def test_caps_fault_and_new_turn_backlog(self):
        with World() as w:
            repo,s,t=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True),patch.object(core,'MAX_SOURCES',2):
                a=self.register(repo,s,t);b=self.register(repo,s,t)
                with self.assertRaises(ValueError):self.register(repo,s,t,'not accepted')
                self.ack(repo,s,t,a);self.ack(repo,s,t,b)
                self.assertFalse(core.inspect(repo,'codex',s,t)['complete'])
                other=self.register(repo,str(uuid.uuid4()),str(uuid.uuid4()))
                backlog=core.backlog(repo)
                self.assertEqual(len(backlog['turns']),2)
                self.assertTrue(any('registration-fault-retained' in row['faults'] for row in backlog['turns']))
    def test_native_opaque_ids_and_private_reference(self):
        with World() as w:
            repo,_,_=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True):
                digest=hashlib.sha256(b'actual').hexdigest()
                obs=core.register(repo,'claude','native-session','native-prompt',digest,native_source_id='native-prompt',source_reference='/private/transcript.jsonl')
                self.assertEqual(core.get(repo,'claude','native-session','native-prompt',obs['observation_id'])['source_reference'],'/private/transcript.jsonl')
                with self.assertRaises(ValueError):core.complete(repo,'claude','native-session','native-prompt',obs['observation_id'],digest,{'verified':True})
    def test_encoded_completion_bound_and_wake_cap(self):
        with World() as w:
            repo,s,t=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True):
                obs=self.register(repo,s,t);inv=core.inspect(repo,'codex',s,t)
                self.assertTrue(core.reserve_wake(repo,'codex',s,t,inv['fingerprint'],'a'*64))
                self.assertFalse(core.reserve_wake(repo,'codex',s,t,inv['fingerprint'],'a'*64))
                self.register(repo,s,t,'second');inv=core.inspect(repo,'codex',s,t)
                self.assertTrue(core.reserve_wake(repo,'codex',s,t,inv['fingerprint'],'b'*64))
                self.register(repo,s,t,'third');inv=core.inspect(repo,'codex',s,t)
                self.assertFalse(core.reserve_wake(repo,'codex',s,t,inv['fingerprint'],'c'*64))
                value={'outcome':'no-learning','assessment':{'source_id':obs['observation_id'],'source_review':'x'*65000,'candidates':[]}}
                with self.assertRaises(ValueError):core.verify_assessment(repo,value,obs['observation_id'])

    def test_real_prompt_registration_steering_stdin_ack_and_cap_blocks_input(self):
        import subprocess
        with World() as w:
            helper=test_admission.AdmissionContract();repo,bank=helper.fixture(w)
            s,t=str(uuid.uuid4()),str(uuid.uuid4())
            def prompt(text):
                event={'hook_event_name':'UserPromptSubmit','session_id':s,'turn_id':t,'cwd':str(repo),'prompt':text}
                r=subprocess.run([sys.executable,'-I',str(helper.pack/'hooks/codex_prompt.py')],input=json.dumps(event),text=True,capture_output=True,env=w.env,timeout=15)
                self.assertEqual(r.returncode,0,r.stderr);return json.loads(r.stdout)
            self.assertIn('hookSpecificOutput',prompt('first actual input'))
            self.assertIn('hookSpecificOutput',prompt('same-turn steering'))
            self.assertIn('hookSpecificOutput',prompt('first actual input'))
            with patch.dict(os.environ,w.env,clear=True):
                current=core.inspect(repo,'codex',s,t);self.assertEqual(len(current['sources']),3)
                first=current['sources'][0]
            request={'session':s,'prompt':t,'observation_id':first['observation_id'],'source_sha256':first['source_sha256'],'outcome':'no-learning','assessment':{'source_id':first['observation_id'],'source_review':'Reviewed actual fixture input; ephemeral test source has no durable learning.','candidates':[]}}
            r=subprocess.run([sys.executable,'-I',str(helper.pack/'scripts/installer.py'),'codex-boundary','--repo',str(repo),'--stdin'],input=json.dumps(request),text=True,capture_output=True,env=w.env,timeout=15)
            self.assertEqual(r.returncode,0,r.stdout+r.stderr);self.assertTrue(json.loads(r.stdout)['verified'])
            with patch.dict(os.environ,w.env,clear=True):
                _,_,_,directory,_=core.namespace(repo,'codex',s,t)
                for i in range(3,128):
                    path=directory/('source-'+str(i)+'.json');path.write_text('{reserved');path.chmod(0o600)
            blocked=prompt('input must not pass capacity failure')
            self.assertEqual(blocked['decision'],'block')
            with patch.dict(os.environ,w.env,clear=True):
                self.assertIn('registration-fault-retained',core.inspect(repo,'codex',s,t)['faults'])

    def test_corrupt_receipt_and_cross_observation_assessment_cannot_complete(self):
        with World() as w:
            repo,s,t=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True):
                a=self.register(repo,s,t);b=self.register(repo,s,t)
                value={'observation_id':a['observation_id'],'source_sha256':a['source_sha256'],'outcome':'no-learning','assessment':{'source_id':a['observation_id'],'source_review':'Actually reviewed this source only.','candidates':[]}}
                receipt=core.verify_assessment(repo,value,a['observation_id'])
                with self.assertRaises(ValueError):core.complete(repo,'codex',s,t,b['observation_id'],b['source_sha256'],receipt)
                self.ack(repo,s,t,a);self.ack(repo,s,t,b)
                _,_,_,directory,_=core.namespace(repo,'codex',s,t)
                path=directory/(a['observation_id']+'.done');record=json.loads(path.read_text())
                for invalid in [{},{**record['receipt'],'publication_verified':True},{**record['receipt'],'assessment':{'source_id':b['observation_id'],'source_review':'wrong source','candidates':[]}}]:
                    path.write_text(json.dumps({**record,'receipt':invalid}));path.chmod(0o600)
                    state=core.inspect(repo,'codex',s,t)
                    self.assertFalse(state['complete']);self.assertIn('completion-unverifiable',state['faults'])

    def test_index_cleanup_restores_racing_append_and_unsafe_index_blocks(self):
        with World() as w:
            repo,s,t=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True):
                first=self.register(repo,s,t)
                _,_,base,_,_=core.namespace(repo,'codex',s,t)
                original=Path.unlink;created=[]
                def racing_unlink(path,*a,**kw):
                    if path.parent==base/'pending' and not created:
                        created.append(self.register(repo,s,t,'late input during cleanup'))
                    return original(path,*a,**kw)
                with patch.object(Path,'unlink',racing_unlink):self.ack(repo,s,t,first)
                backlog=core.backlog(repo)
                self.assertEqual(backlog['turns'][0]['pending'][0]['observation_id'],created[0]['observation_id'])
                ref=next((base/'pending').glob('*.json'));ref.write_text('{partial');ref.chmod(0o600)
                with self.assertRaises(ValueError):self.register(repo,str(uuid.uuid4()),str(uuid.uuid4()))

    def test_fifo_read_refuses_without_waiting_and_bad_json_index_blocks_input(self):
        import subprocess,time
        with World() as w:
            repo,s,t=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True):
                self.register(repo,s,t)
                _,_,base,_,_=core.namespace(repo,'codex',s,t)
                fifo=w.inside('fifo');os.mkfifo(fifo,0o600)
                script="import sys;sys.path.insert(0,sys.argv[1]);from agent_memory import turn_sources as c\ntry:c.read(sys.argv[2],4096)\nexcept ValueError:print('refused')\nelse:raise SystemExit(9)"
                result=subprocess.run([sys.executable,'-I','-c',script,str(Path(core.__file__).parent.parent),str(fifo)],env=w.env,text=True,capture_output=True,timeout=3)
                self.assertEqual(result.returncode,0,result.stderr);self.assertIn('refused',result.stdout)
                reference=next((base/'pending').glob('*.json'))
                for wrong in [{},'garbage']:
                    reference.write_text(json.dumps(wrong));reference.chmod(0o600)
                    with self.assertRaises(ValueError):self.register(repo,s,str(uuid.uuid4()))

    def test_index_hole_does_not_hide_later_bad_reference(self):
        with World() as w:
            repo,s,t=self.setup(w)
            with patch.dict(os.environ,w.env,clear=True):
                self.register(repo,s,t)
                _,_,base,_,_=core.namespace(repo,'codex',s,t)
                slot=base/'pending'/'0.json';later=base/'pending'/'1.json'
                slot.rename(later);later.write_text('{}');later.chmod(0o600)
                with self.assertRaises(ValueError):self.register(repo,s,str(uuid.uuid4()))
                self.assertFalse(slot.exists())
