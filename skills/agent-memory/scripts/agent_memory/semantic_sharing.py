"""Source-bound semantic review of repository-local and landed shared learning.

The agent supplies meaning judgments. This helper binds every judgment to actual
sources, preserves provenance, and keeps disputed updates out of independent
publication/refresh plans. It does not authenticate truth or perform exchange.
"""
from __future__ import annotations

import argparse
import copy
from datetime import datetime, timezone
import difflib
import itertools
import json
from pathlib import Path, PurePosixPath
import sys
import uuid

from . import bank_contract as bank, bank_layout as layout, bank_transactions as tx
from . import bank_writes as writes, consolidation, governance, lifecycle, records
from . import retrieval, scope_binding, security, scopes
from .developer_identity import digest, encoded, load
from .frontmatter import ContractError, read, write
from .identity_git import git

LIMIT = 1048576
CLASSES = {'compatible', 'equivalent', 'contradiction', 'disjoint-conditions', 'related', 'uncertain', 'lifecycle'}


def safe_text(value, bound=4096):
    if not isinstance(value, str) or not value.strip() or len(value.encode()) > bound:
        raise ContractError('A bounded explicit semantic rationale is required')
    security.ensure_safe({'review': value})
    return value


def snapshots(repo, base=None):
    from . import sharing_recovery as recovery
    observation = recovery.observe(repo);root=Path(observation['repo_root']);head=observation['authority_head']
    if base is None:
        values=recovery.command(root,'merge-base','--all',observation['local_head'],head).splitlines()
        if len(values)!=1:raise ContractError('A lost cursor needs an unambiguous actual common base')
        base=values[0]
    recovery.commit(root,base)
    if not recovery.ancestor(root,base,head):raise ContractError('The reviewed baseline must remain in authority history')
    local=tx.files_at(root/observation['bank_root'])
    values={'base':recovery.tree(root,base,observation['bank_root']), 'local':local,
            'landed':recovery.tree(root,head,observation['bank_root'])}
    checked={side:recovery.bounded(files) for side,files in values.items()}
    if any(v['project_id']!=observation['project_id'] for v in checked.values()):
        raise ContractError('Sharing review cannot transfer knowledge between projects')
    binding={key:value for key,value in observation.items() if key not in {'observed_at'}}
    binding.update(base=base,source_hashes={side:tx.file_hashes(files) for side,files in values.items()})
    return root,observation,binding,values,checked


def units(values,checked):
    output={}
    for side in ('local','landed'):
        entries=[{'path':'MEMORY.md','format':'root-router','scope_chain':['repository'],'lifecycle':'active'},*checked[side]['entries']]
        for entry in entries:
            if entry['format'] not in {'context-prose','standalone-record','root-router','topic-index'}:continue
            path=entry['path'];text=values[side][path];sha=digest(text.encode())
            key=digest(encoded({'path':path,'sha256':sha}))
            if key in output:
                output[key]['sides'].append(side)
                output[key]['scope_chains'][side]=entry['scope_chain'];continue
            metadata=None;body=text
            container=None
            if entry['format']=='standalone-record':_,metadata,body=records.parse(text)
            elif entry['format'] in {'root-router','topic-index'}:
                header,fullbody=read(text,allowed=set(bank.SPEC['properties'])|set(layout.SPEC['properties']))
                body=fullbody.split('\n## Routes\n',1)[0]
                container={'metadata':header,'routes':bank.parse_router(text)[1] if entry['format']=='root-router' else layout.parse_index(path,header,fullbody)}
                if entry['format']=='root-router':container['omitted']=bank.parse_router(text)[2]
            row={'key':key,'path':path,'sha256':sha,'sides':[side], 'metadata':metadata,'body':body,
                 'scope_chain':entry['scope_chain'],'scope_chains':{side:entry['scope_chain']},'lifecycle':entry['lifecycle'],
                 'format':entry['format'],'component':'introduction' if container is not None else 'body','container':container}
            security.ensure_safe({path:text})
            output[key]=row
    if len(output)>32 or len(encoded(output))>65536:
        raise ContractError('Complete semantic source review exceeds its bound; split reviewed independent units without claiming absence')
    return output


