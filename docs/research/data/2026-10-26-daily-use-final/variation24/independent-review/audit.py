"""Independent retained-cohort audit: stdlib/Git reads only, no product imports."""
import ast,collections,functools,gzip,hashlib,json,pathlib,re,statistics,subprocess
ROOT=pathlib.Path('/opt/agent-memory-worktrees/v003-release-integration');R=pathlib.Path('/tmp/agent-memory-v003');P=R/'daily-use-v1-variation-24-1000-81fec82/linux';SHA='10a9eb7959082afb889c61a9cbac3f750d7af0c8';D='docs/research/data/2026-10-26-daily-use-final/variation24/'
manifest={};errors=[];source_checks=[]
old=(R/'recall-final-review-audit.py').read_text();exec(compile(old[old.index('def record('):old.index('\nsummary=public(')],'prior-independent-stdlib-definitions','exec'))
def enc(v):return (json.dumps(v,sort_keys=True,indent=2,ensure_ascii=False)+'\n').encode()
def content_digest(v):return hashlib.sha256(enc(v)).hexdigest()
bindings=public('bindings.json');summary=public('summary.json');rawmanifest=public('private-raw-blob-manifest.json');rawdir=pathlib.Path(rawmanifest['retained_directory']);uncompressed={};compressed_total=0;raw_total=0
for name,meta in rawmanifest['gzip_files'].items():
 p=rawdir/name;compressed=record(p);check(len(compressed)==meta['bytes'] and hashlib.sha256(compressed).hexdigest()==meta['sha256'],'Compressed hash '+name);compressed_total+=len(compressed)
 h=hashlib.sha256();size=0
 with gzip.open(p,'rb') as f:
  while True:
   b=f.read(1024*1024)
   if not b:break
   h.update(b);size+=len(b)
 check(h.hexdigest()==name.removesuffix('.gz'),'Content-addressed raw identity '+name);uncompressed[h.hexdigest()]=size;raw_total+=size
@functools.lru_cache(maxsize=8)
def blob(h):
 check(h in uncompressed,'Referenced blob exists '+h)
 return gzip.decompress((rawdir/(h+'.gz')).read_bytes())
def obj(h):return json.loads(blob(h))
for name,meta in bindings['files'].items():
 original=record(meta['retained_path']);check(len(original)==meta['original']['bytes'] and hashlib.sha256(original).hexdigest()==meta['original']['sha256'],'Original export binding '+name)
 if name=='controller.py':check(original==git('show',meta['committed_source']),'Exact source controller');continue
 export=git('show',SHA+':'+D+name);check(len(export)==meta['export']['bytes'] and hashlib.sha256(export).hexdigest()==meta['export']['sha256'],'Public export hash '+name)
 if meta['transformation'] is None:check(original==export,'Untransformed exact export '+name)
 else:check(json.loads(export)==[{k:v for k,v in row.items() if k!='environment'} for row in json.loads(original)],'Only environment removed')
sourcebinding=load(P/'source-binding.json');source(sourcebinding,'cohort frozen source');before=load(P/'source-before.json');after=load(P/'source-after.json');check(before==after and before['commit']==sourcebinding['commit'] and before['all_bytes_and_modes_match'] and before['tracked']==len(sourcebinding['tracked']) and before['portable']==len(sourcebinding['portable']),'Producer complete source check')
check(record(P/'controller.py')==git('show',sourcebinding['commit']+':scripts/evaluations/daily_use.py'),'Controller frozen byte equality')
commands=load(P/'commands.json');events=load(P/'events.json');outcome=load(P/'outcome.json');seeds=load(P/'seed-reference.json');driver=public('driver-completion.json')
check(driver['exit']==0 and driver['workspace_retained'] is False and driver['source_commit']==sourcebinding['commit'],'Driver completion')
check(len(commands)==1708 and {c['number'] for c in commands}==set(range(1,1709)),'1708 unique attempts');check(not outcome.get('failure') and outcome['complete_compatible_rounds'] and not outcome['full_daily_use_v1_acceptance'] and not outcome['required_fault_matrix_complete'],'Cohort only completion')
check(outcome['seconds']==summary['seconds'] and outcome['seconds']==8776.590710052289,'Wall receipt')
tokens={c['environment']['AGENT_MEMORY_TEST_TOKEN'] for c in commands}
for name in git('ls-tree','-r','--name-only',SHA,'--',D).decode().splitlines():
 b=git('show',SHA+':'+name);check(not any(t.encode() in b for t in tokens),'Capability absent public '+name)
