#!/usr/bin/env python3
"""Freeze source-equivalent #206 A/B/C builds; never use historical timing as A."""
import argparse
import ast
import difflib
import hashlib
import importlib.metadata
import json
from pathlib import Path
import platform
import subprocess
import sys
from datetime import datetime, timezone

sys.dont_write_bytecode = True
# This bootstrap must use only stdlib: -B does not prevent cached helper reads.
for _directory in ('tests', 'scripts', 'skills/agent-memory'):
    for _cache in (Path(__file__).resolve().parents[2] / _directory).rglob('*'):
        if (_cache.name == '__pycache__' and _cache.is_symlink()) or _cache.suffix in ('.pyc', '.pyo'):
            raise ValueError('Qualification helper bytecode is not source-bound: ' + str(_cache))
from labeled_recall_probe import source as _source

RUNTIME = 'skills/agent-memory/scripts/agent_memory/'
DELTA_FILES = {RUNTIME + name for name in ('recall_catalog.py', 'retrieval.py', 'validation_memo.py')}

def source(root):
    """Bind helper execution as well as portable source, before local imports."""
    for directory in ('tests', 'scripts', 'skills/agent-memory'):
        for cache in (root / directory).rglob('*'):
            if (cache.name == '__pycache__' and cache.is_symlink()) or cache.suffix in ('.pyc', '.pyo'):
                raise ValueError('Qualification helper bytecode is not source-bound: ' + str(cache))
    return _source(root)



def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def stamp():
    return datetime.now(timezone.utc).isoformat()


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True)


def without_optimization(text, memo):
    """Reverse only the reviewed scoped catalog seam, preserving later integration."""
    tree = ast.parse(text)
    calls = [n for n in ast.walk(tree) if isinstance(n, ast.With) and any(
        isinstance(i.context_expr, ast.Call) and isinstance(i.context_expr.func, ast.Attribute)
        and isinstance(i.context_expr.func.value, ast.Name) and i.context_expr.func.value.id == 'validation_memo'
        and i.context_expr.func.attr == 'suspended' for i in n.items)]
    if len(calls) != 1:
        raise ValueError('Final source must contain exactly the approved #206 catalog suspension seam')
    node = calls[0]
    if not any(isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and n.func.attr == 'load_catalog'
               for n in ast.walk(node)):
        raise ValueError('Suspension no longer encloses the reviewed catalog load; rebind the design')
    lines = text.splitlines(keepends=True)
    lines[node.lineno - 1:node.end_lineno] = [line[4:] for line in lines[node.lineno:node.end_lineno]]
    text = ''.join(lines)
    # The import belongs to the optimization only unless later integration uses it.
    parsed = ast.parse(text)
    if not any(isinstance(n, ast.Name) and isinstance(n.ctx, ast.Load) and n.id == 'validation_memo'
               for n in ast.walk(parsed)):
        imports = [n for n in ast.walk(parsed) if isinstance(n, ast.ImportFrom) and n.module is None
                   and n.level == 1 and any(a.name == 'validation_memo' for a in n.names)]
        if len(imports) != 1:
            raise ValueError('Ambiguous #206 import; source-equivalent reverse needs review')
        node = imports[0]
        names = [a.name + (' as ' + a.asname if a.asname else '') for a in node.names if a.name != 'validation_memo']
        lines = text.splitlines(keepends=True)
        lines[node.lineno - 1:node.end_lineno] = [(' ' * node.col_offset + 'from . import ' + ', '.join(names) + '\n')] if names else []
        text = ''.join(lines)
    tree = ast.parse(memo)
    functions = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'suspended']
    if len(functions) != 1:
        raise ValueError('Missing reviewed #206 memo suspension function')
    node = functions[0]
    lines = memo.splitlines(keepends=True)
    start = min([node.lineno] + [d.lineno for d in node.decorator_list]) - 1
    del lines[start:node.end_lineno]
    return text, ''.join(lines)


