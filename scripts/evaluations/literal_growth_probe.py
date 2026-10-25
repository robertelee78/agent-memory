"""Design-only literal-growth observations; not the labeled quality suite or stress gate."""
import sys
sys.dont_write_bytecode=True
import argparse,copy,hashlib,json,os,posixpath,re,shutil,subprocess,time,uuid,platform
from pathlib import Path
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--root',required=True,help='Trusted candidate checkout')
parser.add_argument('--baseline',required=True,help='Trusted actual v0.0.1 checkout')
parser.add_argument('--out',required=True,help='Private absolute output directory outside both sources')
parser.add_argument('--counts',type=int,nargs='+',choices=(1000,10000,50000),default=[1000,10000,50000])
a=parser.parse_args()
ROOT=Path(a.root).expanduser().resolve(strict=True)
BASE=Path(a.baseline).expanduser().resolve(strict=True)
OUT=Path(a.out).expanduser()
if not OUT.is_absolute():raise ValueError('Output needs an absolute operator-owned private path')
OUT=OUT.resolve()
if any(OUT.is_relative_to(p) or p.is_relative_to(OUT) for p in (ROOT,BASE)):
 raise ValueError('Output must remain outside both source checkouts')
OUT.mkdir(mode=0o700,parents=True,exist_ok=True)
if OUT.is_symlink() or OUT.stat().st_uid!=os.getuid() or OUT.stat().st_mode & 0o077:
 raise ValueError('Preserve existing output; select an owner-only private directory')
sys.path.insert(0,str(ROOT/'tests'))
from world import World
PID='7eea1199-8123-4989-8cde-012d3361c8ee'
STAMP='2026-10-25T08:00:00Z'
CONTEXTS=['projectbrief.md','productContext.md','activeContext.md','systemPatterns.md','techContext.md','progress.md']
TEMPLATE=json.loads((ROOT/'skills/agent-memory/templates/record-proposal.example.json').read_text())['metadata']
def dump(v):return json.dumps(v,sort_keys=True,separators=(',',':'))
def header(v):return '---\n'+''.join(k+': '+json.dumps(x)+'\n' for k,x in v.items())+'---\n'
def row(path,source='MEMORY.md'):
 link=posixpath.relpath(path,posixpath.dirname(source) or '.')
 return '| ['+path+'](<'+link+'>) | Isolated fixture learning | repository | active | During this design experiment |\n'
def table(paths,source='MEMORY.md'):
 return '| Path | Purpose | Scope | Lifecycle | Read when |\n| --- | --- | --- | --- | --- |\n'+''.join(row(p,source) for p in paths)
def index(path,children):
 return header({'schema':'agent-memory/topic-index-v1','project-id':PID,'updated':STAMP})+'# Topic index\n\n## Routes\n\n'+table(children,path)
