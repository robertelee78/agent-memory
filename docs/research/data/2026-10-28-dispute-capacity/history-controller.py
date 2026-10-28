import os,sys,json,hashlib,time,copy,traceback,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4];sys.path[:0]=[str(ROOT/'tests'),str(ROOT/'skills/agent-memory/scripts')]
from world import World
from contracts.test_known_holds_v2 import KnownHoldsVersionTwoContract
from contracts import test_semantic_sharing as fixtures
from contracts.test_comparison import render
from agent_memory import sharing_holds as holds
proof=Path(tempfile.mkdtemp(prefix='am-v010-capacity-proof-')).resolve();proof.chmod(0o700)
print(str(proof),flush=True)
class Repro(KnownHoldsVersionTwoContract):
 def invoke(self,w,*args,expected=0):
  started=time.monotonic();r=w.run([sys.executable,'-I','-B',self.h.pack/'scripts/reconcile_memory.py',*args],check=False,timeout=180);v=json.loads(r.stdout)
  commands.append({'args':[str(a) for a in args],'exit':r.returncode,'seconds':time.monotonic()-started,'stdout':v,'stderr':r.stderr})
  (proof/'commands.json').write_text(json.dumps(commands,indent=2))
  if expected is not None:self.assertEqual(r.returncode,expected,(v,r.stderr))
  return v
commands=[];results=[]
with World() as w:
 h=Repro();h.h=fixtures.SemanticSharingContract();repo,bank,peer,base=h.h.fixture(w)
 _,local=h.h.add(bank,'wall-local','# Atrium wall\n\nUse yellow.\n');_,landed=h.h.add(peer/'memory-bank','wall-peer','# Atrium wall\n\nUse blue.\n');h.claims={local,landed};h.h.publish(w,peer)
 _,initial=h.inspect(w,repo,'initial');path,view=h.inspect(w,repo,'selected',h.select(initial));h.review(w,repo,path,view,True)
 feedback,feedback_path=h.h.add(bank,'clarification','# Clarification\n\nThe detecting developer confirmed blue.\n');feedback['kind']='user-feedback';feedback['evidence']=['user:synthetic-blue-clarification'];(bank/feedback_path).write_text(render(feedback,'# Clarification\n\nThe detecting developer confirmed blue.\n'))
 def renewed(name):
  _,initial=h.inspect(w,repo,name);request=h.select(initial)
  for selection in request['selections'].values():selection['paths'].append(feedback_path)
  path,view=h.inspect(w,repo,name+'-selected',request);evidence=next(row['key'] for row in view['sources'] if row['path']==feedback_path)
  resolutions={cause['id']:{'basis':'operator-clarification','evidence':[evidence],'reason':'Actual attributed fixture clarification resolves retained wall colors.'} for cause in view['known_causes']}
  return path,view,resolutions
 path,view,resolutions=renewed('resolved');h.review(w,repo,path,view,False,resolutions)
 registry=next(w.paths['state'].rglob('semantic-holds/*/registry.json'));backup=registry.with_name('recovery.json');baseline=json.loads(registry.read_text());record=next(iter(baseline['records'].values()));resolution=copy.deepcopy(record['resolution']);project=view['binding']['project_id']
 (proof/'real-resolution-template.json').write_text(json.dumps(baseline,indent=2))
 for count in (4095,4096):
  seeded=copy.deepcopy(baseline);next(iter(seeded['records'].values()))['resolution_history']=[copy.deepcopy(resolution) for _ in range(count)];seeded.pop('checksum');seeded=holds.seal(seeded);holds.registry_value(seeded,repo,project);raw=holds.encoded(seeded);registry.write_bytes(raw);backup.write_bytes(raw)
  (bank/feedback_path).write_text(render(feedback,'# Clarification\n\nThe detecting developer specified blue; clarification renewal '+str(count)+'.\n'))
  before=h.h.state(w,repo,bank);retained={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in registry.parent.iterdir() if p.is_file()}
  try:
   path,view,resolutions=renewed('history-'+str(count));start=time.monotonic()
   try:value=h.review(w,repo,path,view,False,resolutions);status='accepted'
   except AssertionError as e:value=commands[-1]['stdout'];status='refused'
   after=json.loads(registry.read_text());row={'scenario':'resolution-history','existing_entries':count,'attempted_entries':count+1,'seed_bytes':len(raw),'seed_schema_valid':True,'status':status,'seconds':time.monotonic()-start,'result':value,'actual_history_count':len(next(iter(after['records'].values())).get('resolution_history',[])),'bank_index_HEAD_preserved':before==h.h.state(w,repo,bank),'registry_recovery_preserved':retained=={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in registry.parent.iterdir() if p.is_file()}}
  except Exception as e:row={'scenario':'resolution-history','existing_entries':count,'phase':'before-review','exception':repr(e),'last_command':commands[-1]};traceback.print_exc()
  results.append(row);(proof/'results.json').write_text(json.dumps(results,indent=2));print(json.dumps({k:v for k,v in row.items() if k!='result'}),flush=True)
 (proof/'controller.py').write_text(Path(__file__).read_text())
print('finished',proof,flush=True)
