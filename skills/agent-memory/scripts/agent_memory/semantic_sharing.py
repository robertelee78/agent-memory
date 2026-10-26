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
from . import sharing_limits as limits

LIMIT = 1048576
CLASSES = {'compatible', 'equivalent', 'contradiction', 'disjoint-conditions', 'related', 'uncertain', 'lifecycle'}


def safe_text(value, bound=4096):
    if not isinstance(value, str) or not value.strip() or len(value.encode()) > bound:
        raise ContractError('A bounded explicit semantic rationale is required')
    security.ensure_safe({'review': value})
    return value


def snapshots(repo, base=None, *, owned_lock=None, quarantine_records=False):
    from . import sharing_recovery as recovery
    observation = recovery.observe(repo, owned_lock=owned_lock,quarantine_records=quarantine_records);root=Path(observation['repo_root']);head=observation['authority_head']
    if base is None:
        values=recovery.command(root,'merge-base','--all',observation['local_head'],head).splitlines()
        if len(values)!=1:raise ContractError('A lost cursor needs an unambiguous actual common base')
        base=values[0]
    recovery.commit(root,base)
    if not recovery.ancestor(root,base,head):raise ContractError('The reviewed baseline must remain in authority history')
    local=tx.files_at(root/observation['bank_root'])
    values={'base':recovery.tree(root,base,observation['bank_root'],quarantine_records=quarantine_records), 'local':local,
            'landed':recovery.tree(root,head,observation['bank_root'],quarantine_records=quarantine_records)}
    checked={side:recovery.bounded(files,allow_absent=True,quarantine_records=quarantine_records) for side,files in values.items()}
    if any(v['project_id'] not in {None,observation['project_id']} for v in checked.values()):
        raise ContractError('Sharing review cannot transfer knowledge between projects')
    binding={key:value for key,value in observation.items() if key not in {'observed_at'}}
    binding.update(base=base,source_hashes={side:tx.file_hashes(files) for side,files in values.items()})
    return root,observation,binding,values,checked


def units(values,checked):
    output={}
    def add(side,entry,body,metadata=None,*,component='body',target=None,container=None,scope_chain=None):
        path=entry['path'];sha=digest(values[side][path].encode());chain=scope_chain or entry['scope_chain']
        identity={'path':path,'sha256':sha} if component=='body' else {
            'path':path,'component':component,'target_path':target,'body_sha256':digest(body.encode())}
        key=digest(encoded(identity))
        if key in output:
            row=output[key];row['sides'].append(side);row['scope_chains'][side]=chain
            row['source_sha256_by_side'][side]=sha;row['container_variants'][side]=container
            return
        row={'key':key,'path':path,'sha256':sha,'source_sha256_by_side':{side:sha},'sides':[side],
             'metadata':metadata,'body':body,'scope_chain':chain,'scope_chains':{side:chain},
             'lifecycle':entry['lifecycle'],'format':entry['format'],'component':component,'target_path':target,
             'container':container,'container_variants':{side:container}}
        output[key]=row
    for side in ('local','landed'):
        if not values[side]:continue
        entries=[{'path':'MEMORY.md','format':'root-router','scope_chain':['repository'],'lifecycle':'active'},*checked[side]['entries']]
        for entry in entries:
            if entry['format'] not in {'context-prose','standalone-record','root-router','topic-index'}:continue
            path=entry['path'];text=values[side][path];security.ensure_safe({path:text})
            if entry['format']=='standalone-record':
                _,metadata,body=records.parse(text);add(side,entry,body,metadata);continue
            if entry['format']=='context-prose':add(side,entry,text);continue
            header,body=read(text,allowed=set(bank.SPEC['properties'])|set(layout.SPEC['properties']))
            routes=bank.parse_router(text)[1] if entry['format']=='root-router' else layout.parse_index(path,header,body)
            context={'metadata':header,'routes':routes}
            if entry['format']=='root-router':context['omitted']=bank.parse_router(text)[2]
            add(side,entry,body.split('\n## Routes\n',1)[0],component='introduction',container=context)
            for route in routes:
                prose='Purpose: '+route['purpose']+'\nRead when: '+route['read-when']+'\n'
                add(side,entry,prose,component='route-prose',target=route['path'],
                    container={'metadata':header,'route':route},scope_chain=[*entry['scope_chain'],route['scope']])
            for target,reason in context.get('omitted',{}).items():
                add(side,entry,reason,component='omission',target=target,
                    container={'metadata':header,'context_path':target})
    if len(output)>64 or len(encoded(output))>65536:
        raise ContractError('Complete semantic source review exceeds its bound; split reviewed independent units without claiming absence')
    return output


