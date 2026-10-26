"""Authored daily-use workload through real public commands and local Git transport.

This controller supplies known fixture meaning, never a general semantic-quality
claim. Each cohort is one part of daily-use-v1, not whole-story acceptance.
"""
from pathlib import Path
import argparse,copy,gzip,hashlib,json,os,shutil,stat,sys,threading,time,traceback,uuid
from contextlib import ExitStack
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime,timezone
sys.dont_write_bytecode=True
p=argparse.ArgumentParser();p.add_argument('--root',required=True);p.add_argument('--out',required=True)
p.add_argument('--sessions',type=int,required=True);p.add_argument('--lessons',type=int,required=True)
p.add_argument('--recovery-controls',action='store_true');p.add_argument('--semantic-scenarios',action='store_true');p.add_argument('--workers',type=int,default=4);p.add_argument('--source-binding');p.add_argument('--rounds',type=int,default=2)
a=p.parse_args();root=Path(a.root).resolve();out=Path(a.out);out.mkdir(mode=0o700,exist_ok=False)
assert 3<=a.sessions<=48 and a.lessons in (128,1000,10000,50000) and 1<=a.workers<=8 and a.rounds==2
assert not a.semantic_scenarios or a.sessions>=6
sys.path[:0]=[str(root/'tests'),str(root/'scripts/evaluations'),str(root/'skills/agent-memory/scripts')]
from world import World
from publication_fixtures import PublicationFixture
from contracts.test_admission import AdmissionContract
from contracts.test_comparison import render,unpack
from identity_fixtures import bind
import publication_github
from labeled_recall_probe import source,inventory
from agent_memory import bank_layout,bank_contract
if a.source_binding:expected=json.loads(Path(a.source_binding).read_text())
else:expected=source(root)
def verify_source(phase):
 for name,value in expected['tracked'].items():
  target=root/name
  assert not target.is_symlink() and hashlib.sha256(target.read_bytes()).hexdigest()==value['sha256'] and stat.S_IMODE(target.stat().st_mode)==value['mode'],name
 assert inventory(root/'skills/agent-memory')==expected['portable']
 (out/('source-'+phase+'.json')).write_text(json.dumps({'commit':expected['commit'],'tracked':len(expected['tracked']),'portable':len(expected['portable']),'all_bytes_and_modes_match':True})+'\n')
verify_source('before');(out/'source-binding.json').write_text(json.dumps(expected,indent=2)+'\n')
shutil.copyfile(Path(__file__),out/'controller.py')
lock=threading.Lock();counter=0;commands=[];events=[];sessions=[];lessons={};forbidden_public=set();seed_reference={};seed_prune=None;started=time.perf_counter()
blobs=out/'blobs';blobs.mkdir(mode=0o700)
def retain(raw):
 sha=hashlib.sha256(raw).hexdigest();path=blobs/(sha+'.gz')
 with lock:
  if not path.exists():
   with gzip.open(path,'wb',compresslevel=1) as stream:stream.write(raw)
 return sha

def save(w,value,label):
 target=w.inside('inputs/'+label+'.json');target.parent.mkdir(mode=0o700,exist_ok=True)
 target.write_text(json.dumps(value,sort_keys=True,separators=(',',':'))+'\n');return target

def public(w,pack,helper,*args,expected_exit=0,fault_stage=None):
 global counter
 with lock:counter+=1;number=counter
 argv=[str(x) for x in [sys.executable,'-I',pack/'scripts'/helper,*args]]
 inputs={str(x):retain(x.read_bytes()) for x in args if isinstance(x,Path) and x.is_file()}
 overrides={'AGENT_MEMORY_FAULT_STAGE':fault_stage,'AGENT_MEMORY_FAULT_MODE':'exit'} if fault_stage else {}
 row={'number':number,'argv':argv,'inputs':inputs,'environment':{**w.env,**overrides},'exit':None}
 before=time.perf_counter()
 def store(stdout,stderr):
  row.update(seconds=time.perf_counter()-before,stdout_sha256=retain(stdout.encode()),stderr_sha256=retain(stderr.encode()))
  row['public_forbidden_marker_absent']=all(value not in stdout+stderr for value in forbidden_public)
  with lock:
   commands.append(row);(out/'commands.json').write_text(json.dumps(sorted(commands,key=lambda x:x['number']),indent=2)+'\n')
 try:r=w.run(argv,check=False,timeout=300,group=True,env=overrides)
 except BaseException as exc:
  chain=[];seen=set();current=exc;stdout='';stderr=''
  def stream(value):return value.decode('utf-8',errors='replace') if isinstance(value,bytes) else value if isinstance(value,str) else ''
  while current is not None and id(current) not in seen:
   seen.add(id(current));partial_out=stream(getattr(current,'output',None) or getattr(current,'stdout',None));partial_err=stream(getattr(current,'stderr',None))
   stdout=stdout or partial_out;stderr=stderr or partial_err
   chain.append({'type':type(current).__name__,'message':str(current),'stdout_sha256':retain(partial_out.encode()),'stderr_sha256':retain(partial_err.encode())})
   current=current.__cause__ or current.__context__
  row.update(execution_error=chain,timed_out=any(value['type']=='TimeoutExpired' for value in chain));store(stdout,stderr)
  raise
 row['exit']=r.returncode;row['timed_out']=False;store(r.stdout,r.stderr)
 assert row['public_forbidden_marker_absent'],(number,'public withheld-fixture disclosure; private raw evidence retained')
 if r.returncode==expected_exit==86 and not r.stdout.strip():return {'process_exit':86,'injected_interruption':True,'operation_completed':False}
 try:value=json.loads(r.stdout)
 except ValueError:raise AssertionError((number,'non-json-response',r.returncode,r.stderr[-1500:]))
 assert r.returncode==expected_exit,(number,helper,args[:2],r.returncode,value,r.stderr[-1000:])
 return value

def event(name,**values):
 row={'event':name,'seconds':time.perf_counter()-started,**values}
 with lock:
  events.append(row);(out/'events.json').write_text(json.dumps(events,indent=2)+'\n')
 print(json.dumps(row),flush=True)

def state(s):
 w=s['world'];repo=s['repo'];index=Path(w.git('rev-parse','--path-format=absolute','--git-path','index',cwd=repo).stdout.strip())
 code=(repo/'code.py').read_bytes();raw_index=index.read_bytes()
 value={'code_sha256':retain(code),'index_sha256':retain(raw_index),'head':w.git('rev-parse','HEAD',cwd=repo).stdout.strip(),'branch':w.git('symbolic-ref','HEAD',cwd=repo).stdout.strip()}
 with lock:
  with (out/'feature-state-history.jsonl').open('a') as stream:stream.write(json.dumps({'session':s['name'],'repo':str(repo),'seconds':time.perf_counter()-started,**value})+'\n')
 return value

def profile(w,pack,actor,label):
 preview=public(w,pack,'developer_identity.py','preview','--adopt',actor,'--label',label)
 public(w,pack,'developer_identity.py','apply','--proposal',save(w,preview,'developer-profile'))

def inspect(s,request=None,label='view'):
 w=s['world'];path=w.inside('inputs/'+s['name']+'-'+label+'.json')
 args=['inspect-v2','--repo',s['repo'],'--output',path]
 if request is not None:args+=['--input',save(w,request,s['name']+'-'+label+'-request')]
 if request is None:
  # The supported boundary wrapper preserves a verified prior memory cursor;
  # no feature ancestry advance is needed. Unpack the exact inner manifest.
  outer=w.inside('inputs/'+s['name']+'-'+label+'-boundary.json')
  public(w,s['pack'],'refresh_memory.py','inspect','--v2','--repo',s['repo'],'--output',outer)
  value=json.loads(outer.read_text())['inspection'];path.write_text(json.dumps(value));path.chmod(0o600)
  return path,value
 public(w,s['pack'],'reconcile_memory.py',*args)
 return path,json.loads(path.read_text())

