#!/usr/bin/env python3
"""Score hash-bound independent #255 observations offline; never execute retrieval.

The suite is an oracle, not an input to execute.py. This no-cutoff scorer reports
raw mathematical recall and exercised recorded relations only. Outcome, caveat,
public disclosure, literal/status mapping and abstention remain unexecuted. Its
source and every renderer/template must be frozen/reviewed before real outputs.
"""
from __future__ import annotations

import argparse
from collections import Counter
import fnmatch
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import runpy
import subprocess
import stat
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
STARTUP = REPO / 'skills/agent-memory/scripts/source_startup.py'
_cache_guard = runpy.run_path(str(STARTUP), run_name='am_source_startup')['isolate']()

SUITE_SHA256 = '36b663df90a5002ef93996ccc76e0a5de932f984d4a35178fe32b6f74e83b21f'
LABELS_SHA256 = '39b78cd401b4170cc0dcaa35df18dd914477bd9b4a40532d079fd4024ac5c43b'
INPUT_SHA256 = 'fc22d917544e865b9acf2ecc63ff410ccd8d3ced18e810c7755ffb62fa8892f1'
MAP_SHA256 = 'b4d7bfde73ca473c90d1a75239b0daf39e10942887465a02f74484aa09cc3f37'
AM_COMMIT = '175d21fa26bbef41f8ec859b463876934e16db79'
NATIVE_SHA256 = '9bd89d0e341ee8984db9e5082936807a49f1368a05a40ce026193f7b34e88ef3'
SIZES = (1000, 10000, 50000)
VARIANTS = ('base', 'overlay')
CLASSES = ('native', 'semantic', 'typed', 'provider_canaries', 'outcome', 'caveats',
           'public_disclosure', 'literal_status', 'abstention')
RELATIONS = {'CORRECTS': 'corrects', 'SUPERSEDES': 'supersedes',
             'SUPERSEDED_BY': 'superseded-by', 'DERIVED_FROM': 'derived-from',
             'SUPPORTED_BY': 'evidence', 'IN_SCOPE': 'scope',
             'HAS_CONDITION': 'condition', 'CO_ROUTED': 'co-routed',
             'ATTRIBUTED_TO': 'origin', 'SIMILAR_SUGGESTION': 'inferred-similarity'}
LIMITATIONS = [
    'Raw scratch research only; no overall G1, production feature, profile, legal or operator acceptance.',
    'Base/overlay counterparts are controls, not additional independent labels.',
    'No semantic cutoff is derived or applied; abstention is unexecuted.',
    'No semantic-result outcome/caveat/public disclosure or literal/status mapper is implemented.',
    'Provider/canonical metadata are private scientific evidence, not a production diagnostic surface.',
    'ANN uses independent budget 16; raw engine similarity is never accepted as signed cosine.',
    'Quality/resource thresholds are proposed, not operator-frozen acceptance.',
    'Earlier mechanism controls fill none of these suite cells; no synthesized answers or coding benefit.',
]


class IntegrityError(ValueError):
    """A raw artifact cannot be attributed to the agreed frozen experiment."""


def require(condition, message):
    if not condition:
        raise IntegrityError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False, allow_nan=False) + '\n').encode()


def compact(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False, allow_nan=False)


def decode(raw, where):
    def pairs(items):
        value = {}
        for key, item in items:
            require(key not in value, where + ': duplicate JSON member ' + key)
            value[key] = item
        return value

    def invalid(value):
        raise IntegrityError(where + ': non-finite JSON number ' + value)

    try:
        return json.loads(raw, object_pairs_hook=pairs, parse_constant=invalid)
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise IntegrityError(where + ': invalid UTF-8 JSON') from exc


def integer(value):
    return isinstance(value, int) and not isinstance(value, bool)


def digest(value, where):
    require(isinstance(value, str) and len(value) == 64 and
            all(c in '0123456789abcdef' for c in value), where + ': invalid SHA256')
    return value


def relative(value):
    require(isinstance(value, str) and value and '\\' not in value and '\x00' not in value,
            'Invalid relative artifact path')
    path = PurePosixPath(value)
    require(not path.is_absolute() and all(p not in ('', '.', '..') for p in value.split('/')),
            'Traversal/noncanonical artifact path: ' + value)
    return path


class RawRoot:
    """Open every path component without following symlinks, including root parents."""

    def __init__(self, root):
        absolute = Path(os.path.abspath(root))
        fd = os.open(absolute.anchor, os.O_RDONLY | os.O_DIRECTORY)
        try:
            for part in absolute.parts[1:]:
                next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                os.close(fd)
                fd = next_fd
        except OSError as exc:
            os.close(fd)
            raise IntegrityError('Raw root traverses a symlink or missing directory') from exc
        self.fd = fd
        self.root = absolute
        self.hashes = {}

    def close(self):
        os.close(self.fd)

    def open(self, name):
        path = relative(name)
        fd = os.dup(self.fd)
        try:
            for part in path.parts[:-1]:
                next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                os.close(fd)
                fd = next_fd
            result = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
            try:
                require(stat.S_ISREG(os.fstat(result).st_mode), 'Artifact is not a regular file: ' + name)
                return os.fdopen(result, 'rb')
            except BaseException:
                os.close(result)
                raise
        except OSError as exc:
            raise IntegrityError('Missing/symlink/nonregular artifact: ' + name) from exc
        finally:
            os.close(fd)

    def verify(self, pointer):
        require(isinstance(pointer, dict) and 'path' in pointer and 'sha256' in pointer,
                'Expected hash-bound artifact pointer')
        name = relative(pointer['path']).as_posix()
        expected = digest(pointer['sha256'], name)
        require(name not in self.hashes or self.hashes[name] == expected, 'Conflicting artifact hashes: ' + name)
        value = hashlib.sha256()
        with self.open(name) as stream:
            before = os.fstat(stream.fileno())
            for block in iter(lambda: stream.read(1024 * 1024), b''):
                value.update(block)
            after = os.fstat(stream.fileno())
        require((before.st_ino, before.st_size, before.st_mtime_ns) ==
                (after.st_ino, after.st_size, after.st_mtime_ns), 'Artifact changed while hashing: ' + name)
        require(value.hexdigest() == expected, 'Artifact SHA256 mismatch: ' + name)
        self.hashes[name] = expected
        return name

    def read(self, name):
        require(name in self.hashes, 'Read before hash binding: ' + name)
        with self.open(name) as stream:
            raw = stream.read()
        require(sha(raw) == self.hashes[name], 'Artifact changed after binding: ' + name)
        return raw

    def json(self, pointer):
        name = self.verify(pointer)
        return decode(self.read(name), name)

    def pointers(self, value):
        if isinstance(value, dict):
            if 'path' in value and 'sha256' in value:
                self.verify(value)
            for child in value.values():
                self.pointers(child)
        elif isinstance(value, list):
            for child in value:
                self.pointers(child)


def trusted_bytes(path, expected=None):
    # Trusted CLI sources are files, not raw-root pointers, but symlinks are not accepted.
    path = Path(path)
    for node in (path, *path.parents):
        require(not node.is_symlink(), 'Trusted input is a symlink: ' + str(path))
    raw = path.read_bytes()
    if expected is not None:
        require(sha(raw) == expected, 'Frozen input identity mismatch: ' + str(path))
    return raw


def git_source(repository_path, commit, compare_local=True):
    path = relative(repository_path).as_posix()
    result = subprocess.run(['git', '-C', str(REPO), 'cat-file', 'blob', commit + ':' + path],
                            capture_output=True, check=False)
    require(result.returncode == 0, 'Source absent from trusted freeze commit: ' + path)
    if compare_local:
        require(sha(trusted_bytes(REPO / path)) == sha(result.stdout),
                'Current trusted source differs from freeze Git blob: ' + path)
    return result.stdout


