"""Source-bound, meaning-preserving maintenance; no daemon or procedure promotion."""
from __future__ import annotations

import argparse
import copy
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import uuid

from . import admission, bank_contract as bank, bank_layout, bank_transactions as tx
from . import bank_writes as writes, comparison, lifecycle, records, retrieval, scope_binding, security
from .developer_identity import digest, encoded
from .frontmatter import ContractError, read, write

INPUT = {'comparison_request', 'comparison_proof', 'capture', 'context'}
EVIDENCE = INPUT | {'recorded_at', 'git_context'}
AXES = ('kind', 'scope', 'conditions', 'applicability', 'valid-from', 'valid-until', 'trust', 'visibility')


def result(operation, **fields):
    return {'schema': 'agent-memory/consolidation-result-v1', 'operation': operation,
        'procedure_promotion_performed': False, 'semantic_truth_verified': False,
        'main_freshness_verified': False, 'publication_verified': False, **fields}


def reference(metadata):
    return {'id': metadata['id'], 'version': metadata['version']}


def child_id(operation_id, label):
    # Fresh random operation UUID supplies entropy; stable domain-separated
    # children retain the common canonical UUIDv4 record/event contract.
    return str(uuid.UUID(hex=digest(('agent-memory/consolidation/v1\n' + operation_id + '\n' + label).encode())[:32], version=4))


def union(values):
    return list({encoded(value): value for value in values}.values())


def fingerprint(metadata):
    # Repeated observations in one session are not new provenance merely
    # because their transient candidate UUID or capture timestamp differs.
    values = {key: metadata[key] for key in ('kind', 'scope', 'conditions', 'applicability', 'source', 'origin')}
    values['evidence'] = sorted(metadata['evidence'])
    return encoded(values)


def input_value(value, *, evidence=False):
    if not isinstance(value, dict) or set(value) != (EVIDENCE if evidence else INPUT):
        raise ContractError('Maintenance requires exact current comparison, optional qualified capture and actual context')
    comparison.request_value(value['comparison_request'])
    recall = {'project_id': value['comparison_request']['project_id'], 'query': '', 'ids': [], 'paths': [],
              'context': value['context'], 'kinds': list(records.MODEL['properties']['kind']['enum']),
              'minimum_trust': 'reviewed', 'limit': 64, 'include_context': False}
    retrieval.request_value(recall)
    if evidence:
        if not isinstance(value['git_context'], dict):raise ContractError('Maintenance must pin actual Git context')
        lifecycle.time_value(value['recorded_at'])
        if lifecycle.time_value(value['recorded_at']) > lifecycle.time_value(datetime.now(timezone.utc).isoformat()):
            raise ContractError('Maintenance event cannot be future dated')
    return recall


