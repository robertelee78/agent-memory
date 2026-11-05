#!/usr/bin/env python3
"""Source-bound sequential public navigation timing; not a semantic quality test."""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from labeled_recall_probe import source, inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--count', type=int, choices=(1000, 10000, 50000), required=True)
    parser.add_argument('--samples', type=int, default=5)
    parser.add_argument('--timeout', type=int, default=600)
    args = parser.parse_args()
    if args.samples < 5 or args.timeout < 1:
        parser.error('At least five samples and a positive deadline are required')
    root = args.root.resolve(strict=True)
    out = args.out.resolve()
    if out.is_relative_to(root) or root.is_relative_to(out):
        parser.error('Output must be outside source')
    out.mkdir(mode=0o700, parents=False, exist_ok=False)
    sys.path.insert(0, str(root / 'tests'))
    sys.path.insert(0, str(root / 'scripts/evaluations'))
    from world import World
    from identity_fixtures import bind
    from semantic_navigation_fixture import corpus
    before_source = source(root)
    (out / 'source-before.json').write_text(json.dumps(before_source, indent=2))
    commands, observations = [], []
    summary = {'platform': platform.platform(), 'python': sys.version, 'argv': sys.argv,
               'controller_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
               'source_commit': before_source['commit'], 'count': args.count,
               'observations': observations, 'cold_filesystem_claimed': False,
               'semantic_quality_claimed': False, 'model_time_included': False}
    def save_summary():
        (out / 'observations.json').write_text(json.dumps(summary, indent=2))
    with World() as w:
        pack = w.inside('portable pack')
        shutil.copytree(root / 'skills/agent-memory', pack, ignore=shutil.ignore_patterns('__pycache__'))
        repo = w.repository()
        (repo / 'feature.py').write_text('VALUE = 1\n')
        w.git('add', 'feature.py', cwd=repo); w.git('commit', '-m', 'Synthetic source', cwd=repo)
        (repo / 'feature.py').write_text('VALUE = 2  # staged\n'); w.git('add', 'feature.py', cwd=repo)
        template = json.loads((pack / 'templates/record-proposal.example.json').read_text())['metadata']
        setup = time.perf_counter()
        files, rows, stats = corpus(template, args.count)
        bank = repo / 'memory-bank'; bank.mkdir()
        for name, body in files.items():
            target = bank / name; target.parent.mkdir(parents=True, exist_ok=True); target.write_text(body)
        bind(w, repo)
        summary['setup_seconds'] = time.perf_counter() - setup
        summary['fixture'] = stats
        summary['fixture_sha256'] = hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()
        initial = inventory(bank); index = (repo / '.git/index').read_bytes()
        head = w.git('rev-parse', 'HEAD', cwd=repo).stdout
        selected = next(row['path'] for row in rows if row['label'] == 'retry-limit')
        leaf = next(n for n, b in files.items() if n.startswith('topics/n/') and '| ['+selected+'](<' in b)
        group = next(n for n, b in files.items() if n.startswith('topics/g/') and '| ['+leaf+'](<' in b)
        def save(value, name):
            path = w.inside(name); path.write_text(json.dumps(value)); return path
        def invoke(helper, *arguments):
            number = len(commands) + 1
            argv = [str(x) for x in (sys.executable, '-I', '-B', pack/'scripts'/helper, *arguments)]
            started = time.perf_counter()
            try:
                result = w.run(argv, check=False, timeout=args.timeout, group=True)
                stdout, stderr, code, error = result.stdout, result.stderr, result.returncode, None
            except (subprocess.TimeoutExpired, OSError) as exc:
                stdout = getattr(exc, 'output', '') or ''; stderr = getattr(exc, 'stderr', '') or ''
                stdout = stdout.decode(errors='replace') if isinstance(stdout, bytes) else stdout
                stderr = stderr.decode(errors='replace') if isinstance(stderr, bytes) else stderr
                code, error = None, repr(exc)
            (out / f'{number}.stdout').write_text(stdout); (out / f'{number}.stderr').write_text(stderr)
            commands.append({'argv': argv, 'exit': code, 'seconds': time.perf_counter()-started, 'error': error})
            (out / 'commands.json').write_text(json.dumps(commands, indent=2))
            if code != 0:
                raise RuntimeError(f'Public command {number} failed: {error or code}')
            return json.loads(stdout)
        question = 'How often may a temporary network error be attempted again?'
        try:
            for attempt in range(args.samples + 1):
                started = time.perf_counter(); first = len(commands)
                state = invoke('discover_memory.py', 'start', '--repo', repo, '--intent', 'recall', '--task', question)
                for path in (group, leaf):
                    state = invoke('discover_memory.py', 'node', '--repo', repo, '--input', save(state['discovery'], 'nav.json'), '--path', path)
                request = {'project_id': state['discovery']['binding']['project_id'], 'query': question, 'ids': [],
                           'paths': [selected], 'context': {}, 'kinds': ['semantic-fact'], 'minimum_trust': 'reviewed',
                           'limit': 8, 'include_context': False}
                result = invoke('discover_memory.py', 'read', '--repo', repo, '--input', save(state['discovery'], 'nav.json'),
                                '--request', save(request, 'request.json'), '--rationale', 'Fixture-known path for timing; not semantic evaluation.')['qualification']
                observations.append({'attempt': attempt, 'first_empty_catalog': attempt == 0,
                    'whole_seconds': time.perf_counter()-started, 'operations': commands[first:],
                    'optional_indexes_used': result['optional_indexes_used'], 'metrics': result['metrics'],
                    'selected_paths': [r['path'] for r in result['results']]})
                save_summary()
                if observations[-1]['selected_paths'] != [selected]:
                    raise RuntimeError('Selected path changed; retain this failure')
        finally:
            summary['bank_index_head_preserved'] = (initial == inventory(bank) and index == (repo / '.git/index').read_bytes()
                and head == w.git('rev-parse', 'HEAD', cwd=repo).stdout)
            save_summary()
    after_source = source(root)
    (out / 'source-after.json').write_text(json.dumps(after_source, indent=2))
    if before_source != after_source or not summary['bank_index_head_preserved']:
        raise RuntimeError('Source or protected state changed; evidence is not qualified')


if __name__ == '__main__':
    main()
