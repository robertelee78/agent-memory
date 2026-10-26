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
import copy

from . import bank_contract, bank_layout, bank_observation, bank_writes as writes
from . import repository_config, scope_binding, security
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError, read

SCHEMA='agent-memory/discovery-v1'
FIELDS={'schema','task','intent','binding','nodes','sources','complete','rationale'}
BINDING={'repo_root','bank_root','project_id','scope_context','context','git_context','inventory_sha256'}
MAX_INPUT=65536
MAX_FILE=16384


class DiscoveryFailure(ContractError):
    def __init__(self,code,reason,repair):
        super().__init__(reason);self.code=code;self.repair=repair


def changed():
    return DiscoveryFailure('source-context-changed',
        'The memory map, selected sources, repository identity or checkout context changed during navigation.',
        'Run am discover start again with the current task/context, follow the current map, then retry the qualified read.')


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
        raise DiscoveryFailure('selection-incomplete','Semantic navigation has not selected and qualified its plausible sources.',
            'Follow the map and use am discover read with the selected paths and your selection rationale before answering or admitting learning.')
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
    if len(raw)>MAX_FILE:raise DiscoveryFailure('source-over-bound','A navigation node or selected source exceeds the 16KiB reader bound.',
        'Inspect the source through the governed maintenance workflow and split or compact it; retain its conditions/provenance and retry without truncation.')
    return raw


def check_trace(repo,trace,checked,observed,scope,*,intent,selected_paths,owned_lock=None,context=None):
    from . import retrieval
    trace_value(trace)
    root,bank=writes.bank_path(repo)
    actual,git=retrieval.observed_context(root,trace['binding']['context'] if context is None else context)
    expected={'repo_root':str(root),'bank_root':repository_config.relative(root),'project_id':checked['project_id'],
        'scope_context':scope,'context':actual,'git_context':git,'inventory_sha256':digest(encoded(observed))}
    if trace['intent']!=intent or trace['binding']!=expected:
        raise changed()
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


def navigation_observation(root,bank):
    bank_observation.markers(bank,None)
    metrics={};physical,hashes,_=bank_observation.scan(bank,read_sources=False,metrics=metrics)
    bank_observation.physical_contract(physical)
    raw=bounded_source(bank,'MEMORY.md')
    if digest(raw)!=hashes['MEMORY.md']:raise changed()
    metadata,routes,omitted=bank_contract.parse_router(raw.decode('utf-8'))
    for path in omitted:
        if (bank/path).exists() or (bank/path).is_symlink():raise DiscoveryFailure('omitted-context-occupied',
            'A context declared absent is physically present in the bank.',
            'Run am doctor, inspect that context and reconcile its route/omission before retrying; preserve the existing file.')
    checked={'project_id':metadata['project-id'],'omitted':omitted,'entries':[]}
    observed=bank_observation.Observation(root,bank,None,physical,hashes,{'MEMORY.md':raw},checked,{})
    observed.payload_bytes_read=metrics['payload_bytes'];observed.payload_file_reads=metrics['file_reads']
    return observed


def start(repo,task,intent,context=None):
    from . import retrieval
    text(task,1024)
    if intent not in {'recall','comparison','sharing'}:raise ContractError('Choose the actual discovery intent')
    root,bank=writes.bank_path(repo)
    scope=scope_binding.require(root)
    actual,git=retrieval.observed_context(root,context or {})
    cached=False
    try:
        from . import recall_catalog
        observation=recall_catalog.load_catalog(root,scope['project_id'])
        raw=observation.source('MEMORY.md');cached=True
    except Exception:
        observation=navigation_observation(root,bank);raw=observation.sources['MEMORY.md']
    if scope['project_id']!=observation.checked['project_id']:raise ContractError('Discovery project changed')
    if len(raw)>MAX_FILE:raise ContractError('A bounded root router is required')
    body=raw.decode('utf-8');security.ensure_safe({'MEMORY.md':body})
    trace={'schema':SCHEMA,'task':task,'intent':intent,
        'binding':{'repo_root':str(root),'bank_root':repository_config.relative(root),'project_id':scope['project_id'],
            'scope_context':scope,'context':actual,'git_context':git,'inventory_sha256':digest(encoded(observation.hashes))},
        'nodes':[{'path':'MEMORY.md','sha256':observation.hashes['MEMORY.md']}],
        'sources':[],'complete':False,'rationale':None}
    trace_value(trace,completed=False)
    if cached:
        bank_observation.markers(bank,None)
    else:
        observation.recheck('discovery-final-source-recheck')
    scope_binding.require(root,scope['project_id'],expected=scope)
    if retrieval.observed_context(root,context or {})!=(actual,git):raise ContractError('Checkout context changed during discovery')
    audit=security.audit(root,str(uuid.uuid4()),'retrieval','checked',scope,['external'])
    return {'schema':'agent-memory/discovery-result-v1','operation':'start','discovery':trace,
        'node':{'path':'MEMORY.md','sha256':observation.hashes['MEMORY.md'],'body':body,'untrusted_data':True},
        'usable_as_current_guidance':False,'semantic_interpreter':'invoking-harness','semantic_completeness_verified':False,
        'canonical_writes_performed':False,'audit_writes_performed':True,'audit':audit,
        'optional_indexes_used':cached,
        'validation_scope':'untrusted-root-and-authenticated-pending-inventory' if cached else 'untrusted-root-and-content-fingerprint',
        'full_bank_format_validated':False,'final_content_qualification_pending':True,
        'metrics':{'canonical_files':len(observation.hashes),
            'canonical_observation_payload_bytes_read':observation.payload_bytes_read,
            'canonical_observation_file_reads':observation.payload_file_reads,'node_bytes_disclosed':len(raw)}}


