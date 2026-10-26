"""Independent retained-cohort audit: stdlib/Git reads only, no product imports."""
import ast,collections,functools,gzip,hashlib,json,pathlib,re,statistics,subprocess
ROOT=pathlib.Path('/opt/agent-memory-worktrees/v003-release-integration');R=pathlib.Path('/tmp/agent-memory-v003');P=R/'daily-use-v1-required-12-1000-seed-kind-v2/linux';SHA='c7aa380d2ea664ae627aafd31b508d4956cb48cb';D='docs/research/data/2026-10-26-daily-use-final/required12-corrected/'
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
check(len(commands)==977 and {c['number'] for c in commands}==set(range(1,978)),'977 unique attempts');check(not outcome.get('failure') and outcome['complete_compatible_rounds'] and not outcome['full_daily_use_v1_acceptance'] and not outcome['required_fault_matrix_complete'],'Cohort only completion')
check(outcome['seconds']==summary['seconds'] and outcome['seconds']==4129.384333337657,'Wall receipt')
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
check(collections.Counter(c['exit'] for c in commands)=={0:969,1:7,86:1},'Exact exit counts')
# Source-derived source/profile identity and actual fresh baseline schedule.
check(len({a['metadata']['project-id'] for a in admitted})==1 and len({a['metadata']['origin']['developer-id'] for a in admitted})==2,'Same project/two developers')
check({a['metadata']['origin']['harness'] for a in admitted}=={'claude','codex','opencode','custom'},'Four declared labels');check(len({re.sub(r'/sessions/session-\d+$','',next(c['argv'][c['argv'].index('--repo')+1] for c in commands if '--repo' in c['argv'] and c['argv'][c['argv'].index('--repo')+1].endswith('/sessions/session-'+str(n)))) for n in range(12)})==3,'Three operational clones')
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
check(set(round_maps)=={(s,r) for s in range(12) for r in [1,2]},'Every bank/two actual refresh maps')
for (session,roundnum),files in round_maps.items():
 if roundnum==2:check('verify the archive checksum' in files['projectbrief.md'] and 'evidence and its source identity' in files['projectbrief.md'],'Actual same-file merged prose in bank '+str(session))
# All seed inventories are actual retained JSON bodies, not event booleans alone.
check(len(seeds)==1000 and len({v['id'] for v in seeds.values()})==1000,'1000 distinct seed IDs');seed_events=[e for e in events if e['event']=='seed-learning-preserved'];check(len(seed_events)==48,'48 seed snapshots');initial_modes={}
for e in seed_events:
 values=obj(e['inventory_sha256']);deleted={'topics/seed/records/0.md'} if e['phase']=='final' and e['session'] in {0,2,11} else set();check(set(values)==set(seeds)-deleted and set(e['designated_pruned_paths'])==deleted,'Narrow exact seed pathset '+str((e['session'],e['phase'])))
 if e['phase']=='initial':initial_modes[e['session']]={p:v['mode_bits'] for p,v in values.items()}
 for path,v in values.items():check(v['sha256']==seeds[path]['sha256'] and v['id']==seeds[path]['id'] and v['mode_bits']==initial_modes[e['session']][path],'Actual retained seed hash/ID/mode '+str((e['session'],e['phase'],path)))
for key,files in round_maps.items():
 for path,v in seeds.items():check(hashlib.sha256(files[path].encode()).hexdigest()==v['sha256'],'Seed actual proposal bytes '+str((key,path)))