def pairs(rows):
    # All source pairs, including separate files and textually clean edits. No
    # lexical ranking is allowed to omit a possible contradiction.
    return [list(pair) for pair in itertools.combinations(sorted(rows),2)]


def inspect(repo,base=None, *, owned_lock=None):
    root,observation,binding,values,checked=snapshots(repo,base,owned_lock=owned_lock)
    rows=units(values,checked)
    final_sources(root,observation,binding,owned_lock=owned_lock)
    audit=security.audit(root,str(uuid.uuid4()),'retrieval','checked',observation['scope_context'],['external'],
        record_ids=sorted({row['metadata']['id'] for row in rows.values() if row['metadata'] is not None}))
    final_sources(root,observation,binding,owned_lock=owned_lock)
    return {'schema':'agent-memory/semantic-sharing-inspection-v1','binding':binding,
            'sources':list(rows.values()),'required_pairs':pairs(rows),'audit':audit,'audit_writes_performed':True,
            'detecting_developer_id':observation['scope_context']['developer_id'],
            'semantic_truth_verified':False,'publication_verified':False,'canonical_writes_performed':False}


def final_sources(root,observation,binding, *, owned_lock=None):
    from . import sharing_recovery as recovery
    scope_binding.require(root,observation['project_id'],expected=observation['scope_context'],owned_lock=owned_lock)
    if (recovery.server(root,observation['authority_remote'],observation['authority_branch'])!=observation['authority_head']
            or writes.snapshot(root,owned_lock=owned_lock)[1]!=binding['source_hashes']['local'] or git(root,'rev-parse','--verify','HEAD')!=observation['local_head']):
        raise ContractError('Authority or local source/head changed; review the new source set')


def qualified_sources(files,root,project_id):
    if not files:return set()
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
    if not files:return set()
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


def child_paths(files,path):
    if not files:return {path}
    if path=='MEMORY.md':return set(files)
    checked=bank.validate_files(files)
    indices={entry['path'] for entry in checked['entries'] if entry['format']=='topic-index'}
    found=set();pending=[path]
    while pending:
        current=pending.pop()
        if current in found:continue
        found.add(current)
        if current in indices:
            header,body=read(files[current],allowed=layout.SPEC['properties'])
            pending.extend(row['path'] for row in layout.parse_index(current,header,body))
    return found