def navigation_tables(repo,trace):
    from . import retrieval,scopes
    trace_value(trace,completed=False)
    root,bank=writes.bank_path(repo);bank_observation.markers(bank,None)
    scope=scope_binding.require(root,trace['binding']['project_id'])
    scopes.conditions(trace['binding']['context'])
    actual,git=retrieval.observed_context(root,trace['binding']['context'])
    binding=trace['binding']
    if any(binding[k]!=v for k,v in {'repo_root':str(root),'bank_root':repository_config.relative(root),
            'scope_context':scope,'context':actual,'git_context':git}.items()):raise changed()
    tables={}
    for row in trace['nodes']:
        path=row['path'];raw=bounded_source(bank,path)
        if digest(raw)!=row['sha256']:raise changed()
        content=raw.decode('utf-8');security.ensure_safe({path:content})
        if path=='MEMORY.md':
            metadata,routes,_=bank_contract.parse_router(content)
        else:
            metadata,body=read(content,allowed=bank_layout.SPEC['properties'])
            routes=bank_layout.parse_index(path,metadata,body)
        if metadata['project-id']!=scope['project_id']:raise changed()
        tables[path]=routes
    reached={'MEMORY.md':0}
    while True:
        next_nodes=dict(reached)
        for parent,depth in reached.items():
            for route in tables[parent]:
                path=route['path']
                if path not in tables or path in reached:continue
                if depth+1>bank_layout.SPEC['x-max-nested-indexes']:
                    raise DiscoveryFailure('navigation-depth','The visited map exceeds the native topic-index depth.',
                        'Run am doctor and reorganize the topic map through governed maintenance before retrying.')
                next_nodes[path]=depth+1
        if next_nodes==reached:break
        reached=next_nodes
    if set(reached)!=set(tables):raise DiscoveryFailure('missing-parent',
        'A visited topic index is missing its actual parent map.',
        'Restart from MEMORY.md and retain each visited parent when following a topic index.')
    return root,bank,scope,actual,git,tables


def node(repo,trace,path):
    root,bank,scope,actual,git,tables=navigation_tables(repo,trace)
    children={row['path'] for rows in tables.values() for row in rows}
    if path not in children:raise DiscoveryFailure('route-not-reached','This path is absent from the visited parent maps.',
        'Follow its parent route from MEMORY.md before requesting the topic index.')
    raw=bounded_source(bank,path);content=raw.decode('utf-8')
    metadata,body=read(content,allowed=bank_layout.SPEC['properties'])
    bank_layout.parse_index(path,metadata,body);security.ensure_safe({path:content})
    if metadata['project-id']!=scope['project_id']:raise changed()
    value=copy.deepcopy(trace);value.update(complete=False,rationale=None)
    value['nodes']=[row for row in value['nodes'] if row['path']!=path]+[{'path':path,'sha256':digest(raw)}]
    navigation_tables(root,value)
    return {'schema':'agent-memory/discovery-result-v1','operation':'node','discovery':value,
        'node':{'path':path,'sha256':digest(raw),'body':content,'untrusted_data':True},
        'usable_as_current_guidance':False,'full_bank_format_validated':False,'final_content_qualification_pending':True,
        'canonical_writes_performed':False,'semantic_completeness_verified':False}