def pairs(rows):
    # All source pairs, including separate files and textually clean edits. No
    # lexical ranking is allowed to omit a possible contradiction.
    return [list(pair) for pair in itertools.combinations(sorted(rows),2)]


def inspect(repo,base=None):
    root,observation,binding,values,checked=snapshots(repo,base)
    rows=units(values,checked)
    final_sources(root,observation,binding)
    audit=security.audit(root,str(uuid.uuid4()),'retrieval','checked',observation['scope_context'],['external'],
        record_ids=sorted({row['metadata']['id'] for row in rows.values() if row['metadata'] is not None}))
    final_sources(root,observation,binding)
    return {'schema':'agent-memory/semantic-sharing-inspection-v1','binding':binding,
            'sources':list(rows.values()),'required_pairs':pairs(rows),'audit':audit,'audit_writes_performed':True,
            'detecting_developer_id':observation['scope_context']['developer_id'],
            'semantic_truth_verified':False,'publication_verified':False,'canonical_writes_performed':False}


def final_sources(root,observation,binding):
    from . import sharing_recovery as recovery
    scope_binding.require(root,observation['project_id'],expected=observation['scope_context'])
    if (recovery.server(root,observation['authority_remote'],observation['authority_branch'])!=observation['authority_head']
            or writes.snapshot(root)[1]!=binding['source_hashes']['local'] or git(root,'rev-parse','--verify','HEAD')!=observation['local_head']):
        raise ContractError('Authority or local source/head changed; review the new source set')


def qualified_sources(files,root,project_id):
    checked=bank.validate_files(files);actual,_=retrieval.observed_context(root,{})
    now=datetime.now(timezone.utc).isoformat();qualified={};superseders={}
    for entry in checked['entries']:
        if entry['format']!='standalone-record':continue
        path=entry['path'];_,meta,_=records.parse(files[path])
        if retrieval.scope_reasons(project_id,entry,actual):continue
        applies=records.applicability(meta,{'bank_project_id':project_id,'context':actual})['applicable']
        selection=lifecycle.current({**meta,'trust':meta.get('trust','reviewed'),'visibility':meta.get('visibility','visible')},
            {'authorized-scope':True,'applicable':applies,'minimum-trust':'reviewed','now':now,'active-superseders':[]})
        required=('authorized-scope','applicable','active','valid-time','sufficient-trust')
        if not all(selection['gates'][key] for key in required):continue
        qualified[path]=(meta,selection)
        for ref in meta['supersedes']+meta['corrects']:
            superseders.setdefault((ref['id'],ref['version']),[]).append(meta['id']+'/'+str(meta['version']))
    return {path for path,(meta,selection) in qualified.items()
            if selection['current'] and not superseders.get((meta['id'],meta['version']))}


def disjoint(left,right):
    def constraints(row,chain):
        output={key:[value] for key,value in (layout.constraints(row['metadata']) if row['metadata'] else {}).items()}
        for scope in chain:
            parsed=scopes.parse_scope(scope)
            if parsed:output.setdefault(parsed[0],[]).append(parsed[1])
        return output
    def nonoverlap(ca,cb):
        for key in set(ca)&set(cb):
            for a,b in itertools.product(ca[key],cb[key]):
                if a is None or b is None:continue
                if key=='component':
                    pa,pb=PurePosixPath(a),PurePosixPath(b)
                    if pa!=pb and pa not in pb.parents and pb not in pa.parents:return True
                elif a!=b:return True
        return False
    a,b=left['metadata'],right['metadata']
    if a is not None and b is not None:
        if a['valid-until'] and b['valid-from'] and lifecycle.time_value(a['valid-until'])<=lifecycle.time_value(b['valid-from']):return True
        if b['valid-until'] and a['valid-from'] and lifecycle.time_value(b['valid-until'])<=lifecycle.time_value(a['valid-from']):return True
    # Identical bytes can be routed under different conditions on each side.
    # Every actual source-scope variant must be disjoint, not only the first.
    ca=[constraints(left,chain) for chain in left['scope_chains'].values()]
    cb=[constraints(right,chain) for chain in right['scope_chains'].values()]
    return bool(ca and cb) and all(nonoverlap(x,y) for x,y in itertools.product(ca,cb))


def introduction(text):
    return text.split('\n---\n',1)[1].split('\n## Routes\n',1)[0]


