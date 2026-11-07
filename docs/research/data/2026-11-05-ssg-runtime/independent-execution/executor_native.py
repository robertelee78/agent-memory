"""Hash-pinned actual AM175 canonical fixtures and fresh native qualification."""
from __future__ import annotations
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time
from executor_render import document

ROOTS = ('skills/agent-memory/scripts/agent_memory', 'skills/agent-memory/schemas')
FIXTURE_FILES = ('scripts/evaluations/labeled_recall_fixture.py',
                 'scripts/evaluations/semantic_navigation_fixture.py',
                 'skills/agent-memory/templates/record-proposal.example.json')
PINS = {'validate_suite.py': '5b47a20c573d1cbbf093016f6ef33b42c72196d7140990c5d569a773d0c552a4',
        'g1_relationship_overlay.py': '69127e95964ba5e9ee46ff5ce1a616aebe5172f624923445e7cc5714279f993a'}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def compact(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False)


def native_identity(root, inputs):
    revision = subprocess.run(['git', '-C', str(root), 'rev-parse', 'HEAD'], capture_output=True,
                              text=True, check=True).stdout.strip()
    if revision != inputs['authoring_am_commit']:
        raise ValueError('Actual authoring checkout differs from compiled native source')
    rows = []
    for relative in ROOTS:
        for path in sorted((root / relative).rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts:
                if path.is_symlink():
                    raise ValueError('Symlink in native source tree')
                rows.append(path.relative_to(root).as_posix() + ' ' + digest(path.read_bytes()))
    tree = digest('\n'.join(rows).encode())
    if tree != inputs['authoring_native_tree_sha256']:
        raise ValueError('Actual authoring native tree changed')
    for relative in FIXTURE_FILES:
        path = root / relative
        blob = subprocess.run(['git', '-C', str(root), 'show', revision + ':' + relative],
                              capture_output=True, check=True).stdout
        if path.is_symlink() or path.read_bytes() != blob:
            raise ValueError('Actual canonical fixture source differs from AM175 Git blob: ' + relative)
    return {'commit': revision, 'native_tree_sha256': tree, 'files': len(rows), 'roots': list(ROOTS)}


class Fixtures:
    def __init__(self, am_root, inputs, contexts_root):
        self.root, self.inputs, self.contexts_root = am_root, inputs, contexts_root
        self.identity = native_identity(am_root, inputs)
        directory = Path(__file__).resolve().parent.parent / 'independent-suite'
        for name, expected in PINS.items():
            if digest((directory / name).read_bytes()) != expected:
                raise ValueError('Frozen native fixture helper changed: ' + name)
        sys.path.insert(0, str(directory))
        spec = importlib.util.spec_from_file_location('am255_native_fixture', directory / 'validate_suite.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.module, self.native = module, module.Native(am_root)
        self.lrf, self.snf, self.template = module.overlay.load(am_root)
        self.pid = self.lrf.PID
        self.now = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')

    def build(self, size, variant, mutation=None):
        files, rows, info = self.module.overlay.bank(self.root, size, overlay=variant == 'overlay')
        if mutation:
            action = dict(self.inputs['mutations'][mutation])
            target = self.resolve(action['target'], files, rows)
            action['target'] = 'rec:' + target['label']
            action.pop('new_record', None)
            action['new_ref'] = 'rec:independent-mutation-' + str(self.inputs['mutation_order'].index(mutation))
            files, rows = self.module.mutate(self.native, self.lrf, self.snf, files, rows, action, self.pid)
            expected = self.inputs['mutations'][mutation].get('new_record')
            if expected:
                self.resolve(expected, files, rows)
        bank = self.module.Bank(self.native, files, rows, self.pid)
        bank.entry_of = {entry['path']: entry for entry in bank.entries}
        bank.canonical_sha256 = digest(compact({p: bank.hashes[p] for p in sorted(files)}).encode())
        bank.generation_id = 'g-' + str(size) + '-' + variant + '-' + bank.canonical_sha256[:20]
        bank.sources = {}
        for entry in bank.entries:
            path, kind = entry['path'], entry['format']
            source = {'path': path, 'format': kind, 'file_sha256': bank.hashes[path],
                      'source_sha256': bank.hashes[path], 'scope_chain': entry['scope_chain']}
            if kind == 'standalone-record':
                _, meta, body = bank.parsed[path]
                source.update(key=self.record_key(meta), id=meta['id'], version=meta['version'],
                              metadata=meta, status=meta['status'], visibility=meta['visibility'],
                              fixture_label=bank.label_of[path])
                if mutation and expected and path == expected['path']:
                    # new_ref is a private mutator handle, not an authored
                    # canonical fixture label. Identity remains id/version/path.
                    source.pop('fixture_label')
            else:
                body = files[path]
                source['key'] = 'context:' + self.pid + ':' + path
            source['body_sha256'] = digest(body.encode('utf-8'))
            source['body'] = body
            source['scope'] = source.get('metadata', {}).get('scope') or (entry['scope_chain'][-1] if entry['scope_chain'] else 'repository')
            bank.sources[path] = source
        bank.provider_sources = [s for p, s in sorted(bank.sources.items(), key=lambda kv: kv[0].encode())
                                 if self.provider_safe(bank, p)]
        return bank, info

    def record_key(self, meta):
        return 'record:' + self.pid + ':' + meta['id'] + ':' + str(meta['version'])

    def resolve(self, ref, files, rows):
        path = ref['path']
        if path not in files:
            raise ValueError('Compiled reference does not exist in actual fixture: ' + path)
        if ref['format'] == 'record-version':
            row = next((r for r in rows if r['path'] == path), None)
            if row is None or (row['metadata']['id'], row['metadata']['version']) != (ref['id'], ref['version']):
                raise ValueError('Compiled record identity differs from actual canonical fixture')
            return row
        if ref['format'] != 'context-source':
            raise ValueError('Unsupported compiled native reference')
        return {'path': path}

    def provider_safe(self, bank, path):
        entry = bank.entry_of.get(path)
        if not entry or entry['format'] not in ('standalone-record', 'context-prose') or entry['lifecycle'] != 'active':
            return False
        if len(bank.raw[path]) > 16384 or bank.scan.get(path, ['not-scanned']):
            return False
        source = bank.sources[path]
        if source['format'] == 'standalone-record':
            meta = source['metadata']
            gates = self.native.lifecycle.current(meta, {'authorized-scope': True, 'applicable': True,
                    'now': self.now, 'minimum-trust': 'reviewed', 'active-superseders': []})['gates']
            return all(gates.values())
        return True

    def context_root(self, branch):
        path = self.contexts_root / digest(branch.encode())[:20]
        if not path.exists():
            path.mkdir(parents=True, mode=0o700)
            subprocess.run(['git', 'init', '--quiet', '--initial-branch=' + branch, str(path)],
                           capture_output=True, check=True)
        return path

    def qualify(self, bank, case):
        self.now = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')
        truth = case['context_truth']
        request = {k: v for k, v in case['request'].items() if k != 'ranking'}
        initial = {'request_valid': False, 'eligible': [], 'context_sha256': None,
                   'source_binding': {'generation_id': bank.generation_id,
                                      'canonical_sha256': bank.canonical_sha256},
                   'crosscheck': {'matched': False, 'prerequisite': 'request/context not qualified'}}
        try:
            self.native.retrieval.request_value(request)
            context, observed = self.native.retrieval.observed_context(
                self.context_root(truth['observed_git_branch']), truth['supplied'])
        except self.native.ContractError as exc:
            return initial | {'refusal': str(exc)}, None
        deferred = []
        for ref in truth['deferred']:
            if ref['path'] not in bank.files:
                raise ValueError('Deferred compiled source missing from fixture')
            if ref['format'] == 'record-version':
                actual = bank.sources[ref['path']]
                if (actual.get('id'), actual.get('version')) != (ref['id'], ref['version']):
                    raise ValueError('Deferred source identity mismatch')
            deferred.append(ref['path'])
        if truth['evaluation_time_after'] and self.now <= truth['evaluation_time_after']:
            return initial | {'prerequisite': 'Actual current UTC time must exceed declared evaluation time'}, None
        # Fresh gates preserve supersession/security/deferred ordering. The frozen
        # helper's memo key omits selectors and time; never reuse its crosscheck.
        eligible, reasons = bank._eligibility(request, context, self.now, deferred)
        memo_key = ('crosscheck',) + bank.key(request, context, deferred)
        bank.memo.pop(memo_key, None)
        try:
            matched, excluded, mine = bank.native_crosscheck(request, context, self.now, deferred, eligible, reasons)
        finally:
            bank.memo.pop(memo_key, None)
        gate_excluded = {key: value for key, value in excluded.items() if key != 'result-budget'}
        if not matched and (not excluded.get('result-budget') or gate_excluded != mine):
            raise ValueError('Native evaluate_sources crosscheck differs from eligibility qualification')
        # Exact ids OR paths narrow the post-gate semantic ranking domain. Empty
        # query is deliberate: semantic query text is not a native lexical filter.
        selection = {**request, 'query': '', 'limit': 64}
        selected = [p for p in eligible if self.native.retrieval.match(
            selection, {'path': p, 'body': '', 'record': bank.sources[p].get('metadata', {})})]
        rows, native_excluded, used, pool, ranked, canonical_bytes = self.native.retrieval.evaluate_sources(
            None, None, self.pid, bank.entries, lambda path: bank.raw[path],
            lambda path, text: bank.parsed[path], bank.hashes, selection, selection, context, self.now,
            frozenset(deferred), None)
        expected = dict(mine)
        if len(selected) != len(eligible):
            expected['selector-or-text-mismatch'] = len(eligible) - len(selected)
        observed_excluded = {key: value for key, value in native_excluded.items() if key != 'result-budget'}
        if ranked != len(selected) or observed_excluded != expected:
            raise ValueError('Native exact-selector crosscheck differs from qualified ranking domain')
        for row in rows:
            source = bank.sources[row['path']]
            if row['source_sha256'] != source['source_sha256'] or (
                    source['format'] == 'standalone-record' and
                    (row['record']['id'], row['record']['version']) != (source['id'], source['version'])):
                raise ValueError('Native selector crosscheck source identity mismatch')
        complete = len(rows) == ranked and not native_excluded.get('result-budget')
        if complete and sorted(row['path'] for row in rows) != sorted(selected):
            raise ValueError('Native selector crosscheck returned a different source domain')
        crosscheck = {'matched': matched and complete, 'excluded_counts': dict(native_excluded),
                      'qualified_counts': expected,
                      'gate_crosscheck': {'matched': matched, 'excluded_counts': excluded, 'qualified_counts': mine},
                      'native_selection': {'ranked_domain_size': ranked, 'result_count': len(rows),
                                           'result_limit': selection['limit'], 'used_bytes': used,
                                           'result_byte_limit': self.native.retrieval.MAX_RESULT,
                                           'pool': pool, 'canonical_bytes': canonical_bytes}}
        qualified = initial | {'request_valid': True, 'context_sha256': digest(compact(context).encode()),
                               'observed': observed, 'context': context, 'crosscheck': crosscheck}
        if not matched or not complete:
            prerequisite = 'Bounded native output cannot crosscheck the complete qualified domain'
            crosscheck['prerequisite'] = prerequisite
            return qualified | {'prerequisite': prerequisite}, context
        identities = [{k: bank.sources[p][k] for k in ('key', 'path', 'id', 'version') if k in bank.sources[p]}
                      for p in selected]
        if any(not self.provider_safe(bank, p) for p in selected):
            raise ValueError('Native eligible domain contains source outside provider projection')
        return qualified | {'eligible': identities}, context

    def literal(self, bank, case, context):
        request = {k: v for k, v in case['request'].items() if k != 'ranking'}
        self.native.retrieval.request_value(request)
        deferred = frozenset(ref['path'] for ref in case['context_truth']['deferred'])
        start = time.monotonic()
        rows, excluded, used, pool, ranked, canonical_bytes = self.native.retrieval.evaluate_sources(
            None, None, self.pid, bank.entries, lambda path: bank.raw[path],
            lambda path, text: bank.parsed[path], bank.hashes, request, request, context, self.now,
            deferred, None)
        return {'executed': True, 'matches': [{k: row[k] for k in ('path', 'format', 'source_sha256', 'match') if k in row}
                                             for row in rows],
                'excluded_counts': dict(excluded), 'used_bytes': used, 'pool': pool, 'ranked': ranked,
                'canonical_bytes': canonical_bytes, 'seconds': time.monotonic() - start,
                'interpretation': 'Actual unchanged native literal matching only; no semantic-result status mapping'}

    def catalog(self, bank):
        return {'schema': 'am255/canonical-generation-v1', 'generation_id': bank.generation_id,
                'canonical_sha256': bank.canonical_sha256,
                'entries': [{k: v for k, v in s.items() if k != 'body'} for s in bank.sources.values()],
                'provider_source_keys': [s['key'] for s in bank.provider_sources],
                'files': [{'path': p, 'sha256': bank.hashes[p]} for p in sorted(bank.files)]}

    def rendered(self, bank):
        return [(s, document(s['body'], s['scope'])) for s in bank.provider_sources]
