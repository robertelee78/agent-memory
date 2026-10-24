"""Evidence-bound lifecycle governance, with no age-based or implicit deletion."""
from __future__ import annotations

import argparse
import copy
from datetime import datetime, timezone
from pathlib import Path
import re
import sys
import json
import uuid

from . import admission, bank_contract as bank, bank_layout, bank_transactions as tx
from . import bank_writes as writes, comparison, inactive, lifecycle, records, retrieval
from . import scope_binding, security, tombstones
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError, read, write

REQUEST = {'project_id', 'operation', 'target', 'authority', 'comparison', 'capture',
           'replacement', 'delete_ids', 'context_review', 'context_edits', 'context'}
AUTHORITY = {'source_class', 'repository_evidence', 'evidence', 'reason', 'confirmed',
             'strength', 'basis', 'impact', 'review', 'reason_class'}
OPERATIONS = {'correct', 'supersede', 'revoke', 'expire', 'retention', 'delete', 'reactivate'}
WRITE_ACTION = {'correct': 'correct', 'reactivate': 'correct', 'supersede': 'supersede',
                'revoke': 'revoke', 'expire': 'forget', 'retention': 'forget', 'delete': 'forget'}


def result(operation, **fields):
    return {'schema': 'agent-memory/governance-result-v1', 'operation': operation,
            'semantic_truth_verified': False, 'source_assertions_verified': False,
            'main_freshness_verified': False, 'publication_verified': False,
            'messages_sent': False, **fields}


def reference(metadata):
    return {'id': metadata['id'], 'version': metadata['version']}


def union(values):
    return list({encoded(value): value for value in values}.values())


def target_value(value):
    if (not isinstance(value, dict) or set(value) != {'path', 'id', 'version', 'sha256'}
            or not uuid_valid(value['id']) or type(value['version']) is not int or value['version'] < 1
            or not isinstance(value['sha256'], str) or re.fullmatch('[0-9a-f]{64}', value['sha256']) is None):
        raise ContractError('Governance requires an exact observed record path, identity, version and hash')
    bank.path_value(value['path'])


def request_value(value):
    if (not isinstance(value, dict) or set(value) != REQUEST or not uuid_valid(value['project_id'])
            or value['operation'] not in OPERATIONS):
        raise ContractError('Governance needs one explicit project operation and complete evidence')
    target_value(value['target'])
    a = value['authority']
    if (not isinstance(a, dict) or set(a) != AUTHORITY or a['source_class'] not in security.CLASSES
            or type(a['confirmed']) is not bool or a['strength'] not in {'established', 'ambiguous'}
            or a['basis'] not in {'stronger-evidence', 'loss-of-applicability', 'time-boundary', 'policy', 'explicit-deletion', 'ambiguous'}
            or a['impact'] not in {'all-target-conditions', 'narrower-contexts-remain', 'unknown'}
            or (a['review'] is not None and not isinstance(a['review'], str))):
        raise ContractError('Assess actual source, strength, applicable impact and confirmation explicitly')
    comparison.safe_text(a['reason'], 1024)
    if a['review'] is not None:comparison.safe_text(a['review'], 1024)
    records.evidence(a['evidence'])
    if len(a['evidence']) > 64 or len(encoded(a)) > 16384:
        raise ContractError('Governance authority is bounded evidence, not a transcript')
    if (value['operation'] == 'delete' and a['reason_class'] not in records.TOMBSTONE['properties']['reason-class']['enum']
            or value['operation'] != 'delete' and a['reason_class'] is not None):
        raise ContractError('Deletion reason class must remain content free')
    if (not isinstance(value['delete_ids'], list) or len(value['delete_ids']) > 64
            or any(not uuid_valid(identity) for identity in value['delete_ids'])
            or len(set(value['delete_ids'])) != len(value['delete_ids'])):
        raise ContractError('Deletion names an explicit bounded set of authorized stable identities')
    if value['operation'] != 'delete' and value['delete_ids']:
        raise ContractError('Only explicit deletion can authorize terminal identities')
    if value['operation'] in {'revoke', 'expire', 'retention', 'delete'} and any(
            value[key] is not None for key in ('capture', 'comparison', 'replacement')):
        raise ContractError('Deactivation and deletion cannot smuggle replacement or admission evidence')
    if value['operation'] in {'correct', 'reactivate'} and (value['capture'] is None or value['comparison'] is None or value['replacement'] is not None):
        raise ContractError('A new corrected version requires qualified capture and exact comparison')
    if value['operation'] == 'supersede':
        if value['capture'] is not None or value['comparison'] is None:raise ContractError('Supersession reviews an existing replacement')
        target_value(value['replacement'])
    if value['comparison'] is not None:
        if not isinstance(value['comparison'], dict) or set(value['comparison']) != {'request', 'proof'}:
            raise ContractError('Supply the exact comparison request and proof')
        comparison.request_value(value['comparison']['request'])
    if not isinstance(value['context_review'], dict) or not isinstance(value['context_edits'], dict):
        raise ContractError('Review ordinary context for copied guidance before invalidating its source')
    if not set(value['context_edits']).issubset(value['context_review']):
        raise ContractError('Context changes must be among the actual reviewed canonical files')
    for path, text in value['context_edits'].items():
        if path not in bank.CONTEXTS or not isinstance(text, str) or len(text.encode()) > 16384:
            raise ContractError('Only bounded reviewed ordinary context may accompany governance')
    recall = {'project_id': value['project_id'], 'query': '', 'ids': [], 'paths': [], 'context': value['context'],
              'kinds': [], 'minimum_trust': 'reviewed', 'limit': 1, 'include_context': False}
    retrieval.request_value(recall)
    return a