def source_identity(store, manifest, expected_source_commit):
    bindings = manifest['source_bindings']
    require(isinstance(bindings, dict), 'source_bindings must be an object')
    require(bindings.get('authoring_am') == {'commit': AM_COMMIT, 'native_tree_sha256': NATIVE_SHA256},
            'Wrong authoring AM source identity')
    require(isinstance(expected_source_commit, str) and len(expected_source_commit) == 40 and
            all(c in '0123456789abcdef' for c in expected_source_commit),
            'Expected source commit must be an externally supplied full Git SHA')
    kind = subprocess.run(['git', '-C', str(REPO), 'cat-file', '-t', expected_source_commit],
                          capture_output=True, check=False)
    require(kind.returncode == 0 and kind.stdout.strip() == b'commit', 'Expected source identity is not a Git commit')
    prefix = HERE.relative_to(REPO).as_posix() + '/'
    for key, expected in (('inputs', INPUT_SHA256), ('spec', sha(trusted_bytes(HERE / 'spec.json'))),
                          ('io', sha(trusted_bytes(HERE / 'execution-io.json')))):
        pointer = bindings.get(key)
        store.verify(pointer)
        require(pointer['sha256'] == expected, 'Wrong source binding: ' + key)
        require(manifest.get({'inputs': 'input_sha256', 'spec': 'spec_sha256', 'io': 'io_sha256'}[key]) == expected,
                'Manifest source hash mismatch: ' + key)
        expected_repository_path = prefix + {'inputs': 'authored-inputs-20261106.json',
                                             'spec': 'spec.json', 'io': 'execution-io.json'}[key]
        require(pointer.get('repository_path', expected_repository_path if key == 'inputs' else None) == expected_repository_path,
                'Wrong repository path for source binding: ' + key)
        require(sha(git_source(expected_repository_path, expected_source_commit)) == expected,
                'Input/spec/IO bytes differ from trusted freeze commit: ' + key)
    inputs = store.json(bindings['inputs'])
    require(inputs.get('authoring_am_commit') == AM_COMMIT and
            inputs.get('authoring_native_tree_sha256') == NATIVE_SHA256 and
            inputs.get('suite_sha256') == SUITE_SHA256 and inputs.get('spec_sha256') == manifest['spec_sha256'],
            'Compiled source/spec/suite binding mismatch')
    require(inputs.get('compiler_sha256') == sha(trusted_bytes(HERE / 'compile_inputs.py')) and
            inputs.get('source_startup_sha256') == sha(trusted_bytes(STARTUP)),
            'Compiler/source-only guard identity mismatch')
    require(isinstance(bindings.get('helpers'), list), 'Missing frozen helper source list')
    code = [bindings.get('recipe'), *bindings['helpers']]
    require(isinstance(bindings.get('recipe'), dict) and
            bindings['recipe'].get('source_commit', expected_source_commit) == expected_source_commit,
            'Recipe source must bind the trusted reviewed freeze, not the separate native authoring commit')
    require(bindings['recipe'].get('repository_path') == prefix + 'execute.py',
            'Recipe must bind the reviewed executor source execute.py')
    seen = set()
    for pointer in code:
        store.verify(pointer)
        source_name = pointer.get('repository_path')
        relative(source_name)
        commit = pointer.get('source_commit', expected_source_commit)
        require(commit in (expected_source_commit, AM_COMMIT), 'Foreign helper source commit')
        identity = (commit, source_name)
        require(identity not in seen, 'Duplicate frozen recipe/helper source: ' + source_name)
        require(sha(git_source(source_name, commit, compare_local=commit == expected_source_commit)) == pointer['sha256'],
                'Wrong frozen recipe/helper source: ' + source_name)
        seen.add(identity)
    required_sources = {prefix + name for name in (
        'execute.py', 'score.py', 'compile_inputs.py', 'executor_native.py',
        'executor_runtime.py', 'executor_graph.py', 'executor_render.py', 'executor_templates.json')}
    required_sources.update((HERE.parent / name).relative_to(REPO).as_posix() for name in (
        'independent-suite/validate_suite.py', 'independent-suite/g1_relationship_overlay.py',
        'runtime-controls/common.py', 'serving-comparison/compare_serving.py'))
    required_sources.add(STARTUP.relative_to(REPO).as_posix())
    require({(expected_source_commit, name) for name in required_sources} <= seen,
            'Required executor/scorer/helper source absent from frozen source set')
    require(bindings['recipe'].get('commit') == expected_source_commit,
            'Recipe commit differs from externally trusted expected source commit')
    profile = store.json(bindings.get('runtime_profile'))
    require(isinstance(profile, dict), 'Runtime profile must be an object')
    require(profile.get('source_commit') == expected_source_commit and
            bindings['runtime_profile'].get('source_commit') == expected_source_commit,
            'Runtime profile source_commit differs from externally trusted source freeze')
    store.pointers(profile)
    return inputs, profile


def bind_raw(store, manifest_path, expected_source_commit):
    manifest_name = relative(manifest_path).as_posix()
    with store.open(manifest_name) as stream:
        manifest_raw = stream.read()
    store.hashes[manifest_name] = sha(manifest_raw)
    manifest = decode(manifest_raw, manifest_name)
    require(isinstance(manifest, dict) and manifest.get('schema') in (
        'am255/independent-raw-manifest-v1', 'am255/independent-raw-manifest-v2'),
            'Unknown raw manifest schema')
    for key in ('input_sha256', 'spec_sha256', 'io_sha256', 'source_bindings', 'sizes', 'cleanup', 'qualification_scope'):
        require(key in manifest, 'Missing manifest field: ' + key)
    inputs, profile = source_identity(store, manifest, expected_source_commit)
    require(isinstance(manifest['sizes'], list), 'Manifest sizes must be a list')
    compiled = {c['case_ordinal']: c for c in inputs['cases']}
    require(len(compiled) == 101 and set(compiled) == set(range(1, 102)), 'Compiled case ordinals differ')
    observations, size_rows, generations = {}, {}, {}
    paths = set()
    for row in manifest['sizes']:
        require(isinstance(row, dict), 'Manifest size row must be an object')
        size = row.get('size')
        require(integer(size) and size in SIZES and size not in size_rows, 'Duplicate/foreign size')
        size_rows[size] = row
        digest(row.get('profile_sha256'), 'size profile')
        require(row['profile_sha256'] == manifest['source_bindings']['runtime_profile']['sha256'],
                'Size profile does not match bound runtime profile')
        canonical = row.get('canonical_sha256')
        require(isinstance(canonical, dict), 'Size canonical_sha256 must map generation IDs to digests')
        for generation in row.get('generation_artifacts', []):
            gid = generation.get('generation_id')
            require(isinstance(gid, str) and gid and (size, gid) not in generations,
                    'Duplicate/missing generation ID')
            require(generation.get('variant') in VARIANTS, 'Unknown generation variant')
            require(canonical.get(gid) == digest(generation.get('canonical_sha256'), 'canonical generation'),
                    'Generation canonical binding mismatch')
            store.verify(generation.get('catalog'))
            if generation.get('vector_store') is not None:
                store.verify(generation['vector_store'])
            generations[size, gid] = generation
        require(set(canonical) == {gid for n, gid in generations if n == size}, 'Unbound canonical generation')
        if manifest['schema'] == 'am255/independent-raw-manifest-v2':
            bind_delivery(store, row)
        require(isinstance(row.get('observations'), list), 'Missing observation pointer list')
        for pointer in row['observations']:
            ordinal, variant = pointer.get('case_ordinal'), pointer.get('variant')
            require(integer(ordinal) and ordinal in compiled and variant in VARIANTS,
                    'Foreign case ordinal/variant')
            require(variant != 'base' or compiled[ordinal]['partition'] == 'base', 'Overlay-only ordinal in base')
            identity = (size, variant, ordinal)
            require(identity not in observations, 'Duplicate observation ordinal identity')
            name = store.verify(pointer)
            require(name not in paths, 'Duplicate observation path')
            paths.add(name)
            observations[identity] = pointer
    # Bind every observation and all nested request/receipt pointers before reading
    # any oracle field. Hash binding is not an assertion that a model ran honestly.
    for identity, pointer in observations.items():
        observation = store.json(pointer)
        require(isinstance(observation, dict), 'Observation must be an object')
        version = 'v2' if manifest['schema'].endswith('-v2') else 'v1'
        require(observation.get('schema') == 'am255/independent-case-observation-' + version,
                'Observation/manifest accounting schema mismatch')
        provider = observation.get('provider', {})
        require(provider.get('schema') == ('am255/generation-provider-accounting-v2' if version == 'v2' else None),
                'Provider accounting version mismatch')
        require((observation.get('size'), observation.get('variant'), observation.get('case_ordinal')) == identity,
                'Observation source/size/variant/ordinal identity mismatch')
        require(integer(observation.get('size')) and integer(observation.get('case_ordinal')),
                'Observation size/ordinal must be integers, not booleans')
        require(observation.get('operation') == compiled[identity[2]]['operation'], 'Observation operation mismatch')
        require(isinstance(observation.get('executed'), bool), 'Observation executed must be boolean')
        for key in ('prerequisites', 'native', 'semantic', 'typed', 'outcome', 'caveats', 'provider', 'costs'):
            require(key in observation, 'Missing observation field: ' + key)
        for key in ('native', 'semantic', 'typed', 'outcome', 'caveats', 'provider', 'costs'):
            require(isinstance(observation[key], dict), 'Observation section must be an object: ' + key)
        for key in ('semantic', 'typed', 'outcome', 'caveats'):
            require(isinstance(observation[key].get('executed'), bool), 'Section executed must be boolean: ' + key)
        store.pointers(observation)
        if version == 'v2':
            validate_provider_delivery(observation, store)
    store.pointers(manifest)
    return manifest, inputs, profile, observations, size_rows, generations


