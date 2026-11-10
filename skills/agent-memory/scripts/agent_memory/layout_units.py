"""Reviewed semantic refinement of pending units, preserving every predecessor."""
from __future__ import annotations
import copy
import uuid
from . import layout_recovery as recovery, layout_recovery_state as state
from . import recovery_frontier as frontier, local_registry as storage, bank_transactions as tx
from . import bank_writes as writes, layout_resolution as resolution
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError

SCHEMA = 'agent-memory/layout-unit-revision-v1'
FIELDS = {'schema', 'revision_id', 'operation_id', 'parent_unit_map_sha256', 'parent_ledger_head_sha256',
          'manifest_sha256', 'replacements', 'independent_review_sha256', 'intended_ledger_sha256'}


def merged(ranges):
    result = []
    for span in sorted(ranges, key=lambda span: (span['source_id'], span['start'])):
        frontier.range_value(span)
        if result and span['source_id'] == result[-1]['source_id'] and span['start'] < result[-1]['end']:
            raise ContractError('Unit revision double-counts overlapping original bytes')
        if result and span['source_id'] == result[-1]['source_id'] and span['start'] == result[-1]['end']:
            result[-1]['end'] = span['end']
        else:
            result.append(dict(span))
    return result


def revised(current, unit_map, replacements):
    if not isinstance(replacements, list) or not replacements:
        raise ContractError('Semantic revision requires explicit meaningful replacements')
    units = copy.deepcopy(current['units']); mapped = copy.deepcopy(unit_map['units'])
    all_ids = {unit['unit_id'] for unit in units}; parents = set()
    for replacement in replacements:
        recovery.fields(replacement, {'parent_unit_id', 'source_ranges', 'new_units', 'semantic_rationale'})
        parent_id = replacement['parent_unit_id']
        parent = next((unit for unit in units if unit['unit_id'] == parent_id), None)
        primary = next((unit for unit in mapped if unit['unit_id'] == parent_id), None)
        if (parent_id in parents or parent is None or primary is None or parent['state'] != 'pending-admission'
                or not isinstance(replacement['semantic_rationale'], str) or not replacement['semantic_rationale'].strip()
                or merged(replacement['source_ranges']) != merged(parent['source_ranges'])
                or not isinstance(replacement['new_units'], list) or len(replacement['new_units']) < 2):
            raise ContractError('Revision must meaningfully refine one whole still-pending parent')
        parents.add(parent_id); children = []; ranges = []
        for child in replacement['new_units']:
            recovery.fields(child, {'unit_id', 'source_ranges', 'semantic_rationale', 'provenance', 'reason', 'required_actions'})
            if (not uuid_valid(child['unit_id']) or child['unit_id'] in all_ids
                    or not isinstance(child['semantic_rationale'], str) or not child['semantic_rationale'].strip()
                    or not isinstance(child['provenance'], dict) or not isinstance(child['reason'], str) or not child['reason'].strip()
                    or not isinstance(child['required_actions'], list) or not child['required_actions']):
                raise ContractError('Revised children need new stable IDs and reviewed meaningful provenance/actions')
            all_ids.add(child['unit_id']); children.append(child['unit_id']); ranges.extend(child['source_ranges'])
            units.append({'unit_id': child['unit_id'], 'source_ranges': child['source_ranges'], 'provenance': child['provenance'],
                          'reason': child['reason'], 'required_actions': child['required_actions'],
                          'detecting_developer_id': parent['detecting_developer_id'], 'state': 'pending-admission',
                          'terminal_receipts': copy.deepcopy(parent['terminal_receipts'])})
            mapped.append({**primary, 'unit_id': child['unit_id'], 'source_ranges': child['source_ranges'],
                           'provenance': child['provenance'], 'reason': child['reason'], 'required_actions': child['required_actions']})
        if merged(ranges) != merged(parent['source_ranges']):
            raise ContractError('Revision children must partition all original parent bytes without loss')
        parent['state'] = 'superseded-by-revision'; parent['child_unit_ids'] = children
        mapped.remove(primary)
    return units, {**unit_map, 'units': mapped}


def map_at(directory, current):
    original = state.read(directory / 'unit-map.json')
    if digest(encoded(original)) == current['unit_map_sha256']:
        return original
    value = state.read(directory / 'unit-maps' / (current['unit_map_sha256'] + '.json'))
    if digest(encoded(value)) != current['unit_map_sha256']:
        raise ContractError('Revised immutable unit map is missing or changed')
    return value


