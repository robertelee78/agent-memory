"""Separate authored meaning-bearing map; not a rewrite of the frozen literal corpus.

Group descriptions come from declared source subjects, not held-out questions.
Large distractor families have actual component identities, not opaque storage
bucket captions. Fixture assertions are synthetic, not admitted project learning.
"""
from collections import defaultdict
import copy
import posixpath
from pathlib import PurePosixPath

from labeled_recall_fixture import corpus as original_corpus, record, header, PID, STAMP, CONTEXTS

DOMAINS=[
 ('journaling','Journal sealing and footer recovery','journal partition','A partial footer requires recovery from the preceding complete segment; seal partition {n} after {v} entries.'),
 ('workers','Worker admission and drain behavior','worker shard','Worker shard {n} accepts {v} outstanding jobs; draining preserves already completed outcome identifiers.'),
 ('artifacts','Artifact manifests and generations','artifact family','Artifact family {n} uses generation {v}; validate the manifest digest before unpacking.'),
 ('parsing','Parser bounds and diagnostics','parser dialect','Parser dialect {n} permits {v} nesting levels; malformed delimiters retain their input and location diagnostic.'),
 ('sockets','Connection pools and peer closure','socket pool','Socket pool {n} keeps {v} idle connections per origin; remove a peer-closed connection before reassignment.'),
 ('scheduling','Queue weights and accepted ordering','scheduler lane','Scheduler lane {n} has weight {v}; new admissions use changed weights while accepted jobs preserve ordering.'),
 ('snapshots','Snapshot retention and dependent readers','snapshot family','Snapshot family {n} stays for {v} complete cycles; removal checks dependent readers and records the last referenced generation.'),
 ('encoding','Encoding blocks and final checksum','encoder profile','Encoder profile {n} emits blocks of {v} units; failed final checksums preserve the preceding artifact.'),
 ('authorization','Access grants and resource scope','authorization resource','Resource {n} limits grants to {v} members; inherited access never widens the resource scope.'),
 ('certificates','Certificate rotation and trust anchors','certificate family','Certificate family {n} retains {v} prior anchors during rotation; an unknown issuer is rejected.'),
 ('telemetry','Telemetry buffering and dropped samples','telemetry stream','Telemetry stream {n} buffers {v} complete samples; overflow records dropped counts without blocking application work.'),
 ('storage','Storage allocation and committed blocks','storage volume','Storage volume {n} allocates {v} blocks per extent; allocation failure preserves already committed blocks.'),
 ('leases','Lease renewal and ownership expiry','lease resource','Lease resource {n} permits {v} renewal intervals; expired ownership cannot authorize later work.'),
 ('caching','Cache generations and invalidation','cache partition','Cache partition {n} retains {v} generations; an invalidation clears dependent keys before new reads.'),
 ('deployment','Deployment cohorts and rollback evidence','deployment cohort','Deployment cohort {n} advances after {v} health observations; rollback retains the inspected source revision.'),
 ('timekeeping','Clock skew and observation windows','clock domain','Clock domain {n} permits {v} units of skew; observations beyond the window require renewed comparison.'),
 ('transactions','Transaction journals and replay identity','transaction stream','Transaction stream {n} replays {v} pending journal entries; completed operations retain their original identity.'),
 ('compression','Compression framing and block recovery','compression profile','Compression profile {n} frames {v} units per block; a damaged final block preserves preceding complete blocks.'),
 ('dns','Name resolution and authoritative responses','DNS zone','DNS zone {n} keeps {v} authoritative answers; a failed lookup does not create a new record.'),
 ('filesystem','Filesystem links and ordinary file identity','filesystem tree','Filesystem tree {n} checks {v} ordinary entries per batch; a changed link cannot stand in for the inspected regular file.'),
]

SPECIAL={
 'retry-limit':'request-reliability','conditional-resume':'session-recovery','head-dispatch':'work-admission',
 'cursor-empty':'pagination','cursor-repeated':'pagination',
 'ledger-only':'retention','retention-dev':'retention','retention-prod':'retention',
 'timeout-v1':'request-reliability','timeout-v2':'request-reliability',
 'cache-v1':'cache-policy','cache-replacement':'cache-policy','quota-v1':'cache-policy','quota-replacement':'cache-policy',
 'expired-endpoint':'request-reliability','revoked-fallback':'request-reliability','superseded-protocol':'request-reliability',
 'receipt-a':'request-identity','receipt-b':'financial-reconciliation',
 'wall-blue':'workspace-configuration','wall-yellow':'workspace-configuration',
 'deployment-queue':'work-admission','unsafe-secret':'source-review','session-bound':'session-recovery',
}
SUBJECTS={
 'request-reliability':'Request retries, failure handling and confirmed deadlines',
 'work-admission':'Deployment queues and worker dispatch limits',
 'pagination':'Pagination continuation and repeated cursor protection',
 'retention':'Ledger and environment-specific retention requirements',
 'cache-policy':'Cache and quota policy corrections with retained prior versions',
 'request-identity':'Repeated request identities and preservation of first outcomes',
 'financial-reconciliation':'Idempotency receipts and repeated financial requests',
 'workspace-configuration':'Workspace appearance decisions and unresolved room claims',
 'source-review':'Sources awaiting safety or authority review',
 'session-recovery':'Operation resumption, recovery prerequisites and session-specific context',
}