def observe(repo, request, *, owned_lock=None):
    authority = request_value(request)
    root, bank_path = writes.bank_path(repo)
    scope = scope_binding.require(root, request['project_id'], owned_lock=owned_lock)
    checked, observed = writes.snapshot(root, owned_lock=owned_lock)
    base = tx.files_at(bank_path)
    if tx.file_hashes(base) != observed:raise ContractError('Bank changed during governance review')
    expected_contexts = {path: observed[path] for path in {'MEMORY.md', *bank.CONTEXTS} if path in observed}
    if request['context_review'] != expected_contexts:
        raise ContractError('Review the router and every actual ordinary context against its current hash before governance')
    target = request['target'];path = target['path']
    if observed.get(path) != target['sha256']:
        raise ContractError('Governance target changed; read its current meaning and evidence again')
    kind, metadata, body = records.parse(base[path])
    if (kind != 'standalone-record' or reference(metadata) != {key: target[key] for key in ('id', 'version')}
            or metadata['project-id'] != request['project_id']):
        raise ContractError('Governance targets the exact same-project standalone version')
    if authority['repository_evidence'] is not None:
        security.repository_source(root, authority['repository_evidence'])
        if authority['repository_evidence']['path'] not in authority['evidence']:
            raise ContractError('Retain the observed repository source in governance evidence')
    context, git_context = retrieval.observed_context(root, request['context'])
    captured = admission.capture_value(root, request['capture'], owned_lock=owned_lock) if request['capture'] is not None else None
    compared = None
    if request['comparison'] is not None:
        c = request['comparison']
        compared = comparison.check(root, c['request'], c['proof'], owned_lock=owned_lock)
        if c['request']['project_id'] != request['project_id']:
            raise ContractError('Review sources from this project only')
    if captured is not None:
        candidate = captured['request']['candidate']
        if (candidate['metadata']['id'] != metadata['id'] or captured['request']['project_id'] != request['project_id']
                or encoded(candidate) != encoded(request['comparison']['request']['candidate'])
                or authority['source_class'] != captured['request']['source_class']
                or encoded(authority['repository_evidence']) != encoded(captured['request']['repository_evidence'])):
            raise ContractError('Correct the stable identity using exactly its qualified actual source capture')
    replacement = None
    if request['operation'] == 'supersede':
        replacement_ref = request['replacement'];path = replacement_ref['path']
        if observed.get(path) != replacement_ref['sha256']:raise ContractError('The existing replacement changed')
        replacement_kind, replacement_metadata, replacement_body = records.parse(base[path])
        if (replacement_kind != 'standalone-record' or reference(replacement_metadata) !=
                {key: replacement_ref[key] for key in ('id', 'version')} or replacement_metadata['id'] == metadata['id']):
            raise ContractError('Use a distinct exact existing replacement; same-ID changes use correction')
        if encoded(request['comparison']['request']['candidate']) != encoded({'metadata': replacement_metadata, 'body': replacement_body}):
            raise ContractError('Review the complete unchanged existing replacement before linking it')
        if path not in {row['path'] for row in compared['proof']['targets']}:
            raise ContractError('The exact existing replacement must be reviewed as equivalent to itself')
        recalled = retrieval.retrieve(root, {'project_id': request['project_id'], 'query': '', 'ids': [], 'paths': [path],
            'context': request['context'], 'kinds': list(records.MODEL['properties']['kind']['enum']),
            'minimum_trust': 'reviewed', 'limit': 1, 'include_context': False}, owned_lock=owned_lock)
        if ([row['path'] for row in recalled['results']] != [path]
                or recalled['results'][0]['source_sha256'] != replacement_ref['sha256']
                or recalled['bank_observation_sha256'] != digest(encoded({'project_id': request['project_id'], 'sources': observed}))):
            raise ContractError('Supersession needs a currently qualified canonical replacement')
        replacement = {'metadata': replacement_metadata, 'body': replacement_body}
    return {'root': root, 'bank': bank_path, 'base': base, 'checked': checked, 'observed': observed,
            'scope': scope, 'metadata': metadata, 'body': body, 'context': context, 'git_context': git_context,
            'capture': captured, 'comparison': compared, 'replacement': replacement}


