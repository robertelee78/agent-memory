"""Explicit changed-learning selection; agent judgments remain fallible assertions."""
from __future__ import annotations
import copy
import itertools
from pathlib import Path

from . import bank_contract as bank, bank_layout as layout, records, retrieval, security
from . import sharing_limits as limits
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError, read

REQUEST = 'agent-memory/semantic-sharing-request-v2'
INSPECTION = 'agent-memory/semantic-sharing-inspection-v2'
JUDGMENT = 'agent-memory/semantic-sharing-judgment-v2'
PROOF = 'agent-memory/semantic-sharing-proof-v2'
MAX_EDGES=262144


def add_edges(edges,values):
    for edge in values:
        if edge not in edges and len(edges)>=MAX_EDGES:
            raise ContractError('Complete review exceeds 262,144 interactions; preserve the frontier without truncation')
        edges.add(edge)


def semantic_metadata(value):
    return {k:v for k,v in value.items() if k not in {'updated','recorded-at','router-updated'}}


def components(values, checked):
    """Separate stable logical identity from exact semantic variant and side hash."""
    rows = {}
    for side in ('base','local','landed'):
        files=values[side]
        if not files: continue
        containers={};framing={'MEMORY.md':[]}
        for path in ['MEMORY.md',*[e['path'] for e in checked[side]['entries'] if e['format']=='topic-index']]:
            metadata,body=read(files[path],allowed=set(bank.SPEC['properties'])|set(layout.SPEC['properties']))
            routes=bank.parse_router(files[path])[1] if path=='MEMORY.md' else layout.parse_index(path,metadata,body)
            intro=body.split('\n## Routes\n',1)[0]
            containers[path]=(metadata,routes,intro)
        pending=['MEMORY.md']
        while pending:
            parent=pending.pop();metadata,routes,intro=containers[parent]
            for route in routes:
                child=route['path']
                chain=[*framing[parent],{'container_path':parent,'metadata':semantic_metadata(metadata),'introduction':intro,'route':route}]
                if child in framing and framing[child]!=chain: raise ContractError('Ambiguous effective semantic framing')
                framing[child]=chain
                if child in containers: pending.append(child)
        def add(entry,body,metadata=None,*,component='body',target=None,container=None,witness=None):
            path=entry['path'];logical={'path':path,'component':component,'target_path':target}
            effective=framing[path]
            if component=='route-prose': effective=[*effective,{'route':container['route']}]
            meaning={'logical':logical,'body':body,'metadata':semantic_metadata(metadata) if metadata else None,
                     'framing':effective,'lifecycle':entry['lifecycle'],
                     'component_metadata':semantic_metadata(container['metadata']) if container else None,
                     'witness':witness}
            key=digest(encoded(meaning));sha=digest(files[path].encode())
            if key in rows:
                row=rows[key];row['sides'].append(side);row['source_sha256_by_side'][side]=sha
                row['scope_chains'][side]=entry['scope_chain'];row['container_variants'][side]=container
                row['observation_times_by_side'][side]={k:v for k,v in (metadata or {}).items() if k in {'updated','recorded-at'}}
                if side=='local' or 'local' not in row['sides']:
                    row.update(sha256=sha,metadata=metadata,scope_chain=entry['scope_chain'],container=container)
            else:
                rows[key]={'key':key,'logical_key':digest(encoded(logical)),**logical,'sha256':sha,
                    'source_sha256_by_side':{side:sha},'sides':[side],'metadata':metadata,'body':body,
                    'scope_chain':entry['scope_chain'],'scope_chains':{side:entry['scope_chain']},
                    'effective_framing':effective,'lifecycle':entry['lifecycle'],'format':entry['format'],
                    'container':container,'container_variants':{side:container},'witness_metadata':witness}
                rows[key]['observation_times_by_side']={side:{k:v for k,v in (metadata or {}).items() if k in {'updated','recorded-at'}}}
        entries=[{'path':'MEMORY.md','format':'root-router','scope_chain':['repository'],'lifecycle':'active'},*checked[side]['entries']]
        for entry in entries:
            path=entry['path'];text=files[path]
            if entry['format']=='standalone-record':
                _,meta,body=records.parse(text);add(entry,body,meta)
            elif entry['format']=='context-prose':add(entry,text)
            elif path in containers:
                meta,routes,intro=containers[path];context={'metadata':meta,'routes':routes}
                if path=='MEMORY.md':context['omitted']=bank.parse_router(text)[2]
                add(entry,intro,component='introduction',container=context)
                for route in routes:
                    routed={**entry,'scope_chain':[*entry['scope_chain'],route['scope']]}
                    add(routed,'Purpose: '+route['purpose']+'\nRead when: '+route['read-when']+'\n',
                        component='route-prose',target=route['path'],container={'metadata':meta,'route':route})
                for target,reason in context.get('omitted',{}).items():
                    add(entry,reason,component='omission',target=target,container={'metadata':meta,'context_path':target})
            elif entry['format'] in {'tombstone','optional-history-event'}:
                _,meta,body=records.parse(text);add(entry,body,witness=meta)
    return rows


