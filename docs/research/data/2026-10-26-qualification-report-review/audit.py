import hashlib,json,pathlib,posixpath,re,subprocess
R=pathlib.Path('/opt/agent-memory-worktrees/v003-release-integration');O=pathlib.Path(__file__).parent;C='5d0c8542eda5298d23872f666bbcb0d6543768a5';F='81fec82c1619d20c12473d23e3fdf6706d52b641';P='docs/research/2026-10-26-v0.0.3-qualification.md';errors=[];bindings={}
def git(*a):return subprocess.check_output(['git',*a],cwd=R)
def check(ok,msg):
 if not ok:errors.append(msg)
def bound(path):
 b=git('show',C+':'+path);bindings[path]={'sha256':hashlib.sha256(b).hexdigest(),'bytes':len(b),'blob':git('rev-parse',C+':'+path).decode().strip()};return b
report=bound(P).decode();tree_paths=set(git('ls-tree','-r','--name-only',C).decode().splitlines());links=[]
for target in re.findall(r'\]\(([^)]+)\)',report):
 if '://' in target:continue
 p=posixpath.normpath(posixpath.join(posixpath.dirname(P),target.split('#')[0]));check(p in tree_paths,'Missing report reference '+p);links.append(p);bound(p)
parent=git('rev-parse',C+'^').decode().strip();delta=git('diff','--name-only',parent,C).decode().splitlines();check(delta==[P],'Consolidation commit report only')
# Compare path/mode/object identity rather than only diff filenames.
def source_entries(commit):
 out={}
 for x in git('ls-tree','-r','-z',commit).split(b'\0'):
  if x:
   h,p=x.split(b'\t',1);out[p.decode()]=h.decode()
 return out
f=source_entries(F);c=source_entries(C);changes=sorted(p for p in set(f)|set(c) if f.get(p)!=c.get(p));check([p for p in changes if not p.startswith('docs/')]==['scripts/evaluations/daily_use.py'],'Complete frozen81 non-doc source bridge')
controller=bound('scripts/evaluations/daily_use.py');check(controller==git('show','c3e82e963d6fd317b2072ca0af36aa0368e8f99b:scripts/evaluations/daily_use.py'),'c3 controller exact');check(controller==git('show','f92be620065d0565a936782c924e183605026026:scripts/evaluations/daily_use.py'),'f92 controller exact')
reviews=[('docs/research/data/2026-10-26-daily-use-final/required12-corrected/independent-review/review.md','/tmp/agent-memory-v003/daily-required12-corrected-review/review.md'),('docs/research/data/2026-10-26-daily-use-final/variation24/independent-review/review.md','/tmp/agent-memory-v003/daily-variation24-81fec82-review/review.md'),('docs/research/data/2026-10-26-final-native-gate/independent-review/review.md','/tmp/agent-memory-v003/final-native-81fec82-review/review.md'),('docs/research/data/2026-10-26-recall-target/independent-review/review.md','/tmp/agent-memory-v003/recall-seven-second-review/review.md')]
for public,private in reviews:
 b=pathlib.Path(private).read_bytes();check(bound(public)==b,'Exact actual independent review export '+public);bindings[private]={'sha256':hashlib.sha256(b).hexdigest(),'bytes':len(b)}
required=json.loads(bound('docs/research/data/2026-10-26-daily-use-final/required12-corrected/summary.json'));variation=json.loads(bound('docs/research/data/2026-10-26-daily-use-final/variation24/summary.json'));target=json.loads(bound('docs/research/data/2026-10-26-recall-target/summary.json'))
check(required['public_commands']==977 and required['seconds']==4129.384333337657 and required['source_commit']=='f92be620065d0565a936782c924e183605026026' and required['required_scenario_events']==25,'Corrected required12 stated data')
check(variation['public_commands']==1708 and variation['seconds']==8776.590710052289 and variation['source_commit']==F,'Core24 stated data')
check(target['warm_native_target_seconds_exclusive']==7 and target['ai_thinking_time_included'] is False and target['quality_criteria_waived'] is False and target['universal_latency_guarantee'] is False,'Target distinction')
for host in ['macos','linux']:
 j=json.loads(bound('docs/research/data/2026-10-26-final-native-gate/'+host+'/summary.json'));check(j['source_commit']==F and j['registered_rows']==143 and j['test_executions']==715 and j['native_gate_passed'],'Native summary '+host)
status=json.loads(bound('docs/research/data/2026-10-26-daily-use-final/required12/qualification-status.json'));check(status['required_semantic_cohort_accepted'] is False and status['controller_exit']==0 and status['product_failure_inferred'] is False,'Original bad cohort status preserved')
for p in ['scripts/distribution/release.py','skills/agent-memory/scripts/agent_memory/release_distribution.py']:bound(p)
result={'reviewer':'codex-review-59','scope':'bounded static consolidated-report fidelity; not whole99 or release acceptance','candidate':C,'tree':git('rev-parse',C+'^{tree}').decode().strip(),'parent':parent,'changed_paths':delta,'source_bridge_from':F,'all_non_docs_changes':['scripts/evaluations/daily_use.py'],'all_bridge_changed_paths':changes,'references_checked':links,'actual_independent_review_exports_equal':len(reviews),'bindings':bindings,'errors':errors,'all_checks_passed':not errors,'new_tests_or_product_execution':False,'external_writes':False,'holds':'RELEASED','external_writer_grant':'NONE'}
(O/'source.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'errors':errors,'candidate':C,'parent':parent,'checked_references':len(links),'bridge_changed_files':len(changes)},indent=2))