def without_repair(text):
    tree = ast.parse(text)
    assignments = [n for n in tree.body if isinstance(n, ast.Assign)
                   and any(isinstance(t, ast.Name) and t.id == 'ENGINE_ROOTS' for t in n.targets)]
    if len(assignments) != 1:
        raise ValueError('Ambiguous audited catalog closure')
    values = assignments[0].value
    entries = [n for n in values.elts if isinstance(n, ast.Constant) and n.value == 'bank_capacity']
    if len(entries) != 1:
        raise ValueError('Missing independently measured #206 catalog repair')
    node = entries[0]
    lines = text.splitlines(keepends=True)
    line = lines[node.lineno - 1]
    if node.lineno != node.end_lineno or line[node.end_col_offset:node.end_col_offset + 1] != ',':
        raise ValueError('Review changed repair syntax before comparing builds')
    lines[node.lineno - 1] = line[:node.col_offset] + line[node.end_col_offset + 1:]
    return ''.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True, help='Clean final integrated C checkout')
    parser.add_argument('--out', type=Path, required=True, help='New external private build directory')
    args = parser.parse_args()
    root, out = args.root.resolve(strict=True), args.out.resolve()
    if out.is_relative_to(root) or root.is_relative_to(out):
        parser.error('Build directory must be outside source')
    binding = source(root)
    for relative in binding['tracked']:
        if relative.startswith(RUNTIME) and relative.endswith('.py') and not relative.endswith('/retrieval.py'):
            tree = ast.parse((root / relative).read_text())
            if any(isinstance(n, ast.Call) and
                   ((isinstance(n.func, ast.Attribute) and n.func.attr == 'suspended'
                     and isinstance(n.func.value, ast.Name) and n.func.value.id == 'validation_memo')
                    or (isinstance(n.func, ast.Name) and n.func.id == 'suspended'))
                   for n in ast.walk(tree)):
                raise ValueError('Additional memo suspension caller requires an independently rebound reverse delta: ' + relative)
    out.mkdir(mode=0o700, parents=False, exist_ok=False)
    commands = []
    def command(*argv):
        result = subprocess.run(list(map(str, argv)), text=True, capture_output=True)
        number = len(commands) + 1
        (out / f'{number}.stdout').write_text(result.stdout)
        (out / f'{number}.stderr').write_text(result.stderr)
        commands.append({'argv': list(map(str, argv)), 'exit': result.returncode, 'recorded_at': stamp()})
        save(out / 'commands.json', commands)
        result.check_returncode()
        return result.stdout
    paths = {}
    for label in ('A', 'B', 'C'):
        target = out / label
        command('git', 'clone', '--no-hardlinks', '--no-checkout', str(root), str(target))
        command('git', '-C', str(target), 'checkout', '--detach', binding['commit'])
        if label != 'C':
            retrieval = target / (RUNTIME + 'retrieval.py')
            memo = target / (RUNTIME + 'validation_memo.py')
            retrieval_text, memo_text = without_optimization(retrieval.read_text(), memo.read_text())
            retrieval.write_text(retrieval_text)
            memo.write_text(memo_text)
            if label == 'A':
                catalog = target / (RUNTIME + 'recall_catalog.py')
                catalog.write_text(without_repair(catalog.read_text()))
            patch = git(target, 'diff', '--binary', '--no-ext-diff', '--', *sorted(DELTA_FILES))
            (out / f'C-to-{label}.patch').write_text(patch)
            command('git', '-C', str(target), 'add', '--', *sorted(DELTA_FILES))
            command('git', '-C', str(target), '-c', 'user.name=Qualification Controller',
                    '-c', 'user.email=qualification@example.invalid', 'commit', '-m', f'#206 controlled comparison {label}')
        paths[label] = target
    bindings = {label: source(path) for label, path in paths.items()}
    artifacts = {label: json.loads(command(sys.executable, '-I', '-B', path / 'scripts/artifact.py',
                                         'inspect', '--pack', path / 'skills/agent-memory'))
                 for label, path in paths.items()}
    for label in ('A', 'B'):
        changed = {name for name in binding['tracked'] if bindings[label]['tracked'].get(name) != binding['tracked'][name]}
        if not changed.issubset(DELTA_FILES) or set(bindings[label]['tracked']) != set(binding['tracked']):
            raise ValueError('Non-#206 source differs; no qualified comparison')
    if source(root) != binding:
        raise ValueError('Final integrated source changed while freezing comparison builds')
    save(out / 'builds.json', {'schema': 'agent-memory/recall-controlled-builds-v1', 'created_at': stamp(),
        'argv': sys.argv, 'platform': platform.platform(), 'python': sys.version,
        'dependencies': sorted((d.metadata['Name'], d.version) for d in importlib.metadata.distributions()),
        'controller_sha256': sha(Path(__file__).read_bytes()), 'integrated_commit': binding['commit'],
        'definitions': {'A': 'Integrated source minus only #206 repair and optimization',
                        'B': 'Identical integrated source with #206 closure repair only',
                        'C': 'Full integrated candidate; no reverse patch'},
        'builds': {label: {'root': str(paths[label]), 'tree': git(paths[label], 'rev-parse', 'HEAD^{tree}').strip(),
                          'portable_sha256': sha(encoded(b['portable'])), 'artifact': artifacts[label], 'source': b}
                   for label, b in bindings.items()},
        'patches': {p.name: sha(p.read_bytes()) for p in out.glob('*.patch')},
        'non_206_sources_byte_identical': True})
    print(json.dumps({'build_manifest': str(out / 'builds.json'), 'acceptance_claimed': False}))


if __name__ == '__main__':
    main()
