"""Installed source-window and queue contract."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which('node'), 'JavaScript runtime unavailable')
class OpenCodeQueueContract(unittest.TestCase):
    def test_queue_binds_distinct_turns_and_fails_closed_on_missing_sources(self):
        source = r'''
import {pathToFileURL} from 'node:url';
const mod=await import(pathToFileURL(process.argv[1]));
const user=(id,text)=>({id,type:'user',text});
const final=(id,text)=>({id,type:'assistant',finish:'stop',time:{completed:1},content:[{type:'text',text}]});
const rows=[user('u1','yellow'),final('a1','done'),user('u2','blue'),final('a2','done')];
const checks={first:JSON.parse(mod.sourceWindow(rows,'u1','s').content),
 second:JSON.parse(mod.sourceWindow(rows,'u2','s').content),
 missing:JSON.parse(mod.sourceWindow(rows,'missing','s').content),
 incomplete:JSON.parse(mod.sourceWindow([user('u1','yellow')],'u1','s').content),
 compacted:JSON.parse(mod.sourceWindow([user('u1','yellow'),{type:'compaction'}],'u1','s').content),
 oversized:JSON.parse(mod.sourceWindow([user('u1','x'.repeat(20000)),final('a1','done')],'u1','s').content)};
const large=[user('u1','yellow'),{id:'i1',type:'assistant',content:'a'.repeat(9000)},{id:'i2',type:'assistant',content:'b'.repeat(9000)},final('a1','done')];
checks.manifest=JSON.parse(mod.sourceWindow(large,'u1','s').content);
checks.retrieved=JSON.parse(mod.sourceWindow(large,'u1','s','i1',checks.manifest.omitted_records[0].sha256).content);
checks.wrongHash=JSON.parse(mod.sourceWindow(large,'u1','s','i1','bad').content);
const huge=[user('u1','yellow'),{id:'i1',type:'assistant',content:'a'.repeat(20000)},final('a1','done')];
const hugeManifest=JSON.parse(mod.sourceWindow(huge,'u1','s').content);
checks.largeRecord=JSON.parse(mod.sourceWindow(huge,'u1','s','i1',hugeManifest.omitted_records[0].sha256).content);
let offset=0, chunks=[];do {const c=JSON.parse(mod.sourceWindow(huge,'u1','s','i1',hugeManifest.omitted_records[0].sha256,offset).content);chunks.push(Buffer.from(c.chunk,'base64'));offset=c.next_offset} while(offset!==null);
checks.chunked=JSON.parse(Buffer.concat(chunks).toString('utf8'));
checks.badOffset=JSON.parse(mod.sourceWindow(huge,'u1','s','i1',hugeManifest.omitted_records[0].sha256,999999).content);
let promptHook,contextHook,tool;const queued=new Map(), registered={};
const ctx={tool:{transform:async cb=>{cb({namespace(){},add(t){registered[t.name]=t;if(t.name==='source_window')tool=t}});return {dispose(){}}}},
 session:{hook:async(name,cb)=>{if(name==='prompt')promptHook=cb;else contextHook=cb;return {dispose(){}}},
 get:async()=>({location:{directory:process.env.TEST_REPO}}), synthetic:async(v)=>{if(!queued.has(v.id))queued.set(v.id,{id:v.id,payload:{text:v.text},metadata:v.metadata});return queued.get(v.id)},context:async()=>rows}};
await mod.default.setup(ctx);
const p1={sessionID:'s',messageID:'u1'};promptHook(p1);rows[0].metadata=p1.metadata;
await contextHook({sessionID:'s',messages:[{id:'u1'}]});await contextHook({sessionID:'s',messages:[{id:'u1'}]});
const p2={sessionID:'s',messageID:'u2'};promptHook(p2);rows[2].metadata=p2.metadata;
await contextHook({sessionID:'s',messages:[{id:'u2'}]});
const q1=queued.get(mod.syntheticId('s','u1')),q2=queued.get(mod.syntheticId('s','u2'));
rows.push({id:q1.id,type:'synthetic',metadata:q1.metadata},{id:'t1',type:'assistant'});
checks.mismatch=JSON.parse((await tool.execute({source_id:'u2'},{sessionID:'s',messageID:'t1'})).content);
checks.firstTool=JSON.parse((await tool.execute({source_id:'u1'},{sessionID:'s',messageID:'t1'})).content);
rows.push({id:q2.id,type:'synthetic',metadata:q2.metadata},{id:'t2',type:'assistant'});
checks.secondTool=JSON.parse((await tool.execute({source_id:'u2'},{sessionID:'s',messageID:'t2'})).content);
const q1pos=rows.findIndex(row=>row.id===q1.id);const removed=rows.splice(q1pos,1);
checks.recovered=JSON.parse((await tool.execute({source_id:'u1',recover:true},{sessionID:'s',messageID:'t2'})).content);rows.splice(q1pos,0,...removed);
if(registered.source_ack){checks.recoveryAck=JSON.parse((await registered.source_ack.execute({source_id:'u1',outcome:'no-learning',recover:true},{sessionID:'s',messageID:'t2'})).content);checks.ack=JSON.parse((await registered.source_ack.execute({source_id:'u2',outcome:'no-learning'},{sessionID:'s',messageID:'t2'})).content);checks.badAck=JSON.parse((await registered.source_ack.execute({source_id:'u1',outcome:'no-learning'},{sessionID:'s',messageID:'t2'})).content);checks.pubAck=JSON.parse((await registered.source_ack.execute({source_id:'u2',outcome:'published'},{sessionID:'s',messageID:'t2'})).content);}
checks.queued=queued.size;
process.stdout.write(JSON.stringify(checks));
'''
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)/'queue.mjs'
            import os
            env = dict(os.environ)
            (Path(tmp)/'second').mkdir()
            env['AGENT_MEMORY_STATE_HOME']=str((Path(tmp)/'second').resolve())
            shutil.copyfile(ROOT/'skills/agent-memory/hooks/opencode-v2-queue.js',target)
            env['TEST_REPO']=str(Path(tmp).resolve())
            (Path(tmp)/'boundary_ledger.py').write_text('import json,sys,os\nr=json.load(sys.stdin)\nassert r.get("repo")==os.environ["TEST_REPO"]\nprint(json.dumps({"status":"assessed-no-learning" if r.get("action")=="assessed-no-learning" else ("pending-turn" if r.get("action")=="inspect" else "reserved")}))\n')
            supported=subprocess.run([shutil.which('node'),'--input-type=module','-e',source,str(target)],
                                     capture_output=True,text=True,timeout=5,env=env)
            proc = supported
            self.assertEqual(supported.returncode,0,supported.stderr)
            self.assertEqual(json.loads(supported.stdout)['queued'],2)
            self.assertEqual(json.loads(supported.stdout)['ack']['status'],'assessed-no-learning')
            self.assertEqual(json.loads(supported.stdout)['recovered']['source_id'],'u1')
            self.assertEqual(json.loads(supported.stdout)['recoveryAck']['source_id'],'u1')
            self.assertEqual(json.loads(supported.stdout)['badAck']['status'],'pending')
            self.assertEqual(json.loads(supported.stdout)['pubAck']['reason'],'session-repository-unavailable')
            probe = r"import {pathToFileURL} from 'node:url'; const m=await import(pathToFileURL(process.argv[1])); const r=[{id:'u',type:'user',text:'x'},{id:'a',type:'assistant',finish:'stop',time:{completed:1},content:[{type:'tool',output:{truncated:true}}]}]; process.stdout.write(m.sourceWindow(r,'u','s').content)"
            trunc=subprocess.run([shutil.which('node'),'--input-type=module','-e',probe,str(target)],capture_output=True,text=True,timeout=5)
            self.assertEqual(json.loads(trunc.stdout)['reason'],'source-incomplete')
        self.assertEqual(proc.returncode,0,proc.stderr)
        result=json.loads(proc.stdout)
        self.assertEqual(result['first']['final_id'],'a1')
        self.assertEqual(result['second']['final_id'],'a2')
        self.assertEqual(result['first']['status'],'observed')
        self.assertFalse(result['first']['source_completeness_verified'])
        self.assertEqual(result['firstTool']['session_id'],'s')
        self.assertEqual(result['secondTool']['source_id'],'u2')
        self.assertEqual(result['queued'],2)
        self.assertEqual(len(result['manifest']['omitted_records']),2)
        self.assertFalse(result['manifest']['source_completeness_verified'])
        self.assertEqual(result['retrieved']['record']['id'],'i1')
        self.assertEqual(result['wrongHash']['status'],'pending')
        self.assertEqual(result['largeRecord']['reason'],'record-too-large')
        self.assertEqual(len(result['chunked']['content']),20000)
        self.assertEqual(result['badOffset']['reason'],'record-offset-invalid')
        for name in ('missing','incomplete','compacted','oversized','mismatch'):
            self.assertEqual(result[name]['status'],'pending',name)
            self.assertFalse(result[name]['publication_verified'])


if __name__=='__main__':
    unittest.main()
