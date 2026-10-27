"""Reconcile only independently verified, receipt-owned runtime faults."""
from pathlib import Path
import os
import subprocess

from . import artifact, installation as install, updating
from .frontmatter import ContractError


def trusted_receipt(state, code, receipt=None):
    """Verify the running package is the exact recorded source before repair."""
    old = receipt if receipt is not None else install.read_receipt(state)
    install.require(old is not None, 'No active installation receipt')
    source = old['source']
    if source['kind'] in {'managed', 'release'}:
        install.require(Path(source['path']) == code, 'Recorded managed source differs from its owned location')
    if source['kind'] == 'managed':
        root = Path(source['path'])
        install.require(install.git(root, 'remote', 'get-url', 'origin').stdout.strip() == source['remote']
                        and install.git(root, 'symbolic-ref', '--short', 'HEAD').stdout.strip() == source['ref'],
                        'Recorded managed source channel differs')
    install.require(Path(source['pack']).resolve(strict=True) == artifact.PACK.resolve(strict=True),
                    'Executing package differs from the installed receipt')
    observed = install.source_info(Path(source['path']), source['kind'], source.get('remote'), source.get('ref'))
    install.require(observed == source, 'Installed source differs from the receipt')
    return old


def reconcile_hooks(state, code, old):
    """Use the existing transactional installer, never replace a changed target."""
    missing = []
    for target in old['targets']:
        path = Path(target['path'])
        if not os.path.lexists(path) and target['kind'] in {'hook-config', 'hook-link', 'link', 'copy', 'wrapper', 'activation'}:
            missing.append(target)
        elif target['kind'] == 'activation':
            install.require(path.is_file() and not path.is_symlink(), 'Activation target is missing or linked')
            install.activation(path.read_bytes(), target['block_hash'])
        else:
            install.verify_targets([target], source=old['source'])
    args = updating.configuration(old)
    install.preflight_layout(args, state, code, old)
    source = old['source']
    if source['kind'] in {'managed', 'release'}:
        plan = install.prepare(args, state, code, old, candidate_root=source['path'],
                               candidate_kind=source['kind'], repair_missing_hooks=bool(missing))
    else:
        args.development_source = source['path']
        plan = install.prepare(args, state, code, old, repair_missing_hooks=bool(missing))
    with plan as value:
        # A concurrent edit must remain a refusal in the install transaction.
        return install.install(value)


def recover_completed_bank(repo):
    """Finish only a previously receipted outcome using the public recovery protocol."""
    from . import bank_transactions as tx, migration
    root, bank = tx.writes.bank_path(repo)
    marker = bank / tx.MARKER
    lock = bank / '.agent-memory.lock'
    if not os.path.lexists(marker) or os.path.lexists(lock):
        return None
    raw, _ = migration.read_regular(bank, tx.MARKER, migration.MAX_FILE)
    m = tx.load(raw)
    if not isinstance(m, dict) or not tx.uuid_valid(m.get('transaction_id')):
        return None
    journal, receipt_path = tx.locations(root, bank, m['transaction_id'])
    _, proposal = tx.journal_value(tx.limits.read_bytes(journal), root, bank, m['transaction_id'])
    receipt = tx.receipt_value(receipt_path, proposal)
    if receipt is None:
        return None
    mode = 'finish' if receipt['outcome'] == 'finished' else 'rollback'
    preview = tx.recover(repo, mode=mode)
    return tx.recover(repo, prepared=preview['proposal'])


