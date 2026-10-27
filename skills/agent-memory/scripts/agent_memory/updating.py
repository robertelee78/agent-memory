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
    install.require(old is not None and old['source'].get('kind') in ('managed', 'release'),
                    'Updates require a recorded managed installation; development sources are never modified')
    source = old['source']; root = Path(source['path'])
    install.trust_paths.source(root)
    install.require(root == code and source.get('pack') == str(code/'skills/agent-memory'),
                    'Managed source/pack location differs from the recorded layout')
    if source['kind'] == 'managed':
        install.require(install.git(root, 'remote', 'get-url', 'origin').stdout.strip() == source['remote'],
                        'Managed origin differs from its recorded remote')
        install.require(install.git(root, 'symbolic-ref', '--short', 'HEAD').stdout.strip() == source['ref'],
                        'Managed branch/ref differs from its recorded channel')
    actual = install.source_info(root, source['kind'], source['remote'], source['ref'])
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
            install.verify_targets([target], source=source)
    return old


def banks(extra, compatibility, *, require_readable=False):
    """Use the established one-bank locator; never invent another bank path."""
    from concurrent.futures import ThreadPoolExecutor
    from . import migration, bank_transactions
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
            for line in listed.stdout.splitlines():
                if line.startswith('worktree '):
                    worktree = Path(line[9:]).resolve()
                    roots.add(worktree)
    install.require(len(roots)<=1024, 'Discovered repository inventory exceeds its bound')
    def inspect(root):
        if not root.is_dir():
            return None, {'location':str(root), 'reason':'worktree no longer available; not inspected'}
        settings = repository_config.settings(root, router_reader=migration.router_metadata)
        bank = root / settings['bank_root']
        router = bank/'MEMORY.md'
        if not os.path.lexists(router):
            return None, None
        install.require(not bank.is_symlink() and not router.is_symlink() and router.is_file(),
                        'Discovered bank/router must be an ordinary readable path')
        def idle():
            install.require(not any(os.path.lexists(bank/name) for name in
                            ('.agent-memory.lock', bank_transactions.MARKER)),
                            'Compatibility requires an idle bank; preserve its pending operation: ' + str(bank))
        idle()
        raw, _ = migration.read_regular(bank, 'MEMORY.md', 16384)
        metadata = migration.router_metadata(raw)
        install.require(metadata is not None, 'Discovered bank needs an explicit format header')
        schema = metadata['schema']
        if schema == 'agent-memory/v1':
            parse_router(raw.decode('utf-8'))
        readable = schema in compatibility['readable_bank_schemas']
        migratable = schema in compatibility['migrator_source_schemas']
        install.require(readable or (migratable and not require_readable),
                        ('Rollback runtime cannot directly read current bank; explicitly restore/migrate memory first: '
                         if require_readable else 'Candidate cannot read or explicitly migrate discovered bank: ') + str(bank))
        idle()
        install.require(migration.read_regular(bank, 'MEMORY.md', 16384)[0] == raw
                        and repository_config.settings(root, router_reader=migration.router_metadata) == settings,
                        'Discovered bank changed during compatibility observation')
        return {'repository':str(root), 'bank':str(bank), 'schema':schema,
                       'directly_readable':readable, 'explicit_migration_required':not readable,
                       'router_sha256':install.sha(raw)}, None
    result = []; seen = set(); observed_roots = []
    ordered = sorted(roots)
    # Every repository is independently inspected; the bounded pool only
    # overlaps Git and filesystem reads. map preserves deterministic order.
    with ThreadPoolExecutor(max_workers=4) as pool:
        for root, (record, gap) in zip(ordered, pool.map(inspect, ordered)):
            if gap:
                gaps.append(gap)
                continue
            observed_roots.append(str(root))
            if record and Path(record['bank']).resolve() not in seen:
                seen.add(Path(record['bank']).resolve())
                result.append(record)
    return {'banks':result, 'discovery_gaps':gaps, 'observed_repositories':observed_roots,
            'bank_writes_performed':False,
            'implicit_migration_performed':False}


def release_transition(source, candidate, root):
    """Old ownership is checked separately; a signed version never changes identity."""
    install.require(candidate['kind'] == 'release', 'Expected verified release acquisition')
    order = (version_key(candidate['version']) > version_key(source['version'])) - (
        version_key(candidate['version']) < version_key(source['version']))
    install.require(order >= 0, 'Runtime downgrade refused')
    if order == 0 and source['kind'] == 'release':
        install.require(all(source.get(k) == candidate.get(k) for k in
                        ('artifact_digest', 'revision', 'release_manifest_sha256')),
                        'Published release version changed identity')
    elif order == 0:
        old_files = artifact.inspect(Path(source['pack']))['files']
        new_files = artifact.inspect(Path(candidate['pack']))['files']
        modes = {}
        for entry in install.git(Path(source['path']), 'ls-tree', '-rz', 'HEAD', '--', 'skills/agent-memory/').stdout.split('\0'):
            if not entry: continue
            metadata, path = entry.split('\t', 1)
            mode, kind, _ = metadata.split()
            install.require(kind == 'blob' and path.startswith('skills/agent-memory/'), 'Legacy portable Git entry differs')
            modes[path[len('skills/agent-memory/'):]] = mode
        install.require(set(old_files) == set(new_files) == set(modes)
                        and all(old_files[p]['sha256'] == new_files[p]['sha256']
                                and modes[p] == ('100755' if new_files[p]['mode'] == '0o755' else '100644')
                                for p in new_files),
                        'Same-version legacy source differs from signed portable paths, bytes or executable intent')
    minimum = tuple(int(v) for v in candidate['compatibility']['python_minimum'].split('.'))
    install.require(sys.version_info[:2] >= minimum, 'Candidate requires a newer Python interpreter')