class Oracle:
    def __init__(self, suite):
        self.suite = suite
        self.pid = suite['fixture']['project_id']
        self.records = suite['records']
        self.by_key = {}
        for ref, rec in self.records.items():
            self.by_key[self.resolve(ref)] = rec

    def resolve(self, ref):
        if ref in self.records:
            rec = self.records[ref]
            return 'record:%s:%s:%d' % (self.pid, rec['id'], rec['version'])
        if ref.startswith('missing:'):
            label, version = ref[8:].rsplit('@', 1)
            original = self.records['rec:' + label]
            return 'record:%s:%s:%d' % (self.pid, original['id'], int(version))
        for prefix, tag in (('ctx:', 'context'), ('ev:', 'evidence'), ('scope:', 'scope')):
            if ref.startswith(prefix):
                return tag + ':' + self.pid + ':' + ref[len(prefix):]
        for prefix, tag in (('cond:', 'condition'), ('origin:', 'origin')):
            if ref.startswith(prefix):
                field, value = ref[len(prefix):].split('=', 1)
                if prefix == 'cond:' and value == 'unknown':
                    value = None
                return tag + ':' + self.pid + ':' + compact([field, value])
        require(False, 'Unknown frozen canonical reference: ' + ref)


    def forbidden_global(self):
        value = self.suite['global_invariants']
        return {self.resolve(ref) for name in ('native_security_flagged', 'hidden_visibility')
                for variant in VARIANTS for ref in value[name][variant]}

    def forbidden(self, case, catalog):
        result = set()
        for item in case['primary']['forbidden']:
            if 'ref' in item:
                result.add(self.resolve(item['ref']))
            else:
                exceptions = {self.resolve(ref) for ref in item['except']}
                for key, source in catalog.items():
                    if not key.startswith('record:'):
                        continue
                    label = source.get('fixture_label', self.by_key.get(key, {}).get('label'))
                    require(isinstance(label, str), 'Wildcard safety requires actual canonical fixture_label metadata')
                    if key not in exceptions and fnmatch.fnmatchcase(label, item['glob']):
                        result.add(key)
        if case['partition'] == 'base':
            result.update(key for key, source in catalog.items()
                          if source.get('fixture_label', '').startswith('g1-'))
        return result


class Catalog(dict):
    def __init__(self):
        super().__init__()
        self.routes = {}
        self.generation = {}


def catalog_for(store, generation, oracle):
    value = store.json(generation['catalog'])
    require(value.get('canonical_sha256') == generation['canonical_sha256'], 'Catalog canonical binding mismatch')
    entries = value.get('sources', value.get('entries'))
    require(isinstance(entries, list), 'Canonical catalog lacks source entries')
    catalog, paths = Catalog(), set()
    catalog.generation = generation
    mutation = generation.get('mutation')
    require(mutation is None or mutation in oracle.suite['mutations'], 'Unknown canonical mutation generation')
    changed_key = oracle.resolve(oracle.suite['mutations'][mutation]['target']) if mutation else None
    for entry in entries:
        key, path = entry.get('key'), entry.get('path')
        require(isinstance(key, str) and key and key not in catalog, 'Duplicate/missing catalog source key')
        relative(path)
        require(path not in paths, 'Duplicate canonical source path')
        paths.add(path)
        digest(entry.get('file_sha256'), 'catalog source')
        if key.startswith('record:'):
            require(integer(entry.get('version')) and entry.get('id') and
                    key == 'record:%s:%s:%d' % (oracle.pid, entry['id'], entry['version']),
                    'Catalog record source identity mismatch')
        elif key.startswith('context:'):
            require(key == 'context:' + oracle.pid + ':' + path, 'Catalog context source identity mismatch')
        else:
            require(False, 'Catalog source must be canonical record/context, not a fabricated relation node')
        if key in oracle.by_key:
            require(path == oracle.by_key[key]['semantic_navigation_path'], 'Frozen record ID/version/path mismatch')
            known = oracle.by_key[key]
            if key != changed_key and 'file_sha256' in known:
                require(entry['file_sha256'] == known['file_sha256'], 'Wrong frozen canonical source hash')
            if 'fixture_label' in entry:
                require(entry['fixture_label'] == known['label'], 'Wrong source-native fixture label')
            if 'partition' in known:
                require(generation['variant'] == 'overlay' or known['partition'] == 'base',
                        'Overlay source in a base generation catalog')
            metadata = entry.get('metadata')
            if isinstance(metadata, dict):
                for field in ('id', 'version', 'scope', 'conditions', 'evidence', 'kind', 'visibility', 'status'):
                    if field in known and not (key == changed_key and field == 'status'):
                        require(metadata.get(field) == known[field], 'Canonical metadata differs from frozen source: ' + field)
        catalog[key] = entry
    for route in value.get('routes', []):
        require(isinstance(route, dict), 'Malformed canonical route')
        for name in ('index_path', 'target_path'):
            relative(route.get(name))
        require(integer(route.get('ordinal')) and route['ordinal'] >= 0, 'Invalid canonical route ordinal')
        key = 'route:%s:%s:%s:%d' % (oracle.pid, route['index_path'], route['target_path'], route['ordinal'])
        require(route.get('key') == key and key not in catalog.routes, 'Duplicate/foreign canonical route key')
        catalog.routes[key] = route
    return catalog


def metric(executed=False, prerequisite=None, failures=None, **values):
    return {'executed': executed, 'prerequisites': [] if executed else [prerequisite or 'No actual output surface recorded'],
            'failed_predicates': failures or [], **values}


def unexecuted(observation, section, default):
    value = observation.get(section, {})
    prerequisite = value.get('prerequisite') if isinstance(value, dict) else None
    return metric(prerequisite=prerequisite or default)


def eligible_domain(native, catalog):
    eligible = native.get('eligible')
    require(isinstance(eligible, list), 'Missing actual native eligible domain')
    keys, paths = set(), set()
    for row in eligible:
        key = row.get('key')
        require(key in catalog and key not in keys, 'Duplicate/foreign native eligible source key')
        require(row.get('path') == catalog[key]['path'] and row['path'] not in paths, 'Native source path mismatch')
        for field in ('id', 'version'):
            if field in catalog[key]:
                require(row.get(field) == catalog[key][field], 'Native record ID/version mismatch')
        keys.add(key)
        paths.add(row['path'])
    return keys