def review(s,label):
 _,first=inspect(s,label=label+'-initial');rows={r['key']:r for r in first['sources']}
 request=first['request'];request['selections']={}
 for key in first['changed']:
  row=rows[key];subject=row['target_path'] or row['path']
  family=family_for(row) if a.semantic_scenarios else None
  ids=sorted(identity for identity,lesson in lessons.items() if family and lesson.get('role') and lesson['role'].startswith(family))
  request['selections'][key]={'searches':(['atrium wall color','paint specification'] if family=='wall-' else ['temporary connection retry cap','transient network attempts'] if family=='retry-' else [subject,subject.removesuffix('.md')]),
   'paths':[subject],'ids':ids,'complete':True,'uncertain':False}
 if a.semantic_scenarios and s.get('clarified'):
  group=consolidation_group(first)
  request['consolidation_sources']=[group] if group else []
 path,view=inspect(s,request,label+'-selected');w=s['world']
 if a.recovery_controls and s['number']==1 and label=='refresh-1':
  path.unlink()
  public(w,s['pack'],'reconcile_memory.py','page-v2','--repo',s['repo'],'--input',path,'--page','0',expected_exit=1)
  path,reconstructed=inspect(s,request,label+'-reconstructed')
  assert reconstructed==view
  event('lost-page-state-reconstructed',session=s['number'],unchanged_complete_manifest=True)
 for page in view['pages']:public(w,s['pack'],'reconcile_memory.py','page-v2','--repo',s['repo'],'--input',path,'--page',str(page['index']))
 judgment={'schema':'agent-memory/semantic-sharing-judgment-v2','request':request,'binding':view['binding'],
  'manifest_sha256':view['manifest_sha256'],'pages':[page['sha256'] for page in view['pages']],
  'pairs':[{'left':x,'right':y,'classification':classification({r['key']:r for r in view['sources']}[x],{r['key']:r for r in view['sources']}[y]) if a.semantic_scenarios else 'related',
   'reason':'Authored independent archive policy subjects and navigation/provenance retain their own evidence; this is a mechanical fixture assertion.'} for x,y in view['required_pairs']],
  'dependencies':{},'consolidations':[],'rationale':'Complete declared compatible-fixture delta review; not general semantic absence verification.',
  'recorded_at':datetime.now(timezone.utc).isoformat()}
 if a.semantic_scenarios and semantic_judgment(s,label,view,judgment):return review(s,label+'-after-authority-change')
 target=w.inside('inputs/'+s['name']+'-'+label+'-review.json')
 public(w,s['pack'],'reconcile_memory.py','review-v2','--repo',s['repo'],
  '--input',save(w,judgment,s['name']+'-'+label+'-judgment'),'--output',target)
 result=json.loads(target.read_text())
 if a.semantic_scenarios:
  allowed={row['path'] for row in lessons.values() if row.get('role','') and row['role'].startswith('wall-')}
  expected_withheld=set(s.get('expected_withheld',[]));allowed.update(expected_withheld)
  held_route_dependencies={component['path'] for component in result['proof']['blocked_components'] if component['component'] in {'route-prose','introduction'} and component['target_path'] in expected_withheld}
  for path in held_route_dependencies:
   assert all(component['component'] in {'route-prose','introduction'} and component['target_path'] in expected_withheld for component in result['proof']['blocked_components'] if component['path']==path)
  allowed.update(held_route_dependencies)
  assert set(result['blocked_paths']).issubset(allowed),result['blocked_paths']
  components={(value['path'],value['component'],value['target_path']) for value in result['proof']['blocked_components']}
  assert expected_withheld.issubset(set(result['blocked_paths']))
  assert all((path,'body',None) in components for path in expected_withheld)
  for target in s.get('expected_withheld_root_routes',[]):
   assert any(path=='MEMORY.md' and component in {'route-prose','introduction'} and destination==target for path,component,destination in components)
  expected_contradictions=[pair for pair in judgment['pairs'] if pair['classification']=='contradiction']
  selected_rows={row['key']:row for row in view['sources']}
  for pair in expected_contradictions:
   claims=[selected_rows[pair[key]] for key in ('left','right')]
   assert {claim['path'] for claim in claims}.issubset(set(result['blocked_paths'])) and result['conflicts_pending']
   assert all((claim['path'],claim['component'],claim['target_path']) in components for claim in claims)
   peers={claim['metadata']['origin']['developer-id'] for claim in claims if claim['metadata'] is not None}-{s['actor']}
   assert peers.issubset(set(result['other_developer_ids'])) and result['detecting_developer_id']==s['actor'] and result['messages_sent'] is False
  if s['number']==0 and label.startswith('round-2-reconciled') and not s.get('clarified'):
   wall_paths={row['path'] for row in lessons.values() if (row.get('role') or '').startswith('wall-')};assert len(wall_paths)==2
   assert wall_paths.issubset(set(result['blocked_paths'])) and all((path,'body',None) in components for path in wall_paths)
   assert result['conflicts_pending'] and result['other_developer_ids'] and result['detecting_developer_id']==s['actor']
   event('yellow-blue-held-before-clarification',session=0,claim_paths=sorted(wall_paths),other_developer_ids=result['other_developer_ids'])
  if expected_withheld:event('withheld-required-closure-confirmed',session=s['number'],phase=label,required_paths=sorted(expected_withheld),dependent_parent_paths=sorted(held_route_dependencies))
  if result['conflicts_pending']:
   assert result['messages_sent'] is False
   if set(result['blocked_paths'])-expected_withheld-held_route_dependencies:assert result['other_developer_ids']
   event('dispute-retained',session=s['number'],phase=label,held_paths=result['blocked_paths'],other_developer_ids=result['other_developer_ids'],detecting_developer_id=s['actor'],messages_sent=False)
 else:assert not result['blocked_paths'],result['blocked_paths']
 return result['proof']

