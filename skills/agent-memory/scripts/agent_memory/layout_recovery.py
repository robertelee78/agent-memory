"""Source-preserving layout recovery with immutable reviews and typed journals.

Private originals are evidence, never admission. Historical receipts and current
coverage are intentionally separate observations.
"""
from __future__ import annotations

import argparse
import base64
import copy
import json
import os
from pathlib import Path
import stat
import sys
import uuid

from . import artifact, bank_contract as bank, bank_observation, bank_transactions as tx
from . import bank_writes as writes, layout_recovery_observation as raw
from . import layout_recovery_state as state, local_registry as storage
from . import recovery_frontier as frontier, security, sharing_limits as limits
from . import layout_recovery_temps as temps
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError
from .recovery_quiescence import attestation_value
from .bank_capacity import BankCapacityError
from . import layout_history
from .layout_recovery_context import RecoveryReadSet, runtime_guard

RESULT = 'agent-memory/layout-recovery-result-v1'
PROPOSAL = 'agent-memory/layout-recovery-proposal-v1'
JOURNAL = 'agent-memory/layout-recovery-journal-v1'
RECEIPT = 'agent-memory/layout-recovery-receipt-v1'
MARKER = '.agent-memory-transaction.md'
SENTINEL = '.agent-memory-recovery.md'
DISPOSITIONS = {'preserve', 'pending-admission', 'governed-terminal', 'non-guidance-discard'}


def fields(value, names, schema=None):
    if not isinstance(value, dict) or set(value) != set(names) or (schema and value.get('schema') != schema):
        raise ContractError('Unknown or incomplete recovery artifact schema')
    return value


def participant(value):
    fields(value, {'developer_id', 'harness', 'session', 'evidence'})
    if not uuid_valid(value['developer_id']):
        raise ContractError('Recovery review requires real developer attribution')
    for name in ('harness', 'session', 'evidence'):
        if not isinstance(value[name], str) or not value[name].strip() or len(value[name].encode()) > 4096:
            raise ContractError('Recovery review requires retained actual harness/session evidence')
    return value


def independent(author, reviewer):
    participant(author); participant(reviewer)
    if (author['harness'], author['session']) == (reviewer['harness'], reviewer['session']):
        raise ContractError('Recovery requires a distinct independently reviewing agent/session')


def result(operation, operation_id=None, **values):
    return {'schema': RESULT, 'operation': operation, 'operation_id': operation_id,
            'state': 'pending', 'verified': False, 'canonical_eligibility': 'unavailable',
            'semantic_recovery_complete': False, 'pending_unit_ids': [], 'pending_units': 0,
            'required_actions': [], 'writes_performed': False,
            'private_artifact_writes_performed': False, 'temporary_lock_writes_performed': False,
            'canonical_writes_performed': False, 'original_index_writes_performed': False,
            'head_writes_performed': False, 'feature_writes_performed': False,
            'history_writes_performed': False, 'truth_verified': False,
            'code_freshness_verified': False, 'publication_verified': False,
            'other_machine_import_verified': False, **values}


def eligibility(repo, owner=None, transaction=None):
    try:
        bank.validate(repo, owned_lock=owner, owned_transaction=transaction)
        return True
    except BankCapacityError:
        return False
    except (ContractError, OSError, UnicodeError, ValueError):
        return 'unavailable'


def operation(repo, operation_id):
    root, target, base, directory = state.paths(repo, operation_id)
    manifest = state.read(directory / 'manifest.json')
    fields(manifest, {'schema', 'operation_id', 'inspected_at', 'repo_root', 'bank_path', 'project_id', 'identity',
                      'observation_complete', 'physical_entries', 'sources', 'totals', 'git', 'runtime_artifact',
                      'metadata_policy', 'bank_binding', 'predecessor'}, 'agent-memory/layout-source-manifest-v1')
    if (not uuid_valid(manifest['project_id']) or not isinstance(manifest['sources'], list)
            or len(manifest['sources']) > raw.MAX_FILES or not isinstance(manifest['physical_entries'], list)
            or len(manifest['physical_entries']) > raw.MAX_ENTRIES
            or manifest['metadata_policy'] != 'ordinary-modes-ownership-darwin-provenance-exact-no-acl-other-xattr-flags'):
        raise ContractError('Recovery manifest identity/type/capacity is invalid')
    storage.time_value(manifest['inspected_at'])
    ids = set(); paths = set()
    for source in manifest['sources']:
        fields(source, {'source_id', 'path', 'byte_count', 'sha256', 'blob'})
        raw.path_value(source['path'])
        if (source['source_id'] in ids or source['path'] in paths or type(source['byte_count']) is not int
                or not 0 <= source['byte_count'] <= raw.MAX_FILE or not state.sha(source['sha256'])):
            raise ContractError('Recovery manifest duplicates or misrepresents an original source')
        ids.add(source['source_id']); paths.add(source['path'])
    if (sum(source['byte_count'] for source in manifest['sources']) > raw.MAX_BYTES
            or sum(source['byte_count'] for source in manifest['sources']) != manifest['totals']['source_bytes']
            or len(manifest['sources']) != manifest['totals']['source_files']):
        raise ContractError('Recovery complete-source totals do not match the manifest')
    if (manifest.get('schema') != 'agent-memory/layout-source-manifest-v1'
            or manifest.get('operation_id') != operation_id or manifest.get('repo_root') != str(root)
            or manifest.get('bank_path') != str(target) or not manifest.get('observation_complete')):
        raise ContractError('Recovery requires the exact complete inspected operation')
    if state.read_bytes(directory / 'manifest.sha256') != (digest(encoded(manifest)) + '\n').encode():
        raise ContractError('Recovery manifest digest is missing or corrupt')
    for source in manifest['sources']:
        if not uuid_valid(source['source_id']) or source['blob'] != 'originals/' + source['source_id'] + '.bin':
            raise ContractError('Recovery source locator is not owned by this operation')
        body = state.read_bytes(directory / source['blob'], raw.MAX_FILE)
        if len(body) != source['byte_count'] or digest(body) != source['sha256']:
            raise ContractError('Original recovery evidence is missing or damaged')
    return root, target, base, directory, manifest


def predecessor_value(repo, manifest=None, owner=None):
    predecessor = frontier.rolled_back_predecessor(repo, owner)
    pinned = None if predecessor is None else {
        'operation_id': predecessor.operation_id, 'marker_sha256': digest(predecessor.marker),
        'frontier': predecessor.token}
    if manifest is not None and manifest.get('predecessor') != pinned:
        raise ContractError('Raw continuation lost its exact internally checked rollback predecessor')
    return predecessor, pinned


