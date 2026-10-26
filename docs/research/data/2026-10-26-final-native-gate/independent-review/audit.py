import ast,collections,hashlib,io,json,pathlib,re,stat,subprocess,tarfile
R=pathlib.Path('/opt/agent-memory-worktrees/v003-release-integration');O=pathlib.Path(__file__).parent
F='81fec82c1619d20c12473d23e3fdf6706d52b641';C='07176dcddc10b9c6e1da84ad38e9f454791cbcb4';errors=[];manifest={};blobs={}
def check(v,m):
 if not v:errors.append(m)
def sha(b):return hashlib.sha256(b).hexdigest()
def record(p):
 p=pathlib.Path(p);b=p.read_bytes();manifest[str(p)]={'sha256':sha(b),'bytes':len(b)};return b
def load(p):return json.loads(record(p))
def git(*a):return subprocess.check_output(['git',*a],cwd=R)
def tree(c):
 rows=[]
 for row in git('ls-tree','-r','-z',c).split(b'\0'):
  if row:
   header,p=row.split(b'\t',1);mode,kind,oid=header.decode().split();rows.append((p.decode(),int(mode,8)&0o7777,oid))
 p=subprocess.Popen(['git','cat-file','--batch'],cwd=R,stdin=subprocess.PIPE,stdout=subprocess.PIPE)
 out,_=p.communicate(('\n'.join(x[2] for x in rows)+'\n').encode());s=io.BytesIO(out);result={}
 for path,mode,oid in rows:
  h=s.readline().split();data=s.read(int(h[2]));check(s.read(1)==b'\n','batch delimiter');check(hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()==oid,'Git object '+path)
  blobs[(c,path)]=data;result[path]={'sha256':sha(data),'mode':mode}
 return result
f=tree(F);c=tree(C);registry=json.loads(blobs[(F,'tests/contracts.json')])['suites'];executables=[x for x in registry if x.get('path')]
check(len(registry)==143 and len(executables)==142,'registry cardinality')
bridge={p for p in set(f)|set(c) if f.get(p)!=c.get(p)}
check([p for p in bridge if p.startswith(('skills/','tests/','scripts/'))]==['scripts/evaluations/daily_use.py'],'bridge executable delta')
check(blobs[(C,'scripts/evaluations/daily_use.py')]==git('show','c3e82e963d6fd317b2072ca0af36aa0368e8f99b:scripts/evaluations/daily_use.py'),'approved c3 controller equality')
portable={p.removeprefix('skills/agent-memory/'):v for p,v in f.items() if p.startswith('skills/agent-memory/')}
# Static method declarations are read as syntax, never imported.
methods=collections.defaultdict(set)
for p in f:
 if p.startswith('tests/') and p.endswith('.py'):
  t=ast.parse(blobs[(F,p)],filename=p)
  for n in ast.walk(t):
   if isinstance(n,(ast.FunctionDef,ast.AsyncFunctionDef)) and n.name.startswith('test'):methods[n.name].add(p)
