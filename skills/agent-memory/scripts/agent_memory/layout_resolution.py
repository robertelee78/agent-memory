"""Append-only source-bound terminal evidence and atomic recovery hold handoff."""
from __future__ import annotations

import copy
import os
from pathlib import Path
import uuid

from . import bank_transactions as tx, bank_writes as writes, layout_recovery_state as state
from . import local_registry as storage, recovery_frontier as frontier, records
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError


def atomic_marker(bank, expected, replacement, owner):
    from .bank_observation import markers
    markers(bank, owner, expected)
    before = bank.lstat()
    temporary = bank / ('.agent-memory-marker-' + uuid.uuid4().hex + '.tmp')
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(replacement)
            stream.flush()
            os.fsync(stream.fileno())
        markers(bank, owner, expected)
        current = bank.lstat()
        if (before.st_dev, before.st_ino) != (current.st_dev, current.st_ino):
            raise ContractError('Bank parent binding changed before atomic recovery marker handoff')
        os.replace(temporary, bank / tx.MARKER)
        tx.flush_directory(bank)
        markers(bank, owner, replacement)
    finally:
        if temporary.exists():
            temporary.unlink()


def interrupted_successor(repo, operation_id, units, expected_head, *, unit_map_sha256=None):
    """Validate, but never publish, one exact journal-bound head/index gap."""
    root, _, _, directory = state.paths(repo, operation_id)
    indexed = state.index(root)
    row = indexed['operations'][operation_id]
    observed = state.read(directory / 'ledger-head.json', 4096)
    generation = expected_head['generation'] + 1
    previous = state.read(directory / 'ledger' / (str(expected_head['generation']).zfill(8) + '.json'))
    if (expected_head != {'schema': 'agent-memory/layout-ledger-head-v1', 'operation_id': operation_id,
            'generation': previous['generation'], 'sha256': digest(encoded(previous))}
            or previous['operation_id'] != operation_id or previous['project_id'] != row['project_id']):
        raise ContractError('Ledger reconciliation lost its exact immutable predecessor')
    successor = {**copy.deepcopy(previous), 'units': units, 'generation': generation,
        'previous_generation_sha256': expected_head['sha256']}
    if unit_map_sha256 is not None:
        successor['unit_map_sha256'] = unit_map_sha256
    head = {'schema': expected_head['schema'], 'operation_id': operation_id,
        'generation': generation, 'sha256': digest(encoded(successor))}
    transition = state.read(directory / 'ledger-transitions' / (str(generation).zfill(8) + '.json'))
    if (observed != head or transition != {'schema': 'agent-memory/layout-ledger-transition-v1',
            'operation_id': operation_id, 'previous_head': expected_head, 'head': head,
            'units_sha256': digest(encoded(units))}
            or row['ledger_head_sha256'] not in {digest(encoded(expected_head)), digest(encoded(head))}):
        raise ContractError('Ledger retry does not match its exact journaled successor')
    projected = {**row, 'ledger_head_sha256': digest(encoded(head))}
    _, verified_head, current = frontier.ledger(root, operation_id, projected)
    if verified_head != head or current != successor:
        raise ContractError('Journaled ledger successor differs from retained immutable evidence')
    frontier.evidence(directory, current, projected)
    return projected, head, current


