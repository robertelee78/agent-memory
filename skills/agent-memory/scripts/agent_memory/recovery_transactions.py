"""Typed held transactions; legacy recoverers can never consume these journals."""
from __future__ import annotations

import copy
from pathlib import Path
import uuid

from . import bank_contract, bank_observation, bank_transactions as tx, bank_writes as writes
from . import layout_recovery_state as state, layout_resolution as resolution, local_registry as storage
from . import recovery_frontier as frontier, security
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError


def marker_for(held, nested_id):
    original = frontier.marker_value(held.marker)
    return {'schema': frontier.NESTED, 'project_id': original['project_id'],
        'operation_id': original['operation_id'], 'hold_id': original['hold_id'], 'nested_id': nested_id}


def source_check(p, owner, marker):
    tx.validate_outcome(Path(p['repo_root']), writes.bank_path(p['repo_root'])[1], p['files'], p, owner, marker)
    evidence, proof, _ = resolution.transaction_evidence(p)
    if proof is not None:
        frontier.check(proof['judgment']['observation']['recovery_frontier'], p['repo_root'], owner, marker)


def validate(p, owner):
    if 'checkpoint_evidence' in p:
        from .checkpoint import validate_preparation
        validate_preparation(p)
        return [], None
    if 'admission_evidence' in p:
        from .admission import validate_transaction
    elif 'governance_evidence' in p:
        from .governance import validate_transaction
    elif 'consolidation_evidence' in p:
        from .consolidation import validate_transaction
    else:
        raise ContractError('Recovery hold permits only fully evidenced governed learning, not raw replacement')
    validate_transaction(p, owned_lock=owner)
    return resolution.claims_for(p, owner=owner)


def credited_rows(repo, token, claims, owner=None, marker=None):
    rows = frontier.pending_rows(repo, token, owner, marker)
    full = {claim['unit_id'] for claim in claims if claim['coverage'] == 'full'}
    return [row for row in rows if row['unit_id'] not in full]


def resolution_plan(p, held, claims, review):
    indexed = state.index(p['repo_root'])
    remaining = {claim['unit_id']: claim for claim in claims if claim['coverage'] != 'none'}
    plans = []
    for operation_id, row in sorted(indexed['operations'].items()):
        if row['state'] not in {'active', 'rolled-back'}:
            continue
        _, head, current = frontier.ledger(p['repo_root'], operation_id, row)
        units = copy.deepcopy(current['units'])
        resolutions = []
        for unit in units:
            claim = remaining.pop(unit['unit_id'], None)
            if claim is None:
                continue
            if unit['state'] != 'pending-admission':
                raise ContractError('A coverage claim cannot reterminalize a historical unit')
            identity = str(uuid.UUID(hex=digest(encoded([p['operation_id'], unit['unit_id']]))[:32], version=4))
            terminal_state = ('admitted' if 'admission_evidence' in p else 'governed-terminal') if claim['coverage'] == 'full' else 'pending-admission'
            resolutions.append({'resolution_id': identity, 'unit_id': unit['unit_id'], 'claim': claim,
                'terminal_state': terminal_state, 'review_sha256': digest(encoded(review))})
        if resolutions:
            plans.append({'operation_id': operation_id, 'previous_head': head, 'units': units, 'resolutions': resolutions})
    if remaining:
        raise ContractError('Coverage claim omitted its exact retained ledger')
    return {'ledgers': plans}