def route_table(children,source):
 text='| Path | Purpose | Scope | Lifecycle | Read when |\n| --- | --- | --- | --- | --- |\n'
 for path,purpose,scope,state,when in children:
  target=posixpath.relpath(path,str(PurePosixPath(source).parent))
  text+='| ['+path+'](<'+target+'>) | '+purpose+' | '+scope+' | '+state+' | '+when+' |\n'
 return text


def index(path,children):
 return header({'schema':'agent-memory/topic-index-v1','project-id':PID,'updated':STAMP})+'# Topic index\n\n## Routes\n\n'+route_table(children,path)


def common_scope(values):
 # Derive a redundant parent constraint from every actual child scope. This
 # advertises existing applicability before a harness expands irrelevant
 # component headings; it does not add a condition or inspect query labels.
 scopes=[row['metadata']['scope'] for row in values]
 if not scopes or any(not scope.startswith('component:') for scope in scopes):return 'repository'
 parts=[scope.removeprefix('component:').split('/') for scope in scopes]
 prefix=[]
 for components in zip(*parts):
  if len(set(components))!=1:break
  prefix.append(components[0])
 return 'component:'+'/'.join(prefix) if prefix else 'repository'


def corpus(template,count):
 _,base,_=original_corpus(template,64)
 rows=[copy.deepcopy(row) for row in base if not row['label'].startswith('distractor-')]
 assert len({row['metadata']['id'] for row in rows})<count
 groups=defaultdict(list)
 for row in rows:
  assert row['label'] in SPECIAL,row['label']
  groups[SPECIAL[row['label']]].append(row)
 have=len({row['metadata']['id'] for row in rows})
 for n in range(count-have):
  domain,subject,title,pattern=DOMAINS[n%len(DOMAINS)];component=n//len(DOMAINS)
  body='# '+title.title()+' '+str(component)+'\n\n'+pattern.format(n=component,v=2+n%127)+'\n'
  row=record(template,'semantic-'+domain+'-'+str(component),body,scope='component:src/'+domain+'/'+str(component))
  row['metadata']['conditions']={'component':'src/'+domain+'/'+str(component)}
  rows.append(row);groups[domain].append(row)
 subjects={**SUBJECTS,**{domain:subject for domain,subject,_,_ in DOMAINS}}
 files={'projectbrief.md':'# Synthetic project\n\nThis bank contains separately scoped operational observations. Interpret topic subjects, then inspect actual evidence and conditions.\n'}
 root=[]
 for group,values in sorted(groups.items()):
  for row in values:
   row['path']='topics/r/'+row['metadata']['id']+'-v'+str(row['metadata']['version'])+'.md'
   files[row['path']]=header(row['metadata'])+row['body']
  leaves=[];pending=[];start=0
  def rows_for(values):
   return [(r['path'],r['body'].splitlines()[0].lstrip('# ').strip(),r['metadata']['scope'],r['metadata']['status'],'When this subject applies') for r in values]
  def finish(values,start):
   path='topics/n/'+group+'-'+str(start)+'-'+str(start+len(values)-1)+'.md'
   body=index(path,rows_for(values));assert len(body.encode())<=16384
   files[path]=body
   purpose=subjects[group] if group in SUBJECTS else group.title()+' components '+str(start)+' to '+str(start+len(values)-1)
   return (path,purpose,common_scope(values),'active','When investigating '+group.replace('-',' '))
  for row in values:
   probe='topics/n/'+group+'-'+str(start)+'-'+str(start+len(pending))+'.md'
   if pending and (len(pending)>=64 or len(index(probe,rows_for([*pending,row])).encode())>16384):
    leaves.append(finish(pending,start));start+=len(pending);pending=[]
   pending.append(row)
  if pending:leaves.append(finish(pending,start))
  assert len(leaves)<=64,(group,len(leaves))
  path='topics/g/'+group+'.md';files[path]=index(path,leaves);assert len(files[path].encode())<=16384
  root.append((path,subjects[group],common_scope(values),'active','When investigating '+group.replace('-',' ')))
 root.insert(0,('projectbrief.md','Project purpose and source boundaries','repository','active','At session start'))
 files['MEMORY.md']=header({'schema':'agent-memory/v1','project-id':PID,'router-updated':STAMP})+'# Project memory\n\n## Routes\n\n'+route_table(root,'MEMORY.md')+'\n## Omitted contexts\n\n'+''.join('- `'+path+'`: No separate evidence in this synthetic fixture\n' for path in CONTEXTS[1:])
 assert len(files['MEMORY.md'].encode())<=16384 and len(root)<=64
 assert len({row['metadata']['id'] for row in rows})==count
 return files,rows,{'fixture':'semantic-navigation-v3-parent-scopes','distinct_lessons':count,'canonical_files':len(files),
  'canonical_bytes':sum(len(value.encode()) for value in files.values()),'root_routes':len(root),
  'layout_basis':'authored source subjects and actual component identities, independent of held-out questions',
  'frozen_literal_baseline_modified':False}