# Independently inspect required negative acknowledgments, not just event labels.
check(len(negative_inputs['duplicated-page-judgment.json']['pages'])>len(set(negative_inputs['duplicated-page-judgment.json']['pages'])),'Duplicate page actually supplied')
check(len(negative_inputs['missing-edge-judgment.json']['pairs'])+1==len(negative_inputs['stale-pages-judgment.json']['pairs']),'One actual edge missing relative same review')
held=[(n,p) for n,p in proofs if set(x['path'] for x in p['blocked_components']) & {r['path'] for r in roles.values() if r['family']=='wall'}];check(bool(held),'Actual wall-held proofs retained')
wall_paths={r['path'] for r in roles.values() if r['family']=='wall'};check(len(wall_paths)==2 and any(wall_paths<=set(p['blocked_paths']) and all(any(x['path']==w and x['component']=='body' for x in p['blocked_components']) for w in wall_paths) for n,p in held),'Both wall bodies actually held')
# Events require all semantics, order and complete session coverage.
byevent=collections.defaultdict(list)
for e in events:byevent[e['event']].append(e)
required=set(byevent['declared-scenario-events-complete'][0]['required']);check(len(required)==25 and required<=set(byevent),'25 required scenarios actually represented')
for phase in ['initial','round-1','round-2','final']:check({e['session'] for e in seed_events if e['phase']==phase}==set(range(12)),'Complete seed phase '+phase)
for roundnum in [1,2]:
 pending=next(e for e in byevent['all-pending-writers-prepared'] if e['round']==roundnum);landing=next(e for e in byevent['compatible-round-landed'] if e['round']==roundnum);check(pending['count']==12 and landing['stale_reconciliations']==11 and landing['order']==(list(range(12)) if roundnum==1 else list(reversed(range(12)))),'Real pending/order/retry '+str(roundnum))
 recall=next(e for e in byevent['round-peer-recall'] if e['round']==roundnum);check({r['session'] for r in recall['results']}==set(range(12)) and all(r['useful_peer_recall'] and r['feature_preserved'] for r in recall['results']),'Every peer useful '+str(roundnum))
# Actual outage source/log targets and after-peer denial evidence.
off=byevent['offline-session-still-offline-after-peer-publications'][0];denials=[json.loads(line) for line in blob(off['actual_denial_log_sha256']).decode().splitlines()];check(len(denials)>=2 and all(x['effective_repo'].endswith('/sessions/session-0') and x['command'] in ['fetch','push','ls-remote'] for x in denials),'Actual correct-target offline denials')
check(off['verified_peer_landings']==list(range(11,0,-1)) and byevent['offline-session-rejoined-after-peer-publications'][0]['seconds']>off['seconds'],'All peers precede rejoin')
# Raw code/index observations: only ordinary caller code integration changes session0.
history=[json.loads(line) for line in record(P/'feature-state-history.jsonl').decode().splitlines()];features=[]
for session in range(12):
 records=[x for x in history if x['session']=='session-'+str(session)];states={(x['code_sha256'],x['index_sha256'],x['head'],x['branch']) for x in records};check(len(states)==(2 if session==0 else 1),'Only explicit feature integration changes '+str(session))
 for x in records:check(x['code_sha256'] in uncompressed and x['index_sha256'] in uncompressed and blob(x['index_sha256']).startswith(b'DIRC'),'Retained actual code/index '+str(session))
 codes={blob(x['code_sha256']).decode() for x in records};check('VALUE = '+str(100+session)+'  # staged session feature\n' in codes,'Original staged code '+str(session))
 if session==0:check(any('900' in x for x in codes),'Explicit ordinary code integration retained')
 features.append({'session':session,'observations':len(records),'distinct_states':len(states)})
# Matched negative selectors must actually request the target record kind.
negative_recall_findings=[]
seed_kind=None
for files in round_maps.values():
 text=files['topics/seed/records/1.md'];_,front,body=text.split('---\n',2);metadata={k:json.loads(v) for k,v in (line.split(': ',1) for line in front.strip().splitlines())};seed_kind=metadata['kind'];break
for c in commands:
 argv=c['argv']
 if pathlib.Path(argv[2]).name=='retrieve.py':
  request=obj(c['inputs'][argv[argv.index('--request')+1]])
  for path in ['topics/seed/records/0.md','topics/seed/records/1.md']:
   if seeds[path]['id'] in request['ids'] and seed_kind not in request['kinds']:
    negative_recall_findings.append({'command':c['number'],'target':path,'actual_kind':seed_kind,'requested_kinds':request['kinds'],'result_count':len(outputs[c['number']]['results']),'problem':'Wrong kind filters target before intended deletion/withholding gate; empty result is not proof of that gate.'})
