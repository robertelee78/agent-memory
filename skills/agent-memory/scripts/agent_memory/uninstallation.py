"""Remove proven runtime ownership while preserving project memory and edits."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

from . import installation as install, runtime_faults
from .frontmatter import ContractError


def reinstall(old):
    source = old['source']
    command = ['python3', '-I', '${SKILL_DIR}/scripts/installer.py', 'install']
    if source['kind'] == 'managed':
        command += ['--remote', source['remote'], '--ref', source['ref']]
    else:
        command += ['--development-source', source['path']]
    command += ['--mode', old['mode'], '--presets', old['presets'], '--bin-dir', old['bin_dir']]
    for target in old['custom_targets']:
        command += ['--target-dir', target]
    if old.get('no_guidance'):
        command += ['--no-guidance']
    return {'requires': 'Resolve SKILL_DIR to a trusted complete portable package',
            'argv': command, 'project_memory': 'preserved; do not initialize a replacement bank'}


def without_block(path, target, old):
    raw = path.read_bytes(); text = raw.decode('utf-8')
    install.activation(raw, target['block_hash'])  # Prove exactly one owned block.
    begin = text.index(install.BEGIN); end = text.index(install.END) + len(install.END)
    prefix, suffix = text[:begin].encode(), text[end:].encode()
    outside = prefix + suffix
    if target['originally_absent'] and not prefix and suffix in (b'', b'\n'):
        return None
    # Restore exact original bytes only when current surrounding content proves
    # that no later outside edit would be lost. Backups are never deleted.
    variants = {outside}
    if suffix.startswith(b'\n'):
        variants.add(prefix + suffix[1:])
    if prefix.endswith(b'\n'):
        variants.add(prefix[:-1] + suffix)
        if suffix.startswith(b'\n'):
            variants.add(prefix[:-1] + suffix[1:])
    for backup in old.get('backups', []):
        if backup.get('original_path') != str(path) or backup.get('kind') != 'pre-activation-snapshot':
            continue
        saved = Path(backup['path'])
        if (backup['identity'].get('kind') == 'file'
                and install.fingerprint(saved) == backup['identity']):
            original = saved.read_bytes()
            if install.BEGIN.encode() not in original and original in variants:
                return original
        break  # A later update's snapshot is not the original user file.
    return outside


def prepare(old, state, code):
    actions = []; preserved = []
    source = old['source']
    def preserve(target, reason):
        preserved.append({'path': target['path'], 'kind': target['kind'], 'reason': reason})
    def inspect(target):
        path = Path(target['path']); install.preflight_path(path)
        before = install.fingerprint(path); raw = None
        after = None
        if before is not None and target['kind'] == 'activation':
            if before['kind'] != 'file':
                preserve(target, 'Activation path is no longer an ordinary file'); return
            try:
                raw = without_block(path, target, old)
            except (ContractError, UnicodeError):
                preserve(target, 'Owned activation block was changed; surrounding content preserved'); return
            if raw is not None:
                after = {'kind': 'file', 'hash': install.sha(raw), 'mode': before['mode']}
        elif before is not None and before != target['identity']:
            preserve(target, 'Current identity differs from installation ownership'); return
        actions.append({'path': str(path), 'kind': target['kind'], 'before': before, 'after': after,
                        'raw': raw, 'mode_bits': before['mode'] if before and before['kind']=='file' else 0o644,
                        'changed': before != after})
    for target in old['targets']:
        if target['kind'] != 'source':
            inspect(target)
    for target in old['targets']:
        if target['kind'] == 'source':
            if preserved:
                preserve(target, 'Retained conservatively because a preserved client target may still reference it')
            else:
                inspect(target)
    return {'operation': 'uninstall', 'old': old, 'source': source, 'state': state,
            'mode': old['mode'], 'presets': old['presets'], 'custom_targets': old['custom_targets'],
            'no_guidance': old['no_guidance'], 'bin_dir': old['bin_dir'], 'adapters': [],
            'actions': actions, 'restart': [], 'preserved': preserved, 'reinstall': reinstall(old)}


def run(args):
    state, code = install.locations()
    old = install.read_receipt(state)
    install.preflight_layout(args, state, code, old)
    def work():
        current = install.read_receipt(state)
        if current is None:
            historical = install.load(state/'receipt.json')
            install.require(historical.get('uninstalled') is True, 'No active ownership receipt; do not guess removal targets')
            return {'operation':'uninstall', 'verified':not args.check, 'check_complete':bool(args.check),
                    'changed':False, 'uninstalled':True, 'receipt':historical,
                    'preserved':historical['preserved'], 'reinstall':historical['reinstall']}
        plan = prepare(current, state, code)
        if args.check:
            journal = state/'journal.json'
            install.require(not os.path.lexists(journal) or install.load(journal)['status'] in ('complete','rolled-back'),
                            'Recover the interrupted lifecycle transaction before planning uninstall')
            return {'operation':'uninstall', 'verified':False, 'check_complete':True,
                    'installation_writes_performed':False,
                    'targets':[{k:v for k,v in a.items() if k not in ('raw','mode_bits')} for a in plan['actions']],
                    'preserved':plan['preserved'], 'reinstall':plan['reinstall'],
                    'project_memory_writes_performed':False, 'purge_performed':False}
        result = install.install(plan)
        return {**result, 'uninstalled':True, 'preserved':plan['preserved'], 'reinstall':plan['reinstall'],
                'project_memory_writes_performed':False, 'purge_performed':False}
    if args.check:
        return work()
    paths = [state, code, *(Path(t['path']) for t in (old['targets'] if old else []))]
    with runtime_faults.operation('uninstall', paths), install.locked(state):
        install.recover(state, for_install=True)
        return work()


def execute(args):
    try:
        install.require(sys.version_info >= (3,11), 'Python 3.11 or later is required')
        result = run(args)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, subprocess.SubprocessError) as exc:
        result = {'operation':'uninstall', 'verified':False, 'error':str(exc),
                  'repair':'Preserve the external receipt, backups and reported paths; use a trusted complete package to recover or reconcile ownership.'}
    print(json.dumps(result, indent=2))
    return 0 if result.get('verified') or result.get('check_complete') else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    uninstall = commands.add_parser('uninstall')
    uninstall.add_argument('--check', '--dry-run', action='store_true')
    purge = commands.add_parser('purge-data')
    purge.add_argument('--repo', required=True)
    purge.add_argument('--path', required=True)
    consent = purge.add_mutually_exclusive_group(required=True)
    consent.add_argument('--check', action='store_true')
    consent.add_argument('--confirm')
    args = parser.parse_args(argv)
    if args.command == 'purge-data':
        from .purging import execute as purge_execute
        return purge_execute(args)
    return execute(args)