def advance_ledger(repo, operation_id, units, expected_head, owner, transaction=None, *, unit_map_sha256=None):
    from .bank_observation import markers
    root, bank, base, directory = state.paths(repo, operation_id)
    marker = transaction or frontier.marker_bytes(bank)
    markers(bank, owner, marker)
    indexed = state.index(root)
    row = indexed['operations'][operation_id]
    observed_head = state.read(directory / 'ledger-head.json', 4096)
    generation = expected_head['generation'] + 1
    transition_path = directory / 'ledger-transitions' / (str(generation).zfill(8) + '.json')
    if observed_head != expected_head:
        projected, _, _ = interrupted_successor(root, operation_id, units, expected_head,
            unit_map_sha256=unit_map_sha256)
        row.update(projected)
        markers(bank, owner, marker)
        state.set_index(root, indexed, digest(state.read_bytes(base / 'index.json')))
        _, head, current = frontier.ledger(root, operation_id, row)
        frontier.invalidate(root)
        return {'head': head, 'ledger': current, 'already_applied': True}
    _, head, current = frontier.ledger(root, operation_id, row)
    successor = {**copy.deepcopy(current), 'units': units, 'generation': generation,
        'previous_generation_sha256': head['sha256']}
    if unit_map_sha256 is not None:
        if not state.sha(unit_map_sha256) or digest(state.read_bytes(directory / 'unit-maps' / (unit_map_sha256 + '.json'))) != unit_map_sha256:
            raise ContractError('Unit revision must retain its exact immutable reviewed source map')
        successor['unit_map_sha256'] = unit_map_sha256
    payload_sha = state.write_once(directory / 'ledger' / (str(generation).zfill(8) + '.json'), successor)
    new_head = {'schema': head['schema'], 'operation_id': operation_id, 'generation': generation, 'sha256': payload_sha}
    state.write_once(transition_path, {'schema': 'agent-memory/layout-ledger-transition-v1',
        'operation_id': operation_id, 'previous_head': head, 'head': new_head,
        'units_sha256': digest(encoded(units))})
    writes.fault('layout-recovery-ledger-generation-before-head')
    markers(bank, owner, marker)
    state.replace(directory / 'ledger-head.json', new_head, digest(encoded(head)))
    writes.fault('layout-recovery-ledger-generation-after-head')
    row['ledger_head_sha256'] = digest(encoded(new_head))
    state.set_index(root, indexed, digest(state.read_bytes(base / 'index.json')))
    _, verified_head, verified = frontier.ledger(root, operation_id, row)
    if verified != successor or verified_head != new_head:
        raise ContractError('Ledger successor readback differs; preserve the typed marker')
    frontier.invalidate(root)
    return {'head': verified_head, 'ledger': verified, 'already_applied': False}


def restore_hold(repo, operation_id, owner, transaction):
    root, bank, _, _ = state.paths(repo, operation_id)
    token = frontier.observe(root, owner, transaction)
    row = state.index(root)['operations'][operation_id]
    if row['state'] not in {'active', 'rolled-back'}:
        raise ContractError('Only an active or retained rolled-back recovery operation can restore its hold')
    marker = encoded({'schema': frontier.HOLD, 'project_id': row['project_id'],
        'operation_id': operation_id, 'hold_id': row['hold_id']})
    writes.fault('layout-recovery-before-hold-restore')
    atomic_marker(bank, transaction, marker, owner)
    writes.fault('layout-recovery-after-hold-restore')
    frontier.check(token, root, owner)
    return marker


