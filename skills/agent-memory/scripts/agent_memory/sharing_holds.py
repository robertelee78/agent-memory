"""Repository/worktree-bound unresolved review evidence with atomic recovery.

This private registry can constrain a review; it never admits a lesson or
certifies semantic truth. Canonical learning remains Markdown in Git.
"""
from contextlib import contextmanager
import copy
import os
from pathlib import Path
import re
import uuid

from . import bank_contract, bank_writes as writes, local_registry as storage
from .bank_transactions import flush_directory
from . import sharing_limits as limits, trust_paths
from .developer_identity import digest, encoded, load as json_load, uuid_valid
from .frontmatter import ContractError

SCHEMA='agent-memory/known-semantic-holds-v2'
MAX_RECORDS=4096
MAX_STORE=16*1024*1024
MAX_JOURNAL=2*MAX_STORE+65536


class HoldFailure(ContractError):
    def __init__(self,code,reason,repair):
        super().__init__(reason);self.code=code;self.repair=repair


def recovery_needed(code,reason):
    return HoldFailure(code,reason,
        'Preserve the bank, registry, recovery copy and journal. Run am semantic recover-holds with the same --repo and verified --project-id, then repeat inspect-v2 and the source review. Recovery verifies ownership and will not overwrite unknown later evidence.')


def cause_id(cause):
    return digest(encoded({'sources':sorted(cause['sources']),'classification':cause['classification']}))


def locations(root,project_id):
    if not uuid_valid(project_id):raise ContractError('Known holds require the actual canonical project UUID')
    root,bank=writes.bank_path(root)
    operations,_=writes.state_paths(root,bank,str(uuid.uuid4()))
    base=operations.parent/'semantic-holds';directory=base/project_id
    pack=Path(__file__).resolve().parents[2];source=pack.parent.parent if pack.parent.name=='skills' else pack
    if directory.resolve().is_relative_to(source):raise ContractError('Known hold state must stay outside the executing runtime/source')
    return root,bank,directory,base/(project_id+'.present.json')


def seal(value):
    return {**value,'checksum':digest(encoded(value))}


def registry_value(value,root,project_id):
    if (not isinstance(value,dict) or set(value)!={'schema','repo_root','project_id','records','checksum'}
            or value['schema']!=SCHEMA or value['repo_root']!=str(root) or value['project_id']!=project_id
            or value['checksum']!=digest(encoded({k:v for k,v in value.items() if k!='checksum'}))
            or not isinstance(value['records'],dict) or len(value['records'])>MAX_RECORDS
            or len(encoded(value))>MAX_STORE):
        raise ContractError('Known review inventory is changed/oversized; preserve it for recovery')
    for key,record in value['records'].items():
        if (not isinstance(record,dict) or set(record)!={'binding','cause','references','dependencies','judgment_sha256','proof_sha256','resolution'}):
            raise ContractError('Known review evidence has an unknown format')
        cause=record['cause']
        if (not isinstance(cause,dict) or set(cause)!={'sources','classification','reason','origin'}
                or cause['classification'] not in {'contradiction','uncertain','withheld','uncertain-selection'}
                or not isinstance(cause['sources'],list) or not 1<=len(cause['sources'])<=2
                or len(set(cause['sources']))!=len(cause['sources'])
                or any(not isinstance(k,str) or not re.fullmatch('[0-9a-f]{64}',k) for k in cause['sources'])
                or key!=cause_id(cause) or not isinstance(record['references'],dict)
                or set(record['references'])!=set(cause['sources']) or not isinstance(record['dependencies'],dict)
                or not isinstance(cause['reason'],str) or not cause['reason'].strip() or len(cause['reason'].encode())>4096
                or any(not isinstance(record[k],str) or not re.fullmatch('[0-9a-f]{64}',record[k]) for k in ('judgment_sha256','proof_sha256'))
                or not isinstance(cause['origin'],str) or not re.fullmatch('[0-9a-f]{64}',cause['origin'])):
            raise ContractError('Known hold references are incomplete; no resolution is inferred')
        for reference in record['references'].values():
            if (not isinstance(reference,dict) or not {'path','component','target_path','source_sha256_by_side'}.issubset(reference)
                    or set(reference)-{'path','component','target_path','source_sha256_by_side','record'}):
                raise ContractError('Known logical/source reference is incomplete')
            if reference['path']!='MEMORY.md':bank_contract.path_value(reference['path'])
            if reference.get('record') is not None:
                from .records import reference as record_reference
                record_reference(reference['record'])
    return value


