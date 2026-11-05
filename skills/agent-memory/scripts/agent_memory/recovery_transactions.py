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
    directory, head, current = frontier.ledger(p['repo_root'], held.operation_id, indexed['operations'][held.operation_id])
    units = copy.deepcopy(current['units'])
    plans = []
    for claim in claims:
        if claim['coverage'] == 'none':
            continue
        unit = next(unit for unit in units if unit['unit_id'] == claim['unit_id'])
        identity = str(uuid.UUID(hex=digest(encoded([p['operation_id'], unit['unit_id']]))[:32], version=4))
        terminal_state = ('admitted' if 'admission_evidence' in p else 'governed-terminal') if claim['coverage'] == 'full' else 'pending-admission'
        plans.append({'resolution_id': identity, 'unit_id': unit['unit_id'], 'claim': claim,
            'terminal_state': terminal_state, 'review_sha256': digest(encoded(review))})
    return {'previous_head': head, 'units': units, 'resolutions': plans}


def terminalize(root, operation_id, nested_id, journal, receipt, owner, marker):
    directory = state.paths(root, operation_id)[3]
    plan = journal['resolution_plan']
    units = copy.deepcopy(plan['units'])
    for item in plan['resolutions']:
        record = {'schema': 'agent-memory/layout-resolution-receipt-v1',
            'resolution_id': item['resolution_id'], 'operation_id': operation_id, 'unit_id': item['unit_id'],
            'generation': plan['previous_head']['generation'], 'nested_id': nested_id,
            'request_sha256': digest(encoded(journal['proposal'])), 'canonical_operation_id': nested_id,
            'canonical_receipt_sha256': digest(encoded(receipt)), 'terminal_state': item['terminal_state'],
            'claims': [item['claim']], 'independent_review_sha256': item['review_sha256'], 'verified': True}
        state.write_once(directory / 'resolutions' / (item['resolution_id'] + '.json'), record)
        unit = next(unit for unit in units if unit['unit_id'] == item['unit_id'])
        unit['state'] = item['terminal_state']
        unit['terminal_receipts'].append({'resolution_id': item['resolution_id'], 'sha256': digest(encoded(record))})
    writes.fault('layout-recovery-admission-receipt-before-resolution')
    if plan['resolutions']:
        resolution.advance_ledger(root, operation_id, units, plan['previous_head'], owner, marker)
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
        held = frontier.verified_hold(root, owner)
        if held is None or held.scope != p['scope_context'] or writes.snapshot(root, owner)[1] != p['observed_hashes']:
            raise ContractError('Held transaction scope or complete canonical base changed after preview')
        claims, review = validate(p, owner)
        token = held.token
        remaining = credited_rows(root, token, claims, owner)
        frontier.reservation(p['files'], remaining, {**token, 'unit_ids': [row['unit_id'] for row in remaining]})
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
            'outcome': 'finished', 'changed_memory_paths': [bank.relative_to(root).as_posix() + '/' + path for path in paths],
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


def nested_preview(repo, operation_id, nested_id, mode, owner=None):
    root, bank, _, directory = state.paths(repo, operation_id)
    if mode not in {'finish', 'rollback'} or not uuid_valid(nested_id):
        raise ContractError('Known nested recovery needs the exact operation, UUID and finish/rollback mode')
    journal = state.read(directory / 'nested' / nested_id / 'journal.json')
    if (journal.get('schema') != 'agent-memory/layout-nested-journal-v1' or journal.get('operation_id') != operation_id
            or journal.get('nested_id') != nested_id or journal.get('proposal_sha256') != digest(encoded(journal['proposal']))):
        raise ContractError('Unknown, foreign or damaged nested recovery journal')
    marker = encoded(journal['marker'])
    bank_observation.markers(bank, owner, marker)
    p = tx.proposal_value(journal['proposal'])
    observed = writes.hashes(bank)
    legal_partial(p, journal['base_files'], observed)
    _, receipt_path = tx.locations(root, bank, nested_id)
    receipt = nested_receipt(journal, receipt_path)
    if receipt is None and mode != 'rollback':
        raise ContractError('Unfinished governed nested learning permits rollback only, then fresh evidence')
    if receipt is not None and (receipt['outcome'] != 'finished' or mode != 'finish' or observed != receipt['resulting_hashes']):
        raise ContractError('A durably receipted canonical transaction is finish-only and cannot replay over later edits')
    return {'schema': 'agent-memory/layout-nested-recovery-v1', 'recovery_id': str(uuid.uuid4()),
        'repo_root': str(root), 'operation_id': operation_id, 'nested_id': nested_id, 'mode': mode,
        'journal_sha256': digest(encoded(journal)), 'marker_sha256': digest(marker), 'observed_hashes': observed,
        'canonical_receipt_sha256': digest(encoded(receipt)) if receipt else None,
        'ledger_head': state.read(directory / 'ledger-head.json')}


def recover_nested(repo, operation_id=None, nested_id=None, mode=None, proposal=None):
    if proposal is None:
        return {'proposal': nested_preview(repo, operation_id, nested_id, mode), 'writes_performed': False}
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
            terminalize(root, p['operation_id'], p['nested_id'], journal, receipt, owner, marker)
        else:
            running = dict(p['observed_hashes'])
            expected = tx.file_hashes(journal['base_files'])
            for path in sorted(set(running) | set(expected), key=lambda path: (path == 'MEMORY.md', path)):
                if running.get(path) == expected.get(path):
                    continue
                tx.replace_file(bank, path, journal['base_files'].get(path), running, marker,
                    set(original['observed_hashes']) | set(original['files']))
                if path in expected:
                    running[path] = expected[path]
                else:
                    running.pop(path, None)
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
    return {**receipt, 'outcome': 'finished'}


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
        indexed = state.index(root)
        _, head, current = frontier.ledger(root, held.operation_id, indexed['operations'][held.operation_id])
        journal = {'schema': 'agent-memory/layout-nested-journal-v1', 'operation_id': held.operation_id,
            'nested_id': write['operation_id'], 'marker': marker, 'hold': frontier.marker_value(held.marker),
            'proposal': p, 'proposal_sha256': digest(encoded(p)), 'base_files': base, 'write_proposal': write,
            'frontier': held.token, 'resolution_plan': {'previous_head': head, 'units': current['units'], 'resolutions': []}}
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