def capture(s,round_number,role=None):
 w=s['world'];repo=s['repo'];pack=s['pack'];tag=s['name']+'-round-'+str(round_number)+('-'+role if role else '')
 h=AdmissionContract();h.pid=s['project'];h.actor=s['actor'];request=h.request()
 metadata=request['candidate']['metadata'];identity=str(uuid.UUID(hex=hashlib.sha256((s['project']+tag).encode()).hexdigest()[:32],version=4))
 metadata.update(id=identity,scope='repository',conditions={},kind='user-feedback',evidence=['fixture://daily-use-v1/'+tag])
 metadata['origin'].update({'developer-id':s['actor'],'harness':s['harness'],'session':s['name'],'code-state':'not-code'})
 subject='Archive partition '+str(s['number'])
 body=('# '+subject+' retention\n\nThe confirmed archive policy retains '+str(7+s['number'])+' validated snapshots.\n' if round_number==1 else
       '# '+subject+' inspection\n\nThe confirmed archive policy inspects '+str(3+s['number'])+' blocks per maintenance pass.\n')
 if role:
  subject='Atrium wall' if role.startswith('wall-') else 'Transient retry limit'
  body=('# Atrium wall\n\nThe confirmed atrium wall color is '+('yellow' if role=='wall-yellow' else 'blue')+'.\n' if role.startswith('wall-') else
        '# Transient retry limit\n\n'+('Use at most three retries for temporary connection failures.' if role=='retry-dev' else 'The temporary network retry cap is three attempts.')+'\n')
  metadata['conditions']={} if role.startswith('wall-') else {'environment':'development' if role=='retry-dev' else 'production'}
 if round_number==3 and role is None:
  body='# Archive partition '+str(s['number'])+' audit cadence\n\nThe confirmed archive policy audits every '+str(11+s['number'])+' maintenance passes.\n'
 request['candidate']['body']=body;request['assessment']['utility']='Retain this authored archive policy separately from other partitions and policy dimensions.'
 captured=public(w,pack,'admit_learning.py','capture','--repo',repo,'--request',save(w,request,tag+'-capture-request'))
 assert captured['decision']=='eligible',captured
 with lock:existing_rows=[row for row in lessons.values() if row['session']==s['number']]
 existing=[row['path'] for row in existing_rows]
 dimension={1:'retention',2:'inspection',3:'audit cadence'}[round_number]
 searches=([subject+' retention',subject+' inspection',subject+' audit cadence'] if role is None else ['atrium wall color','paint specification'] if role.startswith('wall-') else ['transient retry cap','temporary network attempts'])
 compared={'project_id':s['project'],'candidate':captured['capture']['request']['candidate'],
  'searches':searches,'read_paths':['projectbrief.md',*existing]}
 observed=public(w,pack,'discover_memory.py','start','--repo',repo,'--intent','comparison','--task','Compare the authored '+subject+' policy dimension against existing evidence.')
 trace=observed['discovery']
 parents=sorted({'topics/workspace/room-claims.md' if (row.get('role') or '').startswith('wall-') else 'topics/reliability/retry-claims.md' if (row.get('role') or '').startswith('retry-') else 'topics/archive/partition-'+str(s['number'])+'.md' for row in existing_rows})
 for parent in parents:
  observed=public(w,pack,'discover_memory.py','node','--repo',repo,'--input',save(w,trace,tag+'-navigation-state'),'--path',parent)
  trace=observed['discovery']
 observed=public(w,pack,'discover_memory.py','read','--repo',repo,'--input',save(w,trace,tag+'-navigation-state'),
  '--request',save(w,compared,tag+'-comparison'),'--rationale','Read actual project orientation and the existing authored archive partition policy; seed partition counters are a separate fixture subject. This is a scripted mechanical coverage assertion.')
 compared=observed['request'];inspection=observed['inspection']
 judgment={'observation':inspection['observation'],'decisions':[{'path':row['path'],'classification':'distinct-knowledge',
  'rationale':'The authored archive partition/policy dimension differs; retain each identity, condition and independent evidence.'} for row in inspection['matches']],
  'rationale':'Read all plausible authored partition-policy sources before capture; no model semantic-understanding claim.'}
 checked=public(w,pack,'compare_learning.py','review','--repo',repo,'--request',save(w,compared,tag+'-comparison'),
  '--judgment',save(w,judgment,tag+'-comparison-judgment'))
 node='topics/archive/partition-'+str(s['number'])+'.md' if role is None else ('topics/workspace/room-claims.md' if role.startswith('wall-') else 'topics/reliability/retry-claims.md')
 navigation={'learning':{'parent':node,'purpose':subject+' '+dimension+' policy' if role is None else subject+' color decision' if role.startswith('wall-') else 'Transient retry cap in '+metadata['conditions']['environment'],'read-when':'When this archive policy dimension applies' if role is None else 'When deciding the atrium wall color' if role.startswith('wall-') else 'When temporary connection failures occur in '+metadata['conditions']['environment']},
  'history':{'parent':node,'purpose':'Source and admission evidence for '+subject,'read-when':'When inspecting policy provenance'},
  'indexes':[] if (repo/'memory-bank'/node).exists() else [{'parent':'MEMORY.md','path':node,'scope':'repository','purpose':subject+' policy and evidence','read-when':'When working on this archive partition'}]}
 prepared=public(w,pack,'admit_learning.py','preview','--repo',repo,'--capture',save(w,captured,tag+'-captured'),
  '--request',save(w,compared,tag+'-comparison'),'--proof',save(w,{'schema':'agent-memory/comparison-result-v1','proof':checked['proof']},tag+'-comparison-proof'),
  '--navigation',save(w,navigation,tag+'-navigation'))
 done=public(w,pack,'admit_learning.py','apply','--proposal',save(w,prepared['proposal'],tag+'-admission'))
 assert done['admission_verified'] and state(s)==s['feature'],done
 path='topics/'+identity+'.md';meta,actual=unpack((repo/'memory-bank'/path).read_text())
 assert actual==body and meta['origin']['developer-id']==s['actor'] and meta['origin']['harness']==s['harness'] and meta['evidence']==metadata['evidence']
 return identity,{'path':path,'body':body,'metadata':meta,'session':s['number'],'round':round_number,'role':role,'mode_bits':stat.S_IMODE((repo/'memory-bank'/path).stat().st_mode)}

def prepare_publication(s,label):
 proof=review(s,label)
 result=public(s['world'],s['pack'],'publish_memory.py','prepare','--v2','--repo',s['repo'],
  '--input',save(s['world'],proof,s['name']+'-'+label+'-proof'))
 assert result['sharing_state']=='prepared',result
 return result['request']

def land(s,request,label):
 return public(s['world'],s['pack'],'publish_memory.py','land','--v2','--repo',s['repo'],
  '--input',save(s['world'],request,s['name']+'-'+label+'-landing'))

def refresh(s,round_number,peer):
 proof=review(s,'refresh-'+str(round_number));w=s['world'];tag=s['name']+'-refresh-'+str(round_number)
 recall={'project_id':s['project'],'query':'','ids':[peer[0]],'paths':[],'context':{},'kinds':['user-feedback'],
  'minimum_trust':'reviewed','limit':8,'include_context':False}
 target=w.inside('inputs/'+tag+'-prepared.json')
 public(w,s['pack'],'refresh_memory.py','prepare','--v2','--repo',s['repo'],
  '--input',save(w,{'proof':proof,'recall':recall},tag+'-request'),'--output',target)
 output=w.inside('inputs/'+tag+'-applied.json')
 if a.recovery_controls and s['number']==2 and round_number==1:
  public(w,s['pack'],'refresh_memory.py','apply','--v2','--input',target,'--output',output,
   expected_exit=86,fault_stage='transaction-after-receipt')
  unlock=public(w,s['pack'],'bank_writes.py','recover','--repo',s['repo'])
  public(w,s['pack'],'bank_writes.py','recover','--repo',s['repo'],'--proposal',save(w,unlock,tag+'-unlock'))
  recover=public(w,s['pack'],'bank_transactions.py','recover','--repo',s['repo'],'--mode','finish')
  finished=public(w,s['pack'],'bank_transactions.py','recover','--repo',s['repo'],'--proposal',save(w,recover,tag+'-recovery'))
  assert finished['outcome']=='finished'
  event('interrupted-completed-import-recovered',session=s['number'],stage='transaction-after-receipt')
 public(w,s['pack'],'refresh_memory.py','apply','--v2','--input',target,'--output',output)
 result=json.loads(output.read_text());
 if a.recovery_controls and s['number']==0 and round_number==1:
  repeat=w.inside('inputs/'+tag+'-replayed.json')
  before_replay=inventory(s['repo']/'memory-bank')
  public(w,s['pack'],'refresh_memory.py','apply','--v2','--input',target,'--output',repeat)
  replayed=json.loads(repeat.read_text());assert replayed['receipt']['already_applied'] and not replayed['canonical_writes_performed']
  assert before_replay==inventory(s['repo']/'memory-bank')
  event('lost-refresh-response-read-back',session=s['number'],canonical_reapplication=False)
 assert result['refresh_applied'] and result['remote_freshness_verified'] and (a.semantic_scenarios or not result['conflicts_pending']),result
 rows=result['recall']['results'];assert len(rows)==1 and rows[0]['record']==peer[1]['metadata'] and rows[0]['body']==peer[1]['body'],rows
 assert state(s)==s['feature']
 return {'session':s['number'],'peer_id':peer[0],'round':round_number,'useful_peer_recall':True,'feature_preserved':True}


def family_for(row):
 meta=row.get('metadata') or {};identity=meta.get('id');lesson=lessons.get(identity,{})
 role=lesson.get('role')
 if role:return 'wall-' if role.startswith('wall-') else 'retry-'
 target=row.get('target_path') or row['path']
 if target=='topics/workspace/room-claims.md':return 'wall-'
 if target=='topics/reliability/retry-claims.md':return 'retry-'
 for value in lessons.values():
  if value['path']==target and value.get('role'):return 'wall-' if value['role'].startswith('wall-') else 'retry-'
 return None

def classification(left,right):
 a_meta,b_meta=left.get('metadata'),right.get('metadata')
 if a_meta and b_meta:
  a_ref={'id':a_meta['id'],'version':a_meta['version']};b_ref={'id':b_meta['id'],'version':b_meta['version']}
  if a_ref in b_meta.get('corrects',[])+b_meta.get('supersedes',[]) or b_ref in a_meta.get('corrects',[])+a_meta.get('supersedes',[]):return 'lifecycle'
 if left['path']==right['path']=='projectbrief.md':return 'compatible'
 if not (a_meta and b_meta) or left['path'].startswith('history/') or right['path'].startswith('history/') or left['lifecycle']!='active' or right['lifecycle']!='active':return 'related'
 if family_for(left)==family_for(right)=='wall-' and a_meta['id']!=b_meta['id']:
  return 'contradiction' if min(a_meta['version'],b_meta['version'])==1 and any(lessons.get(m['id'],{}).get('role')=='wall-yellow' and m['version']==1 for m in (a_meta,b_meta)) else 'equivalent'
 if family_for(left)==family_for(right)=='retry-' and a_meta['id']!=b_meta['id']:return 'disjoint-conditions'
 return 'related'

