import os,sys,json,subprocess,hashlib,time,tempfile
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path('/opt/agent-memory-worktrees/v008-boundary-reliability');sys.path.insert(0,str(ROOT/'tests'))
from world import World
from publication_fixtures import PublicationFixture
proof=Path(tempfile.mkdtemp(prefix='am-v008-claude-live-')).resolve();proof.chmod(0o700);print(proof,flush=True)
with World() as w:
 helper=PublicationFixture();repo,bank,peer,base=helper.fixture(w);helper.github(w,repo,bank,peer)
 scripts=helper.pack/'scripts';hooks=helper.pack/'hooks'
 (proof/'source-hashes.json').write_text(json.dumps({str(p.relative_to(helper.pack)):hashlib.sha256(p.read_bytes()).hexdigest() for p in helper.pack.rglob('*') if p.is_file() and '__pycache__' not in str(p)},indent=2))
 binpath=w.inside('bin');binpath.mkdir()
 wrapper=binpath/'am';wrapper.write_text('#!'+sys.executable+'\nimport os,sys\nos.execve('+repr(sys.executable)+', ['+repr(sys.executable)+',"-I",'+repr(str(scripts/'installer.py'))+']+sys.argv[1:], '+repr(w.env)+')\n');wrapper.chmod(0o700)
 def hook_wrapper(name):
  p=binpath/name;p.write_text('#!'+sys.executable+'\nimport os\nos.execve('+repr(sys.executable)+', ['+repr(sys.executable)+',"-I",'+repr(str(hooks/name))+'], '+repr(w.env)+')\n');p.chmod(0o700);return str(p)
 settings=proof/'settings.json';settings.write_text(json.dumps({'hooks':{name:[{'hooks':[{'type':'command','command':hook_wrapper(script),'timeout':15, **({'asyncRewake':True} if name=='Stop' else {})}]}] for name,script in [('UserPromptSubmit','turn_hook.py'),('Stop','claude_stop.py')]}}))
 env=dict(os.environ);env['PATH']=str(binpath)+':'+env['PATH'];env['PYTHONDONTWRITEBYTECODE']='1';env.pop('CLAUDECODE',None)
 binary='/Users/robert.lee/.local/bin/claude'
 basecmd=[binary,'-p','--model','fable','--effort','low','--settings',str(settings),'--setting-sources','','--strict-mcp-config','--dangerously-skip-permissions','--output-format','stream-json','--verbose','--include-hook-events','--debug-file',str(proof/'debug.log')]
 session=None;results=[]
 prompts=['What is 2 + 2? This is a disposable arithmetic fixture, with no lasting project decision.','What is 3 + 3?','What is 3 + 3?']
 payload=''.join(json.dumps({'type':'user','message':{'role':'user','content':prompt},'parent_tool_use_id':None})+'\n' for prompt in prompts)
 start=time.monotonic()
 r=subprocess.run(basecmd+['--input-format','stream-json','--replay-user-messages'],input=payload,cwd=repo,env=env,capture_output=True,text=True,timeout=180)
 (proof/'queued.stdout.jsonl').write_text(r.stdout);(proof/'queued.stderr.txt').write_text(r.stderr)
 rows=[]
 for line in r.stdout.splitlines():
  try:rows.append(json.loads(line))
  except ValueError:pass
 results.append({'exit':r.returncode,'seconds':time.monotonic()-start,'results':[x.get('result') for x in rows if x.get('type')=='result']})
 print(json.dumps(results[-1]),flush=True)
 pending=w.run([sys.executable,'-I',scripts/'installer.py','claude-boundary','--repo',repo,'--list-pending'],check=False)
 (proof/'pending.json').write_text(pending.stdout)
 completed=list(w.paths['state'].rglob('*.done'));(proof/'completions.json').write_text(json.dumps([json.loads(p.read_text()) for p in completed],indent=2))
 (proof/'summary.json').write_text(json.dumps({'runs':results,'pending':json.loads(pending.stdout),'completion_count':len(completed)},indent=2));(proof/'controller.py').write_text(Path(__file__).read_text())
 print('finished',proof,flush=True)
