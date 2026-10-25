"""Small immutable evidence of known unresolved review causes, not memory truth."""
from pathlib import Path
import itertools
import re

from . import bank_writes as writes, local_registry as storage, sharing_limits as limits, trust_paths
from .developer_identity import digest, encoded
from .frontmatter import ContractError

SCHEMA='agent-memory/known-semantic-hold-v1'


def directory(root):
    _,bank=writes.bank_path(root)
    operations,_=writes.state_paths(Path(root).resolve(),bank,'00000000-0000-4000-8000-000000000000')
    return operations.parent/'semantic-holds'


def cause_id(cause):
    return digest(encoded({'sources':sorted(cause['sources']),'classification':cause['classification']}))


def load(root,project_id,*,skip_judgment=None):
    target=directory(root);trust_paths.state(target)
    if not target.exists():return []
    values=[];total=0
    paths=list(itertools.islice(target.iterdir(),4097))
    if len(paths)>4096:raise ContractError('Known review evidence requires bounded explicit recovery; preserve it')
    for path in sorted(paths):
        if not re.fullmatch('[0-9a-f]{64}\\.json',path.name):
            raise ContractError('Known review evidence requires bounded explicit recovery; preserve it')
        trust_paths.record(path);raw=limits.read_bytes(path,65536);total+=len(raw)
        if total>16*1024*1024:raise ContractError('Known hold evidence exceeds its finite inventory budget')
        from .developer_identity import load as json_load
        value=json_load(raw)
        if (not isinstance(value,dict) or set(value)!={'schema','repo_root','project_id','binding','cause','references','dependencies','judgment_sha256','proof_sha256','checksum'}
                or value['schema']!=SCHEMA or value['repo_root']!=str(Path(root).resolve()) or value['project_id']!=project_id
                or value['checksum']!=digest(encoded({k:v for k,v in value.items() if k!='checksum'}))):
            raise ContractError('Known review evidence is changed; preserve it and reconstruct the affected review')
        cause=value['cause']
        if (not isinstance(cause,dict) or set(cause)!={'sources','classification','reason','origin'}
                or cause['classification'] not in {'contradiction','uncertain','withheld','uncertain-selection'}
                or not isinstance(cause['sources'],list) or not 1<=len(cause['sources'])<=2
                or len(set(cause['sources']))!=len(cause['sources'])
                or any(not isinstance(k,str) or not re.fullmatch('[0-9a-f]{64}',k) for k in cause['sources'])
                or path.stem!=cause_id(cause) or not isinstance(value['references'],dict)
                or set(value['references'])!=set(cause['sources']) or not isinstance(value['dependencies'],dict)
                or not isinstance(cause['reason'],str) or not cause['reason'].strip() or len(cause['reason'].encode())>4096
                or any(not isinstance(value[k],str) or not re.fullmatch('[0-9a-f]{64}',value[k]) for k in ('judgment_sha256','proof_sha256'))
                or not isinstance(cause['origin'],str) or not re.fullmatch('[0-9a-f]{64}',cause['origin'])):
            raise ContractError('Known hold references are incomplete; no absence or resolution is inferred')
        if value['judgment_sha256']!=skip_judgment:values.append(value)
    return values


def record(root,project_id,proof,references):
    causes=proof.get('held_causes',[])
    if not causes:return
    target=directory(root);trust_paths.inspect(target)
    target.mkdir(parents=True,exist_ok=True,mode=0o700);trust_paths.state(target)
    binding=proof['judgment']['binding'];judgment_sha=digest(encoded(proof['judgment']))
    for cause in causes:
        path=target/(cause_id(cause)+'.json');trust_paths.record(path)
        if path.exists():
            # Existing first-detection provenance is immutable; a repeated turn
            # cannot generate another full-bank proof copy for the same cause.
            load(root,project_id)
            continue
        value={'schema':SCHEMA,'repo_root':str(Path(root).resolve()),'project_id':project_id,
            'binding':{k:binding[k] for k in ('bank_root','base','authority_head','scope_context')},
            'cause':cause,'references':{key:references[key] for key in cause['sources']},'dependencies':proof['judgment']['dependencies'],
            'judgment_sha256':judgment_sha,'proof_sha256':digest(encoded(proof))}
        value['checksum']=digest(encoded(value));raw=encoded(value)
        if len(raw)>65536:raise ContractError('One complete hold evidence record exceeds 64KiB; preserve the original review')
        storage.replace(path,raw,None)