def preview(repo, operation_id, request):
    root, bank, _, _ = state.paths(repo, operation_id)
    from .turn_sources import canonical_writer_barrier
    with canonical_writer_barrier(root), storage.exclusive(bank / '.agent-memory.lock', operation_id, 'bank-recover') as owner:
        return _preview(root, operation_id, request, owner)


def _preview(repo, operation_id, request, owner):
    resolution.require_completed_resolutions(repo)
    recovery.fields(request, {'schema', 'operation_id', 'replacements', 'review'}, 'agent-memory/layout-unit-revision-request-v1')
    if request['operation_id'] != operation_id:
        raise ContractError('Unit revision belongs to another operation')
    held = frontier.verified_hold(repo, owner)
    if held is None or held.operation_id != operation_id:
        raise ContractError('Unit revision requires the exact persistent checked hold')
    root, _, _, directory, manifest = recovery.operation(repo, operation_id)
    indexed = state.index(root)['operations'][operation_id]
    _, head, current = frontier.ledger(root, operation_id, indexed)
    old_map = map_at(directory, current)
    units, new_map = revised(current, old_map, request['replacements'])
    original = state.read(directory / 'proposals' / (indexed['proposal_id'] + '.json'))
    recovery.unit_map_value(new_map, manifest, original['intended_files'])
    review = request['review']
    recovery.fields(review, {'schema', 'review_id', 'operation_id', 'author', 'reviewer', 'parent_unit_map_sha256',
                             'parent_ledger_head_sha256', 'manifest_sha256', 'replacements_sha256', 'intended_unit_map_sha256',
                             'reviewed_at', 'decision', 'rationale', 'evidence'}, 'agent-memory/layout-unit-revision-review-v1')
    recovery.independent(review['author'], review['reviewer']); storage.time_value(review['reviewed_at'])
    if (not uuid_valid(review['review_id']) or review['operation_id'] != operation_id or review['decision'] != 'approve'
            or review['parent_unit_map_sha256'] != current['unit_map_sha256']
            or review['parent_ledger_head_sha256'] != digest(encoded(head))
            or review['manifest_sha256'] != digest(encoded(manifest))
            or review['replacements_sha256'] != digest(encoded(request['replacements']))
            or review['intended_unit_map_sha256'] != digest(encoded(new_map))
            or not review['rationale'] or not review['evidence']):
        raise ContractError('Unit refinement needs independent full-source/disposition review')
    files = tx.files_at(held.bank)
    pending = [unit for unit in units if unit['state'] == 'pending-admission']
    other_rows = [row for row in frontier.pending_rows(root, held.token, owner) if row['operation_id'] != operation_id]
    rows = recovery.pending_rows(directory, manifest, pending) + other_rows
    token = frontier.successor_token(held.token, [row['unit_id'] for row in rows], operation_id=operation_id)
    frontier.reservation(files, rows, token)
    successor = {**current, 'generation': current['generation'] + 1, 'previous_generation_sha256': head['sha256'],
                 'unit_map_sha256': digest(encoded(new_map)), 'units': units}
    proposal = {'schema': SCHEMA, 'revision_id': str(uuid.uuid4()), 'operation_id': operation_id,
                'parent_unit_map_sha256': current['unit_map_sha256'], 'parent_ledger_head_sha256': digest(encoded(head)),
                'manifest_sha256': digest(encoded(manifest)), 'replacements': request['replacements'],
                'independent_review_sha256': digest(encoded(review)), 'intended_ledger_sha256': digest(encoded(successor))}
    location = directory / 'revisions' / proposal['revision_id']
    state.write_once(location / 'proposal.json', proposal); state.write_once(location / 'review.json', review)
    state.write_once(location / 'unit-map.json', new_map)
    state.write_once(directory / 'unit-maps' / (digest(encoded(new_map)) + '.json'), new_map)
    state.write_once(location / 'journal.json', {'schema': 'agent-memory/layout-unit-revision-journal-v1',
        'operation_id': operation_id, 'proposal_sha256': digest(encoded(proposal)), 'parent_head': head,
        'successor': successor, 'hold_sha256': digest(held.marker), 'frontier': held.token,
        'bank_hashes': tx.file_hashes(files)})
    frontier.check(held.token, root, owner)
    return recovery.result('revise-units', operation_id, state='unit-revision-preview', verified=True,
                           proposal=proposal, private_artifact_writes_performed=True, temporary_lock_writes_performed=True,
                           pending_unit_ids=token['unit_ids'], pending_units=len(token['unit_ids']))