def planned_resolution(plan, journal, receipt, nested_id):
    resolved_operation = plan['operation_id']
    previous = state.read(state.paths(journal['proposal']['repo_root'], resolved_operation)[3]
        / 'ledger' / (str(plan['previous_head']['generation']).zfill(8) + '.json'))
    if (digest(encoded(previous)) != plan['previous_head']['sha256'] or previous['units'] != plan['units']
            or {'operation_id': resolved_operation, 'generation': plan['previous_head']['generation'],
                'sha256': plan['previous_head']['sha256']} not in journal['frontier']['ledger_heads']):
        raise ContractError('Nested resolution plan changed its exact original ledger evidence')
    evidence, proof, _ = resolution.transaction_evidence(journal['proposal'])
    claims = evidence['unit_resolution_claims'] if evidence is not None else []
    units, records = copy.deepcopy(plan['units']), []
    for item in plan['resolutions']:
        expected_id = str(uuid.UUID(hex=digest(encoded([nested_id, item['unit_id']]))[:32], version=4))
        terminal_state = ('admitted' if 'admission_evidence' in journal['proposal'] else 'governed-terminal'
            ) if item['claim']['coverage'] == 'full' else 'pending-admission'
        if (item['resolution_id'] != expected_id or item['claim'] not in claims
                or item['claim']['unit_id'] != item['unit_id'] or item['claim']['coverage'] == 'none'
                or item['terminal_state'] != terminal_state or proof is None
                or item['review_sha256'] != proof['judgment']['observation']['recovery_frontier']['review_digest']):
            raise ContractError('Nested resolution lost its exact proof-bound claim and independent review')
        record = {'schema': 'agent-memory/layout-resolution-receipt-v1',
            'resolution_id': item['resolution_id'], 'operation_id': resolved_operation, 'unit_id': item['unit_id'],
            'generation': plan['previous_head']['generation'], 'nested_id': nested_id,
            'request_sha256': digest(encoded(journal['proposal'])), 'canonical_operation_id': nested_id,
            'canonical_receipt_sha256': digest(encoded(receipt)), 'terminal_state': item['terminal_state'],
            'claims': [item['claim']], 'independent_review_sha256': item['review_sha256'], 'verified': True}
        unit = next(unit for unit in units if unit['unit_id'] == item['unit_id'])
        if unit['state'] != 'pending-admission':
            raise ContractError('Nested resolution cannot replace a historical terminal unit')
        unit['state'] = item['terminal_state']
        unit['terminal_receipts'].append({'resolution_id': item['resolution_id'], 'sha256': digest(encoded(record))})
        records.append(record)
    return units, records


def terminalize(root, operation_id, nested_id, journal, receipt, owner, marker):
    directory = state.paths(root, operation_id)[3]
    for plan in journal['resolution_plan']['ledgers']:
        resolved_operation = plan['operation_id']
        resolved_directory = state.paths(root, resolved_operation)[3]
        units, records = planned_resolution(plan, journal, receipt, nested_id)
        for record in records:
            state.write_once(resolved_directory / 'resolutions' / (record['resolution_id'] + '.json'), record)
        writes.fault('layout-recovery-admission-receipt-before-resolution')
        resolution.advance_ledger(root, resolved_operation, units, plan['previous_head'], owner, marker)
    token = frontier.observe(root, owner, marker)
    pending = frontier.pending_rows(root, token, owner, marker)
    frontier.reservation(journal['proposal']['files'], pending, token)
    state.write_once(directory / 'nested' / nested_id / 'completion.json', {
        'schema': 'agent-memory/layout-nested-completion-v1', 'operation_id': operation_id,
        'nested_id': nested_id, 'canonical_receipt_sha256': digest(encoded(receipt)),
        'ledger_head_sha256': state.index(root)['operations'][operation_id]['ledger_head_sha256'], 'verified': True})
    if pending:
        resolution.restore_hold(root, operation_id, owner, marker)
    else:
        resolution.retire(root, operation_id, owner, marker)