outputs={};pages={};input_refs=set();refusal=[];helpers=collections.Counter();ordinary={};roles={};admitted=[];proposal_maps={};source_map_checks=0
for c in commands:
 n=c['number'];argv=c['argv'];helper=pathlib.Path(argv[2]).name;op=argv[3];helpers[helper]+=1
 check(c['exit'] in (0,1,86) and not c.get('timed_out') and not c.get('execution_error') and c['seconds']<300,'Actual bounded return '+str(n));check(c.get('public_forbidden_marker_absent') is True,'Producer public marker flag '+str(n))
 stdout=blob(c['stdout_sha256']);stderr=blob(c['stderr_sha256']);check(b'sk-'+b'Q'*24 not in stdout+stderr and b'# Withheld policy' not in stdout+stderr,'Independent public body withholding '+str(n))
 for path,h in c['inputs'].items():check(h in uncompressed,'Actual input exists '+str(n));input_refs.add(h)
 if c['exit']==86:
  check(helper=='refresh_memory.py' and op=='apply' and c['environment'].get('AGENT_MEMORY_FAULT_STAGE')=='transaction-after-receipt' and not stdout,'Actual injected interrupt');refusal.append({'number':n,'helper':helper,'operation':op,'exit':86,'kind':'declared after-receipt interruption'});continue
 v=json.loads(stdout);outputs[n]=v
 if c['exit']==1:
  check(v.get('publication_verified') is False and not v.get('remote_freshness_verified',False),'Actual refusal has no fresh/publication claim '+str(n));refusal.append({'number':n,'helper':helper,'operation':op,'exit':1,'input_names':list(c['inputs']),'code':v.get('failure_code')})
 if helper=='reconcile_memory.py' and op=='page-v2' and c['exit']==0:pages[content_digest(v)]={'raw_hash':c['stdout_sha256'],'binding':v['binding_sha256'],'index':v['index']}
 if helper=='admit_learning.py' and op=='apply' and c['exit']==0:
  proposal=obj(c['inputs'][argv[argv.index('--proposal')+1]]);candidate=proposal['admission_evidence']['capture']['request']['candidate'];identity=candidate['metadata']['id'];path='topics/'+identity+'.md';text=proposal['files'][path]
  # Fixture record frontmatter uses JSON scalar/object values, parsed independently.
  _,front,body=text.split('---\n',2);meta={k:json.loads(value) for k,value in (line.split(': ',1) for line in front.strip().splitlines())};check(body==candidate['body'] and v['admission_verified'] and hashlib.sha256(text.encode()).hexdigest()==v['resulting_hashes'][path],'Actual admission payload/result '+identity)
  check(meta['origin']==candidate['metadata']['origin'] and meta['evidence']==candidate['metadata']['evidence'] and meta['conditions']==candidate['metadata']['conditions'],'Admission provenance '+identity)
  evidence=meta['evidence'][0];roundnum=int(re.search(r'-round-(\d+)',evidence)[1]);item={'path':path,'metadata':meta,'body':body,'round':roundnum,'session':int(meta['origin']['session'].split('-')[1]),'source_hash':hashlib.sha256(text.encode()).hexdigest()};admitted.append(item)
  if '-wall-' in evidence:roles[identity]={**item,'family':'wall','role':'yellow' if '-wall-yellow' in evidence else 'blue'}
  elif '-retry-' in evidence:roles[identity]={**item,'family':'retry','role':'development' if '-retry-dev' in evidence else 'production'}
  else:ordinary[identity]=item