def mask(files,baseline,held):
    """Hold exact prose components and their real navigation-dependent payloads."""
    if not files:return {}
    original=bank.validate_files(files);old=bank.validate_files(baseline) if baseline else {'entries':[],'routes':[]}
    current=dict(files)
    protected={entry['path'] for entry in original['entries'] if entry['format'] in {'tombstone','optional-history-event'}
        or (entry['format']=='standalone-record' and entry['lifecycle']!='active')}
    body_paths={path for path,component,target in held if component=='body'}-protected
    for path in body_paths:
        if path in baseline:current[path]=baseline[path]
        else:current.pop(path,None)
    # Every container remains a source-owned prefix/header and a structured map.
    tables={};headers={};prefixes={};omitted={}
    for path in container_paths(files)|container_paths(baseline):
        if path not in current:continue
        header,body=read(current[path],allowed=set(bank.SPEC['properties'])|set(layout.SPEC['properties']))
        if path=='MEMORY.md':_,routes,omitted=bank.parse_router(current[path])
        else:routes=layout.parse_index(path,header,body)
        headers[path]=header;prefixes[path]=body.split('\n## Routes\n',1)[0]
        tables[path]={row['path']:row for row in routes if row['path'] not in body_paths}
    base_routes={row['path']:('MEMORY.md',row) for row in old['routes']}
    for entry in old['entries']:
        if entry['format']!='topic-index':continue
        path=entry['path'];header,body=read(baseline[path],allowed=layout.SPEC['properties'])
        base_routes.update({row['path']:(path,row) for row in layout.parse_index(path,header,body)})
    for path in body_paths:
        if path not in current or path=='MEMORY.md':continue
        parent,row=base_routes[path]
        if parent not in tables:
            # Typed source metadata retains inherited conditions. Context prose
            # is always a direct root route. Structural moves need review.
            if path in container_paths(baseline):raise ContractError('A disputed index move needs explicit dependency isolation')
            parent='MEMORY.md'
        tables[parent][path]=row
    for path,component,target in held:
        if component!='route-prose' or target not in protected or path not in tables or target not in tables[path]:continue
        row=tables[path][target]
        previous=base_routes.get(target)
        if previous:
            tables[path][target]={**row,'purpose':previous[1]['purpose'],'read-when':previous[1]['read-when']}
        else:
            tables[path][target]={**row,'purpose':'Retained canonical invalidation or history',
                'read-when':'Before using or rebuilding affected guidance'}
    for path,component,target in held:
        if component!='introduction' or path not in tables:continue
        if path in baseline:prefixes[path]=introduction(baseline[path])
        elif path!='MEMORY.md':
            prefixes[path]=introduction(layout.index_text(path,headers[path],list(tables[path].values()))) if tables[path] else ''
        else:raise ContractError('A project router cannot disappear during reconciliation')
    # Empty new indexes are dependent on their held children, not independent
    # publishable guidance. Their original prose remains in the untouched input.
    while True:
        removed={path for path,rows in tables.items() if path!='MEMORY.md' and not rows}
        if not removed:break
        for path in removed:tables.pop(path);current.pop(path,None)
        for rows in tables.values():
            for path in removed:rows.pop(path,None)
    for path,rows in tables.items():
        rows={target:row for target,row in rows.items() if target in current}
        content=write(headers[path])+prefixes[path]+'\n## Routes\n\n'+bank.route_table(list(rows.values()),path)
        if path=='MEMORY.md':
            previous=bank.parse_router(baseline[path])[2] if path in baseline else {}
            reasons={}
            for context in bank.CONTEXTS:
                if context in current:continue
                if context in body_paths and context in previous:reasons[context]=previous[context]
                elif context in omitted:reasons[context]=omitted[context]
                elif context in previous:reasons[context]=previous[context]
                else:raise ContractError('An omitted context needs its actual source explanation')
            content+='\n## Omitted contexts\n\n'+(''.join('- `'+p+'`: '+reasons[p]+'\n' for p in bank.CONTEXTS if p in reasons) or 'None.\n')
        current[path]=content
    bank.validate_files(current)
    return current


def preserve_invalidation(values,candidate):
    guards={};inactive={}
    for files in values.values():
        if not files:continue
        checked=bank.validate_files(files)
        for entry in checked['entries']:
            if entry['format']=='tombstone':
                path=entry['path']
                if path in guards and guards[path]!=files[path]:raise ContractError('Conflicting terminal guards require source review')
                guards[path]=files[path]
            elif entry['format']=='standalone-record' and entry['lifecycle']!='active':
                inactive[entry['id']]=max(inactive.get(entry['id'],0),entry['version'])
    if any(candidate.get(path)!=body for path,body in guards.items()):
        raise ContractError('Semantic deferral cannot discard a terminal guard from the original inputs')
    for entry in bank.validate_files(candidate)['entries']:
        if (entry['format']=='standalone-record' and entry['lifecycle']=='active'
                and entry['version']<=inactive.get(entry['id'],0)):
            raise ContractError('Semantic deferral cannot restore an invalidated original version')


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


