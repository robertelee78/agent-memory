import os,sys,json,subprocess,hashlib,time,tempfile
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path('/opt/agent-memory-worktrees/v009-quiet-boundaries');sys.path.insert(0,str(ROOT/'tests'))
from world import World
from publication_fixtures import PublicationFixture
proof=Path(tempfile.mkdtemp(prefix='am-v009-claude-pm-')).resolve();proof.chmod(0o700);print(proof,flush=True)
with World() as w:
 helper=PublicationFixture();repo,bank,peer,base=helper.fixture(w)
 seed="\n## Approved importer status decision\n\nThe project owner approved one top-level review-needed status with a typed reason, distinguishing uncertain input from incomplete analysis. Both retain supporting evidence. This is an existing approved project decision, not a new proposal.\n"
 (bank/'activeContext.md').write_text((bank/'activeContext.md').read_text()+seed)
 w.git('add','memory-bank',cwd=repo);w.git('commit','--only','-m','Seed existing approved PM decision','memory-bank',cwd=repo);w.git('push','origin','HEAD:main',cwd=repo)
 w.git('fetch','origin',cwd=peer);w.git('reset','--hard','origin/main',cwd=peer)
 baseline=helper.state(w,repo,bank)
 scripts=helper.pack/'scripts';hooks=helper.pack/'hooks'
 (proof/'source-hashes.json').write_text(json.dumps({str(p.relative_to(helper.pack)):hashlib.sha256(p.read_bytes()).hexdigest() for p in helper.pack.rglob('*') if p.is_file() and '__pycache__' not in str(p)},indent=2))
 binpath=w.inside('bin');binpath.mkdir()
 wrapper=binpath/'am';wrapper.write_text('#!'+sys.executable+'\nimport os,sys\nos.execve('+repr(sys.executable)+', ['+repr(sys.executable)+',"-I","-B",'+repr(str(scripts/'installer.py'))+']+sys.argv[1:], '+repr(w.env)+')\n');wrapper.chmod(0o700)
 def hook_wrapper(name):
  p=binpath/name;p.write_text('#!'+sys.executable+'\nimport os\nos.execve('+repr(sys.executable)+', ['+repr(sys.executable)+',"-I","-B",'+repr(str(hooks/name))+'], '+repr(w.env)+')\n');p.chmod(0o700);return str(p)
 settings=proof/'settings.json';settings.write_text(json.dumps({'hooks':{name:[{'hooks':[{'type':'command','command':hook_wrapper(script),'timeout':15, **({'asyncRewake':True} if name=='Stop' else {})}]}] for name,script in [('UserPromptSubmit','turn_hook.py'),('Stop','claude_stop.py')]}}))
 env=dict(os.environ);env['PATH']=str(binpath)+':'+env['PATH'];env['PYTHONDONTWRITEBYTECODE']='1';env.pop('CLAUDECODE',None);env['VOX_CONFIG_DIR']=str(w.paths['config']/'vox');env['VOX_DATA_DIR']=str(w.paths['data']/'vox')
 binary='/Users/robert.lee/.local/bin/claude'
 basecmd=[binary,'-p','--model','fable','--effort','low','--settings',str(settings),'--setting-sources','','--strict-mcp-config','--dangerously-skip-permissions','--output-format','stream-json','--verbose','--include-hook-events','--debug-file',str(proof/'debug.log')]
 session=None;results=[]
 prompts=json.loads(Path('/tmp/am-v009-native-final-prompts.json').read_text())[:2]
 for i,prompt in enumerate(prompts):
  if i==2:
   checkout=Path(w.env['AGENT_MEMORY_STATE_HOME'])/'writes'/hashlib.sha256(str(bank).encode()).hexdigest()
   injected=checkout/'transactions'/'native-qualification-retained-evidence';injected.mkdir(parents=True,exist_ok=True);(injected/'journal.json').write_text('{}')
  cmd=basecmd+(['--resume',session] if session else [])+['--',prompt];start=time.monotonic()
  try:r=subprocess.run(cmd,cwd=repo,env=env,capture_output=True,text=True,timeout=180)
  except subprocess.TimeoutExpired as e:
   (proof/f'{i}.stdout.jsonl').write_bytes(e.stdout or b'');(proof/f'{i}.stderr.txt').write_bytes(e.stderr or b'');results.append({'timeout':True,'seconds':time.monotonic()-start});break
  (proof/f'{i}.stdout.jsonl').write_text(r.stdout);(proof/f'{i}.stderr.txt').write_text(r.stderr)
  rows=[]
  for line in r.stdout.splitlines():
   try:rows.append(json.loads(line))
   except ValueError:pass
  for row in rows:
   if row.get('session_id'):session=row['session_id']
  result=[x for x in rows if x.get('type')=='result']
  results.append({'exit':r.returncode,'seconds':time.monotonic()-start,'session':session,'result':result})
  print(json.dumps({'index':i,'exit':r.returncode,'seconds':round(time.monotonic()-start,2),'result':[x.get('result') for x in result]}),flush=True)
  if r.returncode or not session:break
 pending=w.run([sys.executable,'-I','-B',scripts/'installer.py','claude-boundary','--repo',repo,'--list-pending'],check=False)
 (proof/'pending.json').write_text(pending.stdout)
 completed=list(w.paths['state'].rglob('*.done'));(proof/'completions.json').write_text(json.dumps([json.loads(p.read_text()) for p in completed],indent=2))
 (proof/'summary.json').write_text(json.dumps({'runs':results,'pending':json.loads(pending.stdout),'completion_count':len(completed),'workspace_count':len(list(w.paths['state'].rglob('private-workspaces/*'))),'retained_injected_evidence':injected.exists() if 'injected' in locals() else None,'bank_head_index_code_unchanged':helper.state(w,repo,bank)==baseline},indent=2));(proof/'controller.py').write_text(Path(__file__).read_text())
 print('finished',proof,flush=True)