def pending(observation, disposition, **fields):
    peer = observation['metadata']['origin']['developer-id']
    developer = observation['scope']['developer_id']
    defaults = dict(disposition=disposition, proposal=None, canonical_writes_performed=False,
        detecting_developer_id=developer, other_developer_ids=[peer] if peer != developer else [],
        recommend_other_developer_clarification=peer != developer)
    defaults.update(fields)
    return result('preview', **defaults)


def narrower_conditions(old, new):
    preserved = all(new['conditions'].get(key, object()) == value for key, value in old['conditions'].items())
    return (old['kind'] == new['kind'] and old['scope'] == new['scope'] and old['applicability'] == new['applicability']
            and preserved and set(old['conditions']) < set(new['conditions'])
            and all(value is not None for value in new['conditions'].values()))


def permitted_impact(observation, request):
    if request['authority']['impact'] == 'all-target-conditions':return True
    return (request['operation'] == 'correct' and request['authority']['impact'] == 'narrower-contexts-remain'
            and narrower_conditions(observation['metadata'], observation['capture']['request']['candidate']['metadata'])
            and observation['capture']['request']['candidate']['body'] == observation['body'])


def copied_body(body, text):
    target = body.strip()
    return bool(target) and (target in text if '\n' in target else target in {line.strip() for line in text.splitlines()})


def review_deferral(observation, request):
    compared = observation['comparison']
    if compared is None:return None
    rows = {row['path']: row for row in compared['matches']}
    reviewed_target = rows.get(request['target']['path'])
    if reviewed_target is None or reviewed_target['metadata'] != observation['metadata']:
        raise ContractError('Read the exact existing target during semantic review')
    if any(decision['classification'] == 'uncertain' for decision in compared['proof']['judgment']['decisions']):
        return {'disposition': 'clarification-required'}
    if any(rows[path]['metadata'] is None or rows[path]['metadata']['id'] != observation['metadata']['id']
           for path in compared['disputed_paths']):
        return {'disposition': 'clarification-required', 'other_developer_ids': compared['other_developer_ids'],
                'recommend_other_developer_clarification': compared['recommend_other_developer_clarification']}
    if request['operation'] in {'correct', 'reactivate'} and any(target['id'] != observation['metadata']['id']
            for target in compared['proof']['targets']):
        return {'disposition': 'consolidation-required'}
    return None


def next_version(observation, identity):
    versions = []
    for entry in observation['checked']['entries']:
        if entry['format'] == 'standalone-record' and entry['id'] == identity:versions.append(entry['version'])
    return max(versions) + 1


def archive(files, additions, metadata, body, replacement, at, original_path, reason, *, reactivating=False):
    path = 'history/versions/' + metadata['id'] + '/v' + str(metadata['version']) + '.md'
    if path in files:raise ContractError('Do not overwrite an earlier exact source version')
    old = copy.deepcopy(metadata)
    if not reactivating:
        old.update(status='superseded', updated=at, **{'superseded-by': [reference(replacement)],
            'state-reason': reason + '. Original bank-relative link base: ' + original_path})
    else:
        old['state-reason'] += '. Original bank-relative link base: ' + original_path
    files[path] = write(old) + body
    additions.append(route(path, 'Historical version and original source', old['status']))


