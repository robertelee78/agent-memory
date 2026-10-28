import os,sys,json,shutil,subprocess,time,tempfile,threading,queue,hashlib
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path('/opt/agent-memory-worktrees/v008-boundary-reliability');sys.path.insert(0,str(ROOT/'tests'))
from world import World
from contracts import test_sharing_recovery
from unittest.mock import patch
sys.path.insert(0,str(ROOT/'skills/agent-memory/scripts'))
from agent_memory import turn_sources
proof=Path(tempfile.mkdtemp(prefix='am-v008-codex-live-')).resolve();print('proof',proof,flush=True)
deadline=time.monotonic()+450
with World() as w:
 helper=test_sharing_recovery.SharingRecoveryContract();repo,bank,peer,base=helper.fixture(w)
 ch=w.paths['home']/'.codex';ch.mkdir(mode=0o700)
 auth=Path('/Users/robert.lee/.codex/auth.json')
 if auth.exists():shutil.copy2(auth,ch/'auth.json');(ch/'auth.json').chmod(0o600)
 denied=w.inside('no-tools-thread.json')
 deny=w.inside('deny_tools.py')
 deny.write_text("import json,sys\nfrom pathlib import Path\ne=json.load(sys.stdin)\np=Path("+repr(str(denied))+")\nif p.exists() and json.loads(p.read_text()).get('thread')==e.get('session_id'):\n print(json.dumps({'hookSpecificOutput':{'hookEventName':'PreToolUse','permissionDecision':'deny','permissionDecisionReason':'No-tools fixture: preserve the requested task answer; memory assessment remains pending.'}}))\nelse: print('{}')\n")
 hooks={'hooks':{name:[{'hooks':[{'type':'command','command':f'{sys.executable} -I "{helper.pack}/hooks/codex_{adapter}.py"'}]}] for name,adapter in [('UserPromptSubmit','prompt'),('Stop','stop')]}}
 hooks['hooks']['PreToolUse']=[{'hooks':[{'type':'command','command':f'{sys.executable} -I "{deny}"'}]}]
 (ch/'hooks.json').write_text(json.dumps(hooks));(ch/'config.toml').write_text('model = "gpt-6.1-sol"\nmodel_reasoning_effort = "low"\n')
 binary=w.paths['home']/'bin';binary.mkdir();am=binary/'am';am.write_text(f'#!/bin/sh\nexec {sys.executable} -I "{helper.pack}/scripts/installer.py" "$@"\n');am.chmod(0o700)
 env=dict(w.env);env['PATH']=str(binary)+':'+w.env['PATH'];env.pop('CODEX_HOME',None)
 hashes={str(p.relative_to(helper.pack)):hashlib.sha256(p.read_bytes()).hexdigest() for p in helper.pack.rglob('*') if p.is_file() and '__pycache__' not in str(p)}
 (proof/'source-hashes.json').write_text(json.dumps(hashes,indent=2))
 stderr=(proof/'stderr.txt').open('w');events=(proof/'events.jsonl').open('w');q=queue.Queue();responses={};notices=[];rid=0
 child=subprocess.Popen(['/Users/robert.lee/.local/bin/codex','--no-daemon','app-server','--stdio'],env=env,cwd=repo,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True,bufsize=1)
 def collect():
  for line in child.stdout:
   try:v=json.loads(line)
   except ValueError:continue
   events.write(json.dumps(v)+'\n');events.flush();q.put(v)
  q.put(None)
 threading.Thread(target=collect,daemon=True).start()
 def send(method,params=None,notify=False):
  global rid
  rid+=1;value={'method':method,'params':params or {}}
  if not notify:value['id']=rid
  child.stdin.write(json.dumps(value)+'\n');child.stdin.flush();return rid
 def pump():
  remaining=deadline-time.monotonic()
  if remaining<=0:raise TimeoutError('Controller450-second deadline')
  v=q.get(timeout=min(remaining,30))
  if v is None:raise RuntimeError('Appserver exited')
  if 'method' in v and 'id' in v:
   child.stdin.write(json.dumps({'id':v['id'],'error':{'code':-32000,'message':'Unexpected interactive request in bounded fixture'}})+'\n');child.stdin.flush()
  elif 'id' in v:responses[v['id']]=v
  else:notices.append(v)
 def rpc(method,params=None):
  request=send(method,params)
  while request not in responses:pump()
  v=responses.pop(request)
  if 'error' in v:raise RuntimeError(method+': '+json.dumps(v['error']))
  return v.get('result')
 def wait_turn(thread,turn):
  while True:
   for v in notices:
    if v.get('method')=='turn/completed' and v.get('params',{}).get('threadId')==thread and v['params'].get('turn',{}).get('id')==turn:return v
   pump()
 def text(s):return [{'type':'text','text':s,'text_elements':[]}]
 summary={'model':'gpt-6.1-sol','client':'actual app-server0.165.0','steps':[]}
 try:
  rpc('initialize',{'clientInfo':{'name':'am-v008-proof','version':'0.0.8'},'capabilities':{'experimentalApi':True}});send('initialized',notify=True)
  listed=rpc('hooks/list',{'cwds':[str(repo)]})
  entries=[h for row in listed['data'] for h in row['hooks']]
  edits=[{'keyPath':'hooks.state.'+json.dumps(h['key'])+'.trusted_hash','value':h['currentHash'],'mergeStrategy':'upsert'} for h in entries]
  rpc('config/batchWrite',{'edits':edits,'reloadUserConfig':True})
  trusted=rpc('hooks/list',{'cwds':[str(repo)]})
  summary['hook_trust']=[{'event':h['eventName'],'status':h['trustStatus']} for row in trusted['data'] for h in row['hooks']]
  if any(h['status']!='trusted' for h in summary['hook_trust']):raise RuntimeError('Fixture hooks not trusted')
  thread=rpc('thread/start',{'cwd':str(repo),'model':'gpt-6.1-sol','approvalPolicy':'never','sandbox':'danger-full-access','ephemeral':True})['thread']['id']
  first=rpc('turn/start',{'threadId':thread,'input':text('This is an isolated arithmetic/status fixture with no new project decision. Before answering, run a single two-second local wait to give the controller time to deliver native steering. Then answer 2+2 in one short sentence. Assess actual input observations through the installed memory skill before final; do not narrate routine memory bookkeeping.')})['turn']['id']
  for steering in ['Also state that this is a fixture result.','Also state that this is a fixture result.','Keep the answer short; the arithmetic remains 2+2.']:
   response=rpc('turn/steer',{'threadId':thread,'expectedTurnId':first,'input':text(steering)})
   summary['steps'].append({'kind':'native-steer','expected_turn':first,'response':response,'sha256':hashlib.sha256(steering.encode()).hexdigest()})
  summary['steps'].append({'kind':'first-completed','event':wait_turn(thread,first)})
  with patch.dict(os.environ,w.env,clear=True):summary['first_inventory']=turn_sources.inspect(repo,'codex',thread,first)
  second=rpc('turn/start',{'threadId':thread,'input':text('Second ordinary fixture question: what is 3+3? Reply in one short sentence. This introduces no durable project requirement; routine memory completion should stay quiet.')})['turn']['id']
  summary['steps'].append({'kind':'second-completed','event':wait_turn(thread,second)})
  with patch.dict(os.environ,w.env,clear=True):summary['second_inventory']=turn_sources.inspect(repo,'codex',thread,second)
  no_tools=rpc('thread/start',{'cwd':str(repo),'model':'gpt-6.1-sol','approvalPolicy':'never','sandbox':'danger-full-access','ephemeral':True})['thread']['id'];denied.write_text(json.dumps({'thread':no_tools}))
  third=rpc('turn/start',{'threadId':no_tools,'input':text('No-tools fixture: answer 7+4. The fixture denies all tool use. Preserve your substantive arithmetic answer if memory work cannot run; say only a concise pending qualification if necessary, and do not loop.')})['turn']['id']
  summary['steps'].append({'kind':'no-tools-completed','event':wait_turn(no_tools,third)})
  with patch.dict(os.environ,w.env,clear=True):summary['no_tools_inventory']=turn_sources.inspect(repo,'codex',no_tools,third)
  summary['terminal']=True
 except Exception as exc:
  summary['error']=type(exc).__name__+': '+str(exc);summary['terminal']=False
 finally:
  child.terminate()
  try:child.wait(timeout=5)
  except subprocess.TimeoutExpired:child.kill();child.wait(timeout=5)
  events.close();stderr.close()
  # Synthetic fixture only; no auth/config bytes are retained.
  (proof/'controller.py').write_text(Path(__file__).read_text());(proof/'summary.json').write_text(json.dumps(summary,indent=2))
  print(json.dumps({'proof':str(proof),'terminal':summary.get('terminal'),'error':summary.get('error'),'steps':len(summary['steps'])}),flush=True)