def apply(repo, supplied):
    proposal = supplied.get('proposal', supplied)
    recovery.fields(proposal, FIELDS, SCHEMA)
    if not uuid_valid(proposal['revision_id']) or not uuid_valid(proposal['operation_id']):
        raise ContractError('Unit revision requires stable UUIDv4 identities')
    root, bank, base, directory, manifest = recovery.operation(repo, proposal['operation_id'])
    location = directory / 'revisions' / proposal['revision_id']
    if state.read(location / 'proposal.json') != proposal:
        raise ContractError('Unit revision must use its exact immutable preview')
    known = state.read(location / 'receipt.json', missing=True)
    if known is not None:
        if known.get('proposal_sha256') != digest(encoded(proposal)) or known.get('verified') is not True:
            raise ContractError('Unit revision historical receipt is corrupt')
        return recovery.result('revise-units', proposal['operation_id'], state='units-revised', verified=True,
                               receipt=known, receipt_is_historical=True, already_applied=True)
    journal = state.read(location / 'journal.json'); review = state.read(location / 'review.json')
    new_map = state.read(location / 'unit-map.json'); successor = journal['successor']
    if (journal['proposal_sha256'] != digest(encoded(proposal)) or digest(encoded(review)) != proposal['independent_review_sha256']
            or digest(encoded(successor)) != proposal['intended_ledger_sha256'] or successor['unit_map_sha256'] != digest(encoded(new_map))
            or digest(encoded(manifest)) != proposal['manifest_sha256']):
        raise ContractError('Revision source/review/journal evidence changed')
    from .turn_sources import canonical_writer_barrier
    with canonical_writer_barrier(root), storage.exclusive(bank / '.agent-memory.lock', proposal['revision_id'], 'bank-recover') as owner:
        resolution.require_completed_resolutions(root)
        marker = frontier.marker_bytes(bank)
        if digest(marker) != journal['hold_sha256']:
            raise ContractError('Revision hold changed or a nested transaction is interrupted')
        if tx.file_hashes(tx.files_at(bank)) != journal['bank_hashes']:
            raise ContractError('Canonical sources changed after semantic revision review')
        head = state.read(directory / 'ledger-head.json')
        expected_head = {'schema': 'agent-memory/layout-ledger-head-v1', 'operation_id': proposal['operation_id'],
                         'generation': successor['generation'], 'sha256': digest(encoded(successor))}
        if head == journal['parent_head']:
            held = frontier.verified_hold(root, owner)
            pending = [unit for unit in successor['units'] if unit['state'] == 'pending-admission']
            frontier.check(journal['frontier'], root, owner)
            others = [row for row in frontier.pending_rows(root, held.token, owner)
                      if row['operation_id'] != proposal['operation_id']]
            rows = recovery.pending_rows(directory, manifest, pending) + others
            token = frontier.successor_token(held.token, [row['unit_id'] for row in rows],
                                              operation_id=proposal['operation_id'])
            frontier.reservation(tx.files_at(bank), rows, token)
            writes.fault('layout-recovery-ledger-generation-before-head')
            frontier.advance_ledger(root, proposal['operation_id'], successor['units'], head, owner,
                                    unit_map_sha256=successor['unit_map_sha256'])
        elif head == expected_head:
            if state.read(directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')) != successor:
                raise ContractError('Revision head has missing successor evidence')
            # Head is the authority after its durable swap. Repair only this
            # exact journal-pinned index lag, never select an arbitrary ledger.
            indexed = state.index(root)
            indexed['operations'][proposal['operation_id']]['ledger_head_sha256'] = digest(encoded(head))
            state.set_index(root, indexed, digest(state.read_bytes(base / 'index.json')))
        else:
            raise ContractError('Revision ledger generation changed')
        writes.fault('layout-recovery-ledger-generation-after-head')
        held = frontier.verified_hold(root, owner)
        frontier.reservation(tx.files_at(bank), frontier.pending_rows(root, held.token, owner), held.token)
        receipt = {'schema': 'agent-memory/layout-unit-revision-receipt-v1', 'revision_id': proposal['revision_id'],
                   'operation_id': proposal['operation_id'], 'proposal_sha256': digest(encoded(proposal)),
                   'ledger_head_sha256': digest(encoded(expected_head)), 'verified': True, 'readback_at': storage.now()}
        state.write_once(location / 'receipt.json', receipt)
        frontier.invalidate(root)
    return recovery.result('revise-units', proposal['operation_id'], state='units-revised', verified=True, receipt=receipt,
                           pending_unit_ids=held.token['unit_ids'], pending_units=len(held.token['unit_ids']),
                           private_artifact_writes_performed=True, temporary_lock_writes_performed=True)