def compatible_surface(source, candidate):
    _, commands, _ = dispatch('scripts/installer.py', inventory(Path(candidate['pack'])))
    _, existing, _ = dispatch('scripts/installer.py', inventory(Path(source['pack'])))
    install.require({'update','install','recover'} | set(existing) <= set(commands),
                    'Candidate would remove the owned lifecycle command surface')


@contextmanager
def staged(old):
    source = old['source']
    if source['kind'] == 'release' or (source['remote'] == install.DEFAULT_REMOTE and source['ref'] == 'main'):
        from . import release_distribution as release
        with tempfile.TemporaryDirectory(prefix='agent-memory-release-candidate-') as temporary:
            root = install.BOOTSTRAP_SOURCE
            if root is None:
                root = Path(temporary)/'source'
                release.acquire(root)
            candidate = install.source_info(root, 'release')
            release_transition(source, candidate, root)
            compatible_surface(source, candidate)
            yield root, candidate
        return
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
        root = Path(candidate['path'])
        install.require(install.git(root,'merge-base','--is-ancestor',source['revision'],candidate['revision'],check=False).returncode==0,
                        'Candidate is not a fast-forward of the installed revision')
        install.require(version_key(candidate['version'])>=version_key(source['version']), 'Runtime downgrade refused')
        minimum = tuple(int(v) for v in candidate['compatibility']['python_minimum'].split('.'))
        install.require(sys.version_info[:2]>=minimum, 'Candidate requires a newer Python interpreter')
        # The current owned wrapper resolves this stable public entrypoint.
        compatible_surface(source, candidate)
        yield root, candidate


def wrapper_readback(old, targets=None):
    for target in old['targets'] if targets is None else targets:
        if target['kind'] != 'wrapper':
            continue
        for args in (('--help',), ('update','--help')):
            result = subprocess.run([target['path'],*args], text=True, capture_output=True, timeout=30)
            install.require(result.returncode==0,
                            'Updated command failed its public dispatch readback: '+result.stderr.strip())


def run(args):
    install.require(sys.version_info>=(3,11), 'Python 3.11 or later is required')
    state, code = install.locations()
    before = install.read_receipt(state)
    install.require(before is not None, 'No installation receipt; install the managed channel first')
    install.preflight_layout(args,state,code,before)
    source = before['source']
    if install.BOOTSTRAP_SOURCE is None and (source['kind'] == 'release'
            or (source['kind'] == 'managed' and source['remote'] == install.DEFAULT_REMOTE and source['ref'] == 'main')):
        from . import release_distribution as release
        with tempfile.TemporaryDirectory(prefix='am-preverified-update-') as temporary:
            root = Path(temporary)/'source'; release.acquire(root)
            install.BOOTSTRAP_SOURCE = root
            try:
                return run(args)
            finally:
                install.BOOTSTRAP_SOURCE = None
    paths = [state, code, *(Path(t['path']) for t in before['targets']),
             *(Path(before['bin_dir'])/name for name in install.COMMAND_NAMES)]
    if not before.get('no_guidance', False):
        for name, _, _ in install.presets(before['presets']):
            hook = install.hook_paths(name)
            if hook:
                paths.append(install.absolute(hook))
    def work():
        old = current(state,code)
        with staged(old) as (root, candidate):
            summary = {'operation':'update','current':old['source'],'available':{k:v for k,v in candidate.items() if k not in ('path','pack')},
                       'channel':{'remote':old['source']['remote'],'ref':old['source']['ref']},
                       'update_available':any(candidate.get(k) != old['source'].get(k) for k in
                                              ('kind', 'revision', 'artifact_digest', 'release_manifest_sha256')),
                       'loaded_content_changes':candidate['artifact_digest']!=old['source']['artifact_digest'],
                       'atomicity':'per target; journaled rollback across targets'}
            same_revision = not summary['update_available']
            with install.prepare(configuration(old),state,code,old,
                                 candidate_root=None if same_revision else root, candidate_kind=candidate['kind']) as plan:
                plan['operation']='update'
                # With no candidate or target changes there are no bank writes to
                # qualify. Still verify ownership and public wrapper dispatch.
                if (not args.check and not args.repo and same_revision and old['source'] == plan['source']
                        and not plan['restart'] and len(plan['actions']) == len(old['targets'])
                        and not any(a['changed'] for a in plan['actions'])
                        and all(t['identity'] == next(a['after'] for a in plan['actions'] if a['path'] == t['path'])
                                for t in old['targets'])):
                    install.verify_targets(old['targets'], source=old['source'])
                    wrapper_readback(old)
                    return {**summary, 'compatibility': {'bank_scan': 'not-needed-no-changes',
                            'bank_writes_performed': False, 'implicit_migration_performed': False},
                            'verified': True, 'changed': False, 'receipt': old,
                            'restart_clients': [], 'receipt_path': str(state/'receipt.json')}
                compatibility = banks(args.repo,candidate['compatibility'])
                summary['compatibility'] = compatibility
                plan['rollback_repositories'] = compatibility['observed_repositories']
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
                    compatible_now(); wrapper_readback(old, plan['actions'])
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
        runtime_faults.response('update', result)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, SyntaxError, RecursionError, subprocess.SubprocessError) as exc:
        result = {'operation':'update','verified':False,'error':str(exc),
                  'repair':install.failure_repair(exc)}
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