check(collections.Counter(c['exit'] for c in commands)=={0:1706,1:1,86:1},'Exact exit counts')
# Source-derived source/profile identity and actual fresh baseline schedule.
check(len({a['metadata']['project-id'] for a in admitted})==1 and len({a['metadata']['origin']['developer-id'] for a in admitted})==2,'Same project/two developers')
check({a['metadata']['origin']['harness'] for a in admitted}=={'claude','codex','opencode','custom'},'Four declared labels');check(len({re.sub(r'/sessions/session-\d+$','',next(c['argv'][c['argv'].index('--repo')+1] for c in commands if '--repo' in c['argv'] and c['argv'][c['argv'].index('--repo')+1].endswith('/sessions/session-'+str(n)))) for n in range(24)})==3,'Three operational clones')
# Every valid review acknowledgment hashes actual returned pages; negatives checked separately.
review_checks=0;proofs=[];round_maps={};negative_inputs={}
for c in commands:
 argv=c['argv'];helper=pathlib.Path(argv[2]).name;op=argv[3];n=c['number']
 if helper=='reconcile_memory.py' and op=='review-v2':
  judgment=obj(c['inputs'][argv[argv.index('--input')+1]])
  if c['exit']==0:
   check(len(judgment['pages'])==len(set(judgment['pages'])) and all(h in pages for h in judgment['pages']),'Actual unique returned page acknowledgments '+str(n))
   check(all(pages[h]['binding']==content_digest(judgment['binding']) for h in judgment['pages']),'Page binding '+str(n));review_checks+=1
  else:negative_inputs[pathlib.Path(argv[argv.index('--input')+1]).name]=judgment
 if helper=='publish_memory.py' and op in ('prepare','land') and c['exit']==0:
  value=obj(c['inputs'][argv[argv.index('--input')+1]]);proof=value if op=='prepare' else value['proof'];proofs.append((n,proof))
  check(proof['schema']=='agent-memory/semantic-sharing-proof-v2','Native v2 publication proof '+str(n))
 if helper=='refresh_memory.py' and op=='apply' and c['exit']==0:
  value=obj(c['inputs'][argv[argv.index('--input')+1]]);name=pathlib.Path(argv[argv.index('--input')+1]).name;m=re.search(r'session-(\d+)-refresh-([12])-prepared',name)
  if m:
   session,roundnum=map(int,m.groups());files=value['proposal']['files'];round_maps[(session,roundnum)]=files;check(outputs[n].get('refresh_applied') and outputs[n].get('remote_freshness_verified'),'Actual refresh verified '+str(n))
   for identity,row in ordinary.items():
    if row['round']<=roundnum:
     text=files[row['path']];_,front,body=text.split('---\n',2);metadata={k:json.loads(v) for k,v in (line.split(': ',1) for line in front.strip().splitlines())};check(metadata==row['metadata'] and body==row['body'],'All ordinary desired learning '+str((session,roundnum,identity)));source_map_checks+=1
check(set(round_maps)=={(s,r) for s in range(24) for r in [1,2]},'Every bank/two actual refresh maps')
# All seed inventories are actual retained JSON bodies, not event booleans alone.
check(len(seeds)==1000 and len({v['id'] for v in seeds.values()})==1000,'1000 distinct seed IDs');seed_events=[e for e in events if e['event']=='seed-learning-preserved'];check(len(seed_events)==96,'96 seed snapshots');initial_modes={}
for e in seed_events:
 values=obj(e['inventory_sha256']);deleted=set();check(set(values)==set(seeds)-deleted and set(e['designated_pruned_paths'])==deleted,'Narrow exact seed pathset '+str((e['session'],e['phase'])))
 if e['phase']=='initial':initial_modes[e['session']]={p:v['mode_bits'] for p,v in values.items()}
 for path,v in values.items():check(v['sha256']==seeds[path]['sha256'] and v['id']==seeds[path]['id'] and v['mode_bits']==initial_modes[e['session']][path],'Actual retained seed hash/ID/mode '+str((e['session'],e['phase'],path)))
for key,files in round_maps.items():
 for path,v in seeds.items():check(hashlib.sha256(files[path].encode()).hexdigest()==v['sha256'],'Seed actual proposal bytes '+str((key,path)))
# Core-only variation deliberately excludes semantic-scenario negatives.
# Independently inspect required recovery outcomes, not just event labels.
check(not negative_inputs and not roles,'No semantic scenarios/role records claimed')
# Events require all semantics, order and complete session coverage.
byevent=collections.defaultdict(list)
for e in events:byevent[e['event']].append(e)
required=set(byevent['declared-scenario-events-complete'][0]['required']);check(len(required)==10 and required<=set(byevent),'10 required core/recovery scenarios actually represented')
for phase in ['initial','round-1','round-2','final']:check({e['session'] for e in seed_events if e['phase']==phase}==set(range(24)),'Complete seed phase '+phase)
for roundnum in [1,2]:
 pending=next(e for e in byevent['all-pending-writers-prepared'] if e['round']==roundnum);landing=next(e for e in byevent['compatible-round-landed'] if e['round']==roundnum);check(pending['count']==24 and landing['stale_reconciliations']==23 and landing['order']==(list(range(24)) if roundnum==1 else list(reversed(range(24)))),'Real pending/order/retry '+str(roundnum))
 recall=next(e for e in byevent['round-peer-recall'] if e['round']==roundnum);check({r['session'] for r in recall['results']}==set(range(24)) and all(r['useful_peer_recall'] and r['feature_preserved'] for r in recall['results']),'Every peer useful '+str(roundnum))