def retire(repo, operation_id, owner, transaction=None):
    from .bank_observation import markers
    root, bank, base, directory = state.paths(repo, operation_id)
    indexed = state.index(root)
    row = indexed['operations'][operation_id]
    _, head, current = frontier.ledger(root, operation_id, row)
    if any(unit['state'] == 'pending-admission' for unit in current['units']):
        raise ContractError('Unresolved source units cannot retire their recovery safeguard')
    retirement = state.read(directory / 'retirement.json', missing=True)
    if row['state'] == 'terminal':
        if retirement is None or retirement.get('ledger_head') != head:
            raise ContractError('Only the exact durable retirement may finish a terminal safeguard')
        for other_id, other_row in indexed['operations'].items():
            if other_row['state'] == 'cleaned':
                continue
            other_directory, _, other = frontier.ledger(root, other_id, other_row)
            frontier.evidence(other_directory, other, other_row)
            if (any(unit['state'] == 'pending-admission' for unit in other['units'])
                    or (other_id != operation_id and other_row['state'] in {'active', 'interrupted'})):
                raise ContractError('A retained same-bank dependency still requires its safeguard')
    elif frontier.observe(root, owner, transaction)['unit_ids']:
        raise ContractError('Other retained same-bank source units still require this recovery safeguard')
    receipt = state.read(directory / 'receipt.json')
    if receipt.get('verified') is not True:
        raise ContractError('Recovery retirement needs the immutable verified layout receipt')
    nested_base = directory / 'nested'
    if nested_base.exists():
        for nested in nested_base.iterdir():
            if not nested.is_dir() or not uuid_valid(nested.name):
                raise ContractError('Unknown nested dependency prevents recovery retirement')
            completion = state.read(nested / 'completion.json', missing=True)
            if completion is None or completion.get('verified') is not True:
                raise ContractError('Nested canonical/resolution dependency remains interrupted')
    marker = transaction or frontier.marker_bytes(bank)
    if marker is not None:
        value = frontier.marker_value(marker)
        if (value['operation_id'] != operation_id or value['project_id'] != row['project_id']
                or value.get('hold_id') != row['hold_id'] or value['schema'] not in {frontier.HOLD, frontier.NESTED}):
            raise ContractError('Recovery retirement cannot remove another operation marker')
        markers(bank, owner, marker)
    sentinel = encoded({'schema': frontier.SENTINEL, 'project_id': row['project_id'], 'operation_id': operation_id})
    current_sentinel = frontier.marker_bytes(bank, '.agent-memory-recovery.md')
    if current_sentinel not in (None, sentinel):
        raise ContractError('Recovery retirement found a changed companion sentinel')
    dependencies = (retirement['dependencies'] if retirement is not None else [
        {'operation_id': key, 'ledger_head_sha256': value['ledger_head_sha256']}
        for key, value in sorted(indexed['operations'].items()) if value['state'] in {'active', 'rolled-back'}])
    if operation_id not in {item['operation_id'] for item in dependencies}:
        raise ContractError('Retirement omitted the exact holding operation')
    for item in dependencies:
        dependent = indexed['operations'].get(item['operation_id'])
        if (dependent is None or dependent['project_id'] != row['project_id']
                or dependent['ledger_head_sha256'] != item['ledger_head_sha256']
                or dependent['state'] not in {'active', 'rolled-back', 'terminal'}):
            raise ContractError('Retirement dependency changed after durable terminal review')
    state.write_once(directory / 'retirement.json', {'schema': 'agent-memory/layout-retirement-v1',
        'operation_id': operation_id, 'ledger_head': head, 'receipt_sha256': digest(encoded(receipt)),
        'dependencies': dependencies, 'verified': True})
    for item in dependencies:
        indexed['operations'][item['operation_id']]['state'] = 'terminal'
    state.set_index(root, indexed, digest(state.read_bytes(base / 'index.json')))
    writes.fault('layout-recovery-retirement-after-index')
    for item in dependencies:
        dependent_manifest = state.read(state.paths(root, item['operation_id'])[3] / 'manifest.json')
        state.bind(root, item['operation_id'], row['project_id'],
            dependent_manifest['identity']['scope_context']['checkout_key'], 'terminal')
        writes.fault('layout-recovery-retirement-after-binding')
    writes.fault('layout-recovery-sentinel-retirement')
    if marker is not None:
        markers(bank, owner, marker)
        (bank / tx.MARKER).unlink()
        tx.flush_directory(bank)
        writes.fault('layout-recovery-retirement-after-marker-removal')
    if current_sentinel is not None:
        if frontier.marker_bytes(bank, '.agent-memory-recovery.md') != sentinel:
            raise ContractError('Recovery sentinel changed before terminal retirement')
        (bank / '.agent-memory-recovery.md').unlink()
        tx.flush_directory(bank)
        writes.fault('layout-recovery-retirement-after-sentinel-removal')
    frontier.invalidate(root)
    return {'operation_id': operation_id, 'semantic_recovery_complete': True,
        'pending_unit_ids': [], 'safeguard_retired': True}


def transaction_evidence(p):
    if 'admission_evidence' in p:
        value = p['admission_evidence']
        return value, value['comparison_proof'], value['comparison_request']
    if 'consolidation_evidence' in p:
        value = p['consolidation_evidence']
        return value, value['comparison_proof'], value['comparison_request']
    if 'governance_evidence' in p and p['governance_evidence']['request']['comparison'] is not None:
        value = p['governance_evidence']
        comparison = value['request']['comparison']
        return value, comparison['proof'], comparison['request']
    return None, None, None