def full_ranking(observation, catalog):
    keys = eligible_domain(observation['native'], catalog)
    semantic = observation['semantic']
    ranking = semantic.get('exact_ranking')
    require(isinstance(ranking, list), 'Missing full signed-cosine exact ranking')
    seen = set()
    for row in ranking:
        key, cosine = row.get('key'), row.get('cosine')
        require(key in keys and key not in seen, 'Duplicate/foreign exact ranking key')
        require(row.get('path') == catalog[key]['path'], 'Exact ranking source path mismatch')
        for field in ('id', 'version'):
            if field in catalog[key]:
                require(row.get(field) == catalog[key][field], 'Exact ranking record ID/version mismatch')
        require(isinstance(cosine, (float, int)) and not isinstance(cosine, bool) and
                math.isfinite(cosine) and -1.0000001 <= cosine <= 1.0000001,
                'Exact ranking is not finite signed cosine')
        seen.add(key)
    require(seen == keys, 'Exact ranking does not cover the full freshly eligible domain')
    require(ranking == sorted(ranking, key=lambda row: (-row['cosine'], row['key'])),
            'Exact ranking order/tie-break differs from signed cosine')
    return ranking


def top_metrics(keys, required, tolerated, forbidden):
    selected = set(keys)
    hits = selected & required
    precision_denominator = len(selected - tolerated)
    return {'required_hits': len(hits), 'required_count': len(required),
            'required_recall': len(hits) / len(required) if required else None,
            'empty_required': not required, 'tolerated_count': len(selected & tolerated),
            'ordinary_extras': sorted(selected - required - tolerated - forbidden),
            'forbidden_selected': sorted(selected & forbidden),
            'precision_excluding_tolerated': len(hits) / precision_denominator if precision_denominator else None,
            'precision_denominator': precision_denominator}


def ann_metrics(ann, ranking, k, required, tolerated, forbidden):
    if not ann.get('executed', False):
        return metric(prerequisite=ann.get('prerequisite', 'No eligible-domain ANN run recorded'))
    n = len(ranking)
    require(ann.get('candidate_budget') == 16, 'ANN candidate budget differs from independent fixed 16')
    require(ann.get('domain_size') == n, 'ANN native eligible domain cardinality mismatch')
    capacity = ann.get('reported_capacity')
    require(integer(capacity) and capacity >= n, 'ANN reported capacity cannot cover eligible domain')
    raw = ann.get('raw_rows')
    rescored = ann.get('exact_rescored')
    require(isinstance(raw, list) and isinstance(rescored, list), 'Missing ANN raw/exact-rescored rows')
    exact = {row['key']: row['cosine'] for row in ranking}
    candidates = [row.get('key') for row in raw]
    require(len(candidates) == len(set(candidates)) and set(candidates) <= set(exact) and
            len(candidates) <= min(16, n), 'Duplicate/foreign/over-budget ANN candidates')
    for row in raw:
        for field in ('distance', 'similarity'):
            number = row.get(field)
            require(isinstance(number, (int, float)) and not isinstance(number, bool) and math.isfinite(number),
                    'Non-finite/missing raw ANN engine value')
    require([row.get('key') for row in rescored] == sorted(candidates, key=lambda key: (-exact[key], key)),
            'ANN exact-rescored keys/order mismatch')
    for row in rescored:
        value = row.get('cosine')
        require(isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and
                abs(value - exact[row['key']]) <= 1e-7, 'ANN rescoring differs from actual full signed cosine')
    exact_top = {row['key'] for row in ranking[:k]}
    actual_top = [row['key'] for row in rescored[:k]]
    result = top_metrics(actual_top, required, tolerated, forbidden)
    failures = []
    if result['forbidden_selected']:
        failures.append('ANN forbidden primary selected')
    if result['required_hits'] != len(required):
        failures.append('ANN required primary missing from exact-rescored top-K')
    if set(candidates) & exact_top != exact_top:
        failures.append('ANN candidates omit eligible exact top-K')
    return metric(True, failures=failures,
                  candidate_recall=len(set(candidates) & exact_top) / k if k else None,
                  exact_rescored_top_k_overlap=len(set(actual_top) & exact_top) / k if k else None,
                  candidate_count=len(candidates), candidate_budget=16, domain_size=n,
                  k_domain_class='k<N' if k < n else 'exhaustive-k',
                  candidate_domain_class='budget<N' if 16 < n else 'exhaustive-candidates',
                  raw_engine_scores_used_for_acceptance=False, **result)


def semantic_metrics(case, observation, catalog, oracle):
    semantic = observation['semantic']
    if not semantic.get('executed', False):
        result = unexecuted(observation, 'semantic', 'No actual full eligible signed-cosine ranking')
        result['ann'] = metric(prerequisite='Semantic full eligible baseline unexecuted')
        return result
    require(observation['executed'], 'Semantic output on an unexecuted observation')
    require(isinstance(observation['native'].get('request_valid'), bool), 'Missing native request validity')
    ranking = full_ranking(observation, catalog)
    required = {oracle.resolve(ref) for ref in case['primary']['required']}
    tolerated = {oracle.resolve(ref) for ref in case['primary']['tolerated']}
    forbidden = oracle.forbidden(case, catalog)
    k = min(len(ranking), max(3, len(required) + 1))
    scores = {row['key']: (rank, row['cosine']) for rank, row in enumerate(ranking, 1)}
    selected = [row['key'] for row in ranking[:k]]
    result = top_metrics(selected, required, tolerated, forbidden)
    failures = []
    if result['required_hits'] != len(required):
        failures.append('Required primary missing from scorer-only top-K')
    if result['forbidden_selected']:
        failures.append('Native-forbidden record ranked as current primary')
    if observation['native']['request_valid'] is not True:
        failures.append('Semantic ranking ran after native request refusal')
    if (observation['native'].get('crosscheck') or {}).get('matched') is not True:
        failures.append('Actual native eligible-domain crosscheck not matched')
    return metric(True, failures=failures, k=k, requested_k=max(3, len(required) + 1),
                  domain_size=len(ranking), k_domain_class='k<N' if k < len(ranking) else 'exhaustive-k',
                  selected=selected, required_ranks=[{'key': key, 'rank': scores.get(key, (None, None))[0],
                                                    'cosine': scores.get(key, (None, None))[1]}
                                                   for key in sorted(required)],
                  ann=ann_metrics(semantic.get('ann', {}), ranking, k, required, tolerated, forbidden), **result)


def relation_name(value):
    return RELATIONS.get(value, value)


def edge_matches(actual, expected, oracle):
    for field in ('from', 'to', 'asserted_by', 'via'):
        if field in expected and actual.get(field) != oracle.resolve(expected[field]):
            return False
    default_channels = ('recorded', 'navigation', 'provenance')
    channels = (expected['channel'],) if 'channel' in expected else default_channels
    return relation_name(actual.get('rel')) == expected['rel'] and actual.get('channel', 'recorded') in channels