check(not negative_recall_findings,'No wrong-kind seed recall selectors')
# Independently inspect matched controls, not the implementer's corroboration JSON.
matched_controls=[]
for number,path,session,count in [(882,'topics/seed/records/0.md',0,1),(883,'topics/seed/records/0.md',11,1),(918,'topics/seed/records/0.md',11,0),(928,'topics/seed/records/1.md',2,1),(976,'topics/seed/records/1.md',2,0)]:
 command=commands[number-1];argv=command['argv'];check(command['number']==number and pathlib.Path(argv[2]).name=='retrieve.py' and command['exit']==0,'Actual seed retrieval command '+str(number))
 request=obj(command['inputs'][argv[argv.index('--request')+1]]);result=outputs[number]['results'];files=round_maps[(session,2)];text=files[path];_,front,body=text.split('---\n',2);metadata={k:json.loads(v) for k,v in (line.split(': ',1) for line in front.strip().splitlines())}
 check(request['ids']==[seeds[path]['id']] and request['kinds']==['semantic-fact'] and request['context']=={'component':'src/seed/'+path.rsplit('/',1)[1].removesuffix('.md')} and request['paths']==[] and request['query']=='','Matched exact seed selectors '+str(number))
 check(argv[argv.index('--repo')+1].endswith('/sessions/session-'+str(session)) and len(result)==count,'Matched exact session/result '+str(number))
 check(hashlib.sha256(text.encode()).hexdigest()==seeds[path]['sha256'] and metadata['id']==seeds[path]['id'] and metadata['kind']=='semantic-fact','Exact independently retained seed '+str(number))
 if count:check(result[0]['record']==metadata and result[0]['body']==body,'Positive exact complete record/body '+str(number))
 matched_controls.append({'command':number,'path':path,'session':session,'requested_kind':request['kinds'],'context':request['context'],'exact_seed_id':metadata['id'],'result_count':len(result),'positive_record_and_body_equal_seed':True if count else None})
check(record(P/'controller.py')==git('show','c3e82e963d6fd317b2072ca0af36aa0368e8f99b:scripts/evaluations/daily_use.py'),'Corrected c3 controller byte equality')
frozen=git_map('81fec82c1619d20c12473d23e3fdf6706d52b641');changed={p for p in set(frozen)|set(sourcebinding['tracked']) if frozen.get(p)!=sourcebinding['tracked'].get(p)}
check([p for p in sorted(changed) if not p.startswith('docs/')]==['scripts/evaluations/daily_use.py'],'Frozen81 tested code bridge only corrected controller outside docs')
# Current proof/plan content corroborates the semantic cases beyond event labels.
def command_input(number,flag='--input'):
 command=commands[number-1];argv=command['argv'];return obj(command['inputs'][argv[argv.index(flag)+1]])
def parse_record(text):
 _,front,body=text.split('---\n',2);return {k:json.loads(v) for k,v in (line.split(': ',1) for line in front.strip().splitlines())},body
correction_event=byevent['explicit-correction-consolidation-landed'][0];primary=correction_event['primary_id'];primary_path='topics/'+primary+'.md';yellow=roles[primary];blue_id,blue=next((i,r) for i,r in roles.items() if r.get('role')=='blue')
post_correction=command_input(866)['proposal']['files'];latest,body=parse_record(post_correction[primary_path]);archived,oldbody=parse_record(post_correction['history/versions/'+primary+'/v1.md']);superseded,_=parse_record(post_correction[blue['path']])
check(latest['version']==3 and 'blue' in body and archived['origin']==yellow['metadata']['origin'] and 'yellow' in oldbody,'Actual correction v3 plus attributed v1 history')
check(superseded['status']=='superseded' and superseded['origin']==blue['metadata']['origin'] and {'id':blue_id,'version':1} in latest['derived-from'] and set(blue['metadata']['evidence']).issubset(latest['evidence']) and 'user:fixture-wall-blue-clarification' in latest['evidence'],'Actual consolidation source provenance and explicit clarification')
check(blue['metadata']['origin']['developer-id']!=yellow['metadata']['origin']['developer-id'],'Actually independent developer wall origins')
pruned=command_input(917)['proposal']['files'];seed0='topics/seed/records/0.md';tombstone='history/tombstones/'+seeds[seed0]['id']+'.md'
check(seed0 not in pruned and tombstone in pruned and seeds[seed0]['id'] in pruned[tombstone],'Actual older-branch plan keeps tombstone without old claim')
withheld_proof=command_input(944);withheld_plan=command_input(975)['proposal']['files'];withheld_path='topics/withheld-replacement-2.md';old_path='topics/seed/records/1.md';required_bodies={withheld_path,old_path}
check(required_bodies <= {x['path'] for x in withheld_proof['blocked_components'] if x['component']=='body'} and any(x['component']=='route-prose' and x['path']=='MEMORY.md' and x['target_path']==withheld_path for x in withheld_proof['blocked_components']),'Actual withheld current+predecessor body and dependent parent closure')
check(b'sk-'+b'Q'*24 not in enc(withheld_proof) and b'# Withheld policy' not in enc(withheld_proof),'No hostile body in publication proof')
withheld_meta,withheld_body=parse_record(withheld_plan[withheld_path]);old_meta,_=parse_record(withheld_plan[old_path]);check(withheld_meta['visibility']=='hidden' and {'id':old_meta['id'],'version':old_meta['version']} in withheld_meta['supersedes'] and 'sk-'+('Q'*24) in withheld_body,'Actual private refresh plan preserves raw quarantined replacement and supersedes identity')
for event_name,recall_number in [('prune-and-older-branch-nonresurrection',919),('withheld-replacement-held-independent-learning-converged',977)]:
 e=byevent[event_name][0];identity=e.get('new_independent_lesson',e.get('new_independent_id'));payload=ordinary[identity];result=outputs[recall_number]['results'];check(len(result)==1 and result[0]['record']==payload['metadata'] and result[0]['body']==payload['body'],'Independent useful positive paired with '+event_name)