def qualified_authority(request):
    a = request['authority']
    if not a['confirmed'] or a['strength'] != 'established' or a['basis'] == 'ambiguous':return False
    if request['operation'] == 'expire':return a['basis'] == 'time-boundary'
    if a['source_class'] == 'user':return True
    return a['repository_evidence'] is not None and (a['source_class'] == 'verified-repository' or a['review'] is not None)


def reroute(base, checked, changed, removed, additions, at):
    """Keep introductions, remove empty index chains, and never orphan a child."""
    files = {path: body for path, body in base.items() if path not in removed}
    indices = sorted((e['path'] for e in checked['entries'] if e['format'] == 'topic-index'),
                     key=lambda path: path.count('/'), reverse=True)
    def rows_after(rows):
        return [{**row, 'lifecycle': changed.get(row['path'], row['lifecycle'])}
                for row in rows if row['path'] not in removed]
    for path in indices:
        header, body = read(base[path], allowed=bank_layout.SPEC['properties'])
        rows = bank_layout.parse_index(path, header, body);updated = rows_after(rows)
        if not updated:
            original_prefix = body.split('\n## Routes\n', 1)[0]
            generated = bank_layout.index_text(path, header, rows).split('\n---\n', 1)[1].split('\n## Routes\n', 1)[0]
            if ' '.join(original_prefix.split()) != ' '.join(generated.split()):
                raise ContractError('The empty index contains custom context; preserve it before pruning its last route')
            removed.add(path);files.pop(path, None)
        elif rows != updated:
            header['updated'] = at
            files[path] = write(header) + body.split('\n## Routes\n', 1)[0] + '\n## Routes\n\n' + bank.route_table(updated, path)
    metadata, rows, _ = bank.parse_router(base['MEMORY.md'])
    _, body = read(base['MEMORY.md'], allowed=bank.SPEC['properties'])
    prefix, rest = body.split('\n## Routes\n', 1);_, omissions = rest.split('\n## Omitted contexts\n', 1)
    metadata['router-updated'] = at
    files['MEMORY.md'] = (write(metadata) + prefix + '\n## Routes\n\n' + bank.route_table([*rows_after(rows), *additions])
                           + '\n## Omitted contexts\n' + omissions)
    return files


def route(path, purpose, state='active'):
    return {'path': path, 'purpose': purpose, 'scope': 'repository', 'lifecycle': state,
            'read-when': 'Before using or rebuilding affected learning'}


def delete_closure(observation, request):
    identities = {observation['metadata']['id'], *request['delete_ids']};items = []
    for entry in observation['checked']['entries']:
        if entry['format'] == 'standalone-record':
            _, metadata, body = records.parse(observation['base'][entry['path']]);items.append((entry['path'], metadata, body))
    sources = [(path, metadata, records.source_capture_id(path, metadata)) for path, metadata, _ in items
               if path.startswith('history/sources/')]
    source_units = {metadata['id'] for _, metadata, _ in sources}
    known = {metadata['id'] for _, metadata, _ in items} | {alias for _, _, alias in sources if alias is not None} | {
        entry['id'] for entry in observation['checked']['entries'] if entry['format'] == 'tombstone'}
    if not identities.issubset(known):raise ContractError('Deletion scope must name actual same-project canonical identities')
    while True:
        expanded = identities | {metadata['id'] for _, metadata, _ in items
            if any(ref['id'] in identities for ref in metadata['derived-from'])}
        references = {ref['id'] for _, metadata, _ in items if metadata['id'] in identities for ref in metadata['derived-from']}
        expanded.update(metadata['id'] for _, metadata, _ in items if metadata['id'] in source_units & references
            and any(ref['id'] in identities for ref in metadata.get('superseded-by', [])))
        for _, metadata, alias in sources:
            if alias is not None and alias in expanded:expanded.add(metadata['id'])
            if metadata['id'] in expanded and alias is not None:expanded.add(alias)
        if len(expanded) > 64:raise ContractError('Deletion derivation closure requires an explicitly scoped bounded review')
        if expanded == identities:break
        identities = expanded
    unresolved = [path for path, metadata, alias in sources if metadata['id'] in identities and alias is None]
    return identities, items, unresolved