def observed_write(s,path,body,label):
 w=s['world'];content=w.inside('inputs/'+s['name']+'-'+label+'.md');content.write_text(body)
 if path=='MEMORY.md':
  files={p.relative_to(s['repo']/'memory-bank').as_posix():p.read_text() for p in (s['repo']/'memory-bank').rglob('*') if p.is_file()};files[path]=body
  preview=public(w,s['pack'],'bank_transactions.py','preview','--repo',s['repo'],'--files',save(w,files,s['name']+'-'+label+'-files'),'--operation','replace')['proposal']
  helper='bank_transactions.py'
 else:
  preview=public(w,s['pack'],'bank_writes.py','preview','--repo',s['repo'],'--path',path,'--content',content);helper='bank_writes.py'
 done=public(w,s['pack'],helper,'apply','--proposal',save(w,preview,s['name']+'-'+label+'-write'))
 assert done['verified'] and state(s)==s['feature'];return done

def same_file_and_framing(all_sessions):
 for number,old,new in ((4,'Record the archive checksum.','Record and verify the archive checksum.'),(5,'Retain the archive evidence.','Retain the archive evidence and its source identity.')):
  s=all_sessions[number];target=s['repo']/'memory-bank/projectbrief.md';raw=target.read_text();assert old in raw
  observed_write(s,'projectbrief.md',raw.replace(old,new),'compatible-project-brief')
 event('same-file-compatible-edits-observed',sessions=[4,5])

def framing_closure(all_sessions):
 s=all_sessions[4];target=s['repo']/'memory-bank/MEMORY.md';raw=target.read_text();old='Archive partition 4 policy and evidence';assert old in raw
 child=next(row for row in lessons.values() if row['session']==4 and row['round']==1)
 child_path=s['repo']/'memory-bank'/child['path'];before=child_path.read_bytes()
 observed_write(s,'MEMORY.md',raw.replace(old,'Archive partition 4 preparation and inspection policy'),'changed-route-caption')
 assert before==child_path.read_bytes();s['framing_child']=child['path']
 _,view=inspect(s,label='changed-framing-closure')
 rows={row['key']:row for row in view['sources']}
 assert any(rows[key]['path']==child['path'] for key in view['changed'])
 assert before==child_path.read_bytes()
 event('changed-framing-derived-child-closure',session=4,child=child['path'],unchanged_child_sha256=hashlib.sha256(before).hexdigest())
 identity,new_row=capture(s,3);lessons[identity]=new_row
 result=land(s,prepare_publication(s,'framing-and-independent-audit'),'framing-and-independent-audit');assert result['publication_verified']
 refresh(s,15,(identity,new_row))
 assert before==child_path.read_bytes() and state(s)==s['feature']
 event('changed-framing-and-independent-learning-converged',session=4,independent_id=identity)

def semantic_judgment(s,label,view,judgment):
 rows={r['key']:r for r in view['sources']};w=s['world'];before=inventory(s['repo']/'memory-bank')
 if s.get('framing_child') and label=='round-2-initial':
  assert any(rows[key]['path']==s['framing_child'] for key in view['changed'])
  event('changed-framing-derived-child-closure',session=s['number'],child=s['framing_child'])
 if s['number']==0 and label=='round-2-reconciled':
  assert any(pair['classification']=='contradiction' for pair in judgment['pairs'])
  bad=copy.deepcopy(judgment);bad['pairs'].pop()
  public(w,s['pack'],'reconcile_memory.py','review-v2','--repo',s['repo'],'--input',save(w,bad,'missing-edge-judgment'),'--output',w.inside('inputs/missing-edge-refused.json'),expected_exit=1)
  bad=copy.deepcopy(judgment);bad['pages'].append(bad['pages'][0])
  public(w,s['pack'],'reconcile_memory.py','review-v2','--repo',s['repo'],'--input',save(w,bad,'duplicated-page-judgment'),'--output',w.inside('inputs/duplicated-page-refused.json'),expected_exit=1)
  assert before==inventory(s['repo']/'memory-bank') and state(s)==s['feature']
  event('missing-edge-duplicate-page-refused',session=0,canonical_unchanged=True)
  bad_request=copy.deepcopy(view['request']);bad_request['consolidation_sources']=[]
  for key,selection in bad_request['selections'].items():
   selection.update(searches=['unrelated fixture prism','unrelated fixture orbit'],ids=[],paths=[rows[key]['target_path'] or rows[key]['path']])
  _,omitted=inspect(s,bad_request,'deliberately-omitted-known-cross-file-match')
  bad_rows={r['key']:r for r in omitted['sources']}
  missing=[pair for pair in view['required_pairs'] if classification(rows[pair[0]],rows[pair[1]])=='contradiction' and pair not in omitted['required_pairs']]
  assert missing, 'Deliberately bad selectors did not actually omit the known semantic cross-file match'
  event('independent-ground-truth-detected-selection-failure',session=0,missed_semantic_edges=missing,native_omniscience_claimed=False,bad_review_not_applied=True)
  # A real authority commit after reading all pages invalidates their binding.
  w.git('fetch','origin','main',cwd=anchor_for_code);w.git('merge','--ff-only','origin/main',cwd=anchor_for_code)
  w.git('commit','--allow-empty','-m','Synthetic authority advanced after page review',cwd=anchor_for_code);w.git('push','origin','HEAD:main',cwd=anchor_for_code)
  public(w,s['pack'],'reconcile_memory.py','review-v2','--repo',s['repo'],'--input',save(w,judgment,'stale-pages-judgment'),'--output',w.inside('inputs/stale-pages-refused.json'),expected_exit=1)
  assert before==inventory(s['repo']/'memory-bank') and state(s)==s['feature']
  event('authority-change-after-pages-refused',session=0,canonical_unchanged=True)
  return True
 if s.get('clarified'):
  group=consolidation_group(view)
  if group:
   primary=next(key for key in group if rows[key]['metadata']['id']==s['wall_id'])
   judgment['consolidations']=[{'sources':group,'primary':primary,'body':'# Atrium wall\n\nThe confirmed atrium wall color is blue.\n','reason':'Synthetic explicit clarification resolves yellow to blue; preserve both developer origins, all evidence and superseded history.'}]
  evidence=[key for key,row in rows.items() if (row.get('metadata') or {}).get('id')==s['wall_id'] and row['metadata']['version']>=2 and row['lifecycle']=='active' and not row['path'].startswith('history/') and set(row['sides'])&{'local','landed'}]
  assert evidence
  judgment['resolutions']={known['id']:{'basis':'operator-clarification','evidence':[evidence[-1]],'reason':'The predeclared synthetic detecting-developer clarification confirms blue; this real qualified correction preserves the prior yellow evidence.'} for known in view['known_causes'] if not known['resolved']}

def typed_recall(s,ids,context=None):
 request={'project_id':s['project'],'query':'','ids':ids,'paths':[],'context':context or {},'kinds':['user-feedback'],'minimum_trust':'reviewed','limit':8,'include_context':False}
 return public(s['world'],s['pack'],'retrieve.py','--repo',s['repo'],'--request',save(s['world'],request,s['name']+'-typed-recall'))

def post_round_semantics(all_sessions):
 for s in all_sessions:
  text=(s['repo']/'memory-bank/projectbrief.md').read_text()
  assert 'verify the archive checksum' in text and 'evidence and its source identity' in text
  ids=[identity for identity,row in lessons.items() if row.get('role') and row['role'].startswith('retry-')]
  for environment,role in (('development','retry-dev'),('production','retry-prod')):
   found=typed_recall(s,ids,{'environment':environment})['results']
   assert len(found)==1 and lessons[found[0]['record']['id']]['role']==role
  assert state(s)==s['feature']
 event('same-file-and-disjoint-condition-peer-recall',sessions=len(all_sessions),origins_preserved=True)
 framing_closure(all_sessions)
 old_v1_consumer_refusal(all_sessions[0])
 baseline_faults(all_sessions[0])
 correction_and_consolidation(all_sessions)
 prune_and_older_branch(all_sessions)
 ordinary_code_integration(all_sessions)
 withheld_replacement_independent_path(all_sessions)
 event('semantic-round-controls-complete',full_daily_use_v1_acceptance=False,remaining=['required-size-repetition','independent-review'])