def trusted_read(path,maximum):
    trust_paths.record(path)
    return limits.read_bytes(path,maximum) if path.exists() else None


def anchor_value(raw,root,project_id):
    value=json_load(raw)
    if (not isinstance(value,dict) or set(value)!={'schema','repo_root','project_id','checksum'}
            or value['schema']!='agent-memory/known-holds-anchor-v1' or value['repo_root']!=str(root)
            or value['project_id']!=project_id or value['checksum']!=digest(encoded({k:v for k,v in value.items() if k!='checksum'}))):
        raise ContractError('Known hold history anchor changed; preserve it for explicit recovery')
    return value


def legacy_paths(directory):
    base=directory.parent;trust_paths.state(base)
    if not base.exists():return {}
    paths={};count=0
    with os.scandir(base) as entries:
        for entry in entries:
            count+=1
            if count>MAX_RECORDS+64:raise recovery_needed('known-holds-legacy-capacity',
                'The retained dispute-state namespace exceeds its bounded compatibility inventory.')
            if re.fullmatch('[0-9a-f]{64}\\.json',entry.name):paths[entry.name[:-5]]=Path(entry.path)
    return paths


def import_legacy(root,project_id,directory,existing):
    added={};total=0
    for key,path in sorted(legacy_paths(directory).items()):
        if key in existing:continue
        raw=trusted_read(path,65536);total+=len(raw)
        if total>MAX_STORE:raise ContractError('Legacy dispute evidence exceeds its bounded migration budget; preserve it')
        value=json_load(raw)
        fields={'schema','repo_root','project_id','binding','cause','references','dependencies','judgment_sha256','proof_sha256','checksum'}
        if (not isinstance(value,dict) or set(value)!=fields or value['schema']!='agent-memory/known-semantic-hold-v1'
                or value['repo_root']!=str(root) or value['project_id']!=project_id
                or value['checksum']!=digest(encoded({k:v for k,v in value.items() if k!='checksum'}))):
            raise ContractError('Legacy dispute evidence is changed or belongs to another project; preserve it for inspection')
        added[key]={k:v for k,v in value.items() if k not in {'schema','repo_root','project_id','checksum'}}
        added[key]['resolution']=None
    desired=seal({'schema':SCHEMA,'repo_root':str(root),'project_id':project_id,'records':{**existing,**added}})
    registry_value(desired,root,project_id)
    return desired


def read_store(root,project_id,*,owner=None,allow_journal=False,allow_legacy=False):
    root,bank,directory,anchor=locations(root,project_id);trust_paths.state(directory)
    lock=directory/'lock';journal=directory/'journal.json'
    lock_raw=trusted_read(lock,4096)
    if lock_raw is not None and lock_raw!=owner:raise HoldFailure('known-holds-busy',
        'A known-dispute writer is active or its ownership has not been cleared after interruption.',
        'Allow an active review to finish. If interrupted, use am semantic recover-holds with the same --repo and verified --project-id; preserve unknown lock evidence and do not remove it manually.')
    if journal.exists() and not allow_journal:raise recovery_needed('known-holds-interrupted',
        'A known-dispute transaction has an unfinished recovery journal; its claims cannot be treated as resolved.')
    raw=trusted_read(directory/'registry.json',MAX_STORE)
    backup=trusted_read(directory/'recovery.json',MAX_STORE)
    anchored=trusted_read(anchor,4096)
    if anchored is not None:anchor_value(anchored,root,project_id)
    if raw is None:
        if anchored is not None or backup is not None or journal.exists():
            raise recovery_needed('known-holds-registry-missing',
                'The known-dispute registry is missing, but its anchor or retained recovery evidence proves prior review state existed.')
        value=seal({'schema':SCHEMA,'repo_root':str(root),'project_id':project_id,'records':{}})
        if not allow_legacy and legacy_paths(directory):raise recovery_needed('known-holds-legacy-state',
            'Earlier per-cause dispute evidence exists; absence of the new registry does not establish an empty history.')
        return value,None
    value=registry_value(json_load(raw),root,project_id)
    if anchored is None or backup!=raw:
        raise recovery_needed('known-holds-recovery-incomplete',
            'The known-dispute registry and its recovery anchor/copy do not form one complete verified state.')
    if not allow_legacy and set(legacy_paths(directory))-set(value['records']):raise recovery_needed('known-holds-legacy-state',
        'Additional earlier per-cause dispute evidence has not yet been imported into the anchored registry.')
    return value,digest(raw)


