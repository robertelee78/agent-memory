"""Exact-control request shaping never cancels or changes a real user turn."""
import json
from pathlib import Path
import shutil
import subprocess
import unittest

HOOK=Path(__file__).resolve().parents[2]/'skills/agent-memory/hooks/opencode-v2-queue.js'

class TerminalRequest(unittest.TestCase):
    @unittest.skipUnless(shutil.which('node'),'Node unavailable')
    def test_exact_source_and_epoch_only_on_qualified_model(self):
        code=r'''
import {pathToFileURL} from 'node:url';import{createHash}from'node:crypto';
const{terminalContext,syntheticId,sourceWindow,controlEpochs,canArmTerminal}=await import(pathToFileURL(process.argv[1]));
const session='ses_case',source='msg_source',control=syntheticId(session,source),schema='agent-memory/opencode-boundary-v1';
const rows=[{type:'user',id:source,text:'Actual original question'},
{type:'assistant',id:'answer',text:'Actual answer',finish:'stop',time:{completed:1}},
{type:'synthetic',id:control,text:'Actual retained control',metadata:{agent_memory_boundary:{schema,source_id:source}}},
{type:'assistant',id:'ack'}];
const hash=x=>createHash('sha256').update(x).digest('hex');
const marker={session_id:session,source_id:source,control_id:control,assistant_id:'ack',epoch:4,refresh_verified:true,assessment_verified:true,
control_sha256:hash(JSON.stringify(rows[2])),source_window_sha256:hash(sourceWindow(rows,source,session).content)};
const event=()=>({sessionID:session,model:{providerID:'baseten',id:'moonshotai/Kimi-K3'},
messages:[{role:'user',id:source},{role:'assistant',id:'answer'},{role:'user',id:control},{role:'assistant',id:'ack'}],
system:[],tools:{execute:{description:'Code mode',input:{}}},options:{temperature:0.2}});
const green=event();if(!terminalContext(green,rows,marker,4)||Object.keys(green.tools).length||green.options.stop.length!==1||!green.system.at(-1).text.includes(green.options.stop[0]))throw Error('exact check failed');
const cases=[
['epoch',(e,r,m)=>{},marker,5],
['pending refresh',(e,r,m)=>{},{...marker,refresh_verified:false},4],
['assessment-only receipt',(e,r,m)=>{},{...marker,refresh_verified:undefined},4],
['restart',(e,r,m)=>{},undefined,4],
['other provider',(e)=>e.model.providerID='anthropic',marker,4],
['other model',(e)=>e.model.id='GLM-5.3',marker,4],
['new model input',(e)=>e.messages.push({role:'user',id:'msg_new'}),marker,4],
['new native steering',(e,r)=>r.push({type:'user',id:'msg_new',text:'New operator input'}),marker,4],
['different native tail',(e,r)=>r.push({type:'assistant',id:'other'}),marker,4],
['changed source',(e,r)=>r[0].text='Changed original evidence',marker,4],
['changed control',(e,r)=>r[2].text='Changed control',marker,4],
['duplicate control',(e,r)=>r.splice(2,0,structuredClone(r[2])),marker,4],
['missing evidence',(e,r)=>r.splice(0,1),marker,4],
['different outgoing assistant',(e)=>e.messages.at(-1).id='other',marker,4],
];
for(const[name,mutate,m,epoch]of cases){const e=event(),r=structuredClone(rows);mutate(e,r,m);const before=JSON.stringify(e);if(terminalContext(e,r,m,epoch)||JSON.stringify(e)!==before)throw Error(name+' changed unqualified context')}
const epochs=controlEpochs(128),old=epochs.observe(session);
for(let i=0;i<128;i++)epochs.observe('ses_other_'+i);
if(epochs.current(session)!==null)throw Error('epoch not evicted');
const newer=epochs.observe(session);
const verified={verified:true,refresh_verified:true,assessment_verified:true},context={sessionID:session,messageID:'ack'},observation={session_id:session};
if(old===newer||canArmTerminal(verified,{},context,observation,old,epochs.current(session)))throw Error('eviction/inflight/new-admission ABA rearmed stale control');
if(canArmTerminal(verified,{},context,observation,null,null))throw Error('restart absent epoch armed');
if(canArmTerminal(verified,{recover:true},context,observation,newer,newer))throw Error('recovery armed');
if(canArmTerminal({...verified,refresh_verified:false},{},context,observation,newer,newer))throw Error('pending refresh armed');
if(!canArmTerminal(verified,{},context,observation,newer,newer))throw Error('current generation not eligible');
process.stdout.write(JSON.stringify({passed:cases.length+6}));
'''
        result=subprocess.run([shutil.which('node'),'--input-type=module','-e',code,str(HOOK)],text=True,capture_output=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertEqual(json.loads(result.stdout)['passed'],20)

    @unittest.skipUnless(shutil.which('node'),'Node unavailable')
    def test_actual_pending_refresh_and_recovery_do_not_arm(self):
        from world import World
        import publication_fixtures
        code=r'''
import{pathToFileURL}from'node:url';import{renameSync}from'node:fs';
const plugin=(await import(pathToFileURL(process.argv[1]))).default;
const hooks={},tools={},queues=[],rows=[],repo=process.argv[2];
const ctx={tool:{transform:async f=>{f({namespace(){},add(t){tools[t.name]=t.execute}});return{dispose(){}}}},session:{
hook:async(n,f)=>{hooks[n]=f;return{dispose(){}}},get:async()=>({location:{directory:repo}}),context:async()=>rows,
synthetic:async x=>{queues.push(x);return{id:x.id,payload:{text:x.text}}}}};
const dispose=await plugin.setup(ctx),session='ses_negative',source={type:'user',id:'msg_original',text:'Actual transient fixture question'};
rows.push(source);const prompt={sessionID:session,messageID:source.id};await hooks.prompt(prompt);source.metadata=prompt.metadata;
await hooks.context({sessionID:session,messages:[{id:source.id}]});
rows.push({type:'assistant',id:'answer',text:'Actual fixture answer',finish:'stop',time:{completed:1}},
{type:'synthetic',id:queues[0].id,text:queues[0].text,metadata:queues[0].metadata},{type:'assistant',id:'ack'});
const input={source_id:source.id,outcome:'no-learning',assessment:{source_id:source.id,source_review:'Reviewed the transient fixture question; no durable claim.',candidates:[]}};
const failed=JSON.parse((await tools.source_ack(input,{sessionID:session,messageID:'ack'})).content);
if(failed.verified!==false||failed.refresh_verified!==false||failed.assessment_verified!==true)throw Error('false refresh health');
const outgoing=()=>({sessionID:session,model:{providerID:'baseten',id:'moonshotai/Kimi-K3'},messages:[{role:'user',id:source.id},{role:'assistant',id:'answer'},{role:'user',id:queues[0].id},{role:'assistant',id:'ack'}],tools:{execute:{}},options:{},system:[]});
const pending=outgoing();await hooks.context(pending);if(!pending.tools.execute||pending.options.stop)throw Error('pending refresh armed termination');
renameSync(process.argv[3],process.argv[4]);
const recovered=await tools.source_ack({...input,recover:true,source_session_id:session},{sessionID:session,messageID:'ack'});
if(recovered.content!=='')throw Error('recovery failed: '+recovered.content);
const recovery=outgoing();await hooks.context(recovery);if(!recovery.tools.execute||recovery.options.stop)throw Error('recovery armed termination');
dispose();process.stdout.write(JSON.stringify({pending:true,recovery:true}));
'''
        with World() as w:
            fixture=publication_fixtures.PublicationFixture();repo,bank,peer,base=fixture.fixture(w)
            before=fixture.state(w,repo,bank)
            remote=Path(w.git('remote','get-url','origin',cwd=repo).stdout.strip());away=remote.with_name('offline-authority.git');remote.rename(away)
            result=subprocess.run([shutil.which('node'),'--input-type=module','-e',code,str(HOOK),str(repo),str(away),str(remote)],text=True,capture_output=True,env=w.env,timeout=60)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            self.assertEqual(json.loads(result.stdout),{'pending':True,'recovery':True})
            self.assertEqual(fixture.state(w,repo,bank),before)