results={};actual_identities={}
public_base='docs/research/data/2026-10-26-final-native-gate'
for host,base,bindingbase in [('macos',pathlib.Path('/tmp/agent-memory-v003/full-macos-gate-81fec82'),pathlib.Path('/tmp/agent-memory-v003/full-macos-gate-81fec82')),('linux',pathlib.Path('/tmp/agent-memory-v003/full-linux-gate-81fec82/linux'),pathlib.Path('/tmp/agent-memory-v003/full-linux-gate-81fec82'))]:
 binding=load(bindingbase/'source-binding.json');check(binding['commit']==F and binding['tracked']==f and binding['portable']==portable,host+' all Git source bindings')
 before=load(base/'source-before.json');after=load(base/'source-after.json');check(before==after and before['commit']==F and before['files']==len(f) and before['all_tracked_bytes_and_modes_verified'] is True,host+' source before/after')
 completion=load(base/'completion.json');check(completion=={'exit':0,'passed':True,'release_complete':True,'suites':143},host+' completion')
 report=json.loads(record(base/'gate.stdout'));stderr=record(base/'gate.stderr').decode();rows=report['suites']
 check(report['schema']=='agent-memory/test-gate-v1' and report['mode']=='release' and report['passed'] is True and report['release_complete'] is True and not report.get('error'),host+' full gate status')
 check(len(rows)==143 and sum(x.get('tests',0) for x in rows)==715,host+' row/execution total')
 check([r['name'] for r in rows]==[x['name'] for x in registry],host+' registry order')
 for e,r in zip(registry,rows):
  check(r['layer']==e['layer'],host+' layer '+r['name'])
  if e.get('path'):
   check(r['status']=='passed' and r['passed'] is True and r['tests']>0 and all(r[k]==0 for k in ('errors','failures','skipped','expected_failures','unexpected_successes')),host+' counts '+r['name'])
  else:check(r['name']=='optional-stores' and r['status']=='not-applicable' and r['tests']==0 and e['scope']=='future' and r['reason']==e['reason'],host+' sole optional row')
 # Raw verbose method starts and raw unittest success summaries are paired in registry order.
 blocks=[];pos=0
 for m in re.finditer(r'^Ran (\d+) tests? in ([\d.]+)s\n\nOK\s*$',stderr,re.M):
  text=stderr[pos:m.start()];starts=re.findall(r'^(test\S+) \(([^\n]+)\) \.\.\.',text,re.M);blocks.append((int(m[1]),float(m[2]),starts,text));pos=m.end()
 check(len(blocks)==142 and not stderr[pos:].strip(),host+' full raw summary coverage')
 ids=[]
 for e,r,b in zip(executables,[r for r in rows if r['status']=='passed'],blocks):
  n,seconds,starts,text=b;check(n==r['tests']==len(starts),host+' raw counts '+e['name']);check(seconds<300,host+' raw suite deadline '+e['name'])
  for name,ident in starts:check(name in methods and ident.rsplit('.',1)[-1]==name,host+' static declaration '+ident)
  ids.append({'suite':e['name'],'path':e['path'],'tests':n,'raw_unittest_seconds':seconds,'method_ids':[ident for _,ident in starts]})
  proof=r.get('fault_partition')
  if proof:
   raw=[]
   for line in text.splitlines():
    try:v=json.loads(line)
    except ValueError:continue
    if isinstance(v,dict) and 'lifecycle_fault_partition' in v:raw.append(v['lifecycle_fault_partition'])
   check(raw==[proof],host+' raw partition binding '+e['name'])
 # Independently enforce all 24 modulo partitions against their full observed inventories.
 groups=collections.defaultdict(list)
 for r in rows:
  if 'fault_partition' in r:groups[r['fault_partition']['group']].append((r['name'],r['fault_partition']))
 check(len(groups)==6 and sum(map(len,groups.values()))==24,host+' partition cardinality')
 summaries=[]
 for group,parts in groups.items():
  by={p['index']:(name,p) for name,p in parts};check(set(by)==set(range(4)),host+' partition indexes '+group);observed=by[0][1]['observed'];digest=by[0][1]['artifact_digest'];collected=[]
  check(re.fullmatch('[0-9a-f]{64}',digest) is not None,host+' artifact syntax '+group)
  check([x['step'] for x in observed]==list(range(len(observed))) and all(set(x)=={'step','point','kind'} and type(x['step']) is int and isinstance(x['point'],str) and isinstance(x['kind'],str) for x in observed),host+' observed inventory '+group)
  check(len({x['point'] for x in observed})==len(observed),host+' unique observed '+group)
  for index,(name,p) in sorted(by.items()):
   check(name==(group if index==0 else group+'-part-'+str(index)) and p['count']==4 and p['observed']==observed and p['artifact_digest']==digest,host+' partition source coherence '+name)
   expect=[x['point'] for x in observed if x['step']%4==index];check(expect and p['executed']==expect,host+' actual selected point list '+name);collected+=p['executed']
  check(collections.Counter(collected)==collections.Counter(x['point'] for x in observed),host+' exactly once '+group)
  summaries.append({'group':group,'partitions':4,'observed_points':len(observed),'actual_interruptions_and_retries':len(collected),'artifact_digest':digest})
 check(summaries==report['fault_partition_coverage'],host+' aggregate matches raw proofs')
 # Export is independently compared with local original bytes; public summary is not used as authority.
 pubfiles={}
 for filename,private in [('completion.json',base/'completion.json'),('gate.stderr',base/'gate.stderr'),('gate.stdout',base/'gate.stdout'),('source-before.json',base/'source-before.json'),('source-after.json',base/'source-after.json'),('source-binding.json',bindingbase/'source-binding.json')]:
  p=f'{public_base}/{host}/{filename}';check(blobs[(C,p)]==record(private),host+' public/original equality '+filename);pubfiles[filename]=c[p]
 summary=json.loads(blobs[(C,f'{public_base}/{host}/summary.json')]);check(summary['source_commit']==F and summary['native_gate_passed'] and summary['registered_rows']==143 and summary['test_executions']==715 and summary['product_release_published'] is False and summary['whole_99_acceptance_claimed'] is False,host+' public summary qualified')
 # The retained archive is verified without extraction or executing any archived file.
 tarpath=bindingbase/'source.tar';record(tarpath);actual={}
 with tarfile.open(tarpath) as tf:
  for member in tf:
   if member.isdir():continue
   check(member.isfile(),host+' archive only regular files '+member.name)
   if member.isfile():actual[member.name]={'sha256':sha(tf.extractfile(member).read()),'mode':member.mode}
 check(actual==f,host+' archive exact Git bytes and modes')
 if host=='macos':
  physical={}
  for p in f:
   path=base/'source'/p;st=path.lstat();check(stat.S_ISREG(st.st_mode),host+' retained source regular '+p);physical[p]={'sha256':sha(path.read_bytes()),'mode':stat.S_IMODE(st.st_mode)}
  check(physical==f,host+' retained complete physical tracked source')
 else:
  outer=load(bindingbase/'completion.json');check(outer=={'exit':0,'workspace_retained':False,'commit':F},'Linux outer collected workspace')
  check(blobs[(C,f'{public_base}/{host}/driver-completion.json')]==record(bindingbase/'completion.json'),'Linux public driver receipt')
  record(bindingbase/'worker.py');record(bindingbase/'driver.stdout');record(bindingbase/'driver.stderr')
 results[host]={'source_commit':F,'runtime':before,'gate':{'registered_rows':143,'executable_suites':142,'method_executions':715,'optional_not_applicable_rows':1,'bad_counts':0,'raw_unittest_seconds':round(sum(b[1] for b in blocks),3),'longest_raw_unittest_seconds':max(b[1] for b in blocks)},'fault_partitions':summaries,'partitioned_subset_events':sum(x['observed_points'] for x in summaries),'original_exports_match':True,'archive_matches_git_bytes_and_modes':True,'public_files':pubfiles}
 actual_identities[host]=ids