def desired(observation, request, operation_id, at):
    base = observation['base'];old = observation['metadata'];operation = request['operation'];a = request['authority']
    actor = observation['scope']['developer_id'];files = dict(base);removed = set();changed = {};additions = []
    event = {'id': operation_id, 'authority': actor, 'reason': a['reason'], 'evidence': a['evidence'], 'now': at}
    deferred = review_deferral(observation, request)
    if deferred:return None, deferred
    if operation == 'delete':
        if a['source_class'] != 'user' or a['basis'] != 'explicit-deletion':
            raise ContractError('Terminal deletion requires explicit operator authority, not automatic pruning')
        identities, items, unresolved = delete_closure(observation, request)
        if unresolved:
            peers = sorted({metadata['origin']['developer-id'] for path, metadata, _ in items
                if path in unresolved and metadata['origin']['developer-id'] != actor})
            return None, {'disposition': 'provenance-identity-review-required', 'source_paths': sorted(unresolved),
                'other_developer_ids': peers, 'recommend_other_developer_clarification': bool(peers)}
        if set(request['delete_ids']) != identities:
            peers = sorted({metadata['origin']['developer-id'] for _, metadata, _ in items
                if metadata['id'] in identities - set(request['delete_ids']) and metadata['origin']['developer-id'] != actor})
            return None, {'disposition': 'deletion-scope-review-required', 'required_delete_ids': sorted(identities),
                'other_developer_ids': peers, 'recommend_other_developer_clarification': bool(peers)}
        for path, metadata, body in items:
            if metadata['id'] not in identities:continue
            if not lifecycle.transition(metadata, 'delete', event)['legal']:
                raise ContractError('Terminal deletion must use the common lifecycle guards')
            removed.add(path)
        for path, metadata, body in items:
            if metadata['id'] in identities:continue
            # The tombstone now owns terminal invalidation. Remove redundant
            # pointers to erased guidance without deleting an independently
            # evidenced correction/replacement or copying the erased content.
            supersedes = [ref for ref in metadata['supersedes'] if ref['id'] not in identities]
            if supersedes != metadata['supersedes']:
                files[path] = write({**metadata, 'supersedes': supersedes}) + body
        for entry in observation['checked']['entries']:
            if entry['format'] != 'optional-history-event':continue
            _, metadata, _ = records.parse(base[entry['path']])
            if metadata['record']['id'] in identities:removed.add(entry['path'])
        for identity in sorted(identities):
            path = tombstones.relative_path(identity)
            if path in base:
                marker = tombstones.parse(base[path])
                if marker['id'] != identity or marker['scope'] != request['project_id']:
                    raise ContractError('Never overwrite an unknown terminal marker')
                continue
            metadata = {'id': identity, 'scope': request['project_id'], 'deleted-at': at,
                        'deletion-authority': actor, 'reason-class': a['reason_class']}
            files[path] = tombstones.render(metadata)['content'];additions.append(route(path, 'Terminal deletion marker', 'deleted'))
        for path, _, body in items:
            if path not in removed:continue
            if any(copied_body(body, text) for text in request['context_edits'].values()):
                raise ContractError('Deleted learning cannot be copied into retained context')
    elif operation in {'revoke', 'expire', 'retention'}:
        if operation == 'expire' and a['basis'] != 'time-boundary':raise ContractError('Expiry uses an observed valid-time boundary')
        if operation == 'retention' and a['basis'] != 'policy':raise ContractError('Retention requires its actual policy, never age alone')
        if operation == 'revoke' and a['basis'] not in {'stronger-evidence', 'loss-of-applicability'}:
            raise ContractError('Automatic pruning requires stronger correction or actual loss of applicability')
        transition = lifecycle.transition(old, 'expire' if operation == 'expire' else 'revoke', event)
        if not transition['legal']:raise ContractError('The explicit forgetting operation does not satisfy the common lifecycle guards')
        metadata = {**copy.deepcopy(old), 'status': transition['to'], 'updated': at,
                    'state-reason': a['reason'], 'evidence': union([*old['evidence'], *a['evidence']])}
        files[request['target']['path']] = inactive.compact(write(metadata) + observation['body'])['content']
        changed[request['target']['path']] = transition['to']
        history = 'history/events/' + operation_id + '.md'
        if history in base:raise ContractError('Do not overwrite another lifecycle event')
        event_metadata = {'schema': 'agent-memory/history-event-v1', 'id': operation_id, 'project-id': request['project_id'],
            'kind': 'memory-expired' if operation == 'expire' else 'memory-revoked', 'record': reference(old),
            'actor': actor, 'recorded-at': at, 'evidence': a['evidence']}
        records.event(event_metadata);files[history] = write(event_metadata);additions.append(route(history, 'Lifecycle decision evidence'))
    elif operation in {'correct', 'reactivate'}:
        if a['basis'] != 'stronger-evidence' and not (operation == 'correct' and a['basis'] == 'loss-of-applicability'):
            raise ContractError('A new qualified version needs actual stronger evidence or a narrower applicability correction')
        captured = observation['capture'];candidate = captured['request']['candidate'];new = copy.deepcopy(candidate['metadata'])
        if any(new[key] != old[key] for key in ('kind', 'scope', 'applicability')):
            raise ContractError('Corrected versions preserve their type, scope and assessed applicability')
        if any(key not in new['conditions'] or new['conditions'][key] != value for key, value in old['conditions'].items()):
            raise ContractError('Do not widen or replace retained conditions while correcting guidance')
        if operation == 'correct' and any(new[key] != old[key] for key in ('valid-from', 'valid-until')):
            raise ContractError('A correction cannot silently widen its validity period')
        if operation == 'reactivate':
            now = lifecycle.time_value(at)
            if (new['valid-from'] is None or lifecycle.time_value(new['valid-from']) > now
                    or (new['valid-until'] is not None and lifecycle.time_value(new['valid-until']) <= now)):
                raise ContractError('Reactivation requires an explicit newly evidenced currently valid period')
        new.update(version=next_version(observation, old['id']), status='active', trust='reviewed', updated=at,
            visibility='hidden' if 'hidden' in {old.get('visibility'), new.get('visibility')} else 'visible')
        new['admission-reason'] = ('Explicit requalification: ' if operation == 'reactivate' else 'Stronger-evidence correction: ') + a['reason']
        new.pop('state-reason', None);new.pop('superseded-by', None)
        evidence = [*new['evidence'], *a['evidence']]
        if a['repository_evidence'] is not None:
            source = a['repository_evidence'];evidence.append('repo:' + source['path'] + '#sha256=' + source['sha256'])
        event['evidence'] = union(evidence)
        new['evidence'] = union([*old['evidence'], *evidence])
        new['derived-from'] = union([*old['derived-from'], *new['derived-from'], reference(old)])
        new['corrects'] = union([*old['corrects'], *([reference(old)] if operation == 'correct' else [])])
        new['supersedes'] = union([*old['supersedes'], *([reference(old)] if operation == 'reactivate' else [])])
        event['replacement'] = {**reference(new), 'status': 'active', 'replaces': reference(old), 'applicability-preserved': True}
        if not lifecycle.transition(old, operation, event)['legal']:
            raise ContractError('The new version lacks explicit new evidence or a legal common lifecycle transition')
        records.validate(new, candidate['body'])
        archive(files, additions, old, observation['body'], new, at, request['target']['path'], a['reason'],
                reactivating=operation == 'reactivate')
        files[request['target']['path']] = write(new) + candidate['body']
        changed[request['target']['path']] = 'active'
        history = 'history/events/' + operation_id + '.md'
        if history in base:raise ContractError('Do not overwrite another lifecycle event')
        event_metadata = {'schema': 'agent-memory/history-event-v1', 'id': operation_id, 'project-id': request['project_id'],
            'kind': 'memory-corrected' if operation == 'correct' else 'memory-admitted', 'record': reference(new),
            'actor': actor, 'recorded-at': at, 'evidence': event['evidence']}
        records.event(event_metadata);files[history] = write(event_metadata);additions.append(route(history, 'New qualified version event'))
    elif operation == 'supersede':
        if a['basis'] != 'stronger-evidence':raise ContractError('Supersession needs stronger replacement evidence')
        replacement = observation['replacement'];previous = replacement['metadata']
        if any(previous[key] != old[key] for key in ('kind', 'scope', 'conditions', 'applicability', 'valid-from', 'valid-until', 'trust', 'visibility')):
            return None, {'disposition': 'narrowing-required'}
        new = copy.deepcopy(previous);new.update(version=next_version(observation, previous['id']), updated=at)
        new['admission-reason'] = 'Existing evidenced replacement: ' + a['reason']
        new['evidence'] = union([*new['evidence'], *a['evidence']])
        new['derived-from'] = union([*new['derived-from'], reference(previous)])
        new['supersedes'] = union([*new['supersedes'], reference(old), *old['supersedes'], *old['corrects']])
        event['replacement'] = {**reference(new), 'status': 'active', 'replaces': reference(old), 'applicability-preserved': True}
        if not lifecycle.transition(old, 'supersede', event)['legal']:
            raise ContractError('Supersession must name its qualified replacement and preserve applicability')
        records.validate(new, replacement['body'])
        archive(files, additions, previous, replacement['body'], new, at, request['replacement']['path'], a['reason'])
        files[request['replacement']['path']] = write(new) + replacement['body']
        prior = {**copy.deepcopy(old), 'status': 'superseded', 'updated': at,
                 'state-reason': a['reason'], 'superseded-by': [reference(new)]}
        files[request['target']['path']] = write(prior) + observation['body']
        changed[request['target']['path']] = 'superseded'
        for identity, kind, record in ((operation_id, 'memory-superseded', old),
                (str(uuid.UUID(hex=digest((operation_id + ':replacement-version').encode())[:32], version=4)), 'memory-admitted', new)):
            path = 'history/events/' + identity + '.md'
            if path in base:raise ContractError('Do not overwrite another lifecycle event')
            metadata = {'schema': 'agent-memory/history-event-v1', 'id': identity, 'project-id': request['project_id'],
                'kind': kind, 'record': reference(record), 'actor': actor, 'recorded-at': at, 'evidence': a['evidence']}
            records.event(metadata);files[path] = write(metadata);additions.append(route(path, 'Supersession and replacement version event'))
    routed = reroute(base, observation['checked'], changed, removed, additions, at)
    for path, body in files.items():
        if path not in removed and body != base.get(path):routed[path] = body
    routed.update(request['context_edits'])
    if operation == 'delete':
        for _, _, body in (item for item in items if item[0] in removed):
            if any(copied_body(body, routed[path]) for path in request['context_review']):
                raise ContractError('A retained ordinary context still contains the deleted assertion')
    bank.validate_files(routed)
    changed_files = {path: body for path, body in routed.items() if body != base.get(path)}
    if any(len(body.encode()) > 16384 for body in changed_files.values()):raise ContractError('Governance must preserve complete bounded canonical files')
    security.ensure_safe(changed_files)
    return routed, None


