"""Manual public-channel canary. Never runs against the operator's home/banks."""
from pathlib import Path
import argparse,hashlib,json,os,platform,shutil,sys,time,traceback
sys.dont_write_bytecode=True
p=argparse.ArgumentParser();p.add_argument('operation',choices=['prepare-legacy','upgrade-legacy','upgrade-via-cli','fresh']);p.add_argument('--source',required=True);p.add_argument('--out',required=True);p.add_argument('--expected-source');a=p.parse_args()
root=Path(a.source).resolve();out=Path(a.out).resolve()
sys.path.insert(0,str(root/'tests'))
from world import World
from runtime_update_fixtures import snapshot,bank,receipt_path
from identity_fixtures import bind
url='https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh'
started=time.perf_counter()
if a.operation in {'upgrade-legacy','upgrade-via-cli'}:
 state=json.loads((out/'retained-world.json').read_text());owned=Path(state['root'])
 assert owned.parent==Path('/tmp').resolve() and owned.name.startswith('agent-memory-test-') and not owned.is_symlink()
 assert json.loads((owned/'.test-world.json').read_text())['token']==state['token']
 w=World.__new__(World);w.root=owned;w.token=state['token'];w.paths={k:Path(v) for k,v in state['paths'].items()};w.env=state['env'];w._children=[]
else:
 out.mkdir(mode=0o700,exist_ok=False);w=World();w._temp._finalizer.detach()
 # This explicitly separate public-network canary allows Git HTTPS. Ordinary
 # offline contracts keep their local-file-only environment unchanged.
 w.env['GIT_ALLOW_PROTOCOL']='https:file'
 (out/'retained-world.json').write_text(json.dumps({'root':str(w.root),'token':w.token,'paths':{k:str(v) for k,v in w.paths.items()},'env':w.env},indent=2)+'\n')
 (out/'retained-world.json').chmod(0o600)
commands=[]
def run(label,args):
 before=time.perf_counter();result=w.run(args,check=False,timeout=180,group=True)
 (out/(label+'.stdout')).write_text(result.stdout);(out/(label+'.stderr')).write_text(result.stderr)
 row={'label':label,'argv':[str(v) for v in args],'exit':result.returncode,'seconds':time.perf_counter()-before,
 'stdout_sha256':hashlib.sha256(result.stdout.encode()).hexdigest(),'stderr_sha256':hashlib.sha256(result.stderr.encode()).hexdigest()}
 commands.append(row);(out/(a.operation+'-commands.json')).write_text(json.dumps(commands,indent=2)+'\n')
 assert result.returncode==0,(label,result.returncode,result.stdout[-1800:],result.stderr[-1800:])
 return result
