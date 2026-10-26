"""Independent stdlib-only Git/raw evidence audit; no product imports/execution."""
import ast,hashlib,json,pathlib,re,statistics,subprocess
ROOT=pathlib.Path('/opt/agent-memory-worktrees/v003-release-integration');R=pathlib.Path('/tmp/agent-memory-v003');SHA='8d2163bd3c6c5555aeaffc175bf2207917572dd8';D='docs/research/data/2026-10-26-recall-target/'
manifest={};errors=[];source_checks=[]
old=(R/'recall-final-review-audit.py').read_text();defs=old[old.index('def record('):old.index('\nsummary=public(')];exec(compile(defs,'prior-independent-stdlib-definitions','exec'))
summary=public('summary.json');runtime81=git('rev-parse','81fec82').decode().strip();check(not git('diff','--name-only',runtime81,SHA,'--','skills','tests','scripts').strip(),'Runtime/test/controller code identical frozen81')
check(not git('diff','--name-only','e6ad11c6ccc5bd2a67c0f6dfe49557dcfa4b4346',SHA,'--','skills','tests','scripts').strip(),'Documentation-only amendment')
results=[]
for run in summary['runs']:
 label=run['label'];m=public(label+'/raw-evidence-manifest.json');p=pathlib.Path(m['retained_directory']);exported=[]
 for name,meta in m['files'].items():
  b=record(p/name);check(len(b)==meta['bytes'] and hashlib.sha256(b).hexdigest()==meta['sha256'],'Raw binding '+label+'/'+name)
  if name.endswith('.json') and name in ['commands.json','observations.json','result.json','source-after.json','source-before.json']:
   check(b==git('show',SHA+':'+D+label+'/'+name),'Public/raw equality '+label+'/'+name);exported.append(name)
 before=load(p/'source-before.json');after=load(p/'source-after.json');source(before,label+'/before');check(before==after and before['commit']==run['source_commit'],'Source preservation '+label)
 v=load(p/'result.json');check(v['commit']==before['commit'] and v['bank_feature_index_head_preserved'] and v['semantic_quality_claimed'] is False and v['performance_acceptance_claimed'] is False and v['cold_filesystem_cache_claimed'] is False,'Result qualifications '+label)
 commands=load(p/'commands.json');check(len(commands)==12,'12 commands '+label);qualified=[]
 for n,c in enumerate(commands,1):
  out=load(p/f'{n}.stdout');err=record(p/f'{n}.stderr');check(c['exit']==0 and err==b'','Successful actual call '+label+'/'+str(n))
  check(c['argv'][2].endswith('/scripts/discover_memory.py') and c['argv'][3] in ['start','node','read'],'Public discovery operation '+label)
  if n%4==0:
   q=out['qualification'];check(out['qualification_completed'] and len(q['results'])==1 and not out['discovery']['sources'] and out['discovery']['complete'] is False,'Qualified/private trace '+label)
   row_checks(q['results'][0],q['project_id'],label+'/'+str(n));qualified.append(q['results'][0])
   check(q['metrics']==v['observations'][n//4-1]['qualifier_metrics'],'Raw metrics match '+label)
 check([c['seconds'] for c in commands]==[x for o in v['observations'] for x in o['public_operation_seconds']],'Exact raw times '+label)
 for o in v['observations']:
  check(o['whole_workflow_seconds']>=sum(o['public_operation_seconds']) and o['whole_workflow_seconds']-sum(o['public_operation_seconds'])<.1,'Complete elapsed boundary '+label)
 check(v['observations'][0]['whole_workflow_seconds']==run['cold_rebuild_seconds'] and [o['whole_workflow_seconds'] for o in v['observations'][1:]]==run['warm_complete_workflow_seconds'],'Summary timing match '+label)
 check(all(o['whole_workflow_seconds']<7 for o in v['observations'][1:]),'Recorded warm under7 '+label)
 producer=git('show',SHA+':'+D+label+'/producer.py');private=R/('measure-full-navigation-final-validation.py' if label=='repaired-runtime' else 'measure-full-navigation-785ae41.py');check(producer==record(private),'Retained producer exact '+label)
 results.append({'label':label,'source':before['commit'],'tracked_files':len(before['tracked']),'portable_files':len(before['portable']),'source_Git_bytes_modes_match':True,'raw_manifest_entries':len(m['files']),'public_json_exports_equal_raw':exported,'commands':len(commands),'warm_full_native_seconds':run['warm_complete_workflow_seconds'],'cold_full_native_seconds':run['cold_rebuild_seconds'],'setup_seconds':v['setup_seconds'],'fixture':v['fixture'],'qualified_rows':qualified,'producer_sha256':hashlib.sha256(producer).hexdigest()})
check(results[0]['fixture']==results[1]['fixture'],'Identical semantic corpus statistics')
check([(x['path'],x['body'],x['record'],x['source_sha256']) for x in results[0]['qualified_rows']]==[(x['path'],x['body'],x['record'],x['source_sha256']) for x in results[1]['qualified_rows']],'Same actual qualified sources/body/metadata/hash')
base='e6ad11c6ccc5bd2a67c0f6dfe49557dcfa4b4346';changed=git('diff','--name-only',base,SHA).decode().splitlines();bound=[]
for name in changed:
 b=git('show',SHA+':'+name);mode,kind,oid=git('ls-tree',SHA,'--',name).split(b'\t')[0].decode().split();bound.append({'path':name,'sha256':hashlib.sha256(b).hexdigest(),'bytes':len(b),'mode':mode,'git_blob':oid})
r={'reviewer':'codex-review-59','candidate':SHA,'tree':git('rev-parse',SHA+'^{tree}').decode().strip(),'comparison_base':base,'frozen_runtime_commit':runtime81,'runtime_test_controller_bytes_modes_unchanged':True,'changed_files':bound,'source_snapshots_checked':source_checks,'runs':results,'errors':errors,'raw_files_hashed':len(manifest),'new_product_executions':0,'new_models':0,'scope':'supplemental product-target/docs/evidence review, not whole #99 acceptance or release','holds':'RELEASED','external_writer_grant':'NONE / RELEASED','jobs':'terminal and collected','worlds':0}
(R/'recall-seven-second-review/source.json').write_text(json.dumps(r,indent=2)+'\n');(R/'recall-seven-second-review/evidence-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps({'errors':errors,'files_hashed':len(manifest),'runs':[{k:v for k,v in x.items() if k not in ['qualified_rows','public_json_exports_equal_raw']} for x in results]},indent=2))