def preview(repo, request):
    request = copy.deepcopy(request)
    if isinstance(request.get('capture'), dict):request['capture'] = request['capture'].get('capture', request['capture'])
    if isinstance(request.get('comparison'), dict) and isinstance(request['comparison'].get('proof'), dict):
        request['comparison']['proof'] = request['comparison']['proof'].get('proof', request['comparison']['proof'])
    observation = observe(repo, request)
    if not qualified_authority(request):return pending(observation, 'clarification-required')
    if not permitted_impact(observation, request):
        return pending(observation, 'narrowing-required')
    at = datetime.now(timezone.utc).isoformat();operation_id = str(uuid.uuid4())
    files, deferred = desired(observation, request, operation_id, at)
    if deferred:return pending(observation, **deferred)
    p = tx.preview(observation['root'], files, WRITE_ACTION[request['operation']])['proposal']
    p.update(operation_id=operation_id, governance_evidence={'request': request, 'recorded_at': at,
                                                          'git_context': observation['git_context']})
    validate_transaction(p)
    return result('preview', disposition=request['operation'], proposal=p, canonical_writes_performed=False)


def validate_transaction(p, *, owned_lock=None):
    evidence = p['governance_evidence']
    if not isinstance(evidence, dict) or set(evidence) != {'request', 'recorded_at', 'git_context'}:
        raise ContractError('Governance needs its exact request and observed event context')
    at = evidence['recorded_at'];lifecycle.time_value(at)
    if lifecycle.time_value(at) > lifecycle.time_value(datetime.now(timezone.utc).isoformat()):raise ContractError('Governance event cannot be future dated')
    request = evidence['request'];observation = observe(p['repo_root'], request, owned_lock=owned_lock)
    if (p['operation'] != WRITE_ACTION[request['operation']] or p['project_id'] != request['project_id']
            or not qualified_authority(request) or encoded(observation['git_context']) != encoded(evidence['git_context'])):
        raise ContractError('Governance evidence or actual Git context changed')
    if not permitted_impact(observation, request):
        raise ContractError('Preserve narrower applicability before pruning')
    files, deferred = desired(observation, request, p['operation_id'], at)
    if deferred or observation['observed'] != p['observed_hashes'] or encoded(files) != encoded(p['files']):
        raise ContractError('Governance must apply its exact observed canonical delta')
    scope_binding.require(p['repo_root'], request['project_id'], expected=p['scope_context'], owned_lock=owned_lock)
    check_sources(p, owner=owned_lock)


