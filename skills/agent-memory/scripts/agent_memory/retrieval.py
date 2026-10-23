"""Project-resolved file/ID/text recall with hard filters before ranking."""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import time
import uuid

from . import bank_contract as bank_contract
from . import bank_writes as writes
from . import lifecycle, records, scope_binding, scopes, security
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError
from .identity_git import git

FIELDS = {'project_id', 'query', 'ids', 'paths', 'context', 'kinds',
          'minimum_trust', 'limit', 'include_context'}
MAX_FILE = 16384
MAX_RESULT = 65536


def request_value(request):
    if (not isinstance(request, dict) or set(request) != FIELDS or not uuid_valid(request['project_id'])
            or not isinstance(request['query'], str) or len(request['query'].encode()) > 1024
            or any(ord(c) < 32 or ord(c) == 127 for c in request['query'])
            or type(request['limit']) is not int or not 1 <= request['limit'] <= 64
            or type(request['include_context']) is not bool
            or request['minimum_trust'] not in {'reviewed', 'verified'}):
        raise ContractError('Recall needs explicit project, bounded selectors, conditions and safe operation policy')
    for key in ('ids', 'paths', 'kinds'):
        values = request[key]
        if (not isinstance(values, list) or len(values) > 64
                or any(not isinstance(v, str) for v in values) or len(set(values)) != len(values)):
            raise ContractError('Recall selectors require bounded unique strings')
    if any(not uuid_valid(v) for v in request['ids']):
        raise ContractError('Exact record lookup requires stable random IDs')
    for path in request['paths']:
        bank_contract.path_value(path)
    if not set(request['kinds']).issubset(records.MODEL['properties']['kind']['enum']):
        raise ContractError('Recall requires known standalone memory kinds')
    scopes.conditions(request['context'])
    if request['query'].strip() and security.scan(request['query'])['reasons']:
        raise ContractError('Unsafe query must not become returned context or audit content')
    return request


def git_context(root):
    branch = git(root, 'symbolic-ref', '--quiet', '--short', 'HEAD', optional=True)
    revision = git(root, 'rev-parse', '--verify', 'HEAD', optional=True)
    return {key: value for key, value in (('branch', branch), ('revision', revision)) if value is not None}


def observed_context(root, supplied):
    known = git_context(root)
    for key in ('branch', 'revision'):
        if key in supplied and supplied[key] != known.get(key):
            raise ContractError('Current recall cannot substitute another branch/revision for the observed checkout')
    return {**supplied, **known}, known


def scope_reasons(project, entry, context):
    reasons = []
    for scope in entry['scope_chain']:
        reasons.extend(scopes.decide({'bank_project_id': project, 'project_id': project,
            'scope': scope, 'context': context})['reasons'])
    return list(dict.fromkeys(reasons))


def match(request, candidate):
    identity = candidate.get('record', {}).get('id')
    exact_id, exact_path = identity in request['ids'], candidate['path'] in request['paths']
    if request['ids'] or request['paths']:
        if not (exact_id or exact_path):
            return None
        return {'strategy': 'exact-id' if exact_id else 'exact-path', 'score': 10000 if exact_id else 9000,
                'matching_terms': []}
    terms = list(dict.fromkeys(request['query'].casefold().split()))
    text = (candidate['path'] + '\n' + candidate['body']).casefold()
    if terms and not all(term in text for term in terms):
        return None
    score = sum(min(text.count(term), 16) + (10 if term in candidate['path'].casefold() else 0) for term in terms)
    return {'strategy': 'ordinary-text' if terms else 'current-inventory', 'score': score, 'matching_terms': terms}