def canonical_assertion(edge, catalog, oracle):
    source = catalog.get(edge.get('asserted_by'))
    if source is None or edge.get('source_sha256') != source['file_sha256']:
        return False
    rel = relation_name(edge.get('rel'))
    if edge.get('channel') == 'inferred':
        return (rel == 'inferred-similarity' and edge.get('generation_id') == catalog.generation['generation_id']
                and edge.get('from') in catalog and edge.get('to') in catalog)
    if edge.get('channel') in ('navigation', 'provenance'):
        start, end = catalog.get(edge.get('from')), catalog.get(edge.get('to'))
        if start is None:
            return False
        if rel == 'co-routed' and end is not None:
            left = {r['index_path'] for r in catalog.routes.values() if r['target_path'] == start['path']}
            right = {r['index_path'] for r in catalog.routes.values() if r['target_path'] == end['path']}
            return bool(left & right)
        if rel == 'origin':
            values = start.get('metadata', {}).get('origin', {})
            if end is None:
                return any(edge.get('to') == 'origin:' + oracle.pid + ':' + compact([key, value])
                           for key, value in values.items())
            other = end.get('metadata', {}).get('origin', {})
            return any(key in other and other[key] == value and
                       edge.get('via') == 'origin:' + oracle.pid + ':' + compact([key, value])
                       for key, value in values.items())
        return False
    if edge.get('channel') != 'recorded' or edge.get('from') != edge.get('asserted_by'):
        return False
    metadata = source.get('metadata')
    if not isinstance(metadata, dict):
        return None  # Missing provenance is an unexecuted predicate, not proof of a wrong relation.
    target = edge.get('to')
    if rel in ('corrects', 'supersedes', 'superseded-by', 'derived-from'):
        return any(target == 'record:%s:%s:%d' % (oracle.pid, item['id'], item['version'])
                   for item in metadata.get(rel, []))
    if rel == 'evidence':
        return any(target == 'evidence:' + oracle.pid + ':' + value for value in metadata.get('evidence', []))
    if rel == 'scope':
        return target == 'scope:' + oracle.pid + ':' + metadata.get('scope', '')
    if rel == 'condition':
        return any(target == 'condition:' + oracle.pid + ':' + compact([key, value])
                   for key, value in metadata.get('conditions', {}).items())
    if rel == 'origin':
        return any(target == 'origin:' + oracle.pid + ':' + compact([key, value])
                   for key, value in metadata.get('origin', {}).items())
    return False


def typed_metrics(case, observation, catalog, oracle):
    typed = observation['typed']
    if not typed.get('executed', False):
        return unexecuted(observation, 'typed', 'No actual reviewed typed template output')
    require(observation['executed'], 'Typed output on an unexecuted observation')
    edges = typed.get('edges')
    require(isinstance(edges, list), 'Typed edges must be an actual list')
    require(len({compact(edge) for edge in edges}) == len(edges), 'Duplicate typed output edge')
    failures, provenance_missing, disallowed, valid = [], [], [], 0
    globals_forbidden = oracle.forbidden_global()
    for edge in edges:
        require(all(key in edge for key in ('from', 'rel', 'to', 'asserted_by', 'source_sha256', 'channel')),
                'Missing typed edge identity/provenance/channel')
        digest(edge['source_sha256'], 'edge provenance')
        bad = [item for item in case['relationships']['disallowed'] if edge_matches(edge, item, oracle)]
        if bad:
            disallowed.extend(bad)
            failures.append('Frozen disallowed typed path returned')
        if {edge['from'], edge['to'], edge['asserted_by']} & globals_forbidden:
            failures.append('Hidden/security-unsafe endpoint exposed by typed output')
        assertion = canonical_assertion(edge, catalog, oracle)
        if assertion is False:
            failures.append('Typed relation channel/canonical assertion/provenance mismatch')
        elif assertion is None:
            provenance_missing.append(edge)
        elif not bad:
            valid += 1
        if edge['channel'] == 'inferred' and edge.get('to') not in {
                oracle.resolve(ref) for ref in case['relationships']['allowed_inferred']}:
            failures.append('Inferred suggestion not allowed in frozen context')
    required = case['relationships']['required']
    found = [item for item in required if any(edge_matches(edge, item, oracle) and
             canonical_assertion(edge, catalog, oracle) is True for edge in edges)]
    provenance_unexecuted_required = [item for item in required if item not in found and any(
        edge_matches(edge, item, oracle) and canonical_assertion(edge, catalog, oracle) is None
        for edge in edges)]
    missing = [item for item in required if item not in found and item not in provenance_unexecuted_required]
    executed_required = len(required) - len(provenance_unexecuted_required)
    if missing:
        failures.append('Required recorded typed path missing')
    candidates = typed.get('primary_candidates', [])
    require(isinstance(candidates, list) and all(isinstance(key, str) for key in candidates) and
            len(candidates) == len(set(candidates)), 'Duplicate/noncanonical typed primary candidates')
    eligible = {row['key'] for row in observation['native'].get('eligible', [])}
    if set(candidates) - eligible:
        failures.append('Typed current-primary candidate outside native eligible domain')
    primaries = top_metrics(candidates, {oracle.resolve(ref) for ref in case['primary']['required']},
                            {oracle.resolve(ref) for ref in case['primary']['tolerated']},
                            oracle.forbidden(case, catalog))
    if primaries['forbidden_selected']:
        failures.append('Typed forbidden/inactive-as-current primary returned')
    if primaries['required_hits'] != primaries['required_count']:
        failures.append('Typed required current primary missing')
    pointers = typed.get('route_pointers', [])
    require(isinstance(pointers, list) and all(isinstance(key, str) for key in pointers) and
            len(pointers) == len(set(pointers)), 'Duplicate/noncanonical typed route pointers')
    route_missing = []
    if set(pointers) - set(catalog.routes):
        failures.append('Typed route pointer absent from canonical route catalog')
    for item in case['relationships']['route_pointers']:
        index = item['ref'][6:]
        if item['tier'] == 'required' and not any(
                key in catalog.routes and catalog.routes[key]['index_path'] == 'MEMORY.md' and
                catalog.routes[key]['target_path'] == index for key in pointers):
            route_missing.append(item['ref'])
    if route_missing:
        failures.append('Required recorded route pointer missing')
    return metric(True, failures=sorted(set(failures)), required_paths=len(required), matched_required_paths=len(found),
                  executed_required_paths=executed_required,
                  required_path_recall=len(found) / executed_required if executed_required else None,
                  provenance_unexecuted_required=provenance_unexecuted_required,
                  missing_required_paths=missing, disallowed_paths=disallowed,
                  relation_label_accuracy=valid / len(edges) if edges and not provenance_missing else None,
                  actual_edges=len(edges), provenance_unexecuted=len(provenance_missing),
                  valid_canonical_edges=valid,
                  provenance_prerequisite='Canonical source metadata required' if provenance_missing else None,
                  missing_route_pointers=route_missing, primary=primaries)


