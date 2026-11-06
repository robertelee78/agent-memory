"""Distinct child restoration of an exact completed layout outcome."""
from __future__ import annotations
import copy
import json
import os
from pathlib import Path
import stat
import uuid
from . import layout_recovery as recovery, layout_recovery_state as state
from . import layout_recovery_observation as raw, local_registry as storage
from . import recovery_frontier as frontier, bank_transactions as tx, bank_writes as writes
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError

SCHEMA = 'agent-memory/layout-child-restore-v1'
FIELDS = {'schema', 'operation_id', 'proposal_id', 'parent_operation_id', 'parent_receipt_sha256',
          'parent_proposal_sha256', 'parent_ledger_head_sha256', 'manifest_sha256', 'repo_root', 'bank_path',
          'project_id', 'scope_context', 'base_view', 'desired_view', 'actions', 'directories_to_remove',
          'parent_marker', 'parent_index_row', 'frontier', 'git', 'prepared_at'}


def value(repo, supplied):
    p = supplied.get('proposal', supplied)
    recovery.fields(p, FIELDS, SCHEMA)
    if not all(uuid_valid(p[key]) for key in ('operation_id', 'proposal_id', 'parent_operation_id', 'project_id')):
        raise ContractError('Child restore requires canonical stable identities')
    root, bank, _, directory = state.paths(repo, p['operation_id'])
    if (p['operation_id'] == p['parent_operation_id'] or p['repo_root'] != str(root) or p['bank_path'] != str(bank)
            or state.read(directory / 'proposals' / (p['proposal_id'] + '.json')) != p):
        raise ContractError('Child restore proposal is unknown or changed')
    return p


def boundary(p, current, plan=None):
    initial = plan['initial_view'] if plan else p['base_view']
    final = plan['desired_view'] if plan else p['desired_view']
    actions = plan['actions'] if plan else p['actions']
    expected = {row['path']: row for row in initial}
    desired = {row['path']: row for row in final}
    actual = {row['path']: row for row in current}
    changed = set()
    for position in range(len(actions) + 1):
        valid = True
        for path in set(actual) | set(expected):
            found, row = actual.get(path), expected.get(path)
            if path in p['directories_to_remove']:
                if found is not None and (found['type'] != 'directory' or
                        {key: found[key] for key in ('uid', 'gid')} != {key: (row or desired[path])[key] for key in ('uid', 'gid')}):
                    valid = False; break
                continue
            if path not in changed:
                if found != row:
                    valid = False; break
            elif row is None:
                if found is not None:
                    valid = False; break
            elif found is None or any(found[key] != row[key] for key in ('type', 'uid', 'gid', 'mode', 'sha256')):
                valid = False; break
        if valid:
            return position
        if position < len(actions):
            path = actions[position]['path']; changed.add(path)
            if path in desired:
                expected[path] = desired[path]
            else:
                expected.pop(path, None)
    raise ContractError('Child restore contains an impossible partial ordering or external edit')


