#!/usr/bin/env python3
"""Independent #255 raw synthetic research; not production retrieval or a G1 gate.

Freeze-only performs source/native fixture/hash/host-memory preflight without
starting a model or graph. Run requires the exact externally reviewed source
commit, a retained freeze, and an explicitly reserved nonoverlapping host slot.
No oracle, scorer map, prior retrieval result or expected outcome is read here.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import runpy
import signal
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parents[4]
STARTUP = REPOSITORY / 'skills/agent-memory/scripts/source_startup.py'
_cache_guard = runpy.run_path(str(STARTUP), run_name='am_source_startup')['isolate']()
from executor_native import Fixtures, compact, digest, native_identity
from executor_render import identity as renderer_identity, query as render_query
from executor_runtime import (Model, VectorStore, artifacts, command, memory_precondition,
                              no_active_services, pointer, save, sha)
from executor_graph import Graph, projection

INPUT_SHA = '1986071901b4004618310b15f2ebcbf6f03e6ee7afc9e113eca6d4deff893ef2'
AM_COMMIT = '175d21fa26bbef41f8ec859b463876934e16db79'
MAPPER = 'No independently reviewed contract-derived AM175 research outcome/caveat assembler exists'
SCOPE = ('Raw private synthetic scratch mechanisms only; no production P1/G3 API, semantic outcome, '
         'caveat, operator, license, profile, G1, feature, or release acceptance. '
         'Shared host measurements are not clean performance passes.')


def load(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError('Require a real regular input file, not a symlink: ' + str(path))
    return json.loads(path.read_text(encoding='utf-8'))


def fresh_root(path):
    if path.is_symlink() or path.exists():
        raise ValueError('Output must be a fresh owned root; retain failed attempts')
    path.mkdir(parents=True, mode=0o700)
    path = path.resolve(strict=True)
    os.chmod(path, 0o700)
    return path


def source_files():
    files = [HERE / name for name in ('execute.py', 'executor_native.py', 'executor_runtime.py',
                                      'executor_graph.py', 'executor_render.py', 'executor_templates.json',
                                      'compile_inputs.py')]
    files += [STARTUP, HERE.parent / 'independent-suite/validate_suite.py',
              HERE.parent / 'independent-suite/g1_relationship_overlay.py',
              HERE.parent / 'runtime-controls/common.py',
              HERE.parent / 'serving-comparison/compare_serving.py']
    if (HERE / 'score.py').is_file():
        files.append(HERE / 'score.py')
    return files


def pin_source(root, path, commit):
    relative = path.relative_to(REPOSITORY).as_posix()
    blob = subprocess.run(['git', '-C', str(REPOSITORY), 'show', commit + ':' + relative],
                          capture_output=True, check=True).stdout
    raw = path.read_bytes()
    if blob != raw:
        raise ValueError('Executing source differs from exact freeze commit: ' + relative)
    destination = root / 'source' / relative
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    with destination.open('xb') as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    return pointer(root, destination) | {'repository_path': relative}


def copy_input(root, path, name):
    destination = root / name
    with destination.open('xb') as stream:
        stream.write(path.read_bytes())
        stream.flush()
        os.fsync(stream.fileno())
    return pointer(root, destination)


def validate_inputs(inputs, spec, input_path, spec_path):
    if sha(input_path) != INPUT_SHA or inputs.get('schema') != 'am255/oracle-separated-inputs-v1':
        raise ValueError('Only the exact allowlisted compiled inputs are accepted')
    if inputs['authoring_am_commit'] != AM_COMMIT or inputs['spec_sha256'] != sha(spec_path):
        raise ValueError('Compiled source/spec binding differs')
    if inputs['compiler_sha256'] != sha(HERE / 'compile_inputs.py') or inputs['source_startup_sha256'] != sha(STARTUP):
        raise ValueError('Compiler/startup binding differs')
    allowed = {'case_ordinal', 'partition', 'operation', 'request', 'context_truth', 'template'}
    if len(inputs['cases']) != 101 or inputs['case_order'] != list(range(1, 102)):
        raise ValueError('Opaque ordered101 input contract differs')
    for ordinal, case in enumerate(inputs['cases'], 1):
        if set(case) != allowed or case['case_ordinal'] != ordinal or case['partition'] not in ('base', 'overlay'):
            raise ValueError('Nonallowlisted/descriptive case fields or invalid opaque ordinal')
    if sum(c['partition'] == 'base' for c in inputs['cases']) != 88:
        raise ValueError('Expected88 base controls and101 overlay controls')
    renderer_identity(spec)


def generation_stages(inputs):
    return [None, *inputs['mutation_order']]


def selected_mutation(case):
    truth = case['context_truth']
    return truth['mutation'] if truth['stage'] == 'after-rebuild' else None


def mechanism_prerequisite(case):
    truth = case['context_truth']
    if truth['runtime_state'] != 'ready-complete-generation':
        return 'Unexecuted actual state transition requires its own reviewed controlled phase: ' + truth['runtime_state']
    if truth['stage'] == 'before-rebuild':
        return 'Actual changed canonical source with a stale prior generation and reviewed fencing observation required; ordinary rebuild does not exercise stale state'
    return None


def prepare_size(fixtures, inputs, size, root):
    directory = root / 'sizes' / str(size)
    directory.mkdir(parents=True, mode=0o700)
    generations, text_by_hash, ordered = {}, {}, []
    # Base and overlay complete sources precede mutation deltas; exact rendered
    # duplicates within this size use the first byte-identical occurrence only.
    stages = [(v, None) for v in ('base', 'overlay')]
    stages += [(v, m) for m in inputs['mutation_order'] for v in ('base', 'overlay')]
    artifacts_rows = []
    for variant, mutation in stages:
        bank, info = fixtures.build(size, variant, mutation)
        project = projection(fixtures, bank)
        catalog = fixtures.catalog(bank) | {'nodes': project['nodes'], 'routes': project['routes'],
                                          'mutation': mutation, 'stage': 'after-rebuild' if mutation else None}
        stage = directory / bank.generation_id
        stage.mkdir(mode=0o700)
        save(stage / 'catalog.json', catalog)
        save(stage / 'projection.json', project)
        coverage = {}
        for source, rendered in fixtures.rendered(bank):
            h = digest(rendered)
            if h not in text_by_hash:
                text_by_hash[h] = rendered
                ordered.append({'rendered_sha256': h, 'source_keys': [], 'first_generation': bank.generation_id,
                                'first_path': source['path']})
            coverage[source['key']] = h
        generations[(variant, mutation)] = {'bank': bank, 'projection': project, 'coverage': coverage,
                                             'root': stage, 'info': info}
        artifacts_rows.append({'generation_id': bank.generation_id, 'variant': variant,
                               'mutation': mutation, 'stage': 'after-rebuild' if mutation else None,
                               'canonical_sha256': bank.canonical_sha256, 'catalog': pointer(root, stage / 'catalog.json'),
                               'projection': pointer(root, stage / 'projection.json')})
    index_of = {row['rendered_sha256']: i for i, row in enumerate(ordered)}
    keys_by_hash = {h: set() for h in text_by_hash}
    for generation in generations.values():
        for key, h in generation['coverage'].items():
            keys_by_hash[h].add(key)
        generation['vector_rows'] = {key: index_of[h] for key, h in generation['coverage'].items()}
    for row in ordered:
        row['source_keys'] = sorted(keys_by_hash[row['rendered_sha256']])
    batches = [ordered[start:start + 8] for start in range(0, len(ordered), 8)]
    queries, seen = [], set()
    native_checks = []
    for variant in ('base', 'overlay'):
        for case in inputs['cases']:
            if variant == 'base' and case['partition'] != 'base':
                continue
            g = generations[(variant, selected_mutation(case))]
            native, context = fixtures.qualify(g['bank'], case)
            prerequisite = mechanism_prerequisite(case)
            eligible_query = native['request_valid'] and prerequisite is None and 'ranking' in case['request']
            rendered = render_query(case['request']['query'])
            h = digest(rendered)
            first = eligible_query and h not in seen
            if first:
                seen.add(h)
                queries.append({'variant': variant, 'case_ordinal': case['case_ordinal'], 'rendered_sha256': h})
            native_checks.append({'variant': variant, 'case_ordinal': case['case_ordinal'],
                                  'generation_id': g['bank'].generation_id, 'request_valid': native['request_valid'],
                                  'eligible_count': len(native['eligible']), 'crosscheck': native['crosscheck'],
                                  'query_first_allowed_occurrence': first, 'mechanism_prerequisite': prerequisite})
    schedule = {'schema': 'am255/ordered-source-query-schedule-v1', 'size': size,
                'document_batches': batches, 'query_first_occurrences': queries,
                'native_checks': native_checks, 'cross_size_reuse': False,
                'order': 'Base source-path UTF8 byte order; overlay additions; mutation declaration order; base then overlay opaque case order'}
    save(directory / 'schedule.json', schedule)
    return {'size': size, 'root': directory, 'generations': generations, 'ordered': ordered,
            'texts': text_by_hash, 'generation_artifacts': artifacts_rows, 'query_schedule': queries,
            'schedule': pointer(root, directory / 'schedule.json')}


def freeze(args, root):
    inputs, spec, config = load(args.inputs), load(args.spec), load(args.host_config)
    validate_inputs(inputs, spec, args.inputs, args.spec)
    revision = command(['git', '-C', REPOSITORY, 'rev-parse', 'HEAD'])['stdout'].strip()
    if args.reviewed_source_commit and revision != args.reviewed_source_commit:
        raise ValueError('Actual source HEAD differs from externally reviewed commit')
    files = [pin_source(root, p, revision) for p in source_files()]
    authoring_identity = native_identity(args.am_root, inputs)
    save(root / 'authoring-native-identity.json', authoring_identity)
    bindings = {'authoring_am': {key: authoring_identity[key] for key in ('commit', 'native_tree_sha256')},
                'inputs': copy_input(root, args.inputs, 'inputs.json'),
                'spec': copy_input(root, args.spec, 'spec.json'),
                'io': copy_input(root, HERE / 'execution-io.json', 'execution-io.json'),
                'recipe': next(p for p in files if p['repository_path'].endswith('/execute.py')) | {'commit': revision},
                'helpers': [p | {'source_commit': revision} for p in files if not p['repository_path'].endswith('/execute.py')]}
    from executor_native import FIXTURE_FILES
    for relative in FIXTURE_FILES:
        path = args.am_root / relative
        destination = root / 'authoring-source' / relative
        destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with destination.open('xb') as stream:
            stream.write(path.read_bytes())
            stream.flush()
            os.fsync(stream.fileno())
        bindings['helpers'].append(pointer(root, destination) | {
            'repository_path': relative, 'source_commit': AM_COMMIT})
    io_repo_path = (HERE / 'execution-io.json').relative_to(REPOSITORY).as_posix()
    io_blob = subprocess.run(['git', '-C', str(REPOSITORY), 'show', revision + ':' + io_repo_path], capture_output=True, check=True).stdout
    if digest(io_blob) != bindings['io']['sha256']:
        raise ValueError('IO differs from source freeze commit')
    bindings['io']['repository_path'] = io_repo_path
    bindings['spec']['repository_path'] = args.spec.relative_to(REPOSITORY).as_posix()
    bindings['inputs']['repository_path'] = args.inputs.relative_to(REPOSITORY).as_posix()
    save(root / 'host-config.json', config)
    missing = []
    try:
        profile = artifacts(config, spec) | {'source_commit': revision, 'renderers': renderer_identity(spec)}
    except (OSError, ValueError, RuntimeError, importlib_metadata_error()) as exc:
        profile = {'source_commit': revision, 'executed': False, 'prerequisite': str(exc),
                   'renderers': renderer_identity(spec)}
        missing.append('Runtime identity precondition: ' + str(exc))
    save(root / 'runtime-profile.json', profile)
    bindings['runtime_profile'] = pointer(root, root / 'runtime-profile.json') | {'source_commit': revision}
    try:
        memory = memory_precondition(config)
    except (OSError, RuntimeError) as exc:
        memory = {'satisfied': False, 'prerequisite': 'Actual Podman VM memory/inventory unavailable: ' + str(exc)}
    save(root / 'vm-memory-preflight.json', memory)
    if not memory['satisfied']:
        missing.append(memory['prerequisite'])
    if not (HERE / 'score.py').is_file():
        missing.append('Final independent scorer must be integrated and frozen at the externally reviewed source commit before actual run')
    fixture = Fixtures(args.am_root, inputs, root / 'native-contexts')
    prepared = [prepare_size(fixture, inputs, size, root) for size in args.sizes]
    source_identity = {'bindings': bindings, 'sizes': [{'size': p['size'], 'schedule': p['schedule'],
                       'generations': p['generation_artifacts']} for p in prepared]}
    receipt = {'schema': 'am255/independent-freeze-preflight-v1', 'source_commit': revision,
               'input_sha256': sha(args.inputs), 'spec_sha256': sha(args.spec), 'io_sha256': bindings['io']['sha256'],
               'source_bindings': bindings, 'sizes': source_identity['sizes'], 'missing_prerequisites': missing,
               'source_fixture_identity_sha256': digest(compact(source_identity).encode()),
               'native_fixture_checks_executed': True, 'model_or_graph_started': False,
               'semantic_query_executed': False, 'oracles_or_scorer_map_opened': False,
               'qualified_for_actual_run': not missing, 'qualification_scope': SCOPE}
    save(root / 'freeze.json', receipt)
    return receipt, fixture, prepared, inputs, spec, config, profile


def importlib_metadata_error():
    from importlib.metadata import PackageNotFoundError
    return PackageNotFoundError


def verify_previous_freeze(path, current):
    previous = load(path)
    for field in ('source_commit', 'input_sha256', 'spec_sha256', 'io_sha256', 'source_fixture_identity_sha256'):
        if previous.get(field) != current[field]:
            raise ValueError('Retained independently reviewed freeze differs: ' + field)
    if previous.get('missing_prerequisites'):
        raise ValueError('Retained freeze had missing prerequisites; retain it and generate/review a new freeze')


def write_observation(root, prepared, value):
    directory = prepared['root'] / 'observations'
    directory.mkdir(exist_ok=True, mode=0o700)
    path = directory / (value['variant'] + '-' + str(value['case_ordinal']).zfill(3) + '.json')
    save(path, value)
    return {'case_ordinal': value['case_ordinal'], 'variant': value['variant'], **pointer(root, path)}


def run_size(root, prepared, fixtures, inputs, spec, config, profile, run_id):
    work = prepared['root']
    model_root, graph_root = work / 'model', work / 'graph'
    model_root.mkdir(mode=0o700)
    graph_root.mkdir(mode=0o700)
    model, graph = Model(model_root, profile, run_id), Graph(graph_root, config, spec, run_id)
    profile_sha = sha(root / 'runtime-profile.json')
    result = {'size': prepared['size'], 'canonical_sha256': {g['bank'].generation_id: g['bank'].canonical_sha256 for g in prepared['generations'].values()},
              'profile_sha256': profile_sha, 'observations': [], 'generation_artifacts': prepared['generation_artifacts'],
              'costs': {}, 'missing_prerequisites': []}
    observations, query_vectors, query_sends, document_sends = [], {}, {}, []
    started = time.monotonic()
    store = None
    try:
        no_active_services(config['engine'])
        graph.start()
        graph.freeze_plans(load(HERE / 'executor_templates.json'))
        model.start()
        store = VectorStore(work / 'vectors.f32', len(prepared['ordered']))
        for start in range(0, len(prepared['ordered']), 8):
            batch = prepared['ordered'][start:start + 8]
            # Recheck canonical source eligibility/security before each batch; no
            # scoring canary names participate in selection or provider filtering.
            for row in batch:
                generation = next(g for g in prepared['generations'].values() if g['bank'].generation_id == row['first_generation'])
                source = generation['bank'].sources[row['first_path']]
                if not fixtures.provider_safe(generation['bank'], source['path']):
                    raise ValueError('Canonical provider gate changed before document send')
            texts = [prepared['texts'][r['rendered_sha256']].decode('utf-8') for r in batch]
            values, request_path, costs = model.embed(texts, 'documents', sorted({k for r in batch for k in r['source_keys']}))
            document_sends.append(pointer(root, request_path))
            for index, vector in enumerate(values, start):
                store.put(index, vector)
        store_hash = store.finish()
        save(work / 'vector-coverage.json', {'schema': 'am255/persisted-float32-coverage-v1', 'dtype': '<f4',
             'dimension': 4096, 'rows': prepared['ordered'], 'vector_store_sha256': store_hash,
             'generations': {g['bank'].generation_id: {'canonical_sha256': g['bank'].canonical_sha256,
                'sources': [{'key': key, 'row': index,
                             'rendered_sha256': g['coverage'][key]} for key, index in g['vector_rows'].items()]}
                for g in prepared['generations'].values()},
             'source_profile_sha256': profile_sha, 'full_coverage': True})
        for artifact in result['generation_artifacts']:
            artifact['vector_store'] = pointer(root, work / 'vectors.f32')
            artifact['coverage'] = pointer(root, work / 'vector-coverage.json')
        templates = load(HERE / 'executor_templates.json')
        request_ordinal = 0
        for variant in ('base', 'overlay'):
            for case in inputs['cases']:
                if variant == 'base' and case['partition'] != 'base':
                    continue
                request_ordinal += 1
                generation = prepared['generations'][(variant, selected_mutation(case))]
                bank = generation['bank']
                native, context = fixtures.qualify(bank, case)
                native['source_binding']['profile_sha256'] = profile_sha
                state_prerequisite = mechanism_prerequisite(case)
                prerequisite = state_prerequisite
                if not native['request_valid']:
                    prerequisite = native.get('refusal', native.get('prerequisite', 'Native request/context precondition failed'))
                unavailable = {'executed': False, 'prerequisite': prerequisite or 'This case requests a different mechanism'}
                semantic = unavailable | {'exact_ranking': [], 'ann': {'executed': False, 'prerequisite': prerequisite or 'No semantic mechanism requested'}}
                typed = unavailable | {'edges': [], 'primary_candidates': [], 'route_pointers': [], 'costs': {}}
                provider = {'executed': state_prerequisite is None,
                            'prerequisite': state_prerequisite,
                            'document_sends': document_sends, 'query_send': None,
                            'documents_sent': len(prepared['ordered']), 'queries_sent': 0,
                            'task_evidence': {'executed': False, 'prerequisite': 'Finalized actual owned-server task logs are attached after process stop'}}
                costs = {}
                if prerequisite is None:
                    if graph.active != bank.generation_id:
                        graph.projected = generation['projection']
                        graph.activate(graph.projected, generation['vector_rows'], store)
                    graph.context = context
                    if case['operation'].startswith('template:'):
                        typed = graph.typed(fixtures, bank, case, native, templates)
                    if 'ranking' in case['request']:
                        text = render_query(case['request']['query'])
                        h = digest(text)
                        fresh = h not in query_vectors
                        if fresh:
                            expected = prepared['query_schedule'][len(query_vectors)]
                            if expected != {'variant': variant, 'case_ordinal': case['case_ordinal'], 'rendered_sha256': h}:
                                raise ValueError('Actual first-approved query occurrence differs from frozen ordered schedule')
                            values, path, costs = model.embed([text.decode('utf-8')], 'query', [], case['case_ordinal'])
                            persisted = store.np.asarray(values[0], dtype='<f4')
                            query_path = work / ('query-' + h + '.f32')
                            with query_path.open('xb') as stream:
                                stream.write(persisted.tobytes())
                                stream.flush()
                                os.fsync(stream.fileno())
                            query_vectors[h] = persisted.tolist()
                            query_sends[h] = pointer(root, path)
                        domain = [(bank.sources[row['path']], generation['vector_rows'][row['key']]) for row in native['eligible']]
                        began = time.monotonic()
                        exact = store.rank(domain, query_vectors[h])
                        exact_seconds = time.monotonic() - began
                        ann = graph.ann({row['key'] for row in native['eligible']}, query_vectors[h], exact, request_ordinal)
                        semantic = {'executed': True, 'exact_ranking': exact, 'ann': ann, 'query_sha256': h,
                                    'vector_sha256': digest(store.np.asarray(query_vectors[h], dtype='<f4').tobytes()),
                                    'query_vector': pointer(root, work / ('query-' + h + '.f32')),
                                    'costs': {'exact_cosine_seconds': exact_seconds, 'complete_domain_size': len(domain)}}
                        provider.update(query_send=query_sends[h], query_sha256=h, rendered_sha256=h,
                                        queries_sent=1 if fresh else 0, query_vector_reused=not fresh)
                    else:
                        native['literal'] = fixtures.literal(bank, case, context)
                observation = {'schema': 'am255/independent-case-observation-v1', 'case_ordinal': case['case_ordinal'],
                    'size': prepared['size'], 'variant': variant, 'operation': case['operation'],
                    # Native request validation and provider accounting ran even
                    # when semantic/typed mechanisms were refused or unavailable.
                    'executed': isinstance(native.get('request_valid'), bool) or provider['executed'],
                    'prerequisites': [prerequisite] if prerequisite else [],
                    'native': native, 'semantic': semantic, 'typed': typed, 'outcome': {'executed': False, 'prerequisite': MAPPER},
                    'caveats': {'executed': False, 'prerequisite': MAPPER}, 'provider': provider, 'costs': costs}
                observations.append(observation)
                # Preserve every completed case even if a later request/process fails.
                write_observation(root, prepared, observation)
                if prerequisite:
                    result['missing_prerequisites'].append({'case_ordinal': case['case_ordinal'], 'variant': variant, 'prerequisite': prerequisite})
        if len(query_vectors) != len(prepared['query_schedule']):
            raise ValueError('Actual query coverage differs from complete frozen schedule')
    finally:
        stopped, stop_error = None, None
        try:
            stopped = model.stop()
        except BaseException as exc:
            stop_error = {'type': type(exc).__name__, 'reason': str(exc)}
            save(model_root / 'finalization-failure.json', stop_error)
        try:
            actual_graph_metrics = graph.metrics()
        except Exception as exc:
            actual_graph_metrics = {'executed': False, 'prerequisite': 'Actual graph statistics unavailable: ' + str(exc)}
        cleanup = graph.cleanup()
        result['cleanup'] = {'model': stopped, 'model_finalization_error': stop_error, 'graph': cleanup}
        result['costs'] = {'elapsed_seconds': time.monotonic() - started,
                           'shared_host': True, 'clean_performance_acceptance': False,
                           'actual_graph_stats': actual_graph_metrics,
                           'disk_bytes': sum(p.stat().st_size for p in work.rglob('*') if p.is_file() and not p.is_symlink())}
        task_path = model_root / 'cache-evidence.json'
        for observation in observations:
            if task_path.is_file() and stop_error is None:
                observation['provider']['task_evidence'] = {'executed': True, **pointer(root, task_path)}
            else:
                observation['provider']['task_evidence'] = {'executed': False, 'prerequisite': 'Complete actual finalized task/cache parser evidence unavailable; retain launch/failure logs'}
            result['observations'].append(write_observation(root, prepared, observation))
        save(work / 'size-manifest.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--am-root', type=Path, required=True)
    parser.add_argument('--inputs', type=Path, required=True)
    parser.add_argument('--spec', type=Path, required=True)
    parser.add_argument('--host-config', type=Path, required=True)
    parser.add_argument('--sizes', type=int, nargs='+', default=[1000, 10000, 50000])
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--freeze-only', action='store_true')
    parser.add_argument('--freeze', type=Path)
    parser.add_argument('--reviewed-source-commit')
    parser.add_argument('--reserved-host-slot', action='store_true')
    args = parser.parse_args()
    if args.sizes != sorted(set(args.sizes)) or any(size not in (1000, 10000, 50000) for size in args.sizes):
        parser.error('Sizes must be an ordered unique subset of1000,10000,50000; default is all three')
    if not args.freeze_only and (not args.freeze or not args.reviewed_source_commit or not args.reserved_host_slot):
        parser.error('Actual run requires retained --freeze, external --reviewed-source-commit, and --reserved-host-slot')
    for name in ('am_root', 'inputs', 'spec', 'host_config'):
        original = getattr(args, name)
        if original.is_symlink():
            parser.error('Symlinked input path refused: ' + name)
        setattr(args, name, original.resolve(strict=True))
    root = fresh_root(args.output)
    run_id = uuid.uuid4().hex
    save(root / 'owner.json', {'schema': 'am255/independent-owner-v1', 'root': str(root), 'uid': os.getuid(),
                              'run_id': run_id, 'production': False, 'resources': []})
    manifest = {'schema': 'am255/independent-raw-manifest-v1', 'input_sha256': sha(args.inputs),
                'spec_sha256': sha(args.spec), 'io_sha256': sha(HERE / 'execution-io.json'),
                'source_bindings': {}, 'sizes': [], 'cleanup': {'status': 'no-services-started'}, 'qualification_scope': SCOPE}
    def interrupted(signum, frame):
        raise KeyboardInterrupt('Controller terminated; owned finally cleanup required')
    signal.signal(signal.SIGTERM, interrupted)
    try:
        receipt, fixtures, prepared, inputs, spec, config, profile = freeze(args, root)
        manifest['source_bindings'] = receipt['source_bindings']
        if args.freeze_only:
            print(compact({'receipt': str(root / 'freeze.json'), 'sha256': sha(root / 'freeze.json'),
                           'model_or_graph_started': False, 'sizes': args.sizes,
                           'missing_prerequisites': receipt['missing_prerequisites']}))
            return 0
        verify_previous_freeze(args.freeze.resolve(strict=True), receipt)
        if receipt['missing_prerequisites']:
            raise ValueError('Actual run preconditions missing: ' + compact(receipt['missing_prerequisites']))
        # The explicit parent-reserved host slot is the same operational boundary
        # used by runtime-controls. Each size also refuses live model processes
        # and other independent owned graphs; no external service is adopted.
        for item in prepared:
            try:
                result = run_size(root, item, fixtures, inputs, spec, config, profile, run_id)
                manifest['sizes'].append(result)
                manifest['cleanup'] = {'status': 'disposed' if all(
                    s['cleanup']['graph']['status'] == 'disposed' and s['cleanup']['model']
                    and not s['cleanup']['model_finalization_error'] for s in manifest['sizes']) else 'erasure-pending',
                    'sizes': [s['cleanup'] for s in manifest['sizes']]}
                save(root / 'manifest.json', manifest)
            except BaseException:
                retained = item['root'] / 'size-manifest.json'
                if retained.is_file():
                    manifest['sizes'].append(load(retained))
                raise
        print(compact({'manifest': str(root / 'manifest.json'), 'sha256': sha(root / 'manifest.json'), 'qualification_scope': SCOPE}))
        return 0
    except BaseException as exc:
        save(root / 'failure.json', {'type': type(exc).__name__, 'reason': str(exc),
             'qualification_scope': SCOPE, 'retain_failed_attempt': True})
        manifest['cleanup'] = {'status': 'erasure-pending' if any(
            s.get('cleanup', {}).get('graph', {}).get('status') != 'disposed'
            or not s.get('cleanup', {}).get('model')
            or s.get('cleanup', {}).get('model_finalization_error') for s in manifest['sizes']) else 'no-unowned-disposal',
            'sizes': [s.get('cleanup') for s in manifest['sizes']]}
        raise
    finally:
        save(root / 'manifest.json', manifest)


if __name__ == '__main__':
    sys.exit(main())