def inspect(repo, value, *, owned_lock=None):
    recall = input_value(value, evidence='recorded_at' in value)
    captured = admission.capture_value(repo, value['capture'], owned_lock=owned_lock) if value['capture'] is not None else None
    if captured and encoded(captured['request']['candidate']) != encoded(value['comparison_request']['candidate']):
        raise ContractError('Consolidate the exact qualified captured learning')
    compared = comparison.check(repo, value['comparison_request'], value['comparison_proof'], owned_lock=owned_lock)
    root, _ = writes.bank_path(repo)
    if 'git_context' in value and encoded(retrieval.git_context(root)) != encoded(value['git_context']):
        raise ContractError('The observed checkout changed before maintenance')
    if compared['disposition'] not in {'update-existing', 'consolidate-existing'}:
        return compared, None, captured
    targets = compared['proof']['targets']
    if not 1 <= len(targets) <= 8 or any(target['id'] is None for target in targets):
        raise ContractError('Automatic maintenance requires one bounded group of typed existing learning')
    recall['paths'] = [target['path'] for target in targets]
    qualified = retrieval.retrieve(repo, recall, owned_lock=owned_lock)
    if ({row['path'] for row in qualified['results']} != set(recall['paths'])
            or qualified['bank_observation_sha256'] != compared['observation']['bank_observation_sha256']):
        raise ContractError('Every maintenance source must remain current and qualified; do not revive missing or inactive sources')
    by_path = {row['path']: row for row in qualified['results']}
    if any(by_path[target['path']]['source_sha256'] != target['sha256'] for target in targets):
        raise ContractError('A source version changed between comparison and qualification')
    candidate = value['comparison_request']['candidate']
    if captured is None:
        selected = [target for target in targets if target['id'] == candidate['metadata']['id']]
        if len(selected) != 1 or encoded(selected[0]['required_preserved_metadata']) != encoded(candidate['metadata']):
            raise ContractError('A wording-only proposal must retain the exact selected existing metadata')
        primary = selected[0]
    else:
        primary = targets[0]
    metadata = primary['required_preserved_metadata']
    if any(any(encoded(target['required_preserved_metadata'][key]) != encoded(metadata[key]) for key in AXES) for target in targets):
        raise ContractError('Distinct validity, trust, visibility, type or applicability must remain separate')
    if captured and any(encoded(candidate['metadata'][key]) != encoded(metadata[key]) for key in ('valid-from', 'valid-until')):
        raise ContractError('A captured observation with distinct valid time must remain separate')
    return compared, {'primary': primary, 'targets': targets, 'qualified': qualified}, captured


def route(path, purpose, metadata, *, history=False):
    return {'path': path, 'purpose': purpose, 'scope': 'repository' if history else metadata['scope'],
            'lifecycle': 'active' if history else metadata['status'], 'read-when': 'When inspecting this learning and its provenance'}


def routed(base, checked, changes, additions, at):
    files = dict(base)
    metadata, root_routes, _ = bank.parse_router(files['MEMORY.md'])
    root_metadata, root_body = read(files['MEMORY.md'], allowed=bank.SPEC['properties'])
    def updated(rows):
        return [{**row, 'lifecycle': changes.get(row['path'], row['lifecycle'])} for row in rows]
    for entry in checked['entries']:
        if entry['format'] != 'topic-index':continue
        path = entry['path'];header, body = read(files[path], allowed=bank_layout.SPEC['properties'])
        rows = bank_layout.parse_index(path, header, body)
        if any(row['path'] in changes for row in rows):
            header['updated'] = at
            prefix = body.split('\n## Routes\n', 1)[0]
            files[path] = write(header) + prefix + '\n## Routes\n\n' + bank.route_table(updated(rows), path)
    root_metadata['router-updated'] = at
    prefix, tail = root_body.split('\n## Routes\n', 1)
    _, omissions = tail.split('\n## Omitted contexts\n', 1)
    files['MEMORY.md'] = (write(root_metadata) + prefix + '\n## Routes\n\n'
        + bank.route_table([*updated(root_routes), *additions]) + '\n## Omitted contexts\n' + omissions)
    return files