def review(repo,judgment, *, owned_lock=None):
    from . import sharing_recovery as recovery
    if not isinstance(judgment,dict) or not isinstance(judgment.get('binding'),dict):raise ContractError('Review requires an inspection binding')
    version2=judgment.get('schema')=='agent-memory/semantic-sharing-judgment-v2'
    if version2:
        from . import sharing_frontier as frontier
        snapshot,rows,inspection=frontier.prepare_review(repo,judgment,owned_lock=owned_lock)
        root,observation,binding,values,checked=snapshot
        plain={k:v for k,v in judgment.items() if k in {'binding','pairs','dependencies','consolidations','rationale','recorded_at'}}
    else:
        base=judgment['binding'].get('base');root,observation,binding,values,checked=snapshots(repo,base,owned_lock=owned_lock)
        rows=units(values,checked);inspection={'binding':binding,'sources':list(rows.values()),'required_pairs':pairs(rows)}
        plain=judgment
    decisions=reviewed_value(plain,inspection);held=set();blocked=set();conflicts=[];peer=set();equivalents=set()
    causes={}
    origin=digest(encoded(judgment))
    actor=observation['scope_context']['developer_id']
    def remember(keys,classification,reason,source_origin=origin):
        from . import sharing_holds
        cause={'sources':sorted(keys),'classification':classification,'reason':reason,'origin':source_origin}
        causes.setdefault(sharing_holds.cause_id(cause),cause)
    resolutions={}
    if version2:
        from . import sharing_hold_mapping as hold_mapping
        proposed=judgment.get('resolutions',{})
        if not isinstance(proposed,dict) or set(proposed)-{known['id'] for known in inspection['known_causes']}:
            raise ContractError('Resolve only actual retained detected disputes')
        qualified=hold_mapping.qualified_current(rows,values,root,observation['project_id'])
        for known in inspection['known_causes']:
            cause=known['cause']
            if known['id'] in proposed:
                resolutions[known['id']]=hold_mapping.validate_resolution(proposed[known['id']],known,rows,qualified)
                involved=set(known['sources'])|set(resolutions[known['id']]['evidence'])
                if any(key not in rows for key in involved) or any(
                        decisions.get(tuple(sorted(pair)),{}).get('classification') in {None,'uncertain','contradiction'}
                        for pair in itertools.combinations(sorted(involved),2)):
                    raise ContractError('Resolution must read and review all actual claims and stronger evidence without unresolved interactions')
                continue
            if known['resolved']:continue
            remember(cause['sources'],cause['classification'],cause['reason'],cause['origin'])
            for key in known['sources']:
                row=rows[key];held.add((row['path'],row['component'],row['target_path']));blocked.add(row['path'])
            for key in known['missing_sources']:
                reference=known['references'][key]
                held.add((reference['path'],reference['component'],reference['target_path']));blocked.add(reference['path'])
            claims=[rows[key] for key in known['sources']]
            others={row['metadata']['origin']['developer-id'] for row in claims if row['metadata'] is not None}-{actor}
            peer.update(others)
            conflicts.append({'classification':cause['classification'],'claims':claims,'reason':cause['reason'],
                              'missing_source_references':[known['references'][key] for key in known['missing_sources']],
                              'other_developer_ids':sorted(others),'detecting_developer_id':actor,'retained_prior_review':True})
        unresolved=set(inspection['withheld_sources'])
        unresolved.update(k for k,s in inspection['selections'].items() if s.get('uncertain'))
        for key in unresolved:
            row=rows[key];held.add((row['path'],row['component'],row['target_path']));blocked.add(row['path'])
            remember([key],'withheld' if key in inspection['withheld_sources'] else 'uncertain-selection',
                     'Selected evidence remains withheld or plausible-match selection remains uncertain; clarify before relying on it')
        # A hidden selected source cannot silently license affected changed work.
        for key,selection in inspection['selections'].items():
            if set(selection['matched'])&unresolved:
                row=rows[key];held.add((row['path'],row['component'],row['target_path']));blocked.add(row['path'])
    for pair,decision in decisions.items():
        left,right=(rows[key] for key in pair);classification=decision['classification']
        if classification=='disjoint-conditions' and not disjoint(left,right):
            raise ContractError('Disjoint conditions require an actual known non-overlap; unknown is not disjoint')
        if classification=='contradiction' and disjoint(left,right):raise ContractError('Disjoint claims need their distinct conditions preserved')
        if classification=='lifecycle':
            a,b=left['metadata'],right['metadata']
            terminal=version2 and frontier.terminal_relation(left,right)
            if not terminal and (a is None or b is None or not any(
                    {'id':old['id'],'version':old['version']} in new.get('supersedes',[])+new.get('corrects',[])
                    or {'id':new['id'],'version':new['version']} in old.get('superseded-by',[])
                    for old,new in ((a,b),(b,a)))):
                raise ContractError('A lifecycle distinction needs actual retained version links')
        if classification=='equivalent':equivalents.add(pair)
        if classification in {'contradiction','uncertain'}:
            if version2:remember(pair,classification,decision['reason'])
            for row in (left,right):held.add((row['path'],row['component'],row['target_path']))
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
    # A disputed read condition/purpose controls its linked payload. Hold
    # that update and index descendants, while unrelated routes can continue.
    seeds={target for path,component,target in held if component in {'route-prose','omission'}}
    seeds.update(path for path,component,target in held if component=='body')
    while True:
        payload=set()
        for path in seeds:
            for files in values.values():payload.update(child_paths(files,path))
        held.update((path,'body',None) for path in payload)
        blocked.update(payload)
        expanded=seeds|{path for path,required in dependencies.items() if set(required)&blocked}
        if expanded==seeds:break
        seeds=expanded
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
            pair=tuple(sorted(next(key for key,row in rows.items() if row['path']==path and row['component']=='body' and side in row['sides']) for side in ('local','landed')))
            if decisions[pair]['classification'] not in {'compatible','equivalent'}:
                raise ContractError('Concurrent same-file prose needs explicit compatible meaning review')
            body=compatible_prose(baseline[path],local[path],landed[path])
            # Resolve exactly the reviewed nonoverlapping delta, not arbitrary replacement text.
            local=dict(local);landed=dict(landed);baseline=dict(baseline)
            local[path]=landed[path]=baseline[path]=body
    structural=set().union(*(container_paths(f) for f in (baseline,local,landed)))
    for path in structural-{p for p,c,t in held if c in {'introduction','body'}}:
        if not all(path in f for f in (baseline,local,landed)):continue
        prefixes=[introduction(f[path]) for f in (baseline,local,landed)]
        if prefixes[1]!=prefixes[2] and prefixes[1]!=prefixes[0] and prefixes[2]!=prefixes[0]:
            pair=tuple(sorted(next(key for key,row in rows.items() if row['path']==path and row['component']=='introduction' and side in row['sides']) for side in ('local','landed')))
            if decisions[pair]['classification'] not in {'compatible','equivalent'}:
                raise ContractError('Concurrent container introductions require actual compatible meaning review')
            prefix=compatible_prose(*prefixes)
            baseline=dict(baseline);local=dict(local);landed=dict(landed)
            for files in (baseline,local,landed):files[path]=replace_introduction(files[path],prefix)
    refresh_land=mask(landed,baseline,held) if held else landed
    publish_local=mask(local,baseline,held) if held else local
    refresh=recovery.merge_files(baseline,local,refresh_land,quarantine_records=version2)
    publish=recovery.merge_files(baseline,publish_local,landed,quarantine_records=version2)
    if version2:
        # Held raw bytes may already exist on a side. They are retained there,
        # never transported as a new/changed payload to its peer.
        for before,after in ((values['local'],refresh),(values['landed'],publish)):
            for path,text in after.items():
                if text!=before.get(path) and security.scan(path+'\n'+text)['reasons']:
                    raise ContractError('Unsafe changed payload remains held; resolve it before exchange')
    operation_id=str(uuid.UUID(hex=digest(encoded(judgment))[:32],version=4))
    refresh=consolidate(refresh,judgment['consolidations'],rows,decisions,observation,operation_id,judgment['recorded_at'],blocked)
    publish=consolidate(publish,judgment['consolidations'],rows,decisions,observation,operation_id,judgment['recorded_at'],blocked)
    audit=security.audit(root,operation_id,'retrieval','checked',observation['scope_context'],['external'],
        record_ids=sorted({row['metadata']['id'] for row in rows.values() if row['metadata'] is not None}))
    preserve_invalidation(values,refresh);preserve_invalidation(values,publish)
    final_sources(root,observation,binding,owned_lock=owned_lock)
    proof={'schema':'agent-memory/semantic-sharing-proof-v2' if version2 else 'agent-memory/semantic-sharing-proof-v1','judgment':judgment,
           'refresh_hashes':tx.file_hashes(refresh),'publish_hashes':tx.file_hashes(publish),'blocked_paths':sorted(blocked),
           'blocked_components':[{'path':p,'component':c,'target_path':t} for p,c,t in sorted(held,key=lambda x:(x[0],x[1],x[2] or ''))]}
    if version2:
        proof['held_causes']=[causes[key] for key in sorted(causes)]
        proof['known_resolutions']=resolutions
    if len(encoded(proof))>(limits.MAX_PRIVATE if version2 else LIMIT):raise ContractError('Complete semantic proof exceeds bound; no truncation')
    if version2:
        from . import sharing_holds
        references={}
        for key,row in rows.items():
            reference={name:row[name] for name in ('path','component','target_path','source_sha256_by_side')}
            metadata=row.get('metadata') or (row.get('witness_metadata') if row['format']=='standalone-record' else None)
            if metadata is not None:
                reference['record']={name:metadata[name] for name in ('id','version')}
            references[key]=reference
        for known in inspection['known_causes']:references.update(known['references'])
        sharing_holds.record(root,observation['project_id'],proof,references,resolutions=resolutions,owned_lock=owned_lock,
            verify=lambda owner:final_sources(root,observation,binding,owned_lock=owner),
            expected_inventory=inspection['known_inventory_sha256'])
    return {'schema':'agent-memory/semantic-sharing-result-v1','operation':'review','proof':proof,
            'proof_sha256':digest(encoded(proof)),'audit':audit,'audit_writes_performed':True,'refresh_files':refresh,'publish_files':publish,
            'conflicts':conflicts,'blocked_paths':sorted(blocked),
            'blocked_introduction_paths':sorted({p for p,c,t in held if c=='introduction'}),'detecting_developer_id':actor,
            'other_developer_ids':sorted(peer),'recommend_other_developer_clarification':bool(peer),'messages_sent':False,
            'independent_updates_may_continue':True,'conflicts_pending':bool(held),'semantic_truth_verified':False,'semantic_completeness_verified':False,'judgment_is_agent_assertion':True,
            'remote_freshness_verified':True,'publication_verified':False,'canonical_writes_performed':False,
            'git_metadata_writes_performed':True,'refresh_applied':False}


