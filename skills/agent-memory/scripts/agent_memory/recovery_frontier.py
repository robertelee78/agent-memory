"""Live private recovery coverage, independently pinned from canonical bytes.

Pending text is comparison-only. A marker is accepted only after checking the
external binding, immutable evidence and full ledger; caller JSON is not a hold.
"""
from __future__ import annotations

import copy
import os
from pathlib import Path
import stat
import uuid

from . import bank_contract, bank_writes as writes, layout_recovery_state as state, local_registry as storage
from . import security, sharing_limits
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError

HOLD = 'agent-memory/layout-frontier-hold-v1'
NESTED = 'agent-memory/layout-frontier-transaction-v1'
TRANSACTION = 'agent-memory/layout-recovery-transaction-v1'
SENTINEL = 'agent-memory/layout-recovery-sentinel-v1'
TERMINAL = {'admitted', 'governed-terminal', 'reviewed-non-guidance-discard'}
UNIT_FIELDS = {'unit_id', 'source_ranges', 'provenance', 'reason', 'required_actions',
    'detecting_developer_id', 'state', 'terminal_receipts'}


class VerifiedHold:
    """Internal descriptor-derived context, deliberately not a mapping/proof."""
    __slots__ = ('root', 'bank', 'marker', 'token', 'scope', 'operation_id', 'runtime_artifact_digest')

    def __init__(self, root, bank, marker, token, scope, operation_id, runtime_artifact_digest):
        self.root, self.bank, self.marker = root, bank, marker
        self.token, self.scope, self.operation_id = token, scope, operation_id
        self.runtime_artifact_digest = runtime_artifact_digest