def replace_introduction(text,prefix):
    header,body=text.split('\n---\n',1)
    _,tail=body.split('\n## Routes\n',1)
    return header+'\n---\n'+prefix+'\n## Routes\n'+tail


def container_paths(files):
    checked=bank.validate_files(files)
    return {'MEMORY.md',*[e['path'] for e in checked['entries'] if e['format']=='topic-index']}


def compatible_prose(base,local,landed):
    """Merge only nonoverlapping actual line edits; no last-writer selection."""
    source=base.splitlines(keepends=True);edits=[]
    for text in (local,landed):
        for tag,a,b,c,d in difflib.SequenceMatcher(None,source,text.splitlines(keepends=True),autojunk=False).get_opcodes():
            if tag!='equal':edits.append((a,b,text.splitlines(keepends=True)[c:d]))
    unique=[]
    for edit in edits:
        if edit not in unique:unique.append(edit)
    for (a,b,replacement),(c,d,other) in itertools.combinations(unique,2):
        if (max(a,c)<min(b,d) or a==c or (a==b and c<a<d) or (c==d and a<c<b)):
            raise ContractError('Overlapping prose changes require an explicit governed correction, not compatible merge')
    result=list(source)
    for a,b,replacement in sorted(unique,key=lambda x:(x[0],x[1]),reverse=True):result[a:b]=replacement
    return ''.join(result)


def mask(files,baseline,blocked):
    """Undo only disputed payload deltas; structural containers follow the payload."""
    checked=bank.validate_files(files);previous=bank.validate_files(baseline)
    current=dict(files);structural=container_paths(files)|container_paths(baseline)
    for path in blocked&structural:
        if path not in current:continue
        if path in baseline:prefix=introduction(baseline[path])
        elif path!='MEMORY.md':
            header,body=read(current[path],allowed=layout.SPEC['properties'])
            routes=layout.parse_index(path,header,body)
            prefix=introduction(layout.index_text(path,header,routes))
        else:raise ContractError('An existing project router is required')
        current[path]=replace_introduction(current[path],prefix)
    routed_input=dict(current)
    ordinary=blocked-structural
    removed={p for p in ordinary if p in files};additions=[]
    for path in ordinary:
        if path in baseline:
            current[path]=baseline[path]
            route=next((r for r in previous['routes'] if r['path']==path),None)
            if route is None:
                entry=next((e for e in previous['entries'] if e['path']==path),None)
                if entry is None or entry['format']!='standalone-record':
                    raise ContractError('Disputed nested structural changes require explicit isolation')
                _,meta,_=records.parse(baseline[path]);route=consolidation.route(path,'Retained reviewed source',meta)
            additions.append(route)
        else:current.pop(path,None)
    # Reuse the existing route-preserving removal protocol. Context omission
    # changes need their original explanations, rather than fabricated prose.
    at=bank.parse_router(files['MEMORY.md'])[0]['router-updated']
    routed=governance.reroute(routed_input,bank.validate_files(routed_input),{},removed,additions,at)
    for path in removed:
        if path in current:routed[path]=current[path]
    # The intermediate router can lack the omission row for a disputed new
    # context. Read its actual table before completing omission bookkeeping;
    # never validate or publish that intermediate shape.
    meta,body=read(routed['MEMORY.md'],allowed=bank.SPEC['properties'])
    route_text=body.split('\n## Routes\n',1)[1].split('\n## Omitted contexts\n',1)[0]
    routes=bank.parse_table(route_text)
    omitted=dict(bank.parse_router(files['MEMORY.md'])[2])
    before_omitted=bank.parse_router(baseline['MEMORY.md'])[2]
    for context in bank.CONTEXTS:
        if context in routed:omitted.pop(context,None)
        elif context in before_omitted:omitted[context]=before_omitted[context]
    _,body=read(routed['MEMORY.md'],allowed=meta)
    prefix,tail=body.split('\n## Routes\n',1)
    routed['MEMORY.md']=write(meta)+prefix+'\n## Routes\n\n'+bank.route_table(routes)+'\n## Omitted contexts\n\n'+(
        ''.join('- `'+p+'`: '+omitted[p]+'\n' for p in bank.CONTEXTS if p in omitted) or 'None.\n')
    bank.validate_files(routed)
    return routed