def inspect(repo, identity_review=None):
    root, target = raw.locate(repo)
    operation_id = str(uuid.uuid4())
    # Unknown/live ownership precedes reading any source body.
    predecessor, _ = predecessor_value(root)
    bank_observation.markers(target, None, predecessor.marker if predecessor else None)
    from .turn_sources import canonical_writer_barrier
    with canonical_writer_barrier(root), storage.exclusive(target / '.agent-memory.lock', operation_id, 'bank-recover') as owner:
        from . import layout_resolution
        layout_resolution.require_completed_resolutions(root)
        try:
            predecessor, pinned = predecessor_value(root, owner=owner)
            transaction = predecessor.marker if predecessor else None
            manifest = raw.observe(root, operation_id, owner=owner, transaction=transaction,
                                   identity_review=identity_review, predecessor=pinned)
        except (ContractError, OSError, UnicodeError, ValueError) as exc:
            exc.operation_id = operation_id
            raise
        directory = state.paths(root, operation_id)[3]
        state.write_once(directory / 'manifest.json', manifest)
        state.put_bytes(directory / 'manifest.sha256', (digest(encoded(manifest)) + '\n').encode())
        writes.fault('layout-recovery-originals-completed')
        raw.recheck(root, manifest, owner, transaction)
        eligible = eligibility(root, owner, transaction)
    return result('inspect', operation_id, state='observed', verified=True,
                  canonical_eligibility=eligible, private_artifact_writes_performed=True,
                  temporary_lock_writes_performed=True, observation_complete=True,
                  source_files=manifest['totals']['source_files'], source_bytes=manifest['totals']['source_bytes'],
                  page_count=sum(max(1, (source['byte_count'] + 24575) // 24576) for source in manifest['sources']),
                  manifest=manifest, required_actions=['Review every source page and the full intended native result independently before prepare'])


def page(repo, operation_id, number):
    _, _, _, directory, manifest = operation(repo, operation_id)
    if type(number) is not int or number < 0:
        raise ContractError('Recovery page requires a nonnegative ordered number')
    cursor = 0
    for source in manifest['sources']:
        count = max(1, (source['byte_count'] + 24575) // 24576)
        if cursor <= number < cursor + count:
            offset = (number - cursor) * 24576
            body = state.read_bytes(directory / source['blob'], raw.MAX_FILE)
            part = body[offset:offset + 24576]
            value = {'schema': 'agent-memory/layout-source-page-v1', 'operation_id': operation_id,
                     'manifest_sha256': digest(encoded(manifest)), 'page': number,
                     'source_id': source['source_id'], 'path': source['path'], 'offset': offset,
                     'byte_count': len(part), 'source_byte_count': len(body), 'encoding': 'base64',
                     'body': base64.b64encode(part).decode('ascii'), 'source_complete': offset + len(part) == len(body),
                     'next_page': number + 1 if number + 1 < sum(max(1, (s['byte_count'] + 24575) // 24576) for s in manifest['sources']) else None}
            value['page_token'] = digest(encoded(value))
            if len(encoded(value)) > limits.MAX_PAGE:
                raise ContractError('Complete recovery page exceeds its encoded bound')
            return value
        cursor += count
    raise ContractError('Recovery page is outside the complete source inventory')


def unit_map_value(value, manifest, intended, retained=None):
    fields(value, {'schema', 'operation_id', 'manifest_sha256', 'units'}, 'agent-memory/layout-unit-map-v1')
    if value['operation_id'] != manifest['operation_id'] or value['manifest_sha256'] != digest(encoded(manifest)) or not isinstance(value['units'], list):
        raise ContractError('Unit map belongs to another raw source observation')
    target_hashes = {path: digest(body.encode()) for path, body in intended.items()}
    target_hashes.update(retained or {})
    sources = {source['source_id']: source for source in manifest['sources']}
    coverage = {key: [] for key in sources}
    seen = set()
    source_pages = {}; page_cursor = 0
    for source in manifest['sources']:
        count = max(1, (source['byte_count'] + 24575) // 24576)
        source_pages[source['source_id']] = list(range(page_cursor, page_cursor + count))
        page_cursor += count
    unit_fields = {'unit_id', 'source_ranges', 'semantic_kind', 'existing_record', 'provenance',
                   'disposition', 'targets', 'reason', 'required_actions', 'context_pages'}
    for unit in value['units']:
        fields(unit, unit_fields)
        if not uuid_valid(unit['unit_id']) or unit['unit_id'] in seen or unit['disposition'] not in DISPOSITIONS:
            raise ContractError('Every semantic unit needs one stable identity and explicit disposition')
        seen.add(unit['unit_id'])
        if (not isinstance(unit['source_ranges'], list) or not unit['source_ranges']
                or not isinstance(unit['reason'], str) or not unit['reason'].strip()
                or not isinstance(unit['required_actions'], list) or not isinstance(unit['provenance'], dict)
                or not isinstance(unit['context_pages'], list) or not unit['context_pages']):
            raise ContractError('Unit disposition needs complete context, reason and real/uncertain provenance')
        if unit['semantic_kind'] not in {'canonical-guidance', 'canonical-framing', 'unadmitted-source', 'non-guidance', 'terminal-history'}:
            raise ContractError('Unknown semantic source kind')
        for source_range in unit['source_ranges']:
            fields(source_range, {'source_id', 'start', 'end'})
            source = sources.get(source_range['source_id'])
            start, end = source_range['start'], source_range['end']
            if (source is None or type(start) is not int or type(end) is not int
                    or not 0 <= start <= end <= source['byte_count'] or (start == end and source['byte_count'] != 0)):
                raise ContractError('Unit map range does not name exact complete original bytes')
            coverage[source_range['source_id']].append((start, end))
            if unit['disposition'] == 'pending-admission' and (source['path'] in {'MEMORY.md', *bank.CONTEXTS}
                    or unit['semantic_kind'] != 'unadmitted-source' or unit['existing_record'] is not None):
                raise ContractError('Already available canonical guidance cannot be demoted into private pending material')
        required_pages = sorted({number for span in unit['source_ranges'] for number in source_pages[span['source_id']]})
        if (any(type(number) is not int for number in unit['context_pages'])
                or unit['context_pages'] != required_pages):
            raise ContractError('Unit review must bind the complete original source context pages')
        if not isinstance(unit['targets'], list):
            raise ContractError('Source mapping requires exact resulting targets')
        for target in unit['targets']:
            fields(target, {'path', 'sha256'})
            if target['path'] not in target_hashes or target['sha256'] != target_hashes[target['path']]:
                raise ContractError('Unit mapping does not bind the exact intended result')
        if unit['disposition'] in {'preserve', 'governed-terminal'} and not unit['targets']:
            raise ContractError('Preserved knowledge requires an available native result')
        if unit['disposition'] == 'pending-admission' and (unit['targets'] or not unit['required_actions']):
            raise ContractError('Pending evidence is neither guidance nor automatically admitted')
        if unit['disposition'] in {'non-guidance-discard', 'governed-terminal'} and unit['semantic_kind'] not in {'non-guidance', 'terminal-history'}:
            raise ContractError('Guidance lifecycle changes require existing governed predicates; preserve/re-layout first')
        if unit['disposition'] == 'governed-terminal' and (
                unit['semantic_kind'] != 'terminal-history' or unit['existing_record'] is None):
            raise ContractError('Governed terminal handling must preserve an already real typed terminal outcome')
    for source_id, ranges in coverage.items():
        ranges.sort(); position = 0
        if not ranges:
            raise ContractError('A retained source was omitted from primary semantic coverage')
        for start, end in ranges:
            if start != position:
                raise ContractError('Primary unit coverage has a gap or overlapping double-count')
            position = end
        if position != sources[source_id]['byte_count']:
            raise ContractError('Primary unit coverage omitted original source bytes')
    return value


def review_value(review, manifest, unit_map, intended, author, metadata, attestation, inherited=()):
    fields(review, {'schema', 'review_id', 'operation_id', 'manifest_sha256', 'unit_map_sha256',
                    'intended_files_sha256', 'metadata_plan_sha256', 'authority_sha256', 'author', 'reviewer', 'reviewed_at',
                    'unit_decisions', 'inherited_frontier_sha256', 'inherited_unit_decisions',
                    'quiescence_attestation_sha256', 'semantic_preservation', 'security', 'identity', 'decision', 'evidence'},
           'agent-memory/layout-review-v1')
    independent(author, review['reviewer'])
    storage.time_value(review['reviewed_at'])
    if (not uuid_valid(review['review_id']) or review['operation_id'] != manifest['operation_id']
            or review['manifest_sha256'] != digest(encoded(manifest))
            or review['unit_map_sha256'] != digest(encoded(unit_map))
            or review['intended_files_sha256'] != digest(encoded(intended))
            or review['metadata_plan_sha256'] != digest(encoded(metadata))
            or review['authority_sha256'] != digest(encoded(manifest['identity']['project_evidence']))
            or review['quiescence_attestation_sha256'] != (digest(encoded(attestation)) if attestation is not None else None)
            or (attestation is not None and (not isinstance(attestation, dict) or attestation.get('review_id') != review['review_id']))
            or review['author'] != author or review['decision'] != 'approve'
            or any(review[key] is not True for key in ('semantic_preservation', 'security', 'identity'))
            or not isinstance(review['evidence'], str) or not review['evidence'].strip()):
        raise ContractError('Independent full-source/result review is missing, stale or not approved')
    decisions = review['unit_decisions']
    if not isinstance(decisions, list) or len(decisions) != len(unit_map['units']):
        raise ContractError('Independent review must decide every source unit')
    expected = {unit['unit_id']: unit['disposition'] for unit in unit_map['units']}
    seen = set()
    for decision in decisions:
        terminal = decision.get('disposition') in {'non-guidance-discard', 'governed-terminal'}
        fields(decision, {'unit_id', 'disposition', 'rationale'} | ({'authority'} if terminal else set()))
        if terminal:
            authority = decision['authority']
            fields(authority, {'developer_id', 'harness', 'session', 'evidence', 'statement'})
            participant({key: authority[key] for key in ('developer_id', 'harness', 'session', 'evidence')})
            if authority['statement'] != 'Explicitly authorize this source-bound non-guidance archive/removal; byte retention alone is not the reason.':
                raise ContractError('Non-guidance disposition requires actual explicit source-bound authority')
        if (decision['unit_id'] in seen or expected.get(decision['unit_id']) != decision['disposition']
                or not isinstance(decision['rationale'], str) or not decision['rationale'].strip()):
            raise ContractError('Independent review omitted/substituted a source disposition')
        seen.add(decision['unit_id'])
    if review['inherited_frontier_sha256'] != digest(encoded(list(inherited))):
        raise ContractError('Independent review must bind the complete inherited private source frontier')
    inherited_decisions = review['inherited_unit_decisions']
    if not isinstance(inherited_decisions, list) or len(inherited_decisions) != len(inherited):
        raise ContractError('Independent review omitted inherited pending obligations')
    seen = set()
    for decision in inherited_decisions:
        fields(decision, {'unit_id', 'disposition', 'rationale'})
        if (decision['unit_id'] in seen or decision['unit_id'] not in {row['unit_id'] for row in inherited}
                or decision['disposition'] != 'retain-pending'
                or not isinstance(decision['rationale'], str) or not decision['rationale'].strip()):
            raise ContractError('Raw repair cannot discharge inherited pending guidance')
        seen.add(decision['unit_id'])
    return review


def preserve_records(directory, manifest, intended, unit_map):
    from . import records
    originals = {}
    for source in manifest['sources']:
        body = state.read_bytes(directory / source['blob'], raw.MAX_FILE)
        try:
            kind, metadata, _ = records.parse(body.decode('utf-8'))
        except (ContractError, UnicodeError, ValueError, KeyError):
            continue
        if kind not in {'standalone-record', 'optional-history-event', 'tombstone'}:
            continue
        key = (metadata.get('id'), metadata.get('version'), kind)
        if key in originals:
            raise ContractError('Ambiguous original record identity requires explicit review')
        originals[key] = metadata
        for unit in unit_map['units']:
            if any(r['source_id'] == source['source_id'] for r in unit['source_ranges']):
                already_terminal = (kind in {'optional-history-event', 'tombstone'}
                                    or metadata.get('status') in {'superseded', 'revoked', 'expired'})
                if (unit['disposition'] not in ({'preserve', 'governed-terminal'} if already_terminal else {'preserve'})
                        or unit['existing_record'] != {'id': metadata['id'], 'version': metadata.get('version')}):
                    raise ContractError('Existing typed guidance/history must retain its actual identity and stay canonical')
    resulting = {}
    for path, body in intended.items():
        try:
            kind, metadata, _ = records.parse(body)
        except (ContractError, ValueError, KeyError):
            continue
        if kind not in {'standalone-record', 'optional-history-event', 'tombstone'}:
            continue
        key = (metadata.get('id'), metadata.get('version'), kind)
        if key not in originals or metadata != originals[key]:
            raise ContractError('Recovery cannot invent record metadata/admission or change stable provenance')
        resulting[key] = metadata
    known = {(identity, version) for identity, version, _ in originals}
    for unit in unit_map['units']:
        if unit['disposition'] == 'governed-terminal' and (
                unit['existing_record'].get('id'), unit['existing_record'].get('version')) not in known:
            raise ContractError('Recovery cannot manufacture terminal predicates for unadmitted source material')
    if resulting != originals:
        raise ContractError('Recovery result lost existing record identities/versions or provenance')


def metadata_plan(value, manifest, intended, retained_source_ids=()):
    fields(value, {'files', 'directories'})
    if not isinstance(value['files'], dict) or set(value['files']) != set(intended) or not isinstance(value['directories'], dict):
        raise ContractError('Required metadata plan must cover the full intended map')
    original = {row['path']: row for row in manifest['physical_entries']}
    for path, facts in value['files'].items():
        fields(facts, {'uid', 'gid', 'mode', 'xattrs'})
        if (any(type(facts[key]) is not int for key in ('uid', 'gid', 'mode')) or not 0 <= facts['mode'] <= 0o777
                or not raw.valid_xattrs(facts['xattrs'])):
            raise ContractError('Recovery metadata requires ordinary exact ownership/mode and supported xattrs')
        old = original.get(path)
        if old and (old['type'] != 'file' or facts != {'uid': old['uid'], 'gid': old['gid'], 'mode': stat.S_IMODE(old['mode']), 'xattrs': old['xattrs']}):
            raise ContractError('Recovery must preserve original required file metadata')
        changed = old is None or old.get('sha256') != digest(intended[path].encode())
        if changed and (facts['uid'] != os.geteuid() or facts['gid'] not in {os.getegid(), *os.getgroups()}):
            raise ContractError('pending-metadata-preservation')
    needed = {str(parent) for path in intended for parent in Path(path).parents if str(parent) != '.' and str(parent) not in original}
    if set(value['directories']) != needed:
        raise ContractError('Recovery must pin every newly created directory')
    for path, facts in value['directories'].items():
        raw.path_value(path); fields(facts, {'uid', 'gid', 'mode', 'xattrs'})
        if (facts['uid'] != os.geteuid() or facts['gid'] not in {os.getegid(), *os.getgroups()}
                or type(facts['mode']) is not int or not 0 <= facts['mode'] <= 0o777 or not raw.valid_xattrs(facts['xattrs'])):
            raise ContractError('pending-metadata-preservation')
    for row in manifest['physical_entries']:
        if (row['type'] == 'file' and not row['runtime'] and row['path'] not in intended
                and not any(source['source_id'] in retained_source_ids and source['path'] == row['path'] for source in manifest['sources'])):
            if row['uid'] != os.geteuid() or row['gid'] not in {os.getegid(), *os.getgroups()} or row['mode'] & 0o7000:
                raise ContractError('pending-metadata-preservation')
    return value


def pending_rows(directory, manifest, units):
    sources = {source['source_id']: source for source in manifest['sources']}
    rows = []
    for unit in units:
        pieces = []
        for span in unit['source_ranges']:
            source = sources[span['source_id']]
            body = state.read_bytes(directory / source['blob'], raw.MAX_FILE)
            pieces.append(body[span['start']:span['end']])
        body = b''.join(pieces)
        if len(body) > limits.MAX_FILE:
            raise ContractError('Indivisible pending unit exceeds ordinary comparison/admission capacity')
        text = body.decode('utf-8')
        security.ensure_safe({'pending-source.md': text})
        rows.append({'unit_id': unit['unit_id'], 'operation_id': manifest['operation_id'], 'body': text,
                     'source_ranges': unit['source_ranges'], 'provenance': unit['provenance'], 'reason': unit['reason'],
                     'selection_class': 'comparison-only-not-current-guidance', 'source_assertions_verified': False})
    return rows






def prepare(repo, operation_id, request):
    root, target, _, _ = state.paths(repo, operation_id)
    from .turn_sources import canonical_writer_barrier
    with canonical_writer_barrier(root), storage.exclusive(target / '.agent-memory.lock', operation_id, 'bank-recover') as owner:
        from . import layout_resolution
        layout_resolution.require_completed_resolutions(root)
        return _prepare(root, operation_id, request, owner)


def _prepare(repo, operation_id, request, owner):
    root, target, _, directory, manifest = operation(repo, operation_id)
    fields(request, {'schema', 'operation_id', 'author', 'unit_map', 'intended_files', 'metadata_plan', 'review',
                     'quiescence_attestation', 'retained_source_ids'}, 'agent-memory/layout-prepare-v1')
    if request['operation_id'] != operation_id:
        raise ContractError('Prepare request belongs to another operation')
    participant(request['author'])
    predecessor, predecessor_pin = predecessor_value(root, manifest, owner)
    inherited = frontier.inherited_pending_rows(root)
    if request['author']['developer_id'] != manifest['identity']['scope_context']['developer_id']:
        raise ContractError('Prepare author must retain the actual detecting developer')
    intended = request['intended_files']
    retained_ids = request['retained_source_ids']
    if (not isinstance(retained_ids, list) or len(set(retained_ids)) != len(retained_ids)
            or any(not uuid_valid(source_id) for source_id in retained_ids)):
        raise ContractError('Retained ordinary occupants require exact source UUIDs')
    retained = {source['path']: source['sha256'] for source in manifest['sources'] if source['source_id'] in retained_ids}
    if len(retained) != len(retained_ids) or any(path.endswith('.md') for path in retained):
        raise ContractError('Retained occupants cannot hide an unsupported canonical Markdown source')
    checked = bank.validate_files(intended)
    if checked['project_id'] != manifest['project_id']:
        raise ContractError('Layout recovery cannot change project identity')
    for path, body in intended.items():
        security.ensure_safe({path: path + '\n' + body})
    unit_map = unit_map_value(request['unit_map'], manifest, intended, retained)
    metadata = metadata_plan(request['metadata_plan'], manifest, intended, retained_ids)
    attestation = request['quiescence_attestation']
    review = review_value(request['review'], manifest, unit_map, intended, request['author'], metadata, attestation, inherited)
    preserve_records(directory, manifest, intended, unit_map)
    bank.file_topology({source['path'] for source in manifest['sources']} | set(intended))
    tx.check_layout(target, {source['path'] for source in manifest['sources']} | set(intended),
                    {source['path']: source['sha256'] for source in manifest['sources']})
    units = initial_units(manifest, unit_map)
    capacity_token = frontier.prospective_token(root, manifest, units, predecessor=predecessor, owner=owner)
    reserve = frontier.reservation(intended, pending_rows(directory, manifest, units) + inherited, capacity_token)
    runtime = artifact.inspect(artifact.PACK)
    # The review above binds even an optional attestation, or explicit null.
    if units or inherited:
        attestation_value(attestation, manifest, runtime)
        if attestation['review_id'] != review['review_id']:
            raise ContractError('Quiescence attestation needs the exact independent review')
    elif attestation is not None:
        attestation_value(attestation, manifest, runtime)
    raw.recheck(root, manifest, owner=owner, transaction=predecessor.marker if predecessor else None)
    originals = {source['path']: source['sha256'] for source in manifest['sources']}
    final = {**retained, **{path: digest(body.encode()) for path, body in intended.items()}}
    actions = [{'path': path, 'before': originals.get(path), 'after': final.get(path),
                'action': 'remove' if path not in final else 'add' if path not in originals else 'replace'}
               for path in sorted(set(originals) | set(final), key=lambda path: (path == 'MEMORY.md', path))
               if originals.get(path) != final.get(path)]
    proposal = {'schema': PROPOSAL, 'proposal_id': str(uuid.uuid4()), 'operation_id': operation_id,
                'repo_root': str(root), 'bank_path': str(target), 'project_id': manifest['project_id'],
                'scope_context': manifest['identity']['scope_context'], 'manifest_sha256': digest(encoded(manifest)),
                'unit_map_sha256': digest(encoded(unit_map)), 'review_id': review['review_id'],
                'review_sha256': digest(encoded(review)), 'review': review, 'author': request['author'],
                'intended_files': intended, 'metadata_plan': metadata, 'pending_units': units,
                'retained_source_ids': retained_ids,
                'inherited_unit_ids': sorted(row['unit_id'] for row in inherited),
                'inherited_frontier_sha256': digest(encoded(inherited)), 'capacity_token': capacity_token,
                'predecessor': predecessor_pin,
                'actions': actions, 'unchanged_occupants': [row for row in manifest['physical_entries']
                    if row['path'] not in {action['path'] for action in actions}],
                'reservation': reserve, 'runtime_artifact_digest': runtime['artifact_digest'],
                'quiescence_attestation': attestation,
                'quiescence_attestation_sha256': digest(encoded(attestation)) if attestation is not None else None,
                'hold_id': str(uuid.uuid4()), 'prepared_at': storage.now()}
    state.write_once(directory / 'unit-map.json', unit_map)
    state.write_once(directory / 'reviews' / (review['review_id'] + '.json'), review)
    if attestation is not None:
        state.write_once(directory / 'attestations' / (attestation['attestation_id'] + '.json'), attestation)
    state.write_once(directory / 'proposals' / (proposal['proposal_id'] + '.json'), proposal)
    return result('prepare', operation_id, state='prepared', verified=True,
                  canonical_eligibility=eligibility(root, owner, predecessor.marker if predecessor else None),
                  temporary_lock_writes_performed=True,
                  pending_unit_ids=capacity_token['unit_ids'], pending_units=len(capacity_token['unit_ids']),
                  private_artifact_writes_performed=True, proposal=proposal,
                  preview={'actions': actions, 'unchanged_occupants': proposal['unchanged_occupants'],
                           'original_source_bytes_retained': manifest['totals']['source_bytes'],
                           'all_source_units': unit_map['units'], 'reservation': reserve})


def proposal_value(repo, supplied):
    proposal = supplied.get('proposal', supplied) if isinstance(supplied, dict) else supplied
    names = {'schema', 'proposal_id', 'operation_id', 'repo_root', 'bank_path', 'project_id', 'scope_context',
             'manifest_sha256', 'unit_map_sha256', 'review_id', 'review_sha256', 'review', 'author', 'intended_files',
             'metadata_plan', 'pending_units', 'retained_source_ids', 'actions', 'unchanged_occupants', 'reservation',
             'runtime_artifact_digest', 'quiescence_attestation', 'quiescence_attestation_sha256', 'hold_id', 'prepared_at'}
    names.update({'inherited_unit_ids', 'inherited_frontier_sha256', 'capacity_token', 'predecessor'})
    fields(proposal, names, PROPOSAL)
    if not all(uuid_valid(proposal[key]) for key in ('proposal_id', 'operation_id', 'project_id', 'review_id', 'hold_id')):
        raise ContractError('Recovery proposal has invalid stable identities')
    root, target, _, directory = state.paths(repo, proposal['operation_id'])
    if proposal['repo_root'] != str(root) or proposal['bank_path'] != str(target):
        raise ContractError('Recovery proposal scope changed')
    if state.read(directory / 'proposals' / (proposal['proposal_id'] + '.json')) != proposal:
        raise ContractError('Recovery apply requires the immutable exact reviewed proposal')
    return proposal


def marker(proposal, kind='apply'):
    return encoded({'schema': frontier.TRANSACTION, 'project_id': proposal['project_id'],
                    'operation_id': proposal['operation_id'], 'transaction_kind': kind})


def hold(proposal):
    return encoded({'schema': frontier.HOLD, 'project_id': proposal['project_id'],
                    'operation_id': proposal['operation_id'], 'hold_id': proposal['hold_id']})


def predecessor_marker(repo, proposal):
    pinned = proposal['predecessor']
    if pinned is None:
        return None
    indexed = state.index(repo)['operations'].get(pinned['operation_id'])
    if indexed is None or indexed['state'] != 'rolled-back' or indexed['project_id'] != proposal['project_id']:
        raise ContractError('The journaled predecessor is no longer a retained same-project rollback')
    directory, _, ledger = frontier.ledger(repo, pinned['operation_id'], indexed)
    frontier.evidence(directory, ledger, indexed)
    previous = state.read(directory / 'proposals' / (indexed['proposal_id'] + '.json'))
    value = hold(previous)
    if digest(value) != pinned['marker_sha256']:
        raise ContractError('The journaled predecessor marker identity changed')
    return value


def marker_payloads(repo, proposal):
    values = [marker(proposal, kind) for kind in ('apply', 'finish', 'rollback')] + [hold(proposal)]
    previous = predecessor_marker(repo, proposal)
    if previous is not None:
        values.append(previous)
    values.append(encoded({'schema': frontier.SENTINEL, 'project_id': proposal['project_id'],
                           'operation_id': proposal['operation_id']}))
    return values


def atomic_marker(target, before, after, owner, *, name=MARKER, transaction=None):
    expected_transaction = before if name == MARKER else transaction
    bank_observation.markers(target, owner, expected_transaction)
    if frontier.marker_bytes(target, name) != before:
        raise ContractError('Owned recovery marker changed before replacement')
    if before is None:
        tx.create_marker(target / name, after)
        return
    operation_id = json.loads(after)['operation_id']
    temporary = '.agent-memory-layout-marker-' + operation_id + '.tmp'
    directory = os.open(target, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    installed = False
    created = False
    try:
        fd = os.open(temporary, os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW, 0o600, dir_fd=directory)
        created = True
        with os.fdopen(fd, 'wb') as stream:
            stream.write(after); stream.flush(); os.fsync(stream.fileno())
        bank_observation.markers(target, owner, expected_transaction)
        if frontier.marker_bytes(target, name) != before:
            raise ContractError('Owned recovery marker changed immediately before swap')
        writes.fault('layout-recovery-before-marker-swap')
        os.replace(temporary, name, src_dir_fd=directory, dst_dir_fd=directory)
        installed = True
        os.fsync(directory)
        writes.fault('layout-recovery-after-marker-swap')
    finally:
        if created and not installed:
            try:
                os.unlink(temporary, dir_fd=directory)
            except FileNotFoundError:
                pass
        os.close(directory)


def view(target, ignore=()):
    physical, _, _ = raw.inventory(target, allow_temporaries=True, ignored_paths=set(ignore))
    ignored = {'.agent-memory.lock', MARKER, SENTINEL, *ignore}
    rows = []
    for row in physical:
        if row['path'] in ignored:
            continue
        if row['type'] == 'directory':
            row = {key: row[key] for key in ('path', 'type', 'runtime', 'dev', 'ino', 'mode', 'uid', 'gid', 'xattrs')}
        rows.append(row)
    return rows


def manifest_view(manifest):
    rows = []
    for item in manifest['physical_entries']:
        if item['path'] in {MARKER, SENTINEL, '.agent-memory.lock'}:
            continue
        row = dict(item)
        if row['type'] == 'directory':
            row = {key: row[key] for key in ('path', 'type', 'runtime', 'dev', 'ino', 'mode', 'uid', 'gid', 'xattrs')}
        rows.append(row)
    return rows


def rebind(root, manifest, owner, transaction):
    target = Path(manifest['bank_path'])
    bank_observation.markers(target, owner, transaction)
    current, _ = raw.read_source(target, 'MEMORY.md')
    identity_review = manifest['identity'].get('identity_review')
    if identity_review is None and transaction is not None:
        current_marker = frontier.marker_value(transaction)
        if current_marker['schema'] == frontier.TRANSACTION and current_marker['operation_id'] == manifest['operation_id']:
            directory = state.paths(root, manifest['operation_id'])[3]
            journal = state.read(directory / 'journal.json')
            if journal.get('parent_operation_id') is not None:
                from . import layout_restore
                child = layout_restore.value(root, state.read(directory / 'proposals' / (journal['proposal_id'] + '.json')))
                _, _, _, parent_dir, parent_manifest, parent, _ = recovery_source(root, child['parent_operation_id'])
                if (digest(encoded(parent)) != child['parent_proposal_sha256']
                        or digest(encoded(receipt_value(parent_dir, parent))) != child['parent_receipt_sha256']):
                    raise ContractError('Child restore raw identity parent evidence changed')
                identity_review = parent_manifest['identity'].get('identity_review')
    resolved = raw.identity(root, current, identity_review, owner=owner, transaction=transaction)
    old = manifest['identity']
    if resolved['scope_context'] != old['scope_context'] or raw.git_pins(root) != manifest['git']:
        raise ContractError('Recovery identity/authority/settings/HEAD/index pins changed')
    binding = {**{key: raw.signature(target.stat())[key] for key in ('dev', 'ino', 'mode', 'uid', 'gid')},
               'xattrs': raw.metadata_supported(target)}
    if binding != manifest['bank_binding']:
        raise ContractError('Recovery bank binding changed')
    # Keep remote/checkout facts, excluding only the router extraction replaced
    # by the reviewed result. No registry/authority conflict is waived.
    ignored = {'router_sha256', 'bank_project_id', 'bank_canonical_remote'}
    if ({key: value for key, value in resolved['project_evidence'].items() if key not in ignored}
            != {key: value for key, value in old['project_evidence'].items() if key not in ignored}):
        raise ContractError('Recovery authority/checkout evidence changed')


def initial_units(manifest, unit_map):
    return [{key: unit[key] for key in ('unit_id', 'source_ranges', 'provenance', 'reason', 'required_actions')}
            | {'detecting_developer_id': manifest['identity']['scope_context']['developer_id'],
               'state': 'pending-admission', 'terminal_receipts': []}
            for unit in unit_map['units'] if unit['disposition'] == 'pending-admission']


def ledger_value(proposal):
    return {'schema': 'agent-memory/layout-pending-ledger-v1', 'operation_id': proposal['operation_id'],
            'project_id': proposal['project_id'], 'manifest_sha256': proposal['manifest_sha256'],
            'unit_map_sha256': proposal['unit_map_sha256'], 'proposal_sha256': digest(encoded(proposal)),
            'generation': 0, 'previous_generation_sha256': None, 'units': proposal['pending_units']}


def check_review(root, directory, manifest, proposal):
    unit_map = state.read(directory / 'unit-map.json')
    review = state.read(directory / 'reviews' / (proposal['review_id'] + '.json'))
    if (digest(encoded(manifest)) != proposal['manifest_sha256'] or digest(encoded(unit_map)) != proposal['unit_map_sha256']
            or digest(encoded(review)) != proposal['review_sha256'] or review != proposal['review']):
        raise ContractError('Recovery reviewed source/result pins changed')
    if (proposal['scope_context'] != manifest['identity']['scope_context']
            or proposal['project_id'] != manifest['project_id']
            or proposal['author']['developer_id'] != manifest['identity']['scope_context']['developer_id']
            or proposal['pending_units'] != initial_units(manifest, unit_map)):
        raise ContractError('Recovery proposal changed its detecting identity or omitted pending source coverage')
    retained = {source['path']: source['sha256'] for source in manifest['sources']
                if source['source_id'] in proposal['retained_source_ids']}
    unit_map_value(unit_map, manifest, proposal['intended_files'], retained)
    inherited = frontier.inherited_pending_rows(root)
    if (proposal['inherited_unit_ids'] != sorted(row['unit_id'] for row in inherited)
            or proposal['inherited_frontier_sha256'] != digest(encoded(inherited))):
        raise ContractError('Recovery inherited private obligations changed')
    review_value(review, manifest, unit_map, proposal['intended_files'], proposal['author'], proposal['metadata_plan'],
                 proposal['quiescence_attestation'], inherited)
    preserve_records(directory, manifest, proposal['intended_files'], unit_map)
    metadata_plan(proposal['metadata_plan'], manifest, proposal['intended_files'], proposal['retained_source_ids'])
    for path, body in proposal['intended_files'].items():
        security.ensure_safe({path: path + '\n' + body})
    if bank.validate_files(proposal['intended_files'])['project_id'] != manifest['project_id']:
        raise ContractError('Recovery intended identity differs')
    runtime_guard(root, manifest, proposal)
    frontier.reservation(proposal['intended_files'],
                         pending_rows(directory, manifest, proposal['pending_units']) + inherited,
                         proposal['capacity_token'])


def ensure_setup(root, target, directory, proposal, owner, predecessor=None):
    current = state.index(root)
    existing = current['operations'].get(proposal['operation_id'])
    ledger = ledger_value(proposal)
    if existing is None:
        sentinel = frontier.install(root, proposal['operation_id'], proposal, ledger, proposal['hold_id'],
                                    owner, predecessor=predecessor)
    else:
        head = {'schema': 'agent-memory/layout-ledger-head-v1', 'operation_id': proposal['operation_id'],
                'generation': 0, 'sha256': digest(encoded(ledger))}
        if existing != {'state': 'active', 'project_id': proposal['project_id'], 'ledger_head_sha256': digest(encoded(head)),
                         'proposal_id': proposal['proposal_id'], 'hold_id': proposal['hold_id']}:
            raise ContractError('Recovery setup conflicts with the current operation generation')
        state.write_once(directory / 'ledger' / '00000000.json', ledger)
        state.write_once(directory / 'ledger-head.json', head)
        state.bind(root, proposal['operation_id'], proposal['project_id'], proposal['scope_context']['checkout_key'], 'active')
        sentinel = {'schema': frontier.SENTINEL, 'project_id': proposal['project_id'], 'operation_id': proposal['operation_id']}
    return encoded(sentinel)


def install_sentinel(root, target, proposal, owner, transaction):
    expected = encoded({'schema': frontier.SENTINEL, 'project_id': proposal['project_id'],
                        'operation_id': proposal['operation_id']})
    present = frontier.marker_bytes(target, SENTINEL)
    if present == expected:
        return
    if proposal['predecessor'] is None:
        previous = None
    else:
        predecessor_marker(root, proposal)
        previous = encoded({'schema': frontier.SENTINEL, 'project_id': proposal['project_id'],
                            'operation_id': proposal['predecessor']['operation_id']})
    if present != previous:
        raise ContractError('Recovery sentinel is unknown or changed')
    writes.fault('layout-recovery-before-sentinel')
    atomic_marker(target, present, expected, owner, name=SENTINEL, transaction=transaction)
    writes.fault('layout-recovery-after-sentinel')


def set_metadata(descriptor, facts):
    after = os.fstat(descriptor)
    if {'uid': after.st_uid, 'gid': after.st_gid, 'mode': stat.S_IMODE(after.st_mode)} != {key: facts[key] for key in ('uid', 'gid', 'mode')}:
        raise ContractError('pending-metadata-preservation')


def replace_source(root, target, path, body, facts, running, context):
    if (type(context) is not RecoveryReadSet or Path(root) != context.root
            or Path(target) != Path(context.manifest['bank_path'])):
        raise ContractError('Raw replacement requires its actual internal checked bank observer')
    owner, transaction = context.owner, context.transaction
    action = next((row for row in context.proposal['actions'] if row['path'] == path), None)
    body_sha = digest(body) if body is not None else None
    if action is None or body_sha not in {action['before'], action['after']}:
        raise ContractError('Raw replacement bytes/path are outside the exact reviewed operation')
    if body is not None:
        if context.proposal['schema'] == 'agent-memory/layout-child-restore-v1':
            rows = context.proposal['base_view'] + context.proposal['desired_view']
            allowed = [{'uid': row['uid'], 'gid': row['gid'], 'mode': stat.S_IMODE(row['mode']), 'xattrs': row['xattrs']}
                       for row in rows if row['path'] == path and row.get('sha256') == body_sha]
        else:
            rows = context.manifest['physical_entries']
            allowed = [{'uid': row['uid'], 'gid': row['gid'], 'mode': stat.S_IMODE(row['mode']), 'xattrs': row['xattrs']}
                       for row in rows if row['path'] == path and row.get('sha256') == body_sha]
            if path in context.proposal['metadata_plan']['files'] and action['after'] == body_sha:
                allowed.append(context.proposal['metadata_plan']['files'][path])
        if facts not in allowed:
            raise ContractError('Raw replacement metadata differs from the reviewed forward/rollback plan')
    raw.path_value(path)
    if view(target) != running:
        raise ContractError('External source/metadata edit observed; preserve interrupted recovery')
    context.check()
    descriptors = []; temporary = None
    try:
        descriptors.append(os.open(target, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW))
        for part in Path(path).parts[:-1]:
            descriptors.append(os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptors[-1]))
        parent = descriptors[-1]
        if body is not None:
            candidate = Path(temps.source_name(json.loads(transaction)['operation_id'], path)).name
            fd = os.open(candidate, os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW, facts['mode'], dir_fd=parent)
            temporary = candidate
            with os.fdopen(fd, 'wb') as stream:
                set_metadata(stream.fileno(), facts)
                stream.write(body); stream.flush(); os.fsync(stream.fileno())
            if raw.metadata_supported(target / Path(path).parent / candidate) != facts['xattrs']:
                raise ContractError('pending-metadata-preservation')
            writes.fault('layout-recovery-temporary-write:' + path)
            temporary_facts = raw.signature(os.stat(candidate, dir_fd=parent, follow_symlinks=False))
        ignored = str(Path(path).parent / temporary) if temporary else None
        if ignored and ignored.startswith('./'):
            ignored = ignored[2:]
        if view(target, [ignored] if ignored else []) != running:
            raise ContractError('Raw read set changed before recovery replacement')
        context.check()
        named = target
        for descriptor, part in zip(descriptors, ('', *Path(path).parts[:-1])):
            if part:
                named = named / part
            opened = os.fstat(descriptor); actual = os.stat(named, follow_symlinks=False)
            if (opened.st_dev, opened.st_ino) != (actual.st_dev, actual.st_ino):
                raise ContractError('Recovery replacement parent binding changed')
        writes.fault('layout-recovery-before-replace:' + path)
        original = next((row for row in running if row['path'] == path), None)
        if original is not None:
            current_hash, current_facts = raw.read_source(target, path, hash_only=True)
            if (current_hash != original['sha256'] or any(current_facts[key] != original[key] for key in current_facts)
                    or raw.metadata_supported(target / path) != original['xattrs']):
                raise ContractError('Recovery source changed immediately before replacement')
        if temporary is not None:
            current_hash, current_facts = raw.read_source(target, ignored, hash_only=True)
            if (current_hash != body_sha or current_facts != temporary_facts
                    or raw.metadata_supported(target / ignored) != facts['xattrs']):
                raise ContractError('Recovery temporary changed immediately before replacement')
        if body is None:
            os.unlink(Path(path).name, dir_fd=parent)
        else:
            os.replace(temporary, Path(path).name, src_dir_fd=parent, dst_dir_fd=parent)
            temporary = None
            if raw.metadata_supported(target / path) != facts['xattrs']:
                raise ContractError('pending-metadata-preservation')
        os.fsync(parent)
        writes.fault('layout-recovery-after-rename:' + path)
    finally:
        if temporary is not None and descriptors:
            os.unlink(temporary, dir_fd=descriptors[-1])
        for descriptor in reversed(descriptors):
            os.close(descriptor)


def new_directories(target, proposal):
    for path, facts in sorted(proposal['metadata_plan']['directories'].items(), key=lambda row: (len(Path(row[0]).parts), row[0])):
        descriptors = [os.open(target, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)]
        try:
            parts = Path(path).parts
            for part in parts[:-1]:
                descriptors.append(os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptors[-1]))
            try:
                os.mkdir(parts[-1], facts['mode'], dir_fd=descriptors[-1])
            except FileExistsError:
                pass
            child = os.open(parts[-1], os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptors[-1])
            try:
                set_metadata(child, facts)
                if raw.metadata_supported(target / path) != facts['xattrs']:
                    raise ContractError('pending-metadata-preservation')
                os.fsync(child); os.fsync(descriptors[-1])
            finally:
                os.close(child)
        finally:
            for descriptor in reversed(descriptors):
                os.close(descriptor)


def expected_result(proposal, manifest, current, *, rollback=False):
    original = {source['path']: source for source in manifest['sources']}
    desired = ({path: {'sha256': source['sha256'], 'facts': next(row for row in manifest['physical_entries'] if row['path'] == path)}
                for path, source in original.items()} if rollback else
               {path: {'sha256': digest(body.encode()), 'facts': proposal['metadata_plan']['files'][path]}
                for path, body in proposal['intended_files'].items()})
    if not rollback:
        for source in manifest['sources']:
            if source['source_id'] in proposal['retained_source_ids']:
                desired[source['path']] = {'sha256': source['sha256'],
                    'facts': next(row for row in manifest['physical_entries'] if row['path'] == source['path'])}
    actual = {row['path']: row for row in current if row['type'] == 'file' and not row['runtime']}
    if set(actual) != set(desired):
        raise ContractError('Recovery result contains missing or unknown source occupants')
    for path, value in desired.items():
        row, facts = actual[path], value['facts']
        mode = stat.S_IMODE(facts['mode'])
        if (row['sha256'] != value['sha256'] or row['xattrs'] != facts['xattrs']
                or (row['uid'], row['gid'], stat.S_IMODE(row['mode'])) != (facts['uid'], facts['gid'], mode)):
            raise ContractError('Recovery readback bytes/required metadata do not match the reviewed outcome')
    # Every runtime aid and preexisting directory remains physically bound.
    original_rows = {row['path']: row for row in manifest_view(manifest)}
    current_rows = {row['path']: row for row in current}
    for path, row in original_rows.items():
        if row['runtime'] or row['type'] == 'directory':
            if current_rows.get(path) != row:
                raise ContractError('Unrelated physical/runtime occupant changed')
    extra_dirs = {path for path, row in current_rows.items() if row['type'] == 'directory'} - {path for path, row in original_rows.items() if row['type'] == 'directory'}
    if extra_dirs != (set() if rollback else set(proposal['metadata_plan']['directories'])):
        raise ContractError('Unknown directory introduced into recovery result')
    for path in extra_dirs:
        row, facts = current_rows[path], proposal['metadata_plan']['directories'][path]
        if (row['uid'], row['gid'], stat.S_IMODE(row['mode']), row['xattrs']) != (facts['uid'], facts['gid'], facts['mode'], facts['xattrs']):
            raise ContractError('Recovery directory metadata readback differs from the reviewed plan')
    return True


def receipt_value(directory, proposal):
    value = state.read(directory / 'receipt.json', missing=True)
    if value is None:
        return None
    if (value.get('schema') != RECEIPT or value.get('operation_id') != proposal['operation_id']
            or value.get('project_id') != proposal['project_id'] or value.get('request_sha256') != digest(encoded(proposal))
            or value.get('proposal_id') != proposal['proposal_id'] or value.get('verified') is not True
            or value.get('outcome') not in {'layout-restored', 'rolled-back'}):
        raise ContractError('Recovery historical receipt is corrupt or operation ID was reused')
    return value


def historical(repo, proposal, directory, receipt):
    # Readback deliberately precedes original/result source demands. Later edits
    # never cause an exact completed request to replay over current knowledge.
    layout_history.check(repo, proposal['scope_context'], proposal['project_id'])
    current_match = 'unavailable'
    if (directory / 'manifest.json').exists():
        try:
            _, _, _, _, manifest = operation(repo, proposal['operation_id'])
            expected_result(proposal, manifest, view(Path(proposal['bank_path'])), rollback=receipt['outcome'] == 'rolled-back')
            current_match = view(Path(proposal['bank_path'])) == receipt['resulting_view']
        except (ContractError, OSError, UnicodeError, ValueError):
            current_match = False
    return result('apply', proposal['operation_id'], state=receipt['outcome'], verified=True,
                  receipt=receipt, receipt_is_historical=True, already_applied=True,
                  current_receipt_match=current_match, canonical_eligibility=eligibility(repo),
                  semantic_recovery_complete=False, pending_unit_ids=receipt['pending_unit_ids_at_commit'],
                  pending_units=len(receipt['pending_unit_ids_at_commit']),
                  required_actions=['Consult current status; historical receipt does not assert current semantic coverage'])


def finish_index(root, proposal, outcome, owner, transaction):
    current = state.index(root); _, target, base, _ = state.paths(root)
    indexed = current['operations'][proposal['operation_id']]
    pending = bool(proposal['pending_units'] or proposal['inherited_unit_ids'])
    indexed['state'] = 'rolled-back' if outcome == 'rolled-back' else 'active' if pending else 'terminal'
    state.set_index(root, current, digest(state.read_bytes(base / 'index.json')))
    state.bind(root, proposal['operation_id'], proposal['project_id'], proposal['scope_context']['checkout_key'],
               'active' if pending else 'terminal')
    writes.fault('layout-recovery-final-index')
    bank_observation.markers(target, owner, transaction)
    if pending:
        atomic_marker(target, transaction, hold(proposal), owner)
    else:
        (target / MARKER).unlink(); tx.flush_directory(target)
        writes.fault('layout-recovery-marker-removal')
        expected = encoded({'schema': frontier.SENTINEL, 'project_id': proposal['project_id'], 'operation_id': proposal['operation_id']})
        if frontier.marker_bytes(target, SENTINEL) != expected:
            raise ContractError('Sentinel changed before retirement')
        writes.fault('layout-recovery-before-sentinel-retirement')
        (target / SENTINEL).unlink(); tx.flush_directory(target)
        writes.fault('layout-recovery-sentinel-retirement')
    frontier.invalidate(root)


def run_journal(root, target, directory, manifest, proposal, owner, transaction, *, mode='finish'):
    context = RecoveryReadSet.observe(root, manifest, proposal, owner, transaction)
    current = view(target)
    running = current
    if mode == 'finish':
        boundary = partial_boundary(proposal, manifest, current)
        new_directories(target, proposal)
        running = view(target)
        desired = proposal['intended_files']
        actions = proposal['actions'][boundary:]
    else:
        plan = state.read(directory / 'rollback-plan.json')
        boundary = rollback_boundary(plan, manifest, current)
        sources = {source['path']: source for source in manifest['sources']}
        desired = sources
        actions = plan['actions'][boundary:]
    for number, action in enumerate(actions):
        path = action['path']
        if path == 'MEMORY.md':
            writes.fault('layout-recovery-before-router')
        body = desired.get(path)
        if mode == 'rollback' and body is not None:
            body = state.read_bytes(directory / body['blob'], raw.MAX_FILE)
        elif isinstance(body, str):
            body = body.encode()
        if mode == 'finish':
            facts = proposal['metadata_plan']['files'].get(path)
        else:
            old = next((row for row in manifest['physical_entries'] if row['path'] == path), None)
            facts = {'uid': old['uid'], 'gid': old['gid'], 'mode': stat.S_IMODE(old['mode']), 'xattrs': old['xattrs']} if old else None
        replace_source(root, target, path, body, facts, running, context)
        running = view(target)
        progress = {'schema': 'agent-memory/layout-recovery-progress-v1', 'operation_id': proposal['operation_id'],
                    'mode': mode, 'path': path, 'observed_entry': next((row for row in running if row['path'] == path), None),
                    'inventory_sha256': digest(encoded(running)), 'observed_at': storage.now()}
        # Progress is evidence, not authority for an impossible partial ordering.
        state.write_once(directory / 'progress' / (mode + '-' + str(boundary + number).zfill(8) + '.json'), progress)
        writes.fault('layout-recovery-after-progress:' + path)
        if path == 'MEMORY.md':
            writes.fault('layout-recovery-after-router')
    if mode == 'rollback':
        for path in sorted(proposal['metadata_plan']['directories'], key=lambda p: (-len(Path(p).parts), p)):
            location = target / path
            if location.exists():
                if location.is_symlink() or not location.is_dir() or any(location.iterdir()):
                    raise ContractError('Created directory gained an unrelated occupant; preserve it')
                info = location.stat(follow_symlinks=False)
                facts = proposal['metadata_plan']['directories'][path]
                if ({'uid': info.st_uid, 'gid': info.st_gid, 'mode': stat.S_IMODE(info.st_mode),
                     'xattrs': raw.metadata_supported(location)} != facts):
                    raise ContractError('Recovery directory metadata changed before rollback removal')
                location.rmdir(); tx.flush_directory(location.parent)
        running = view(target)
    writes.fault('layout-recovery-before-final-readback')
    expected_result(proposal, manifest, running, rollback=mode == 'rollback')
    context.check()
    eligible = eligibility(root, owner, transaction)
    if mode == 'finish' and eligible is not True:
        raise ContractError('Intended native result failed the common complete bank eligibility check')
    writes.fault('layout-recovery-before-receipt')
    units = sorted([unit['unit_id'] for unit in proposal['pending_units']] + proposal['inherited_unit_ids'])
    receipt = {'schema': RECEIPT, 'operation_id': proposal['operation_id'], 'project_id': proposal['project_id'],
               'proposal_id': proposal['proposal_id'], 'request_sha256': digest(encoded(proposal)),
               'original_manifest_sha256': proposal['manifest_sha256'],
               'intended_manifest_sha256': digest(encoded(proposal['intended_files'])),
               'outcome': 'layout-restored' if mode == 'finish' else 'rolled-back',
               'readback_at': storage.now(), 'verified': True, 'byte_metadata_readback_verified': True,
               'exact_xattr_readback_verified': True,
               'resulting_view': running,
               'changed_memory_paths': [path['path'] for path in proposal['actions']],
               'canonical_eligibility_at_commit': eligible, 'pending_unit_ids_at_commit': units,
               'semantic_recovery_complete_at_commit': mode == 'finish' and not units,
               'original_index_writes_performed': False, 'head_writes_performed': False, 'feature_writes_performed': False}
    state.write_once(directory / 'receipt.json', receipt)
    writes.fault('layout-recovery-after-receipt')
    finish_index(root, proposal, receipt['outcome'], owner, transaction)
    return receipt


def partial_boundary(proposal, manifest, current):
    """Only an ordered action prefix (including rename-before-progress) is legal."""
    before = {row['path']: row for row in manifest_view(manifest)}
    actual = {row['path']: row for row in current}
    planned_dirs = set(proposal['metadata_plan']['directories'])
    for path, row in actual.items():
        if path not in before and path not in proposal['intended_files'] and path not in planned_dirs:
            raise ContractError('Interrupted recovery contains an unknown physical occupant')
        if path in planned_dirs and (row['type'] != 'directory' or
                {'uid': row['uid'], 'gid': row['gid'], 'mode': stat.S_IMODE(row['mode']), 'xattrs': row['xattrs']} != proposal['metadata_plan']['directories'][path]):
            raise ContractError('Interrupted recovery directory metadata changed')
    def equivalent(observed, expected, changed):
        if expected is None:
            return observed is None
        if observed is None:
            return False
        if not changed:
            return observed == expected
        return (observed['type'] == 'file' and observed.get('sha256') == expected.get('sha256')
                and (observed['uid'], observed['gid'], stat.S_IMODE(observed['mode']), observed['nlink'], observed['xattrs'])
                == (expected['uid'], expected['gid'], stat.S_IMODE(expected['mode']), 1, expected['xattrs']))
    changed = set()
    participants = set(before) | set(proposal['intended_files'])
    for boundary in range(len(proposal['actions']) + 1):
        if all(equivalent(actual.get(path), before.get(path), path in changed) for path in participants):
            return boundary
        if boundary < len(proposal['actions']):
            action = proposal['actions'][boundary]; path = action['path']; changed.add(path)
            if action['after'] is None:
                before.pop(path, None)
            else:
                facts = proposal['metadata_plan']['files'][path]
                before[path] = {'type': 'file', 'sha256': action['after'], 'uid': facts['uid'],
                                'gid': facts['gid'], 'mode': facts['mode'], 'xattrs': facts['xattrs']}
    raise ContractError('Interrupted recovery is not an attainable ordered action boundary')


def probe_requirements(proposal, manifest, actions, *, mode):
    old = {row['path']: row for row in manifest['physical_entries'] if row['type'] == 'file'}
    result = []
    for action in actions:
        path = action['path']
        needs_old = (mode == 'initial' and action.get('before') is not None
                     or mode == 'rollback' and action.get('after') is not None)
        if needs_old and path in old:
            row = old[path]
            result.append({'path': path, 'facts': {'uid': row['uid'], 'gid': row['gid'],
                           'mode': stat.S_IMODE(row['mode']), 'xattrs': row['xattrs']}})
        if mode in ('initial', 'finish') and action.get('after') is not None:
            result.append({'path': path, 'facts': proposal['metadata_plan']['files'][path]})
    return result


def apply(repo, supplied):
    proposal = proposal_value(repo, supplied)
    root, target, _, directory = state.paths(repo, proposal['operation_id'])
    known = receipt_value(directory, proposal)
    if known is not None:
        return historical(root, proposal, directory, known)
    _, _, _, _, manifest = operation(root, proposal['operation_id'])
    from .turn_sources import canonical_writer_barrier
    with canonical_writer_barrier(root), storage.exclusive(target / '.agent-memory.lock', proposal['operation_id'], 'bank-recover') as owner:
        from . import layout_resolution
        layout_resolution.require_completed_resolutions(root)
        predecessor, predecessor_pin = predecessor_value(root, manifest, owner)
        if proposal['predecessor'] != predecessor_pin:
            raise ContractError('Reviewed recovery predecessor changed')
        raw.recheck(root, manifest, owner, predecessor.marker if predecessor else None)
        check_review(root, directory, manifest, proposal)
        if frontier.prospective_token(root, manifest, proposal['pending_units'], predecessor=predecessor,
                                      owner=owner) != proposal['capacity_token']:
            raise ContractError('Actual complete recovery reservation changed before installation')
        transaction = marker(proposal)
        requirements = probe_requirements(proposal, manifest, proposal['actions'], mode='initial')
        probes = temps.probe_plan(proposal['operation_id'], requirements, proposal['metadata_plan']['directories'])
        journal = {'schema': JOURNAL, 'operation_id': proposal['operation_id'], 'proposal_id': proposal['proposal_id'],
                   'proposal_sha256': digest(encoded(proposal)), 'manifest_sha256': proposal['manifest_sha256'],
                   'review_sha256': proposal['review_sha256'], 'marker': json.loads(transaction),
                   'hold': json.loads(hold(proposal)), 'actions': proposal['actions'],
                   'original_view': manifest_view(manifest), 'initial_ledger_sha256': digest(encoded(ledger_value(proposal))),
                   'parent_operation_id': None, 'parent_receipt_sha256': None, 'metadata_probes': probes}
        writes.fault('layout-recovery-before-journal')
        state.write_once(directory / 'journal.json', journal)
        writes.fault('layout-recovery-after-journal')
        writes.fault('layout-recovery-probe-journal')
        temps.probe(target, probes, directory, owner=owner, transaction=predecessor.marker if predecessor else None)
        rebind(root, manifest, owner, predecessor.marker if predecessor else None)
        if view(target) != manifest_view(manifest):
            raise ContractError('Recovery source changed during metadata preflight')
        ensure_setup(root, target, directory, proposal, owner, predecessor)
        writes.fault('layout-recovery-before-marker')
        before = predecessor.marker if predecessor else None
        if predecessor is None and (proposal['pending_units'] or proposal['inherited_unit_ids']):
            atomic_marker(target, None, hold(proposal), owner)
            writes.fault('layout-recovery-after-hold')
            before = hold(proposal)
        atomic_marker(target, before, transaction, owner)
        install_sentinel(root, target, proposal, owner, transaction)
        writes.fault('layout-recovery-after-marker')
        receipt = run_journal(root, target, directory, manifest, proposal, owner, transaction)
    recalled = recall_after(root)
    return result('apply', proposal['operation_id'], state=receipt['outcome'], verified=True,
                  canonical_eligibility=receipt['canonical_eligibility_at_commit'],
                  semantic_recovery_complete=receipt['semantic_recovery_complete_at_commit'],
                  pending_unit_ids=receipt['pending_unit_ids_at_commit'], pending_units=len(receipt['pending_unit_ids_at_commit']),
                  writes_performed=bool(proposal['actions']), canonical_writes_performed=bool(proposal['actions']),
                  history_writes_performed=any(action['path'].startswith('history/') for action in proposal['actions']),
                  private_artifact_writes_performed=True, temporary_lock_writes_performed=True,
                  receipt=receipt, receipt_is_historical=False, current_receipt_match=True, recall=recalled)


def recall_after(repo):
    """Actual ordinary qualified recall is separate from the durable layout receipt."""
    from . import retrieval
    try:
        token = frontier.observe(repo)
        request = {'project_id': token['project_id'], 'query': '', 'ids': [], 'paths': [], 'context': {},
                   'kinds': ['semantic-fact', 'user-feedback', 'project-decision', 'episodic-attempt', 'result', 'reference', 'procedural-candidate'],
                   'minimum_trust': 'reviewed', 'limit': 8, 'include_context': False}
        value = retrieval.retrieve(repo, request)
        return {'state': 'evaluated', 'result': value}
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, AttributeError):
        return {'state': 'pending', 'required_actions': ['Run ordinary qualified recall with applicable context; the historical layout receipt remains committed']}


def recovery_source(repo, operation_id):
    root, target, base, directory, manifest = operation(repo, operation_id)
    journal = state.read(directory / 'journal.json')
    if journal.get('schema') != JOURNAL or journal.get('operation_id') != operation_id:
        raise ContractError('Known recovery requires its exact typed journal')
    proposal = state.read(directory / 'proposals' / (journal['proposal_id'] + '.json'))
    proposal_value(root, proposal)
    if (journal.get('proposal_sha256') != digest(encoded(proposal))
            or journal.get('manifest_sha256') != digest(encoded(manifest))
            or journal.get('review_sha256') != proposal['review_sha256']
            or journal.get('metadata_probes') != temps.probe_plan(proposal['operation_id'],
                probe_requirements(proposal, manifest, proposal['actions'], mode='initial'), proposal['metadata_plan']['directories'])):
        raise ContractError('Known recovery journal/proposal/source pins do not agree')
    return root, target, base, directory, manifest, proposal, journal


def recovery_plan(directory, proposal, manifest, current, mode):
    if mode == 'finish':
        partial_boundary(proposal, manifest, current)
        return None
    active = state.read(directory / 'rollback-plan.json', missing=True)
    if active is not None:
        fields(active, {'schema', 'operation_id', 'proposal_sha256', 'initial_view', 'actions'},
               'agent-memory/layout-rollback-plan-v1')
        if active['operation_id'] != proposal['operation_id'] or active['proposal_sha256'] != digest(encoded(proposal)):
            raise ContractError('Rollback plan is foreign or corrupt')
        rollback_boundary(active, manifest, current)
        return active
    partial_boundary(proposal, manifest, current)
    original = {source['path']: source for source in manifest['sources']}
    now = {row['path']: row for row in current if row['type'] == 'file' and not row['runtime']}
    actions = [{'path': path, 'after': original[path]['sha256'] if path in original else None}
               for path in sorted(set(original) | set(now), key=lambda path: (path == 'MEMORY.md', path))
               if (original[path]['sha256'] if path in original else None) != now.get(path, {}).get('sha256')]
    return {'schema': 'agent-memory/layout-rollback-plan-v1', 'operation_id': proposal['operation_id'],
            'proposal_sha256': digest(encoded(proposal)), 'initial_view': current, 'actions': actions}


def rollback_boundary(plan, manifest, current):
    actual = {row['path']: row for row in current}
    expected = {row['path']: row for row in plan['initial_view']}
    original = {row['path']: row for row in manifest['physical_entries']}
    participants = set(actual) | set(expected)
    changed = set()
    for boundary in range(len(plan['actions']) + 1):
        valid = True
        for path in participants:
            row, desired = actual.get(path), expected.get(path)
            if path not in changed:
                if row != desired and not (boundary == len(plan['actions']) and desired and desired['type'] == 'directory'
                                           and path not in original and row is None):
                    valid = False; break
            elif desired is None:
                if row is not None:
                    valid = False; break
            elif row is None or (row.get('sha256'), row['uid'], row['gid'], stat.S_IMODE(row['mode']), row['nlink'], row['xattrs']) != (
                    desired.get('sha256'), desired['uid'], desired['gid'], stat.S_IMODE(desired['mode']), 1, desired['xattrs']):
                valid = False; break
        if valid:
            return boundary
        if boundary < len(plan['actions']):
            action = plan['actions'][boundary]; path = action['path']; changed.add(path)
            if action['after'] is None:
                expected.pop(path, None)
            else:
                expected[path] = original[path]
    raise ContractError('Interrupted rollback contains an impossible ordering or external edit')


def recover(repo, operation_id=None, mode=None, proposal=None):
    from . import layout_restore
    if proposal is not None and proposal.get('proposal', proposal).get('schema') == 'agent-memory/layout-child-recovery-v1':
        return layout_restore.recover(repo, proposal=proposal)
    if proposal is None and operation_id is not None:
        _, _, _, known_dir = state.paths(repo, operation_id)
        known_journal = state.read(known_dir / 'journal.json')
        if known_journal.get('parent_operation_id') is not None:
            return layout_restore.recover(repo, operation_id, mode)
    if proposal is None:
        root, target, base, directory, manifest, original, journal = recovery_source(repo, operation_id)
        if mode not in {'finish', 'rollback'}:
            raise ContractError('Known recovery needs an explicit finish or rollback mode')
        current_marker = frontier.marker_bytes(target)
        bank_observation.markers(target, None, current_marker)
        allowed = set(marker_payloads(root, original))
        if current_marker is not None and current_marker not in allowed:
            raise ContractError('Unknown operation marker; preserve it')
        temporary_entries = temps.entries(target, original['operation_id'], original['actions'], manifest_view(manifest),
            original['metadata_plan']['files'], marker_payloads(root, original))
        probe_entries = temps.probe_entries(target, journal['metadata_probes'], directory)
        current = view(target, [pin['path'] for pin in temporary_entries + probe_entries])
        known = receipt_value(directory, original)
        if known is not None:
            if mode == 'rollback' and known['outcome'] == 'layout-restored':
                return rollback(repo, operation_id)
            if (mode == 'finish') != (known['outcome'] == 'layout-restored'):
                raise ContractError('A durable historical outcome cannot be reversed by interrupted recovery')
            expected_result(original, manifest, current, rollback=known['outcome'] == 'rolled-back')
            if current != known['resulting_view']:
                raise ContractError('Durable receipt result changed; do not replay cleanup over later edits')
        else:
            recovery_plan(directory, original, manifest, current, mode)
        rebind(root, manifest, None, current_marker)
        if mode == 'finish':
            check_review(root, directory, manifest, original)
        preview = {'schema': 'agent-memory/layout-known-recovery-v1', 'recovery_id': str(uuid.uuid4()),
                   'operation_id': original['operation_id'], 'repo_root': str(root), 'mode': mode,
                   'proposal_sha256': digest(encoded(original)), 'journal_sha256': digest(encoded(journal)),
                   'marker_sha256': digest(current_marker), 'sentinel_sha256': digest(frontier.marker_bytes(target, SENTINEL)),
                   'index_sha256': digest(state.read_bytes(base / 'index.json', missing=True)),
                   'ledger_head_sha256': digest(state.read_bytes(directory / 'ledger-head.json', missing=True)),
                   'receipt_sha256': digest(encoded(known)) if known is not None else None,
                   'observed_view': view(target, [pin['path'] for pin in probe_entries]), 'temporary_entries': temporary_entries, 'probe_entries': probe_entries,
                   'git': raw.git_pins(root),
                   'rollback_plan': recovery_plan(directory, original, manifest, current, mode) if known is None else None}
        state.write_once(directory / 'recovery' / (preview['recovery_id'] + '.json'), preview)
        return result('recover', operation_id, state='recovery-preview', verified=True, proposal=preview,
                      private_artifact_writes_performed=True, canonical_eligibility=eligibility(repo),
                      required_actions=['Apply this exact known-operation preview under the bank lock'])
    preview = proposal.get('proposal', proposal)
    fields(preview, {'schema', 'recovery_id', 'operation_id', 'repo_root', 'mode', 'proposal_sha256', 'journal_sha256',
                     'marker_sha256', 'sentinel_sha256', 'index_sha256', 'ledger_head_sha256', 'receipt_sha256',
                     'observed_view', 'temporary_entries', 'probe_entries', 'git', 'rollback_plan'}, 'agent-memory/layout-known-recovery-v1')
    root, target, base, directory = state.paths(repo, preview['operation_id'])
    if not uuid_valid(preview['recovery_id']) or state.read(directory / 'recovery' / (preview['recovery_id'] + '.json')) != preview:
        raise ContractError('Known recovery preview is unknown or changed')
    completed = layout_history.completed(directory, preview)
    if completed is not None:
        original = state.read(directory / 'proposals' / (completed['proposal_id'] + '.json'))
        proposal_value(root, original)
        layout_history.check(root, original['scope_context'], original['project_id'])
        return result('recover', preview['operation_id'], state=completed['outcome'], verified=True,
                      receipt=completed, receipt_is_historical=True, already_applied=True)
    root, target, base, directory, manifest, original, journal = recovery_source(repo, preview['operation_id'])
    from .turn_sources import canonical_writer_barrier
    with canonical_writer_barrier(root), storage.exclusive(target / '.agent-memory.lock', original['operation_id'], 'bank-recover') as owner:
        from . import layout_resolution
        layout_resolution.require_completed_resolutions(root)
        current_marker = frontier.marker_bytes(target)
        known = receipt_value(directory, original)
        temporary_entries = temps.entries(target, original['operation_id'], original['actions'], manifest_view(manifest),
            original['metadata_plan']['files'], marker_payloads(root, original))
        probe_entries = temps.probe_entries(target, journal['metadata_probes'], directory)
        checks = {'repo_root': str(root), 'proposal_sha256': digest(encoded(original)),
                  'journal_sha256': digest(encoded(journal)), 'marker_sha256': digest(current_marker),
                  'sentinel_sha256': digest(frontier.marker_bytes(target, SENTINEL)),
                  'index_sha256': digest(state.read_bytes(base / 'index.json', missing=True)),
                  'ledger_head_sha256': digest(state.read_bytes(directory / 'ledger-head.json', missing=True)),
                  'receipt_sha256': digest(encoded(known)) if known is not None else None,
                  'observed_view': view(target, [pin['path'] for pin in probe_entries]), 'temporary_entries': temporary_entries, 'probe_entries': probe_entries, 'git': raw.git_pins(root)}
        if any(preview[key] != value for key, value in checks.items()):
            raise ContractError('Known recovery preview became stale; preserve newer state')
        rebind(root, manifest, owner, current_marker)
        temps.clean(target, temporary_entries, owner, current_marker)
        temps.probe_clean(target, probe_entries, owner, current_marker)
        temps.probe_complete(target, journal['metadata_probes'], directory)
        cleaned_view = view(target)
        if known is not None:
            expected_result(original, manifest, cleaned_view, rollback=known['outcome'] == 'rolled-back')
            if cleaned_view != known['resulting_view']:
                raise ContractError('Durable receipt result changed; do not replay cleanup over later edits')
            if current_marker is None and (frontier.marker_bytes(target, SENTINEL) is not None
                    or state.index(root)['operations'][original['operation_id']]['state'] == 'active'):
                current_marker = marker(original, preview['mode'])
                atomic_marker(target, None, current_marker, owner)
            if current_marker is not None:
                finish_index(root, original, known['outcome'], owner, current_marker)
            receipt = known
        else:
            if preview['mode'] == 'finish':
                check_review(root, directory, manifest, original)
            else:
                plan = recovery_plan(directory, original, manifest, cleaned_view, 'rollback')
                if plan != preview['rollback_plan']:
                    raise ContractError('Rollback action plan changed')
                state.write_once(directory / 'rollback-plan.json', plan)
            if preview['mode'] == 'finish':
                boundary = partial_boundary(original, manifest, cleaned_view)
                remaining = original['actions'][boundary:]
                directories = [path for path in original['metadata_plan']['directories'] if not (target / path).exists()]
            else:
                boundary = rollback_boundary(plan, manifest, cleaned_view)
                remaining = plan['actions'][boundary:]
                directories = []
            requirements = probe_requirements(original, manifest, remaining, mode=preview['mode'])
            temps.probe(target, journal['metadata_probes'], directory, requirements=requirements, directories=directories,
                        owner=owner, transaction=current_marker)
            if view(target) != cleaned_view:
                raise ContractError('Recovery sources changed during metadata preflight')
            before_setup = predecessor_marker(root, original)
            predecessor = None
            if original['operation_id'] not in state.index(root)['operations']:
                predecessor, _ = predecessor_value(root, manifest, owner)
            ensure_setup(root, target, directory, original, owner, predecessor)
            transaction = marker(original, preview['mode'])
            if current_marker != transaction:
                if current_marker is None and (original['pending_units'] or original['inherited_unit_ids']):
                    atomic_marker(target, None, hold(original), owner)
                    current_marker = hold(original)
                if current_marker == before_setup and before_setup is not None:
                    predecessor_marker(root, original)
                atomic_marker(target, current_marker, transaction, owner)
            install_sentinel(root, target, original, owner, transaction)
            receipt = run_journal(root, target, directory, manifest, original, owner, transaction, mode=preview['mode'])
        state.write_once(directory / 'recovery' / (preview['recovery_id'] + '.receipt.json'),
                         layout_history.recovery_receipt(directory, preview, receipt))
    return result('recover', original['operation_id'], state=receipt['outcome'], verified=True,
                  receipt=receipt, canonical_eligibility=receipt['canonical_eligibility_at_commit'],
                  semantic_recovery_complete=receipt['semantic_recovery_complete_at_commit'],
                  pending_unit_ids=receipt['pending_unit_ids_at_commit'], pending_units=len(receipt['pending_unit_ids_at_commit']),
                  canonical_writes_performed=known is None and bool(original['actions']), private_artifact_writes_performed=True,
                  temporary_lock_writes_performed=True)


def status(repo, operation_id=None):
    root, target, _, directory = state.paths(repo, operation_id)
    if operation_id is None:
        indexed = state.index(root)
        return result('status', state='observed', verified=True, canonical_eligibility=eligibility(root),
                      operations=[{'operation_id': key, 'state': row['state']} for key, row in sorted(indexed['operations'].items())])
    tombstone = state.read(directory / 'tombstone.json', missing=True)
    receipt = state.read(directory / 'receipt.json', missing=True)
    if tombstone is not None:
        return result('status', operation_id, state='retention-cleaned' if tombstone.get('cleanup_complete') is True else 'cleanup-interrupted',
                      verified=True, receipt_is_historical=True,
                      original_evidence_available=False if tombstone.get('cleanup_complete') is True else 'unknown',
                      canonical_eligibility=eligibility(root),
                      historical_outcome=receipt.get('outcome') if receipt else None)
    if not (directory / 'manifest.json').exists():
        staging = state.read(directory / 'staging.json')
        if staging.get('operation_id') != operation_id or staging.get('observation_complete') is not False:
            raise ContractError('Unknown incomplete observation evidence')
        return result('status', operation_id, state='observation-incomplete', verified=True, observation_complete=False,
                      required_actions=['Preserve the partial private staging evidence; inspect a new complete operation before prepare'])
    _, _, _, _, manifest = operation(root, operation_id)
    journal = state.read(directory / 'journal.json', missing=True)
    if journal is not None and 'metadata_probes' in journal:
        temps.probe_entries(target, journal['metadata_probes'], directory)
    if journal is not None and journal.get('parent_operation_id') is not None:
        return result('status', operation_id, state=receipt['outcome'] if receipt else 'interrupted',
                      verified=True, receipt_is_historical=receipt is not None,
                      parent_operation_id=journal['parent_operation_id'], canonical_eligibility=eligibility(root))
    indexed = state.index(root)['operations'].get(operation_id)
    unit_ids = []; complete = False
    if indexed is not None:
        _, _, current = frontier.ledger(root, operation_id, indexed)
        frontier.evidence(directory, current, indexed)
        unit_ids = [unit['unit_id'] for unit in current['units'] if unit['state'] == 'pending-admission']
        complete = not unit_ids and indexed['state'] == 'terminal'
    current_match = 'unavailable'
    if receipt is not None and journal is not None:
        original = state.read(directory / 'proposals' / (journal['proposal_id'] + '.json'))
        receipt_value(directory, original)
        try:
            expected_result(original, manifest, view(target), rollback=receipt['outcome'] == 'rolled-back')
            current_match = view(target) == receipt['resulting_view']
        except (ContractError, OSError, ValueError):
            current_match = False
    current_marker = frontier.marker_bytes(target)
    if indexed is not None and indexed['state'] in {'active', 'rolled-back', 'terminal'} and (
            current_marker is None or json.loads(current_marker).get('schema') == frontier.HOLD):
        current_frontier = frontier.observe(root)
        unit_ids = current_frontier['unit_ids']
        complete = complete and current_frontier['coverage_complete']
    stage = ('interrupted' if current_marker is not None and json.loads(current_marker).get('schema') != frontier.HOLD
             else receipt['outcome'] if receipt else 'journaled' if journal else 'observed')
    eligible = eligibility(root)
    complete = complete and eligible is True
    return result('status', operation_id, state=stage, verified=True, observation_complete=True,
                  canonical_eligibility=eligible, semantic_recovery_complete=complete,
                  pending_unit_ids=unit_ids, pending_units=len(unit_ids), current_receipt_match=current_match,
                  receipt_is_historical=receipt is not None,
                  required_actions=[] if complete else ['Retain originals and complete operation-specific review/recovery or pending-unit actions'])


def rollback(repo, operation_id=None, proposal=None):
    from . import layout_restore
    if proposal is None:
        return layout_restore.preview(repo, operation_id)
    value = proposal.get('proposal', proposal)
    if value.get('schema') == 'agent-memory/layout-known-recovery-v1':
        return recover(repo, proposal=value)
    return layout_restore.apply(repo, value)


def safe_envelope(value, exported=None):
    operation_id = value.get('operation_id')
    answer = result(value.get('operation', 'inspect'), operation_id)
    safe_fields = {'state', 'verified', 'canonical_eligibility', 'semantic_recovery_complete', 'pending_unit_ids',
                   'pending_units', 'required_actions', 'writes_performed', 'private_artifact_writes_performed',
                   'temporary_lock_writes_performed', 'canonical_writes_performed', 'original_index_writes_performed',
                   'head_writes_performed', 'feature_writes_performed', 'history_writes_performed',
                   'truth_verified', 'code_freshness_verified', 'publication_verified', 'other_machine_import_verified',
                   'observation_complete', 'source_files', 'source_bytes', 'page_count', 'receipt_is_historical',
                   'already_applied', 'current_receipt_match', 'parent_operation_id', 'operations',
                   'canonical_mutation_may_have_occurred', 'failure_code', 'capacity', 'original_evidence_available', 'probe_path',
                   'private_artifact_deletions_performed', 'cleanup_id', 'original_bytes_available', 'rollback_available',
                   'git_metadata_writes_performed', 'remote_freshness_verified', 'source_assertions_verified',
                   'feature_code_freshness_verified', 'cleanup_complete', 'intended_canonical_eligibility'}
    answer.update({key: value[key] for key in safe_fields if key in value})
    if exported is not None:
        answer['private_output'] = {'path': exported['path'], 'bytes': exported['bytes']}
        answer['private_artifact_writes_performed'] = True
    receipt = value.get('receipt')
    if isinstance(receipt, dict):
        answer['receipt'] = {key: receipt[key] for key in ('operation_id', 'proposal_id', 'outcome', 'verified',
            'canonical_eligibility_at_commit', 'semantic_recovery_complete_at_commit', 'parent_operation_id') if key in receipt}
    if isinstance(value.get('recall'), dict):
        answer['recall'] = {'state': value['recall']['state']}
    return answer


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('inspect', 'prepare', 'apply', 'status', 'recover', 'rollback', 'resolve', 'revise-units', 'recover-nested'):
        command = commands.add_parser(name)
        command.add_argument('--repo', required=True)
        command.add_argument('--operation')
        command.add_argument('--input')
        command.add_argument('--proposal')
        command.add_argument('--output')
        if name == 'inspect':
            command.add_argument('--page', type=int)
            command.add_argument('--identity-review')
        if name in ('recover', 'recover-nested'):
            command.add_argument('--mode', choices=('finish', 'rollback'))
        if name == 'recover-nested':
            command.add_argument('--nested')
        if name == 'resolve':
            command.add_argument('--unit')
    retention = commands.add_parser('retention')
    retention_stages = retention.add_subparsers(dest='stage', required=True)
    for stage in ('inspect', 'apply'):
        child = retention_stages.add_parser(stage)
        child.add_argument('--repo', required=True)
        child.add_argument('--operation')
        child.add_argument('--input')
        child.add_argument('--proposal')
        child.add_argument('--output')
    publication = commands.add_parser('publish')
    publication_stages = publication.add_subparsers(dest='stage', required=True)
    for stage in ('inspect', 'prepare', 'land'):
        child = publication_stages.add_parser(stage)
        child.add_argument('--repo', required=True)
        child.add_argument('--operation')
        child.add_argument('--input')
        child.add_argument('--proposal')
        child.add_argument('--output')
    args = parser.parse_args(argv)
    value = None
    execution_started = False
    failure_operation = args.operation
    try:
        output = limits.preflight_private_output(args.output, args.repo) if args.output else None
        request = state.read(args.input) if args.input else None
        proposed = state.read(args.proposal) if args.proposal else None
        if proposed is not None:
            failure_operation = proposed.get('proposal', proposed).get('operation_id', args.operation)
        execution_started = True
        if args.command == 'inspect':
            if output is None:
                raise ContractError('Private output is required for recovery source inspection')
            if args.page is not None:
                if not args.operation:
                    raise ContractError('Page inspection needs its exact operation')
                value = page(args.repo, args.operation, args.page)
            else:
                if args.operation:
                    raise ContractError('Existing observations use an explicit page, status or known recovery')
                review = state.read(args.identity_review) if args.identity_review else None
                value = inspect(args.repo, review)
        elif args.command == 'prepare':
            if request is None or output is None or args.operation is None:
                raise ContractError('Prepare requires complete private input/output and inspected operation')
            value = prepare(args.repo, args.operation, request)
        elif args.command == 'apply':
            if proposed is None:
                raise ContractError('Apply requires the exact private proposal')
            if args.operation and proposed.get('proposal', proposed).get('operation_id') != args.operation:
                raise ContractError('Apply operation differs from the exact proposal')
            value = apply(args.repo, proposed)
        elif args.command == 'status':
            value = status(args.repo, args.operation)
        elif args.command == 'recover':
            if proposed is None and output is None:
                raise ContractError('Known recovery preview requires a private output')
            value = recover(args.repo, args.operation, args.mode, proposed)
        elif args.command == 'rollback':
            if proposed is None and output is None:
                raise ContractError('Rollback preview requires a private output')
            value = rollback(args.repo, args.operation, proposed)
        elif args.command == 'resolve':
            if proposed is None and (request is None or output is None or not args.unit or not args.operation):
                raise ContractError('Resolution preview requires actual terminal evidence and exact unit/operation')
            value = frontier.resolve(args.repo, operation_id=args.operation, unit_id=args.unit, terminal=request, proposal=proposed)
            resolved_operation = (proposed.get('proposal', proposed)['operation_id']
                                  if proposed is not None else args.operation)
            historical = value.get('receipt_is_historical', False)
            value = result('resolve', resolved_operation, **{**value,
                'state': 'prepared' if proposed is None else 'historical' if historical else 'resolved',
                'verified': True, 'private_artifact_writes_performed': value.get('writes_performed', False),
                'temporary_lock_writes_performed': not historical})
        elif args.command == 'revise-units':
            from . import layout_units
            if proposed is None and (request is None or output is None or not args.operation):
                raise ContractError('Unit refinement requires independently reviewed private input/output')
            value = layout_units.apply(args.repo, proposed) if proposed is not None else layout_units.preview(args.repo, args.operation, request)
        elif args.command == 'recover-nested':
            if proposed is None and (output is None or not args.operation or not args.nested or not args.mode):
                raise ContractError('Nested recovery preview requires the exact operation/nested UUID and mode')
            value = frontier.recover_nested(args.repo, operation_id=args.operation, nested_id=args.nested, mode=args.mode, proposal=proposed)
        elif args.command == 'retention':
            from . import layout_recovery_retention
            if args.stage == 'inspect':
                if output is None or not args.operation:
                    raise ContractError('Retention inspection requires operation-scoped private output')
                value = layout_recovery_retention.inspect(args.repo, args.operation, request)
            else:
                if proposed is None:
                    raise ContractError('Retention apply requires the exact independently reviewed proposal')
                value = layout_recovery_retention.apply(args.repo, proposed)
        else:
            from . import layout_recovery_publication
            if output is None or not args.operation:
                raise ContractError('Recovery publication requires operation-scoped private output')
            if args.stage == 'inspect':
                value = layout_recovery_publication.inspect(args.repo, args.operation, request)
            elif args.stage == 'prepare':
                if request is None:
                    raise ContractError('Publication preparation requires complete independent review')
                value = layout_recovery_publication.prepare(args.repo, args.operation, request)
            else:
                if request is None:
                    raise ContractError('Publication landing requires its exact reviewed request')
                value = layout_recovery_publication.land(args.repo, args.operation, request)
        exported = limits.write_private(output, value, args.repo) if output is not None else None
        if value.get('schema') == 'agent-memory/layout-source-page-v1':
            safe = result('inspect', value['operation_id'], state='source-page-exported', verified=True)
            safe['private_output'] = {'path': exported['path'], 'bytes': exported['bytes']}
            safe['private_artifact_writes_performed'] = True
        else:
            safe = safe_envelope(value, exported)
        print(json.dumps(safe, ensure_ascii=False, indent=2))
        return 0 if value.get('verified', True) else 1
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError, RecursionError) as exc:
        if value is not None:
            preserved = safe_envelope(value)
            preserved.update(private_output_state='pending-export', operation_outcome_retained=True)
            print(json.dumps(preserved, ensure_ascii=False, indent=2))
            return 1
        changed = False
        try:
            if failure_operation and args.command not in {'publish', 'retention'}:
                _, _, _, directory = state.paths(args.repo, failure_operation)
                changed = state.read(directory / 'journal.json', missing=True) is not None
        except (ContractError, OSError, ValueError):
            changed = True
        ambiguous = isinstance(exc, temps.ProbeOwnershipError)
        failure = result(args.command, getattr(exc, 'operation_id', failure_operation), state='pending' if ambiguous else 'interrupted' if changed else 'pending',
                         failure_code=('pending-recovery-capacity' if isinstance(exc, raw.RawCapacityError)
                                       else str(exc) if str(exc) in {'pending-identity-review', 'pending-runtime-compatibility', 'pending-metadata-preservation', 'pending-ambiguous-probe-ownership'}
                                       else 'pending-evidence-or-state'),
                         writes_performed='unknown' if changed or ambiguous else False,
                         temporary_lock_writes_performed='unknown' if execution_started else False,
                         canonical_writes_performed='unknown' if changed or ambiguous else False,
                         canonical_mutation_may_have_occurred=changed or ambiguous,
                         private_artifact_writes_performed='unknown' if execution_started and args.command != 'status' else False,
                         required_actions=(['Operator: inspect the named probe and its private journal; only after independently establishing ownership, manually remove the ambiguous entry, then repeat known-operation recovery.']
                                           if ambiguous else ['Preserve original sources and private evidence; inspect status and use only the exact known-operation review/recovery path']))
        if ambiguous:
            failure['probe_path'] = exc.probe_path
        if args.command == 'publish' and execution_started:
            failure['git_metadata_writes_performed'] = 'unknown'
        if args.command == 'retention' and execution_started:
            failure['private_artifact_deletions_performed'] = 'unknown' if args.stage == 'apply' else False
        if isinstance(exc, raw.RawCapacityError):
            failure['capacity'] = exc.details
        print(json.dumps(safe_envelope(failure), ensure_ascii=False, indent=2))
        return 1
