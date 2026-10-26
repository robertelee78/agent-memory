from pathlib import Path
import hashlib,json,stat,sys,time
sys.dont_write_bytecode=True
root=Path('/tmp/agent-memory-v003/full-macos-gate-81fec82/source');outer=Path('/tmp/agent-memory-v003/fable-memo-boundaries-81fec82');outer.mkdir(mode=0o700,exist_ok=False)
binding=json.loads(Path('/tmp/agent-memory-v003/full-macos-gate-81fec82/source-binding.json').read_text());assert binding['commit']=='81fec82c1619d20c12473d23e3fdf6706d52b641'
def verify():
 for name,v in binding['tracked'].items():
  p=root/name;assert not p.is_symlink() and hashlib.sha256(p.read_bytes()).hexdigest()==v['sha256'] and stat.S_IMODE(p.stat().st_mode)==v['mode'],name
verify();sys.path.insert(0,str(root/'tests'));sys.path.insert(0,str(root/'skills/agent-memory/scripts'))
from world import World
from contracts.test_validation_memo import ValidationMemoContract
from agent_memory import bank_contract as bank,validation_memo as memo
from unittest.mock import patch
started=time.monotonic()
with World() as w:
 files=ValidationMemoContract().files(w);reverse=dict(reversed(list(files.items())))
 assert list(files)!=list(reverse) and files==reverse
 oversized={**files,'projectbrief.md':files['projectbrief.md']+'\n'+('Bounded fixture prose. '*900)+'\n'}
 assert len(oversized['projectbrief.md'].encode())>16384 and memo.map_input(oversized) is None
 expected=bank._validate_files(oversized)
 with memo.operation() as active:
  with patch.object(bank,'_validate_files',wraps=bank._validate_files) as original:
   first=bank.validate_files(files);assert original.call_count==1
   second=bank.validate_files(reverse);assert original.call_count==1 and first==second
   large=bank.validate_files(oversized);assert original.call_count==2 and large==expected
   assert sum(name=='bank-format' for name,key in active.values)==1
assert memo.current() is None;verify()
result={'source_commit':binding['commit'],'platform':sys.platform,'python':sys.version,'map_order_changed_but_equal':True,'same_map_cache_hit_confirmed':True,'oversized_context_bytes':len(oversized['projectbrief.md'].encode()),'oversized_map_not_cache_eligible':True,'oversized_fallback_equals_full_validation':True,'operation_state_cleared':True,'source_bytes_and_modes_preserved':True,'static_review_of_freshness_still_required':False,'whole_story_acceptance_claimed':False,'seconds':time.monotonic()-started}
(outer/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