def check_sources(p, *, owner=None, marker=None):
    evidence = p['governance_evidence'];request = evidence['request'];root = Path(p['repo_root'])
    scope_binding.require(root, request['project_id'], expected=p['scope_context'], owned_lock=owner, owned_transaction=marker)
    if encoded(retrieval.git_context(root)) != encoded(evidence['git_context']):raise ContractError('Checkout changed during governance')
    source = request['authority']['repository_evidence']
    if source is not None:security.repository_source(root, source)
    if request['capture'] is not None:
        admission_evidence = {'capture': request['capture'], 'comparison_request': request['comparison']['request'],
            'comparison_proof': request['comparison']['proof'], 'recorded_at': evidence['recorded_at']}
        admission.check_sources({**p, 'admission_evidence': admission_evidence}, owner=owner, marker=marker)
    if request['operation'] == 'reactivate':
        _, metadata, _ = records.parse(p['files'][request['target']['path']]);now = lifecycle.time_value(datetime.now(timezone.utc).isoformat())
        if (lifecycle.time_value(metadata['valid-from']) > now
                or (metadata['valid-until'] is not None and lifecycle.time_value(metadata['valid-until']) <= now)):
            raise ContractError('The newly requalified period ended during governance')
    if request['operation'] == 'supersede':
        checked = bank.validate_files(p['files']);path = request['replacement']['path']
        _, replacement, _ = records.parse(p['files'][path])
        context, _ = retrieval.observed_context(root, request['context']);now = datetime.now(timezone.utc).isoformat()
        superseders = []
        for entry in checked['entries']:
            if entry['format'] != 'standalone-record' or retrieval.scope_reasons(request['project_id'], entry, context):continue
            _, metadata, _ = records.parse(p['files'][entry['path']])
            applicable = records.applicability(metadata, {'bank_project_id': request['project_id'], 'context': context})
            selection = lifecycle.current(metadata, {'authorized-scope': True, 'applicable': applicable['applicable'],
                'now': now, 'minimum-trust': 'reviewed', 'active-superseders': []})
            if (all(selection['gates'][key] for key in ('authorized-scope', 'applicable', 'active', 'valid-time', 'sufficient-trust'))
                    and reference(replacement) in metadata['supersedes'] + metadata['corrects']):
                superseders.append(metadata['id'] + '/' + str(metadata['version']))
        applicable = records.applicability(replacement, {'bank_project_id': request['project_id'], 'context': context})
        if not lifecycle.current(replacement, {'authorized-scope': True, 'applicable': applicable['applicable'],
                'now': now, 'minimum-trust': 'reviewed', 'active-superseders': superseders})['current']:
            raise ContractError('The replacement lost its current qualification during governance')