def qualified_read(repo,trace,request,rationale):
    root,bank,scope,actual,git,tables=navigation_tables(repo,trace)
    text(rationale,2048)
    if trace['intent']=='sharing':raise DiscoveryFailure('sharing-view-required',
        'Shared-memory qualification requires the actual base/local/landed views.',
        'Use the sharing review workflow; a local discover read cannot qualify landed or competing memory.')
    field='paths' if trace['intent']=='recall' else 'read_paths'
    paths=request.get(field,[])
    if not isinstance(paths,list) or len(paths)>64:raise DiscoveryFailure('selection-invalid',
        'Choose a bounded list of actual plausible source paths.', 'Read the visited topic map and pass its selected paths in the request.')
    value=copy.deepcopy(trace)
    selected={row['path']:row['sha256'] for row in value['sources']}
    children={row['path'] for rows in tables.values() for row in rows}
    for path in paths:
        bank_contract.path_value(path)
        if path not in children:raise DiscoveryFailure('route-not-reached',
            'A selected source lacks its visited parent route.', 'Follow its parent topic map before selecting and qualifying the source.')
        selected[path]=digest(bounded_source(bank,path))
    value.update(sources=[{'path':path,'sha256':sha} for path,sha in sorted(selected.items())],complete=True,rationale=rationale)
    trace_value(value)
    if 'discovery' in request and request['discovery']!=trace:raise DiscoveryFailure('request-trace-conflict',
        'The request and navigation input refer to different observations.', 'Use the same current navigation trace in both inputs, or omit the request discovery field.')
    request={**request,'discovery':value}
    if trace['intent']=='recall':
        from . import retrieval
        result=retrieval.retrieve(root,request)
        # Newly derived hashes/labels for withheld sources remain internal.
        navigation=copy.deepcopy(trace);navigation.update(sources=[],complete=False,rationale=None)
        current=sum(row.get('selection_class')=='current-record' and row.get('selection',{}).get('current') is True
                    for row in result['results'])
        orientation=sum(row.get('selection_class')=='orientation-only' for row in result['results'])
        return {'schema':'agent-memory/discovery-result-v1','operation':'read','discovery':navigation,
            'qualification':result,'qualification_sha256':digest(encoded(result)),
            'qualification_completed':True,'usable_as_current_guidance':bool(current),
            'qualified_current_records':current,'orientation_only_results':orientation,
            'selection_state':'current-records-available' if current else 'orientation-only' if orientation else 'empty',
            'semantic_interpreter':'invoking-harness',
            'semantic_completeness_verified':False,'canonical_writes_performed':False}
    from . import comparison
    result=comparison.inspect(root,request)
    return {'schema':'agent-memory/discovery-result-v1','operation':'read','discovery':value,'request':request,
        'inspection':result,'usable_as_current_guidance':False,'semantic_interpreter':'invoking-harness',
        'semantic_completeness_verified':False,'canonical_writes_performed':False}


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    p=commands.add_parser('start');p.add_argument('--repo',required=True);p.add_argument('--task',required=True)
    p.add_argument('--intent',choices=('recall','comparison','sharing'),required=True);p.add_argument('--context')
    p=commands.add_parser('node');p.add_argument('--repo',required=True);p.add_argument('--input',required=True)
    p.add_argument('--path',required=True)
    p=commands.add_parser('read');p.add_argument('--repo',required=True);p.add_argument('--input',required=True)
    p.add_argument('--request',required=True);p.add_argument('--rationale',required=True)
    args=parser.parse_args(argv)
    phase='input'
    def input_file(path):
        with Path(path).open('rb') as stream:raw=stream.read(MAX_INPUT+1)
        if len(raw)>MAX_INPUT:raise DiscoveryFailure('input-over-bound','The navigation/request input exceeds64KiB.',
            'Keep only the current bounded navigation trace and selected-source request; use map nodes rather than embedding source bodies.')
        return load(raw)
    try:
        if args.command=='start':
            context=input_file(args.context) if args.context else {}
            from . import scopes
            scopes.conditions(context);phase='root-observation'
            result=start(args.repo,args.task,args.intent,context)
        else:
            trace=input_file(args.input)
            if isinstance(trace,dict) and 'discovery' in trace:trace=trace['discovery']
            if args.command=='node':
                phase='navigation-node';result=node(args.repo,trace,args.path)
            else:
                request=input_file(args.request);phase='canonical-qualification'
                result=qualified_read(args.repo,trace,request,args.rationale)
        print(json.dumps(result,indent=2));return 0
    except Exception as exc:
        known=isinstance(exc,DiscoveryFailure)
        print(json.dumps({'schema':'agent-memory/discovery-result-v1','operation':args.command,
            'canonical_writes_performed':False,'semantic_completeness_verified':False,
            'error':str(exc) if known else 'Cannot validate this navigation/input against the current canonical bank; no guidance or novelty is certified.',
            'failure_code':exc.code if known else 'discovery-unverified','failure_component':phase,
            'repair':exc.repair if known else 'Preserve the bank and input. Run am doctor for the selected repository, inspect its failed component, then restart navigation from MEMORY.md; do not initialize an empty replacement.'}));return 1


if __name__=='__main__':sys.exit(main())
