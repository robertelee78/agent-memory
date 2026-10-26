"""Ready-bank public operation cost, with fixture-known paths, not semantic quality."""
from pathlib import Path
import argparse,hashlib,json,shutil,sys,time
sys.dont_write_bytecode=True

parser=argparse.ArgumentParser();parser.add_argument('--count',type=int,choices=[1000,10000,50000],required=True);parser.add_argument('--out',required=True)
args=parser.parse_args()
root=Path('/opt/agent-memory-worktrees/v003-release-integration')
sys.path[:0]=[str(root/'tests'),str(root/'scripts/evaluations')]
from world import World
from identity_fixtures import bind
from labeled_recall_probe import source,inventory
from semantic_navigation_fixture import corpus

out=Path(args.out);out.mkdir(mode=0o700,exist_ok=False)
before=source(root);(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n')
commands=[];question='How often may a temporary network error be attempted again?'
with World() as w:
    pack=w.inside('portable semantic pack');shutil.copytree(root/'skills/agent-memory',pack,ignore=shutil.ignore_patterns('__pycache__'))
    repo=w.repository();(repo/'feature.py').write_text('VALUE = 1\n');w.git('add','feature.py',cwd=repo);w.git('commit','-m','Synthetic source evidence',cwd=repo)
    (repo/'feature.py').write_text('VALUE = 2  # staged feature work\n');w.git('add','feature.py',cwd=repo)
    template=json.loads((pack/'templates/record-proposal.example.json').read_text())['metadata']
    setup=time.perf_counter();files,rows,stats=corpus(template,args.count);bank=repo/'memory-bank';bank.mkdir()
    for name,body in files.items():
        p=bank/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(body)
    bind(w,repo);setup_seconds=time.perf_counter()-setup
    initial=inventory(bank);index=(repo/'.git/index').read_bytes();head=w.git('rev-parse','HEAD',cwd=repo).stdout
    selected=next(row['path'] for row in rows if row['label']=='retry-limit')
    parents=[name for name,text in files.items() if name.startswith('topics/n/') and '| ['+selected+'](<' in text]
    assert len(parents)==1
    leaf=parents[0]
    parents=[name for name,text in files.items() if name.startswith('topics/g/') and '| ['+leaf+'](<' in text]
    assert len(parents)==1
    group=parents[0]
    def invoke(helper,*args):
        n=len(commands)+1;argv=[str(a) for a in [sys.executable,'-I',pack/'scripts'/helper,*args]]
        started=time.perf_counter();r=w.run(argv,check=False,timeout=300,group=True);elapsed=time.perf_counter()-started
        (out/(str(n)+'.stdout')).write_text(r.stdout);(out/(str(n)+'.stderr')).write_text(r.stderr)
        item={'argv':argv,'exit':r.returncode,'seconds':elapsed};commands.append(item)
        (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        assert r.returncode==0,(helper,r.stdout,r.stderr)
        return json.loads(r.stdout)
    def save(value,name):
        path=w.inside(name);path.write_text(json.dumps(value));return path
    observations=[]
    for attempt in range(3):
        start=time.perf_counter();first=len(commands)
        state=invoke('discover_memory.py','start','--repo',repo,'--intent','recall','--task',question)
        for path in (group,leaf):
            state=invoke('discover_memory.py','node','--repo',repo,'--input',save(state['discovery'],'navigation.json'),'--path',path)
        request={'project_id':state['discovery']['binding']['project_id'],'query':question,'ids':[],'paths':[selected],
            'context':{},'kinds':['semantic-fact'],'minimum_trust':'reviewed','limit':8,'include_context':False}
        value=invoke('discover_memory.py','read','--repo',repo,'--input',save(state['discovery'],'navigation.json'),
            '--request',save(request,'request.json'),'--rationale','Fixture-known source selection for operation-cost measurement; this is not a blind semantic quality evaluation.')
        elapsed=time.perf_counter()-start;qualified=value['qualification']
        assert [row['path'] for row in qualified['results']]==[selected]
        observations.append({'attempt':attempt,'kind':'cold-rebuild' if attempt==0 else 'repeated-warm',
            'whole_workflow_seconds':elapsed,'public_operation_seconds':[row['seconds'] for row in commands[first:]],
            'qualifier_metrics':qualified['metrics'],'optional_indexes_used':qualified['optional_indexes_used'],
            'qualified_context_bytes':sum(len(json.dumps(row).encode()) for row in qualified['results'])})
        (out/'observations.json').write_text(json.dumps(observations,indent=2)+'\n')
    assert initial==inventory(bank) and index==(repo/'.git/index').read_bytes() and head==w.git('rev-parse','HEAD',cwd=repo).stdout
    result={'commit':before['commit'],'fixture':stats,'setup_seconds':setup_seconds,'observations':observations,
        'bank_feature_index_head_preserved':True,'semantic_quality_claimed':False,'performance_acceptance_claimed':False,
        'cold_filesystem_cache_claimed':False,'model_reasoning_included':False}
    (out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
after=source(root);assert before==after;(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n')