def claims_for(p, *, owner=None, transaction=None):
    evidence, proof, request = transaction_evidence(p)
    token = frontier.observe(p['repo_root'], owner, transaction)
    if proof is None:
        if token['unit_ids']:
            raise ContractError('A mutation under recovery hold needs complete comparison-v2 frontier evidence')
        return [], None
    if proof.get('schema') != 'agent-memory/comparison-proof-v2':
        raise ContractError('Unfinished comparison-v1 evidence cannot authorize a current governed write')
    claims = [decision['coverage_claim'] for decision in proof['judgment']['pending_decisions']]
    if (evidence.get('unit_resolution_claims') != claims or evidence.get('comparison_proof_sha256') != digest(encoded(proof))
            or evidence.get('reference_map_sha256') != proof['judgment']['observation']['recovery_frontier']['reference_map_sha256']):
        raise ContractError('Transaction evidence must repeat the exact proof-bound unit claims and private reference map')
    frontier.check(proof['judgment']['observation']['recovery_frontier'], p['repo_root'], owner, transaction)
    review = frontier.validate_comparison_review(p['repo_root'], proof, owner, transaction=transaction)
    if review is not None:
        if review['intended_output_sha256'] != digest(encoded(p['files'])):
            raise ContractError('Coverage reviewer did not inspect this exact complete intended canonical output')
        origin = request['candidate']['metadata']['origin']
        if (review['reviewer_harness'], review['reviewer_session']) == (origin['harness'], origin['session']):
            raise ContractError('Coverage review must come from a distinct actual reviewing agent/session')
    for claim in claims:
        for ref in claim['target_record_refs']:
            if ref['path'].startswith('ref:'):
                ref = frontier.resolve_reference(p['repo_root'], ref)
            body = p['files'].get(ref['path'])
            if body is None or digest(body.encode()) != ref['resulting_sha256']:
                raise ContractError('Coverage target is absent or differs from the intended complete result')
            kind, metadata, _ = records.parse(body)
            if kind != 'standalone-record' or metadata['id'] != ref['id'] or metadata['version'] != ref['version']:
                raise ContractError('Coverage target identity/version differs from its real resulting record')
    return claims, review


def evidence_fields(proof):
    if proof is None:
        return {'unit_resolution_claims': [], 'comparison_proof_sha256': None,
            'reference_map_sha256': digest(encoded({}))}
    return {'unit_resolution_claims': [decision['coverage_claim'] for decision in proof['judgment']['pending_decisions']],
        'comparison_proof_sha256': digest(encoded(proof)),
        'reference_map_sha256': proof['judgment']['observation']['recovery_frontier']['reference_map_sha256']}


def resolution_receipt(p):
    return {'schema': 'agent-memory/layout-resolution-receipt-v1', 'resolution_id': p['resolution_id'],
        'operation_id': p['operation_id'], 'unit_id': p['unit_id'], 'generation': p['ledger_head']['generation'],
        'request_sha256': digest(encoded(p)), 'terminal_state': p['terminal_state'],
        'canonical_operation_id': p['canonical_operation_id'], 'canonical_receipt_sha256': p['canonical_receipt_sha256'],
        'claims': p['claims'], 'independent_review_sha256': p['independent_review_sha256'], 'verified': True}


def resolution_completion(resolution_id, receipt, journal):
    return {'schema': 'agent-memory/layout-resolution-completion-v1',
        'resolution_id': resolution_id, 'receipt_sha256': digest(encoded(receipt)),
        'journal_sha256': digest(encoded(journal)), 'verified': True}


def require_completed_resolutions(repo, retry=None):
    """Under the bank lock, reserve every unfinished journal for its exact retry."""
    root, _, base, _ = state.paths(repo)
    for operation_id, row in state.index(root)['operations'].items():
        if row['state'] == 'cleaned':
            continue
        directory = base / operation_id
        journals = directory / 'resolution-journals'
        if not journals.exists():
            continue
        for path in journals.iterdir():
            resolution_id = path.stem
            journal = state.read(path)
            p = journal.get('proposal', {}) if isinstance(journal, dict) else {}
            if (path.name != resolution_id + '.json' or not uuid_valid(resolution_id)
                    or not isinstance(journal, dict)
                    or set(journal) != {'schema', 'proposal', 'units', 'hold', 'scope_context'}
                    or journal['schema'] != 'agent-memory/layout-resolution-journal-v1'
                    or not isinstance(p, dict) or p.get('operation_id') != operation_id
                    or p.get('resolution_id') != resolution_id or p.get('repo_root') != str(root)):
                raise ContractError('Unknown resolution journal prevents a new recovery action')
            receipt = state.read(directory / 'resolutions' / path.name, missing=True)
            completion = state.read(directory / 'resolution-completions' / path.name, missing=True)
            if receipt is not None and receipt != resolution_receipt(p):
                raise ContractError('Resolution journal lost its exact source-bound receipt')
            if completion is not None:
                if receipt is None or completion != resolution_completion(resolution_id, receipt, journal):
                    raise ContractError('Resolution completion differs from its exact journal and receipt')
            elif retry != p:
                raise ContractError('An unfinished resolution journal requires its exact proposal retry before new recovery actions')