def preview(repo, operation_id):
    root, bank, _, parent_dir, manifest, original, _ = recovery.recovery_source(repo, operation_id)
    receipt = recovery.receipt_value(parent_dir, original)
    if receipt is None:
        return recovery.recover(repo, operation_id, 'rollback')
    if receipt['outcome'] != 'layout-restored':
        raise ContractError('Only a completed repair has a child restore')
    existing_marker = frontier.marker_bytes(bank)
    recovery.bank_observation.markers(bank, None, existing_marker)
    if existing_marker not in (None, recovery.hold(original)):
        raise ContractError('Finish known marker cleanup before child restore')
    current = recovery.view(bank)
    recovery.expected_result(original, manifest, current)
    indexed = state.index(root)['operations'].get(operation_id)
    if indexed is None or indexed['state'] not in {'active', 'terminal'}:
        raise ContractError('Child restore needs the checked current parent operation')
    _, head, ledger = frontier.ledger(root, operation_id, indexed)
    if head['generation'] != 0 or ledger != recovery.ledger_value(original):
        raise ContractError('Later admission/governance/revision depends on the repaired bank')
    child = str(uuid.uuid4())
    with storage.exclusive(bank / '.agent-memory.lock', child, 'bank-recover') as owner:
        from . import layout_resolution
        layout_resolution.require_completed_resolutions(root)
        frontier.require_scope(root, original['project_id'], expected=original['scope_context'],
                               owned_lock=owner, owned_transaction=existing_marker)
        source_frontier = frontier.observe(root, owner, existing_marker)
        observed = raw.observe(root, child, owner=owner, transaction=existing_marker)
        directory = state.paths(root, child)[3]
        state.write_once(directory / 'manifest.json', observed)
        state.put_bytes(directory / 'manifest.sha256', (digest(encoded(observed)) + '\n').encode())
        old = {source['path']: source for source in manifest['sources']}
        now = {source['path']: source for source in observed['sources']}
        actions = [{'path': path, 'before': now.get(path, {}).get('sha256'), 'after': old.get(path, {}).get('sha256')}
                   for path in sorted(set(old) | set(now), key=lambda path: (path == 'MEMORY.md', path))
                   if old.get(path, {}).get('sha256') != now.get(path, {}).get('sha256')]
        p = {'schema': SCHEMA, 'operation_id': child, 'proposal_id': str(uuid.uuid4()), 'parent_operation_id': operation_id,
             'parent_receipt_sha256': digest(encoded(receipt)), 'parent_proposal_sha256': digest(encoded(original)),
             'parent_ledger_head_sha256': digest(encoded(head)), 'manifest_sha256': digest(encoded(observed)),
             'repo_root': str(root), 'bank_path': str(bank), 'project_id': manifest['project_id'],
             'scope_context': original['scope_context'], 'base_view': current, 'desired_view': recovery.manifest_view(manifest),
             'actions': actions, 'directories_to_remove': original['metadata_plan']['directories'],
             'parent_marker': json.loads(existing_marker) if existing_marker else None, 'parent_index_row': indexed,
             'frontier': source_frontier, 'git': raw.git_pins(root), 'prepared_at': storage.now()}
        state.write_once(directory / 'proposals' / (p['proposal_id'] + '.json'), p)
    return recovery.result('rollback', child, state='child-restore-preview', verified=True, proposal=p,
                           parent_operation_id=operation_id, private_artifact_writes_performed=True,
                           temporary_lock_writes_performed=True)


def temporaries(bank, p):
    marker = encoded({'schema': frontier.TRANSACTION, 'project_id': p['project_id'],
                      'operation_id': p['operation_id'], 'transaction_kind': 'rollback'})
    markers = [marker] + ([encoded(p['parent_marker'])] if p['parent_marker'] else [])
    return recovery.temps.entries(bank, p['operation_id'], p['actions'], p['base_view'],
        {row['path']: row for row in p['desired_view'] if row['type'] == 'file'}, markers)


