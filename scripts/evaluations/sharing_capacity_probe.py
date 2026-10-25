"""Design experiment using isolated installed public operations, not a quality pass."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time

p = argparse.ArgumentParser()
p.add_argument('--root', required=True)
p.add_argument('--out', required=True)
a = p.parse_args()
root = Path(a.root)
sys.path.insert(0, str(root / 'tests'))
from world import World
import installed_cli_fixtures as installed
from contracts import test_installed_layout as layout
from contracts.test_semantic_sharing import SemanticSharingContract
from contracts.test_retrieval import RetrievalContract
from record_fixtures import record_text, project_id

source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
paths = subprocess.check_output(['git', 'ls-tree', '-rz', '--name-only', source,
    '--', 'skills/agent-memory'], cwd=root).decode().rstrip('\0').split('\0')
expected = {name: hashlib.sha256(subprocess.check_output(['git', 'show', source + ':' + name], cwd=root)).hexdigest()
            for name in paths}
raw_output = Path(a.out).with_suffix('')
raw_output.mkdir(parents=True, exist_ok=True, mode=0o700)
rows = []
for count in (8, 24, 32, 256, 1000):
    with World() as w:
        channel = w.repository('capacity runtime channel')
        shutil.copytree(root / 'skills/agent-memory', channel / 'skills/agent-memory',
                        ignore=shutil.ignore_patterns('__pycache__'))
        w.git('add', '.', cwd=channel)
        w.git('commit', '-m', 'Exact source for isolated capacity experiment', cwd=channel)
        activation = layout.install(w, channel, 'copy', 'all')
        assert activation['verified']
        cli = Path(activation['receipt']['bin_dir']) / 'agent-memory'
        receipt = json.loads((w.paths['state'] / 'agent-memory/install/receipt.json').read_text())
        pack = Path(receipt['source']['pack'])
        actual = {'skills/agent-memory/' + str(f.relative_to(pack)): hashlib.sha256(f.read_bytes()).hexdigest()
                  for f in pack.rglob('*') if f.is_file()}
        assert actual == expected
        f = SemanticSharingContract()
        repo, bank, peer, base = f.fixture(w)
        pid = project_id((bank / 'MEMORY.md').read_text())
        (bank / 'topics').mkdir(exist_ok=True)
        route_rows = []
        for start in range(0, count, 64):
            index = 'topics/index-' + str(start // 64).zfill(3) + '.md'
            routes = []
            for n in range(start, min(start + 64, count)):
                name = 'topics/lesson-' + str(n).zfill(6) + '.md'
                (bank / name).write_text(record_text('Capacity probe learning' + str(n).zfill(6)
                    + ': disposable fixture data only.\n', pid))
                routes.append({'path': name, 'purpose': 'Disposable capacity fixture lesson',
                    'scope': 'repository', 'lifecycle': 'active', 'read-when': 'For this isolated capacity experiment'})
            request = f.save(w, {'path': index, 'project-id': pid,
                'updated': '2026-10-24T12:00:00Z', 'routes': routes})
            rendered = w.run([sys.executable, '-I', pack / 'scripts/bank_layout.py',
                'render-index', '--input', request], check=False)
            assert rendered.returncode == 0, rendered.stdout + rendered.stderr
            (bank / index).write_text(json.loads(rendered.stdout)['content'])
            route_rows.append('| [' + index + '](<' + index + '>) | Capacity fixture group | repository | active | During the capacity experiment |\n')
        router = bank / 'MEMORY.md'
        router.write_text(router.read_text().replace('\n## Omitted contexts',
            ''.join(route_rows) + '\n## Omitted contexts', 1))
        valid, validation = installed.command(w, cli, 'bank', 'validate', '--repo', repo)
        assert valid.returncode == 0, validation or valid.stderr
        before = f.state(w, repo, bank)
        request = RetrievalContract().request(pid, query='capacity probe learning000000', include_context=False)
        started = time.perf_counter()
        recall, recalled = installed.command(w, cli, 'search', '--repo', repo, '--request', f.save(w, request))
        recall_seconds = time.perf_counter() - started
        started = time.perf_counter()
        result, value = installed.command(w, cli, 'semantic', 'inspect', '--repo', repo)
        semantic_seconds = time.perf_counter() - started
        (raw_output / (str(count) + '-semantic.json')).write_text(result.stdout)
        (raw_output / (str(count) + '-recall.json')).write_text(recall.stdout)
        after = f.state(w, repo, bank)
        assert before == after, 'Experiment changed canonical bank or feature code/index/HEAD'
        row = {'distinct_fixture_lessons': count, 'canonical_files': len(before[0]),
            'canonical_bytes': sum(len(raw) for raw in before[0].values()), 'structural_validation': True,
            'exact_literal_recall': {'exit': recall.returncode, 'seconds': recall_seconds,
                'result_count': len(recalled.get('results', [])), 'json_bytes': len(recall.stdout.encode())},
            'semantic_inspect': {'exit': result.returncode, 'seconds': semantic_seconds,
                'source_components': len(value['sources']) if 'sources' in value else None,
                'required_pairs': len(value['required_pairs']) if 'required_pairs' in value else None,
                'diagnostic': value.get('error'), 'json_bytes': len(result.stdout.encode())},
            'canonical_and_feature_state_preserved': True}
        rows.append(row)
        print(json.dumps(row), flush=True)

output = {'intent': 'Design capacity/refusal experiment; not retrieval-quality or stress acceptance',
    'source_commit': source, 'probe_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    'platform': platform.system(), 'python': platform.python_version(), 'portable_files_verified': len(expected),
    'fixture_schema': 'agent-memory/v1', 'live_configuration_or_memory_changed': False,
    'token_measurement': None, 'token_measurement_reason': 'No harness tokenizer used; no token estimate claimed',
    'measurements': rows}
Path(a.out).write_text(json.dumps(output, indent=2) + '\n')