def changed_components(rows,audit_paths):
    logical={}
    for key,row in rows.items():logical.setdefault(row['logical_key'],[]).append(key)
    changed=set()
    for keys in logical.values():
        versions={side:{key for key in keys if side in rows[key]['sides']} for side in ('base','local','landed')}
        if versions['base']!=versions['local'] or versions['base']!=versions['landed'] or any(rows[k]['path'] in audit_paths for k in keys):
            changed.update(keys)
    return changed


def identities(row):
    meta=row['metadata'] or row['witness_metadata'] or {}
    result={meta['id']} if 'id' in meta else set()
    for field in ('corrects','supersedes','superseded-by','derived-from'):
        result.update(ref['id'] for ref in meta.get(field,[]))
    if 'record' in meta:result.add(meta['record']['id'])
    parts=row['path'].split('/')
    if len(parts)==4 and parts[:2]==['history','sources'] and uuid_valid(parts[2]):result.add(parts[2])
    return result


def request_value(request):
    if request is None: return {'schema':REQUEST,'base':None,'selections':{},'audit_paths':[],'dependencies':{},'consolidation_sources':[]}
    if not isinstance(request,dict) or set(request)!={'schema','base','selections','audit_paths','dependencies','consolidation_sources'} or request['schema']!=REQUEST:
        raise ContractError('Use an explicit complete v2 selection request')
    if not isinstance(request['selections'],dict) or not isinstance(request['audit_paths'],list) or len(set(request['audit_paths']))!=len(request['audit_paths']):
        raise ContractError('Review requires unique actual audit paths and per-change selectors')
    for path in request['audit_paths']:
        if path!='MEMORY.md':bank.path_value(path)
    if not isinstance(request['dependencies'],dict) or not isinstance(request['consolidation_sources'],list) or len(request['consolidation_sources'])>8:
        raise ContractError('Declare actual dependency paths and consolidation source groups')
    limits.private_bytes(request)
    return request


