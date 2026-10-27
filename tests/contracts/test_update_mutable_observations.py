import pathlib,sys,unittest,threading
from unittest.mock import patch
ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tests'))
sys.path.insert(0, str(ROOT / 'skills/agent-memory/scripts'))
from world import World
from runtime_update_fixtures import bank
from agent_memory import updating,migration
from agent_memory.frontmatter import ContractError

class FreshObservationControls(unittest.TestCase):
 def test_mutations_between_router_observations_refuse(self):
  compatibility={'readable_bank_schemas':['agent-memory/v1'],'migrator_source_schemas':[]}
  for mode in ('head','index','config','bank','lock'):
   with self.subTest(mode=mode),World() as w:
    repo=w.repository('actual fixture'); b=bank(w,repo); w.git('add','.',cwd=repo);w.git('commit','-m','base',cwd=repo)
    roots=[repo]
    for n in range(3):
     sibling=w.inside('sibling-'+str(n));w.git('worktree','add','--detach',sibling,cwd=repo);roots.append(sibling)
    original=migration.read_regular; state={'count':0}; mutex=threading.Lock()
    def intercept(target,path,limit,*args,**kwargs):
     value=original(target,path,limit,*args,**kwargs)
     if pathlib.Path(target)==b and path=='MEMORY.md':
      with mutex:
       state['count']+=1
       if state['count']==1:
        if mode in ('head','index'):
         other=repo/'other';other.mkdir();(other/'MEMORY.md').write_bytes((b/'MEMORY.md').read_bytes());w.git('add','other/MEMORY.md',cwd=repo)
         if mode=='head':w.git('commit','-m','changed head',cwd=repo)
        elif mode=='config':w.git('config','--local','agent-memory.bankroot','other',cwd=repo)
        elif mode=='bank':(b/'MEMORY.md').write_bytes((b/'MEMORY.md').read_bytes()+b'\n')
        elif mode=='lock':(b/'.agent-memory.lock').write_text('active operation')
     return value
    with patch.object(pathlib.Path,'cwd',return_value=repo),patch.object(updating.project_identity,'observe',return_value=({'checkouts':{}},None)),patch.object(migration,'read_regular',side_effect=intercept):
     with self.assertRaises(ContractError):updating.banks(roots,compatibility)
    print(mode,'refused',flush=True)
if __name__=='__main__':unittest.main(verbosity=2)