def apply_transaction(prepared):
    p = tx.proposal_value(prepared)
    root, bank = writes.bank_path(p['repo_root'])
    _, receipt_path = tx.locations(root, bank, p['operation_id'])
    known = tx.receipt_value(receipt_path, p)
    if known is not None:
        # Never demand that a historical base reappear or replay over later edits.
        return tx.result('apply', **{key: value for key, value in known.items() if key != 'schema'},
            writes_performed=False, already_applied=True, receipt_is_historical=True,
            still_current=None, receipt_path=str(receipt_path), interrupted=frontier.marker_bytes(bank) is not None)
    with storage.exclusive(bank / '.agent-memory.lock', p['operation_id'], 'bank-' + p['operation']) as owner:
        resolution.require_completed_resolutions(root)
        held = frontier.verified_hold(root, owner)
        if held is None or held.scope != p['scope_context'] or writes.snapshot(root, owner)[1] != p['observed_hashes']:
            raise ContractError('Held transaction scope or complete canonical base changed after preview')
        claims, review = validate(p, owner)
        token = held.token
        remaining = credited_rows(root, token, claims, owner)
        capacity_token = token
        for ledger_plan in resolution_plan(p, held, claims, review)['ledgers']:
            capacity_token = frontier.successor_token(capacity_token, [row['unit_id'] for row in remaining],
                operation_id=ledger_plan['operation_id'])
        frontier.reservation(p['files'], remaining, capacity_token)
        base_files = tx.files_at(bank)
        tx.preflight(bank, p)
        security.guarded_start(root, p['operation_id'], held.scope,
            {path: p['files'][path] for path in tx.changed(p) if path in p['files']}, ['external'], action=p['operation'])
        marker = marker_for(held, p['operation_id'])
        directory = state.paths(root, held.operation_id)[3] / 'nested' / p['operation_id']
        journal = {'schema': 'agent-memory/layout-nested-journal-v1', 'operation_id': held.operation_id,
            'nested_id': p['operation_id'], 'marker': marker, 'hold': frontier.marker_value(held.marker),
            'proposal': p, 'proposal_sha256': digest(encoded(p)), 'base_files': base_files,
            'frontier': token, 'resolution_plan': resolution_plan(p, held, claims, review)}
        state.write_once(directory / 'journal.json', journal)
        writes.fault('layout-recovery-before-nested-marker')
        resolution.atomic_marker(bank, held.marker, encoded(marker), owner)
        writes.fault('layout-recovery-after-nested-marker')
        running = dict(p['observed_hashes'])
        paths = sorted(tx.changed(p), key=lambda path: (path == 'MEMORY.md', path))
        for index, path in enumerate(paths):
            tx.replace_file(bank, path, p['files'].get(path), running, encoded(marker),
                set(p['observed_hashes']) | set(p['files']), before_replace=lambda: frontier.check(token, root, owner, encoded(marker)))
            if path in p['files']:
                running[path] = digest(p['files'][path].encode())
            else:
                running.pop(path)
            writes.fault('layout-recovery-nested-after-rename:' + path)
            state.write_once(directory / 'progress' / (str(index).zfill(8) + '.json'), {'path': path, 'hashes': running})
        source_check(p, owner, encoded(marker))
        writes.fault('layout-recovery-nested-before-receipt')
        receipt = {'schema': 'agent-memory/transaction-receipt-v1', 'operation_id': p['operation_id'],
            'request_sha256': digest(encoded(p)), 'bank_path': str(bank), 'project_id': p['project_id'],
            'observed_hashes': p['observed_hashes'], 'resulting_hashes': tx.file_hashes(p['files']),
            'outcome': 'finished', 'changed_memory_paths': [bank.relative_to(root).as_posix() + '/' + path for path in tx.changed(p)],
            'readback_at': storage.now(), 'verified': True}
        writes.record(receipt_path, encoded(receipt))
        writes.fault('layout-recovery-nested-after-receipt')
        terminalize(root, held.operation_id, p['operation_id'], journal, receipt, owner, encoded(marker))
        return tx.result('apply', **{key: value for key, value in receipt.items() if key != 'schema'},
            writes_performed=bool(paths), already_applied=False, still_current=True, receipt_path=str(receipt_path),
            interrupted=False, resolved_scope=held.scope)


def legal_partial(p, base, observed):
    final = tx.file_hashes(p['files'])
    running = tx.file_hashes(base)
    if observed == running:
        return
    for path in sorted(tx.changed(p), key=lambda path: (path == 'MEMORY.md', path)):
        if path in final:
            running[path] = final[path]
        else:
            running.pop(path, None)
        if observed == running:
            return
    raise ContractError('Nested recovery found an impossible ordered partial state or an external edit')


def rollback_plan(directory, journal, observed):
    path = directory / 'rollback-plan.json'
    known = state.read(path, missing=True)
    initial = observed if known is None else known['initial_hashes']
    legal_partial(journal['proposal'], journal['base_files'], initial)
    expected = tx.file_hashes(journal['base_files'])
    actions = [{'path': name, 'after': expected.get(name)}
        for name in sorted(set(initial) | set(expected), key=lambda name: (name == 'MEMORY.md', name))
        if initial.get(name) != expected.get(name)]
    plan = {'schema': 'agent-memory/layout-nested-rollback-v1', 'operation_id': journal['operation_id'],
        'nested_id': journal['nested_id'], 'journal_sha256': digest(encoded(journal)),
        'initial_hashes': initial, 'actions': actions}
    if known is not None and known != plan:
        raise ContractError('Nested rollback lost its exact durable original state/action plan')
    running = dict(initial)
    for boundary in range(len(actions) + 1):
        if running == observed:
            return plan, boundary
        if boundary < len(actions):
            action = actions[boundary]
            if action['after'] is None:
                running.pop(action['path'], None)
            else:
                running[action['path']] = action['after']
    raise ContractError('Nested rollback found an impossible ordered boundary or external edit')


