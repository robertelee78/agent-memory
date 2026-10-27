"""Source-bound search/read and explicit agent judgment before durable learning."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import uuid

from . import bank_contract, bank_writes as writes, records, retrieval, scope_binding, security
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError, write

REQUEST = {'project_id', 'candidate', 'searches', 'read_paths'}
CLASSES = {'equivalent', 'refinement', 'scope-correction', 'distinct-conditions', 'distinct-knowledge',
           'contradiction', 'related-context', 'uncertain'}
LIMIT = 65536


def safe_text(value, maximum=2048):
    if (not isinstance(value, str) or not value.strip() or len(value.encode()) > maximum
            or any(ord(c) < 32 and c not in '\n\t' for c in value)):
        raise ContractError('Comparison needs bounded explicit search/reason text')
    if security.scan(value)['reasons']:
        raise ContractError('Unsafe comparison text must be reviewed without creating a usable proof')
    return value


def request_value(request):
    if (not isinstance(request, dict) or not REQUEST.issubset(request) or set(request)-REQUEST-{'discovery'} or not uuid_valid(request['project_id'])
            or not isinstance(request['candidate'], dict) or set(request['candidate']) != {'metadata', 'body'}
            or not isinstance(request['searches'], list) or not 2 <= len(request['searches']) <= 8
            or not isinstance(request['read_paths'], list) or len(request['read_paths']) > 64
            or len(encoded(request)) > LIMIT):
        raise ContractError('Compare one explicit record proposal with bounded alternative searches and read paths')
    kind, metadata = records.validate(request['candidate']['metadata'], request['candidate']['body'])
    if kind != 'standalone-record' or metadata['project-id'] != request['project_id']:
        raise ContractError('Candidate must be a same-project standalone proposal, never a tombstone or event')
    security.ensure_safe({'candidate.md': write(metadata) + request['candidate']['body']})
    searches = [' '.join(safe_text(value, 256).casefold().split()) for value in request['searches']]
    if len(set(searches)) != len(searches):
        raise ContractError('Search the concept and genuinely different wording, not repeated query strings')
    for path in request['read_paths']:
        bank_contract.path_value(path)
    if len(set(request['read_paths'])) != len(request['read_paths']):
        raise ContractError('Explicit plausible source paths must be unique')
    return metadata


def inspect(repo, request, *, owned_lock=None):
    metadata = request_value(request)
    root, bank = writes.bank_path(repo)
    context = scope_binding.require(root, request['project_id'], owned_lock=owned_lock)
    checked, observed = writes.snapshot(root, owned_lock=owned_lock)
    discovered_paths=set()
    if 'discovery' in request:
        from . import discovery
        discovered_paths={row['path'] for row in request['discovery'].get('sources',[])}
        discovery.check_trace(root,request['discovery'],checked,observed,context,
            intent='comparison',selected_paths=request['read_paths'],owned_lock=owned_lock)
    if checked['project_id'] != request['project_id']:
        raise ContractError('Bank identity changed during comparison')
    deleted = {entry['id'] for entry in checked['entries'] if entry['format'] == 'tombstone'}
    referenced = {metadata['id']} | {ref['id'] for key in ('derived-from', 'corrects', 'supersedes') for ref in metadata[key]}
    if deleted & referenced:
        raise ContractError('Terminal deleted identity cannot be reused or restored through a comparison proposal')
    entries = {entry['path']: entry for entry in checked['entries']}
    if any(path not in entries or entries[path]['format'] not in {'standalone-record', 'context-prose'}
           for path in request['read_paths']):
        raise ContractError('Read each plausible match from actual canonical learning, not an absent path or event')
    capture_owners, capture_identity_review_paths = set(), set()
    for entry in checked['entries']:
        path = entry['path']
        if not path.startswith('history/sources/'):continue
        if entry['format'] != 'standalone-record':
            raise ContractError('Original capture provenance is incomplete; inspect before claiming absence')
        with (bank / path).open('rb') as stream:raw = stream.read(16385)
        if len(raw) > 16384 or digest(raw) != observed[path]:
            raise ContractError('Capture provenance changed or exceeds its reader')
        _, source, _ = records.parse(raw.decode('utf-8'))
        captured_id = records.source_capture_id(path, source)
        if path.startswith('history/sources/' + metadata['id'] + '/') and captured_id != metadata['id']:
            raise ContractError('Original capture identity is unresolved; inspect before claiming absence')
        if captured_id == metadata['id']:
            capture_owners.add(source['id'])
            capture_owners.update(ref['id'] for ref in source.get('superseded-by', []))
        elif captured_id is None and any(ref['id'] == metadata['id'] for ref in source['derived-from']):
            # Older admitted banks use flat source paths. Their references do
            # not identify which UUID was the consumed capture. A lexical miss
            # after compaction cannot establish novelty or license re-admission.
            # Preserve every source/ref and require actual role clarification.
            capture_identity_review_paths.add(path)
    router = (bank / 'MEMORY.md').read_bytes()
    if digest(router) != observed['MEMORY.md']:
        raise ContractError('Router changed during comparison')
    security.ensure_safe({'MEMORY.md': router.decode('utf-8')})
    matches, used = [], 0
    for path, entry in entries.items():
        if entry['format'] not in {'standalone-record', 'context-prose'}:
            continue
        with (bank / path).open('rb') as stream:
            raw = stream.read(16385)
        if len(raw) > 16384:
            # A truncated or skipped source cannot establish novelty.
            raise ContractError('Existing learning exceeds the comparison reader; inspect/split it before claiming absence')
        if digest(raw) != observed[path]:
            raise ContractError('Memory changed during plausible-match search')
        text = raw.decode('utf-8')
        existing, body = None, text
        if entry['format'] == 'standalone-record':
            _, existing, body = records.parse(text)
        query_hits = [query for query in request['searches'] if retrieval.match(
            {'ids': [], 'paths': [], 'query': query}, {'path': path, 'body': body}) is not None]
        same_id = existing is not None and existing['id'] in {metadata['id'], *capture_owners}
        if not query_hits and path not in request['read_paths'] and path not in discovered_paths and path not in capture_identity_review_paths and not same_id:
            continue
        security.ensure_safe({path: text + '\n' + json.dumps(entry['scope_chain'], ensure_ascii=False)})
        if existing is not None and existing.get('visibility') == 'hidden':
            raise ContractError('A plausible hidden match needs explicit source review; do not create a duplicate around it')
        row = {'path': path, 'sha256': observed[path], 'format': entry['format'], 'body': body,
               'metadata': existing, 'scope_chain': entry['scope_chain'], 'lifecycle': entry['lifecycle'],
               'matched_searches': query_hits, 'selection_class': 'comparison-only-not-current-guidance',
               'source_assertions_verified': False}
        used += len(encoded(row))
        if len(matches) >= 64 or used > LIMIT:
            raise ContractError('Plausible matches exceed a complete bounded review; refine and inspect before writing')
        matches.append(row)
    matches.sort(key=lambda row: row['path'])
    scope_binding.require(root, checked['project_id'], expected=context, owned_lock=owned_lock)
    if writes.snapshot(root, owned_lock=owned_lock)[1] != observed:
        raise ContractError('Canonical memory changed during comparison; repeat searches and read new matches')
    observation = {'schema': 'agent-memory/comparison-observation-v1', 'project_id': checked['project_id'],
        'request_sha256': digest(encoded(request)),
        'bank_observation_sha256': digest(encoded({'project_id': checked['project_id'], 'sources': observed})),
        'router_sha256': observed['MEMORY.md'],
        'matches': [{'path': row['path'], 'sha256': row['sha256']} for row in matches]}
    ids = sorted({row['metadata']['id'] for row in matches if row['metadata'] is not None})
    audit = security.audit(root, str(uuid.uuid4()), 'retrieval', 'checked', context, ['external'], record_ids=ids)
    scope_binding.require(root, checked['project_id'], expected=context, owned_lock=owned_lock)
    if writes.snapshot(root, owned_lock=owned_lock)[1] != observed:
        raise ContractError('Memory changed before comparison readback; do not use an earlier novelty judgment')
    return {'schema': 'agent-memory/comparison-result-v1', 'operation': 'inspect', 'observation': observation,
        'capture_identity_owners': sorted(capture_owners),
        'capture_identity_review_paths': sorted(capture_identity_review_paths),
        'router': router.decode('utf-8'), 'matches': matches, 'candidate': request['candidate'],
        'detecting_developer_id': context['developer_id'], 'audit': audit, 'comparison_read_complete': True,
        'semantic_completeness_verified': False, 'source_assertions_verified': False,
        'main_freshness_verified': False, 'publication_verified': False, 'admission_verified': False,
        'canonical_writes_performed': False, 'audit_writes_performed': True,
        'validation_scope': 'local-source-bound-search-and-agent-judgment-only'}


def judge(inspection, request, judgment):
    if (not isinstance(judgment, dict) or set(judgment) != {'observation', 'decisions', 'rationale'}
            or judgment['observation'] != inspection['observation'] or not isinstance(judgment['decisions'], list)):
        raise ContractError('Judgment must name the actual read observation; changed bank requires a new comparison')
    safe_text(judgment['rationale'])
    decisions = judgment['decisions']
    if any(not isinstance(d, dict) or set(d) != {'path', 'classification', 'rationale'}
           or not isinstance(d['path'], str) or d['classification'] not in CLASSES for d in decisions):
        raise ContractError('Every plausible match needs an explicit classified semantic judgment')
    paths = [d['path'] for d in decisions]
    if len(paths) != len(set(paths)) or set(paths) != {row['path'] for row in inspection['matches']}:
        raise ContractError('Do not omit, substitute or duplicate a plausible source in the comparison judgment')
    by_path = {row['path']: row for row in inspection['matches']}
    candidate = request['candidate']['metadata']
    targets, conflicts, uncertain, inactive, scope_corrections, other_developers = [], [], [], [], [], set()
    for decision in decisions:
        safe_text(decision['rationale'])
        row, classification = by_path[decision['path']], decision['classification']
        existing = row['metadata']
        if (row['path'] in inspection['capture_identity_review_paths'] and existing
                and existing['origin']['developer-id'] != inspection['detecting_developer_id']):
            other_developers.add(existing['origin']['developer-id'])
        if classification == 'contradiction':
            conflicts.append(row['path'])
            if existing and existing['origin']['developer-id'] != inspection['detecting_developer_id']:
                other_developers.add(existing['origin']['developer-id'])
        if classification == 'uncertain':
            uncertain.append(row['path'])
        if existing:
            same_conditions = all(existing[key] == candidate[key] for key in ('scope', 'conditions', 'applicability'))
            if classification in {'equivalent', 'refinement'} and (not same_conditions or existing['kind'] != candidate['kind']):
                raise ContractError('Different conditions or types must stay distinct rather than be silently combined')
            if classification == 'distinct-conditions' and same_conditions:
                raise ContractError('A condition distinction needs actual different conditions, not a duplicate label')
        if classification == 'scope-correction':
            if (existing is None or existing['id'] != candidate['id'] or existing['kind'] != candidate['kind']
                    or existing['scope'] != candidate['scope'] or existing['applicability'] != candidate['applicability']
                    or any(candidate['conditions'].get(key, object()) != value for key, value in existing['conditions'].items())
                    or not set(existing['conditions']) < set(candidate['conditions'])
                    or any(value is None for value in candidate['conditions'].values())
                    or row['body'] != request['candidate']['body']):
                raise ContractError('Scope correction retains the exact same learning body and known conditions while adding a narrower constraint')
            scope_corrections.append(row['path'])
        if classification in {'equivalent', 'refinement', 'scope-correction'}:
            targets.append({'path': row['path'], 'sha256': row['sha256'],
                'id': existing['id'] if existing else None, 'version': existing['version'] if existing else None,
                'required_preserved_metadata': existing, 'scope_chain': row['scope_chain']})
            if row['lifecycle'] != 'active':
                inactive.append(row['path'])
    if conflicts:
        disposition = 'clarification-required'
    elif inspection['capture_identity_review_paths']:
        disposition = 'provenance-identity-review-required'
    elif uncertain:
        disposition = 'review-required'
    elif scope_corrections:
        disposition = 'governance-required'
    elif inactive:
        disposition = 'lifecycle-review-required'
    elif targets:
        disposition = 'update-existing' if len(targets) == 1 else 'consolidate-existing'
    else:
        disposition = 'create-distinct'
    if disposition == 'create-distinct' and (inspection['capture_identity_owners'] or any(
            row['metadata'] and row['metadata']['id'] == candidate['id'] for row in inspection['matches'])):
        raise ContractError('Distinct new knowledge must use a separate stable identity, not reuse an existing one')
    proof = {'schema': 'agent-memory/comparison-proof-v1', 'judgment': judgment,
             'disposition': disposition, 'targets': targets}
    if len(encoded(proof)) > LIMIT:
        raise ContractError('Complete comparison proof exceeds its reader budget; review smaller independent learning units')
    return {**inspection, 'operation': 'review', 'proof': proof, 'proof_sha256': digest(encoded(proof)),
        'disposition': disposition, 'same_learning_requires_existing_identity': bool(targets),
        'disputed_paths': conflicts, 'other_developer_ids': sorted(other_developers),
        'recommend_other_developer_clarification': bool(other_developers), 'messages_sent': False,
        'judgment_is_agent_assertion': True, 'semantic_completeness_verified': False,
        'admission_verified': False, 'publication_verified': False}


def review(repo, request, judgment, *, owned_lock=None):
    return judge(inspect(repo, request, owned_lock=owned_lock), request, judgment)


def check(repo, request, proof, *, owned_lock=None):
    if not isinstance(proof, dict) or set(proof) != {'schema', 'judgment', 'disposition', 'targets'} or proof['schema'] != 'agent-memory/comparison-proof-v1':
        raise ContractError('Revalidation requires the exact original comparison proof')
    result = review(repo, request, proof['judgment'], owned_lock=owned_lock)
    if encoded(result['proof']) != encoded(proof):
        raise ContractError('Comparison proof was changed or does not describe the current canonical sources')
    return {**result, 'operation': 'check', 'source_observation_current': True}


def read_input(path):
    with Path(path).open('rb') as stream:
        raw = stream.read(LIMIT + 1)
    if len(raw) > LIMIT:
        raise ContractError('Comparison input exceeds its bounded reader')
    return load(raw)


def cli_judgment(value):
    """Accept the inspect envelope as shorthand; proofs still use the exact observation."""
    if isinstance(value, dict) and set(value) == {'observation', 'decisions', 'rationale'}:
        observed = value['observation']
        if (isinstance(observed, dict) and observed.get('schema') == 'agent-memory/comparison-result-v1'
                and observed.get('operation') == 'inspect' and observed.get('comparison_read_complete') is True):
            return {**value, 'observation': observed.get('observation')}
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('inspect', 'review', 'check'):
        sub = commands.add_parser(name)
        sub.add_argument('--repo', required=True)
        sub.add_argument('--request', required=True)
        if name == 'review': sub.add_argument('--judgment', required=True)
        if name == 'check': sub.add_argument('--proof', required=True)
    args = parser.parse_args(argv)
    try:
        request = read_input(args.request)
        value = inspect(args.repo, request) if args.command == 'inspect' else (
            review(args.repo, request, cli_judgment(read_input(args.judgment))) if args.command == 'review'
            else check(args.repo, request, read_input(args.proof)))
        print(json.dumps(value, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError, RecursionError):
        print(json.dumps({'schema': 'agent-memory/comparison-result-v1', 'verified': False,
            'canonical_writes_performed': False, 'comparison_read_complete': False,
            'error': 'Incomplete, unsafe, unavailable or changed comparison; preserve sources and do not infer novelty'}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