def bind_delivery(store, size_row):
    """Validate all first sends and exact generation memberships before oracles."""
    ledger_pointer = size_row.get('delivery_ledger')
    ledger = store.json(ledger_pointer)
    require(ledger.get('schema') == 'am255/generation-delivery-ledger-v2', 'Unknown delivery ledger')
    profile = size_row['profile_sha256']
    require(ledger.get('profile_sha256') == profile, 'Delivery profile mismatch')
    require(ledger.get('erasure', {}).get('executed') is False, 'Delivery membership cannot assert erasure')
    coverage = store.json(ledger.get('coverage'))
    require(coverage.get('schema') == 'am255/persisted-float32-coverage-v2' and
            coverage.get('source_profile_sha256') == profile and coverage.get('full_coverage') is True,
            'Missing complete source/profile coverage')
    artifacts = {g['generation_id']: g for g in size_row['generation_artifacts']}
    require(set(coverage.get('generations', {})) == set(artifacts), 'Coverage generation set mismatch')
    catalogs, memberships = {}, {}
    for gid, generation in artifacts.items():
        catalog = store.json(generation['catalog'])
        require(catalog.get('generation_id') == gid and
                catalog.get('canonical_sha256') == generation['canonical_sha256'],
                'Delivery catalog generation mismatch')
        entries = catalog.get('sources', catalog.get('entries'))
        require(isinstance(entries, list), 'Delivery catalog lacks sources')
        catalogs[gid] = {entry['key']: entry for entry in entries}
        require(len(catalogs[gid]) == len(entries), 'Duplicate delivery catalog key')
        require(generation.get('coverage') == ledger['coverage'], 'Generation coverage pointer mismatch')
        require(generation.get('vector_store', {}).get('sha256') == coverage.get('vector_store_sha256'),
                'Coverage vector-store hash mismatch')
        declared = coverage['generations'][gid]
        require(declared.get('canonical_sha256') == generation['canonical_sha256'],
                'Coverage canonical hash mismatch')
        keys = set()
        for source in declared.get('sources', []):
            key, index = source.get('key'), source.get('row')
            require(key in catalogs[gid] and key not in keys and integer(index), 'Foreign/duplicate coverage source')
            keys.add(key)
            memberships[gid, key] = (index, source.get('rendered_sha256'))
        require(keys == set(catalog.get('provider_source_keys', [])),
                'Incomplete native provider projection coverage')
    rows = coverage.get('rows')
    require(isinstance(rows, list), 'Missing delivery rows')
    observed, hashes = set(), set()
    for index, row in enumerate(rows):
        h = digest(row.get('rendered_sha256'), 'delivery rendered row')
        require(h not in hashes, 'Duplicate rendered row under exact profile')
        hashes.add(h)
        first = row.get('first_generation')
        require(first in artifacts and isinstance(row.get('memberships'), list) and row['memberships'],
                'Missing first-generation membership')
        first_keys = []
        for member in row['memberships']:
            gid, key = member.get('generation_id'), member.get('key')
            identity = (gid, key)
            require(identity in memberships and identity not in observed, 'Absent/duplicate generation membership')
            require(memberships[identity] == (index, h), 'Wrong generation row/rendered membership')
            require(member.get('file_sha256') == catalogs[gid][key].get('file_sha256'),
                    'Generation source hash mismatch')
            observed.add(identity)
            if gid == first:
                first_keys.append(key)
        require(first_keys and sorted(first_keys) == row.get('source_keys'), 'First-send original source keys mismatch')
        order = list(artifacts)
        require(first == min((m['generation_id'] for m in row['memberships']), key=order.index),
                'Forged first-send generation attribution')
    require(observed == set(memberships), 'Incomplete generation membership coverage')
    schedule = store.json(size_row.get('schedule'))
    require(schedule.get('schema') == 'am255/ordered-source-query-schedule-v2' and
            schedule.get('size') == size_row['size'], 'Wrong delivery schedule')
    batches = schedule.get('document_batches')
    require(isinstance(batches, list) and [r for b in batches for r in b] == rows,
            'Delivery rows differ from frozen ordered batches')
    requests = ledger.get('requests')
    require(isinstance(requests, list) and len(requests) == len(batches), 'Missing actual document request')
    actual, reused, all_keys, seen_paths = {g: [] for g in artifacts}, {g: [] for g in artifacts}, set(), set()
    offset = 0
    for batch, binding in zip(batches, requests):
        require(1 <= len(batch) <= 8, 'Invalid contiguous document batch size')
        first = batch[0]['first_generation']
        require(all(r['first_generation'] == first for r in batch), 'Cross-generation document batch')
        require(binding.get('first_generation') == first and
                binding.get('rows') == list(range(offset, offset + len(batch))), 'Wrong request generation/row pointer')
        offset += len(batch)
        pointer = binding.get('request')
        request = store.json(pointer)
        require(pointer['path'] not in seen_paths, 'Duplicate actual request pointer')
        seen_paths.add(pointer['path'])
        keys = sorted({key for r in batch for key in r['source_keys']})
        rendered = [r['rendered_sha256'] for r in batch]
        body = request.get('body', {}).get('input')
        require(request.get('kind') == 'documents' and request.get('source_keys') == keys,
                'Request original source keys differ from complete first-generation batch')
        require(request.get('rendered_sha256') == rendered and isinstance(body, list) and
                all(isinstance(text, str) for text in body) and
                [sha(text.encode('utf-8')) for text in body] == rendered, 'Request rendered bytes/hash mismatch')
        for row, text in zip(batch, body):
            for member in row['memberships']:
                source = catalogs[member['generation_id']][member['key']]
                suffix = '\nScope: ' + source['scope']
                require(text.endswith(suffix) and
                        sha(text[:-len(suffix)].encode('utf-8')) == source['body_sha256'],
                        'Request bytes differ from canonical source body/scope')
        actual[first].append(pointer)
        all_keys.update(keys)
        for gid in {m['generation_id'] for r in batch for m in r['memberships']} - {first}:
            reused[gid].append(pointer)
    if not hasattr(store, 'deliveries'):
        store.deliveries = {}
    store.deliveries[size_row['size']] = {'ledger': ledger_pointer, 'actual': actual, 'reused': reused,
                                         'global_keys': all_keys, 'request_count': len(requests)}


def validate_provider_delivery(observation, store):
    provider = observation['provider']
    delivery = getattr(store, 'deliveries', {}).get(observation['size'])
    require(delivery is not None and provider.get('delivery_ledger') == delivery['ledger'],
            'Absent/foreign generation delivery ledger')
    gid = observation['native'].get('source_binding', {}).get('generation_id')
    require(gid in delivery['actual'] and provider.get('generation_id') == gid,
            'Wrong provider generation membership')
    require(provider.get('document_sends') == delivery['actual'][gid] and
            provider.get('reused_document_requests') == delivery['reused'][gid],
            'Missing/forged/cross-generation request pointer')
    count = sum(len(store.json(p)['rendered_sha256']) for p in provider['document_sends'])
    require(provider.get('documents_sent') == count, 'First-generation document count mismatch')
    return delivery


def generation_provider_metrics(case, observation, store, oracle):
    provider = observation['provider']
    delivery = validate_provider_delivery(observation, store)
    # Global hard exclusions apply to every real request, not mutation-local
    # deletion labels. A prior authorized send never disappears from this log.
    global_exposed = sorted(delivery['global_keys'] & oracle.forbidden_global())
    sent = {key for p in provider['document_sends'] for key in store.json(p)['source_keys']}
    exposed = sorted(sent & {oracle.resolve(ref) for ref in case['disclosure']['provider_send_forbidden']})
    result_scope = {'global_forbidden_source_keys_sent': global_exposed,
                    'global_retained_document_sources': len(delivery['global_keys']),
                    'global_retained_document_requests': delivery['request_count'],
                    'reused_prior_document_requests': len(provider['reused_document_requests']),
                    'erasure_executed': False,
                    'interpretation': 'Generation first-send accounting plus explicit exact-byte/profile reuse; global scientific log retained; no temporal delete or forensic/provider/cache erasure proof'}
    # Query accounting and unexecuted-state handling remain the old rules.
    result = provider_metrics(case, observation, store, oracle, legacy=True)
    if result['executed']:
        failures = []
        if exposed:
            failures.append('Forbidden source sent in this generation')
        if global_exposed:
            failures.append('Globally forbidden source sent to provider')
        if case['disclosure']['query_send_forbidden'] and result['actual_query_sends']:
            failures.append('Forbidden query sent to provider')
        result.update(failed_predicates=failures, forbidden_source_keys_sent=exposed)
    result.update(result_scope)
    return result


def provider_metrics(case, observation, store, oracle, legacy=False):
    provider = observation['provider']
    if provider.get('schema') == 'am255/generation-provider-accounting-v2' and not legacy:
        return generation_provider_metrics(case, observation, store, oracle)
    require(legacy or provider.get('schema') is None, 'Unknown provider accounting schema')
    if not isinstance(provider, dict):
        return metric(prerequisite='No complete actual provider-send accounting surface')
    if not provider.get('executed', False):
        return metric(prerequisite=provider.get('prerequisite', 'No complete actual provider-send accounting surface'))
    require(observation['executed'], 'Provider output on an unexecuted observation')
    require(isinstance(provider.get('document_sends'), list), 'Missing provider document request list')
    sent, query_count, requests = set(), 0, set()
    for pointer in provider['document_sends']:
        request = store.json(pointer)
        require(pointer['path'] not in requests, 'Duplicate provider document request')
        requests.add(pointer['path'])
        keys = request.get('source_keys')
        require(isinstance(keys, list) and all(isinstance(key, str) for key in keys),
                'Provider request lacks actual canonical source keys')
        sent.update(keys)
    query = provider.get('query_send')
    if query is not None:
        request = store.json(query)
        require(request.get('query_sha256') == provider.get('query_sha256'), 'Provider query hash identity mismatch')
        query_count = 0 if provider.get('query_vector_reused') is True else 1
    require(provider.get('queries_sent') == query_count, 'Provider query send accounting mismatch')
    forbidden = oracle.forbidden_global() | {oracle.resolve(ref) for ref in case['disclosure']['provider_send_forbidden']}
    exposed = sorted(sent & forbidden)
    failures = ['Forbidden source sent to provider'] if exposed else []
    if case['disclosure']['query_send_forbidden'] and query_count:
        failures.append('Forbidden query sent to provider')
    return metric(True, failures=failures, forbidden_source_keys_sent=exposed, unique_document_sources=len(sent),
                  actual_document_requests=len(requests), actual_query_sends=query_count,
                  interpretation='Private sent-source accounting only; not a public body/disclosure/status pass')