def retired_nested(root, operation_id, nested_id, journal, receipt, indexed, owner):
    """Recognize only the terminal prefix of this journal's durable retirement."""
    from . import layout_history
    _, bank, _, directory = state.paths(root, operation_id)
    row = indexed['operations'][operation_id]
    _, head, _ = frontier.ledger(root, operation_id, row)
    retirement = state.read(directory / 'retirement.json')
    canonical = state.read(directory / 'receipt.json')
    completion = state.read(directory / 'nested' / nested_id / 'completion.json')
    expected_completion = {'schema': 'agent-memory/layout-nested-completion-v1',
        'operation_id': operation_id, 'nested_id': nested_id,
        'canonical_receipt_sha256': digest(encoded(receipt)),
        'ledger_head_sha256': digest(encoded(head)), 'verified': True}
    if (receipt is None or completion != expected_completion or canonical.get('verified') is not True
            or not isinstance(retirement, dict)
            or set(retirement) != {'schema', 'operation_id', 'ledger_head', 'receipt_sha256', 'dependencies', 'verified'}
            or retirement['schema'] != 'agent-memory/layout-retirement-v1'
            or retirement['operation_id'] != operation_id or retirement['ledger_head'] != head
            or retirement['receipt_sha256'] != digest(encoded(canonical)) or retirement['verified'] is not True
            or not isinstance(retirement['dependencies'], list) or journal['hold']['hold_id'] != row['hold_id']):
        raise ContractError('Terminal nested recovery needs its exact completed linkage and durable retirement')
    dependencies = set()
    bindings = state.bindings()['bindings']
    for item in retirement['dependencies']:
        if (not isinstance(item, dict) or set(item) != {'operation_id', 'ledger_head_sha256'}
                or item['operation_id'] in dependencies):
            raise ContractError('Nested retirement has an unknown or duplicated dependency')
        dependency = indexed['operations'].get(item['operation_id'])
        binding = bindings.get(item['operation_id'])
        manifest = state.read(state.paths(root, item['operation_id'])[3] / 'manifest.json')
        if (dependency is None or dependency['state'] != 'terminal' or dependency['project_id'] != row['project_id']
                or dependency['ledger_head_sha256'] != item['ledger_head_sha256']
                or binding is None or binding['lifecycle_state'] not in {'active', 'terminal'}
                or binding['project_id'] != row['project_id'] or binding['bank_path'] != str(bank)
                or binding['checkout_key'] != manifest['identity']['scope_context']['checkout_key']):
            raise ContractError('Nested retirement dependency no longer matches its terminal binding and ledger')
        dependencies.add(item['operation_id'])
    if operation_id not in dependencies:
        raise ContractError('Nested retirement omitted its exact holding operation')
    for other_id, other_row in indexed['operations'].items():
        if other_row['state'] == 'cleaned':
            continue
        other_directory, _, current = frontier.ledger(root, other_id, other_row)
        frontier.evidence(other_directory, current, other_row)
        if (other_row['project_id'] != row['project_id'] or other_row['state'] in {'active', 'interrupted'}
                or any(unit['state'] == 'pending-admission' for unit in current['units'])):
            raise ContractError('Retained same-bank recovery evidence still requires its safeguard')
    if any(other_id not in dependencies and binding['bank_path'] == str(bank)
            and binding['lifecycle_state'] in {'active', 'interrupted'}
            for other_id, binding in bindings.items()):
        raise ContractError('An active bound dependency prevents nested retirement recovery')
    scoped = journal.get('write_proposal', journal['proposal'])
    layout_history.check(root, scoped['scope_context'], row['project_id'])
    marker = frontier.marker_bytes(bank)
    sentinel = frontier.marker_bytes(bank, '.agent-memory-recovery.md')
    expected_sentinel = encoded({'schema': frontier.SENTINEL, 'project_id': row['project_id'], 'operation_id': operation_id})
    if (marker not in (None, encoded(journal['marker'])) or sentinel not in (None, expected_sentinel)
            or (marker is not None and sentinel is None)):
        raise ContractError('Nested retirement found a foreign safeguard or impossible removal order')
    bank_observation.markers(bank, owner, marker)
    return {'retirement_sha256': digest(encoded(retirement)), 'completion_sha256': digest(encoded(completion)),
        'marker_sha256': digest(marker), 'sentinel_sha256': digest(sentinel)}


