"""Source-bound observations for semantic navigation by the invoking harness.

The harness interprets meanings. This module checks its source/version handoff;
it neither ranks by synonyms nor certifies semantic completeness.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import re
import stat
import sys
import uuid

from . import bank_contract, bank_layout, bank_observation, bank_writes as writes
from . import repository_config, scope_binding, security
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError, read

SCHEMA='agent-memory/discovery-v1'
FIELDS={'schema','task','intent','binding','nodes','sources','complete','rationale'}
BINDING={'repo_root','bank_root','project_id','scope_context','context','git_context','inventory_sha256'}
MAX_INPUT=65536
MAX_FILE=16384


def text(value,maximum):
    if not isinstance(value,str) or not value.strip() or len(value.encode())>maximum:
        raise ContractError('Discovery requires bounded actual task and semantic selection rationale')
    if security.scan(value)['reasons']:raise ContractError('Unsafe discovery text needs source clarification')
    return value


def trace_value(trace,*,completed=True):
    if (not isinstance(trace,dict) or set(trace)!=FIELDS or trace['schema']!=SCHEMA
            or trace['intent'] not in {'recall','comparison','sharing'} or type(trace['complete']) is not bool
            or not isinstance(trace['binding'],dict) or set(trace['binding'])!=BINDING
            or not uuid_valid(trace['binding']['project_id']) or len(encoded(trace))>MAX_INPUT):
        raise ContractError('Use a complete bounded same-project discovery trace')
    text(trace['task'],1024)
    if completed and not trace['complete']:
        raise ContractError('Uncertain/incomplete semantic navigation cannot certify selection or novelty')
    if trace['complete']:text(trace['rationale'],2048)
    elif trace['rationale'] is not None:text(trace['rationale'],2048)
    for name in ('nodes','sources'):
        rows=trace[name]
        if (not isinstance(rows,list) or len(rows)>64 or (name=='nodes' and not rows)
                or any(not isinstance(row,dict) or set(row)!={'path','sha256'} for row in rows)):
            raise ContractError('Retain bounded exact visited nodes and selected sources')
        paths=[]
        for row in rows:
            if row['path']!='MEMORY.md':bank_contract.path_value(row['path'])
            if not isinstance(row['sha256'],str) or not re.fullmatch('[0-9a-f]{64}',row['sha256']):
                raise ContractError('Discovery source references require actual complete-byte hashes')
            paths.append(row['path'])
        if len(paths)!=len(set(paths)):raise ContractError('Do not duplicate a discovery observation')
    if 'MEMORY.md' not in {row['path'] for row in trace['nodes']}:
        raise ContractError('Semantic navigation starts at the actual bounded repository router')
    return trace


def bounded_source(bank,path):
    target=bank/path
    if any(p.is_symlink() for p in (target,*target.parents)) or not target.resolve().is_relative_to(bank.resolve()):
        raise ContractError('Discovery reads only canonical ordinary bank paths')
    fd=os.open(target,os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0))
    with os.fdopen(fd,'rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):raise ContractError('Discovery source is not an ordinary file')
        raw=stream.read(MAX_FILE+1)
    if len(raw)>MAX_FILE:raise ContractError('A discovery node exceeds the bounded map; reorganize without truncation')
    return raw


def check_trace(repo,trace,checked,observed,scope,*,intent,selected_paths,owned_lock=None,context=None):
    from . import retrieval
    trace_value(trace)
    root,bank=writes.bank_path(repo)
    actual,git=retrieval.observed_context(root,trace['binding']['context'] if context is None else context)
    expected={'repo_root':str(root),'bank_root':repository_config.relative(root),'project_id':checked['project_id'],
        'scope_context':scope,'context':actual,'git_context':git,'inventory_sha256':digest(encoded(observed))}
    if trace['intent']!=intent or trace['binding']!=expected:
        raise ContractError('Discovery inventory, checkout, identity or context changed; navigate the current map again')
    entries={entry['path']:entry for entry in checked['entries']}
    tables={}
    for row in trace['nodes']:
        path=row['path']
        if observed.get(path)!=row['sha256'] or (path!='MEMORY.md' and entries.get(path,{}).get('format')!='topic-index'):
            raise ContractError('A visited navigation node changed or is not a canonical index')
        raw=bounded_source(bank,path)
        if digest(raw)!=row['sha256']:raise ContractError('Navigation changed during its source recheck')
        value=raw.decode('utf-8')
        security.ensure_safe({path:value})
        if path=='MEMORY.md':tables[path]=bank_contract.parse_router(value)[1]
        else:
            metadata,body=read(value,allowed=bank_layout.SPEC['properties'])
            tables[path]=bank_layout.parse_index(path,metadata,body)
    reached={'MEMORY.md'}
    while True:
        next_nodes=reached|{route['path'] for parent in reached for route in tables[parent] if route['path'] in tables}
        if next_nodes==reached:break
        reached=next_nodes
    if reached!=set(tables):raise ContractError('Keep the actual visited parent route for each discovered index')
    children={route['path'] for routes in tables.values() for route in routes}
    sources={row['path']:row['sha256'] for row in trace['sources']}
    if not set(selected_paths).issubset(sources):
        raise ContractError('Semantic selection must retain its actual discovered source versions')
    for path,sha in sources.items():
        if path not in children or observed.get(path)!=sha or entries.get(path,{}).get('format') not in {'standalone-record','context-prose'}:
            raise ContractError('A selected source changed or lacks its visited canonical parent route')
    scope_binding.require(root,checked['project_id'],expected=scope,owned_lock=owned_lock)
    return {'trace_sha256':digest(encoded(trace)),'semantic_interpreter':'invoking-harness',
        'semantic_completeness_verified':False,'visited_nodes':len(tables),'selected_sources':len(sources)}


def start(repo,task,intent,context=None):
    from . import retrieval
    text(task,1024)
    if intent not in {'recall','comparison','sharing'}:raise ContractError('Choose the actual discovery intent')
    root,bank=writes.bank_path(repo)
    scope=scope_binding.require(root)
    actual,git=retrieval.observed_context(root,context or {})
    observation=bank_observation.observe(root)
    if scope['project_id']!=observation.checked['project_id']:raise ContractError('Discovery project changed')
    raw=observation.sources['MEMORY.md']
    if len(raw)>MAX_FILE:raise ContractError('A bounded root router is required')
    body=raw.decode('utf-8');security.ensure_safe({'MEMORY.md':body})
    trace={'schema':SCHEMA,'task':task,'intent':intent,
        'binding':{'repo_root':str(root),'bank_root':repository_config.relative(root),'project_id':scope['project_id'],
            'scope_context':scope,'context':actual,'git_context':git,'inventory_sha256':digest(encoded(observation.hashes))},
        'nodes':[{'path':'MEMORY.md','sha256':observation.hashes['MEMORY.md']}],
        'sources':[],'complete':False,'rationale':None}
    trace_value(trace,completed=False)
    observation.recheck('discovery-final-source-recheck')
    scope_binding.require(root,scope['project_id'],expected=scope)
    if retrieval.observed_context(root,context or {})!=(actual,git):raise ContractError('Checkout context changed during discovery')
    audit=security.audit(root,str(uuid.uuid4()),'retrieval','checked',scope,['external'])
    return {'schema':'agent-memory/discovery-result-v1','operation':'start','discovery':trace,
        'node':{'path':'MEMORY.md','sha256':observation.hashes['MEMORY.md'],'body':body,'untrusted_data':True},
        'usable_as_current_guidance':False,'semantic_interpreter':'invoking-harness','semantic_completeness_verified':False,
        'canonical_writes_performed':False,'audit_writes_performed':True,'audit':audit,
        'metrics':{'canonical_files':len(observation.hashes),'canonical_bytes':sum(map(len,observation.sources.values())),
            'canonical_observation_payload_bytes_read':observation.payload_bytes_read,
            'canonical_observation_file_reads':observation.payload_file_reads,'node_bytes_disclosed':len(raw)}}


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    p=commands.add_parser('start');p.add_argument('--repo',required=True);p.add_argument('--task',required=True)
    p.add_argument('--intent',choices=('recall','comparison','sharing'),required=True);p.add_argument('--context')
    args=parser.parse_args(argv)
    try:
        context={}
        if args.context:
            with Path(args.context).open('rb') as stream:raw=stream.read(MAX_INPUT+1)
            if len(raw)>MAX_INPUT:raise ContractError('Discovery context exceeds its input budget')
            context=load(raw)
            from . import scopes
            scopes.conditions(context)
        print(json.dumps(start(args.repo,args.task,args.intent,context),indent=2));return 0
    except (ContractError,OSError,UnicodeError,ValueError,TypeError,KeyError,AttributeError,RecursionError):
        print(json.dumps({'schema':'agent-memory/discovery-result-v1','operation':args.command,
            'canonical_writes_performed':False,'semantic_completeness_verified':False,
            'error':'Unavailable or changed discovery; preserve memory and do not infer semantic absence'}));return 1


if __name__=='__main__':sys.exit(main())
