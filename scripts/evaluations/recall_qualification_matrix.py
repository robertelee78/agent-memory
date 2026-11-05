#!/usr/bin/env python3
"""Serial #206 qualification orchestration; one host slot, immutable A/B/C inputs."""
import argparse
import json
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from labeled_recall_probe import source
from recall_comparison_builds import save, sha, stamp

SAFETY_SUITES = (
    'test_validation_memo.py', 'test_recall_catalog.py', 'test_discovery.py', 'test_discovery_workflow.py',
    'test_memory_refresh.py', 'test_refresh_path_policy.py', 'test_refresh_receipt_snapshot.py', 'test_refresh_v2.py',
    'test_large_refresh_security.py', 'test_refresh_recovery.py', 'test_refresh_conflicts.py', 'test_refresh_cursor.py',
    'test_refresh_integration.py', 'test_refresh_invalidation.py', 'test_refresh_pending.py',
    'test_multi_machine_merge_replay.py', 'test_multi_machine_race.py', 'test_multi_machine_compatible.py',
    'test_multi_machine_conflicts.py', 'test_multi_machine_matrix.py',
    'test_layout_recovery.py', 'test_layout_recovery_frontier.py', 'test_layout_recovery_publication.py',
    'test_layout_recovery_retention.py',
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('performance', 'safety', 'complete'))
    parser.add_argument('--builds', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=7200, help='Per public operation; not whole 50k controller')
    parser.add_argument('--controller-timeout', type=int, default=172800)
    parser.add_argument('--model')
    parser.add_argument('--model-command', type=Path)
    args = parser.parse_args()
    if args.mode == 'complete' and not args.model:
        parser.error('Complete includes the frozen semantic cases; select an authorized exact real model')
    manifest = json.loads(args.builds.read_text())
    roots = {label: Path(row['root']).resolve(strict=True) for label, row in manifest['builds'].items()}
    if set(roots) != {'A', 'B', 'C'}:
        parser.error('Use all three frozen controlled builds')
    for label, path in roots.items():
        if source(path) != manifest['builds'][label]['source']:
            parser.error('Source differs from controlled build manifest: ' + label)
    root, out = roots['C'], args.out.resolve()
    if any(out.is_relative_to(p) or p.is_relative_to(out) for p in roots.values()):
        parser.error('Private output must remain outside all source builds')
    out.mkdir(mode=0o700, exist_ok=False)
    commands, failures, gates = [], [], []
    summary = {'schema': 'agent-memory/recall-qualification-matrix-v1', 'mode': args.mode,
        'platform': platform.platform(), 'python': sys.version, 'started_at': stamp(), 'argv': sys.argv,
        'controller_sha256': sha(Path(__file__).read_bytes()), 'build_manifest_sha256': sha(args.builds.read_bytes()),
        'commands': commands, 'failures': failures, 'gates': gates, 'qualification_passed': False,
        'single_platform_only': True, 'os_cold_claimed': False}
    def persist():
        save(out / 'matrix.json', summary)
    def run(label, argv, *, world=None):
        number = len(commands) + 1; began, start = stamp(), time.perf_counter()
        try:
            if world is None:
                process = subprocess.run(list(map(str, argv)), text=True, capture_output=True, timeout=args.controller_timeout)
            else:
                process = world.run(argv, check=False, timeout=args.controller_timeout, group=True)
            stdout, stderr, code, error = process.stdout, process.stderr, process.returncode, None
        except (subprocess.TimeoutExpired, OSError) as exc:
            def text(value):
                return value.decode(errors='replace') if isinstance(value, bytes) else value or ''
            stdout, stderr, code, error = text(getattr(exc, 'output', None)), text(getattr(exc, 'stderr', None)), None, repr(exc)
        (out / f'{number:03d}-{label}.stdout').write_text(stdout)
        (out / f'{number:03d}-{label}.stderr').write_text(stderr)
        commands.append({'label': label, 'argv': list(map(str, argv)), 'exit': code, 'error': error,
            'started_at': began, 'finished_at': stamp(), 'seconds': time.perf_counter() - start})
        if code != 0:
            failures.append({'label': label, 'exit': code, 'error': error})
        persist()
        return code, stdout
    def controller(name, mode, target, *extra):
        argv = [sys.executable, '-B', root / 'scripts/evaluations' / name]
        if mode:
            argv.append(mode)
        return run(target.name, [*argv, '--builds', args.builds.resolve(), '--out', target,
                                '--timeout', args.timeout, *extra])
    if args.mode in ('performance', 'complete'):
        navigation = {}
        for count in (1000, 10000, 50000):
            navigation[count] = {}
            for label, path in roots.items():
                target = out / f'navigation-{count}-{label}'
                code, _ = run(target.name, [sys.executable, '-B', root / 'scripts/evaluations/recall_navigation_qualification.py',
                    '--root', path, '--out', target, '--count', count, '--samples', '5', '--timeout', args.timeout])
                if code == 0:
                    navigation[count][label] = json.loads((target / 'observations.json').read_text())
            cell = navigation[count]
            if set(cell) != {'A', 'B', 'C'}:
                gates.append({'gate': 'navigation', 'count': count, 'passed': False, 'reason': 'missing actual build sample'}); continue
            hashes = {v['fixture_sha256'] for v in cell.values()}
            times = {label: [r['whole_seconds'] for r in value['observations'] if not r['first_empty_catalog']]
                     for label, value in cell.items()}
            medians = {label: statistics.median(values) for label, values in times.items()}
            complete = len(hashes) == 1 and all(len(values) == 5 for values in times.values())
            gates.append({'gate': 'navigation', 'count': count, 'samples': times, 'medians': medians,
                'fixture_equivalent': len(hashes) == 1, 'C_over_A': medians['C'] / medians['A'],
                'B_over_A_restoration': medians['B'] / medians['A'], 'C_over_B_incremental': medians['C'] / medians['B'],
                'required_50k_improvement': 0.20 if count == 50000 else None,
                'passed': complete and (count != 50000 or medians['C'] / medians['A'] <= 0.8)})
        refresh = out / 'refresh-50000'
        code, _ = controller('recall_refresh_qualification.py', 'refresh', refresh, '--count', '50000', '--samples', '5')
        gates.append({'gate': 'four-condition-refresh', 'passed': code == 0,
                      'evidence': str(refresh / 'observations.json')})
    if args.mode in ('safety', 'complete'):
        sys.path.insert(0, str(root / 'tests'))
        from world import World
        # Full modules, not selected tests. Missing final integration controls are
        # failures, never omissions or an inferred native platform pass.
        for name in SAFETY_SUITES:
            path = root / 'tests/contracts' / name
            if not path.is_file():
                failures.append({'label': name, 'error': 'required final integrated public control module unavailable'})
                continue
            with World() as world:
                code, stdout = run(name, [sys.executable, '-I', '-B', root / 'scripts/run_contract_suite.py', path], world=world)
                try:
                    result = json.loads(stdout)
                except ValueError:
                    result = {}
                gates.append({'gate': 'public-safety-module', 'module': name, 'counts': result,
                              'passed': code == 0 and result.get('passed') is True and result.get('tests', 0) > 0})
        literal = out / 'literal-quality'
        code, _ = controller('recall_quality_qualification.py', 'literal', literal)
        gates.append({'gate': '24-frozen-literal-cases-per-size-and-build', 'passed': code == 0})
    if args.mode == 'complete':
        memo = out / 'memo-24-worlds'
        code, _ = controller('recall_refresh_qualification.py', 'memo', memo, '--count', '1000')
        gates.append({'gate': '48-real-public-memo-observations-per-build', 'passed': code == 0})
        semantic = out / 'semantic-quality'
        extra = ['--model', args.model]
        if args.model_command:
            extra += ['--model-command', str(args.model_command.resolve())]
        code, _ = controller('recall_quality_qualification.py', 'semantic', semantic, *extra)
        gates.append({'gate': '16-frozen-semantic-cases-per-size-and-build', 'passed': code == 0})
    for label, path in roots.items():
        binding = source(path); save(out / ('source-after-' + label + '.json'), binding)
        if binding != manifest['builds'][label]['source']:
            failures.append({'label': label, 'error': 'source changed during qualification'})
    summary['qualification_passed'] = not failures and bool(gates) and all(g['passed'] for g in gates)
    summary['finished_at'] = stamp(); persist()
    print(json.dumps({'qualification_passed': summary['qualification_passed'], 'evidence': str(out / 'matrix.json')}))
    return 0 if summary['qualification_passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