def reviewed_value(judgment,inspection):
    required={'binding','pairs','dependencies','consolidations','rationale','recorded_at'}
    if not isinstance(judgment,dict) or set(judgment)!=required or judgment['binding']!=inspection['binding']:
        raise ContractError('Semantic judgment must bind complete actual current sources')
    safe_text(judgment['rationale']);lifecycle.time_value(judgment['recorded_at'])
    if lifecycle.time_value(judgment['recorded_at'])>lifecycle.time_value(datetime.now(timezone.utc).isoformat()):
        raise ContractError('Review cannot be future dated')
    if not isinstance(judgment['pairs'],list):raise ContractError('Review every actual source pair')
    decisions={}
    for decision in judgment['pairs']:
        if (not isinstance(decision,dict) or set(decision)!={'left','right','classification','reason'}
                or decision['classification'] not in CLASSES):raise ContractError('Unknown semantic pair decision')
        key=(decision['left'],decision['right']);safe_text(decision['reason'])
        if key in decisions:raise ContractError('Do not duplicate source judgments')
        decisions[key]=decision
    if set(decisions)!={tuple(p) for p in inspection['required_pairs']}:
        raise ContractError('Semantic review must cover cross-file and same-file source pairs completely')
    return decisions


def consolidate(files,groups,rows,decisions,context,operation_id,at,blocked):
    if not isinstance(groups,list) or len(groups)>8:raise ContractError('Consolidate bounded explicit duplicate groups')
    used=set();current=files
    for group in groups:
        if (not isinstance(group,dict) or set(group)!={'sources','primary','body','reason'}
                or not isinstance(group['sources'],list) or not 2<=len(group['sources'])<=8
                or len(set(group['sources']))!=len(group['sources']) or group['primary'] not in group['sources']):
            raise ContractError('A duplicate group names distinct complete source keys and an existing primary')
        safe_text(group['body'],4096);safe_text(group['reason'])
        if any(key not in rows or key in used for key in group['sources']):raise ContractError('Source group is missing or overlaps another group')
        used.update(group['sources']);selected=[rows[key] for key in group['sources']]
        if any(row['path'] in blocked for row in selected):continue
        if len({row['path'] for row in selected})!=len(selected):raise ContractError('Divergent versions of one path require lifecycle review')
        for pair in itertools.combinations(sorted(group['sources']),2):
            if decisions[pair]['classification']!='equivalent':raise ContractError('Only reviewed equivalent claims can consolidate')
        primary=rows[group['primary']];metadata=primary['metadata']
        if metadata is None or any(row['metadata'] is None or row['lifecycle']!='active' for row in selected):
            raise ContractError('Duplicate maintenance requires active typed records')
        if any(any(row['metadata'].get(k)!=metadata.get(k) for k in consolidation.AXES) for row in selected):
            raise ContractError('Distinct conditions, validity, type, trust and visibility must remain separate')
        targets=[]
        for row in selected:
            if current.get(row['path']) is None or digest(current[row['path']].encode())!=row['sha256']:
                raise ContractError('A duplicate source changed during three-way reconciliation')
            meta=row['metadata']
            if row['path'] not in qualified_sources(current,Path(context['repo_root']),meta['project-id']):
                raise ContractError('Unknown, invalidated or inactive duplicate applicability requires lifecycle review')
            targets.append({'path':row['path'],'id':meta['id'],'version':meta['version'],'sha256':row['sha256'],
                            'required_preserved_metadata':meta,'scope_chain':row['scope_chain']})
        chosen=next(t for t in targets if t['path']==primary['path'])
        evidence={'comparison_request':{'candidate':{'metadata':metadata,'body':group['body']}},
                  'comparison_proof':{'judgment':{'rationale':group['reason']}},'recorded_at':at}
        plan={'primary':chosen,'targets':targets,'qualified':{'scope_context':context['scope_context']}}
        child=consolidation.child_id(operation_id,'group-'+str(len(used)))
        result=consolidation.build(current,evidence,child,plan,None)
        if result is not None:current=result
    return current


