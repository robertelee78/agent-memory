import hashlib,json,pathlib,stat,subprocess
R=pathlib.Path('/opt/agent-memory-worktrees/v003-release-integration');O=pathlib.Path(__file__).parent;C='19a9985bd1467b13e5f6f1f7c28f17dd0a998487';D='docs/research/data/2026-10-26-publication-validation/fable-implemented-runtime/';F='81fec82c1619d20c12473d23e3fdf6706d52b641';errors=[];manifest={}
def check(ok,label):
 if not ok:errors.append(label)
def sha(b):return hashlib.sha256(b).hexdigest()
def git(*a):return subprocess.check_output(['git',*a],cwd=R)
def public(p):
 b=git('show',C+':'+p);manifest[C+':'+p]={'sha256':sha(b),'bytes':len(b)};return b
def private(p):
 p=pathlib.Path(p);b=p.read_bytes();manifest[str(p)]={'sha256':sha(b),'bytes':len(b)};return b
binding=json.loads(public(D+'bindings.json'))
for name,value in binding['files'].items():
 b=public(D+name);check(sha(b)==value['sha256'] and len(b)==value['bytes'],'published evidence hash '+name)
raw=private(binding['raw_response_retained']);check(sha(raw)==binding['raw_response_sha256'] and len(raw)==binding['raw_response_bytes'],'Actual private provider response binding');response=json.loads(raw);review=public(D+'review.md');check(review.decode().strip()==response['result'].strip(),'Verbatim actual model review')
check(response['is_error'] is False and response['subtype']=='success','Actual model response successful');check(set(response['modelUsage'])=={'claude-opus-5-5'},'Actual provider model identity')
base=pathlib.Path(binding['raw_response_retained']).parent;before=json.loads(public(D+'source-before.json'));after=json.loads(public(D+'source-after.json'));check(before==after,'Capsule before after identical');check(before['current_commit']==F and before['prior_commit']==binding['prior_source'],'Capsule commits')
counts={'current':0,'before':0}
for path,value in before['files'].items():
 side,relative=path.split('/',1);commit=F if side=='current' else binding['prior_source'];check(side in counts,'Capsule source side');b=git('show',commit+':'+relative);mode=int(git('ls-tree',commit,'--',relative).split()[0],8)&0o7777
 check(sha(b)==value['sha256'] and mode==value['mode'],'Capsule Git bytes/modes '+path)
 p=base/path;actual=private(p);check(not p.is_symlink() and stat.S_ISREG(p.lstat().st_mode) and sha(actual)==value['sha256'] and stat.S_IMODE(p.stat().st_mode)==value['mode'],'Retained actual capsule bytes/modes '+path);counts[side]+=1
for n in ['source-before.json','source-after.json','prompt.txt','review.md','result.json']:check(private(base/n)==public(D+n),'Original/public exact '+n)
for public_script,private_script,public_result,private_result in [('initial-check.py','/tmp/agent-memory-v003/fable-memo-boundaries-check.py','initial-check-result.json','/tmp/agent-memory-v003/fable-memo-boundaries-81fec82/result.json'),('clarified-check.py','/tmp/agent-memory-v003/fable-memo-boundaries-check-v2.py','clarified-check-result.json','/tmp/agent-memory-v003/fable-memo-boundaries-81fec82-v2/result.json')]:
 check(public(D+public_script)==private(private_script),'Exact diagnostic producer '+public_script);data=public(D+public_result);check(data==private(private_result),'Exact diagnostic result '+public_result);j=json.loads(data);check(j['source_commit']==F and j['platform']=='darwin' and j['map_order_changed_but_equal'] and j['same_map_cache_hit_confirmed'] and j['oversized_context_bytes']==20773 and j['oversized_map_not_cache_eligible'] and j['oversized_fallback_equals_full_validation'] and j['operation_state_cleared'] and j['source_bytes_and_modes_preserved'] and not j['whole_story_acceptance_claimed'],'Actual bounded diagnostic results '+public_result)
clarified=json.loads(public(D+'clarified-check-result.json'));check(clarified['freshness_proved_by_this_check'] is False,'Clarified no freshness proof')
paths=git('diff','--name-only',F,C).decode().splitlines();check([p for p in paths if not p.startswith('docs/')]==['scripts/evaluations/daily_use.py'],'Frozen81 runtime/tests bridge unchanged')
for p in ['skills/agent-memory/scripts/agent_memory/retrieval.py','skills/agent-memory/scripts/agent_memory/recall_catalog.py','skills/agent-memory/scripts/agent_memory/memory_refresh.py','skills/agent-memory/scripts/agent_memory/validation_memo.py','skills/agent-memory/scripts/agent_memory/bank_contract.py','skills/agent-memory/scripts/agent_memory/security.py']:
 check(public(p)==git('show',F+':'+p),'Current/source code identical '+p)
for p in ['docs/releases/v0.0.3.md','docs/research/2026-10-26-publication-validation-cost.md','docs/research/2026-10-26-v0.0.3-qualification.md']:public(p)
out={'reviewer':'codex-review-59','candidate':C,'tree':git('rev-parse',C+'^{tree}').decode().strip(),'reviewed_runtime':F,'model':binding['actual_model'],'capsule_files_verified':counts,'external_review_scope':'actual provider static read only; not product tests or release acceptance','diagnostics_scope':'two actual local macOS reorder-hit/oversized-fallback checks; no freshness or refresh latency pass','errors':errors,'all_checks_passed':not errors,'bindings':manifest,'jobs':'read-only inspection subprocesses terminal and collected','new_product_or_test_execution':False,'holds':'RELEASED','external_writer_grant':'NONE','external_writes':False,'whole99_or_release_acceptance':False};(O/'source.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps({'errors':errors,'capsule_counts':counts,'bound_artifacts':len(manifest),'tree':out['tree']},indent=2))