def resolution_units(directory, p, receipt):
    previous = state.read(directory / 'ledger' / (str(p['ledger_head']['generation']).zfill(8) + '.json'))
    if digest(encoded(previous)) != p['ledger_head']['sha256']:
        raise ContractError('Resolution lost its exact immutable original ledger generation')
    units = copy.deepcopy(previous['units'])
    record = next(unit for unit in units if unit['unit_id'] == p['unit_id'])
    if record['state'] != 'pending-admission':
        raise ContractError('Resolution cannot replace an already terminal unit')
    record['state'] = p['terminal_state']
    record['terminal_receipts'].append({'resolution_id': p['resolution_id'], 'sha256': digest(encoded(receipt))})
    return units


def resolve(repo, operation_id=None, unit_id=None, terminal=None, proposal=None):
    """A durable receipt resumes linkage/retirement; completion alone is historical."""
    if proposal is None:
        resolution_id = str(uuid.uuid4())
        root, bank, _, _ = state.paths(repo, operation_id)
        with storage.exclusive(bank / '.agent-memory.lock', resolution_id, 'bank-recover') as owner:
            preview = _resolution_preview(root, operation_id, unit_id, terminal, resolution_id, owner)
        return {'proposal': preview, 'canonical_writes_performed': False, 'writes_performed': False,
            'temporary_lock_writes_performed': True}
    p = proposal.get('proposal', proposal)
    root, bank, _, directory = state.paths(repo, p['operation_id'])
    if (p.get('schema') != 'agent-memory/layout-resolution-proposal-v1' or p['repo_root'] != str(root)
            or not uuid_valid(p['resolution_id'])):
        raise ContractError('Use the exact private source-bound resolution proposal')
    resolution_id = p['resolution_id']
    operation_id = p['operation_id']
    receipt = resolution_receipt(p)
    known = state.read(directory / 'resolutions' / (resolution_id + '.json'), missing=True)
    if known is not None and known != receipt:
        raise ContractError('Resolution ID was reused with different evidence or receipt bytes')
    completion_path = directory / 'resolution-completions' / (resolution_id + '.json')
    completed = state.read(completion_path, missing=True)
    journal_path = directory / 'resolution-journals' / (resolution_id + '.json')
    journal = state.read(journal_path, missing=True)
    if completed is not None:
        if (known is None or journal is None or journal['proposal'] != p
                or completed != resolution_completion(resolution_id, receipt, journal)):
            raise ContractError('Historical resolution lost its exact completion/linkage evidence')
        from . import layout_history
        layout_history.check(root, journal['scope_context'], p['recovery_frontier']['project_id'])
        return {'receipt': known, 'already_applied': True, 'receipt_is_historical': True, 'writes_performed': False}
    with storage.exclusive(bank / '.agent-memory.lock', resolution_id, 'bank-recover') as owner:
        require_completed_resolutions(root, retry=p)
        units = resolution_units(directory, p, receipt)
        if known is None:
            fresh = _resolution_preview(root, operation_id, p['unit_id'], p['terminal'], resolution_id, owner)
            if fresh != p:
                raise ContractError('Resolution evidence changed after preview')
            held = frontier.verified_hold(root, owner)
            expected_journal = {'schema': 'agent-memory/layout-resolution-journal-v1', 'proposal': p,
                'units': units, 'hold': frontier.marker_value(held.marker), 'scope_context': held.scope}
            state.write_once(journal_path, expected_journal)
            journal = expected_journal
            state.write_once(directory / 'resolutions' / (resolution_id + '.json'), receipt)
            writes.fault('layout-recovery-resolution-after-receipt')
        if (journal is None or set(journal) != {'schema', 'proposal', 'units', 'hold', 'scope_context'}
                or journal['schema'] != 'agent-memory/layout-resolution-journal-v1'
                or journal['proposal'] != p or journal['units'] != units):
            raise ContractError('Resolution retry requires its exact durable source-bound journal')
        from . import bank_observation, layout_history
        layout_history.check(root, journal['scope_context'], p['recovery_frontier']['project_id'])
        marker = frontier.marker_bytes(bank)
        hold_operation = journal['hold']['operation_id']
        retirement = state.read(state.paths(root, hold_operation)[3] / 'retirement.json', missing=True)
        if marker != encoded(journal['hold']) and not (marker is None and retirement is not None):
            raise ContractError('Standalone resolution cannot bypass a nested, foreign or missing hold')
        bank_observation.markers(bank, owner, marker)
        indexed = state.index(root)
        observed = state.read(directory / 'ledger-head.json')
        if observed['generation'] <= p['ledger_head']['generation'] + 1:
            successor = advance_ledger(root, operation_id, units, p['ledger_head'], owner, marker)
        else:
            expected = state.read(directory / 'ledger' / (str(p['ledger_head']['generation'] + 1).zfill(8) + '.json'))
            if expected['units'] != units or expected['previous_generation_sha256'] != p['ledger_head']['sha256']:
                raise ContractError('Historical resolution linkage differs from its exact successor')
            frontier.ledger(root, operation_id, indexed['operations'][operation_id])
            successor = {'head': observed}
        if state.index(root)['operations'][hold_operation]['state'] == 'terminal':
            retire(root, hold_operation, owner)
        token = frontier.observe(root, owner)
        if token['coverage_complete'] and frontier.marker_bytes(bank) is not None:
            held = frontier.verified_hold(root, owner)
            if held is None or held.operation_id != hold_operation:
                raise ContractError('Terminal resolution lost its exact same-bank recovery hold')
            retire(root, hold_operation, owner)
        state.write_once(completion_path, resolution_completion(resolution_id, receipt, journal))
        return {'receipt': receipt, 'ledger_head': successor['head'], 'writes_performed': True,
            'already_applied': known is not None, 'semantic_recovery_complete': token['coverage_complete']}