def selected(rows,changed,request):
    from . import semantic_sharing as semantic
    if set(request['selections'])-changed:raise ContractError('Selectors cannot substitute an unchanged or absent change reference')
    logical={};identity={}
    for key,row in rows.items():
        logical.setdefault(row['logical_key'],set()).add(key)
        for ref in identities(row):identity.setdefault(ref,set()).add(key)
    selected_keys=set(changed);edges=set();entries={};pending=[]
    for key in sorted(changed):
        row=rows[key];selection=request['selections'].get(key)
        mandatory=set(logical[row['logical_key']])
        for ref in identities(row):mandatory.update(identity[ref])
        matches=set(mandatory)
        if selection is None:
            pending.append(key);entries[key]={'complete':False,'matched':sorted(matches)}
        else:
            if (not isinstance(selection,dict) or set(selection)!={'searches','ids','paths','complete','uncertain'}
                    or type(selection['complete']) is not bool or type(selection['uncertain']) is not bool
                    or not isinstance(selection['searches'],list) or not 2<=len(selection['searches'])<=8
                    or len(encoded(selection))>limits.MAX_PAGE):
                raise ContractError('Each changed meaning needs bounded alternative searches and an explicit completed/uncertain selection assertion')
            searches=[]
            for search in selection['searches']:
                semantic.safe_text(search,256);searches.append(' '.join(search.casefold().split()))
            if len(set(searches))!=len(searches):raise ContractError('Use distinct plausible-match wordings')
            for name in ('ids','paths'):
                if not isinstance(selection[name],list) or len(selection[name])>64 or len(set(selection[name]))!=len(selection[name]):
                    raise ContractError('Use bounded unique exact source selectors')
            if any(not uuid_valid(ref) for ref in selection['ids']):raise ContractError('Select actual stable record identities')
            for path in selection['paths']:
                if path!='MEMORY.md':bank.path_value(path)
            for other,value in rows.items():
                if value['path'] in selection['paths'] or identities(value)&set(selection['ids']) or any(retrieval.match({'ids':[],'paths':[],'query':search},{'path':value['path'],'body':value['body']+'\n'+str(value['effective_framing'])}) is not None for search in searches):matches.add(other)
            entries[key]={**selection,'matched':sorted(matches)}
            if not selection['complete']:pending.append(key)
        selected_keys.update(matches)
        add_edges(edges,(tuple(sorted((key,other))) for other in matches if other!=key))
    actual_paths={r['path'] for r in rows.values()}
    for path,required in request['dependencies'].items():
        if path not in actual_paths or not isinstance(required,list) or len(set(required))!=len(required) or any(v not in actual_paths for v in required):
            raise ContractError('Dependencies require actual complete unique source paths')
        left=[k for k,r in rows.items() if r['path']==path]
        right=[k for k,r in rows.items() if r['path'] in required]
        selected_keys.update(left+right);add_edges(edges,(tuple(sorted((a,b))) for a,b in itertools.product(left,right) if a!=b))
    for group in request['consolidation_sources']:
        if not isinstance(group,list) or not 2<=len(group)<=8 or len(set(group))!=len(group) or any(k not in rows for k in group):
            raise ContractError('Consolidation requires actual distinct complete component references')
        selected_keys.update(group);add_edges(edges,itertools.combinations(sorted(group),2))
    return {key:rows[key] for key in sorted(selected_keys)},entries,[list(p) for p in sorted(edges)],pending


def page_payloads(binding,rows,edges):
    binding_sha=digest(encoded(binding));pages=[];items=[]
    def payload(values):return {'schema':'agent-memory/semantic-sharing-page-v2','binding_sha256':binding_sha,'index':len(pages),'items':values}
    for item in [*({'source':row} for row in rows.values()),*({'edge':edge} for edge in edges)]:
        if len(encoded(payload([*items,item])))>limits.MAX_PAGE:
            if not items:raise ContractError('One complete source/framing exceeds a disclosure page; split/review explicitly without truncation')
            pages.append(payload(items));items=[]
        items.append(item)
        if len(encoded(payload(items)))>limits.MAX_PAGE:raise ContractError('One complete source exceeds its page budget')
    if items:pages.append(payload(items))
    for page in pages:security.ensure_safe({'review-page':encoded(page).decode('utf-8')})
    return pages