def recover(repo, operation_id=None, mode=None, proposal=None):
    if proposal is None:
        root, bank, _, directory, observed = recovery.operation(repo, operation_id)
        journal = state.read(directory / 'journal.json')
        p = value(root, state.read(directory / 'proposals' / (journal['proposal_id'] + '.json')))
        marker = frontier.marker_bytes(bank)
        recovery.bank_observation.markers(bank, None, marker)
        known = state.read(directory / 'receipt.json', missing=True)
        if mode not in {'finish', 'rollback'} or (known and mode != known['restore_mode']):
            raise ContractError('Durable child restore outcome cannot be reversed by interrupted recovery')
        allowed = {encoded(journal['marker'])}
        allowed.add(encoded(p['parent_marker']) if p['parent_marker'] else None)
        if known is not None:
            allowed.add(None)
        if marker not in allowed:
            raise ContractError('Child recovery refuses an unknown operation marker')
        temporary_entries = temporaries(bank, p)
        current = recovery.view(bank, [pin['path'] for pin in temporary_entries])
        if known is None:
            active = state.read(directory / 'child-rollback-plan.json', missing=True)
            boundary(p, current, active if mode == 'rollback' else None)
        elif current != known['resulting_view']:
            raise ContractError('Later external edits prevent child marker cleanup')
        plan = {'schema': 'agent-memory/layout-child-recovery-v1', 'recovery_id': str(uuid.uuid4()),
                'operation_id': operation_id, 'mode': mode, 'proposal_sha256': digest(encoded(p)),
                'observed_view': recovery.view(bank), 'temporary_entries': temporary_entries,
                'marker_sha256': digest(marker), 'journal_sha256': digest(encoded(journal)),
                'receipt_sha256': digest(encoded(known)) if known else None, 'git': raw.git_pins(root)}
        state.write_once(directory / 'recovery' / (plan['recovery_id'] + '.json'), plan)
        return recovery.result('recover', operation_id, state='child-recovery-preview', verified=True, proposal=plan,
                               private_artifact_writes_performed=True)
    plan = proposal.get('proposal', proposal)
    recovery.fields(plan, {'schema', 'recovery_id', 'operation_id', 'mode', 'proposal_sha256', 'observed_view',
                           'marker_sha256', 'journal_sha256', 'receipt_sha256', 'git', 'temporary_entries'}, 'agent-memory/layout-child-recovery-v1')
    root, _, _, directory = state.paths(repo, plan['operation_id'])
    if state.read(directory / 'recovery' / (plan['recovery_id'] + '.json')) != plan:
        raise ContractError('Child recovery preview is unknown or changed')
    completed = recovery.layout_history.completed(directory, plan)
    if completed is not None:
        p = value(root, state.read(directory / 'proposals' / (completed['proposal_id'] + '.json')))
        recovery.layout_history.check(root, p['scope_context'], p['project_id'])
        return recovery.result('recover', p['operation_id'], state=completed['outcome'], verified=True,
                               receipt=completed, receipt_is_historical=True, already_applied=True)
    recovery.operation(root, plan['operation_id'])
    journal = state.read(directory / 'journal.json')
    p = value(root, state.read(directory / 'proposals' / (journal['proposal_id'] + '.json')))
    if state.read(directory / 'recovery' / (plan['recovery_id'] + '.json')) != plan or digest(encoded(p)) != plan['proposal_sha256']:
        raise ContractError('Child recovery preview is not exact')
    return apply(root, p, plan=plan)