def build(base, evidence, operation_id, plan, captured):
    checked = bank.validate_files(base)
    primary, targets = plan['primary'], plan['targets']
    original = copy.deepcopy(primary['required_preserved_metadata'])
    candidate = evidence['comparison_request']['candidate']
    inventory = {}
    deleted = {entry['id'] for entry in checked['entries'] if entry['format'] == 'tombstone'}
    for entry in checked['entries']:
        if entry['format'] == 'standalone-record':
            _, metadata, _ = records.parse(base[entry['path']])
            inventory[(metadata['id'], metadata['version'])] = metadata
    ordered_targets = [primary, *[target for target in targets if target['path'] != primary['path']]]
    existing_sources = [target['required_preserved_metadata'] for target in ordered_targets]
    pending = [reference(source) for source in existing_sources]
    visited = set()
    while pending:
        ref = pending.pop();key = (ref['id'], ref['version'])
        if key in visited:continue
        visited.add(key)
        if ref['id'] in deleted:raise ContractError('Deleted provenance cannot be restored by consolidation')
        if len(visited) > 64:raise ContractError('Provenance exceeds bounded maintenance; inspect smaller independent groups')
        metadata = inventory.get(key)
        if metadata is not None:
            if metadata['status'] in {'revoked', 'expired'}:
                raise ContractError('Inactive provenance requires explicit requalification, not automatic consolidation')
            existing_sources.append(metadata);pending += metadata['derived-from']
    captured_metadata = copy.deepcopy(captured['request']['candidate']['metadata']) if captured else None
    if captured_metadata is not None and captured['request']['repository_evidence'] is not None:
        observed = captured['request']['repository_evidence']
        captured_metadata['evidence'] = union([*captured_metadata['evidence'], 'repo:' + observed['path'] + '#sha256=' + observed['sha256']])
    new_capture = captured_metadata is not None and fingerprint(captured_metadata) not in {fingerprint(m) for m in existing_sources}
    _, _, old_body = records.parse(base[primary['path']])
    if len(targets) == 1 and old_body == candidate['body'] and not new_capture:
        return None
    new = copy.deepcopy(original)
    new['version'] = max(version for identity, version in inventory if identity == original['id']) + 1
    new['updated'] = evidence['recorded_at']
    new.pop('state-reason', None);new.pop('superseded-by', None)
    new['admission-reason'] = 'Meaning-preserving consolidation: ' + evidence['comparison_proof']['judgment']['rationale']
    sources = [copy.deepcopy(target['required_preserved_metadata']) for target in ordered_targets]
    provenance = None
    if new_capture:
        provenance = copy.deepcopy(captured_metadata)
        # A separate inactive observation preserves a new capture's complete
        # identity/source even when its transient candidate reused the lesson ID.
        provenance['id'] = child_id(operation_id, 'captured-source')
        provenance['version'] = 1
        provenance['derived-from'] = union([*provenance['derived-from'], reference(captured_metadata)])
        sources.append(provenance)
    new['evidence'] = union([*[ref for source in sources for ref in source['evidence']],
                             *['record:' + source['id'] + '/v' + str(source['version']) for source in sources]])
    new['derived-from'] = union([*new['derived-from'], *[reference(source) for source in sources],
                                *[ref for source in sources for ref in source['derived-from']]])
    new['supersedes'] = union([*new['supersedes'], *[reference(source) for source in sources],
                              *[ref for source in sources for ref in source['supersedes'] + source['corrects']]])
    records.validate(new, candidate['body'])
    authority = plan['qualified']['scope_context']['developer_id']
    files, additions, changes = dict(base), [], {}
    for index, source in enumerate(sources):
        event_id = child_id(operation_id, 'supersede-' + str(index))
        event = {'id': event_id, 'authority': authority, 'reason': 'Equivalent source retained by consolidated current learning',
            'evidence': source['evidence'], 'replacement': {**reference(new), 'status': 'active',
                'replaces': reference(source), 'applicability-preserved': True}}
        if source is provenance:
            admission_event = {'id': child_id(operation_id, 'capture-admission'),
                'evidence': source['evidence'], 'authority': authority, 'reviewed': True}
            if not lifecycle.transition(source, 'admit', admission_event)['legal']:
                raise ContractError('Captured provenance cannot bypass admission')
            source.update(status='active', trust='reviewed', visibility='visible', **{'admission-reason': 'Qualified duplicate captured for provenance only'})
        if not lifecycle.transition(source, 'supersede', event)['legal']:
            raise ContractError('Source does not satisfy the common supersession model')
        inactive = {**source, 'status': 'superseded', 'updated': evidence['recorded_at'],
                    'state-reason': event['reason'], 'superseded-by': [reference(new)]}
        if index == 0:
            path = 'history/versions/' + original['id'] + '/v' + str(original['version']) + '.md'
            if path in base:raise ContractError('Do not replace existing version history')
            inactive['state-reason'] += '. Original bank-relative link base: ' + primary['path']
            files[path] = write(inactive) + old_body
            additions.append(route(path, 'Previous exact learning version and origin', inactive))
        elif source is provenance:
            path = 'history/sources/' + provenance['id'] + '.md'
            if path in base:raise ContractError('Do not replace existing captured-source provenance')
            files[path] = (write(inactive) + '# Consolidated captured source\n\nThis inactive observation preserves qualified source provenance, not separate current guidance.\n\n'
                + 'Observed capture request SHA256: ' + digest(encoded(captured['request'])) + '.\n')
            additions.append(route(path, 'Captured source provenance', inactive))
        else:
            path = ordered_targets[index]['path']
            files[path] = write(inactive) + '# Consolidated duplicate\n\nUse the explicitly linked current replacement; retain these source conditions and provenance.\n'
            changes[path] = 'superseded'
        history = 'history/events/' + event_id + '.md'
        event_record = {'schema': 'agent-memory/history-event-v1', 'id': event_id, 'project-id': new['project-id'],
            'kind': 'memory-superseded', 'record': reference(source), 'actor': authority,
            'recorded-at': evidence['recorded_at'], 'evidence': source['evidence']}
        if history in base:raise ContractError('Do not overwrite a prior maintenance event')
        files[history] = write(event_record);records.event(event_record)
        additions.append(route(history, 'Consolidation source event', event_record, history=True))
    files[primary['path']] = write(new) + candidate['body']
    event_path = 'history/events/' + operation_id + '.md'
    event = {'schema': 'agent-memory/history-event-v1', 'id': operation_id, 'project-id': new['project-id'],
        'kind': 'memory-admitted', 'record': reference(new), 'actor': authority,
        'recorded-at': evidence['recorded_at'], 'evidence': new['evidence']}
    if event_path in base:raise ContractError('Maintenance operation ID was already used')
    records.event(event);files[event_path] = write(event)
    additions.append(route(event_path, 'Consolidated version event', event, history=True))
    routed_files = routed(base, checked, changes, additions, evidence['recorded_at'])
    for path, body in files.items():
        if body != base.get(path):routed_files[path] = body
    bank.validate_files(routed_files)
    if any(len(body.encode()) > 16384 for path, body in routed_files.items() if body != base.get(path)):
        raise ContractError('Complete provenance exceeds a bounded record/router; inspect and reorganize without dropping evidence')
    security.ensure_safe({path: body for path, body in routed_files.items() if body != base.get(path)})
    return routed_files