def review(repo,judgment):
    from . import sharing_recovery as recovery
    if not isinstance(judgment,dict) or not isinstance(judgment.get('binding'),dict):raise ContractError('Review requires an inspection binding')
    base=judgment['binding'].get('base');root,observation,binding,values,checked=snapshots(repo,base)
    rows=units(values,checked);inspection={'binding':binding,'sources':list(rows.values()),'required_pairs':pairs(rows)}
    decisions=reviewed_value(judgment,inspection);blocked=set();conflicts=[];peer=set();equivalents=set()
    actor=observation['scope_context']['developer_id']
    for pair,decision in decisions.items():
        left,right=(rows[key] for key in pair);classification=decision['classification']
        if classification=='disjoint-conditions' and not disjoint(left,right):
            raise ContractError('Disjoint conditions require an actual known non-overlap; unknown is not disjoint')
        if classification=='contradiction' and disjoint(left,right):raise ContractError('Disjoint claims need their distinct conditions preserved')
        if classification=='lifecycle':
            a,b=left['metadata'],right['metadata']
            if (a is None or b is None or not any(
                    {'id':old['id'],'version':old['version']} in new.get('supersedes',[])+new.get('corrects',[])
                    or {'id':new['id'],'version':new['version']} in old.get('superseded-by',[])
                    for old,new in ((a,b),(b,a)))):
                raise ContractError('A lifecycle distinction needs actual retained version links')
        if classification=='equivalent':equivalents.add(pair)
        if classification in {'contradiction','uncertain'}:
            blocked.update((left['path'],right['path']))
            other={row['metadata']['origin']['developer-id'] for row in (left,right) if row['metadata'] is not None}- {actor}
            peer.update(other)
            conflicts.append({'classification':classification,'claims':[left,right],'reason':decision['reason'],
                              'other_developer_ids':sorted(other),'detecting_developer_id':actor})
    if not isinstance(judgment['consolidations'],list):raise ContractError('Duplicate groups require an explicit bounded list')
    dependencies=judgment['dependencies']
    if not isinstance(dependencies,dict):raise ContractError('Dependent updates need an explicit path graph')
    actual=set().union(*(set(f) for f in values.values()))
    for path,prerequisites in dependencies.items():
        if path not in actual or not isinstance(prerequisites,list) or any(p not in actual for p in prerequisites):
            raise ContractError('Dependencies must name actual bank paths')
    while True:
        expanded=blocked|{p for p,required in dependencies.items() if set(required)&blocked}
        if expanded==blocked:break
        blocked=expanded
    # No ungrouped same-axis equivalent records can silently become duplicates.
    grouped={tuple(sorted(pair)) for group in judgment['consolidations'] for pair in itertools.combinations(group.get('sources',[]),2)}
    for pair in equivalents:
        a,b=(rows[k] for k in pair)
        if a['path'] in blocked or b['path'] in blocked or a['path']==b['path']:continue
        if (a['metadata'] is not None and b['metadata'] is not None and a['lifecycle']==b['lifecycle']=='active'
                and all(a['metadata'].get(k)==b['metadata'].get(k) for k in consolidation.AXES) and pair not in grouped):
            raise ContractError('Equivalent current typed learning requires explicit provenance-preserving consolidation')
    local,landed=values['local'],values['landed'];baseline=values['base']
    for path in set(local)&set(landed)&set(baseline)&set(bank.CONTEXTS)-blocked:
        if local[path]!=landed[path] and local[path]!=baseline[path] and landed[path]!=baseline[path]:
            pair=tuple(sorted([digest(encoded({'path':path,'sha256':digest(side[path].encode())})) for side in (local,landed)]))
            if decisions[pair]['classification'] not in {'compatible','equivalent'}:
                raise ContractError('Concurrent same-file prose needs explicit compatible meaning review')
            body=compatible_prose(baseline[path],local[path],landed[path])
            # Resolve exactly the reviewed nonoverlapping delta, not arbitrary replacement text.
            local=dict(local);landed=dict(landed);baseline=dict(baseline)
            local[path]=landed[path]=baseline[path]=body
    structural=set().union(*(container_paths(f) for f in (baseline,local,landed)))
    for path in structural-blocked:
        if not all(path in f for f in (baseline,local,landed)):continue
        prefixes=[introduction(f[path]) for f in (baseline,local,landed)]
        if prefixes[1]!=prefixes[2] and prefixes[1]!=prefixes[0] and prefixes[2]!=prefixes[0]:
            pair=tuple(sorted([digest(encoded({'path':path,'sha256':digest(side[path].encode())})) for side in (local,landed)]))
            if decisions[pair]['classification'] not in {'compatible','equivalent'}:
                raise ContractError('Concurrent container introductions require actual compatible meaning review')
            prefix=compatible_prose(*prefixes)
            baseline=dict(baseline);local=dict(local);landed=dict(landed)
            for files in (baseline,local,landed):files[path]=replace_introduction(files[path],prefix)
    refresh_land=mask(landed,baseline,blocked) if blocked else landed
    publish_local=mask(local,baseline,blocked) if blocked else local
    refresh=recovery.merge_files(baseline,local,refresh_land)
    publish=recovery.merge_files(baseline,publish_local,landed)
    operation_id=str(uuid.UUID(hex=digest(encoded(judgment))[:32],version=4))
    refresh=consolidate(refresh,judgment['consolidations'],rows,decisions,observation,operation_id,judgment['recorded_at'],blocked)
    publish=consolidate(publish,judgment['consolidations'],rows,decisions,observation,operation_id,judgment['recorded_at'],blocked)
    audit=security.audit(root,operation_id,'retrieval','checked',observation['scope_context'],['external'],
        record_ids=sorted({row['metadata']['id'] for row in rows.values() if row['metadata'] is not None}))
    final_sources(root,observation,binding)
    proof={'schema':'agent-memory/semantic-sharing-proof-v1','judgment':judgment,
           'refresh_hashes':tx.file_hashes(refresh),'publish_hashes':tx.file_hashes(publish),'blocked_paths':sorted(blocked)}
    if len(encoded(proof))>LIMIT:raise ContractError('Complete semantic proof exceeds bound; no truncation')
    return {'schema':'agent-memory/semantic-sharing-result-v1','operation':'review','proof':proof,
            'proof_sha256':digest(encoded(proof)),'audit':audit,'audit_writes_performed':True,'refresh_files':refresh,'publish_files':publish,
            'conflicts':conflicts,'blocked_paths':sorted(blocked),
            'blocked_introduction_paths':sorted(blocked&structural),'detecting_developer_id':actor,
            'other_developer_ids':sorted(peer),'recommend_other_developer_clarification':bool(peer),'messages_sent':False,
            'independent_updates_may_continue':True,'semantic_truth_verified':False,'judgment_is_agent_assertion':True,
            'remote_freshness_verified':True,'publication_verified':False,'canonical_writes_performed':False,
            'git_metadata_writes_performed':True,'refresh_applied':False}


