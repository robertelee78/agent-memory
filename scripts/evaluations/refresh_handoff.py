"""Bounded public prepared-file refresh acceptance in disposable Worlds."""
import sys,json,copy,time,hashlib,tempfile,os,subprocess
if not __debug__: raise SystemExit("Acceptance requires assertions; rerun without optimization")
from pathlib import Path
from datetime import datetime,timezone
ROOT=Path(__file__).resolve().parents[2];sys.path.insert(0,str(ROOT/'tests'))
SOURCE=subprocess.check_output(['git','-C',str(ROOT),'rev-parse','HEAD'],text=True).strip()
from world import World
_world_run=World.run
def guarded_world_run(self,argv,*args,**kwargs):
 argv=list(argv)
 if Path(str(argv[0])).name.startswith('python'):
  if '-I' not in argv:argv.insert(1,'-I')
  if '-B' not in argv:argv.insert(1,'-B')
 return _world_run(self,argv,*args,**kwargs)
World.run=guarded_world_run
from refresh_fixtures import RefreshFixture
from contracts.test_retrieval import RetrievalContract
out=Path(tempfile.mkdtemp(prefix='am-refresh-handoff-',dir='/tmp'));out.chmod(0o700)
rows=[];cases=[];started=time.monotonic()
def digest(b):return hashlib.sha256(b).hexdigest()
def state(h,w,repo,bank):
 value=h.state(w,repo,bank)
 return {'bank':{k:digest(v) for k,v in value[0].items()},'code':digest(value[1]),'index':digest(value[2]),'HEAD':value[3].strip(),'branch':value[4].strip()}
def stamp(path):
 s=path.stat();return {'sha256':digest(path.read_bytes()),'bytes':s.st_size,'mtime_ns':s.st_mtime_ns,'inode':s.st_ino,'mode':oct(s.st_mode&0o777)}
def run(w,h,label,helper,*args):
 t=time.monotonic();argv=[sys.executable,'-I','-B',h.pack/'scripts'/helper,*args]
 p=w.run(argv,check=False,timeout=120);v=json.loads(p.stdout)
 rows.append({'label':label,'argv':[str(a) for a in argv],'exit':p.returncode,'seconds':time.monotonic()-t,'stdout':v,'stderr':p.stderr})
 (out/'commands.json').write_text(json.dumps(rows,indent=2))
 return p.returncode,v
def prepare(w,h,repo,bank,peer,label,imported=False):
 incoming=None
 if imported:_,incoming=h.imported(w,peer,'handoff-lesson','# Handoff lesson\n\nUse the verified direct prepared file.\n')
 ip=w.inside(label+'-inspection.json');rc,v=run(w,h,label+'-inspect','refresh_memory.py','inspect','--v2','--repo',repo,'--output',ip);assert rc==0,v
 view=json.loads(ip.read_text())['inspection']
 if view['pending_selections']:
  req=copy.deepcopy(view['request']);paths=sorted({p.relative_to(b).as_posix() for b in [bank,peer/'memory-bank'] for p in b.rglob('*.md')})
  req['selections']={k:{'searches':['handoff','prepared file'],'paths':paths,'ids':[],'complete':True,'uncertain':False} for k in view['changed']}
  rp=w.inside(label+'-selection.json');rp.write_text(json.dumps(req));sp=w.inside(label+'-selected.json')
  rc,v=run(w,h,label+'-select','reconcile_memory.py','inspect-v2','--repo',repo,'--input',rp,'--output',sp);assert rc==0,v;view=json.loads(sp.read_text())
 assert not view['pending_selections']
 j={'schema':'agent-memory/semantic-sharing-judgment-v2','request':view['request'],'binding':view['binding'],'manifest_sha256':view['manifest_sha256'],'pages':[p['sha256'] for p in view['pages']],'pairs':[{'left':a,'right':b,'classification':'related','reason':'Complete mechanical fixture source comparison retaining imported content.'} for a,b in view['required_pairs']],'dependencies':{},'consolidations':[],'rationale':'Verify existing prepared-file handoff, not model semantic-quality acceptance.','recorded_at':datetime.now(timezone.utc).isoformat()}
 jp=w.inside(label+'-judgment.json');jp.write_text(json.dumps(j));proofpath=w.inside(label+'-reviewed.json')
 rc,v=run(w,h,label+'-review','reconcile_memory.py','review-v2','--repo',repo,'--input',jp,'--output',proofpath);assert rc==0,v
 proof=json.loads(proofpath.read_text())['proof'];request=w.inside(label+'-request.json');request.write_text(json.dumps({'proof':proof,'recall':RetrievalContract().request(view['binding']['project_id'],query='handoff',include_context=True)}))
 path=w.inside(label+'-prepared.json');rc,summary=run(w,h,label+'-prepare','refresh_memory.py','prepare','--v2','--repo',repo,'--input',request,'--output',path);assert rc==0,summary
 return path,json.loads(path.read_text()),summary,incoming