# Condition-separated retries are actually selected, not merely present in input.
condition_recall_checks=0
for command in commands:
 argv=command['argv']
 if pathlib.Path(argv[2]).name!='retrieve.py' or '--request' not in argv:continue
 req=obj(command['inputs'][argv[argv.index('--request')+1]])
 if req['context'] in [{'environment':'development'},{'environment':'production'}]:
  expected='development' if req['context']['environment']=='development' else 'production';result=outputs[command['number']]['results'];check(len(result)==1 and result[0]['record']['conditions']=={'environment':expected} and result[0]['record']['id'] in roles and roles[result[0]['record']['id']]['role']==expected,'Actual disjoint condition selection '+str(command['number']));condition_recall_checks+=1
check(condition_recall_checks==24,'Two matched retry conditions in every actual session')
framing=byevent['changed-framing-derived-child-closure'][0];child=framing['child'];check(any(row['path']==child and row['source_hash']==framing['unchanged_child_sha256'] for row in ordinary.values()),'Actual unchanged child bytes bound to framing closure')
# Actual recovery/readback successes exist for each intentionally lost response/fault.
check(outputs[277]['refresh_applied'] and outputs[277]['remote_freshness_verified'],'Actual after-receipt interrupted import subsequently qualified')
check(any(pathlib.Path(c['argv'][2]).name=='publish_memory.py' and c['argv'][3]=='land' and outputs.get(c['number'],{}).get('sharing_state')=='already-landed' for c in commands),'Lost publication response actual already-landed readback')
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
r={'reviewer':'codex-review-59','evidence_commit':SHA,'tree':git('rev-parse',SHA+'^{tree}').decode().strip(),'runtime_commit':sourcebinding['commit'],'source_snapshots_checked':source_checks,'controller_sha256':hashlib.sha256(record(P/'controller.py')).hexdigest(),'raw_gzip_files_verified':len(uncompressed),'compressed_bytes_verified':compressed_total,'decompressed_bytes_verified':raw_total,'public_commands':len(commands),'exit_counts':dict(collections.Counter(c['exit'] for c in commands)),'expected_refusals_and_interruption':refusal,'unique_input_blobs':len(input_refs),'valid_review_page_ack_checks':review_checks,'retained_actual_page_bodies':len(pages),'admitted_records':len(admitted),'ordinary_records':len(ordinary),'role_records':len(roles),'actual_ordinary_refresh_plan_checks':source_map_checks,'complete_refreshed_banks_rounds':len(round_maps),'seed_inventory_events_verified':len(seed_events),'seed_initial_IDs':len(seeds),'required_scenarios':len(required),'feature_observations':features,'negative_recall_findings':negative_recall_findings,'matched_seed_controls':matched_controls,'condition_recall_checks':condition_recall_checks,'semantic_content_checks':{'wall_v3_and_two_origins_history':True,'prune_tombstone_and_absence':True,'withheld_identity_closure_and_private_payload':True,'paired_independent_useful_recall':True},'source_bridge_from_81_changed_paths':sorted(changed),'publication_states':{str(k):v for k,v in publication_states.items()},'private_apply_readback_bodies_retained':private_apply_readbacks_available,'private_apply_readback_bodies_not_retained':private_apply_readbacks_missing,'seconds':outcome['seconds'],'driver_exit':driver['exit'],'errors':errors,'new_product_or_test_executions':0,'new_model_calls':0,'whole99_or_release_accepted':False,'jobs':'terminal and collected','worlds':0,'source_holds':'RELEASED','external_writer_grant':'NONE / RELEASED'}
(R/'daily-required12-corrected-review/source.json').write_text(json.dumps(r,indent=2)+'\n');(R/'daily-required12-corrected-review/evidence-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps(r,indent=2))
