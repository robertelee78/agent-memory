"""Keep a detected dispute attached to its logical claims as source variants move.

A compatible label or new file hash does not resolve an earlier disagreement.
Explicit resolutions remain source-bound, fallible agent assertions.
"""
from .frontmatter import ContractError


def current_rows(rows):
    return {key:row for key,row in rows.items() if set(row['sides']) & {'local','landed'}}


def source_index(rows):
    logical={};identities={};corrections={}
    for key,row in current_rows(rows).items():
        logical.setdefault(tuple(row[n] for n in ('path','component','target_path')),set()).add(key)
        meta=row.get('metadata') or row.get('witness_metadata') or {}
        if 'id' in meta:identities.setdefault(meta['id'],set()).add(key)
        for name in ('corrects','supersedes'):
            for ref in meta.get(name,[]):corrections.setdefault((ref['id'],ref['version']),set()).add(key)
    return logical,identities,corrections


def successors(reference,index):
    logical,identities,corrections=index
    record=reference.get('record')
    if record is not None:
        return sorted(identities.get(record['id'],set())|corrections.get((record['id'],record['version']),set()))
    return sorted(logical.get(tuple(reference[n] for n in ('path','component','target_path')),set()))


def describe(record,rows,*,index=None):
    from .sharing_holds import cause_id
    index=index if index is not None else source_index(rows)
    endpoints={key:successors(reference,index) for key,reference in record['references'].items()}
    return {'id':cause_id(record['cause']),'cause':record['cause'],'references':record['references'],
        'endpoint_sources':endpoints,'sources':sorted(set().union(*(set(v) for v in endpoints.values()))),
        'missing_sources':sorted(key for key,value in endpoints.items() if not value),
        'resolution':record.get('resolution'),'resolved':False}


def validate_resolution(value,known,rows,qualified,*,stored=False):
    from . import semantic_sharing as semantic, security
    required={'basis','evidence','reason'}
    if stored:required|={'endpoint_sources','judgment_sha256','proof_sha256'}
    if (not isinstance(value,dict) or set(value)!=required
            or value['basis'] not in {'operator-clarification','qualified-correction','terminal-deletion','disjoint-conditions'}
            or not isinstance(value['evidence'],list) or not 1<=len(value['evidence'])<=64
            or len(set(value['evidence']))!=len(value['evidence'])):
        raise ContractError('A known dispute needs an explicit bounded stronger-evidence resolution')
    semantic.safe_text(value['reason'])
    if stored and value['endpoint_sources']!=known['endpoint_sources']:
        raise ContractError('Resolved claims changed; renew the source-bound clarification')
    actual=current_rows(rows)
    if any(key not in actual for key in value['evidence']):
        raise ContractError('Resolution evidence must be present at its exact current semantic source version')
    evidence=[actual[key] for key in value['evidence']]
    for row in evidence:
        if row.get('content_withheld') or row.get('terminal_body_withheld'):
            raise ContractError('Withheld assertions cannot silently resolve a known dispute')
        if security.scan(row['body'])['reasons']:
            raise ContractError('Unsafe resolution evidence needs source clarification')
    references=[ref['record'] for ref in known['references'].values() if ref.get('record') is not None]
    if value['basis']=='terminal-deletion':
        if not any(row['format']=='tombstone' and (row.get('witness_metadata') or {}).get('id') in {r['id'] for r in references} for row in evidence):
            raise ContractError('Terminal resolution needs the actual retained deletion witness')
    elif value['basis']=='disjoint-conditions':
        groups=list(known['endpoint_sources'].values())
        if known['missing_sources'] or len(groups)!=2 or any(not semantic.disjoint(rows[a],rows[b]) for a in groups[0] for b in groups[1]):
            raise ContractError('Scope resolution needs actual known non-overlap for every current claim variant')
        if not set(value['evidence']).issubset(set(known['sources'])):
            raise ContractError('Scope resolution must name the actual changed conditional claims')
    else:
        if any(row['key'] not in qualified or row.get('metadata') is None for row in evidence):
            raise ContractError('Resolution evidence must be safe qualified current learning, not a newer unqualified label')
        if value['basis']=='operator-clarification':
            if not any(row['metadata']['kind']=='user-feedback' for row in evidence):
                raise ContractError('Operator clarification needs the inspected attributed feedback record')
        elif not any(reference in row['metadata'].get('corrects',[])+row['metadata'].get('supersedes',[]) for reference in references for row in evidence):
            raise ContractError('Correction resolution must name an actual stronger exact-version correction')
    return {**{k:value[k] for k in ('basis','evidence','reason')},'endpoint_sources':known['endpoint_sources']}


def qualified_current(rows,values,root,project_id):
    from . import semantic_sharing as semantic
    paths={side:semantic.qualified_sources(values[side],root,project_id) for side in ('local','landed')}
    return {key for key,row in rows.items() if any(side in row['sides'] and row['path'] in paths[side] for side in paths)}


def cached(known,rows,qualified):
    if known['resolution'] is None:return False
    try:validate_resolution(known['resolution'],known,rows,qualified,stored=True)
    except (ContractError,KeyError,TypeError,ValueError):return False
    return True