def consolidation_group(view):
 current={}
 for row in view['sources']:
  metadata=row.get('metadata')
  if not metadata or row['path'].startswith('history/') or row['lifecycle']!='active' or not set(row['sides'])&{'local','landed'} or family_for(row)!='wall-':continue
  previous=current.get(metadata['id'])
  if previous is None or previous['metadata']['version']<metadata['version']:current[metadata['id']]=row
 if len(current)!=2:return []
 chosen=list(current.values())
 yellow=next((row for row in chosen if lessons.get(row['metadata']['id'],{}).get('role')=='wall-yellow'),None)
 if yellow is None or yellow['metadata']['version']!=2 or classification(*chosen)!='equivalent':return []
 return sorted(row['key'] for row in chosen)

def correction_and_consolidation(all_sessions):
 from contracts.test_governance import GovernanceContract
 s=all_sessions[0];w=s['world'];pack=s['pack'];repo=s['repo'];bank=repo/'memory-bank'
 identity,previous=next((identity,row) for identity,row in lessons.items() if row.get('role')=='wall-yellow')
 old,_=unpack((bank/previous['path']).read_text());assert old['version']==1
 h=AdmissionContract();h.pid=s['project'];h.actor=s['actor'];request=h.request()
 request['candidate']['metadata'].update(id=identity,scope='repository',conditions={},kind='user-feedback',evidence=['user:fixture-wall-blue-clarification'])
 request['candidate']['metadata']['origin'].update({'developer-id':s['actor'],'session':s['name'],'harness':s['harness'],'code-state':'not-code'})
 request['candidate']['body']='# Atrium wall\n\nThe confirmed atrium wall color is blue.\n'
 capture=public(w,pack,'admit_learning.py','capture','--repo',repo,'--request',save(w,request,'clarified-wall-capture'))['capture']
 compared={'project_id':s['project'],'candidate':capture['request']['candidate'],'searches':['atrium wall color','paint specification'],'read_paths':[previous['path']]}
 inspected=public(w,pack,'compare_learning.py','inspect','--repo',repo,'--request',save(w,compared,'clarified-wall-comparison'))
 judgment={'observation':inspected['observation'],'decisions':[{'path':row['path'],'classification':'contradiction' if row['path']==previous['path'] else 'related-context','rationale':'Predeclared synthetic detecting-developer clarification confirms blue and explicitly corrects this yellow claim.'} for row in inspected['matches']],'rationale':'Preserve old yellow origin and all conditions; synthetic fixture clarification is not a real operator color decision.'}
 checked=public(w,pack,'compare_learning.py','review','--repo',repo,'--request',save(w,compared,'clarified-wall-comparison'),'--judgment',save(w,judgment,'clarified-wall-judgment'))
 gov=GovernanceContract();gov.pid=s['project'];gov.actor=s['actor'];value=gov.request(bank,old,previous['path'],'correct')
 value['authority'].update(evidence=['user:fixture-wall-blue-clarification'],reason='Synthetic detecting developer A clarifies the atrium wall is blue after consulting the independently attributed developer B source.')
 value['capture']=capture;value['comparison']={'request':compared,'proof':checked['proof']}
 prepared=public(w,pack,'govern_memory.py','preview','--repo',repo,'--request',save(w,value,'wall-correction-request'))
 applied=public(w,pack,'govern_memory.py','apply','--proposal',save(w,prepared['proposal'],'wall-correction-proposal'))
 now,body=unpack((bank/previous['path']).read_text());assert now['version']==2 and 'blue' in body and state(s)==s['feature']
 archived,oldbody=unpack((bank/'history/versions'/identity/'v1.md').read_text());assert archived['origin']==old['origin'] and 'yellow' in oldbody
 s['wall_id']=identity;s['clarified']=True
 peer=next((i,row) for i,row in lessons.items() if row['session']==1 and row['round']==2 and row.get('role') is None)
 refresh(s,20,peer)
 current,_=unpack((bank/previous['path']).read_text());assert current['version']==3
 blue=next((i,row) for i,row in lessons.items() if row.get('role')=='wall-blue')
 archived_blue,_=unpack((bank/blue[1]['path']).read_text());assert archived_blue['status']=='superseded' and archived_blue['origin']==blue[1]['metadata']['origin']
 assert {'id':blue[0],'version':1} in current['derived-from']
 assert set(blue[1]['metadata']['evidence']).issubset(current['evidence']) and 'user:fixture-wall-blue-clarification' in current['evidence']
 # Consolidation is an actual refresh composition. Qualify after its local state
 # change before preparing a publication; a pre-apply proof cannot authorize it.
 request=prepare_publication(s,'clarified-wall-publication');result=land(s,request,'clarified-wall-publication')
 assert result['publication_verified']
 refresh(s,21,peer)
 assert 'blue' in typed_recall(s,[identity])['results'][0]['body']
 event('explicit-correction-consolidation-landed',session=0,primary_id=identity,version=3,both_origins_and_history_preserved=True,feature_preserved=state(s)==s['feature'])


def prune_and_older_branch(all_sessions):
 global seed_prune
 from contracts.test_governance import GovernanceContract
 s=all_sessions[0];w=s['world'];bank=s['repo']/'memory-bank';path='topics/seed/records/0.md'
 metadata,body=unpack((bank/path).read_text());identity=metadata['id']
 gov=GovernanceContract();gov.pid=s['project'];gov.actor=s['actor'];value=gov.request(bank,metadata,path,'delete')
 value['authority'].update(basis='explicit-deletion',reason_class='obsolete',reason='Predeclared synthetic fixture confirms this seed policy obsolete under every retained condition; remove its stable identity.')
 value['delete_ids']=[identity]
 prepared=public(w,s['pack'],'govern_memory.py','preview','--repo',s['repo'],'--request',save(w,value,'obsolete-seed-delete'))
 done=public(w,s['pack'],'govern_memory.py','apply','--proposal',save(w,prepared['proposal'],'obsolete-seed-delete-proposal'))
 assert not (bank/path).exists() and (bank/'history/tombstones'/(identity+'.md')).is_file() and state(s)==s['feature']
 seed_prune={'record_path':path,'tombstone_path':'history/tombstones/'+identity+'.md','tombstone_bytes':(bank/'history/tombstones'/(identity+'.md')).read_bytes()}
 result=land(s,prepare_publication(s,'prune-seed-publication'),'prune-seed-publication');assert result['publication_verified']
 old=all_sessions[-1];assert (old['repo']/'memory-bank'/path).is_file()
 new_id,new_row=capture(old,3);lessons[new_id]=new_row
 result=land(old,prepare_publication(old,'old-branch-new-audit'),'old-branch-new-audit');assert result['publication_verified']
 refresh(old,30,(new_id,new_row))
 assert not (old['repo']/'memory-bank'/path).exists() and not typed_recall(old,[identity],{'component':'src/seed/0'})['results']
 assert typed_recall(old,[new_id])['results'][0]['body']==new_row['body'] and state(old)==old['feature']
 event('prune-and-older-branch-nonresurrection',pruned_id=identity,older_session=old['number'],new_independent_lesson=new_id,feature_preserved=True)


def ordinary_code_integration(all_sessions):
 s=all_sessions[0];w=s['world'];repo=s['repo'];anchor=repo.parents[1]/'repo'
 # World0 anchor is retained explicitly by the caller, not guessed from paths.
 anchor=anchor_for_code
 w.git('fetch','origin','main',cwd=anchor);w.git('merge','--ff-only','origin/main',cwd=anchor)
 bank_before=inventory(anchor/'memory-bank');(anchor/'code.py').write_text('VALUE = 900  # ordinary landed feature\n')
 w.git('add','code.py',cwd=anchor);w.git('commit','-m','Synthetic ordinary landed code change',cwd=anchor);w.git('push','origin','HEAD:main',cwd=anchor)
 assert bank_before==inventory(anchor/'memory-bank')
 peer=next((i,row) for i,row in lessons.items() if row['session']==1 and row['round']==2 and row.get('role') is None)
 refresh(s,40,peer);assert state(s)==s['feature'] and '900' not in (repo/'code.py').read_text()
 bank_before=inventory(repo/'memory-bank')
 # These are intentional caller Git operations, outside the memory-only boundary.
 w.git('add','memory-bank','code.py',cwd=repo);w.git('commit','-m','Synthetic caller checkpoints its feature and current memory',cwd=repo)
 w.git('fetch','origin','main',cwd=repo);merged=w.git('merge','--no-edit','origin/main',cwd=repo,check=False)
 if merged.returncode:
  conflicted=w.git('diff','--name-only','--diff-filter=U',cwd=repo).stdout.splitlines();assert conflicted==['code.py'],conflicted
  (repo/'code.py').write_text('VALUE = 900  # explicitly resolved synthetic caller merge\n');w.git('add','code.py',cwd=repo);w.git('commit','--no-edit',cwd=repo)
 assert bank_before==inventory(repo/'memory-bank') and '900' in (repo/'code.py').read_text()
 assert typed_recall(s,[peer[0]])['results'][0]['body']==peer[1]['body']
 s['feature']=state(s)
 event('ordinary-feature-integration-preserved-memory',session=0,caller_explicit_git_integration=True,memory_refresh_did_not_import_code=True,bank_exact_before_after=True)