def filtered_records(value,skip_judgment=None):
    records=[]
    for key in sorted(value['records']):
        record=value['records'][key]
        if record['judgment_sha256']==skip_judgment:continue
        current=copy.deepcopy(record)
        if current['resolution'] and current['resolution'].get('judgment_sha256')==skip_judgment:
            current['resolution']=None
        records.append(current)
    return records


def load(root,project_id,*,skip_judgment=None):
    value,_=read_store(root,project_id)
    return filtered_records(value,skip_judgment)


def prepare_directory(directory):
    for path in (directory.parent,directory):
        trust_paths.inspect(path);path.mkdir(parents=True,exist_ok=True,mode=0o700);trust_paths.state(path)


def commit_store(root,project_id,desired,observed_sha,directory,anchor):
    # All count/encoded-size validation precedes any durable registry mutation.
    registry_value(desired,root,project_id);raw=encoded(desired)
    anchored=seal({'schema':'agent-memory/known-holds-anchor-v1','repo_root':str(root),'project_id':project_id})
    journal=seal({'schema':'agent-memory/known-holds-journal-v1','repo_root':str(root),'project_id':project_id,
                  'observed_sha256':observed_sha,'desired':desired,'desired_sha256':digest(raw)})
    jraw=encoded(journal)
    if len(jraw)>MAX_JOURNAL:raise ContractError('Complete known-hold recovery journal exceeds its finite budget')
    target=directory/'journal.json'
    if target.exists():raise ContractError('Known-hold recovery is pending; preserve the original journal')
    storage.replace(target,jraw,None);writes.fault('holds-after-journal')
    storage.replace(directory/'registry.json',raw,observed_sha);writes.fault('holds-after-registry')
    backup=trusted_read(directory/'recovery.json',MAX_STORE)
    storage.replace(directory/'recovery.json',raw,digest(backup))
    previous=trusted_read(anchor,4096)
    if previous is not None:anchor_value(previous,root,project_id)
    else:storage.replace(anchor,encoded(anchored),None)
    writes.fault('holds-before-journal-cleanup')
    if trusted_read(target,MAX_JOURNAL)!=jraw or trusted_read(directory/'registry.json',MAX_STORE)!=raw or trusted_read(directory/'recovery.json',MAX_STORE)!=raw:
        raise ContractError('Known-hold transaction changed; preserve its recovery journal')
    target.unlink();flush_directory(directory)


@contextmanager
def bank_owner(root,operation_id,owned_lock):
    _,bank=writes.bank_path(root)
    if owned_lock is not None:
        path=bank/'.agent-memory.lock'
        if path.is_symlink() or limits.read_bytes(path,4096)!=owned_lock:
            raise ContractError('Actual bank ownership is required for known-hold state changes')
        yield owned_lock
    else:
        with storage.exclusive(bank/'.agent-memory.lock',operation_id,'semantic-review-holds') as owner:yield owner


def record(root,project_id,proof,references,*,resolutions=None,owned_lock=None,verify=None,expected_inventory=None):
    proof_raw=limits.private_bytes(proof)
    causes=proof.get('held_causes',[]);resolutions=resolutions or {}
    root,bank,directory,anchor=locations(root,project_id);prepare_directory(directory)
    judgment_sha=digest(encoded(proof['judgment']));operation_id=str(uuid.UUID(hex=judgment_sha[:32],version=4))
    with bank_owner(root,operation_id,owned_lock) as bank_token:
        if verify is not None:verify(bank_token)
        with storage.exclusive(directory/'lock',operation_id,'semantic-holds') as owner:
            current,observed_sha=read_store(root,project_id,owner=owner)
            if expected_inventory is not None and digest(encoded(filtered_records(current,judgment_sha)))!=expected_inventory:
                raise ContractError('Known dispute inventory changed; repeat the actual current review')
            records=copy.deepcopy(current['records'])
            binding=proof['judgment']['binding']
            for cause in causes:
                key=cause_id(cause)
                if key in records:continue
                records[key]={'binding':{k:binding[k] for k in ('bank_root','base','authority_head','scope_context')},
                    'cause':cause,'references':{key:references[key] for key in cause['sources']},
                    'dependencies':proof['judgment']['dependencies'],'judgment_sha256':judgment_sha,
                    'proof_sha256':digest(proof_raw),'resolution':None}
            for key,value in resolutions.items():
                if key not in records:raise ContractError('Resolution must retain its actual first-detection evidence')
                records[key]['resolution']={**value,'judgment_sha256':judgment_sha,'proof_sha256':digest(proof_raw)}
            desired=seal({'schema':SCHEMA,'repo_root':str(root),'project_id':project_id,'records':records})
            registry_value(desired,root,project_id)
            if desired!=current:commit_store(root,project_id,desired,observed_sha,directory,anchor)
        if verify is not None:verify(bank_token)