def corpus(count):
 files={'projectbrief.md':'# Project\n\nDisposable growth experiment; not actual admitted learning.\n'}
 leaves=[]
 for start in range(0,count,64):
  path='topics/leaves/index-'+str(start//64).zfill(4)+'.md'; children=[]
  for n in range(start,min(start+64,count)):
   name='topics/records/lesson-'+str(n).zfill(6)+'.md';children.append(name)
   m=copy.deepcopy(TEMPLATE);m.update({'id':str(uuid.UUID(bytes=hashlib.sha256(('fixture-'+str(n)).encode()).digest()[:16],version=4)),
    'project-id':PID,'kind':'semantic-fact','status':'active','scope':'repository','conditions':{},'trust':'reviewed','visibility':'visible',
    'admission-reason':'Structural fixture only; not real semantic admission evidence'})
   m['origin']['code-state']='not-code'
   body='# Baselinegrowth sentinel\n\nThe disposable target has its own evidence and provenance.\n' if n==0 else '# Distractor '+str(n).zfill(6)+'\n\nDisposable unrelated fixture text.\n'
   files[name]=header(m)+body
  files[path]=index(path,children);leaves.append(path)
 groups=[]
 for start in range(0,len(leaves),64):
  path='topics/groups/group-'+str(start//64).zfill(3)+'.md';files[path]=index(path,leaves[start:start+64]);groups.append(path)
 files['MEMORY.md']=header({'schema':'agent-memory/v1','project-id':PID,'router-updated':STAMP})+'# Project memory\n\n## Routes\n\n'+table(['projectbrief.md',*groups])+'\n## Omitted contexts\n\n'+''.join('- `'+p+'`: No separate evidence in this fixture\n' for p in CONTEXTS[1:])
 return files
def inventory(path):return {p.relative_to(path).as_posix():{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'mode':p.stat().st_mode & 0o777} for p in path.rglob('*') if p.is_file()}
def invoke(w,pack,name,*args):
 r=w.run([sys.executable,'-I',pack/'scripts'/name,*args],check=False,timeout=300,group=True)
 return r,json.loads(r.stdout) if r.stdout else None
def save(w,value,name='input.json'):
 p=w.inside(name);p.write_text(json.dumps(value));return p
def bind(w,pack,repo):
 _,v=invoke(w,pack,'developer_identity.py','status','--repo',repo)
 if v['setup_required']:
  r,p=invoke(w,pack,'developer_identity.py','preview','--create','--label','Isolated baseline developer');assert r.returncode==0,p
  r,v=invoke(w,pack,'developer_identity.py','apply','--proposal',save(w,p));assert r.returncode==0,v
 r,p=invoke(w,pack,'scope_binding.py','preview','--repo',repo,'--action','bind','--reason','Isolated structural fixture adoption');assert r.returncode==0,p
 r,v=invoke(w,pack,'scope_binding.py','apply','--proposal',save(w,p));assert r.returncode==0,v
sources=[]
expected_versions={}
for label,root in [('published-v0.0.1',BASE),('pre-scale-repair-candidate',ROOT)]:
 sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
 assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=root,text=True).strip()
 if label=='published-v0.0.1':
  assert sha=='63d3a64c0e535148cc1b5deb9d67169dbbc7c525','Baseline must be the actual published v0.0.1 source'
 expected_versions[label]=re.search(r'^metadata: \{"version": "([^\"]+)"\}',(root/'skills/agent-memory/SKILL.md').read_text(),re.M)[1]
 expected={}
 for line in subprocess.check_output(['git','ls-tree','-r',sha,'--','skills/agent-memory'],cwd=root,text=True).splitlines():
  descriptor,name=line.split('\t',1);mode,kind,oid=descriptor.split();assert kind=='blob'
  raw=subprocess.check_output(['git','cat-file','blob',oid],cwd=root)
  expected[name.removeprefix('skills/agent-memory/')]={'sha256':hashlib.sha256(raw).hexdigest(),'mode':0o755 if mode=='100755' else 0o644}
 assert inventory(root/'skills/agent-memory')==expected,'Complete source bytes/modes must match committed artifact'
 sources.append((label,root,sha,expected))
observations=[]
for count in a.counts:
 files=corpus(count);fixture_sha=hashlib.sha256(dump({n:hashlib.sha256(b.encode()).hexdigest() for n,b in files.items()}).encode()).hexdigest()
 for label,root,sha,expected in sources:
  with World() as w:
   pack=w.inside('trusted-source/skills/agent-memory');shutil.copytree(root/'skills/agent-memory',pack)
   assert inventory(pack)==expected
   r,installed=invoke(w,pack,'installer.py','install','--development-source',pack,'--presets','none');assert r.returncode==0,installed
   receipt=installed['receipt'];assert receipt['source']['version']==expected_versions[label]
   repo=w.repository();(repo/'feature.py').write_text('fixture = True\n');w.git('add','feature.py',cwd=repo);w.git('commit','-m','Disposable project evidence',cwd=repo)
   (repo/'feature.py').write_text('uncommitted_feature = True\n');w.git('add','feature.py',cwd=repo)
   bank=repo/'memory-bank';bank.mkdir()
   for name,body in files.items():
    path=bank/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(body)
   bind(w,pack,repo);before=inventory(bank);index_before=(repo/'.git/index').read_bytes();head_before=w.git('rev-parse','HEAD',cwd=repo).stdout
   started=time.monotonic();r,validated=invoke(w,pack,'bank_contract.py','validate','--repo',repo);validation_seconds=time.monotonic()-started
   (OUT/(str(count)+'-'+label+'-validation.json')).write_text(dump({'exit':r.returncode,'stdout_sha256':hashlib.sha256(r.stdout.encode()).hexdigest(),'diagnostic':validated.get('error') if isinstance(validated,dict) else None,'seconds':validation_seconds}))
   queries=[]
   if r.returncode==0:
    request={'project_id':PID,'query':'baselinegrowth sentinel','ids':[],'paths':[],'context':{},'kinds':['semantic-fact'],'minimum_trust':'reviewed','limit':8,'include_context':False}
    for attempt in range(2):
     started=time.monotonic();read,value=invoke(w,pack,'retrieve.py','--repo',repo,'--request',save(w,request));seconds=time.monotonic()-started
     (OUT/(str(count)+'-'+label+'-query-'+str(attempt)+'.json')).write_text(read.stdout)
     results=value.get('results') if isinstance(value,dict) else None
     queries.append({'exit':read.returncode,'seconds':seconds,'result_count':len(results) if results is not None else None,'selected_paths':[v['path'] for v in results] if results is not None else None,'json_bytes':len(read.stdout.encode()),'metrics':value.get('metrics') if isinstance(value,dict) else None,'diagnostic':value.get('error') if isinstance(value,dict) else None})
   assert inventory(bank)==before and (repo/'.git/index').read_bytes()==index_before and w.git('rev-parse','HEAD',cwd=repo).stdout==head_before
   assert (repo/'feature.py').read_text()=='uncommitted_feature = True\n';assert inventory(pack)==expected
   observation={'label':label,'source_commit':sha,'declared_installed_version':receipt['source']['version'],'portable_files':len(expected),'artifact_digest':receipt['source']['artifact_digest'],'fixture_sha256':fixture_sha,'distinct_lessons':count,'canonical_files':len(files),'canonical_bytes':sum(len(b.encode()) for b in files.values()),'validation_exit':r.returncode,'validation_seconds':validation_seconds,'literal_queries':queries,'canonical_code_index_head_preserved':True,'true_cold_cache_claimed':False,'token_measurement':None}
   observations.append(observation);print(dump(observation),flush=True)
   (OUT/'observations.json').write_text(json.dumps({'intent':__doc__,'platform':platform.system(),'python':platform.python_version(),'producer_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'labeled_quality_pass':False,'stress_pass':False,'observations':observations},indent=2)+'\n')