def preview(repo, value):
    input_value(value)
    root, _ = writes.bank_path(repo)
    evidence = {**copy.deepcopy(value), 'recorded_at': datetime.now(timezone.utc).isoformat(), 'git_context': retrieval.git_context(root)}
    if isinstance(evidence['comparison_proof'], dict):evidence['comparison_proof'] = evidence['comparison_proof'].get('proof', evidence['comparison_proof'])
    if isinstance(evidence['capture'], dict):evidence['capture'] = evidence['capture'].get('capture', evidence['capture'])
    compared, plan, captured = inspect(repo, evidence)
    if plan is None:
        return result('preview', disposition='proposal-only' if compared['disposition'] == 'create-distinct' else compared['disposition'],
            proposal=None, canonical_writes_performed=False, detecting_developer_id=compared['detecting_developer_id'],
            other_developer_ids=compared['other_developer_ids'], messages_sent=False,
            recommend_other_developer_clarification=compared['recommend_other_developer_clarification'])
    root, bank_path = writes.bank_path(repo);base = tx.files_at(bank_path);operation_id = str(uuid.uuid4())
    desired = build(base, evidence, operation_id, plan, captured)
    if desired is None:return result('preview', disposition='no-change', proposal=None, canonical_writes_performed=False)
    p = tx.preview(root, desired, 'compact')['proposal']
    p.update(operation_id=operation_id, consolidation_evidence=evidence)
    if len(encoded(p)) > 33554432:raise ContractError('Complete maintenance proposal exceeds its reader; inspect smaller bounded banks')
    validate_transaction(p)
    return result('preview', disposition='consolidate-existing', proposal=p, canonical_writes_performed=False,
                  per_record_operator_approval_required=False, run_at_turn_boundary=True)