def recover(root,project_id):
    root,bank,directory,anchor=locations(root,project_id)
    from . import scope_binding
    # Establish canonical project/scope before mkdir or clearing any marker.
    # A stale bank marker is read only after the common recovery preview has
    # established its supported ownership; its bytes permit read-only identity
    # inspection, never mutation. Clearing still uses the common checked apply.
    bank_lock=bank/'.agent-memory.lock';stale=None
    if bank_lock.exists() or bank_lock.is_symlink():
        storage.recover(bank/'MEMORY.md',bank_lock,{'semantic-review-holds'})
        stale=trusted_read(bank_lock,4096)
    expected_scope=scope_binding.require(root,project_id,owned_lock=stale)
    prepare_directory(directory)
    for registry,lock,operations in ((bank/'MEMORY.md',bank/'.agent-memory.lock',{'semantic-review-holds'}),
                                     (directory/'registry.json',directory/'lock',{'semantic-holds'})):
        if lock.exists() or lock.is_symlink():
            trusted_read(lock,4096)
            preview=storage.recover(registry,lock,operations)
            storage.recover(registry,lock,operations,preview)
    operation_id=str(uuid.uuid4())
    with bank_owner(root,operation_id,None) as bank_token:
        scope_binding.require(root,project_id,expected=expected_scope,owned_lock=bank_token)
        with storage.exclusive(directory/'lock',operation_id,'semantic-holds') as owner:
            target=directory/'journal.json';jraw=trusted_read(target,MAX_JOURNAL)
            if jraw is not None:
                j=json_load(jraw)
                if (not isinstance(j,dict) or set(j)!={'schema','repo_root','project_id','observed_sha256','desired','desired_sha256','checksum'}
                        or j['schema']!='agent-memory/known-holds-journal-v1' or j['repo_root']!=str(root) or j['project_id']!=project_id
                        or j['checksum']!=digest(encoded({k:v for k,v in j.items() if k!='checksum'}))):
                    raise ContractError('Unknown hold journal; preserve it for explicit inspection')
                desired=registry_value(j['desired'],root,project_id);raw=encoded(desired)
                if digest(raw)!=j['desired_sha256']:raise ContractError('Changed hold recovery content')
                current=trusted_read(directory/'registry.json',MAX_STORE)
                if digest(current) not in {j['observed_sha256'],j['desired_sha256']}:raise ContractError('Later hold evidence is preserved; do not replay over it')
                if current!=raw:storage.replace(directory/'registry.json',raw,digest(current))
                backup=trusted_read(directory/'recovery.json',MAX_STORE);storage.replace(directory/'recovery.json',raw,digest(backup))
                previous=trusted_read(anchor,4096)
                if previous is not None:anchor_value(previous,root,project_id)
                else:storage.replace(anchor,encoded(seal({'schema':'agent-memory/known-holds-anchor-v1','repo_root':str(root),'project_id':project_id})),None)
                if trusted_read(target,MAX_JOURNAL)!=jraw:raise ContractError('Hold journal changed during recovery')
                target.unlink();flush_directory(directory)
            elif not (directory/'registry.json').exists():
                backup=trusted_read(directory/'recovery.json',MAX_STORE);anchored=trusted_read(anchor,4096)
                if backup is not None and anchored is not None:
                    anchor_value(anchored,root,project_id);registry_value(json_load(backup),root,project_id)
                    storage.replace(directory/'registry.json',backup,None)
                elif backup is not None or anchored is not None:
                    raise ContractError('Missing hold evidence cannot be treated as resolution; inspect canonical/source evidence')
            current,observed=read_store(root,project_id,owner=owner,allow_legacy=True)
            desired=import_legacy(root,project_id,directory,current['records'])
            if desired!=current:commit_store(root,project_id,desired,observed,directory,anchor)
            read_store(root,project_id,owner=owner)
    return {'schema':'agent-memory/known-holds-recovery-v1','verified':True,'canonical_writes_performed':False,
            'private_state_writes_performed':True,'source_observation_current':False,'requires_fresh_review':True}