def score_case(case, observation, store, generation, catalog, profile_hash, oracle):
    if observation is None:
        return {name: metric(prerequisite='No hash-bound observation for this exact size/variant/ordinal') for name in CLASSES}
    classes = {}
    native = observation['native']
    exercised = isinstance(native.get('request_valid'), bool)
    if exercised:
        require(observation['executed'], 'Native output on an unexecuted observation')
        binding = native.get('source_binding')
        require(isinstance(binding, dict), 'Missing per-case native source binding')
        require(generation is not None and binding.get('generation_id') == generation['generation_id'] and
                binding.get('canonical_sha256') == generation['canonical_sha256'] and
                binding.get('profile_sha256') == profile_hash, 'Wrong per-case native source/profile/generation identity')
        if native.get('request_valid') is True:
            digest(native.get('context_sha256'), 'native context')
            context = native.get('context')
            require(isinstance(context, dict), 'Native qualification lacks actual observed context')
            require(sha(compact(context).encode()) == native['context_sha256'], 'Native context digest mismatch')
            supplied = case['context_truth']['supplied']
            require(all(context.get(key) == value for key, value in supplied.items()),
                    'Actual native context differs from compiled case context')
            require(context.get('branch') == case['context_truth']['observed_git_branch'],
                    'Actual observed branch differs from compiled native branch')
        if observation['semantic'].get('executed'):
            require(generation.get('vector_store') is not None, 'Semantic output lacks a hash-bound vector store')
        failures = []
        eligible = eligible_domain(native, catalog)
        forbidden = oracle.forbidden_global() | oracle.forbidden(case, catalog)
        if eligible & forbidden:
            failures.append('Frozen native-forbidden current source in eligible domain')
        if native['request_valid'] != (case['availability']['outcome'] != 'invalid-request'):
            failures.append('Actual native request gate differs from frozen validation expectation')
        if native['request_valid'] and (native.get('crosscheck') or {}).get('matched') is not True:
            failures.append('Native evaluate_sources crosscheck absent/unmatched')
        truth = case['context_truth']
        if generation.get('mutation') != truth['mutation'] or generation.get('stage') != truth['stage']:
            failures.append('Canonical mutation/stage differs from requested case transition')
        if truth['stage'] == 'before-rebuild':
            classes['native'] = metric(prerequisite='Actual stale-generation transition lacks a reviewed receipt verifier')
        else:
            classes['native'] = metric(True, failures=failures, request_valid=native['request_valid'])
    else:
        classes['native'] = unexecuted(observation, 'native', 'Native request/context qualification not run')
    if observation['semantic'].get('executed') or observation['typed'].get('executed'):
        require(exercised, 'Semantic/typed outputs without a source-bound native gate')
    runtime_state = case['context_truth']['runtime_state']
    if runtime_state != 'ready-complete-generation' or case['context_truth']['stage'] == 'before-rebuild':
        # This scorer has no reviewed state-transition receipt verifier. A reused
        # ready-generation vector cannot qualify a remote/stopped/stale state.
        reason = 'Actual runtime/generation transition lacks a reviewed receipt verifier: ' + runtime_state
        classes['semantic'] = metric(prerequisite=reason, ann=metric(prerequisite=reason))
        classes['typed'] = metric(prerequisite=reason)
    else:
        classes['semantic'] = semantic_metrics(case, observation, catalog, oracle)
        classes['typed'] = typed_metrics(case, observation, catalog, oracle)
    classes['provider_canaries'] = provider_metrics(case, observation, store, oracle)
    for name, reason in (
            ('outcome', 'No separately reviewed/frozen semantic-result outcome mapper'),
            ('caveats', 'No separately reviewed/frozen mandatory-caveat assembler'),
            ('public_disclosure', 'No actual public body/identifier/count/diagnostic surface mapper'),
            ('literal_status', 'No reviewed native literal/status/exit/no-match comparison mapper'),
            ('abstention', 'No calibration-only cutoff frozen; raw ranking is not abstention')):
        classes[name] = unexecuted(observation, name, reason)
        if isinstance(observation.get(name), dict) and observation[name].get('executed'):
            classes[name]['unsupported_raw_execution_claim'] = True
            classes[name]['prerequisites'] = [reason]
    return classes


def summarize(cells):
    output = {'expected_cells': len(cells), 'observed_cells': sum(cell['observation_present'] for cell in cells), 'classes': {}}
    for name in CLASSES:
        measured = [cell['classes'][name] for cell in cells if cell['classes'][name]['executed']]
        failed = sum(bool(value['failed_predicates']) for value in measured)
        output['classes'][name] = {'executed': len(measured), 'failed': failed,
                                 'without_measured_failure': len(measured) - failed,
                                 'unexecuted': len(cells) - len(measured),
                                 'measured_denominator': len(measured), 'intended_denominator': len(cells),
                                 'measured_failure_fraction': failed / len(measured) if measured else None,
                                 'prerequisites': dict(Counter(reason for cell in cells
                                      for reason in cell['classes'][name]['prerequisites']))}
    semantic = [cell['classes']['semantic'] for cell in cells if cell['classes']['semantic']['executed']]
    required = [value for value in semantic if not value['empty_required']]
    required_hits = sum(value['required_hits'] for value in required)
    required_total = sum(value['required_count'] for value in required)
    output['semantic_raw_recall'] = {
        'executed_nonempty_required_cells': len(required), 'executed_empty_required_cells': len(semantic) - len(required),
        'required_hits': required_hits, 'executed_required_label_denominator': required_total,
        'micro_recall_on_executed_required_labels': required_hits / required_total if required_total else None,
        'intended_required_label_denominator': sum(cell['required_label_count'] for cell in cells),
        'unexecuted_required_label_count': sum(cell['required_label_count'] for cell in cells
                                             if not cell['classes']['semantic']['executed']),
        'macro_recall_on_executed_nonempty_cells': sum(value['required_recall'] for value in required) / len(required) if required else None,
        'domain_classes': dict(Counter(value['k_domain_class'] for value in semantic)),
        'complete_required_sets': sum(value['required_hits'] == value['required_count'] for value in required),
        'complete_required_set_denominator': len(required),
    }
    output['ann'] = {}
    for label in ('budget<N', 'exhaustive-candidates'):
        measured = [value['ann'] for value in semantic if value['ann']['executed'] and
                    value['ann']['candidate_domain_class'] == label]
        output['ann'][label] = {'executed': len(measured), 'failed': sum(bool(v['failed_predicates']) for v in measured),
                               'k_domain_classes': dict(Counter(value['k_domain_class'] for value in measured)),
                               'candidate_recall_denominator': sum(v['candidate_recall'] is not None for v in measured),
                               'mean_candidate_recall': sum(v['candidate_recall'] for v in measured if v['candidate_recall'] is not None) /
                               sum(v['candidate_recall'] is not None for v in measured) if any(v['candidate_recall'] is not None for v in measured) else None,
                               'mean_exact_rescored_top_k_overlap': sum(v['exact_rescored_top_k_overlap'] for v in measured
                                   if v['exact_rescored_top_k_overlap'] is not None) /
                               sum(v['exact_rescored_top_k_overlap'] is not None for v in measured)
                               if any(v['exact_rescored_top_k_overlap'] is not None for v in measured) else None}
    output['ann']['unexecuted'] = len(cells) - sum(v['ann']['executed'] for v in semantic)
    typed = [cell['classes']['typed'] for cell in cells if cell['classes']['typed']['executed']]
    required_paths = sum(value['executed_required_paths'] for value in typed)
    edges = sum(value['actual_edges'] for value in typed if not value['provenance_unexecuted'])
    output['typed_raw_paths'] = {
        'matched_required_paths': sum(value['matched_required_paths'] for value in typed),
        'executed_required_path_denominator': required_paths,
        'intended_required_path_denominator': sum(cell['required_path_count'] for cell in cells),
        'unexecuted_required_path_count': sum(cell['required_path_count'] for cell in cells if not cell['classes']['typed']['executed']) +
                                          sum(len(value['provenance_unexecuted_required']) for value in typed),
        'micro_required_path_recall': sum(value['matched_required_paths'] for value in typed) / required_paths if required_paths else None,
        'fully_provenanced_edge_denominator': edges,
        'micro_relation_label_accuracy': sum(value['valid_canonical_edges'] for value in typed
                                             if not value['provenance_unexecuted']) / edges if edges else None,
        'unverified_provenance_edge_count': sum(value['provenance_unexecuted'] for value in typed),
    }
    output['conflict_evidence_completeness'] = metric(
        prerequisite='Raw primary ranks/edges do not implement an outcome or mandatory-caveat/evidence assembler')
    return output