def old_v1_consumer_refusal(s):
 import tarfile
 w=s['world'];archive=root/'tests/fixtures/v0.0.2-runtime/portable-pack.tar.gz';provenance=json.loads((archive.parent/'provenance.json').read_text())
 assert hashlib.sha256(archive.read_bytes()).hexdigest()==provenance['sha256']
 destination=w.inside('old-runtime-v002');destination.mkdir()
 with tarfile.open(archive,'r:gz') as stream:
  for member in stream.getmembers():
   assert not Path(member.name).is_absolute() and '..' not in Path(member.name).parts and (member.isfile() or member.isdir())
  stream.extractall(destination,filter='data')
 proof=review(s,'actual-old-consumer-proof');before=inventory(s['repo']/'memory-bank')
 failed=public(w,destination/'skills/agent-memory','reconcile_memory.py','check','--repo',s['repo'],'--input',save(w,proof,'old-v1-actual-v2-proof'),expected_exit=1)
 assert before==inventory(s['repo']/'memory-bank') and state(s)==s['feature']
 event('actual-old-v1-consumer-refused-v2',released_runtime=provenance['source_commit'],canonical_unchanged=True)


def baseline_faults(s):
 w=s['world'];repo=s['repo'];policy_path=w.inside('github-policy.json');policy=policy_path.read_bytes();remote=Path(json.loads(policy)['remote'])
 wrapper=w.inside('fixture-bin/git');wrapper_before=wrapper.read_bytes();mirror=w.inside('fault-baseline-mirror.git')
 w.git('clone','--bare',str(remote),str(mirror))
 base=w.git('rev-parse','refs/heads/main',cwd=mirror).stdout.strip();tree=w.git('rev-parse',base+'^{tree}',cwd=mirror).stdout.strip()
 def commit(message,parents=()):
  argv=['commit-tree',tree]
  for parent in parents:argv+=['-p',parent]
  return w.git(*argv,'-m',message,cwd=mirror).stdout.strip()
 left=commit('Synthetic left independent ancestor',[base]);right=commit('Synthetic right independent ancestor',[base])
 local_merge=commit('Synthetic local criss-cross merge',[left,right]);remote_merge=commit('Synthetic remote criss-cross merge',[right,left])
 w.git('fetch',str(mirror),local_merge,cwd=repo)
 probe=w.inside('ambiguity-probe');w.git('worktree','add','-b','ambiguity-probe',str(probe),local_merge,cwd=repo);bind(w,probe)
 controlled={**s,'repo':probe,'name':'ambiguity-probe'};controlled['feature']=state(controlled)
 original_before=inventory(repo/'memory-bank');probe_before=inventory(probe/'memory-bank')
 try:
  wrapper.write_bytes(wrapper_before.replace(str(remote).encode(),str(mirror).encode()))
  redirected=json.loads(policy);redirected['remote']=str(mirror);policy_path.write_text(json.dumps(redirected))
  w.git('update-ref','refs/heads/main',remote_merge,cwd=mirror)
  actual=w.git('merge-base','--all',local_merge,remote_merge,cwd=mirror).stdout.splitlines();assert set(actual)=={left,right}
  failed=public(w,s['pack'],'refresh_memory.py','inspect','--v2','--repo',probe,'--output',w.inside('ambiguity-output.json'),expected_exit=1)
  assert not failed['remote_freshness_verified'] and probe_before==inventory(probe/'memory-bank') and state(controlled)==controlled['feature']
  orphan=commit('Synthetic rewritten authority, same bank tree')
  w.git('update-ref','refs/heads/main',orphan,cwd=mirror)
  failed=public(w,s['pack'],'refresh_memory.py','inspect','--v2','--repo',repo,'--output',w.inside('rewrite-output.json'),expected_exit=1)
  assert not failed['remote_freshness_verified'] and original_before==inventory(repo/'memory-bank') and state(s)==s['feature']
 finally:
  wrapper.write_bytes(wrapper_before);policy_path.write_bytes(policy)
 w.git('fetch','origin','main',cwd=repo)
 peer=next((identity,row) for identity,row in lessons.items() if row['session']==1 and row['round']==2 and row.get('role') is None)
 assert typed_recall(s,[peer[0]])['results'][0]['body']==peer[1]['body']
 event('ambiguous-and-rewritten-bases-refused',two_actual_common_bases=True,rewritten_tree_preserved_identity=True,original_bank_feature_preserved=True,existing_peer_recall_useful=True)


def withheld_replacement_independent_path(all_sessions):
 s=all_sessions[2];w=s['world'];bank=s['repo']/'memory-bank';old_path='topics/seed/records/1.md';old,body=unpack((bank/old_path).read_text())
 new_id,new_row=capture(s,3);lessons[new_id]=new_row
 replacement=copy.deepcopy(old);identity=str(uuid.uuid4());replacement.update(id=identity,visibility='hidden',supersedes=[{'id':old['id'],'version':old['version']}])
 path='topics/withheld-replacement-'+str(s['number'])+'.md';marker='sk-'+('Q'*24)
 # Deliberately seed hostile/untrusted source bytes as a negative fixture input;
 # this is not claimed as a successful governed admission or a real credential.
 (bank/path).write_text(render(replacement,'# Withheld policy\n\n'+marker+'\n'))
 meta,routes,omitted=bank_contract.parse_router((bank/'MEMORY.md').read_text());routes.append({'path':path,'purpose':'Seed partition replacement evidence','scope':replacement['scope'],'lifecycle':'active','read-when':'When inspecting the seed partition policy'})
 (bank/'MEMORY.md').write_text(bank_contract.router_text(meta,routes,omitted))
 s['expected_withheld']=[path,old_path];s['expected_withheld_root_routes']=[path];raw=(bank/path).read_bytes()
 forbidden_public.update({marker,'# Withheld policy'})
 proof=review(s,'selected-withheld-replacement')
 assert marker not in json.dumps(proof)
 result=land(s,public(w,s['pack'],'publish_memory.py','prepare','--v2','--repo',s['repo'],'--input',save(w,proof,'withheld-publication-proof'))['request'],'withheld-independent-publication')
 assert result['publication_verified']
 refresh(s,50,(new_id,new_row))
 assert (bank/path).read_bytes()==raw and not typed_recall(s,[old['id']],{'component':'src/seed/1'})['results']
 assert typed_recall(s,[new_id])['results'][0]['body']==new_row['body'] and state(s)==s['feature']
 event('withheld-replacement-held-independent-learning-converged',session=2,old_claim_not_reactivated=True,raw_body_stays_private=True,new_independent_id=new_id,feature_preserved=True)