def check(repo,proof, *, owned_lock=None):
    fields={'schema','judgment','refresh_hashes','publish_hashes','blocked_paths','blocked_components'}
    if isinstance(proof,dict) and proof.get('schema')=='agent-memory/semantic-sharing-proof-v2':fields.update({'held_causes','known_resolutions'})
    if not isinstance(proof,dict) or set(proof)!=fields or proof['schema'] not in {'agent-memory/semantic-sharing-proof-v1','agent-memory/semantic-sharing-proof-v2'}:
        raise ContractError('Use the complete actual semantic proof')
    value=review(repo,proof['judgment'],owned_lock=owned_lock)
    if encoded(value['proof'])!=encoded(proof):raise ContractError('Semantic proof does not describe current reviewed sources and outputs')
    return {**value,'operation':'check','source_observation_current':True}


def read_input(path, *, version2=False):
    raw=limits.read_bytes(path,limits.MAX_PRIVATE if version2 else LIMIT)
    value=load(raw)
    # Exact schema dispatch keeps legacy proof limits and unknown formats fail
    # in their actual consumers; a larger envelope never changes v1 semantics.
    candidate=value.get('proof',value) if isinstance(value,dict) else value
    if isinstance(candidate,dict) and candidate.get('schema') in {'agent-memory/semantic-sharing-proof-v2','agent-memory/semantic-sharing-judgment-v2','agent-memory/semantic-sharing-request-v2','agent-memory/semantic-sharing-inspection-v2'}:
        return value
    if version2:raise ContractError('Explicit v2 reader requires an actual v2 source-bound format')
    return value


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='command',required=True)
    for name in ('inspect','review','check'):
        p=sub.add_parser(name);p.add_argument('--repo',required=True)
        if name=='inspect':p.add_argument('--base')
        else:p.add_argument('--input',required=True)
        if name=='check':p.add_argument('--v2',action='store_true')
        p.add_argument('--output')
    p=sub.add_parser('inspect-v2');p.add_argument('--repo',required=True);p.add_argument('--input');p.add_argument('--output',required=True)
    p=sub.add_parser('page-v2');p.add_argument('--repo',required=True);p.add_argument('--input',required=True);p.add_argument('--page',type=int,required=True)
    p=sub.add_parser('review-v2');p.add_argument('--repo',required=True);p.add_argument('--input',required=True);p.add_argument('--output',required=True)
    p=sub.add_parser('recover-holds');p.add_argument('--repo',required=True);p.add_argument('--project-id',required=True)
    args=parser.parse_args(argv)
    try:
        if args.command=='recover-holds':
            from . import sharing_holds
            # Recovery operates only on this explicitly selected project, never on authority refs or canonical memory.
            value=sharing_holds.recover(args.repo,args.project_id)
        elif args.command in {'inspect-v2','page-v2','review-v2'}:
            from . import sharing_frontier as frontier
            if args.command=='inspect-v2':value=frontier.inspect(args.repo,read_input(args.input,version2=True) if args.input else None)
            elif args.command=='page-v2':value=frontier.page(args.repo,read_input(args.input,version2=True),args.page)
            else:value=review(args.repo,read_input(args.input,version2=True))
        elif args.command=='inspect':value=inspect(args.repo,args.base)
        elif args.command=='review':value=review(args.repo,read_input(args.input))
        else:value=check(args.repo,read_input(args.input,version2=args.v2))
        if getattr(args,'output',None):value=limits.write_private(args.output,value,args.repo)
        elif isinstance(value,dict) and value.get('proof',{}).get('schema')=='agent-memory/semantic-sharing-proof-v2':
            # Complete candidate banks can retain quarantined local originals.
            # They belong only in explicitly selected private artifacts.
            value={k:v for k,v in value.items() if k not in {'refresh_files','publish_files'}}
        print(json.dumps(value,indent=2));return 0
    except (ContractError,OSError,ValueError,TypeError,KeyError,AttributeError,UnicodeError,RecursionError) as exc:
        from .sharing_holds import HoldFailure
        from .sharing_recovery import FetchFailure
        fetch_failed=isinstance(exc,FetchFailure)
        known=isinstance(exc,HoldFailure) or fetch_failed
        print(json.dumps({'schema':'agent-memory/semantic-sharing-result-v1','operation':args.command,
            'publication_verified':False,'canonical_writes_performed':False,'remote_freshness_verified':False,
            'failure_component':'authority-fetch' if fetch_failed else 'known-dispute-state' if known else args.command,
            'failure_code':exc.code if known else 'semantic-review-unverified',
            'error':str(exc) if known else 'Incomplete, changed or unsafe semantic review; retain local work and inspect actual sources',
            'repair':exc.repair if known else 'Preserve the bank, original request and pending evidence. Run am doctor for this repository, inspect the failed component, then repeat source inspection and review; do not substitute an empty bank or a compatibility judgment.'}));return 1


if __name__=='__main__':sys.exit(main())