def marker_bytes(bank, name='.agent-memory-transaction.md', *, expected=None):
    # Ordinary transaction markers contain complete base hashes. Only an exact
    # caller-owned byte witness can extend the unknown-marker disclosure bound.
    if expected is not None and (name != '.agent-memory-transaction.md'
            or not isinstance(expected, bytes) or len(expected) > sharing_limits.MAX_PRIVATE):
        raise ContractError('Invalid exact owned transaction marker witness')
    limit = 4096 if expected is None else len(expected)
    path = bank / name
    try:
        before = path.lstat()
    except FileNotFoundError:
        return None
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_size > limit:
        raise ContractError('Unknown or unsafe recovery marker; retain it for reviewed recovery')
    directory = os.open(bank, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
        with os.fdopen(fd, 'rb') as stream:
            opened = os.fstat(stream.fileno())
            raw = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
        named = os.stat(name, dir_fd=directory, follow_symlinks=False)
        parent = bank.lstat()
        pinned = os.fstat(directory)
        if (len(raw) > limit or (expected is not None and raw != expected)
                or state.signature(before) != state.signature(opened)
                or state.signature(opened) != state.signature(after) or state.signature(after) != state.signature(named)
                or (parent.st_dev, parent.st_ino) != (pinned.st_dev, pinned.st_ino)):
            raise ContractError('Recovery marker or bank binding changed during observation')
        return raw
    finally:
        os.close(directory)


def marker_value(raw):
    value = load(raw)
    if not isinstance(value, dict):
        raise ContractError('Unknown recovery marker')
    schema = value.get('schema')
    fields = {'schema', 'project_id', 'operation_id', 'hold_id'}
    if schema == NESTED:
        fields.add('nested_id')
    elif schema == TRANSACTION:
        fields = {'schema', 'project_id', 'operation_id', 'transaction_kind'}
    elif schema != HOLD:
        raise ContractError('Bank is busy or interrupted; only a verified persistent recovery hold permits ordinary use')
    if (set(value) != fields or any(not uuid_valid(value[key]) for key in fields - {'schema', 'transaction_kind'})
            or (schema == TRANSACTION and value['transaction_kind'] not in {'apply', 'finish', 'rollback'})
            or encoded(value) != raw):
        raise ContractError('Unknown or malformed typed recovery marker')
    return value


def ledger(repo, operation_id, row):
    _, _, _, directory = state.paths(repo, operation_id)
    head = state.read(directory / 'ledger-head.json', 4096)
    if (not isinstance(head, dict) or set(head) != {'schema', 'operation_id', 'generation', 'sha256'}
            or head['schema'] != 'agent-memory/layout-ledger-head-v1' or head['operation_id'] != operation_id
            or type(head['generation']) is not int or not 0 <= head['generation'] <= 99999999
            or not state.sha(head['sha256']) or digest(encoded(head)) != row['ledger_head_sha256']):
        raise ContractError('Recovery ledger head is missing, changed or inconsistent with its index')
    current = state.read(directory / 'ledger' / (str(head['generation']).zfill(8) + '.json'))
    fields = {'schema', 'operation_id', 'project_id', 'manifest_sha256', 'unit_map_sha256', 'proposal_sha256',
        'generation', 'previous_generation_sha256', 'units'}
    if (not isinstance(current, dict) or set(current) != fields
            or current['schema'] != 'agent-memory/layout-pending-ledger-v1'
            or current['operation_id'] != operation_id or current['project_id'] != row['project_id']
            or current['generation'] != head['generation'] or digest(encoded(current)) != head['sha256']
            or not isinstance(current['units'], list)):
        raise ContractError('Recovery ledger generation is incomplete or corrupt')
    if current['generation'] == 0:
        if current['previous_generation_sha256'] is not None:
            raise ContractError('Initial recovery ledger cannot replace prior history')
    else:
        expected_previous = current['previous_generation_sha256']
        remaining = sharing_limits.MAX_PRIVATE - len(encoded(current))
        for generation in range(head['generation'] - 1, -1, -1):
            raw_previous = state.read_bytes(directory / 'ledger' / (str(generation).zfill(8) + '.json'), remaining)
            remaining -= len(raw_previous)
            previous = load(raw_previous)
            if (digest(raw_previous) != expected_previous or encoded(previous) != raw_previous
                    or previous.get('generation') != generation or previous.get('operation_id') != operation_id
                    or previous.get('project_id') != row['project_id']):
                raise ContractError('Recovery ledger successor lost its complete immutable predecessor chain')
            expected_previous = previous.get('previous_generation_sha256')
        if expected_previous is not None:
            raise ContractError('Recovery ledger history does not terminate at its original generation')
    seen = set()
    for unit in current['units']:
        extra = {'child_unit_ids'} if unit.get('state') == 'superseded-by-revision' else set()
        if (not isinstance(unit, dict) or set(unit) != UNIT_FIELDS | extra or not uuid_valid(unit['unit_id'])
                or unit['unit_id'] in seen or not uuid_valid(unit['detecting_developer_id'])
                or unit['state'] not in TERMINAL | {'pending-admission', 'superseded-by-revision'}
                or not isinstance(unit['terminal_receipts'], list) or not isinstance(unit['required_actions'], list)
                or not isinstance(unit['source_ranges'], list) or not unit['source_ranges']):
            raise ContractError('Recovery unit is missing, duplicated or has an unreviewed disposition')
        seen.add(unit['unit_id'])
        for source_range in unit['source_ranges']:
            range_value(source_range)
        if unit['state'] in TERMINAL:
            if not unit['terminal_receipts']:
                raise ContractError('Terminal recovery unit has no actual durable resolution evidence')
            for receipt in unit['terminal_receipts']:
                if (not isinstance(receipt, dict) or set(receipt) != {'resolution_id', 'sha256'}
                        or not uuid_valid(receipt['resolution_id']) or not state.sha(receipt['sha256'])):
                    raise ContractError('Recovery terminal receipt linkage is invalid')
                terminal = state.read(directory / 'resolutions' / (receipt['resolution_id'] + '.json'))
                if (digest(encoded(terminal)) != receipt['sha256'] or terminal.get('unit_id') != unit['unit_id']
                        or terminal.get('operation_id') != operation_id
                        or terminal.get('terminal_state') not in {unit['state'], 'pending-admission'}):
                    raise ContractError('Recovery terminal receipt is missing or mismatched')
            if not any(state.read(directory / 'resolutions' / (receipt['resolution_id'] + '.json')).get('terminal_state') == unit['state']
                    for receipt in unit['terminal_receipts']):
                raise ContractError('Partial coverage receipts cannot falsely declare a whole unit terminal')
        if extra and (not isinstance(unit['child_unit_ids'], list) or not unit['child_unit_ids']
                or any(not uuid_valid(child) for child in unit['child_unit_ids'])):
            raise ContractError('Recovery unit revision is incomplete')
    for unit in current['units']:
        if unit['state'] == 'superseded-by-revision' and not set(unit['child_unit_ids']).issubset(seen):
            raise ContractError('Recovery revision omitted a retained child unit')
    return directory, head, current


def range_value(value):
    if (not isinstance(value, dict) or set(value) != {'source_id', 'start', 'end'}
            or not uuid_valid(value['source_id']) or type(value['start']) is not int or type(value['end']) is not int
            or not 0 <= value['start'] < value['end'] <= 1048576):
        raise ContractError('Coverage requires exact bounded original source ranges')
    return value


def evidence(directory, current, row):
    manifest = state.read(directory / 'manifest.json')
    unit_map = state.read(directory / 'unit-map.json')
    if digest(encoded(unit_map)) != current['unit_map_sha256']:
        unit_map = state.read(directory / 'unit-maps' / (current['unit_map_sha256'] + '.json'))
    proposal = state.read(directory / 'proposals' / (row['proposal_id'] + '.json'))
    for value, key in ((manifest, 'manifest_sha256'), (unit_map, 'unit_map_sha256'), (proposal, 'proposal_sha256')):
        if digest(encoded(value)) != current[key]:
            raise ContractError('Recovery source, unit map or reviewed proposal changed')
    if manifest.get('observation_complete') is not True or manifest.get('project_id') != row['project_id']:
        raise ContractError('Recovery source identity or complete observation is unavailable')
    review = proposal.get('review')
    if review is None:
        review_id = proposal.get('review_id')
        if not uuid_valid(review_id):
            raise ContractError('Recovery requires retained independent source-bound review')
        review = state.read(directory / 'reviews' / (review_id + '.json'))
    if digest(encoded(review)) != proposal.get('review_sha256'):
        raise ContractError('Recovery independent review does not match its reviewed proposal')
    source_entries = manifest['sources']
    sources = {source['source_id']: source for source in source_entries}
    if len(sources) != len(source_entries):
        raise ContractError('Recovery manifest duplicates source identities')
    for unit in current['units']:
        if unit['state'] != 'pending-admission':
            continue
        for source_range in unit['source_ranges']:
            source = sources.get(source_range['source_id'])
            if source is None or source_range['end'] > source['byte_count']:
                raise ContractError('Pending recovery range is outside its exact original source')
            # Never follow a caller-controlled private locator.
            raw = state.read_bytes(directory / 'originals' / (source['source_id'] + '.bin'), 1048576)
            if len(raw) != source['byte_count'] or digest(raw) != source['sha256']:
                raise ContractError('Pending recovery original is missing, damaged or changed')
    return manifest, proposal


def retained_handoff(root, operation, row, manifest, current_operation, index):
    """A retained obligation needs its own hold or an exact reviewed successor."""
    seen = set()
    successor = current_operation
    while successor != operation:
        if successor is None or successor in seen:
            raise ContractError('Retained rolled-back dependency lost its verified successor hold')
        seen.add(successor)
        successor_row = index['operations'].get(successor)
        if successor_row is None:
            raise ContractError('Recovery handoff names an absent successor')
        directory, _, current = ledger(root, successor, successor_row)
        source, proposal = evidence(directory, current, successor_row)
        predecessor = source.get('predecessor')
        if (not isinstance(predecessor, dict) or set(predecessor) != {'operation_id', 'marker_sha256', 'frontier'}
                or proposal.get('predecessor') != predecessor):
            raise ContractError('Retained rollback has no exact independently reviewed handoff')
        previous = predecessor['operation_id']
        previous_row = index['operations'].get(previous)
        if previous_row is None or previous_row['state'] != 'rolled-back':
            raise ContractError('Recovery handoff predecessor is absent or no longer retained')
        expected_marker = encoded({'schema': HOLD, 'project_id': previous_row['project_id'],
            'operation_id': previous, 'hold_id': previous_row['hold_id']})
        pinned = predecessor['frontier']
        heads = [head for head in pinned['ledger_heads'] if head['operation_id'] == previous]
        if (predecessor['marker_sha256'] != digest(expected_marker)
                or pinned['hold_sha256'] != digest(expected_marker)
                or pinned['bank_path'] != str(state.paths(root)[1])
                or pinned['project_id'] != row['project_id'] or len(heads) != 1
                or source['identity']['scope_context'] != manifest['identity']['scope_context']):
            raise ContractError('Recovery handoff changed its predecessor identity or frontier evidence')
        old = state.read(state.paths(root, previous)[3] / 'ledger' / (str(heads[0]['generation']).zfill(8) + '.json'))
        if digest(encoded(old)) != heads[0]['sha256'] or old['operation_id'] != previous:
            raise ContractError('Recovery handoff lost its exact retained predecessor generation')
        successor = previous




def observe(repo, owner=None, transaction=None):
    return _observe_index(repo, owner, transaction, state.index(repo))


def _observe_index(repo, owner, transaction, index):
    """Internal read-only view after exact known-transition validation only."""
    root, bank, base, _ = state.paths(repo)
    raw = marker_bytes(bank, expected=transaction)
    sentinel_raw = marker_bytes(bank, '.agent-memory-recovery.md')
    raw_value = load(raw) if raw is not None else {}
    if not isinstance(raw_value, dict):
        raise ContractError('Recovery observation found a malformed canonical transaction marker')
    marker_operation = raw_value.get('operation_id')
    active = [(operation, row) for operation, row in index['operations'].items()
              if row['state'] in {'active', 'interrupted'} or (
                  row['state'] == 'rolled-back' and operation == marker_operation)]
    if len(active) > 1:
        raise ContractError('Competing recovery operations require explicit reconciliation')
    child_parent = None
    if raw is not None and owner is not None and transaction == raw:
        candidate_marker = load(raw)
        if (isinstance(candidate_marker, dict) and candidate_marker.get('schema') == TRANSACTION
                and candidate_marker.get('operation_id') not in index['operations']):
            child_parent = verified_child_transaction(root, owner, raw, index)
    for bound_id, binding in state.bindings()['bindings'].items():
        if binding['bank_path'] != str(bank) or binding['lifecycle_state'] not in {'active', 'interrupted'}:
            continue
        indexed_row = index['operations'].get(bound_id)
        known_child = child_parent is not None and transaction is not None and marker_value(transaction)['operation_id'] == bound_id
        if not known_child and (indexed_row is None or indexed_row['state'] not in {'active', 'interrupted', 'rolled-back'}):
            raise ContractError('An active bound recovery dependency is missing from the operation index')
    marker = marker_value(raw) if raw is not None and (active or sentinel_raw is not None or child_parent is not None) else None
    if child_parent is not None:
        marker = {**marker, 'operation_id': child_parent}
    if active:
        operation, row = active[0]
        if (marker is None or marker['operation_id'] != operation or marker['project_id'] != row['project_id']
                or (marker.get('hold_id') is not None and marker['hold_id'] != row['hold_id'])):
            raise ContractError('Recovery hold is absent, foreign or changed; semantic coverage remains pending')
        if marker['schema'] != HOLD and (owner is None or transaction != raw):
            raise ContractError('Recovery is interrupted; use the exact known-operation recovery interface')
        expected_sentinel = {'schema': SENTINEL, 'operation_id': operation, 'project_id': row['project_id']}
        if sentinel_raw != encoded(expected_sentinel):
            raise ContractError('Recovery sentinel/index pair is missing, orphaned or changed')
    elif child_parent is None and (sentinel_raw is not None or (raw is not None and load(raw).get('schema') in {HOLD, NESTED, TRANSACTION})):
        raise ContractError('Orphan recovery marker has no verified private ledger')
    ledgers, unit_ids, source_pins, project_ids, reference_maps = [], [], [], set(), {}
    for operation, row in sorted(index['operations'].items()):
        if row['state'] == 'cleaned':
            continue
        directory, head, current = ledger(root, operation, row)
        manifest, proposal = evidence(directory, current, row)
        mapping = state.read(directory / 'reference-map.json', missing=True)
        if mapping:
            if not isinstance(mapping, dict):
                raise ContractError('Private recovery reference map is malformed')
            reference_maps[operation] = mapping
        project_ids.add(row['project_id'])
        pending = [unit for unit in current['units'] if unit['state'] == 'pending-admission']
        if pending and row['state'] not in {'active', 'rolled-back'}:
            raise ContractError('Recovery index falsely declares unresolved evidence terminal')
        unit_ids.extend(unit['unit_id'] for unit in pending)
        ledgers.append({'operation_id': operation, 'generation': head['generation'], 'sha256': head['sha256']})
        source_pins.append({'operation_id': operation, 'manifest_sha256': current['manifest_sha256'],
            'unit_map_sha256': current['unit_map_sha256'], 'units': pending,
            'quiescence_heads_sha256': digest(state.read_bytes(directory / 'quiescence-attestations.json', missing=True))})
        binding = state.bindings()['bindings'].get(operation)
        if row['state'] in {'active', 'interrupted'} or (
                row['state'] == 'rolled-back' and (pending or operation == marker_operation
                    or (binding is not None and binding['lifecycle_state'] in {'active', 'interrupted'}))):
            if (binding is None or binding['project_id'] != row['project_id']
                    or binding['lifecycle_state'] not in {'active', 'interrupted'}
                    or binding['checkout_key'] != manifest['identity']['scope_context']['checkout_key']):
                raise ContractError('Recovery private-state binding is missing or changed')
            journal = state.read(directory / 'journal.json')
            if (journal.get('operation_id') != operation or journal.get('proposal_sha256') != current['proposal_sha256']
                    or journal.get('manifest_sha256') != current['manifest_sha256']):
                raise ContractError('Recovery journal does not bind the exact reviewed operation')
            if row['state'] == 'rolled-back':
                retained_handoff(root, operation, row, manifest, marker['operation_id'] if marker else None, index)
            if marker is not None and marker['schema'] == NESTED and marker['operation_id'] == operation:
                nested = state.read(directory / 'nested' / marker['nested_id'] / 'journal.json')
                if nested.get('marker') != marker or nested.get('operation_id') != operation:
                    raise ContractError('Nested recovery marker has no exact typed private journal')
    if unit_ids and not active:
        raise ContractError('Retained unresolved source units lost their fail-closed recovery hold')
    if len(project_ids) > 1:
        raise ContractError('Recovery private state conflicts on project identity')
    if project_ids:
        project_id = next(iter(project_ids))
    else:
        from .bank_observation import read_source
        project_id = bank_contract.parse_router(read_source(bank, 'MEMORY.md', 16384).decode('utf-8'))[0]['project-id']
    index_raw = state.read_bytes(base / 'index.json', missing=True)
    if index_raw is not None:
        index_raw = encoded(index)
    token = {'bank_path': str(bank), 'project_id': project_id, 'operation_ids': [row['operation_id'] for row in ledgers],
        'ledger_heads': ledgers, 'frontier_digest': digest(encoded(source_pins)), 'coverage_complete': not unit_ids,
        'unit_ids': sorted(unit_ids), 'reference_map_sha256': digest(encoded(reference_maps)), 'review_digest': None,
        'index_sha256': digest(index_raw), 'hold_sha256': (digest(encoded({
            'schema': HOLD, 'project_id': marker['project_id'], 'operation_id': marker['operation_id'],
            'hold_id': index['operations'][marker['operation_id']]['hold_id']})) if marker is not None and active else None),
        'sentinel_sha256': digest(sentinel_raw)}
    return token


def check(expected, repo, owner=None, transaction=None):
    current = observe(repo, owner, transaction)
    # Review attestations are comparison proof fields, not mutable ledger state.
    pinned = {**expected, 'review_digest': None}
    if current != pinned:
        raise ContractError('Private recovery frontier changed; reread every pending source before proceeding')
    return current


def verified_hold(repo, owner=None, transaction=None):
    from . import artifact, scope_binding
    token = observe(repo, owner, transaction)
    root, bank, _, _ = state.paths(repo)
    raw = marker_bytes(bank)
    if raw is None:
        return None
    marker = marker_value(raw)
    if marker['schema'] != HOLD and transaction != raw:
        raise ContractError('Only an internally checked persistent hold permits qualified canonical access')
    measured_runtime = artifact.inspect(artifact.PACK)
    capabilities = measured_runtime['compatibility'].get('private_state_capabilities', [])
    if 'layout-frontier-v1' not in capabilities:
        raise ContractError('Active runtime cannot enforce the private recovery frontier')
    row = state.index(root)['operations'][marker['operation_id']]
    directory, _, current = ledger(root, marker['operation_id'], row)
    manifest, proposal = evidence(directory, current, row)
    expected = manifest['identity']['scope_context']
    if row['state'] == 'rolled-back':
        from . import layout_recovery_observation as raw_observer, project_identity
        router, _ = raw_observer.read_source(bank, 'MEMORY.md')
        original = next((source for source in manifest['sources'] if source['path'] == 'MEMORY.md'), None)
        if original is None or len(router) != original['byte_count'] or digest(router) != original['sha256']:
            raise ContractError('Rolled-back identity requires the exact reviewed original raw router')
        identity = project_identity.RawIdentityEvidence.observe(root, router, manifest['identity'].get('identity_review'))
        context = scope_binding.raw_recovery_context(root, identity, owner, raw)
        if context != expected:
            raise ContractError('Rolled-back raw identity differs from the independently reviewed original scope')
    else:
        context = scope_binding.require(root, token['project_id'], expected=expected,
            owned_lock=owner, owned_transaction=raw)
    if context['checkout_key'] != state.bindings()['bindings'][marker['operation_id']]['checkout_key']:
        raise ContractError('Recovery checkout continuity changed')
    from .recovery_quiescence import selected_attestation
    attestation = selected_attestation(root, marker['operation_id'], proposal, measured_runtime['artifact_digest'])
    if attestation is None:
        raise ContractError('Recovery hold has no independently reviewed attestation for the measured executing runtime')
    if (attestation.get('operation_id') != marker['operation_id'] or attestation.get('project_id') != token['project_id']
            or attestation.get('repo_root') != str(root) or attestation.get('bank_path') != str(bank)
            or attestation.get('checkout_key') != context['checkout_key']):
        raise ContractError('Recovery quiescence attestation has stale identity or route pins')
    check(token, root, owner, transaction)
    return VerifiedHold(root, bank, raw, token, context, marker['operation_id'], measured_runtime['artifact_digest'])


def rolled_back_predecessor(repo, owner=None):
    """Typed continuation authority; never accepts an arbitrary expected marker."""
    root, bank, _, _ = state.paths(repo)
    if marker_bytes(bank) is None:
        from .bank_observation import markers
        markers(bank, owner)
        indexed = state.index(root)
        if marker_bytes(bank, '.agent-memory-recovery.md') is not None or any(
                binding['bank_path'] == str(bank) and binding['lifecycle_state'] in {'active', 'interrupted'}
                for binding in state.bindings()['bindings'].values()):
            raise ContractError('Recovery inspection found an orphaned private dependency or sentinel')
        for operation, row in indexed['operations'].items():
            if row['state'] == 'cleaned':
                continue
            directory, _, current = ledger(root, operation, row)
            evidence(directory, current, row)
            if row['state'] in {'active', 'interrupted'} or any(
                    unit['state'] == 'pending-admission' for unit in current['units']):
                raise ContractError('Recovery inspection cannot omit a retained frontier without its hold')
        return None
    held = verified_hold(repo, owner)
    if held is None:
        return None
    if state.index(repo)['operations'][held.operation_id]['state'] != 'rolled-back':
        raise ContractError('Only a completed exact rollback may hand its hold to a fresh reviewed repair')
    return held


def same_hold(first, second):
    return (type(first) is VerifiedHold and type(second) is VerifiedHold
        and all(getattr(first, name) == getattr(second, name) for name in VerifiedHold.__slots__))


def require_scope(repo, project_id=None, *, expected=None, owned_lock=None, owned_transaction=None, **kwargs):
    from . import scope_binding
    root, bank = writes.bank_path(repo)
    if kwargs.get('initialize') and not (bank / 'MEMORY.md').exists():
        return scope_binding.require(root, project_id, expected=expected, owned_lock=owned_lock,
            owned_transaction=owned_transaction, **kwargs)
    raw = marker_bytes(bank)
    if raw is None or load(raw).get('schema') not in {HOLD, NESTED}:
        # Also detects a removed hold with a surviving private binding/index.
        observe(root, owned_lock, owned_transaction)
        return scope_binding.require(root, project_id, expected=expected, owned_lock=owned_lock,
            owned_transaction=owned_transaction, **kwargs)
    held = verified_hold(root, owned_lock, owned_transaction)
    if project_id is not None and held.scope['project_id'] != project_id:
        raise ContractError('Recovery hold belongs to a different project')
    if expected is not None and held.scope != expected:
        raise ContractError('Recovery scope changed during the operation')
    return held.scope


def pending_rows(repo, token, owner=None, transaction=None):
    check(token, repo, owner, transaction)
    rows = []
    for operation, indexed in sorted(state.index(repo)['operations'].items()):
        if indexed['state'] not in {'active', 'rolled-back'}:
            continue
        directory, _, current = ledger(repo, operation, indexed)
        manifest, _ = evidence(directory, current, indexed)
        sources = {source['source_id']: source for source in manifest['sources']}
        for unit in current['units']:
            if unit['state'] != 'pending-admission':
                continue
            pieces = []
            for source_range in unit['source_ranges']:
                source = sources[source_range['source_id']]
                raw = state.read_bytes(directory / 'originals' / (source['source_id'] + '.bin'), 1048576)
                pieces.append(raw[source_range['start']:source_range['end']])
            body = b''.join(pieces)
            if len(body) > sharing_limits.MAX_FILE:
                raise ContractError('An indivisible pending source exceeds the ordinary complete comparison reader')
            text = body.decode('utf-8')
            security.ensure_safe({'pending-source.md': text})
            rows.append({'unit_id': unit['unit_id'], 'operation_id': operation, 'body': text,
                'source_ranges': unit['source_ranges'], 'provenance': unit['provenance'],
                'reason': unit['reason'], 'selection_class': 'comparison-only-not-current-guidance',
                'source_assertions_verified': False})
    if {row['unit_id'] for row in rows} != set(token['unit_ids']):
        raise ContractError('Pending frontier disclosure omitted or substituted a source unit')
    check(token, repo, owner, transaction)
    return rows


def inherited_pending_rows(repo):
    """Read exact rolled-back obligations without trusting an invalid router."""
    rows = []
    indexed = state.index(repo)
    for operation, row in sorted(indexed['operations'].items()):
        if row['state'] != 'rolled-back':
            continue
        directory, _, current = ledger(repo, operation, row)
        manifest, _ = evidence(directory, current, row)
        sources = {source['source_id']: source for source in manifest['sources']}
        for unit in current['units']:
            if unit['state'] != 'pending-admission':
                continue
            body = b''.join(state.read_bytes(directory / 'originals' / (part['source_id'] + '.bin'), 1048576)[part['start']:part['end']]
                for part in unit['source_ranges'])
            if len(body) > sharing_limits.MAX_FILE:
                raise ContractError('Retained rolled-back unit exceeds the complete comparison reader')
            text = body.decode('utf-8')
            security.ensure_safe({'pending-source.md': text})
            rows.append({'unit_id': unit['unit_id'], 'operation_id': operation, 'body': text,
                'source_ranges': unit['source_ranges'], 'provenance': unit['provenance'],
                'reason': unit['reason'], 'selection_class': 'comparison-only-not-current-guidance',
                'source_assertions_verified': False})
    if len({row['unit_id'] for row in rows}) != len(rows):
        raise ContractError('Retained recovery units have conflicting opaque identities')
    return rows


def review_for_comparison(repo, token, owner=None, transaction=None):
    return pending_rows(repo, token, owner, transaction)


def require_complete(repo, owner=None, transaction=None):
    token = observe(repo, owner, transaction)
    if not token['coverage_complete'] or token['hold_sha256'] is not None:
        raise ContractError('Private recovery coverage is incomplete; resolve all units before refresh or publication')
    return token


def public_coverage(token):
    return {'semantic_coverage': 'complete-canonical-observation' if token['coverage_complete'] else 'incomplete-recovery',
        'pending_recovery_unit_ids': token['unit_ids'], 'pending_recovery_units': len(token['unit_ids']),
        'recovery_required_actions': [] if token['coverage_complete'] else ['Review and resolve the retained private recovery units; do not infer semantic absence']}


def invalidate(repo):
    from . import recall_catalog
    try:
        return recall_catalog.invalidate_frontier_transition(repo)
    except (ContractError, OSError, ValueError, recall_catalog.CacheMiss):
        return 'deferred-to-live-frontier-check'


def install(repo, operation_id, proposal, pending_ledger, hold_id, owner, *, predecessor=None):
    from .bank_observation import markers
    root, bank, base, directory = state.paths(repo, operation_id)
    if predecessor is None:
        markers(bank, owner)
    else:
        if not same_hold(rolled_back_predecessor(root, owner), predecessor):
            raise ContractError('Recovery predecessor must be the exact internally verified rolled-back hold')
        markers(bank, owner, predecessor.marker)
    if not uuid_valid(hold_id) or pending_ledger['operation_id'] != operation_id:
        raise ContractError('Recovery installation needs exact operation/hold identities')
    previous = state.index(root)
    if any(row['state'] in {'active', 'interrupted'} for row in previous['operations'].values()):
        raise ContractError('Another unresolved recovery operation already protects this bank')
    state.write_once(directory / 'ledger' / '00000000.json', pending_ledger)
    head = {'schema': 'agent-memory/layout-ledger-head-v1', 'operation_id': operation_id,
        'generation': 0, 'sha256': digest(encoded(pending_ledger))}
    state.write_once(directory / 'ledger-head.json', head)
    writes.fault('layout-recovery-index-intent')
    old = digest(state.read_bytes(base / 'index.json', missing=True))
    previous['operations'][operation_id] = {'state': 'active', 'project_id': pending_ledger['project_id'],
        'ledger_head_sha256': digest(encoded(head)), 'proposal_id': proposal['proposal_id'], 'hold_id': hold_id}
    state.set_index(root, previous, old)
    state.bind(root, operation_id, pending_ledger['project_id'], proposal['scope_context']['checkout_key'], 'active')
    # Caller creates and atomically hands off the exact marker before mutation.
    invalidate(root)
    return {'schema': SENTINEL, 'project_id': pending_ledger['project_id'], 'operation_id': operation_id}


def claim_value(claim, row):
    fields = {'unit_id', 'covered_source_ranges', 'target_record_refs', 'coverage'}
    if (not isinstance(claim, dict) or set(claim) != fields or claim['unit_id'] != row['unit_id']
            or claim['coverage'] not in {'full', 'partial', 'none'}
            or not isinstance(claim['covered_source_ranges'], list) or len(claim['covered_source_ranges']) > 2
            or not isinstance(claim['target_record_refs'], list) or len(claim['target_record_refs']) > 2):
        raise ContractError('Pending coverage must be one bounded source-bound claim fragment')
    ranges = claim['covered_source_ranges']
    for value in ranges:
        range_value(value)
        if not any(parent['source_id'] == value['source_id'] and parent['start'] <= value['start']
                and value['end'] <= parent['end'] for parent in row['source_ranges']):
            raise ContractError('Coverage fragment escapes its original reviewed unit')
    ordered = sorted(ranges, key=lambda value: (value['source_id'], value['start']))
    if any(a['source_id'] == b['source_id'] and a['end'] > b['start'] for a, b in zip(ordered, ordered[1:])):
        raise ContractError('Coverage fragment double-counts overlapping original ranges')
    seen = set()
    for ref in claim['target_record_refs']:
        if (not isinstance(ref, dict) or set(ref) != {'id', 'version', 'path', 'resulting_sha256'}
                or not uuid_valid(ref['id']) or type(ref['version']) is not int
                or not 1 <= ref['version'] <= 2147483647 or not isinstance(ref['path'], str)
                or len(ref['path'].encode()) > 64 or not state.sha(ref['resulting_sha256'])):
            raise ContractError('Coverage references need exact bounded record identity, version, path and hash')
        if ref['path'].startswith('ref:'):
            if not uuid_valid(ref['path'][4:]):
                raise ContractError('Private reference locator must be an opaque UUIDv4')
        else:
            bank_contract.path_value(ref['path'])
        key = encoded(ref)
        if key in seen:
            raise ContractError('Coverage fragment duplicates a resulting reference')
        seen.add(key)
    if claim['coverage'] == 'none':
        if ranges or claim['target_record_refs']:
            raise ContractError('No-coverage claim cannot assert source ranges or resulting records')
    elif not ranges or not claim['target_record_refs']:
        raise ContractError('Coverage requires actual source ranges and resulting record references')
    if claim['coverage'] == 'full':
        expected = sorted(row['source_ranges'], key=lambda value: (value['source_id'], value['start']))
        merged = []
        for value in ordered:
            if merged and merged[-1]['source_id'] == value['source_id'] and merged[-1]['end'] == value['start']:
                merged[-1]['end'] = value['end']
            else:
                merged.append(dict(value))
        if merged != expected:
            raise ContractError('A full claim must cover the entire original unit, including framing and conditions')
    return claim


def proof_input(proof):
    value = copy.deepcopy(proof)
    value['judgment']['observation']['recovery_frontier']['review_digest'] = None
    return digest(encoded(value))


def validate_comparison_review(repo, proof, owner=None, transaction=None):
    token = proof['judgment']['observation']['recovery_frontier']
    check(token, repo, owner, transaction)
    claims = [decision['coverage_claim'] for decision in proof['judgment']['pending_decisions']]
    if not token['unit_ids']:
        if token['review_digest'] is not None or claims:
            raise ContractError('An empty checked frontier cannot substitute a private coverage review')
        return None
    if not state.sha(token['review_digest']):
        raise ContractError('Pending-source comparison requires an independently retained complete coverage review')
    reviews = []
    fields = {'schema', 'review_id', 'operation_id', 'reviewer_developer_id', 'reviewer_harness',
        'reviewer_session', 'review_time', 'manifest_sha256', 'unit_map_sha256', 'ledger_head_sha256',
        'comparison_proof_input_sha256', 'intended_output_sha256', 'claims', 'rationale', 'decision'}
    for operation in token['operation_ids']:
        _, _, _, directory = state.paths(repo, operation)
        path = directory / 'coverage-reviews' / (token['review_digest'] + '.json')
        review = state.read(path, missing=True)
        if review is None:
            continue
        indexed = state.index(repo)['operations'][operation]
        _, head, current = ledger(repo, operation, indexed)
        if (not isinstance(review, dict) or set(review) != fields
                or review['schema'] != 'agent-memory/layout-coverage-review-v1'
                or digest(encoded(review)) != token['review_digest'] or not uuid_valid(review['review_id'])
                or review['operation_id'] != operation or not uuid_valid(review['reviewer_developer_id'])
                or review['manifest_sha256'] != current['manifest_sha256']
                or review['unit_map_sha256'] != current['unit_map_sha256']
                or review['ledger_head_sha256'] != head['sha256'] or review['decision'] != 'approve'
                or review['comparison_proof_input_sha256'] != proof_input(proof)
                or review['claims'] != claims or not state.sha(review['intended_output_sha256'])):
            raise ContractError('Independent coverage review is stale, incomplete or bound to another proposed output')
        from . import lifecycle, comparison
        lifecycle.time_value(review['review_time'])
        comparison.safe_text(review['rationale'], 4096)
        if not review['reviewer_harness'] or not review['reviewer_session']:
            raise ContractError('Coverage review needs the actual independent reviewing harness/session')
        reviews.append(review)
    if len(reviews) != 1:
        raise ContractError('Pending comparison needs one exact retained independent review')
    return reviews[0]


def reservation(files, pending, token):
    """Serialize extremal proofs for every actual mandatory-identity witness."""
    from . import records
    checked = bank_contract.validate_files(files)
    rows, captures = [], []
    identities = {str(uuid.uuid4())}
    for entry in checked['entries']:
        if entry['format'] != 'standalone-record':
            continue
        _, metadata, body = records.parse(files[entry['path']])
        identities.add(metadata['id'])
        row = {'path': entry['path'], 'sha256': digest(files[entry['path']].encode()), 'format': entry['format'],
            'body': body, 'metadata': metadata, 'scope_chain': entry['scope_chain'],
            'lifecycle': entry['lifecycle'], 'matched_searches': ['x' * 256] * 8,
            'selection_class': 'comparison-only-not-current-guidance', 'source_assertions_verified': False}
        rows.append(row)
        if entry['path'].startswith('history/sources/'):
            captured = records.source_capture_id(entry['path'], metadata)
            if captured is not None:
                identities.add(captured)
            captures.append((captured, row))
    for row in pending:
        if len(row['body'].encode()) > sharing_limits.MAX_FILE:
            raise ContractError('Pending semantic unit is indivisible within the ordinary source bound')
        security.ensure_safe({'pending-source.md': row['body']})
    maximum = {'rows': 0, 'disclosure_bytes': 0, 'proof_bytes': 0, 'witness_identity': None}
    for identity in sorted(identities):
        owners = {row['metadata']['id'] for captured, row in captures if captured == identity}
        owners.update(ref['id'] for captured, row in captures if captured == identity
            for ref in row['metadata'].get('superseded-by', []))
        mandatory = [row for row in rows if row['metadata']['id'] in {identity, *owners} or any(
            captured is None and source['path'] == row['path'] and any(ref['id'] == identity
            for ref in source['metadata']['derived-from']) for captured, source in captures)]
        disclosure = sum(len(encoded(row)) for row in [*pending, *mandatory])
        observation = {'schema': 'agent-memory/comparison-observation-v2', 'project_id': checked['project_id'],
            'request_sha256': 'f' * 64, 'bank_observation_sha256': 'f' * 64, 'router_sha256': 'f' * 64,
            'recovery_frontier': {**token, 'review_digest': 'f' * 64},
            'matches': [{'path': row['path'], 'sha256': row['sha256']} for row in mandatory]}
        fragments = []
        for row in pending:
            fragments.append({'unit_id': row['unit_id'], 'classification': 'distinct-conditions', 'rationale': '"' * 2048,
                'coverage_claim': {'unit_id': row['unit_id'], 'coverage': 'partial',
                    'covered_source_ranges': [{'source_id': 'ffffffff-ffff-4fff-bfff-ffffffffffff',
                        'start': 1048574, 'end': 1048576}] * 2,
                    'target_record_refs': [{'id': 'ffffffff-ffff-4fff-bfff-ffffffffffff', 'version': 2147483647,
                        'path': '"' * 64, 'resulting_sha256': 'f' * 64}] * 2}})
        skeleton = {'schema': 'agent-memory/comparison-proof-v2', 'judgment': {'observation': observation,
            'decisions': [{'path': row['path'], 'classification': 'distinct-conditions', 'rationale': '"' * 2048}
                for row in mandatory], 'pending_decisions': fragments, 'rationale': '"' * 2046},
            'disposition': 'provenance-identity-review-required',
            'targets': [{'path': row['path'], 'sha256': row['sha256'], 'id': row['metadata']['id'],
                'version': row['metadata']['version'], 'required_preserved_metadata': row['metadata'],
                'scope_chain': row['scope_chain']} for row in mandatory]}
        proof_bytes = len(encoded(skeleton))
        count = len(pending) + len(mandatory)
        if count > 64 or disclosure > 65536 or proof_bytes > 65536:
            raise ContractError('Mandatory recovery frontier reservation cannot guarantee a complete bounded comparison')
        if proof_bytes >= maximum['proof_bytes']:
            maximum = {'rows': count, 'disclosure_bytes': disclosure, 'proof_bytes': proof_bytes, 'witness_identity': identity}
    return maximum


def advance_ledger(repo, operation_id, units, expected_head, owner, transaction=None, *, unit_map_sha256=None):
    from .layout_resolution import advance_ledger as advance
    return advance(repo, operation_id, units, expected_head, owner, transaction, unit_map_sha256=unit_map_sha256)


def restore_hold(repo, operation_id, owner, transaction):
    from .layout_resolution import restore_hold as restore
    return restore(repo, operation_id, owner, transaction)


def retire(repo, operation_id, owner, transaction=None):
    from .layout_resolution import retire as finish
    return finish(repo, operation_id, owner, transaction)


def resolve(repo, operation_id=None, unit_id=None, terminal=None, proposal=None):
    from .layout_resolution import resolve as resolution
    return resolution(repo, operation_id, unit_id, terminal, proposal)


def resolve_reference(repo, reference):
    found = []
    for operation, row in state.index(repo)['operations'].items():
        if row['state'] not in {'active', 'terminal'}:
            continue
        directory = state.paths(repo, operation)[3]
        mapping = state.read(directory / 'reference-map.json', missing=True) or {}
        if reference['path'] in mapping:
            actual = mapping[reference['path']]
            if (not isinstance(actual, dict) or set(actual) != {'id', 'version', 'path', 'resulting_sha256'}
                    or actual['id'] != reference['id'] or actual['resulting_sha256'] != reference['resulting_sha256']
                    or type(actual['version']) is not int or actual['version'] < 1 or reference['version'] != 1):
                raise ContractError('Private reference locator does not bind its real record identity/version/hash')
            bank_contract.path_value(actual['path'])
            found.append(actual)
    if len(found) != 1:
        raise ContractError('Private record reference is missing, ambiguous or foreign')
    return found[0]


def recover_nested(repo, operation_id=None, nested_id=None, mode=None, proposal=None):
    from .recovery_transactions import recover_nested as recover
    return recover(repo, operation_id, nested_id, mode, proposal)


def capture_scope(repo):
    """Preserve advisory unbound previews, never weaken a visible recovery hold."""
    from .bank_capacity import BankCapacityError
    try:
        return require_scope(repo)
    except BankCapacityError:
        raise
    except (ContractError, OSError, ValueError):
        root, bank, _, _ = state.paths(repo)
        if marker_bytes(bank) is not None or any(row['state'] in {'active', 'interrupted'}
                for row in state.index(root)['operations'].values()):
            raise
        return None


def private_preview(repo, value, output=None):
    token = observe(repo)
    if value.get('proposal') is not None and token['unit_ids']:
        if output is None:
            raise ContractError('Recovery-bound proposal contains private coverage claims; use checked --private-output')
        return {'schema': value['schema'], 'operation': value['operation'],
            'private_artifact': sharing_limits.write_private(output, value, repo),
            'pending_recovery_units': len(token['unit_ids']), 'canonical_writes_performed': False}
    return value


def validate_quiescence(repo, value, *, runtime_artifact_digest, required_routes=(), publish=False, owner=None):
    from .recovery_quiescence import validate
    return validate(repo, value, runtime_artifact_digest=runtime_artifact_digest,
        required_routes=required_routes, publish=publish, owner=owner)


def verified_child_transaction(repo, owner, transaction, indexed):
    """Allow only exact owned child readback, never an ordinary semantic reader."""
    from .bank_observation import markers
    from . import scope_binding
    marker = marker_value(transaction)
    root, bank, _, directory = state.paths(repo, marker['operation_id'])
    markers(bank, owner, transaction)
    journal = state.read(directory / 'journal.json')
    p = state.read(directory / 'proposals' / (journal['proposal_id'] + '.json'))
    if (journal.get('schema') != 'agent-memory/layout-recovery-journal-v1'
            or journal.get('operation_id') != marker['operation_id'] or journal.get('marker') != marker
            or p.get('schema') != 'agent-memory/layout-child-restore-v1'
            or p.get('operation_id') != marker['operation_id'] or digest(encoded(p)) != journal.get('proposal_sha256')
            or p.get('manifest_sha256') != journal.get('manifest_sha256')
            or p.get('parent_operation_id') != journal.get('parent_operation_id')
            or p.get('parent_receipt_sha256') != journal.get('parent_receipt_sha256')):
        raise ContractError('Child recovery marker lacks its exact typed source-bound parent journal')
    parent = p['parent_operation_id']
    row = indexed['operations'].get(parent)
    if row is None or row['project_id'] != marker['project_id']:
        raise ContractError('Child recovery cannot infer an absent or foreign parent operation')
    parent_directory, head, parent_ledger = ledger(root, parent, row)
    parent_manifest, _ = evidence(parent_directory, parent_ledger, row)
    receipt = state.read(parent_directory / 'receipt.json')
    if (digest(encoded(receipt)) != p['parent_receipt_sha256'] or receipt.get('verified') is not True
            or digest(encoded(head)) != p['parent_ledger_head_sha256']):
        raise ContractError('Child recovery parent receipt or ledger dependency changed')
    child_manifest = state.read(directory / 'manifest.json')
    if digest(encoded(child_manifest)) != p['manifest_sha256'] or p['scope_context'] != child_manifest['identity']['scope_context']:
        raise ContractError('Child recovery identity evidence is missing or changed')
    from . import layout_recovery_observation as raw_observer, project_identity
    router, _ = raw_observer.read_source(bank, 'MEMORY.md')
    identity = project_identity.RawIdentityEvidence.observe(root, router, parent_manifest['identity'].get('identity_review'))
    context = scope_binding.raw_recovery_context(root, identity, owner, transaction)
    if context != p['scope_context'] or context['project_id'] != marker['project_id']:
        raise ContractError('Owned child restore raw identity differs from its exact reviewed parent scope')
    markers(bank, owner, transaction)
    return parent


def prospective_token(repo, manifest, pending_units, *, predecessor=None, owner=None):
    """Capacity witness only: include all checked prior terminal private state."""
    root, bank, _, _ = state.paths(repo)
    if (manifest.get('observation_complete') is not True or manifest.get('repo_root') != str(root)
            or manifest.get('bank_path') != str(bank) or not uuid_valid(manifest.get('operation_id'))
            or manifest['identity']['scope_context']['project_id'] != manifest['project_id']):
        raise ContractError('Recovery reservation requires the actual complete independently bound raw manifest')
    index = state.index(root)
    if any(row['state'] in {'active', 'interrupted'} for row in index['operations'].values()):
        raise ContractError('A prospective repair cannot omit a competing private recovery frontier')
    if predecessor is None:
        if marker_bytes(bank) is not None or marker_bytes(bank, '.agent-memory-recovery.md') is not None:
            raise ContractError('A prospective repair cannot replace an unknown transaction or sentinel')
    elif not same_hold(rolled_back_predecessor(root, owner), predecessor):
        raise ContractError('A prospective continuation must retain the exact checked rollback frontier')
    operations, heads, references = [], [], {}
    for operation, row in sorted(index['operations'].items()):
        if row['state'] == 'cleaned':
            continue
        directory, head, current = ledger(root, operation, row)
        evidence(directory, current, row)
        if row['project_id'] != manifest['project_id'] or (row['state'] != 'rolled-back'
                and any(unit['state'] == 'pending-admission' for unit in current['units'])):
            raise ContractError('Prior private recovery evidence is foreign, unresolved or falsely terminal')
        mapping = state.read(directory / 'reference-map.json', missing=True)
        if mapping:
            references[operation] = mapping
        operations.append(operation)
        heads.append({'operation_id': operation, 'generation': head['generation'], 'sha256': head['sha256']})
    operation = manifest['operation_id']
    if operation in index['operations']:
        raise ContractError('A new raw reservation cannot reuse an existing recovery operation')
    operations.append(operation)
    heads.append({'operation_id': operation, 'generation': 0, 'sha256': 'f' * 64})
    unit_ids = sorted([unit['unit_id'] for unit in pending_units if unit.get('state', 'pending-admission') == 'pending-admission']
        + [row['unit_id'] for row in inherited_pending_rows(root)])
    return {'bank_path': str(bank), 'project_id': manifest['project_id'], 'operation_ids': sorted(operations),
        'ledger_heads': sorted(heads, key=lambda row: row['operation_id']), 'frontier_digest': 'f' * 64,
        'coverage_complete': not unit_ids, 'unit_ids': unit_ids, 'reference_map_sha256': digest(encoded(references)),
        'review_digest': None, 'index_sha256': 'f' * 64, 'hold_sha256': 'f' * 64 if unit_ids else None,
        'sentinel_sha256': 'f' * 64 if unit_ids else None}


def successor_token(token, pending_unit_ids, *, operation_id, generation_change=True):
    """A capacity-only successor; it is never accepted as an observed proof."""
    result = copy.deepcopy(token)
    result['unit_ids'] = sorted(pending_unit_ids)
    result['coverage_complete'] = not pending_unit_ids
    if generation_change:
        for head in result['ledger_heads']:
            if head['operation_id'] == operation_id:
                head['generation'] += 1
                head['sha256'] = 'f' * 64
    if not pending_unit_ids:
        result['hold_sha256'] = None
        result['sentinel_sha256'] = None
    result['review_digest'] = None
    return result