def nested_preview(repo, operation_id, nested_id, mode, owner=None):
    root, bank, _, directory = state.paths(repo, operation_id)
    if mode not in {'finish', 'rollback'} or not uuid_valid(nested_id):
        raise ContractError('Known nested recovery needs the exact operation, UUID and finish/rollback mode')
    journal = state.read(directory / 'nested' / nested_id / 'journal.json')
    fields = {'schema', 'operation_id', 'nested_id', 'marker', 'hold', 'proposal', 'proposal_sha256',
              'base_files', 'frontier', 'resolution_plan'}
    if not isinstance(journal, dict) or set(journal) not in (fields, fields | {'write_proposal'}):
        raise ContractError('Nested recovery requires the exact typed retained journal')
    if (journal.get('schema') != 'agent-memory/layout-nested-journal-v1' or journal.get('operation_id') != operation_id
            or journal.get('nested_id') != nested_id or journal.get('proposal_sha256') != digest(encoded(journal['proposal']))):
        raise ContractError('Unknown, foreign or damaged nested recovery journal')
    marker = encoded(journal['marker'])
    # Only the exact receipt/completion/retirement path below may recognize an
    # already removed nested marker; ordinary frontier observers remain strict.
    p = tx.proposal_value(journal['proposal'])
    original_hold = frontier.marker_value(encoded(journal['hold']))
    expected_marker = {'schema': frontier.NESTED, 'project_id': p['project_id'],
        'operation_id': operation_id, 'nested_id': nested_id, 'hold_id': original_hold.get('hold_id')}
    if (journal['marker'] != expected_marker or original_hold['schema'] != frontier.HOLD
            or original_hold['operation_id'] != operation_id or original_hold['project_id'] != p['project_id']
            or p['operation_id'] != nested_id or p['repo_root'] != str(root)
            or tx.file_hashes(journal['base_files']) != p['observed_hashes']
            or journal['frontier']['project_id'] != p['project_id'] or journal['frontier']['bank_path'] != str(bank)):
        raise ContractError('Nested journal changed its canonical base, marker or exact held scope')
    observed = writes.hashes(bank)
    nested_directory = directory / 'nested' / nested_id
    rollback = None
    if state.read(nested_directory / 'rollback-plan.json', missing=True) is not None:
        if mode != 'rollback':
            raise ContractError('A durable pre-receipt nested rollback cannot become a finish')
        rollback, _ = rollback_plan(nested_directory, journal, observed)
    else:
        legal_partial(p, journal['base_files'], observed)
    _, receipt_path = tx.locations(root, bank, nested_id)
    receipt = nested_receipt(journal, receipt_path)
    indexed = state.index(root)
    original_index_sha256 = digest(state.read_bytes(directory.parent / 'index.json'))
    if receipt is not None:
        for plan in journal['resolution_plan']['ledgers']:
            resolved_directory = state.paths(root, plan['operation_id'])[3]
            head = state.read(resolved_directory / 'ledger-head.json')
            if head != plan['previous_head']:
                units, _ = planned_resolution(plan, journal, receipt, nested_id)
                projected, _, _ = resolution.interrupted_successor(root, plan['operation_id'], units, plan['previous_head'])
                indexed['operations'][plan['operation_id']] = projected
    retirement = None
    if indexed['operations'][operation_id]['state'] == 'terminal':
        retirement = retired_nested(root, operation_id, nested_id, journal, receipt, indexed, owner)
        marker = frontier.marker_bytes(bank)
        frontier_token = None
    else:
        bank_observation.markers(bank, owner, marker)
        frontier_token = frontier._observe_index(root, owner, marker, indexed)
    if receipt is None:
        frontier.check(journal['frontier'], root, owner, marker)
    if receipt is None and mode != 'rollback':
        raise ContractError('Unfinished governed nested learning permits rollback only, then fresh evidence')
    if receipt is not None and (receipt['outcome'] not in {'finished', 'replaced', 'already-present'} or mode != 'finish' or observed != receipt['resulting_hashes']):
        raise ContractError('A durably receipted canonical transaction is finish-only and cannot replay over later edits')
    if receipt is None:
        rollback, _ = rollback_plan(nested_directory, journal, observed)
    return {'schema': 'agent-memory/layout-nested-recovery-v1', 'recovery_id': str(uuid.uuid4()),
        'repo_root': str(root), 'operation_id': operation_id, 'nested_id': nested_id, 'mode': mode,
        'journal_sha256': digest(encoded(journal)), 'marker_sha256': digest(marker), 'observed_hashes': observed,
        'canonical_receipt_sha256': digest(encoded(receipt)) if receipt else None,
        'ledger_head': state.read(directory / 'ledger-head.json'), 'recovery_frontier': frontier_token,
        'ledger_index_sha256': original_index_sha256, 'rollback_plan': rollback, 'retirement': retirement}