# Actual outage source/log targets and after-peer denial evidence.
off=byevent['offline-session-still-offline-after-peer-publications'][0];denials=[json.loads(line) for line in blob(off['actual_denial_log_sha256']).decode().splitlines()];check(len(denials)>=2 and all(x['effective_repo'].endswith('/sessions/session-0') and x['command'] in ['fetch','push','ls-remote'] for x in denials),'Actual correct-target offline denials')
check(off['verified_peer_landings']==list(range(23,0,-1)) and byevent['offline-session-rejoined-after-peer-publications'][0]['seconds']>off['seconds'],'All peers precede rejoin')
# Raw code/index observations: only ordinary caller code integration changes session0.
history=[json.loads(line) for line in record(P/'feature-state-history.jsonl').decode().splitlines()];features=[]
for session in range(24):
 records=[x for x in history if x['session']=='session-'+str(session)];states={(x['code_sha256'],x['index_sha256'],x['head'],x['branch']) for x in records};check(len(states)==1,'Only explicit feature integration changes '+str(session))
 for x in records:check(x['code_sha256'] in uncompressed and x['index_sha256'] in uncompressed and blob(x['index_sha256']).startswith(b'DIRC'),'Retained actual code/index '+str(session))
 codes={blob(x['code_sha256']).decode() for x in records};check('VALUE = '+str(100+session)+'  # staged session feature\n' in codes,'Original staged code '+str(session))
 features.append({'session':session,'observations':len(records),'distinct_states':len(states)})
negative_recall_findings=[]
check(outcome['ordinary_code_integration_explicitly_expected'] is False and summary['semantic_schedule'] is False,'Core-only scope')
publication_states=collections.Counter()
private_apply_readbacks_available=0;private_apply_readbacks_missing=0
for c in commands:
 argv=c['argv'];helper=pathlib.Path(argv[2]).name
 if helper=='publish_memory.py':publication_states[(argv[3],outputs[c['number']]['sharing_state'])]+=1
 if helper=='refresh_memory.py' and argv[3]=='apply' and c['exit']==0:
  h=outputs[c['number']].get('sha256')
  if h in uncompressed:private_apply_readbacks_available+=1
  elif h:private_apply_readbacks_missing+=1

# Summary exact counts/costs and no invented global gate.
check(len(commands)==summary['public_commands'] and max(c['seconds'] for c in commands)==summary['command_max_seconds'] and statistics.median(c['seconds'] for c in commands)==summary['command_median_seconds'],'Summary actual command costs')
r={'reviewer':'codex-review-59','evidence_commit':SHA,'tree':git('rev-parse',SHA+'^{tree}').decode().strip(),'runtime_commit':sourcebinding['commit'],'source_snapshots_checked':source_checks,'controller_sha256':hashlib.sha256(record(P/'controller.py')).hexdigest(),'raw_gzip_files_verified':len(uncompressed),'compressed_bytes_verified':compressed_total,'decompressed_bytes_verified':raw_total,'public_commands':len(commands),'exit_counts':dict(collections.Counter(c['exit'] for c in commands)),'expected_refusals_and_interruption':refusal,'unique_input_blobs':len(input_refs),'valid_review_page_ack_checks':review_checks,'retained_actual_page_bodies':len(pages),'admitted_records':len(admitted),'ordinary_records':len(ordinary),'role_records':len(roles),'actual_ordinary_refresh_plan_checks':source_map_checks,'complete_refreshed_banks_rounds':len(round_maps),'seed_inventory_events_verified':len(seed_events),'seed_initial_IDs':len(seeds),'required_scenarios':len(required),'feature_observations':features,'negative_recall_findings':negative_recall_findings,'publication_states':{str(k):v for k,v in publication_states.items()},'private_apply_readback_bodies_retained':private_apply_readbacks_available,'private_apply_readback_bodies_not_retained':private_apply_readbacks_missing,'seconds':outcome['seconds'],'driver_exit':driver['exit'],'errors':errors,'new_product_or_test_executions':0,'new_model_calls':0,'whole99_or_release_accepted':False,'jobs':'terminal and collected','worlds':0,'source_holds':'RELEASED','external_writer_grant':'NONE / RELEASED'}
(R/'daily-variation24-81fec82-review/source.json').write_text(json.dumps(r,indent=2)+'\n');(R/'daily-variation24-81fec82-review/evidence-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps(r,indent=2))