def score(suite_path, map_path, raw_root, manifest_path, expected_source_commit):
    suite_raw = trusted_bytes(suite_path, SUITE_SHA256)
    map_raw = trusted_bytes(map_path, MAP_SHA256)
    store = RawRoot(raw_root)
    try:
        manifest, inputs, profile, observations, size_rows, generations = bind_raw(store, manifest_path, expected_source_commit)
        # The oracle is opened only after all raw observations/receipts have hashes.
        suite = decode(suite_raw, 'frozen suite')
        mapping = decode(map_raw, 'frozen scorer map')
        require(sha(compact(suite['cases']).encode()) == LABELS_SHA256, 'Frozen case label digest mismatch')
        require(mapping.get('input_sha256') == INPUT_SHA256 and mapping.get('suite_sha256') == SUITE_SHA256,
                'Scorer map compiled-input/suite binding mismatch')
        expected_map = [{'case_ordinal': i, 'case_id': case['case_id'], 'split': case['split']}
                        for i, case in enumerate(suite['cases'], 1)]
        require(mapping.get('cases') == expected_map, 'Scorer map ordinal/case/split identity mismatch')
        require(Counter(c['split'] for c in suite['cases']) == {'calibration': 16, 'held-out': 85},
                'Independent label denominator differs')
        oracle = Oracle(suite)
        catalogs = {identity: catalog_for(store, generation, oracle) for identity, generation in generations.items()}
        cells = []
        for size in SIZES:
            for variant in VARIANTS:
                for ordinal, case in enumerate(suite['cases'], 1):
                    if variant == 'base' and case['partition'] != 'base':
                        continue
                    pointer = observations.get((size, variant, ordinal))
                    observation = store.json(pointer) if pointer else None
                    native = observation.get('native', {}) if observation else {}
                    binding = native.get('source_binding')
                    require(binding is None or isinstance(binding, dict), 'Native source binding must be an object or null')
                    gid = binding.get('generation_id') if binding else None
                    generation = generations.get((size, gid))
                    require(generation is None or generation['variant'] == variant, 'Foreign variant generation source')
                    classes = score_case(case, observation, store, generation, catalogs.get((size, gid), {}),
                                         size_rows.get(size, {}).get('profile_sha256'), oracle)
                    cells.append({'size': size, 'variant': variant, 'case_ordinal': ordinal,
                                  'case_id': case['case_id'], 'split': case['split'], 'operation': case['operation'],
                                  'required_label_count': len(case['primary']['required']),
                                  'required_path_count': len(case['relationships']['required']),
                                  'no_content_overlap': case['paraphrase_class'] == 'no-content-overlap',
                                  'observation_present': observation is not None,
                                  'prerequisites': observation['prerequisites'] if observation else ['Missing exact observation'],
                                  'classes': classes})
        groups = []
        for size in SIZES:
            for variant in VARIANTS:
                for split in ('calibration', 'held-out'):
                    selected = [c for c in cells if (c['size'], c['variant'], c['split']) == (size, variant, split)]
                    groups.append({'size': size, 'variant': variant, 'split': split, **summarize(selected),
                                   'no_content_overlap': summarize([c for c in selected if c['no_content_overlap']])})
        # Rehash bound files at completion as well; changed raw evidence is rejected.
        for path, expected in tuple(store.hashes.items()):
            store.verify({'path': path, 'sha256': expected})
        return {'schema': 'am255/independent-offline-score-v1', 'status': 'raw-research-report-not-acceptance',
                'accounting_protocol': manifest['schema'],
                'interpretation': {'correction_held_out_informed': True,
                    'affected_repeats_independent_held_out': False,
                    'old_frozen_failure_rescored': False,
                    'new_independent_review_and_full_closure_freeze_required': True,
                    'temporal_delete_erasure': {'executed': False,
                        'prerequisite': 'Separate controlled source/profile/generation-bound canonical delete, rebuild, derived-store purge, provider/cache lifecycle and forensic readback implementation'}},
                'source': {'suite_sha256': SUITE_SHA256, 'labels_sha256': LABELS_SHA256,
                           'scorer_map_sha256': MAP_SHA256, 'input_sha256': INPUT_SHA256,
                           'spec_sha256': manifest['spec_sha256'], 'io_sha256': manifest['io_sha256'],
                           'expected_source_commit': expected_source_commit,
                           'scorer_sha256': sha(Path(__file__).read_bytes()),
                           'manifest_sha256': store.hashes[relative(manifest_path).as_posix()],
                           'source_bindings': manifest['source_bindings'], 'bound_artifact_count': len(store.hashes)},
                'qualification_scope': manifest['qualification_scope'], 'limitations': LIMITATIONS,
                'independent_labels': {'calibration': 16, 'held-out': 85, 'total': 101,
                                       'base_cases': 88, 'overlay_cases': 101,
                                       'counterparts_not_independent_labels': True},
                'cutoff': {'executed': False, 'prerequisite': 'No calibration-only cutoff method/value frozen',
                           'held_out_tuning': False, 'abstention': 'unexecuted'},
                'expected_cells': len(cells), 'observation_cells': len(observations), 'groups': groups, 'cells': cells,
                'costs': [{'size': size, 'actual': row.get('costs', {}),
                           'missing_prerequisites': row.get('missing_prerequisites', [])} for size, row in size_rows.items()],
                'cleanup': manifest['cleanup'], 'proposed_thresholds_not_acceptance': suite['proposals'],
                'model_graph_llm_calls': 0}
    finally:
        store.close()


def save(path, value):
    # A retry never overwrites a prior report, including a failed integrity attempt.
    path = Path(path)
    require(not path.is_symlink() and path.parent.is_dir(), 'Output parent must already exist')
    with path.open('xb') as stream:
        stream.write(encoded(value))
        stream.flush()
        os.fsync(stream.fileno())
    fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', required=True, type=Path)
    parser.add_argument('--scorer-map', required=True, type=Path)
    parser.add_argument('--raw-root', required=True, type=Path)
    parser.add_argument('--manifest', required=True, help='Path relative to --raw-root (no symlinks/traversal)')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--expected-source-commit', required=True,
                        help='Externally trusted full reviewed source freeze SHA, never inferred from raw evidence')
    args = parser.parse_args()
    try:
        report = score(args.suite, args.scorer_map, args.raw_root, args.manifest, args.expected_source_commit)
        result = 0
    except (IntegrityError, OSError, KeyError, TypeError, ValueError, AttributeError, RecursionError) as exc:
        report = {'schema': 'am255/independent-offline-score-v1', 'status': 'integrity-rejected',
                  'error': str(exc), 'error_type': type(exc).__name__, 'model_graph_llm_calls': 0,
                  'qualification_scope': 'No quality claims from rejected evidence'}
        result = 2
    save(args.output, report)
    print(json.dumps({'status': report['status'], 'output': str(args.output),
                      'output_sha256': sha(encoded(report)), 'model_graph_llm_calls': 0}))
    return result


if __name__ == '__main__':
    sys.exit(main())