check([[x['suite'],x['path'],x['method_ids']] for x in actual_identities['macos']]==[[x['suite'],x['path'],x['method_ids']] for x in actual_identities['linux']],'cross-platform raw method identity parity')
# Read worker/gate boundary as immutable syntax. Per-suite deadline remains exactly 300.
gate=blobs[(F,'scripts/test_gate.py')].decode();check('check=False, timeout=300, group=True' in gate,'unchanged 300 second suite boundary')
# Resolve actual reported classes through static local imports and inherited mixins.
module_cache={}
def module_nodes(path):
 if path not in module_cache:
  tree_ast=ast.parse(blobs[(F,path)],filename=path);classes={n.name:n for n in tree_ast.body if isinstance(n,ast.ClassDef)};imports={}
  for node in tree_ast.body:
   if isinstance(node,ast.ImportFrom) and node.module:
    for alias in node.names:imports[alias.asname or alias.name]=(node.module,alias.name)
   elif isinstance(node,ast.Import):
    for alias in node.names:imports[alias.asname or alias.name]=(alias.name,None)
  module_cache[path]=(classes,imports)
 return module_cache[path]
def class_methods(path,name,seen=None):
 seen=set() if seen is None else seen
 if (path,name) in seen:return set()
 seen.add((path,name));classes,imports=module_nodes(path)
 if name not in classes:
  if name in imports:
   module,original=imports[name];target='tests/'+module.replace('.','/')+'.py'
   if original is not None and (F,target) in blobs:return class_methods(target,original,seen)
  return set()
 node=classes[name];out={n.name for n in node.body if isinstance(n,(ast.FunctionDef,ast.AsyncFunctionDef)) and n.name.startswith('test')}
 for base in node.bases:
  if isinstance(base,ast.Name):out |= class_methods(path,base.id,seen)
  elif isinstance(base,ast.Attribute) and isinstance(base.value,ast.Name) and base.value.id in imports:
   module,original=imports[base.value.id]
   target='tests/'+(module+'.'+original if original else module).replace('.','/')+'.py'
   if (F,target) in blobs:out |= class_methods(target,base.attr,seen)
 return out
static_classes=0
for suite in actual_identities['macos']:
 by_class=collections.defaultdict(set)
 for ident in suite['method_ids']:
  _,class_name,method=ident.rsplit('.',2);by_class[class_name].add(method)
 for class_name,actual in by_class.items():
  expected=class_methods(suite['path'],class_name);check(actual==expected,'static complete inherited methods '+suite['suite']+'.'+class_name);static_classes+=1
output={'reviewer':'codex-review-59','scope':'bounded frozen-source native gate/source carry-forward audit; not whole #99 or release acceptance','evidence_commit':C,'evidence_tree':git('rev-parse',C+'^{tree}').decode().strip(),'executed_commit':F,'static_reported_classes_checked':static_classes,'tracked_files':len(f),'portable_files':len(portable),'source_bridge_changed_paths':sorted(bridge),'executable_bridge_changed_paths':sorted(p for p in bridge if p.startswith(('skills/','tests/','scripts/'))),'approved_controller_commit':'c3e82e963d6fd317b2072ca0af36aa0368e8f99b','results':results,'errors':errors,'all_checks_passed':not errors,'jobs':'No product/test execution or World created by this audit. All read-only subprocesses synchronous/collected.','holds':'RELEASED','external_writer_grant':'NONE'}
(O/'source.json').write_text(json.dumps(output,indent=2)+'\n');(O/'actual-suite-identities.json').write_text(json.dumps(actual_identities,indent=2)+'\n');(O/'evidence-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'errors':errors,'hosts':{k:{'gate':v['gate'],'partitioned_subset_events':v['partitioned_subset_events']} for k,v in results.items()},'bridge_paths':len(bridge),'files':len(f),'portable':len(portable),'raw_files_hashed':len(manifest)},indent=2))