try:
 if a.operation not in {'upgrade-legacy','upgrade-via-cli'}:
  repo=w.repository('consumer');memory=bank(w,repo);bind(w,repo)
  (repo/'app.py').write_text('VALUE = 1\n');w.git('add','.',cwd=repo);w.git('commit','-m','Disposable public consumer',cwd=repo)
  (repo/'app.py').write_text('VALUE = 2  # staged consumer work\n');w.git('add','app.py',cwd=repo)
  configs={str(w.paths['codex']/'AGENTS.md'):'Keep codex user instructions.\n',str(w.paths['claude']/'CLAUDE.md'):'Keep claude user instructions.\n',str(w.paths['opencode']/'AGENTS.md'):'Keep opencode user instructions.\n'}
  for name,text in configs.items():Path(name).write_text(text)
  (out/'consumer-before.json').write_text(json.dumps({'repo':str(repo),'snapshot':snapshot(repo),'configs':configs},indent=2)+'\n')
 original=json.loads((out/'consumer-before.json').read_text());repo=Path(original['repo'])
 assert snapshot(repo)==original['snapshot']
 if a.operation=='upgrade-via-cli':
  old=json.loads(receipt_path(w).read_text());cli=Path(old['bin_dir'])/'am'
  assert old['source']['version']=='0.0.2' and old['source']['kind']=='managed'
  run('old-process-update',[cli,'update'])
  transition=json.loads(receipt_path(w).read_text());assert transition['source']['version']=='0.0.3' and transition['source']['kind']=='managed'
  interim=json.loads(run('first-transition-version-json',[cli,'--json','version']).stdout)
  assert interim['security']['publisher_authentication']=='not_established' and not interim['security']['signature_verification_performed']
  run('new-process-signed-adoption',[cli,'update'])
 else:
  run(a.operation+'-one-line',['bash','-o','pipefail','-c','curl -fsSL "$1" | bash','public-canary',url])
 receipt=json.loads(receipt_path(w).read_text());cli=Path(receipt['bin_dir'])/'am';source=Path(receipt['source']['path'])
 assert source.is_relative_to(w.root) and cli.is_relative_to(w.root)
 assert all(Path(t['path']).is_relative_to(w.root) for t in receipt['targets'])
 expected='0.0.2' if a.operation=='prepare-legacy' else '0.0.3'
 assert receipt['verified'] and receipt['source']['version']==expected,receipt['source']
 version=run(a.operation+'-version',[cli,'--version'])
 assert version.stdout.startswith('am '+expected+'\n') and not version.stdout.lstrip().startswith('{'),version.stdout
 assert snapshot(repo)==original['snapshot']
 if a.operation=='prepare-legacy':
  assert receipt['source']['kind']=='managed' and receipt['source']['remote']=='https://github.com/robertelee78/agent-memory.git' and receipt['source']['ref']=='main'
  (out/'legacy-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
  result={'prepared':True,'legacy_version':expected,'source_revision':receipt['source']['revision'],'world_retained':True,'actual_public_default_install':True,'platform':platform.platform(),'seconds':time.perf_counter()-started}
 else:
  assert a.expected_source and receipt['source']['kind']=='release' and receipt['source']['revision']==a.expected_source,receipt['source']
  assert not (source/'memory-bank').exists() and not (source/'.git').exists()
  assert {v.name for v in source.iterdir()}=={'skills','release-manifest.json','release-manifest.json.sshsig'},[v.name for v in source.iterdir()]
  diagnostic=json.loads(run(a.operation+'-version-json',[cli,'--json','version']).stdout)
  assert diagnostic['security']['publisher_authentication']=='verified' and diagnostic['security']['signature_verification_performed']
  checked=json.loads(run(a.operation+'-update-check',[cli,'--json','update','--check']).stdout);assert checked['check_complete'] and checked['eligible'] and checked['verified'] is False and not checked['update_available']
  updated=json.loads(run(a.operation+'-update',[cli,'--json','update']).stdout);assert updated['verified'] and not updated['changed']
  human=run(a.operation+'-status',[cli,'status','--repo',repo]);assert not human.stdout.lstrip().startswith('{')
  checked=json.loads(run(a.operation+'-uninstall-check',[cli,'--json','uninstall','--check']).stdout);assert checked['check_complete'] and checked['verified'] is False
  run(a.operation+'-uninstall',[cli,'uninstall']);assert not cli.exists() and snapshot(repo)==original['snapshot']
  for name,text in original['configs'].items():assert Path(name).read_text()==text
  result={'passed':True,'actual_public_default_install':a.operation!='upgrade-via-cli','source_commit':a.expected_source,'signed_publisher_verified':True,'development_memory_absent':True,'consumer_and_staged_code_preserved':True,'client_text_restored':True,'public_update_noop_verified':True,'uninstalled':True,'world_retained':False,'platform':platform.platform(),'seconds':time.perf_counter()-started}
  assert json.loads((w.root/'.test-world.json').read_text())['token']==w.token;shutil.rmtree(w.root)
 (out/(a.operation+'-result.json')).write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
except BaseException:
 (out/(a.operation+'-failure.txt')).write_text(traceback.format_exc());print(json.dumps({'passed':False,'world_retained':str(w.root)}),flush=True);raise
