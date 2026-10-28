# Retained incomplete apparatus: intentionally non-executable to preserve the time budget.
raise SystemExit("Incomplete redundant-page apparatus; use count-bytes-controller.py instead")
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
 registry=next(w.paths['state'].rglob('semantic-holds/*/registry.json'));backup=registry.with_name('recovery.json');baseline=json.loads(registry.read_text());template=copy.deepcopy(next(iter(baseline['records'].values())));project=view['binding']['project_id']
 (proof/'real-hold-template.json').write_text(json.dumps(baseline,indent=2))
 def bulk(count):
  seeded=copy.deepcopy(baseline);seeded['records']={}
  for i in range(count):
   row=copy.deepcopy(template);old=row['cause']['sources'];keys=[hashlib.sha256((key+':disposable-capacity-variant:'+str(i)).encode()).hexdigest() for key in old];row['cause']['sources']=keys;row['references']={b:row['references'][a] for a,b in zip(old,keys)};seeded['records'][holds.cause_id(row['cause'])]=row
  seeded.pop('checksum');return holds.seal(seeded)
 def sized(value,size):
  value=copy.deepcopy(value);remaining=size-len(holds.encoded(value))
  for row in value['records'].values():
   n=min(remaining,4096-len(row['cause']['reason'].encode()));row['cause']['reason']+='x'*n;remaining-=n
   if not remaining:break
  assert remaining==0,remaining
  value.pop('checksum');value=holds.seal(value);assert len(holds.encoded(value))==size;return value
 def attempt(label,seeded):
  holds.registry_value(seeded,repo,project);raw=holds.encoded(seeded);registry.write_bytes(raw);backup.write_bytes(raw)
  before=h.h.state(w,repo,bank);retained={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in registry.parent.iterdir() if p.is_file()}
  start=time.monotonic()
  try:
   _,initial=h.inspect(w,repo,label);path,view=h.inspect(w,repo,label+'-selected',h.select(initial))
   try:value=h.review(w,repo,path,view,True);status='accepted'
   except AssertionError:value=commands[-1]['stdout'];status='refused'
   after=json.loads(registry.read_text());row={'scenario':label,'existing_records':len(seeded['records']),'seed_bytes':len(raw),'seed_schema_valid':True,'status':status,'seconds':time.monotonic()-start,'result':value,'actual_records':len(after['records']),'actual_bytes':len(registry.read_bytes()),'bank_index_HEAD_preserved':before==h.h.state(w,repo,bank),'registry_recovery_preserved':retained=={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in registry.parent.iterdir() if p.is_file()}}
  except Exception as e:row={'scenario':label,'phase':'before-review','exception':repr(e),'last_command':commands[-1]};traceback.print_exc()
  results.append(row);(proof/'results.json').write_text(json.dumps(results,indent=2));print(json.dumps({k:v for k,v in row.items() if k!='result'}),flush=True);return row
 row=attempt('record-count-boundary',bulk(4095));delta=row['actual_bytes']-row['seed_bytes'];assert row['status']=='accepted' and row['actual_records']==4096
 attempt('record-count-excess',bulk(4096))
 row=attempt('store-bytes-boundary',sized(bulk(4095),holds.MAX_STORE-delta));assert row['status']=='accepted' and row['actual_bytes']==holds.MAX_STORE,row
 attempt('store-bytes-excess',sized(bulk(4095),holds.MAX_STORE-delta+1))
 (proof/'controller.py').write_text(Path(__file__).read_text())
print('finished',proof,flush=True)