def apply(repo, supplied, plan=None):
    p = value(repo, supplied)
    root, bank, base, directory = state.paths(repo, p['operation_id'])
    known = state.read(directory / 'receipt.json', missing=True)
    if known is not None and plan is None:
        if known.get('request_sha256') != digest(encoded(p)) or known.get('verified') is not True:
            raise ContractError('Child historical receipt is corrupt')
        recovery.layout_history.check(root, p['scope_context'], p['project_id'])
        return recovery.result('rollback', p['operation_id'], state=known['outcome'], verified=True,
                               receipt=known, receipt_is_historical=True, already_applied=True)
    _, _, _, _, observed = recovery.operation(root, p['operation_id'])
    _, _, _, parent_dir, manifest, original, _ = recovery.recovery_source(root, p['parent_operation_id'])
    if (digest(encoded(recovery.receipt_value(parent_dir, original))) != p['parent_receipt_sha256']
            or digest(encoded(original)) != p['parent_proposal_sha256'] or digest(encoded(observed)) != p['manifest_sha256']):
        raise ContractError('Child restore parent/source evidence is damaged')
    mode = plan['mode'] if plan else 'finish'
    mutated = False
    with storage.exclusive(bank / '.agent-memory.lock', p['operation_id'], 'bank-recover') as owner:
        from . import layout_resolution
        layout_resolution.require_completed_resolutions(root)
        current_marker = frontier.marker_bytes(bank)
        starting_marker = encoded(p['parent_marker']) if p['parent_marker'] else None
        transaction = encoded({'schema': frontier.TRANSACTION, 'project_id': p['project_id'],
                               'operation_id': p['operation_id'], 'transaction_kind': 'rollback'})
        current = recovery.view(bank)
        if plan:
            journal = state.read(directory / 'journal.json')
            if (current != plan['observed_view'] or digest(current_marker) != plan['marker_sha256']
                    or digest(encoded(journal)) != plan['journal_sha256'] or raw.git_pins(root) != plan['git']
                    or (digest(encoded(known)) if known else None) != plan['receipt_sha256']
                    or temporaries(bank, p) != plan['temporary_entries']):
                raise ContractError('Child recovery pins changed')
        elif current != p['base_view'] or current_marker != starting_marker:
            raise ContractError('Child restore would overwrite a later edit')
        if raw.git_pins(root) != p['git']:
            raise ContractError('Child restore HEAD/index/configuration changed')
        index = state.index(root); parent = index['operations'][p['parent_operation_id']]
        _, head, _ = frontier.ledger(root, p['parent_operation_id'], parent)
        if digest(encoded(head)) != p['parent_ledger_head_sha256']:
            raise ContractError('Child restore has a later pending-ledger dependency')
        proxy = observed
        recovery.rebind(root, proxy, owner, current_marker)
        if plan:
            recovery.temps.clean(bank, plan['temporary_entries'], owner, current_marker)
            current = recovery.view(bank)
        if known is not None and current != known['resulting_view']:
            raise ContractError('External changes after durable child receipt remain pending')
        journal = {'schema': recovery.JOURNAL, 'operation_id': p['operation_id'], 'proposal_id': p['proposal_id'],
                   'proposal_sha256': digest(encoded(p)), 'manifest_sha256': p['manifest_sha256'],
                   'review_sha256': original['review_sha256'], 'marker': json.loads(transaction),
                   'parent_operation_id': p['parent_operation_id'], 'parent_receipt_sha256': p['parent_receipt_sha256']}
        state.write_once(directory / 'journal.json', journal)
        state.bind(root, p['operation_id'], p['project_id'], p['scope_context']['checkout_key'], 'interrupted')
        if current_marker != transaction:
            recovery.atomic_marker(bank, current_marker, transaction, owner)
        if known is None:
            context = recovery.RecoveryReadSet.observe(root, observed, p, owner, transaction)
            if mode == 'finish':
                position = boundary(p, current); actions = p['actions'][position:]
                desired_manifest, desired_dir = manifest, parent_dir
            else:
                reverse = state.read(directory / 'child-rollback-plan.json', missing=True)
                if reverse is None:
                    boundary(p, current)
                    expected = {source['path']: source for source in observed['sources']}
                    current_sources = {row['path']: row.get('sha256') for row in current if row['type'] == 'file' and not row['runtime']}
                    actions = [{'path': path} for path in sorted(set(expected) | set(current_sources), key=lambda path: (path == 'MEMORY.md', path))
                               if expected.get(path, {}).get('sha256') != current_sources.get(path)]
                    reverse = {'initial_view': current, 'desired_view': p['base_view'], 'actions': actions}
                    state.write_once(directory / 'child-rollback-plan.json', reverse)
                position = boundary(p, current, reverse); actions = reverse['actions'][position:]
                desired_manifest, desired_dir = observed, directory
                recovery.new_directories(bank, {'metadata_plan': {'directories': p['directories_to_remove']}})
            sources = {source['path']: source for source in desired_manifest['sources']}
            facts = {row['path']: row for row in desired_manifest['physical_entries']}
            running = recovery.view(bank)
            for action in actions:
                path = action['path']; source = sources.get(path); row = facts.get(path)
                body = state.read_bytes(desired_dir / source['blob'], raw.MAX_FILE) if source else None
                required = {'uid': row['uid'], 'gid': row['gid'], 'mode': stat.S_IMODE(row['mode'])} if row else None
                recovery.replace_source(root, bank, path, body, required, running, context)
                mutated = True
                running = recovery.view(bank)
            if mode == 'finish':
                for path in sorted(p['directories_to_remove'], key=lambda path: -len(Path(path).parts)):
                    location = bank / path
                    if location.exists():
                        if location.is_symlink() or not location.is_dir() or any(location.iterdir()):
                            raise ContractError('Child restore directory gained an unrelated occupant')
                        location.rmdir(); tx.flush_directory(location.parent)
            final = p['desired_view'] if mode == 'finish' else p['base_view']
            actual = {row['path']: row for row in recovery.view(bank)}
            if set(actual) != {row['path'] for row in final}:
                raise ContractError('Child restore physical readback failed')
            for row in final:
                keys = ('type', 'runtime', 'uid', 'gid', 'mode') + (('sha256',) if row['type'] == 'file' else ())
                if any(actual[row['path']][key] != row[key] for key in keys):
                    raise ContractError('Child restore bytes/metadata readback failed')
            context.check()
            known = {'schema': recovery.RECEIPT, 'operation_id': p['operation_id'], 'proposal_id': p['proposal_id'],
                     'project_id': p['project_id'], 'request_sha256': digest(encoded(p)),
                     'parent_operation_id': p['parent_operation_id'], 'parent_receipt_sha256': p['parent_receipt_sha256'],
                     'outcome': 'rolled-back' if mode == 'finish' else 'layout-restored', 'restore_mode': mode,
                     'verified': True, 'readback_at': storage.now(), 'resulting_view': recovery.view(bank),
                     'canonical_eligibility_at_commit': recovery.eligibility(root, owner, transaction),
                     'semantic_recovery_complete_at_commit': False,
                     'pending_unit_ids_at_commit': p['frontier']['unit_ids'],
                     'original_index_writes_performed': False, 'head_writes_performed': False, 'feature_writes_performed': False}
            state.write_once(directory / 'receipt.json', known)
            writes.fault('layout-recovery-after-receipt')
        index = state.index(root)
        index['operations'][p['parent_operation_id']] = ({**p['parent_index_row'], 'state': 'rolled-back'} if mode == 'finish' else p['parent_index_row'])
        state.set_index(root, index, digest(state.read_bytes(base / 'index.json')))
        state.bind(root, p['parent_operation_id'], p['project_id'], p['scope_context']['checkout_key'],
                   'active' if p['frontier']['unit_ids'] else 'terminal')
        state.bind(root, p['operation_id'], p['project_id'], p['scope_context']['checkout_key'], 'terminal')
        if starting_marker and (mode == 'rollback' or p['frontier']['unit_ids']):
            recovery.atomic_marker(bank, transaction, starting_marker, owner)
        else:
            tx.check_marker(bank, transaction); (bank / recovery.MARKER).unlink(); tx.flush_directory(bank)
            sentinel = frontier.marker_bytes(bank, recovery.SENTINEL)
            if sentinel is not None:
                expected = encoded({'schema': frontier.SENTINEL, 'operation_id': p['parent_operation_id'], 'project_id': p['project_id']})
                if sentinel != expected:
                    raise ContractError('Unknown sentinel during child restore')
                (bank / recovery.SENTINEL).unlink(); tx.flush_directory(bank)
        frontier.invalidate(root)
        if plan:
            state.write_once(directory / 'recovery' / (plan['recovery_id'] + '.receipt.json'),
                             recovery.layout_history.recovery_receipt(directory, plan, known))
    return recovery.result('rollback', p['operation_id'], state=known['outcome'], verified=True, receipt=known,
                           canonical_eligibility=known['canonical_eligibility_at_commit'],
                           parent_operation_id=p['parent_operation_id'], canonical_writes_performed=mutated,
                           private_artifact_writes_performed=True, temporary_lock_writes_performed=True)
