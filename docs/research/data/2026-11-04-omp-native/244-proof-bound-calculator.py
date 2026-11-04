"""Design-only conservative JSON serialization calculation; not product/test code."""
import json
enc = lambda value: (json.dumps(value, sort_keys=True, ensure_ascii=False, indent=2, allow_nan=False) + '\n').encode()
u = '12345678-1234-4123-8123-123456789abc'
h = 'f' * 64
path = 'topics/' + '"' * 54 + '.md'  # 64 bytes, worst escaped permitted fixture path
ran = {'source_id': u, 'start': 1048575, 'end': 1048576}
ref = {'id': u, 'version': 2147483647, 'path': path, 'resulting_sha256': h}
claim = {'unit_id': u, 'covered_source_ranges': [ran, ran], 'target_record_refs': [ref, ref], 'coverage': 'partial'}
pending = {'unit_id': u, 'classification': 'distinct-conditions', 'rationale': '"' * 2048, 'coverage_claim': claim}
decision = {'path': path, 'classification': 'scope-correction', 'rationale': '"' * 2048}
metadata = {'schema':'agent-memory/record-v1','id':u,'version':1,'project-id':u,'kind':'semantic-fact','status':'active','scope':'repository','applicability':'known','conditions':{},'source':{'actor':'fixture','kind':'test'},'evidence':['repo:file'],'origin':{'developer-id':u,'harness':'fixture','session':'s','code-state':'not-code'},'recorded-at':'2026-10-22T21:00:00Z','updated':'2026-10-22T21:00:00Z','valid-from':None,'valid-until':None,'derived-from':[],'corrects':[],'supersedes':[],'admission-reason':'reviewed','trust':'reviewed','visibility':'visible'}
target = {'path': path, 'sha256': h, 'id': u, 'version': 1, 'required_preserved_metadata': metadata, 'scope_chain': ['repository']}
obs = {'schema': 'agent-memory/comparison-observation-v2', 'project_id': u, 'request_sha256': h, 'bank_observation_sha256': h, 'router_sha256': h, 'matches': [{'path': path, 'sha256': h} for _ in range(10)], 'recovery_frontier': {'operation_ids': [u], 'ledger_heads': [h], 'frontier_digest': h, 'review_digest': h, 'reference_map_sha256': h, 'coverage_complete': True, 'unit_ids': [u] * 8}}
proof = {'schema': 'agent-memory/comparison-proof-v2', 'judgment': {'observation': obs, 'decisions': [], 'pending_decisions': [], 'rationale': '"' * 2048}, 'disposition': 'create-distinct', 'targets': []}
base = len(enc(proof))
proof['judgment']['pending_decisions'] = [pending]
dp = len(enc(proof)) - base
proof['judgment']['pending_decisions'] = []
proof['judgment']['decisions'] = [decision]
proof['targets'] = [target]
dc = len(enc(proof)) - base
print('path bytes:', len(path.encode()))
print('encoded metadata:', len(enc(target['required_preserved_metadata'])))
print('encoded scope chain:', len(enc(target['scope_chain'])))
proof['judgment']['pending_decisions'] = []
proof['judgment']['decisions'] = [decision]
proof['targets'] = [target]
full_target_proof = len(enc(proof))
target['required_preserved_metadata'] = None
nested_metadata = full_target_proof - len(enc(proof)) + len('null')
target['required_preserved_metadata'] = metadata
target['scope_chain'] = None
nested_scope = full_target_proof - len(enc(proof)) + len('null')
target['scope_chain'] = ['repository']
print('actual target proof-nested metadata contribution:', nested_metadata)
print('actual target proof-nested scope contribution:', nested_scope)
print('H serialized with 10 observations and 8 unit IDs:', base)
print('pending proof marginal:', dp)
print('canonical decision plus full target marginal:', dc)
for p, c in ((8, 2), (7, 3)):
    proof['judgment']['pending_decisions'] = [pending] * p
    proof['judgment']['decisions'] = [decision] * c
    proof['targets'] = [target] * c
    upper = 8500 + p * dp + c * dc
    print(f'P={p} C={c} serialized={len(enc(proof))} bound=8500+{p}*{dp}+{c}*{dc}={upper} slack={65536-upper}')