def recover_nested(repo, operation_id=None, nested_id=None, mode=None, proposal=None):
    if proposal is None:
        root, bank, _, _ = state.paths(repo, operation_id)
        with storage.exclusive(bank / '.agent-memory.lock', nested_id, 'bank-recover') as owner:
            preview = nested_preview(root, operation_id, nested_id, mode, owner)
        return {'proposal': preview, 'writes_performed': False, 'temporary_lock_writes_performed': True}
    p = proposal.get('proposal', proposal)
    root, bank, _, directory = state.paths(repo, p['operation_id'])
    with storage.exclusive(bank / '.agent-memory.lock', p['recovery_id'], 'bank-recover') as owner:
        current = nested_preview(root, p['operation_id'], p['nested_id'], p['mode'], owner)
        current['recovery_id'] = p['recovery_id']
        if current != p:
            raise ContractError('Nested recovery source, journal, marker, ledger or durable receipt changed')
        journal = state.read(directory / 'nested' / p['nested_id'] / 'journal.json')
        original = journal['proposal']
        marker = encoded(journal['marker'])
        _, receipt_path = tx.locations(root, bank, p['nested_id'])
        if p['mode'] == 'finish':
            receipt = nested_receipt(journal, receipt_path)
            if p['retirement'] is not None:
                resolution.retire(root, p['operation_id'], owner, frontier.marker_bytes(bank))
            else:
                terminalize(root, p['operation_id'], p['nested_id'], journal, receipt, owner, marker)
        else:
            nested_directory = directory / 'nested' / p['nested_id']
            plan, boundary = rollback_plan(nested_directory, journal, p['observed_hashes'])
            if plan != p['rollback_plan']:
                raise ContractError('Nested rollback changed its exact reviewed initial/action pins')
            state.write_once(nested_directory / 'rollback-plan.json', plan)
            writes.fault('layout-recovery-nested-rollback-after-plan')
            running = dict(p['observed_hashes'])
            expected = tx.file_hashes(journal['base_files'])
            for number, action in enumerate(plan['actions'][boundary:], boundary):
                path = action['path']
                tx.replace_file(bank, path, journal['base_files'].get(path), running, marker,
                    set(original['observed_hashes']) | set(original['files']),
                    before_replace=lambda: frontier.check(journal['frontier'], root, owner, marker))
                if path in expected:
                    running[path] = expected[path]
                else:
                    running.pop(path, None)
                writes.fault('layout-recovery-nested-rollback-after-rename:' + path)
                state.write_once(nested_directory / 'rollback-progress' / (str(number).zfill(8) + '.json'),
                    {'path': path, 'hashes': running})
                writes.fault('layout-recovery-nested-rollback-after-progress:' + path)
            bank_contract.validate(root, owned_lock=owner, owned_transaction=marker)
            state.write_once(directory / 'nested' / p['nested_id'] / 'completion.json', {
                'schema': 'agent-memory/layout-nested-completion-v1', 'operation_id': p['operation_id'],
                'nested_id': p['nested_id'], 'outcome': 'rolled-back', 'verified': True})
            resolution.restore_hold(root, p['operation_id'], owner, marker)
        return {'verified': True, 'operation_id': p['operation_id'], 'nested_id': p['nested_id'],
            'mode': p['mode'], 'writes_performed': True, 'canonical_writes_performed': p['mode'] == 'rollback'}


def nested_receipt(journal, path):
    if 'write_proposal' not in journal:
        return tx.receipt_value(path, journal['proposal'])
    write = journal['write_proposal']
    receipt = state.read(path, missing=True)
    if receipt is None:
        return None
    expected = {**write['observed_hashes'], write['path']: digest(write['content'].encode())}
    if (receipt.get('schema') != 'agent-memory/write-receipt-v1' or receipt.get('operation_id') != write['operation_id']
            or receipt.get('request_sha256') != digest(encoded(write)) or receipt.get('project_id') != write['project_id']
            or receipt.get('verified') is not True or receipt.get('resulting_hashes') != expected
            or receipt.get('outcome') not in {'replaced', 'already-present'}):
        raise ContractError('Held checkpoint receipt is unknown or differs from its actual write proposal')
    return receipt