def _resolution_preview(repo, operation_id, unit_id, terminal, resolution_id, owner=None):
    root, bank, _, directory = state.paths(repo, operation_id)
    journal = state.read(directory / 'resolution-journals' / (resolution_id + '.json'), missing=True)
    retry = journal['proposal'] if journal is not None else None
    require_completed_resolutions(root, retry=retry)
    held = frontier.verified_hold(root, owner)
    if held is None:
        raise ContractError('Resolution needs the exact persistent hold, never an interrupted nested marker')
    indexed = state.index(root)
    row = indexed['operations'].get(operation_id)
    if row is None or row['project_id'] != held.token['project_id'] or (
            operation_id != held.operation_id and row['state'] != 'rolled-back'):
        raise ContractError('Resolution may name only the active or explicitly retained rolled-back same-bank ledger')
    _, head, current = frontier.ledger(root, operation_id, row)
    unit = next((unit for unit in current['units'] if unit['unit_id'] == unit_id), None)
    if unit is None or unit['state'] != 'pending-admission':
        raise ContractError('Resolution must name one actual unresolved source unit')
    if not isinstance(terminal, dict):
        raise ContractError('Terminal resolution requires actual committed coverage or independently reviewed non-guidance authority')
    files = tx.files_at(bank)
    if set(terminal) == {'discard_authority'}:
        authority = terminal['discard_authority']
        fields = {'schema', 'review_id', 'operation_id', 'unit_id', 'manifest_sha256', 'unit_map_sha256',
            'ledger_head_sha256', 'source_ranges', 'author_harness', 'author_session', 'reviewer_developer_id',
            'reviewer_harness', 'reviewer_session', 'review_time', 'non_guidance', 'reason', 'decision'}
        if (not isinstance(authority, dict) or set(authority) != fields
                or authority['schema'] != 'agent-memory/layout-nonguidance-discard-v1'
                or not uuid_valid(authority['review_id']) or not uuid_valid(authority['reviewer_developer_id'])
                or authority['operation_id'] != operation_id or authority['unit_id'] != unit_id
                or authority['manifest_sha256'] != current['manifest_sha256']
                or authority['unit_map_sha256'] != current['unit_map_sha256']
                or authority['ledger_head_sha256'] != head['sha256'] or authority['source_ranges'] != unit['source_ranges']
                or authority['non_guidance'] is not True or authority['decision'] != 'discard'
                or (authority['author_harness'], authority['author_session']) ==
                   (authority['reviewer_harness'], authority['reviewer_session'])):
            raise ContractError('Non-guidance discard needs exact full-source independent authority, not a backup or elapsed time')
        from . import comparison, lifecycle
        comparison.safe_text(authority['reason'], 4096)
        lifecycle.time_value(authority['review_time'])
        for key in ('author_harness', 'author_session', 'reviewer_harness', 'reviewer_session'):
            comparison.safe_text(authority[key], 256)
        state.write_once(directory / 'discard-reviews' / (authority['review_id'] + '.json'), authority)
        selected = {'terminal_state': 'reviewed-non-guidance-discard', 'canonical_operation_id': None,
            'canonical_receipt_sha256': None, 'claims': [], 'independent_review_sha256': digest(encoded(authority))}
    elif set(terminal) == {'transaction_proposals', 'coverage_review'}:
        selected = accumulated_coverage(root, operation_id, unit, head, current, terminal, files)
    elif set(terminal) == {'transaction_proposal'}:
        p = tx.proposal_value(terminal['transaction_proposal'])
        claims, review = claims_for(p, owner=owner)
        full = [claim for claim in claims if claim['unit_id'] == unit_id and claim['coverage'] == 'full']
        if len(full) != 1 or review is None:
            raise ContractError('Only full independently reviewed committed coverage resolves the whole original unit')
        _, receipt_path = tx.locations(root, bank, p['operation_id'])
        receipt = tx.receipt_value(receipt_path, p)
        if receipt is None or receipt['outcome'] != 'finished' or writes.snapshot(root, owner)[1] != receipt['resulting_hashes']:
            raise ContractError('A proposed, interrupted, changed or partial canonical write cannot resolve pending knowledge')
        selected = {'terminal_state': 'admitted' if 'admission_evidence' in p else 'governed-terminal',
            'canonical_operation_id': p['operation_id'], 'canonical_receipt_sha256': digest(encoded(receipt)),
            'claims': full, 'independent_review_sha256': digest(encoded(review))}
    else:
        raise ContractError('Unknown terminal resolution evidence')
    remaining = [row for row in frontier.pending_rows(root, held.token, owner) if row['unit_id'] != unit_id]
    frontier.reservation(files, remaining, frontier.successor_token(held.token,
        [row['unit_id'] for row in remaining], operation_id=operation_id))
    return {'schema': 'agent-memory/layout-resolution-proposal-v1', 'resolution_id': resolution_id,
        'operation_id': operation_id, 'unit_id': unit_id, 'repo_root': str(root), 'ledger_head': head,
        'recovery_frontier': held.token, 'terminal': terminal, **selected}