def run(repo='.', projection=None):
    from . import diagnostics
    before = diagnostics.run('doctor', repo, projection)
    actions = []
    state, code = install.locations()
    acquired = False
    pending = any(c['component'] == 'runtime.transaction' for c in before['warnings'])
    gaps = any(c['component'].startswith('runtime.hook_migration.') for c in before['warnings'])
    hook_failures = [c for c in before['checks']
                     if c['component'] in {'runtime.target.hook-link', 'runtime.target.hook-config', 'runtime.target.link', 'runtime.target.copy', 'runtime.target.wrapper', 'runtime.target.activation'}
                     and c['state'] == 'failure']
    missing = any(c.get('path') and not os.path.lexists(c['path']) for c in hook_failures)
    if pending or gaps or hook_failures:
        try:
            observed = install.read_receipt(state)
            paths = [state, code, *(Path(t['path']) for t in observed['targets'])] if observed else [state, code]
            with install.runtime_faults.operation('doctor-fix', paths), install.locked(state):
                acquired = True
                if pending:
                    journal = install.read_journal(state)
                    try:
                        trusted_receipt(state, code)
                    except (OSError, ValueError, TypeError, KeyError, UnicodeError, RecursionError, subprocess.SubprocessError, ContractError):
                        # During activation the source can already be the journal's
                        # candidate while the receipt still records the old source.
                        install.require(journal is not None, 'Missing recovery journal')
                        trusted_receipt(state, code, journal['receipt'])
                    was_pending = bool(journal and journal['status'] not in ('complete', 'rolled-back'))
                    recovered = install.recover(state)
                    install.require(recovered.get('verified'), 'Lifecycle recovery was not verified')
                    actions.append({'component': 'runtime.transaction', 'state': 'repaired' if was_pending else 'unchanged',
                                    'detail': 'Verified the recorded runtime transaction.'})
                old = trusted_receipt(state, code)
                if gaps or hook_failures:
                    outcome = reconcile_hooks(state, code, old)
                    install.require(outcome.get('verified'), 'Hook reconciliation was not verified')
                    actions.append({'component': 'runtime.hooks',
                                    'state': 'repaired' if outcome.get('changed') else 'unchanged',
                                    'detail': 'Reconciled receipt-owned client integration.',
                                    'restart_clients': outcome.get('restart_clients', [])})
        except (OSError, ValueError, TypeError, KeyError, UnicodeError, RecursionError, subprocess.SubprocessError, ContractError):
            actions.append({'component': 'runtime.repair', 'state': 'refused',
                            'detail': 'Cannot safely verify and reconcile the owned runtime; existing state was preserved.',
                            'repair': 'Inspect am doctor and the installation receipt; preserve modified targets and use the trusted recover helper when directed.'})
    bank_attempted = False
    if any(c['component'] == 'bank.transaction' for c in before['warnings']):
        try:
            # No decision is made without an already verified outcome receipt.
            bank_attempted = True
            recovered = recover_completed_bank(repo)
            if recovered is not None:
                install.require(recovered.get('verified'), 'Bank recovery was not verified')
                actions.append({'component': 'bank.transaction', 'state': 'repaired',
                                'detail': 'Completed cleanup for the previously verified bank transaction outcome.'})
        except diagnostics.ERRORS:
            actions.append({'component': 'bank.transaction', 'state': 'refused',
                            'detail': 'The recorded bank outcome could not be safely recovered.',
                            'repair': 'Preserve the bank and inspect am doctor and the original transaction evidence.'})
    try:
        after = diagnostics.run('doctor', repo, projection)
    except diagnostics.ERRORS:
        report = diagnostics.Report('doctor-fix')
        report.add('diagnostic.post_repair', 'failure',
                   'Post-repair inspection could not be completed.',
                   'Preserve the current state and rerun am doctor; inspect the receipt and journal if it still fails.')
        after = report.finish()
    # Report every remaining actionable finding explicitly. A bank transaction may
    # be live, and neither its recovery direction nor arbitrary projections are
    # owned by the runtime installer, so never guess or overwrite them here.
    for check in after['checks']:
        if check['state'] not in {'warning', 'failure'}:
            continue
        actions.append({'component': check['component'], 'state': 'skipped',
                        'detail': 'This finding remains; automatic repair cannot safely determine the required action.',
                        'repair': check.get('repair', 'Inspect this finding with am doctor and resolve its reported cause.')})
    after['operation'] = 'doctor-fix'
    after['repairs'] = actions
    after['repairs_performed'] = any(v['state'] == 'repaired' for v in actions)
    # Acquiring the existing lifecycle lock may update private owner metadata
    # even if a later ownership check refuses the repair.
    after['writes_performed'] = True if acquired or any(v['component'] == 'bank.transaction' and v['state'] == 'repaired' for v in actions) else None if (pending or gaps or hook_failures or bank_attempted) else False
    after['restart_clients'] = sorted({name for item in actions for name in item.get('restart_clients', [])})
    if missing and any(v['component'] == 'runtime.hooks' and v['state'] == 'repaired' for v in actions):
        warning = {'component': 'runtime.hook_recreated', 'state': 'warning',
                   'detail': 'Missing owned integration targets were recreated from the ownership receipt; other content previously removed from missing files was not restored.',
                   'repair': 'Review the affected client configuration and restore any separately maintained settings if needed.'}
        after['checks'].append(warning)
        after['warnings'].append(warning)
    if any(v['state'] == 'refused' for v in actions):
        after['ok'] = False
    return after