def apply_checkpoint(prepared):
    """Checkpoint retains its ordinary evidence and write-receipt identity."""
    from . import checkpoint
    write = writes.proposal_value(prepared)
    if write['operation'] != 'checkpoint' or 'checkpoint_evidence' not in write:
        raise ContractError('A verified recovery hold does not authorize a raw single-file replacement')
    root, bank = writes.bank_path(write['repo_root'])
    _, receipt_path = writes.state_paths(root, bank, write['operation_id'])
    known = state.read(receipt_path, missing=True)
    if known is not None:
        if known.get('request_sha256') != digest(encoded(write)) or known.get('schema') != 'agent-memory/write-receipt-v1':
            raise ContractError('Checkpoint ID was reused for different evidence')
        return writes.result('apply', **{key: value for key, value in known.items() if key != 'schema'},
            already_applied=True, writes_performed=False, still_current=None, receipt_path=str(receipt_path))
    with storage.exclusive(bank / '.agent-memory.lock', write['operation_id'], 'bank-checkpoint') as owner:
        resolution.require_completed_resolutions(root)
        held = frontier.verified_hold(root, owner)
        if held is None or held.scope != write['scope_context'] or writes.snapshot(root, owner)[1] != write['observed_hashes']:
            raise ContractError('Held checkpoint scope or source changed after preview')
        checkpoint.validate_write(write)
        base = tx.files_at(bank)
        files = {**base, write['path']: write['content']}
        bank_contract.validate_files(files)
        pending = frontier.pending_rows(root, held.token, owner)
        frontier.reservation(files, pending, held.token)
        p = {'schema': tx.SCHEMA, 'operation_id': write['operation_id'], 'operation': 'replace',
            'repo_root': str(root), 'project_id': write['project_id'], 'observed_hashes': write['observed_hashes'],
            'files': files, 'scope_context': write['scope_context']}
        marker = marker_for(held, write['operation_id'])
        journal = {'schema': 'agent-memory/layout-nested-journal-v1', 'operation_id': held.operation_id,
            'nested_id': write['operation_id'], 'marker': marker, 'hold': frontier.marker_value(held.marker),
            'proposal': p, 'proposal_sha256': digest(encoded(p)), 'base_files': base, 'write_proposal': write,
            'frontier': held.token, 'resolution_plan': {'ledgers': []}}
        directory = state.paths(root, held.operation_id)[3] / 'nested' / write['operation_id']
        state.write_once(directory / 'journal.json', journal)
        security.guarded_start(root, write['operation_id'], held.scope, {write['path']: write['content']},
            ['assistant'], action='checkpoint')
        resolution.atomic_marker(bank, held.marker, encoded(marker), owner)
        tx.replace_file(bank, write['path'], write['content'], write['observed_hashes'], encoded(marker),
            set(files), before_replace=lambda: (frontier.check(held.token, root, owner, encoded(marker)),
                checkpoint.validate_write(write)))
        checkpoint.validate_write(write)
        if bank_contract.validate(root, owned_lock=owner, owned_transaction=encoded(marker))['project_id'] != write['project_id']:
            raise ContractError('Held checkpoint readback changed project identity')
        receipt = {'schema': 'agent-memory/write-receipt-v1', 'operation_id': write['operation_id'],
            'project_id': write['project_id'], 'bank_path': str(bank), 'request_sha256': digest(encoded(write)),
            'observed_hashes': write['observed_hashes'], 'resulting_hashes': tx.file_hashes(files),
            'changed_memory_paths': [bank.relative_to(root).as_posix() + '/' + write['path']],
            'outcome': 'replaced', 'readback_at': storage.now(), 'verified': True}
        writes.fault('layout-recovery-nested-before-receipt')
        writes.record(receipt_path, encoded(receipt))
        writes.fault('layout-recovery-nested-after-receipt')
        terminalize(root, held.operation_id, write['operation_id'], journal, receipt, owner, encoded(marker))
        return writes.result('apply', **{key: value for key, value in receipt.items() if key != 'schema'},
            already_applied=False, writes_performed=True, still_current=True, receipt_path=str(receipt_path))