def check(repo,proof):
    if not isinstance(proof,dict) or set(proof)!={'schema','judgment','refresh_hashes','publish_hashes','blocked_paths'} or proof['schema']!='agent-memory/semantic-sharing-proof-v1':
        raise ContractError('Use the complete actual semantic proof')
    value=review(repo,proof['judgment'])
    if encoded(value['proof'])!=encoded(proof):raise ContractError('Semantic proof does not describe current reviewed sources and outputs')
    return {**value,'operation':'check','source_observation_current':True}


def read_input(path):
    with Path(path).open('rb') as stream:raw=stream.read(LIMIT+1)
    if len(raw)>LIMIT:raise ContractError('Semantic request exceeds complete reader budget')
    return load(raw)


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='command',required=True)
    for name in ('inspect','review','check'):
        p=sub.add_parser(name);p.add_argument('--repo',required=True)
        if name=='inspect':p.add_argument('--base')
        else:p.add_argument('--input',required=True)
    args=parser.parse_args(argv)
    try:
        if args.command=='inspect':value=inspect(args.repo,args.base)
        elif args.command=='review':value=review(args.repo,read_input(args.input))
        else:value=check(args.repo,read_input(args.input))
        print(json.dumps(value,indent=2));return 0
    except (ContractError,OSError,ValueError,TypeError,KeyError,AttributeError,UnicodeError,RecursionError):
        print(json.dumps({'schema':'agent-memory/semantic-sharing-result-v1','operation':args.command,
            'publication_verified':False,'canonical_writes_performed':False,'remote_freshness_verified':False,
            'error':'Incomplete, changed or unsafe semantic review; retain local work and inspect actual sources'}));return 1


if __name__=='__main__':sys.exit(main())