def validate_transaction(p, *, owned_lock=None):
    evidence = p['consolidation_evidence'];input_value(evidence, evidence=True)
    if p['operation'] != 'compact' or p['project_id'] != evidence['comparison_request']['project_id']:
        raise ContractError('Maintenance evidence belongs only to its own project operation')
    _, plan, captured = inspect(p['repo_root'], evidence, owned_lock=owned_lock)
    if plan is None:raise ContractError('Maintenance now requires clarification or a separate admission')
    root, bank_path = writes.bank_path(p['repo_root']);base = tx.files_at(bank_path)
    desired = build(base, evidence, p['operation_id'], plan, captured)
    if tx.file_hashes(base) != p['observed_hashes'] or encoded(desired) != encoded(p['files']):
        raise ContractError('Maintenance proposal changed its exact canonical delta or source provenance')
    scope_binding.require(root, p['project_id'], expected=p['scope_context'], owned_lock=owned_lock)


def check_sources(p, *, owner=None, marker=None):
    evidence = p['consolidation_evidence']
    scope_binding.require(p['repo_root'], p['project_id'], expected=p['scope_context'], owned_lock=owner, owned_transaction=marker)
    if encoded(retrieval.git_context(Path(p['repo_root']))) != encoded(evidence['git_context']):
        raise ContractError('Checkout context changed during maintenance')
    if evidence['capture'] is not None:
        admission_evidence = {key: evidence[key] for key in
            ('capture', 'comparison_request', 'comparison_proof', 'recorded_at')}
        admission.check_sources({**p, 'admission_evidence': admission_evidence}, owner=owner, marker=marker)
    context, _ = retrieval.observed_context(Path(p['repo_root']), evidence['context'])
    for path, body in p['files'].items():
        if p['observed_hashes'].get(path) == digest(body.encode()):continue
        try:kind, metadata, _ = records.parse(body)
        except ContractError:continue
        if kind != 'standalone-record' or metadata['status'] != 'active':continue
        applicable = records.applicability(metadata, {'bank_project_id': p['project_id'], 'context': context})
        if not lifecycle.current(metadata, {'authorized-scope': True, 'applicable': applicable['applicable'],
                'now': datetime.now(timezone.utc).isoformat(), 'minimum-trust': 'reviewed', 'active-superseders': []})['current']:
            raise ContractError('Consolidated learning lost current qualification during the transaction')


def apply(prepared):
    p = tx.proposal_value(prepared)
    if 'consolidation_evidence' not in p:raise ContractError('Use a complete governed maintenance proposal')
    receipt = tx.apply(p)
    return result('apply', **{key: value for key, value in receipt.items() if key not in {'schema', 'operation'}},
                  consolidation_verified=receipt.get('verified') is True and receipt.get('outcome') == 'finished')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__);commands = parser.add_subparsers(dest='command', required=True)
    sub = commands.add_parser('preview');sub.add_argument('--repo', required=True);sub.add_argument('--input', required=True)
    sub = commands.add_parser('apply');sub.add_argument('--proposal', required=True)
    args = parser.parse_args(argv)
    try:
        value = (preview(args.repo, admission.read_input(args.input, 262144)) if args.command == 'preview'
                 else apply(admission.read_input(args.proposal, 33554432)))
        print(json.dumps(value, indent=2, ensure_ascii=False));return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError, RecursionError):
        print(json.dumps(result(args.command, verified=False,
            canonical_writes_performed=False if args.command == 'preview' else None,
            recovery_may_be_required=args.command == 'apply',
            error='Incomplete, unsafe, changed or unqualified maintenance; preserve evidence and inspect before retry')));return 1


if __name__ == '__main__':sys.exit(main())
