#!/usr/bin/env python3
"""Compile only frozen execution inputs; never rank, score, embed or query a graph.

The executor MUST read this output rather than suite.json. The separate scorer
may read oracle fields only after raw scientific outputs have durable hashes.
This structural separation is not a claim that the implementer was blinded.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import runpy
import subprocess
import sys

sys.dont_write_bytecode = True
STARTUP = Path(__file__).resolve().parents[5] / 'skills/agent-memory/scripts/source_startup.py'
_cache_guard = runpy.run_path(str(STARTUP), run_name='am_source_startup')['isolate']()

CASE_FIELDS = ('partition', 'operation', 'request', 'context_truth')
TEMPLATE_FIELDS = {'seed', 'historical', 'component', 'relations'}
SUITE_SHA256 = '36b663df90a5002ef93996ccc76e0a5de932f984d4a35178fe32b6f74e83b21f'
AM_COMMIT = '175d21fa26bbef41f8ec859b463876934e16db79'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encode(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False, allow_nan=False) + '\n').encode()


def fixture_references(am_root, suite_path, mutations):
    directory = suite_path.parent
    for name, expected in {
        'validate_suite.py': '5b47a20c573d1cbbf093016f6ef33b42c72196d7140990c5d569a773d0c552a4',
        'g1_relationship_overlay.py': '69127e95964ba5e9ee46ff5ce1a616aebe5172f624923445e7cc5714279f993a',
    }.items():
        if sha((directory / name).read_bytes()) != expected:
            raise ValueError('Frozen fixture helper changed: ' + name)
    sys.path.insert(0, str(directory))
    module_spec = importlib.util.spec_from_file_location('g1_input_fixture', directory / 'validate_suite.py')
    validator = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(validator)
    native = validator.Native(am_root)
    files, rows, _ = validator.overlay.bank(am_root, 1000, overlay=True)
    refs = {'rec:' + row['label']: {'format': 'record-version',
            'id': row['metadata']['id'], 'version': row['metadata']['version'], 'path': row['path']}
            for row in rows}
    refs.update({'ctx:' + path: {'format': 'context-source', 'path': path}
                 for path in files if not path.startswith('topics/')})
    lrf, snf, template = validator.overlay.load(am_root)
    for mutation in mutations.values():
        _, changed = validator.mutate(native, lrf, snf, files, rows, mutation, lrf.PID)
        if 'new_ref' in mutation:
            new = next(row for row in changed if row['label'] == mutation['new_ref'][4:])
            refs[mutation['new_ref']] = {'format': 'record-version', 'id': new['metadata']['id'],
                                         'version': new['metadata']['version'], 'path': new['path']}
    return refs


def compile_inputs(am_root, suite_path, spec_path):
    raw = suite_path.read_bytes()
    if sha(raw) != SUITE_SHA256:
        raise ValueError('Frozen suite byte identity differs; do not retune labels')
    suite = json.loads(raw)
    revision = subprocess.run(['git', '-C', str(am_root), 'rev-parse', 'HEAD'],
                              capture_output=True, text=True, check=True).stdout.strip()
    if revision != AM_COMMIT:
        raise ValueError('Use the original authoring source, not an unreviewed runtime substitution')
    pins = suite['sources']
    for relative, expected in pins['pinned_files_sha256'].items():
        if sha((am_root / relative).read_bytes()) != expected:
            raise ValueError('Authored source pin changed: ' + relative)
    tree = []
    for relative in pins['native_package_tree']['roots']:
        for path in sorted((am_root / relative).rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts:
                tree.append(path.relative_to(am_root).as_posix() + ' ' + sha(path.read_bytes()))
    if sha('\n'.join(tree).encode()) != pins['native_package_tree']['sha256']:
        raise ValueError('Authored native package tree changed')
    refs = fixture_references(am_root, suite_path, suite['mutations'])
    cases = []
    scorer_rows = []
    for ordinal, case in enumerate(suite['cases'], 1):
        item = {'case_ordinal': ordinal, **{field: case[field] for field in CASE_FIELDS}}
        scorer_rows.append({'case_ordinal': ordinal, 'case_id': case['case_id'], 'split': case['split']})
        item['context_truth'] = {**case['context_truth'],
            'deferred': [refs[ref] for ref in case['context_truth']['deferred']]}
        template = case['template']
        if template is not None and (not isinstance(template, dict) or set(template) - TEMPLATE_FIELDS):
            raise ValueError('Unreviewed template input fields')
        item['template'] = {**template, 'seed': refs[template['seed']]} if template is not None else None
        cases.append(item)
    mutations = {}
    for name, mutation in suite['mutations'].items():
        mutations[name] = {key: value for key, value in mutation.items() if key not in {'target', 'new_ref'}}
        mutations[name]['target'] = refs[mutation['target']]
        if 'new_ref' in mutation:
            mutations[name]['new_record'] = refs[mutation['new_ref']]
    # No primary/caveat/relationship/disclosure/availability/rationale fields are
    # accessed or serialized into executor inputs. The separate scorer map owns
    # descriptive case IDs and splits; mutation bodies are actions, not results.
    value = {
        'schema': 'am255/oracle-separated-inputs-v1',
        'authoring_am_commit': revision,
        'authoring_native_tree_sha256': pins['native_package_tree']['sha256'],
        'suite_sha256': sha(raw),
        'spec_sha256': sha(spec_path.read_bytes()),
        'compiler_sha256': sha(Path(__file__).read_bytes()),
        'source_startup_sha256': sha(STARTUP.read_bytes()),
        'independence_limit': 'Structural oracle separation; implementer not claimed blinded',
        'case_order': [case['case_ordinal'] for case in cases],
        'cases': cases,
        'mutation_order': list(suite['mutations']),
        'mutations': mutations,
    }
    return value, scorer_rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--am-root', type=Path, required=True)
    parser.add_argument('--suite', type=Path, required=True)
    parser.add_argument('--spec', type=Path, default=Path(__file__).with_name('spec.json'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scorer-map', type=Path, required=True)
    args = parser.parse_args()
    value, scorer_rows = compile_inputs(args.am_root.resolve(strict=True), args.suite.resolve(strict=True),
                                       args.spec.resolve(strict=True))
    raw = encode(value)
    scorer_raw = encode({'schema': 'am255/scorer-only-case-map-v1',
                         'input_sha256': sha(raw), 'suite_sha256': SUITE_SHA256,
                         'cases': scorer_rows})
    # Exclusive creation preserves any historical compiled artifact on retries.
    with args.output.open('xb') as stream, args.scorer_map.open('xb') as scorer_stream:
        import os
        for target, content in ((stream, raw), (scorer_stream, scorer_raw)):
            target.write(content)
            target.flush()
            os.fsync(target.fileno())
    print(json.dumps({'schema': value['schema'], 'cases': len(value['cases']),
                      'output_sha256': sha(raw), 'scorer_map_sha256': sha(scorer_raw),
                      'model_or_graph_executed': False}))
    return 0


if __name__ == '__main__':
    sys.exit(main())
