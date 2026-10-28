import os,sys,json,shutil,subprocess,time,tempfile,threading,queue,hashlib
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path('/opt/agent-memory-worktrees/v009-quiet-boundaries');sys.path.insert(0,str(ROOT/'tests'))
from world import World
from contracts import test_sharing_recovery
from unittest.mock import patch
sys.path.insert(0,str(ROOT/'skills/agent-memory/scripts'))
from agent_memory import turn_sources
proof=Path(tempfile.mkdtemp(prefix='am-v009-codex-live-')).resolve();print('proof',proof,flush=True)
deadline=time.monotonic()+700
with World() as w:
 helper=test_sharing_recovery.SharingRecoveryContract();repo,bank,peer,base=helper.fixture(w)
 from record_fixtures import record_text,project_id
 decision='The importer uses one needs-review status with a typed reason and supporting evidence, preserving uncertainty versus incomplete analysis as distinct reasons.'
 body='# Importer review decision\n\n'+decision+'\n'
 (bank/'topics').mkdir(exist_ok=True)
 (bank/'topics/importer-review.md').write_text(record_text(body,project_id((bank/'MEMORY.md').read_text()),kind='project-decision'))
 router=bank/'MEMORY.md';router.write_text(router.read_text().replace('\n## Omitted contexts','| [topics/importer-review.md](<topics/importer-review.md>) | Approved importer status decision | repository | active | When discussing importer status |\n\n## Omitted contexts'))
 (bank/'activeContext.md').write_text('# Current focus\n\nApproved importer product decision: '+decision+' This decision is already recorded in topics/importer-review.md.\n')
 w.git('reset','--','code.py',cwd=repo);w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Seed already-approved disposable PM decision',cwd=repo);w.git('push','origin','HEAD:main',cwd=repo);w.git('add','code.py',cwd=repo)
 before=helper.state(w,repo,bank)

 ch=w.paths['home']/'.codex';ch.mkdir(mode=0o700)
 auth=Path('/Users/robert.lee/.codex/auth.json')
 if auth.exists():shutil.copy2(auth,ch/'auth.json');(ch/'auth.json').chmod(0o600)
 denied=w.inside('no-tools-thread.json')
 deny=w.inside('deny_tools.py')
 deny.write_text("import json,sys\nfrom pathlib import Path\ne=json.load(sys.stdin)\np=Path("+repr(str(denied))+")\nif p.exists() and json.loads(p.read_text()).get('thread')==e.get('session_id'):\n print(json.dumps({'hookSpecificOutput':{'hookEventName':'PreToolUse','permissionDecision':'deny','permissionDecisionReason':'No-tools fixture: preserve the requested task answer; memory assessment remains pending.'}}))\nelse: print('{}')\n")
 hooks={'hooks':{name:[{'hooks':[{'type':'command','command':f'{sys.executable} -I -B "{helper.pack}/hooks/codex_{adapter}.py"'}]}] for name,adapter in [('UserPromptSubmit','prompt'),('Stop','stop')]}}
 hooks['hooks']['PreToolUse']=[{'hooks':[{'type':'command','command':f'{sys.executable} -I -B "{deny}"'}]}]
 (ch/'hooks.json').write_text(json.dumps(hooks));(ch/'config.toml').write_text('model = "gpt-6.1-sol"\nmodel_reasoning_effort = "low"\n')
 binary=w.paths['home']/'bin';binary.mkdir();am=binary/'am';am.write_text(f'#!/bin/sh\nexec {sys.executable} -I -B "{helper.pack}/scripts/installer.py" "$@"\n');am.chmod(0o700)
 env=dict(w.env);env['PATH']=str(binary)+':'+w.env['PATH'];env.pop('CODEX_HOME',None);env['VOX_CONFIG_DIR']=str(w.paths['config']/'vox');env['VOX_DATA_DIR']=str(w.paths['data']/'vox')
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
  
  try:v=q.get(timeout=min(remaining,30))
  except queue.Empty:return
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
  rpc('initialize',{'clientInfo':{'name':'am-v009-proof','version':'0.0.9'},'capabilities':{'experimentalApi':True}});send('initialized',notify=True)
  listed=rpc('hooks/list',{'cwds':[str(repo)]})
  entries=[h for row in listed['data'] for h in row['hooks']]
  edits=[{'keyPath':'hooks.state.'+json.dumps(h['key'])+'.trusted_hash','value':h['currentHash'],'mergeStrategy':'upsert'} for h in entries]
  rpc('config/batchWrite',{'edits':edits,'reloadUserConfig':True})
  trusted=rpc('hooks/list',{'cwds':[str(repo)]})
  summary['hook_trust']=[{'event':h['eventName'],'status':h['trustStatus']} for row in trusted['data'] for h in row['hooks']]
  if any(h['status']!='trusted' for h in summary['hook_trust']):raise RuntimeError('Fixture hooks not trusted')
  thread=rpc('thread/start',{'cwd':str(repo),'model':'gpt-6.1-sol','approvalPolicy':'never','sandbox':'danger-full-access','ephemeral':True})['thread']['id']
  prompts=[
   'We previously approved one needs-review status for the importer, with a typed reason and supporting evidence. Keep that decision. In two sentences, explain how it covers uncertain input versus incomplete analysis and ask me one refinement question. Do not change implementation files.',
   'Keep the distinction inside the typed reason. What evidence should the user see for each reason? Give one short recommendation and ask one next refinement question; no implementation changes.',
   'We are still discussing presentation, without changing the approved status design. Give one concise recommendation for the next product review and one question. No implementation changes.',
   'Keep the existing importer status decision. Which of those two reasons should we explain first in the UI, and why? Give one recommendation and one refinement question; no implementation changes.'
  ]
  for i,prompt in enumerate(prompts):
   if i==2:
    checkout=Path(w.env['AGENT_MEMORY_STATE_HOME'])/'writes'/hashlib.sha256(str(bank).encode()).hexdigest()
    orphan=checkout/'transactions'/'unknown'/'journal.json';orphan.parent.mkdir(parents=True,mode=0o700);orphan.write_text('{}')
   deadline=time.monotonic()+180
   start=time.monotonic()
   turn=rpc('turn/start',{'threadId':thread,'input':text(prompt)})['turn']['id']
   completed=wait_turn(thread,turn)
   with patch.dict(os.environ,w.env,clear=True):inventory=turn_sources.inspect(repo,'codex',thread,turn)
   summary['steps'].append({'index':i,'turn':turn,'seconds':time.monotonic()-start,'event':completed,'inventory':inventory,'prompt_sha256':hashlib.sha256(prompt.encode()).hexdigest(),'feature_and_bank_preserved':helper.state(w,repo,bank)==before})
   print(json.dumps({'index':i,'turn':turn,'seconds':round(time.monotonic()-start,2),'complete':completed['params']['turn']['status'],'preserved':summary['steps'][-1]['feature_and_bank_preserved']}),flush=True)
  summary['workspaces']=len(list(Path(w.env['AGENT_MEMORY_STATE_HOME']).rglob('private-workspaces')))
  summary['completed_receipts']=[json.loads(p.read_text()) for p in Path(w.env['AGENT_MEMORY_STATE_HOME']).rglob('*.done')]
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
