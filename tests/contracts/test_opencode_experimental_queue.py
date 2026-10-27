"""Isolated source-window prototype; not an installed or accepted adapter."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which('node'), 'JavaScript runtime unavailable')
class ExperimentalQueueContract(unittest.TestCase):
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
let promptHook,contextHook,tool,queued=0;
const ctx={tool:{transform:async cb=>{cb({namespace(){},add(t){tool=t}});return {dispose(){}}}},
 session:{hook:async(name,cb)=>{if(name==='prompt')promptHook=cb;else contextHook=cb;return {dispose(){}}},
 synthetic:async()=>({id:'q'+(++queued)}),context:async()=>rows}};
await mod.default.setup(ctx);
promptHook({sessionID:'s',messageID:'u1'});await contextHook({sessionID:'s',messages:[{id:'u1'}]});
promptHook({sessionID:'s',messageID:'u1'});await contextHook({sessionID:'s',messages:[{id:'u1'}]});
promptHook({sessionID:'s',messageID:'u2'});await contextHook({sessionID:'s',messages:[{id:'u2'}]});
await contextHook({sessionID:'s',messages:[{id:'q1'}]});
checks.mismatch=JSON.parse((await tool.execute({source_id:'u2'},{sessionID:'s'})).content);
checks.firstTool=JSON.parse((await tool.execute({source_id:'u1'},{sessionID:'s'})).content);
await contextHook({sessionID:'s',messages:[{id:'q2'}]});
checks.secondTool=JSON.parse((await tool.execute({source_id:'u2'},{sessionID:'s'})).content);
await contextHook({sessionID:'s',messages:[{id:'q2'}]});checks.queued=queued;
process.stdout.write(JSON.stringify(checks));
'''
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)/'queue.mjs'
            shutil.copyfile(ROOT/'prototypes/opencode-v2-queue.experimental.js',target)
            proc=subprocess.run([shutil.which('node'),'--input-type=module','-e',source,str(target)],
                                capture_output=True,text=True,timeout=5)
        self.assertEqual(proc.returncode,0,proc.stderr)
        result=json.loads(proc.stdout)
        self.assertEqual(result['first']['final_id'],'a1')
        self.assertEqual(result['second']['final_id'],'a2')
        self.assertEqual(result['first']['status'],'observed')
        self.assertFalse(result['first']['source_completeness_verified'])
        self.assertEqual(result['firstTool']['session_id'],'s')
        self.assertEqual(result['secondTool']['source_id'],'u2')
        self.assertEqual(result['queued'],2)
        for name in ('missing','incomplete','compacted','oversized','mismatch'):
            self.assertEqual(result[name]['status'],'pending',name)
            self.assertFalse(result[name]['publication_verified'])


if __name__=='__main__':
    unittest.main()
