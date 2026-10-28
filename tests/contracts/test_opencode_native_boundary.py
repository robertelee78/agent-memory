"""Native OpenCode inputs survive interruption and wake attempts remain bounded."""
import concurrent.futures
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from world import World
from publication_fixtures import PublicationFixture

HOOKS=Path(__file__).resolve().parents[2]/'skills/agent-memory/hooks'
spec=importlib.util.spec_from_file_location('native_opencode_ledger',HOOKS/'boundary_ledger.py')
ledger=importlib.util.module_from_spec(spec);spec.loader.exec_module(ledger)

class NativeLedger(unittest.TestCase):
    def test_enrollment_backlog_interruption_and_truthful_completion(self):
        with tempfile.TemporaryDirectory() as raw:
            root=os.path.realpath(raw)
            self.assertEqual(ledger.reserve(root,'native-session','native-source','enroll')['status'],'enrolled')
            self.assertEqual(ledger.reserve(root,'next-session','inventory','backlog')['sources'],[{'session_id':'native-session','source_id':'native-source'}])
            self.assertEqual(ledger.reserve(root,'native-session','native-source','inspect')['status'],'pending-turn')
            self.assertEqual(ledger.reserve(root,'native-session','native-source','enroll')['status'],'enrolled')
            done=ledger.reserve(root,'native-session','native-source','assessed-no-learning',assessment={'source_id':'native-source','source_review':'Reviewed actual native source and found no durable project claim.','candidates':[]})
            self.assertFalse(done['publication_verified'])
            self.assertEqual(ledger.reserve(root,'next-session','inventory','backlog')['pending_count'],0)
            self.assertEqual(ledger.reserve(root,'native-session','native-source','wake',fingerprint='a'*64)['status'],'assessed-no-learning')

    def test_fingerprint_once_and_hard_two_attempt_cap_survive_restart_and_contention(self):
        with tempfile.TemporaryDirectory() as raw:
            root=os.path.realpath(raw);ledger.reserve(root,'session','source','enroll')
            def invoke(_):
                try:return ledger.reserve(root,'session','source','wake',fingerprint='a'*64)
                except BlockingIOError:return {'status':'pending'}
            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:results=list(pool.map(invoke,range(8)))
            self.assertEqual(sum(v['status']=='reserved'for v in results),1)
            self.assertEqual(ledger.reserve(root,'session','source','wake',fingerprint='a'*64)['reason'],'wake-already-attempted')
            self.assertEqual(ledger.reserve(root,'session','source','wake',fingerprint='b'*64)['attempt'],2)
            self.assertEqual(ledger.reserve(root,'session','source','wake',fingerprint='c'*64)['reason'],'wake-cap')
            ledger.reserve(root,'session','next-native-source','enroll')
            self.assertEqual(ledger.reserve(root,'session','next-native-source','wake',fingerprint='c'*64)['attempt'],1)

    def test_native_backlog_exposes_legacy_uncertainty_without_inventing_ids(self):
        with tempfile.TemporaryDirectory() as raw:
            root=os.path.realpath(raw);ledger.reserve(root,'old','legacy-source')
            value=ledger.reserve(root,'new','inventory','backlog')
            self.assertEqual(value['legacy_pending_count'],1);self.assertEqual(value['sources'],[])
            directory=Path(root)/'experimental-boundary'/hashlib.sha256(b'old').hexdigest()
            next(directory.glob('*.intent')).write_text('partial')
            with self.assertRaises(ValueError):ledger.reserve(root,'new','inventory','backlog')

    def test_backlog_pagination_recovers_sources_beyond_first_sixteen(self):
        with tempfile.TemporaryDirectory() as raw:
            root=os.path.realpath(raw)
            for number in range(21):ledger.reserve(root,'original-session',f'source-{number:02}','enroll')
            first=ledger.reserve(root,'current-session','inventory','backlog')
            second=ledger.reserve(root,'current-session','inventory','backlog',offset=first['next_offset'])
            self.assertEqual(first['pending_count'],21);self.assertEqual(first['next_offset'],16)
            self.assertIsNone(second['next_offset'])
            self.assertEqual(first['inventory_sha256'],second['inventory_sha256'])
            self.assertEqual([v['source_id']for v in first['sources']+second['sources']],[f'source-{n:02}'for n in range(21)])
            with self.assertRaises(ValueError):ledger.reserve(root,'current-session','inventory','backlog',offset=-1)

    def test_state_inside_checkout_or_common_git_is_rejected_before_writes(self):
        with World() as w:
            fixture=PublicationFixture();repo,bank,peer,base=fixture.fixture(w)
            linked=w.inside('linked-native-session');w.git('worktree','add','-b','linked-native-session',str(linked),cwd=repo)
            for private,session_repo in ((repo,repo),(repo/'.git',repo),(bank,linked)):
                with self.assertRaisesRegex(ValueError,'outside repository'):
                    ledger.reserve(str(private),'native-session','native-source','enroll',repo=str(session_repo))
                self.assertFalse((private/'experimental-boundary').exists())

    def test_fifo_markers_and_locks_refuse_without_blocking(self):
        with tempfile.TemporaryDirectory() as raw:
            root=os.path.realpath(raw)
            ledger.reserve(root,'session','source','enroll')
            directory=Path(root)/'experimental-boundary'/hashlib.sha256(b'session').hexdigest()
            key=hashlib.sha256(json.dumps(['session','source'],separators=(',',':')).encode()).hexdigest()
            intent=directory/(key+'.intent');original=intent.read_bytes()
            for suffix,action in (('.intent','inspect'),('.wake-lock','wake'),('.assessed','inspect')):
                path=directory/(key+suffix)
                if path.exists():path.unlink()
                os.mkfifo(path,0o600)
                data={'root':root,'session':'session','source':'source','action':action,'fingerprint':'a'*64}
                result=subprocess.run([sys.executable,'-I',str(HOOKS/'boundary_ledger.py')],input=json.dumps(data),text=True,capture_output=True,timeout=2)
                self.assertNotEqual(result.returncode,0,result.stdout)
                path.unlink()
                if suffix=='.intent':intent.write_bytes(original);intent.chmod(0o600)

    def test_new_and_existing_enrollment_verify_namespace_before_acceptance(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as raw:
            root=os.path.realpath(raw);original_fsync=os.fsync;calls=0
            def swap(fd):
                nonlocal calls
                original_fsync(fd);calls+=1
                if calls==4:
                    path=Path(root)/'experimental-boundary';path.rename(Path(root)/'original-state');path.mkdir(mode=0o700)
            with patch.object(ledger.os,'fsync',side_effect=swap):
                with self.assertRaisesRegex(ValueError,'namespace changed'):ledger.reserve(root,'session','source','enroll')
            self.assertTrue(list((Path(root)/'original-state').rglob('*.intent')))
        with tempfile.TemporaryDirectory() as raw:
            root=os.path.realpath(raw);ledger.reserve(root,'session','source','enroll')
            with patch.object(ledger,'verify_namespace',side_effect=ValueError('namespace changed')):
                with self.assertRaisesRegex(ValueError,'namespace changed'):ledger.reserve(root,'session','source','enroll')

    @unittest.skipUnless(shutil.which('node'),'Node unavailable')
    def test_actual_plugin_enrolls_before_context_queues_once_and_recovers_native_session(self):
        code=r'''
import {pathToFileURL} from 'node:url';
const plugin=(await import(pathToFileURL(process.argv[1]))).default;
const repo=process.argv[2],hooks={},tools={},queues=[],sessions=new Map();
const ctx={tool:{transform:async fn=>{fn({namespace(){},add(x){tools[x.name]=x.execute}});return{dispose(){}}}},session:{
 hook:async(name,fn)=>{const prior=hooks[name];hooks[name]=async event=>{if(prior)await prior(event);await fn(event)};return{dispose(){}}},get:async()=>({location:{directory:repo}}),
 context:async({sessionID})=>sessions.get(sessionID),synthetic:async x=>{queues.push(x);return{id:x.id,payload:{text:x.text}}}
}};
const dispose=await plugin.setup(ctx);
const source={type:'user',id:'msg_original',text:'Explain the fixture value without creating memory.'};sessions.set('ses_original',[source]);
const prompt={messageID:source.id,sessionID:'ses_original'};await hooks.prompt(prompt);source.metadata=prompt.metadata;
const firstBacklog=JSON.parse((await tools.pending_sources({}, {sessionID:'ses_original'})).content);
await hooks.context({sessionID:'ses_original',messages:[{id:source.id}]});await hooks.context({sessionID:'ses_original',messages:[{id:source.id}]});
if(queues.length!==1||Buffer.byteLength(queues[0].text)>2048||!queues[0].text.includes('Preserve the substantive task answer'))throw Error('unbounded or duplicate queue');
const rows=sessions.get('ses_original');rows.push({type:'assistant',id:'answer',text:'The fixture value is seven.',finish:'stop',time:{completed:1}},
 {type:'synthetic',id:queues[0].id,text:queues[0].text,metadata:queues[0].metadata},{type:'assistant',id:'continuation'});
const quiet=await tools.source_ack({source_id:source.id,outcome:'no-learning',assessment:{source_id:source.id,source_review:'Reviewed the fixture explanation; no durable claim.',candidates:[]}}, {sessionID:'ses_original',messageID:'continuation'});
if(quiet.content!=='')throw Error('routine receipt body');
const outgoing=()=>({sessionID:'ses_original',model:{providerID:'baseten',id:'moonshotai/Kimi-K3'},messages:[{role:'user',id:source.id},{role:'assistant',id:'answer'},{role:'user',id:queues[0].id},{role:'assistant',id:'continuation'}],system:[],tools:{execute:{}},options:{}});
const terminal=outgoing();await hooks.context(terminal);
if(Object.keys(terminal.tools).length||terminal.options.stop?.length!==1||!terminal.system.at(-1)?.text.includes(terminal.options.stop[0]))throw Error('exact control not terminal-shaped');

const ack=JSON.parse((await tools.source_ack({source_id:source.id,diagnostic:true,outcome:'no-learning',assessment:{source_id:source.id,source_review:'Reviewed the fixture explanation; no durable claim.',candidates:[]}}, {sessionID:'ses_original',messageID:'continuation'})).content);
if(ack.schema!=='agent-memory/turn-check-v1'||!ack.assessment_verified||!ack.refresh_verified||!ack.verified||ack.publication_verified)throw Error('false completion: '+JSON.stringify(ack));
const interrupted={type:'user',id:'msg_interrupted',text:'Inspect the temporary fixture.'};rows.push(interrupted);
const next={messageID:interrupted.id,sessionID:'ses_original'};await hooks.prompt(next);interrupted.metadata=next.metadata;
const steered=outgoing();await hooks.context(steered);if(!steered.tools.execute||steered.options.stop)throw Error('new operator admission changed');
sessions.set('ses_later',[{type:'assistant',id:'later-assistant'}]);
const backlog=JSON.parse((await tools.pending_sources({}, {sessionID:'ses_later'})).content);
if(backlog.pending_count!==1||backlog.sources[0].session_id!=='ses_original'||backlog.sources[0].source_id!=='msg_interrupted')throw Error('native source lost');
const missing=JSON.parse((await tools.source_window({source_id:interrupted.id,recover:true,source_session_id:'ses_original'}, {sessionID:'ses_later',messageID:'later-assistant'})).content);
if(missing.status!=='pending')throw Error('interrupted source falsely complete');
rows.push({type:'assistant',id:'resumed-answer',text:'The fixture is temporary.',finish:'stop',time:{completed:2}});
const recovered=JSON.parse((await tools.source_ack({source_id:interrupted.id,recover:true,source_session_id:'ses_original',diagnostic:true,outcome:'no-learning',assessment:{source_id:interrupted.id,source_review:'Recovered original native source and resumed answer; no durable claim.',candidates:[]}}, {sessionID:'ses_later',messageID:'later-assistant'})).content);
if(!recovered.verified||!recovered.assessment_verified||recovered.session_id!=='ses_original'||recovered.source_id!==interrupted.id)throw Error('source reassigned');
await hooks.context({sessionID:'ses_original',messages:[{id:interrupted.id}]});
if(queues.length!==1)throw Error('completed source requeued');
dispose();process.stdout.write(JSON.stringify({firstBacklog,ack,backlog,missing,recovered,queues:queues.length,bytes:Buffer.byteLength(queues[0].text)}));
'''
        with World() as w:
            fixture=PublicationFixture();repo,bank,peer,base=fixture.fixture(w)
            result=subprocess.run([shutil.which('node'),'--input-type=module','-e',code,str(HOOKS/'opencode-v2.js'),str(repo)],env=w.env,text=True,capture_output=True,timeout=60)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            value=json.loads(result.stdout);self.assertEqual(value['queues'],1);self.assertLessEqual(value['bytes'],2048)