def apply(prepared):
    p = tx.proposal_value(prepared)
    if 'governance_evidence' not in p:raise ContractError('Use a complete governed lifecycle proposal')
    receipt = tx.apply(p)
    return result('apply', **{key: value for key, value in receipt.items() if key not in {'schema', 'operation'}},
        governance_verified=receipt.get('verified') is True and receipt.get('outcome') == 'finished')


def main(argv=None, *, required_operations=None):
    parser = argparse.ArgumentParser(description=__doc__);commands = parser.add_subparsers(dest='command', required=True)
    sub = commands.add_parser('preview');sub.add_argument('--repo', required=True);sub.add_argument('--request', required=True)
    sub = commands.add_parser('apply');sub.add_argument('--proposal', required=True)
    args = parser.parse_args(argv)
    try:
        prepared = admission.read_input(args.request if args.command == 'preview' else args.proposal,
                                        262144 if args.command == 'preview' else 33554432)
        if required_operations is not None:
            request = (prepared if args.command == 'preview' else
                       tx.proposal_value(prepared)['governance_evidence']['request'])
            if request['operation'] not in required_operations:
                raise ContractError('The requested operation differs from the CLI intent')
        value = preview(args.repo, prepared) if args.command == 'preview' else apply(prepared)
        print(json.dumps(value, indent=2, ensure_ascii=False));return 0
    except (ContractError, OSError, ValueError, TypeError, KeyError, AttributeError, RecursionError):
        print(json.dumps(result(args.command, verified=False, canonical_writes_performed=False if args.command == 'preview' else None,
            recovery_may_be_required=args.command == 'apply', error='Unqualified, ambiguous, changed or incomplete governance; preserve evidence and inspect')))
        return 1


if __name__ == '__main__':sys.exit(main())