def accumulated_coverage(repo, operation_id, unit, head, ledger, terminal, files):
    """A fresh independent review credits the complete union of actual receipts."""
    from . import comparison, lifecycle
    proposals = terminal['transaction_proposals']
    review = terminal['coverage_review']
    if not isinstance(proposals, list) or not proposals:
        raise ContractError('Accumulated coverage must retain every actual committed transaction')
    claims, receipts, origins, operation_ids = [], [], [], set()
    for value in proposals:
        p = tx.proposal_value(value)
        if p['operation_id'] in operation_ids or p['repo_root'] != str(Path(repo).resolve()):
            raise ContractError('Accumulated coverage cannot duplicate or transfer committed evidence')
        operation_ids.add(p['operation_id'])
        evidence, proof, request = transaction_evidence(p)
        if proof is None or proof.get('schema') != 'agent-memory/comparison-proof-v2':
            raise ContractError('Accumulated resolution requires actual comparison-v2 proof-bound fragments')
        if any(evidence.get(key) != item for key, item in evidence_fields(proof).items()):
            raise ContractError('A committed transaction does not bind its claimed source coverage')
        _, receipt_path = tx.locations(*writes.bank_path(repo), p['operation_id'])
        receipt = tx.receipt_value(receipt_path, p)
        if receipt is None or receipt['outcome'] != 'finished':
            raise ContractError('A partial/proposed canonical transaction cannot enter terminal coverage')
        receipts.append({'operation_id': p['operation_id'], 'receipt_sha256': digest(encoded(receipt)),
            'proof_sha256': digest(encoded(proof))})
        origins.append(request['candidate']['metadata']['origin'])
        for decision in proof['judgment']['pending_decisions']:
            if decision['unit_id'] != unit['unit_id'] or decision['coverage_claim']['coverage'] == 'none':
                continue
            claim = decision['coverage_claim']
            frontier.claim_value(claim, unit)
            for reference in claim['target_record_refs']:
                ref = frontier.resolve_reference(repo, reference) if reference['path'].startswith('ref:') else reference
                body = files.get(ref['path'])
                if body is None or digest(body.encode()) != ref['resulting_sha256']:
                    raise ContractError('Accumulated coverage target changed, was replaced or was removed')
                kind, metadata, _ = records.parse(body)
                if kind != 'standalone-record' or (metadata['id'], metadata['version']) != (ref['id'], ref['version']):
                    raise ContractError('Accumulated target reference no longer names its actual record')
            claims.append(claim)
    ranges = {encoded(value): value for claim in claims for value in claim['covered_source_ranges']}
    merged = []
    for value in sorted(ranges.values(), key=lambda row: (row['source_id'], row['start'], row['end'])):
        if merged and merged[-1]['source_id'] == value['source_id']:
            if merged[-1]['end'] > value['start']:
                raise ContractError('Accumulated coverage cannot count overlapping excerpts twice')
            if merged[-1]['end'] == value['start']:
                merged[-1]['end'] = value['end']
                continue
        merged.append(dict(value))
    if merged != sorted(unit['source_ranges'], key=lambda row: (row['source_id'], row['start'], row['end'])):
        raise ContractError('Accumulated coverage omits original conditions, framing or semantic source ranges')
    fields = {'schema', 'review_id', 'operation_id', 'reviewer_developer_id', 'reviewer_harness',
        'reviewer_session', 'review_time', 'manifest_sha256', 'unit_map_sha256', 'ledger_head_sha256',
        'comparison_proof_input_sha256', 'intended_output_sha256', 'claims', 'rationale', 'decision'}
    if (not isinstance(review, dict) or set(review) != fields or review['schema'] != 'agent-memory/layout-coverage-review-v1'
            or not uuid_valid(review['review_id']) or not uuid_valid(review['reviewer_developer_id'])
            or review['operation_id'] != operation_id or review['manifest_sha256'] != ledger['manifest_sha256']
            or review['unit_map_sha256'] != ledger['unit_map_sha256'] or review['ledger_head_sha256'] != head['sha256']
            or review['comparison_proof_input_sha256'] != digest(encoded(receipts))
            or review['intended_output_sha256'] != digest(encoded(files)) or review['claims'] != claims
            or review['decision'] != 'approve' or any((review['reviewer_harness'], review['reviewer_session']) ==
                (origin['harness'], origin['session']) for origin in origins)):
        raise ContractError('Terminal union requires a distinct fresh independent review of every receipt and current target')
    comparison.safe_text(review['rationale'], 4096)
    lifecycle.time_value(review['review_time'])
    directory = state.paths(repo, operation_id)[3]
    state.write_once(directory / 'union-reviews' / (review['review_id'] + '.json'), review)
    return {'terminal_state': 'admitted' if all('admission_evidence' in value for value in proposals) else 'governed-terminal',
        'canonical_operation_id': sorted(operation_ids), 'canonical_receipt_sha256': digest(encoded(receipts)),
        'claims': claims, 'independent_review_sha256': digest(encoded(review))}