def seed_check(s,phase,*,allow_designated_prune=False):
 expected=set(seed_reference);deleted=set()
 if allow_designated_prune and seed_prune is not None and s['number'] in {0,2,a.sessions-1}:
  deleted={seed_prune['record_path']};expected-=deleted
  assert not (s['repo']/'memory-bank'/seed_prune['record_path']).exists()
  assert (s['repo']/'memory-bank'/seed_prune['tombstone_path']).read_bytes()==seed_prune['tombstone_bytes']
 bank=s['repo']/'memory-bank';actual_paths={p.relative_to(bank).as_posix() for p in (bank/'topics/seed/records').glob('*.md')};assert actual_paths==expected
 actual={}
 for path in sorted(expected):
  target=bank/path;assert target.is_file() and not target.is_symlink()
  value={'sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'mode_bits':stat.S_IMODE(target.stat().st_mode),'id':seed_reference[path]['id']}
  assert value['sha256']==seed_reference[path]['sha256'] and value['mode_bits']==s['seed_modes'][path],(s['number'],path,'seed-changed')
  actual[path]=value
 event('seed-learning-preserved',session=s['number'],phase=phase,records=len(actual),expected_initial_ids=a.lessons,designated_pruned_paths=sorted(deleted),inventory_sha256=retain(json.dumps(actual,sort_keys=True).encode()))


def offline_start(s):
 w=s['world'];wrapper=w.inside('fixture-bin/git');original=wrapper.read_bytes();mode=stat.S_IMODE(wrapper.stat().st_mode)
 online=w.inside('fixture-bin/daily-online-git');online.write_bytes(original);online.chmod(mode)
 flag=w.inside('daily-offline-session.json');flag.write_text(json.dumps({'repo':str(s['repo'])}));flag.chmod(0o600)
 adapter='#!'+sys.executable+'\nimport json,os,sys\nfrom pathlib import Path\nflag=Path('+repr(str(flag))+')\nif flag.exists() and str(Path.cwd().resolve())==json.loads(flag.read_text())["repo"] and any(arg in {"fetch","push","ls-remote"} for arg in sys.argv[1:]):\n sys.stderr.write("Owned session transport is offline.\\n");sys.exit(1)\nos.execv("/bin/sh",["/bin/sh",'+repr(str(online))+',*sys.argv[1:]])\n'
 compile(adapter,str(wrapper),'exec')
 wrapper.write_text(adapter);wrapper.chmod(mode);s['offline_adapter']={'wrapper':wrapper,'original':original,'mode':mode,'flag':flag}
 event('session-specific-outage-started',session=s['number'],scope=str(s['repo']),adapter_sha256=retain(adapter.encode()),original_adapter_sha256=retain(original))
 before=inventory(s['repo']/'memory-bank');pending=public(w,s['pack'],'sharing_recovery.py','inspect','--fetch','--repo',s['repo'])
 assert pending['sharing_state']=='pending-stale' and not pending['remote_freshness_verified']
 own=next((identity,row) for identity,row in lessons.items() if row['session']==0 and row['round']==2 and row.get('role') is None)
 assert typed_recall(s,[own[0]])['results'][0]['body']==own[1]['body'] and before==inventory(s['repo']/'memory-bank') and state(s)==s['feature']
 event('offline-session-preserved-local-learning',session=0,peer_publications_so_far=0)


def offline_rejoin(s,landed_peers):
 assert set(landed_peers)==set(range(1,a.sessions));adapter=s['offline_adapter'];assert adapter['flag'].is_file()
 pending=public(s['world'],s['pack'],'sharing_recovery.py','inspect','--fetch','--repo',s['repo'])
 assert pending['sharing_state']=='pending-stale' and not pending['remote_freshness_verified'] and state(s)==s['feature']
 event('offline-session-still-offline-after-peer-publications',session=0,verified_peer_landings=list(landed_peers))
 adapter['flag'].unlink();adapter['wrapper'].write_bytes(adapter['original']);adapter['wrapper'].chmod(adapter['mode'])
 assert adapter['wrapper'].read_bytes()==adapter['original']
 event('offline-session-rejoined-after-peer-publications',session=0,verified_peer_landings=list(landed_peers),transport_restored=True)


def require_scenario_events():
 required={'all-round-learning-preserved','seed-learning-preserved'}
 if a.recovery_controls:required.update({'lost-publication-response-read-back','lost-refresh-response-read-back','interrupted-completed-import-recovered','lost-page-state-reconstructed','lost-cursor-reconstructed','offline-session-preserved-local-learning','offline-session-still-offline-after-peer-publications','offline-session-rejoined-after-peer-publications'})
 if a.semantic_scenarios:required.update({'same-file-compatible-edits-observed','missing-edge-duplicate-page-refused','independent-ground-truth-detected-selection-failure','authority-change-after-pages-refused','yellow-blue-held-before-clarification','same-file-and-disjoint-condition-peer-recall','changed-framing-derived-child-closure','changed-framing-and-independent-learning-converged','actual-old-v1-consumer-refused-v2','ambiguous-and-rewritten-bases-refused','explicit-correction-consolidation-landed','prune-and-older-branch-nonresurrection','ordinary-feature-integration-preserved-memory','withheld-required-closure-confirmed','withheld-replacement-held-independent-learning-converged'})
 actual={row['event'] for row in events};assert required.issubset(actual),sorted(required-actual)
 assert {row['round'] for row in events if row['event']=='all-round-learning-preserved'}=={1,2}
 for phase in ('initial','round-1','round-2','final'):
  assert {row['session'] for row in events if row['event']=='seed-learning-preserved' and row['phase']==phase}==set(range(a.sessions)),phase
 event('declared-scenario-events-complete',required=sorted(required),global_acceptance_claimed=False)


outcome={'complete_compatible_rounds':False,'full_daily_use_v1_acceptance':False,'required_fault_matrix_complete':False}
try:
 with ExitStack() as stack:
  worlds=[stack.enter_context(World()) for _ in range(3)]
  w=worlds[0];h=PublicationFixture();repo,bank,peer,base=h.fixture(w);pack=h.pack
  w.git('reset','--','code.py',cwd=repo);(repo/'code.py').write_text('VALUE = 1\n')
  template=json.loads((pack/'templates/record-proposal.example.json').read_text())['metadata']
  project=json.loads(next(line.split(': ',1)[1] for line in (bank/'MEMORY.md').read_text().splitlines() if line.startswith('project-id: ')))
  status=bind(w,repo);actor_a=status['developer_id'];actor_b=str(uuid.uuid4())
  stamp='2026-10-25T00:00:00Z';header=lambda value:'---\n'+''.join(key+': '+json.dumps(value)+'\n' for key,value in value.items())+'---\n'
  seed={};leaf_routes=[];seed_reference={}
  for start in range(0,a.lessons,48):
   routes=[]
   for n in range(start,min(start+48,a.lessons)):
    metadata=copy.deepcopy(template);metadata.update({'id':str(uuid.UUID(hex=hashlib.sha256((project+'seed-'+str(n)).encode()).hexdigest()[:32],version=4)),
     'project-id':project,'status':'active','scope':'component:src/seed/'+str(n),'conditions':{'component':'src/seed/'+str(n)},
     'trust':'reviewed','visibility':'visible','admission-reason':'Authored seed fixture, not real admission evidence','evidence':['fixture://daily-use-v1/seed/'+str(n)]})
    metadata['origin'].update({'developer-id':actor_a,'code-state':'not-code'})
    path='topics/seed/records/'+str(n)+'.md';seed[path]=header(metadata)+'# Seed partition '+str(n)+'\n\nThis authored seed partition retains '+str(2+n%113)+' verified work units.\n'
    seed_reference[path]={'id':metadata['id'],'sha256':hashlib.sha256(seed[path].encode()).hexdigest()}
    routes.append({'path':path,'purpose':'Seed partition '+str(n)+' policy','scope':metadata['scope'],'lifecycle':'active','read-when':'When inspecting this source partition'})
   path='topics/seed/leaves/'+str(start)+'.md';seed[path]=bank_layout.index_text(path,{'schema':'agent-memory/topic-index-v1','project-id':project,'updated':stamp},routes)
   leaf_routes.append({'path':path,'purpose':'Seed partitions '+str(start)+' to '+str(min(start+47,a.lessons-1)),'scope':'component:src/seed','lifecycle':'active','read-when':'When inspecting seed partition policies'})
  parents=[]
  for start in range(0,len(leaf_routes),48):
   path='topics/seed/groups/'+str(start)+'.md';seed[path]=bank_layout.index_text(path,{'schema':'agent-memory/topic-index-v1','project-id':project,'updated':stamp},leaf_routes[start:start+48])
   parents.append({'path':path,'purpose':'Seed partition policy group '+str(start),'scope':'component:src/seed','lifecycle':'active','read-when':'When inspecting these source partitions'})
  meta,routes,omitted=bank_contract.parse_router((bank/'MEMORY.md').read_text());routes.extend(parents)
  (bank/'MEMORY.md').write_text(bank_contract.router_text(meta,routes,omitted))
  for name,text in seed.items():
   target=bank/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(text)
  assert len(seed_reference)==a.lessons and len({value['id'] for value in seed_reference.values()})==a.lessons
  (out/'seed-reference.json').write_text(json.dumps(seed_reference,indent=2)+'\n')
  from agent_memory import records
  for name,text in seed.items():
   if '/records/' in name:
    metadata,body=unpack(text);records.validate(metadata,body)
  bank_contract.validate_files({p.relative_to(bank).as_posix():p.read_text() for p in bank.rglob('*') if p.is_file()})
  if a.semantic_scenarios:(bank/'projectbrief.md').write_text('# Shared project\n\n## Archive preparation\nRecord the archive checksum.\n\n## Archive audit\nRetain the archive evidence.\n')
  h.github(w,repo,bank)
  w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Shared authored daily-use seed',cwd=repo);w.git('push','origin','HEAD:main',cwd=repo)
  remote=json.loads(w.inside('github-policy.json').read_text())['remote']
  anchor_for_code=repo
  anchors=[repo]
  for machine in (1,2):
   current=worlds[machine];current_pack=current.inside('portable pack');shutil.copytree(pack,current_pack)
   publication_github.install(current,current.paths['repo'],Path(remote))
   anchor=current.inside('anchor');current.git('clone','--',str(remote),str(anchor))
   current.git('remote','set-url','origin','https://github.com/fixture/memory.git',cwd=anchor)
   current.git('config','--local','agent-memory.authorityBranch','main',cwd=anchor)
   profile(current,current_pack,actor_b if machine==1 else actor_a,'Authored developer B' if machine==1 else 'Authored developer A')
   anchors.append(anchor)
  for number in range(a.sessions):
   machine=number%3;current=worlds[machine];anchor=anchors[machine];checkout=current.inside('sessions/session-'+str(number));checkout.parent.mkdir(exist_ok=True)
   current.git('worktree','add','-b','session-'+str(number),str(checkout),'main',cwd=anchor)
   status=bind(current,checkout)
   own=checkout/('feature-'+str(number)+'.txt');own.write_text('Unmerged feature ancestor for this session.\n')
   current.git('add',own.name,cwd=checkout);current.git('commit','-m','Unmerged feature session '+str(number),cwd=checkout)
   (checkout/'code.py').write_text('VALUE = '+str(number+100)+'  # staged session feature\n');current.git('add','code.py',cwd=checkout)
   session={'world':current,'repo':checkout,'pack':pack if machine==0 else current.inside('portable pack'),
    'machine':machine,'number':number,'name':'session-'+str(number),'actor':status['developer_id'],'project':project,'harness':('claude','codex','opencode','custom')[number%4]}
   session['seed_modes']={path:stat.S_IMODE((checkout/'memory-bank'/path).stat().st_mode) for path in seed_reference}
   session['feature']=state(session);session['initial_feature']=copy.deepcopy(session['feature']);sessions.append(session)
   seed_check(session,'initial')
  assert len({s['actor'] for s in sessions})==2 and len({str(s['repo']) for s in sessions})==a.sessions
  event('sessions-created',sessions=a.sessions,seed_lessons=a.lessons,independent_clones=3,developers=2,harness_labels=sorted({s['harness'] for s in sessions}),all_own_worktrees=True)
  for round_number in (1,2):
   with ThreadPoolExecutor(max_workers=a.workers) as executor:
    for identity,value in executor.map(lambda s:capture(s,round_number),sessions):
     with lock:lessons[identity]=value
   if a.semantic_scenarios and round_number==2:
    for n,role in enumerate(('wall-yellow','wall-blue','retry-dev','retry-prod')):
     identity,value=capture(sessions[n],round_number,role);lessons[identity]=value
    same_file_and_framing(sessions)
   event('all-captures-admitted',round=round_number,lessons=len(lessons))
   with ThreadPoolExecutor(max_workers=a.workers) as executor:prepared=list(executor.map(lambda s:prepare_publication(s,'round-'+str(round_number)+'-initial'),sessions))
   # All N source-bound pending writers truly predate the first actual push.
   authority={request['base'] for request in prepared};assert len(authority)==1
   event('all-pending-writers-prepared',round=round_number,count=len(prepared),authority=next(iter(authority)))
   order=list(range(a.sessions));order=order if round_number==1 else list(reversed(order));retries=0
   landed_peers=[]
   if a.recovery_controls and round_number==2:offline_start(sessions[0])

   for index in order:
    s=sessions[index]
    if a.recovery_controls and round_number==2 and index==0:offline_rejoin(s,landed_peers)
    result=land(s,prepared[index],'round-'+str(round_number)+'-original')
    if not result['publication_verified']:
     assert result['sharing_state']=='pending-reconciliation',result
     retries+=1;request=prepare_publication(s,'round-'+str(round_number)+'-reconciled')
     result=land(s,request,'round-'+str(round_number)+'-reconciled')
    assert result['publication_verified'] and state(s)==s['feature'],result
    if round_number==2 and index!=0:landed_peers.append(index)
    if a.recovery_controls and round_number==1 and index==0:
     before_replay=s['world'].git('ls-remote','origin','refs/heads/main',cwd=s['repo']).stdout
     replay=land(s,prepared[index],'round-1-lost-response')
     assert replay['publication_verified'] and replay['sharing_state']=='already-landed'
     assert before_replay==s['world'].git('ls-remote','origin','refs/heads/main',cwd=s['repo']).stdout
     event('lost-publication-response-read-back',session=index,duplicate_push=False)

   event('compatible-round-landed',round=round_number,order=order,stale_reconciliations=retries)
   def refresh_peer(s):
    target=next((identity,row) for identity,row in lessons.items() if row['session']==(s['number']+1)%a.sessions and row['round']==round_number)
    return refresh(s,round_number,target)
   with ThreadPoolExecutor(max_workers=a.workers) as executor:results=list(executor.map(refresh_peer,sessions))
   event('round-peer-recall',round=round_number,results=results)
   # A sampled peer query alone cannot prove that other sessions' learning
   # survived. Compare every ordinary admitted source in every refreshed bank.
   ordinary={identity:row for identity,row in lessons.items() if row.get('role') is None and row['round']<=round_number}
   for current in sessions:
    for identity,row in ordinary.items():
     metadata,body=unpack((current['repo']/'memory-bank'/row['path']).read_text())
     assert metadata==row['metadata'] and body==row['body'] and stat.S_IMODE((current['repo']/'memory-bank'/row['path']).stat().st_mode)==row['mode_bits'],(current['number'],identity,'admitted-source-not-preserved')
    assert state(current)==current['feature']
    seed_check(current,'round-'+str(round_number))
   event('all-round-learning-preserved',round=round_number,unique_sources=len(ordinary),refreshed_banks=len(sessions),exact_body_metadata_origin_conditions_evidence=True)
   if a.recovery_controls and round_number==1:
    lost=sessions[1];cursors=[path for path in lost['world'].paths['state'].rglob('refresh-cursor.json') if json.loads(path.read_text()).get('repo_root')==str(lost['repo'])]
    assert len(cursors)==1;retain(cursors[0].read_bytes());cursors[0].unlink()
    boundary=lost['world'].inside('inputs/lost-cursor-observation.json')
    public(lost['world'],lost['pack'],'refresh_memory.py','inspect','--v2','--repo',lost['repo'],'--output',boundary)
    value=json.loads(boundary.read_text());assert value['baseline_source']=='git-common-base'
    event('lost-cursor-reconstructed',session=1,baseline_source=value['baseline_source'])

  if a.semantic_scenarios:post_round_semantics(sessions)
  for current in sessions:seed_check(current,'final',allow_designated_prune=a.semantic_scenarios)
  require_scenario_events()
  outcome.update(complete_compatible_rounds=True,recovery_controls_executed=a.recovery_controls,sessions=a.sessions,seed_lessons=a.lessons,developers=2,independent_clones=3,
   captured_lessons=len(lessons),all_sessions_expected_feature_state_preserved=all(state(s)==s['feature'] for s in sessions),ordinary_code_integration_explicitly_expected=a.semantic_scenarios)
  event('compatible-rounds-complete',**outcome)
except BaseException as exc:
 outcome['failure']={'type':type(exc).__name__,'message':str(exc)}
 (out/'failure.txt').write_text(traceback.format_exc());raise
finally:
 outcome['seconds']=time.perf_counter()-started
 (out/'outcome.json').write_text(json.dumps(outcome,indent=2)+'\n')
 verify_source('after')