for label,imported in [('noop',False),('import',True),('local-change',True),('authority-change',True)]:
 with World() as w:
  h=RefreshFixture();repo,bank,peer,_=h.fixture(w);path,full,summary,incoming=prepare(w,h,repo,bank,peer,label,imported)
  before=state(h,w,repo,bank);file_before=stamp(path)
  if label=='noop':
   def all_metadata():
    roots={'git':Path(w.git('rev-parse','--absolute-git-dir',cwd=repo).stdout.strip()),'private-state':w.paths['state']}
    return {name:{str(p.relative_to(directory)):digest(p.read_bytes()) for p in directory.rglob('*') if p.is_file()} for name,directory in roots.items()}
   mutations=[('outer-schema',lambda x:x.__setitem__('schema','foreign-schema')),('outer-operation',lambda x:x.__setitem__('operation','apply')),('outer-stage',lambda x:x.__setitem__('sharing_state','refreshed')),('outer-delta',lambda x:x.__setitem__('changed_memory_paths',['nonexistent.md'])),('outer-blocked',lambda x:x.__setitem__('blocked_paths',['nonexistent.md'])),('outer-repo-pin',lambda x:x.__setitem__('repo_root','/unrelated/repository')),('outer-project-pin',lambda x:x.__setitem__('project_id','00000000-0000-4000-8000-000000000001')),('integer-bool',lambda x:x.__setitem__('remote_freshness_verified',1)),('false-completion',lambda x:x.__setitem__('refresh_applied',True)),('integer-mutation-bool',lambda x:x.__setitem__('canonical_writes_performed',0)),('false-availability',lambda x:x.__setitem__('local_memory_available',False)),('non-list-conflicts',lambda x:x.__setitem__('conflicts','not-a-list'))]
   for name,mutate in mutations:
    for private_output in (False,True):
     bad=copy.deepcopy(full);mutate(bad);bp=w.inside(name+str(private_output)+'.json');bp.write_text(json.dumps(bad));outpath=w.inside(name+str(private_output)+'-result.json');metadata_before=all_metadata()
     rc,v=run(w,h,name+str(private_output),'refresh_memory.py','apply','--v2','--input',bp,*(['--output',outpath] if private_output else []))
     assert rc==1 and v['failure_code']=='refresh-prepared-envelope-invalid' and v['failure_component']=='apply.prepared-envelope',(name,v)
     assert v['canonical_writes_performed'] is False and v['git_metadata_writes_performed'] is False and not v['refresh_applied']
     assert not outpath.exists() and all_metadata()==metadata_before and state(h,w,repo,bank)==before
     cases.append({'case':name,'with_private_output':private_output,'refused':True,'exact_failure_code':v['failure_code'],'git_and_entire_private_state_preserved':True})
   for name,mutate in [('operation',lambda x:x['proposal'].__setitem__('operation','capture')),('project',lambda x:x['proposal'].__setitem__('project_id','00000000-0000-4000-8000-000000000001')),('source-pin',lambda x:x['proposal']['refresh_evidence']['proof']['judgment']['binding'].__setitem__('authority_head','0'*40))]:
    bad=copy.deepcopy(full);mutate(bad);badpath=w.inside(name+'.json');badpath.write_text(json.dumps(bad));rc,v=run(w,h,name,'refresh_memory.py','apply','--v2','--input',badpath,'--output',w.inside(name+'-result.json'));assert rc!=0 and state(h,w,repo,bank)==before,(name,v);cases.append({'case':name,'refused':True,'state_preserved':True,'failure_code':v.get('failure_code')})
   bad=w.inside('artifact-summary.json');bad.write_text(json.dumps(summary));rc,v=run(w,h,'artifact-summary','refresh_memory.py','apply','--v2','--input',bad,'--output',w.inside('summary-result.json'));assert rc!=0 and state(h,w,repo,bank)==before;cases.append({'case':'artifact-summary','refused':True,'state_preserved':True})
   rc,v=run(w,h,'unsafe-output','refresh_memory.py','apply','--v2','--input',path,'--output',repo/'unsafe-output.json');assert rc!=0 and not (repo/'unsafe-output.json').exists() and state(h,w,repo,bank)==before;cases.append({'case':'unsafe-output','refused':True,'state_preserved':True})
  if label=='local-change':(bank/'activeContext.md').write_text((bank/'activeContext.md').read_text()+'\nKeep a newer local correction.\n')
  if label=='authority-change':
   target=peer/'memory-bank'/'projectbrief.md';target.write_text(target.read_text()+'\nNew authority evidence.\n');w.git('add','memory-bank',cwd=peer);w.git('commit','-m','Advance authority before apply',cwd=peer);w.git('push','origin','HEAD:main',cwd=peer)
  expected_before=state(h,w,repo,bank);ap=w.inside(label+'-applied.json');rc,v=run(w,h,label+'-apply','refresh_memory.py','apply','--v2','--input',path,'--output',ap)
  after=state(h,w,repo,bank);assert stamp(path)==file_before
  if label.endswith('change'):
   assert rc!=0 and after==expected_before,(label,v);cases.append({'case':label,'refused':True,'before':expected_before,'after':after,'prepared_stamp_unchanged':True});continue
  assert rc==0,v;applied=json.loads(ap.read_text());assert applied['refresh_applied'] and applied['remote_freshness_verified'] and applied['relevant_imports_read'];assert {k:v for k,v in before.items() if k!='bank'}=={k:v for k,v in after.items() if k!='bank'}
  assert applied['canonical_writes_performed']==imported;assert bool(applied['receipt']['changed_memory_paths'])==imported
  if imported:assert (bank/incoming).exists() and 'verified direct prepared file' in (bank/incoming).read_text()
  for mode in ['full-retry','inner-compat']:
   inputpath=path
   if mode=='inner-compat':inputpath=w.inside('inner.json');inputpath.write_text(json.dumps(full['proposal']))
   rc,v=run(w,h,label+'-'+mode,'refresh_memory.py','apply','--v2','--input',inputpath,'--output',w.inside(label+'-'+mode+'-applied.json'));assert rc==0 and state(h,w,repo,bank)==after,(label,mode,v)
  cases.append({'case':label,'prepare_changed_memory_paths':full['changed_memory_paths'],'apply_flags':{k:applied[k] for k in ['sharing_state','refresh_applied','canonical_writes_performed','remote_freshness_verified','relevant_imports_read','publication_verified','feature_code_freshness_verified']},'receipt_changed_memory_paths':applied['receipt']['changed_memory_paths'],'full_retry_and_inner_compat':True,'before':before,'after':after,'prepared_stamp_before':file_before,'prepared_stamp_after':stamp(path)})
result={'controller_sha256':digest(Path(__file__).read_bytes()),'passed':True,'seconds':time.monotonic()-started,'source_commit':SOURCE,'runtime_sha256':{str(p.relative_to(ROOT)):digest(p.read_bytes()) for p in [ROOT/'skills/agent-memory/scripts/agent_memory/memory_refresh.py',ROOT/'skills/agent-memory/scripts/agent_memory/bank_transactions.py']},'cases':cases,'all_calls_I_B':True,'command_count':len(rows),'actual_profile_modified':False}
(out/'controller.py').write_bytes(Path(__file__).read_bytes())
(out/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps({'proof':str(out),'passed':True,'seconds':result['seconds'],'commands':len(rows),'cases':len(cases)}))
