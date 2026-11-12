#!/usr/bin/env python3
"""Source-bound #206 four-condition refresh and 24-world/48-operation memo proof.

No model, graph service, private reader substitution, or synthetic memo calls.
The full public inspect/review-v2/prepare/apply protocol derives from the retained
206-refresh-50000-v5.py driver; import/conflict fixtures use existing contracts.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import time
import traceback

sys.dont_write_bytecode = True
# Reject caches before the first local import, including the binding helper.
for _directory in ('tests', 'scripts', 'skills/agent-memory'):
    for _cache in (Path(__file__).resolve().parents[2] / _directory).rglob('*'):
        if (_cache.name == '__pycache__' and _cache.is_symlink()) or _cache.suffix in ('.pyc', '.pyo'):
            raise ValueError('Qualification helper bytecode is not source-bound: ' + str(_cache))
from recall_comparison_builds import encoded, save, sha, source, stamp
from labeled_recall_probe import inventory

CONDITIONS = ('primed-unchanged', 'no-catalog', 'import-invalidated', 'held-conflict')
QUESTION = 'How often may a temporary network error be attempted again?'
HISTORICAL_DRIVER = {'path': '/tmp/agent-memory-v15-readiness/206-refresh-50000-v5.py',
                     'sha256': '39398f0dd1353ac5b0a02bef4549ae2a264bb577d126b6c864a0086d8586ccf0'}
# sys.setprofile observes original frames without replacing validators/functions or
# changing catalog engine fingerprints. Exact typed key bytes, not a fake count.
PROFILE = r'''import hashlib,json,runpy,sys
from pathlib import Path
sys.dont_write_bytecode=True
output=Path(sys.argv[1]);helper=sys.argv[2];sys.argv=sys.argv[2:]
def typed(value):
 if isinstance(value,bytes):return {'bytes':value.hex()}
 if isinstance(value,tuple):return {'tuple':[typed(v) for v in value]}
 if isinstance(value,dict):return {'dict':[[typed(k),typed(v)] for k,v in value.items()]}
 if isinstance(value,(str,int,bool)) or value is None:return value
 raise TypeError('Unbound memo key type: '+type(value).__name__)
active={};ordinal=0;inputs={}
blobs=output.parent/'memo-inputs';blobs.mkdir(mode=0o700,exist_ok=True)
with output.open('x') as log:
 def profile(frame,event,arg):
  global ordinal
  if frame.f_code.co_name!='checked' or not frame.f_code.co_filename.endswith('/agent_memory/validation_memo.py'):return
  if event=='call':
   values=frame.f_locals;namespace=values['namespace']
   if namespace not in ('bank-format','security-scan'):return
   ordinal+=1;key=values['key'];memo=values['self']
   identity=(namespace,key)
   if identity not in inputs:
    produce=values['produce']
    closure=dict(zip(produce.__code__.co_freevars,produce.__closure__ or ()))
    names=('stable',) if namespace=='bank-format' else ('content','maximum')
    captured={name:typed(closure[name].cell_contents) for name in names}
    raw=json.dumps(captured,sort_keys=True,separators=(',',':')).encode()
    digest=hashlib.sha256(raw).hexdigest();target=blobs/(digest+'.json')
    if not target.exists():target.write_bytes(raw)
    inputs[identity]=digest
   row={'ordinal':ordinal,'namespace':namespace,'key':typed(key),'hit_at_entry':(namespace,key) in memo.values,
        'memo_instance':id(memo),'entries_at_entry':len(memo.values),'exact_input_sha256':inputs[identity]}
   active[id(frame)]=row
   log.write(json.dumps({'event':'call',**row},sort_keys=True)+'\n');log.flush()
  elif event=='return' and id(frame) in active:
   row=active.pop(id(frame));values=frame.f_locals
   log.write(json.dumps({'event':'return',**row,'returned_value':arg is not None,
     'stored_after_return':(values['namespace'],values['key']) in values['self'].values},sort_keys=True)+'\n');log.flush()
 sys.setprofile(profile)
 try:runpy.run_path(helper,run_name='__main__')
 finally:sys.setprofile(None)
'''


def public_projection(result):
    """Compare public guidance/provenance/safety, not sanctioned metric changes."""
    recall = result.get('recall') or {}
    return {key: result.get(key) for key in ('refresh_applied', 'remote_freshness_verified',
        'relevant_imports_read', 'conflicts_pending', 'blocked_paths', 'sharing_state', 'failure_code')} | {
        'recall': {key: recall.get(key) for key in ('results', 'semantic_coverage', 'pending_recovery_units',
            'pending_recovery_unit_ids', 'recovery_required_actions', 'code_working_bytes_verified')}}


def hits(path):
    counts = Counter()
    if path.exists():
        for line in path.read_text().splitlines():
            row = json.loads(line)
            if row['event'] == 'return' and row['hit_at_entry'] and row['returned_value']:
                counts[(row['namespace'], json.dumps(row['key'], sort_keys=True))] += 1
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('refresh', 'memo'))
    parser.add_argument('--builds', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--count', type=int, choices=(1000, 10000, 50000), default=50000)
    parser.add_argument('--samples', type=int, default=5)
    parser.add_argument('--timeout', type=int, default=7200, help='Per public child, including 50k review/apply')
    args = parser.parse_args()
    if args.samples < 5 or args.timeout < 1:
        parser.error('Qualification requires five or more samples and a positive child deadline')
    manifest = json.loads(args.builds.read_text())
    roots = {k: Path(v['root']).resolve(strict=True) for k, v in manifest['builds'].items()}
    if set(roots) != {'A', 'B', 'C'}:
        parser.error('Use the complete source-equivalent A/B/C build manifest')
    for label, root in roots.items():
        if source(root) != manifest['builds'][label]['source']:
            parser.error('Source changed after controlled build freeze: ' + label)
    root = roots['C']
    out = args.out.resolve()
    if any(out.is_relative_to(p) or p.is_relative_to(out) for p in roots.values()):
        parser.error('Private evidence must remain outside all source builds')
    out.mkdir(mode=0o700, exist_ok=False)
    sys.path[:0] = [str(root / 'tests'), str(root / 'scripts/evaluations')]
    from world import World
    from contracts.test_sharing_recovery import SharingRecoveryContract
    from contracts.test_semantic_sharing import SemanticSharingContract
    from semantic_navigation_fixture import corpus
    from labeled_recall_fixture import PID
    from record_fixtures import project_id
    from recall_refresh_smoke import completed_request

    observations, commands, failures = [], [], []
    summary = {'schema': 'agent-memory/recall-refresh-qualification-v1', 'mode': args.mode,
        'started_at': stamp(), 'argv': sys.argv, 'platform': platform.platform(), 'python': sys.version,
        'build_manifest_sha256': sha(args.builds.read_bytes()), 'controller_sha256': sha(Path(__file__).read_bytes()),
        'historical_driver': HISTORICAL_DRIVER, 'conditions': CONDITIONS, 'count': args.count,
        'samples_per_condition': args.samples, 'model_calls': 0, 'os_cold_claimed': False,
        'observations': observations, 'failures': failures, 'qualification_passed': False}
    save(out / 'builds.json', manifest)
    profile_script = out / 'memo-observer.py'
    profile_script.write_text(PROFILE)
    save(out / 'protocol.json', summary)
    current_directory = out / 'setup'
    current_directory.mkdir(mode=0o700)

    class RecordedWorld(World):
        def run(self, argv, **kwargs):
            argv = list(map(str, argv))
            # Existing identity fixture helpers omit -B; prevent source pycache
            # pollution while retaining their actual public setup operations.
            if argv[0] == sys.executable and '-B' not in argv:
                argv.insert(1, '-B')
            number = len(commands) + 1
            prefix = current_directory / f'{number:05d}'
            inputs = {}
            for value in argv[1:]:
                p = Path(value)
                if p.is_absolute() and p.is_file() and p.suffix == '.json':
                    raw = p.read_bytes(); identity = sha(raw)
                    (current_directory / (identity + '.input.json')).write_bytes(raw)
                    inputs[value] = identity
            start, started = time.perf_counter(), stamp()
            try:
                result = super().run(argv, **kwargs)
                stdout, stderr, code, error = result.stdout, result.stderr, result.returncode, None
            except (subprocess.TimeoutExpired, subprocess.CalledProcessError, OSError) as exc:
                def text(value):
                    return value.decode(errors='replace') if isinstance(value, bytes) else value or ''
                stdout, stderr = text(getattr(exc, 'output', None)), text(getattr(exc, 'stderr', None))
                code, error = getattr(exc, 'returncode', None), repr(exc)
                result = None
            prefix.with_suffix('.stdout').write_text(stdout)
            prefix.with_suffix('.stderr').write_text(stderr)
            row = {'number': number, 'argv': argv, 'cwd': str(kwargs.get('cwd') or self.paths['repo']),
                   'input_sha256': inputs, 'started_at': started, 'finished_at': stamp(),
                   'seconds': time.perf_counter() - start, 'exit': code, 'error': error,
                   'stdout': str(prefix.with_suffix('.stdout').relative_to(out)),
                   'stderr': str(prefix.with_suffix('.stderr').relative_to(out)),
                   'load_average': list(__import__('os').getloadavg())}
            commands.append(row); save(out / 'commands.json', commands)
            if result is None:
                raise RuntimeError('Recorded public child failed: ' + str(row))
            return result

    def persist():
        save(out / 'observations.json', summary)

    def operation(world, pack, label, helper, *argv, instrument=False):
        log = current_directory / (label + '.memo.jsonl')
        command = [sys.executable, '-I', '-B']
        if instrument:
            command += [profile_script, log]
        command += [pack / 'scripts' / helper, *argv]
        result = world.run(command, check=False, timeout=args.timeout, group=True)
        value = json.loads(result.stdout) if result.stdout.strip() else {}
        if result.returncode == 0 and '--output' in argv:
            target = Path(argv[argv.index('--output') + 1])
            if target.is_file():
                raw = target.read_bytes()
                artifact = current_directory / (f'{len(commands):05d}-' + label + '.json')
                artifact.write_bytes(raw)
                commands[-1]['artifact'] = str(artifact.relative_to(out))
                save(out / 'commands.json', commands)
                value = json.loads(raw)
        if result.returncode:
            raise RuntimeError(label + ' failed with retained typed result: ' + json.dumps(value))
        return value, commands[-1]

    def save_input(world, value, name):
        target = world.inside('qualification-inputs/' + name + '.json')
        target.parent.mkdir(mode=0o700, exist_ok=True)
        save(target, value)
        return target

    def private_output(world, name):
        target = world.inside('qualification-inputs/' + name + '-' + str(len(commands)) + '.json')
        target.parent.mkdir(mode=0o700, exist_ok=True)
        if target.exists():
            raise RuntimeError('Preserve existing private output')
        return target

    def protected(world, repo, bank):
        index = Path(world.git('rev-parse', '--path-format=absolute', '--git-path', 'index', cwd=repo).stdout.strip())
        return {'bank': inventory(bank), 'code_sha256': sha((repo / 'code.py').read_bytes()),
                'index_sha256': sha(index.read_bytes()), 'HEAD': world.git('rev-parse', 'HEAD', cwd=repo).stdout,
                'branch': world.git('symbolic-ref', 'HEAD', cwd=repo).stdout,
                'unrelated_sha256': sha((repo / 'unrelated.txt').read_bytes())}

    def discover(world, pack, repo, bank, selected, parents):
        trace, _ = operation(world, pack, 'discover-start', 'discover_memory.py', 'start', '--repo', repo,
                             '--intent', 'recall', '--task', QUESTION)
        trace = trace['discovery']
        for index, parent in enumerate(parents):
            node, _ = operation(world, pack, 'discover-node-' + str(index), 'discover_memory.py', 'node',
                '--repo', repo, '--input', save_input(world, trace, 'navigation'), '--path', parent)
            trace = node['discovery']
        return completed_request(trace, bank, [selected], QUESTION)

    def review(world, pack, repo, conflicts):
        path = private_output(world, 'inspection-output')
        initial, _ = operation(world, pack, 'inspect', 'refresh_memory.py', 'inspect', '--v2', '--repo', repo, '--output', path)
        view = initial['inspection']
        if view['pending_selections']:
            request = view['request']
            # Controlled fixture labels cover all changed components, both wall
            # claims and full route prose. This is mechanical, never model quality.
            for key in view['pending_selections']:
                request['selections'][key] = {'searches': ['room wall color', 'project source orientation'],
                    'ids': [], 'paths': sorted(set(['projectbrief.md', *conflicts])), 'complete': True, 'uncertain': False}
            selected_path = private_output(world, 'selected-output')
            view, _ = operation(world, pack, 'inspect-selected', 'reconcile_memory.py', 'inspect-v2', '--repo', repo,
                '--input', save_input(world, request, 'selection'), '--output', selected_path)
        if view['pending_selections']:
            raise RuntimeError('Actual selected frontier remains incomplete')
        # Retain every real page, not just its hash acknowledgment.
        manifest_path = save_input(world, view, 'manifest')
        for page in view['pages']:
            operation(world, pack, 'page-' + str(page['index']), 'reconcile_memory.py', 'page-v2',
                      '--repo', repo, '--input', manifest_path, '--page', page['index'])
        rows = {row['key']: row for row in view['sources']}
        judgment = {'schema': 'agent-memory/semantic-sharing-judgment-v2', 'request': view['request'],
            'binding': view['binding'], 'manifest_sha256': view['manifest_sha256'],
            'pages': [page['sha256'] for page in view['pages']],
            'pairs': [{'left': a, 'right': b,
                'classification': 'contradiction' if conflicts and {rows[a]['path'], rows[b]['path']} == set(conflicts) else 'related',
                'reason': 'Controlled complete source comparison; retain both contradictory wall claims and all independent guidance.'}
                for a, b in view['required_pairs']], 'dependencies': view['request']['dependencies'], 'consolidations': [],
            'rationale': 'Mechanical fixed fixture review; not semantic quality acceptance.', 'recorded_at': '2026-11-05T00:00:00+00:00'}
        result, _ = operation(world, pack, 'review', 'reconcile_memory.py', 'review-v2', '--repo', repo,
            '--input', save_input(world, judgment, 'judgment'), '--output', private_output(world, 'review-output'))
        return result

    def scenario(world, pack, repo, bank, peer, selected, parents, condition, conflicts, build, control, rounds):
        if condition == 'no-catalog':
            world.env['AGENT_MEMORY_NO_RECALL_CATALOG'] = '1'
        else:
            world.env.pop('AGENT_MEMORY_NO_RECALL_CATALOG', None)
        def pointers():
            return {p.relative_to(world.paths['state']).as_posix(): sha(p.read_bytes())
                    for p in world.paths['state'].rglob('recall-catalog-v1/current.json')}
        if pointers():
            raise RuntimeError('Initial derived state was not empty; preserve it instead of relabeling cold')
        initial_request = discover(world, pack, repo, bank, selected, parents)
        # Empty derived state is observed separately from sequential samples.
        empty, cold_command = operation(world, pack, 'empty-derived-state', 'retrieve.py', '--repo', repo,
                             '--request', save_input(world, initial_request, 'initial-recall'))
        save(current_directory / 'cold-state.json', {'optional_indexes_used': empty['optional_indexes_used'],
            'metrics': empty['metrics'], 'whole_seconds': cold_command['seconds'], 'filesystem_cold_claimed': False})
        primed, _ = operation(world, pack, 'prime', 'retrieve.py', '--repo', repo,
                             '--request', save_input(world, initial_request, 'prime-recall'))
        save(current_directory / 'priming.json', primed)
        primed_pointers = pointers()
        save(current_directory / 'primed-pointers.json', primed_pointers)
        for round_number in range(rounds):
            tag = f'{round_number:02d}'
            if condition == 'import-invalidated':
                target = peer / 'memory-bank/projectbrief.md'
                target.write_text(target.read_text() + '\nIndependent import observation ' + str(round_number) + '.\n')
                world.git('add', '--', 'memory-bank', cwd=peer)
                world.git('commit', '-m', 'Deterministic independent import ' + tag, cwd=peer)
                world.git('push', 'origin', 'HEAD:main', cwd=peer)
            request = discover(world, pack, repo, bank, selected, parents)
            if condition in ('import-invalidated', 'held-conflict'):
                # Import cannot use a pre-import inventory-bound trace. Use the
                # unchanged daily-use non-discovery request shape, not fake trace.
                request.pop('discovery')
                request['query'] = ''
                request['ids'] = [next(row['metadata']['id'] for row in rows if row['path'] == selected)]
                request['paths'] = []
            before = protected(world, repo, bank)
            reviewed = review(world, pack, repo, conflicts)
            prepared_path = private_output(world, 'prepared-' + tag)
            prepared, prepare_command = operation(world, pack, 'prepare-' + tag, 'refresh_memory.py', 'prepare', '--v2',
                '--repo', repo, '--input', save_input(world, {'proof': reviewed['proof'], 'recall': request}, 'request-' + tag),
                '--output', prepared_path, instrument=control == 'on')
            applied_path = private_output(world, 'applied-' + tag)
            result, apply_command = operation(world, pack, 'apply-' + tag, 'refresh_memory.py', 'apply', '--v2',
                '--input', prepared_path, '--output', applied_path, instrument=control == 'on')
            after = protected(world, repo, bank)
            expected = {name: {'sha256': sha(body.encode()), 'mode': before['bank'].get(name, {'mode': 0o644})['mode']}
                        for name, body in reviewed['refresh_files'].items()}
            preserve = {key: before[key] == after[key] for key in before if key != 'bank'}
            preserve['bank_equals_complete_reviewed_result'] = after['bank'] == expected
            recall = result['recall']
            selections = [row['path'] for row in recall['results']]
            if selections != [selected] or any(path in selections for path in conflicts):
                raise RuntimeError('Changed guidance or held-source leak: ' + json.dumps(selections))
            if not all(preserve.values()) or not result['refresh_applied'] or not result['remote_freshness_verified']:
                raise RuntimeError('Public refresh or protected-state preservation failed')
            if condition == 'held-conflict' and not result['conflicts_pending']:
                raise RuntimeError('Real contradictory claims were not retained pending')
            if condition == 'primed-unchanged' and build == 'C' and not recall['optional_indexes_used']:
                raise RuntimeError('Full candidate did not use its primed catalog during actual nested recall')
            if condition == 'no-catalog' and recall['optional_indexes_used']:
                raise RuntimeError('Catalog-disabled control unexpectedly used an optional index')
            if condition == 'import-invalidated' and before['bank'] == after['bank']:
                raise RuntimeError('Actual import fixture did not change the bank')
            if recall.get('derived_cache_writes_performed'):
                raise RuntimeError('Nested refresh falsely published derived catalog')
            row = {'build': build, 'instrumentation': control, 'world': world_number, 'condition': condition,
                'round': round_number, 'sample': round_number, 'cold_observation': args.mode == 'refresh' and round_number == 0,
                'whole_apply_seconds': apply_command['seconds'], 'prepare_seconds': prepare_command['seconds'],
                'nested_recall_seconds': recall['metrics']['elapsed_seconds'], 'recall_metrics': recall['metrics'],
                'optional_indexes_used': recall['optional_indexes_used'], 'projection': public_projection(result),
                'preservation': preserve, 'before': before, 'after': after,
                'prepared_sha256': sha(prepared_path.read_bytes()), 'proof_sha256': sha(encoded(reviewed['proof'])),
                'prepared': prepare_command.get('artifact'),
                'apply_command': apply_command['number'], 'prepare_command': prepare_command['number'],
                'memo': {phase: str((current_directory / (phase + '-' + tag + '.memo.jsonl')).relative_to(out))
                         for phase in ('prepare', 'apply')}, 'success': True}
            observations.append(row); persist()
            if condition == 'import-invalidated':
                # A new, actual post-import operation; never synthesize its trace.
                post = discover(world, pack, repo, bank, selected, parents)
                operation(world, pack, 'post-import-read-' + tag, 'retrieve.py', '--repo', repo,
                          '--request', save_input(world, post, 'post-import-' + tag))
            elif condition == 'primed-unchanged':
                later, _ = operation(world, pack, 'later-read-' + tag, 'retrieve.py', '--repo', repo,
                                     '--request', save_input(world, request, 'later-' + tag))
                if build == 'C' and (not later['optional_indexes_used'] or later['metrics'].get('catalog_updated')):
                    raise RuntimeError('Later ordinary recall lost catalog or republished it')
                if build == 'C' and pointers() != primed_pointers:
                    raise RuntimeError('Unchanged-inventory refresh changed its authenticated catalog pointer')

    worlds = 24 if args.mode == 'memo' else len(CONDITIONS)
    try:
        for world_number in range(worlds):
            condition = CONDITIONS[world_number % len(CONDITIONS)]
            directory = out / f'world-{world_number:02d}-{condition}'
            directory.mkdir(mode=0o700)
            current_directory = directory / 'setup'; current_directory.mkdir(mode=0o700)
            with RecordedWorld() as world:
                # Fixed seed affects Git identity/time and fixture content; random
                # private roots/authority IDs are frozen once, then restored for
                # every paired build/control, preserving exact memo input keys.
                world.env.update(GIT_AUTHOR_DATE='2026-11-05T00:00:00Z', GIT_COMMITTER_DATE='2026-11-05T00:00:00Z')
                helper = SharingRecoveryContract()
                repo, bank, peer, base = helper.fixture(world)
                pack = helper.pack
                router = (bank / 'MEMORY.md').read_text()
                pid = project_id(router)
                authority = next(line for line in router.splitlines() if line.startswith('canonical-remote: '))
                template = json.loads((pack / 'templates/record-proposal.example.json').read_text())['metadata']
                files, rows, stats = corpus(template, args.count)
                shutil.rmtree(bank); bank.mkdir()
                for name, body in files.items():
                    target = bank / name; target.parent.mkdir(parents=True, exist_ok=True)
                    body = body.replace(PID, pid)
                    if name == 'MEMORY.md':
                        body = body.replace('router-updated:', authority + '\nrouter-updated:', 1)
                    target.write_text(body)
                world.git('add', '-A', '--', 'memory-bank', cwd=repo)
                world.git('commit', '--only', '-m', 'Frozen source-bound navigation corpus', '--', 'memory-bank', cwd=repo)
                world.git('push', 'origin', 'HEAD:main', cwd=repo)
                world.git('pull', '--ff-only', cwd=peer)
                world.git('checkout', '-b', 'feature-session', cwd=repo)
                world.git('commit', '-m', 'Unfinished feature ancestor', cwd=repo)
                (repo / 'code.py').write_text('VALUE = 3  # staged independent work\n')
                world.git('add', 'code.py', cwd=repo)
                (repo / 'unrelated.txt').write_text('Unstaged independent work\n')
                selected = next(row['path'] for row in rows if row['label'] == 'retry-limit')
                leaf = next(n for n, b in files.items() if n.startswith('topics/n/') and '| [' + selected + '](<' in b)
                group = next(n for n, b in files.items() if n.startswith('topics/g/') and '| [' + leaf + '](<' in b)
                conflicts = []
                if condition == 'held-conflict':
                    semantic = SemanticSharingContract()
                    _, local = semantic.add(bank, 'yellow', '# Wall\n\nThe room wall should be yellow.\n')
                    _, landed = semantic.add(peer / 'memory-bank', 'blue', '# Wall\n\nThe same room wall should be blue.\n')
                    conflicts = [local, landed]
                    world.git('add', '--', 'memory-bank', cwd=peer)
                    world.git('commit', '-m', 'Actual contradictory source', cwd=peer)
                    world.git('push', 'origin', 'HEAD:main', cwd=peer)
                fixture = {'seed': world_number, 'condition': condition, 'stats': stats, 'bank': inventory(bank),
                    'peer_bank': inventory(peer / 'memory-bank'), 'selected_path': selected, 'conflict_paths': conflicts,
                    'source_fixtures': {name: sha((root / name).read_bytes()) for name in (
                        'scripts/evaluations/semantic_navigation_fixture.py', 'scripts/evaluations/labeled_recall_fixture.py',
                        'tests/contracts/test_sharing_recovery.py', 'tests/contracts/test_semantic_sharing.py',
                        'scripts/evaluations/daily_use.py', 'scripts/evaluations/recall_refresh_smoke.py')}}
                save(directory / 'fixture.json', fixture)
                snapshot = directory / 'frozen-world'
                shutil.copytree(world.root, snapshot, ignore=shutil.ignore_patterns('__pycache__'))
                save(directory / 'frozen-world-inventory.json', inventory(snapshot))
                labels = ('A', 'C') if args.mode == 'memo' else ('A', 'B', 'C')
                for build in labels:
                    for control in (('on', 'off') if args.mode == 'memo' else ('off',)):
                        current_directory = directory / (build + '-instrument-' + control)
                        current_directory.mkdir(mode=0o700)
                        try:
                            for child in world.root.iterdir():
                                if child.is_dir() and not child.is_symlink():
                                    shutil.rmtree(child)
                                else:
                                    child.unlink()
                            shutil.copytree(snapshot, world.root, dirs_exist_ok=True)
                            shutil.rmtree(pack)
                            shutil.copytree(roots[build] / 'skills/agent-memory', pack)
                            if inventory(pack) != manifest['builds'][build]['source']['portable']:
                                raise RuntimeError('Executing portable pack is not the frozen build')
                            save(current_directory / 'executed-pack.json', inventory(pack))
                            scenario(world, pack, repo, bank, peer, selected, (group, leaf), condition, conflicts,
                                     build, control, 2 if args.mode == 'memo' else args.samples + 1)
                            if inventory(pack) != manifest['builds'][build]['source']['portable']:
                                raise RuntimeError('Portable executing source changed')
                        except Exception as exc:
                            failures.append({'world': world_number, 'condition': condition, 'build': build,
                                             'instrumentation': control, 'error': repr(exc)})
                            (current_directory / 'failure.txt').write_text(traceback.format_exc())
                            save(current_directory / 'failure-protected-state.json', protected(world, repo, bank))
                            persist()
        if args.mode == 'memo':
            lost, changed, missing, empty = [], [], [], []
            keyed = {(r['world'], r['round'], r['build'], r['instrumentation']): r for r in observations}
            for world_number in range(24):
                for round_number in range(2):
                    expected_keys = [(world_number, round_number, b, c) for b in ('A', 'C') for c in ('on', 'off')]
                    if any(k not in keyed for k in expected_keys):
                        missing.append([world_number, round_number]); continue
                    rows = {k[2:]: keyed[k] for k in expected_keys}
                    for build in ('A', 'C'):
                        if rows[(build, 'on')]['projection'] != rows[(build, 'off')]['projection']:
                            changed.append({'world': world_number, 'round': round_number, 'build': build, 'control': 'instrumentation'})
                    if rows[('A', 'on')]['projection'] != rows[('C', 'on')]['projection']:
                        changed.append({'world': world_number, 'round': round_number, 'control': 'A/C'})
                    for phase in ('prepare', 'apply'):
                        baseline = hits(out / rows[('A', 'on')]['memo'][phase])
                        candidate = hits(out / rows[('C', 'on')]['memo'][phase])
                        missing_namespaces = {'bank-format', 'security-scan'} - {identity[0] for identity in baseline}
                        if missing_namespaces:
                            empty.append({'world': world_number, 'round': round_number, 'phase': phase,
                                          'namespaces_without_successful_hits': sorted(missing_namespaces)})
                        for identity, count in (baseline - candidate).items():
                            lost.append({'world': world_number, 'round': round_number, 'phase': phase,
                                         'namespace': identity[0], 'exact_key': json.loads(identity[1]), 'lost_hits': count})
            summary['memo_rule'] = {'decision': 'keep' if not (lost or changed or missing or empty or failures) else 'revert-or-pending',
                'required_observations_per_build': 48, 'actual_instrumented_applies': dict(Counter(
                    r['build'] for r in observations if r['instrumentation'] == 'on')),
                'actual_uninstrumented_controls': dict(Counter(r['build'] for r in observations if r['instrumentation'] == 'off')),
                'lost_baseline_exact_key_hits': lost, 'public_outcome_changes': changed,
                'unexecuted_pairs': missing, 'no_observed_baseline_hits': empty}
            summary['qualification_passed'] = summary['memo_rule']['decision'] == 'keep'
        else:
            keyed = {(r['world'], r['round'], r['build']): r for r in observations}
            public_changes = []
            for world_number in range(len(CONDITIONS)):
                for round_number in range(args.samples + 1):
                    paired = [keyed.get((world_number, round_number, b)) for b in ('A', 'B', 'C')]
                    if any(r is None for r in paired) or any(r['projection'] != paired[0]['projection'] for r in paired[1:]):
                        public_changes.append({'world': world_number, 'round': round_number})
            summary['paired_public_outcome_changes_or_missing'] = public_changes
            cells = {}
            for condition in CONDITIONS:
                cells[condition] = {}
                for build in ('A', 'B', 'C'):
                    samples = [r for r in observations if r['condition'] == condition and r['build'] == build and not r['cold_observation']]
                    cells[condition][build] = {metric: {'samples': [r[metric] for r in samples],
                        'median': statistics.median(r[metric] for r in samples) if samples else None,
                        'range': [min(r[metric] for r in samples), max(r[metric] for r in samples)] if samples else None}
                        for metric in ('nested_recall_seconds', 'whole_apply_seconds')}
            summary['cells'] = cells
            gates = []
            for condition in CONDITIONS:
                for metric in (('nested_recall_seconds',) if condition == 'primed-unchanged' else ('nested_recall_seconds', 'whole_apply_seconds')):
                    a, b, c = [cells[condition][label][metric]['median'] for label in ('A', 'B', 'C')]
                    complete = all(len(cells[condition][label][metric]['samples']) == args.samples for label in ('A', 'B', 'C'))
                    gates.append({'condition': condition, 'metric': metric, 'complete': complete,
                        'C_over_A': c / a if a and c else None, 'B_over_A_restoration': b / a if a and b else None,
                        'C_over_B_incremental': c / b if b and c else None,
                        'maximum_C_over_A': 0.8 if condition == 'primed-unchanged' else 1.1,
                        'passed': bool(complete and a and c and c / a <= (0.8 if condition == 'primed-unchanged' else 1.1))})
            summary['performance_gates'] = gates
            summary['qualification_passed'] = args.count == 50000 and not failures and not public_changes and all(g['passed'] for g in gates)
        for label, path in roots.items():
            after = source(path)
            save(out / ('source-after-' + label + '.json'), after)
            if after != manifest['builds'][label]['source']:
                raise RuntimeError('Controlled source changed after qualification')
        summary['finished_at'] = stamp(); persist()
    except BaseException:
        (out / 'failure.txt').write_text(traceback.format_exc()); persist(); raise
    print(json.dumps({'observations': len(observations), 'qualification_passed': summary['qualification_passed'],
                      'evidence': str(out / 'observations.json')}))
    return 0 if summary['qualification_passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