def retrieve(repo, request):
    started = time.perf_counter()
    request_value(request)
    root, bank = writes.bank_path(repo)
    selected_scope = scope_binding.require(root, request['project_id'])
    context, observed_git = observed_context(root, request['context'])
    checked, observed = writes.snapshot(root)
    project = checked['project_id']
    if project != request['project_id']:
        raise ContractError('Canonical project changed after scope resolution')
    now = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')
    excluded = Counter()
    pool = []
    superseders = {}
    canonical_bytes = 0
    for entry in checked['entries']:
        path, kind = entry['path'], entry['format']
        if kind not in {'standalone-record', 'context-prose'}:
            excluded['non-guidance-format'] += 1
            continue
        if entry['lifecycle'] != 'active':
            excluded['active'] += 1
            continue
        if kind == 'context-prose' and not request['include_context']:
            excluded['orientation-not-requested'] += 1
            continue
        reasons = scope_reasons(project, entry, context)
        if reasons:
            excluded.update(reasons)
            continue
        with (bank / path).open('rb') as stream:
            raw = stream.read(MAX_FILE + 1)
        canonical_bytes += len(raw)
        if len(raw) > MAX_FILE:
            if kind == 'standalone-record':
                # An unreadable active record may invalidate a smaller record.
                # Do not turn an incomplete invalidation inventory into absence.
                raise ContractError('Active record exceeds safe inspection bounds; resolve its metadata before recall')
            excluded['content-limit'] += 1
            continue
        if digest(raw) != observed[path]:
            raise ContractError('Canonical memory changed while reading candidates')
        text = raw.decode('utf-8')
        metadata = None
        if kind == 'standalone-record':
            _, metadata, body = records.parse(text)
            applicable = records.applicability(metadata, {'bank_project_id': project, 'context': context})
            selection = lifecycle.current(metadata, {'authorized-scope': True, 'applicable': applicable['applicable'],
                'now': now, 'minimum-trust': request['minimum_trust'], 'active-superseders': []})
            # Type, visibility and query do not make a qualified active
            # supersession disappear. Resolve replacements before those filters.
            required = ('authorized-scope', 'applicable', 'active', 'valid-time', 'sufficient-trust')
            if not all(selection['gates'][key] for key in required):
                excluded.update(selection['reasons'])
                excluded.update(applicable['reasons'])
                continue
            # A canonical replacement relation can invalidate an older version
            # even when the replacement body is unsafe to display. Quarantining
            # that body must not silently reactivate the obsolete assertion.
            for reference in metadata['supersedes'] + metadata['corrects']:
                superseders.setdefault((reference['id'], reference['version']), []).append(metadata['id'] + '/' + str(metadata['version']))
        else:
            body, selection = text, None
        try:
            inspected = security.scan(text + '\n' + json.dumps({'path': path, 'scope_chain': entry['scope_chain']}, ensure_ascii=False))
        except (ContractError, UnicodeError):
            excluded['security-content-limit'] += 1
            continue
        if inspected['reasons']:
            excluded.update('security:' + reason for reason in inspected['reasons'])
            continue
        candidate = {'path': path, 'format': kind, 'body': body,
            'source_sha256': observed[path], 'scope_chain': entry['scope_chain'],
            'source_class': 'external', 'untrusted_data': True, 'source_assertions_verified': False,
            'admission_verified': False, 'procedural_guidance': False}
        if metadata is not None:
            candidate.update(record=metadata, selection_class='current-record', selection=selection)
        else:
            candidate.update(selection_class='orientation-only', trust=None,
                explanation='Active applicable context file; independently qualify its statements before use')
        pool.append(candidate)
    ranked = []
    for candidate in pool:
        metadata = candidate.get('record')
        if metadata is not None:
            replacing = sorted(set(superseders.get((metadata['id'], metadata['version']), [])))
            selection = lifecycle.current(metadata, {'authorized-scope': True, 'applicable': True,
                'now': now, 'minimum-trust': request['minimum_trust'], 'active-superseders': replacing})
            if not selection['current']:
                excluded.update(selection['reasons'])
                continue
            if metadata['kind'] not in request['kinds']:
                excluded['memory-type'] += 1
                continue
            candidate['selection'] = selection
        matching = match(request, candidate)
        if matching is None:
            excluded['selector-or-text-mismatch'] += 1
            continue
        candidate['match'] = matching
        ranked.append(candidate)
    ranked.sort(key=lambda item: (-item['match']['score'], item['path']))
    results, used = [], 0
    for candidate in ranked:
        size = len(encoded(candidate))
        if len(results) >= request['limit'] or used + size > MAX_RESULT:
            excluded['result-budget'] += 1
            continue
        results.append(candidate)
        used += size
    scope_binding.require(root, project, expected=selected_scope)
    record_ids = sorted({item['record']['id'] for item in results if 'record' in item})
    audit = security.audit(root, str(uuid.uuid4()), 'retrieval', 'checked', selected_scope,
                           ['external'], record_ids=record_ids)
    if writes.snapshot(root)[1] != observed or git_context(root) != observed_git:
        raise ContractError('Bank or checkout context changed during recall; reread before use')
    scope_binding.require(root, project, expected=selected_scope)
    return {'schema': 'agent-memory/retrieval-result-v1', 'project_id': project, 'results': results,
        'evaluated_at': now, 'context': context, 'scope_context': selected_scope,
        'bank_observation_sha256': digest(encoded({'project_id': project, 'sources': observed})),
        'excluded_counts': dict(sorted(excluded.items())),
        'metrics': {'inventory_files': len(observed), 'entries': len(checked['entries']), 'qualified_candidates': len(pool),
            'ranked': len(ranked), 'selected': len(results), 'bounded_candidate_bytes_read': canonical_bytes,
            'result_bytes': used, 'elapsed_seconds': time.perf_counter() - started},
        'audit': audit, 'canonical_writes_performed': False, 'audit_writes_performed': True,
        'optional_indexes_used': False, 'source_assertions_verified': False, 'publication_verified': False,
        'code_working_bytes_verified': False, 'validation_scope': 'metadata-currentness-and-bounded-file-recall'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True)
    parser.add_argument('--request', required=True)
    args = parser.parse_args(argv)
    try:
        with Path(args.request).open('rb') as stream:
            raw = stream.read(65537)
        if len(raw) > 65536:
            raise ContractError('Recall request exceeds its bounded reader')
        print(json.dumps(retrieve(args.repo, load(raw)), indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError, RecursionError):
        print(json.dumps({'schema': 'agent-memory/retrieval-result-v1', 'verified': False,
            'canonical_writes_performed': False,
            'error': 'Invalid, unsafe or changed recall input, scope or canonical state; preserve memory and reread before use'}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