def build(repo,request=None,*,owned_lock=None,skip_judgment=None):
    from . import semantic_sharing as semantic
    request=copy.deepcopy(request_value(request))
    root,observation,binding,values,checked=semantic.snapshots(repo,request['base'],owned_lock=owned_lock)
    rows=components(values,checked)
    from . import sharing_holds
    current={key for key,row in rows.items() if set(row['sides'])&{'local','landed'}}
    known=[]
    actual_paths={r['path'] for r in rows.values()}
    for record in sharing_holds.load(root,observation['project_id'],skip_judgment=skip_judgment):
        cause=record['cause']
        if not set(cause['sources']).issubset(current):continue
        semantic.safe_text(cause['reason']);known.append(cause)
        request['audit_paths']=sorted(set(request['audit_paths'])|{rows[key]['path'] for key in cause['sources']})
        for path,required in record['dependencies'].items():
            if path in actual_paths and all(v in actual_paths for v in required):
                request['dependencies'][path]=sorted(set(request['dependencies'].get(path,[]))|set(required))
    if any(path not in set().union(*(set(f) for f in values.values())) for path in request['audit_paths']):raise ContractError('Audit paths must name actual observed source')
    changed=changed_components(rows,set(request['audit_paths']))
    chosen,selections,edges,pending=selected(rows,changed,request)
    for cause in known:
        if len(cause['sources'])==2:
            pair=sorted(cause['sources'])
            if pair not in edges:
                if len(edges)>=MAX_EDGES:raise ContractError('Complete known-dispute frontier exceeds its finite interaction budget')
                edges.append(pair)
    edges.sort()
    deleted={entry['id'] for side in ('local','landed') for entry in checked[side]['entries'] if entry['format']=='tombstone'}
    for key,row in list(chosen.items()):
        if row['metadata'] and row['metadata']['id'] in deleted:
            # An old branch/history object cannot make erased learning displayable.
            chosen[key]=copy.deepcopy(row);chosen[key]['body']='';chosen[key]['metadata']=None
            chosen[key]['witness_metadata']={'id':row['metadata']['id'],'version':row['metadata']['version']}
            chosen[key]['terminal_body_withheld']=True
    # Hidden plausible evidence remains a hold, never absence or a disclosed body.
    hidden=[key for key,row in chosen.items() if row['metadata'] and row['metadata'].get('visibility')=='hidden']
    for key in hidden:
        chosen[key]=copy.deepcopy(chosen[key]);chosen[key]['body']='';chosen[key]['metadata']=None
        chosen[key]['content_withheld']=True
    pages=page_payloads(binding,chosen,edges)
    manifest={'schema':INSPECTION,'request':request,'binding':binding,'inventory_digests':{side:digest(encoded(hashes)) for side,hashes in binding['source_hashes'].items()},
        'changed':sorted(changed),'sources':list(chosen.values()),'selections':selections,'required_pairs':edges,'pending_selections':pending,'withheld_sources':hidden,
        'known_causes':known,
        'pages':[{'index':page['index'],'sha256':digest(encoded(page)),'bytes':len(encoded(page)),'source_keys':[item['source']['key'] for item in page['items'] if 'source' in item],'edges':[item['edge'] for item in page['items'] if 'edge' in item]} for page in pages],
        'semantic_completeness_verified':False,'publication_verified':False}
    manifest['manifest_sha256']=digest(limits.private_bytes(manifest))
    semantic.final_sources(root,observation,binding,owned_lock=owned_lock)
    return (root,observation,binding,values,checked),chosen,manifest,pages


def inspect(repo,request=None,*,owned_lock=None):
    return build(repo,request,owned_lock=owned_lock)[2]


def page(repo,manifest,index,*,owned_lock=None):
    current=build(repo,manifest.get('request'),owned_lock=owned_lock)
    if current[2]!=manifest or type(index) is not int or not 0<=index<len(current[3]):
        raise ContractError('Missing, mixed or stale page observation; inspect actual sources again')
    return current[3][index]


def prepare_review(repo,judgment,*,owned_lock=None):
    if not isinstance(judgment,dict) or set(judgment)!={'schema','request','binding','manifest_sha256','pages','pairs','dependencies','consolidations','rationale','recorded_at'} or judgment['schema']!=JUDGMENT:
        raise ContractError('Use the complete explicit v2 source-bound judgment')
    snapshot,rows,manifest,_=build(repo,judgment['request'],owned_lock=owned_lock,skip_judgment=digest(encoded(judgment)))
    if judgment['binding']!=manifest['binding'] or judgment['manifest_sha256']!=manifest['manifest_sha256'] or manifest['pending_selections']:
        raise ContractError('Complete all actual changed-learning selections against current sources')
    if judgment['pages']!=[p['sha256'] for p in manifest['pages']]:raise ContractError('Acknowledge all exact pages once, in manifest order')
    if judgment['dependencies']!=judgment['request']['dependencies'] or not isinstance(judgment['consolidations'],list) or sorted(sorted(g['sources']) for g in judgment['consolidations'])!=sorted(sorted(g) for g in judgment['request']['consolidation_sources']):
        raise ContractError('Dependencies/consolidation must match the complete reviewed page frontier')
    return snapshot,rows,manifest


def terminal_relation(left,right):
    a=left.get('metadata') or left.get('witness_metadata') or {}
    b=right.get('metadata') or right.get('witness_metadata') or {}
    return (left['format']=='tombstone' or right['format']=='tombstone') and a.get('id') is not None and a.get('id')==b.get('id')
