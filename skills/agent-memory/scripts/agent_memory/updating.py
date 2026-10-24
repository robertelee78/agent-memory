"""Stage and verify a recorded fast-forward runtime without migrating banks."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from . import artifact, installation as install, project_identity, repository_config, runtime_faults
from .bank_contract import parse_router
from .frontmatter import ContractError
from .pack_validation import dispatch, inventory, semantic_version


def version_key(value):
    install.require(semantic_version(value), 'Invalid declared semantic version')
    core, _, pre = value.split('+', 1)[0].partition('-')
    return (tuple(int(v) for v in core.split('.')), int(not pre),
            tuple((0, int(v)) if v.isdecimal() else (1, v) for v in pre.split('.') if pre))


def configuration(old):
    return argparse.Namespace(command='install', development_source=None, remote=None, ref=None,
                              mode=None, presets=None, target_dir=None, bin_dir=None, no_guidance=False)


def current(state, code):
    old = install.read_receipt(state)
    install.require(old is not None and old['source'].get('kind')=='managed',
                    'Updates require a recorded managed installation; development sources are never modified')
    source = old['source']; root = Path(source['path'])
    install.require(root == code and source.get('pack') == str(code/'skills/agent-memory'),
                    'Managed source/pack location differs from the recorded layout')
    install.require(install.git(root, 'remote', 'get-url', 'origin').stdout.strip() == source['remote'],
                    'Managed origin differs from its recorded remote')
    install.require(install.git(root, 'symbolic-ref', '--short', 'HEAD').stdout.strip() == source['ref'],
                    'Managed branch/ref differs from its recorded channel')
    actual = install.source_info(root, 'managed', source['remote'], source['ref'])
    install.require(actual == source, 'Current source revision, version or artifact differs from its receipt')
    journal_path = state/'journal.json'
    if os.path.lexists(journal_path):
        journal = install.load(journal_path)
        install.require(journal.get('status') in ('complete','rolled-back'),
                        'Incomplete lifecycle transaction; use the trusted recover helper before checking updates')
        if journal['status']=='complete':
            install.require(install.fingerprint(state/'receipt.json') == journal['receipt_identity'],
                            'Current receipt differs from its committed journal')
    for target in old['targets']:
        if target['kind']=='activation':
            path = Path(target['path'])
            install.require(path.is_file() and not path.is_symlink(), 'Activation target is missing or linked')
            install.activation(path.read_bytes(), target['block_hash'])
        else:
            install.verify_targets([target])
    return old


def banks(extra, compatibility):
    """Use the established one-bank locator; never invent another bank path."""
    roots = set(); gaps = []
    cwd = Path.cwd().resolve()
    top = install.git(cwd, 'rev-parse', '--show-toplevel', check=False)
    roots.add(Path(top.stdout.strip()).resolve() if top.returncode==0 else cwd)
    for value in extra:
        path = install.absolute(value)
        install.require(path.is_dir(), 'Explicit repository is unavailable: ' + str(path))
        roots.add(path.resolve())
    registry, _ = project_identity.observe()
    for entry in registry['checkouts'].values():
        for location in entry['locations']:
            path = Path(location)
            if not path.is_dir():
                gaps.append({'location':location, 'reason':'registered checkout is unavailable; not inspected'})
                continue
            listed = install.git(path, 'worktree', 'list', '--porcelain', check=False)
            if listed.returncode and path.name!='.git' and not (path/'.git').exists():
                roots.add(path.resolve())
                continue
            if listed.returncode:
                gaps.append({'location':location, 'reason':'registered Git location cannot list worktrees'})
                continue
            roots.update(Path(line[9:]).resolve() for line in listed.stdout.splitlines() if line.startswith('worktree '))
    install.require(len(roots)<=1024, 'Discovered repository inventory exceeds its bound')
    result = []; seen = set()
    for root in sorted(roots):
        if not root.is_dir():
            gaps.append({'location':str(root), 'reason':'worktree no longer available; not inspected'})
            continue
        bank = repository_config.bank_path(root)
        router = bank/'MEMORY.md'
        if not os.path.lexists(router):
            continue
        install.require(not bank.is_symlink() and not router.is_symlink() and router.is_file(),
                        'Discovered bank/router must be an ordinary readable path')
        path = bank.resolve()
        if path in seen:
            continue
        seen.add(path)
        with router.open('rb') as stream:
            raw = stream.read(16385)
        install.require(len(raw)<=16384, 'Discovered bank router exceeds its reader bound')
        metadata = parse_router(raw.decode('utf-8'))[0]
        schema = metadata['schema']
        readable = schema in compatibility['readable_bank_schemas']
        migratable = schema in compatibility['migrator_source_schemas']
        install.require(readable or migratable,
                        'Candidate cannot read or explicitly migrate discovered bank: ' + str(bank))
        result.append({'repository':str(root), 'bank':str(bank), 'schema':schema,
                       'directly_readable':readable, 'explicit_migration_required':not readable,
                       'router_sha256':install.sha(raw)})
    return {'banks':result, 'discovery_gaps':gaps, 'bank_writes_performed':False,
            'implicit_migration_performed':False}


@contextmanager
def staged(old):
    source = old['source']
    with tempfile.TemporaryDirectory(prefix='agent-memory-update-candidate-') as temporary:
        reference = 'refs/heads/' + source['ref']
        listed = install.git(Path(source['path']), 'ls-remote', '--exit-code', '--refs', '--',
                             source['remote'], reference).stdout.splitlines()
        matches = [line.split() for line in listed if line.split()[-1:]==[reference]]
        install.require(len(matches)==1 and len(matches[0])==2, 'Recorded channel does not resolve to one branch')
        if matches[0][0]==source['revision']:
            # Reuse the already validated artifact. Re-cloning the same commit
            # under a different umask must not invent a new runtime payload.
            root = Path(source['path'])
        else:
            root = Path(temporary)/'source'
            clone = subprocess.run(['git','clone','--branch',source['ref'],'--single-branch','--',source['remote'],str(root)],
                                   text=True, capture_output=True, timeout=120,
                                   env={**os.environ,'GIT_TERMINAL_PROMPT':'0'})
            install.require(clone.returncode==0, 'Could not stage recorded channel: ' + clone.stderr.strip())
        install.require(install.git(root,'remote','get-url','origin').stdout.strip()==source['remote']
                        and install.git(root,'symbolic-ref','--short','HEAD').stdout.strip()==source['ref'],
                        'Staged checkout differs from the recorded remote/ref')
        candidate = install.source_info(root,'managed',source['remote'],source['ref'])
        install.require(install.git(root,'merge-base','--is-ancestor',source['revision'],candidate['revision'],check=False).returncode==0,
                        'Candidate is not a fast-forward of the installed revision')
        install.require(version_key(candidate['version'])>=version_key(source['version']), 'Runtime downgrade refused')
        minimum = tuple(int(v) for v in candidate['compatibility']['python_minimum'].split('.'))
        install.require(sys.version_info[:2]>=minimum, 'Candidate requires a newer Python interpreter')
        # The current owned wrapper resolves this stable public entrypoint.
        files = inventory(Path(candidate['pack']))
        _, commands, _ = dispatch('scripts/installer.py', files)
        install.require('update' in commands and 'install' in commands and 'recover' in commands,
                        'Candidate would remove the owned lifecycle command surface')
        yield root, candidate


def wrapper_readback(old):
    wrapper = Path(old['bin_dir'])/'agent-memory'
    for args in (('--help',), ('update','--help')):
        result = subprocess.run([str(wrapper),*args], text=True, capture_output=True, timeout=30)
        install.require(result.returncode==0, 'Updated command failed its public dispatch readback: '+result.stderr.strip())


def run(args):
    install.require(sys.version_info>=(3,11), 'Python 3.11 or later is required')
    state, code = install.locations()
    before = install.read_receipt(state)
    install.require(before is not None, 'No installation receipt; install the managed channel first')
    install.preflight_layout(args,state,code,before)
    paths = [state, code, *(Path(t['path']) for t in before['targets'])]
    def work():
        old = current(state,code)
        with staged(old) as (root, candidate):
            compatibility = banks(args.repo,candidate['compatibility'])
            summary = {'operation':'update','current':old['source'],'available':{k:v for k,v in candidate.items() if k not in ('path','pack')},
                       'channel':{'remote':old['source']['remote'],'ref':old['source']['ref']},
                       'update_available':candidate['revision']!=old['source']['revision'],
                       'loaded_content_changes':candidate['artifact_digest']!=old['source']['artifact_digest'],
                       'compatibility':compatibility,'atomicity':'per target; journaled rollback across targets'}
            same_revision = candidate['revision']==old['source']['revision']
            with install.prepare(configuration(old),state,code,old,
                                 candidate_root=None if same_revision else root) as plan:
                plan['operation']='update'
                if args.check:
                    return {**summary,'check_complete':True,'eligible':True,'verified':False,
                            'installation_writes_performed':False,'restart_clients':plan['restart']}
                if same_revision:
                    wrapper_readback(old)
                    # The shared planner refreshes a stale outside-text snapshot
                    # without reactivating identical code or restarting clients.
                def compatible_now():
                    banks(args.repo,candidate['compatibility'])
                def readback():
                    compatible_now(); wrapper_readback(old)
                result = install.install(plan,before_activate=compatible_now,after_activate=readback)
                return {**summary,**result}
    if args.check:
        return work()
    with runtime_faults.operation('update', paths), install.locked(state):
        # Retry a killed activation before selecting another candidate. Never
        # treat a journal's historical completion as current ownership proof.
        install.recover(state,for_install=True)
        return work()


def execute(args):
    try:
        result = run(args)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, SyntaxError, RecursionError, subprocess.SubprocessError) as exc:
        result = {'operation':'update','verified':False,'error':str(exc),
                  'repair':'Preserve the receipt, source and bank; correct the reported channel/ownership/compatibility issue. '
                           'For an interrupted activation, use the trusted installer helper recover before retrying.'}
    print(json.dumps(result,indent=2))
    return 0 if result.get('verified') or result.get('check_complete') else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation',choices=('update',))
    parser.add_argument('--check',action='store_true')
    parser.add_argument('--repo',action='append',default=[])
    args = parser.parse_args(argv)
    args.command='update'
    return execute(args)
