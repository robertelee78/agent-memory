"""Light validator for the frozen G1 independent suite (no graph, model or ranking).

Checks the suite against the *actual* AM checkout: pinned source hashes, native
bank validation of each fixture variant/size, canonical record identities,
paths, versions and hashes, native request validation, native per-record
eligibility in each case's finite context (cross-checked against the native
`retrieval.evaluate_sources` exclusion counts with an empty query), recorded
relationship assertions, caveat/disclosure consistency, global invariants and
the coverage map. It never executes query-dependent retrieval, embedding, graph
or similarity code, so it cannot see or be tuned to retrieval outputs.

python3 validate_suite.py --am-root CHECKOUT [--sizes 1000 10000 50000]
    [--crosscheck-sizes 1000] [--report out.json] [--emit-oracles oracles.json]
"""
from __future__ import annotations

import argparse
from collections import Counter
import copy
from datetime import datetime, timezone
import fnmatch
import hashlib
import json
from pathlib import Path
import re
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import g1_relationship_overlay as overlay  # noqa: E402

STOP = set('''a an the and or but if of to in on at by for from with without into onto over under after before is are
was were be been being do does did done have has had can could may might must shall should will would it its this that these
those there their them they we our us you your i me my what which who whom whose when where why how any some all each every no
not yet still so than then too very just about again also as up out off get gets got one now more most much many own only
let'''.split())
RECORD_FIELDS = ('status', 'visibility', 'scope', 'conditions', 'evidence', 'kind')
EXIT = {'qualified-guidance': {0}, 'qualified-empty': {0}, 'qualified-empty-unresolved': {0}, 'withheld-replacement': {0},
        'literal-unchanged': {0}, 'literal-fallback': {0}, 'non-success': {1}, 'invalid-request': {1, 2}}
UNRESOLVED = ('missing-context:', 'unknown-condition:', 'unknown-applicability', 'pending-semantic-conflict', 'security:', 'visible',
              'no-active-superseder', 'memory-type', 'active')


def tokens(text):
    return {t for t in re.findall(r'[a-z0-9]+', text.casefold()) if t not in STOP}


def sha256(data):
    return hashlib.sha256(data if isinstance(data, bytes) else data.encode()).hexdigest()


class Problems(list):
    def check(self, condition, where, message):
        if not condition:
            self.append(where + ': ' + message)
        return condition


class Native:
    """Thin handle on the actual AM native modules at --am-root."""

    def __init__(self, am_root):
        scripts = str(am_root / 'skills/agent-memory/scripts')
        if scripts not in sys.path:
            sys.path.insert(0, scripts)
        from agent_memory import bank_contract, lifecycle, records, retrieval, scopes, security, tombstones
        from agent_memory.developer_identity import digest
        from agent_memory.frontmatter import ContractError
        for module in (bank_contract, retrieval):
            if not Path(module.__file__).resolve().is_relative_to(am_root):
                raise RuntimeError('Native module resolved outside --am-root: ' + module.__file__)
        self.bank_contract, self.lifecycle, self.records = bank_contract, lifecycle, records
        self.retrieval, self.scopes, self.security, self.tombstones = retrieval, scopes, security, tombstones
        self.digest, self.ContractError = digest, ContractError

    def observed(self, supplied, branch):
        original = self.retrieval.git_context
        self.retrieval.git_context = lambda root: {'branch': branch}
        try:
            return self.retrieval.observed_context(None, supplied)[0]
        finally:
            self.retrieval.git_context = original


class Bank:
    """One natively validated canonical map with per-entry precomputation."""

    def __init__(self, native, files, rows, pid):
        self.native, self.files, self.pid = native, files, pid
        self.parsed = {}
        checked = native.bank_contract.validate_files(files, parsed_records=self.parsed)
        self.entries, self.routes = checked['entries'], checked['routes']
        self.raw = {path: value.encode() for path, value in files.items()}
        self.hashes = {path: native.digest(raw) for path, raw in self.raw.items()}
        self.by_label = {row['label']: row for row in rows if row['path'] in files}
        self.label_of = {row['path']: row['label'] for row in rows if row['path'] in files}
        self.scan = {}
        for entry in self.entries:
            if entry['format'] in {'standalone-record', 'context-prose'} and len(self.raw[entry['path']]) <= 16384:
                text = self.raw[entry['path']].decode()
                self.scan[entry['path']] = native.security.scan(text + '\n' + json.dumps(
                    {'path': entry['path'], 'scope_chain': entry['scope_chain']}, ensure_ascii=False))['reasons']
        self.parents = {}
        for path, value in files.items():
            if path.startswith('topics/n/'):
                for child in re.findall(r'^\| \[([^\]]+)\]', value, re.M):
                    self.parents.setdefault(child, set()).add(path)
        self.memo = {}

    def key(self, request, context, deferred):
        return (json.dumps(context, sort_keys=True), request['include_context'], tuple(request['kinds']),
                request['minimum_trust'], tuple(sorted(deferred)))

    def eligibility(self, request, context, now, deferred):
        """Memoized: cases sharing the same finite context share one native qualification."""
        key = ('eligibility',) + self.key(request, context, deferred)
        if key not in self.memo:
            self.memo[key] = self._eligibility(request, context, now, deferred)
        return self.memo[key]

    def _eligibility(self, request, context, now, deferred):
        """Per-path native reasons, replicating retrieval.evaluate_sources gate order."""
        n = self.native
        reasons, pool, superseders = {}, [], {}
        for entry in self.entries:
            path, kind = entry['path'], entry['format']
            if kind not in {'standalone-record', 'context-prose'}:
                reasons[path] = ['non-guidance-format']; continue
            if entry['lifecycle'] != 'active':
                reasons[path] = ['active']; continue
            if kind == 'context-prose' and not request['include_context']:
                reasons[path] = ['orientation-not-requested']; continue
            scoped = n.retrieval.scope_reasons(self.pid, entry, context)
            if scoped:
                reasons[path] = scoped; continue
            metadata = None
            if kind == 'standalone-record':
                _, metadata, _ = self.parsed[path]
                applicable = n.records.applicability(metadata, {'bank_project_id': self.pid, 'context': context})
                selection = n.lifecycle.current(metadata, {'authorized-scope': True, 'applicable': applicable['applicable'],
                    'now': now, 'minimum-trust': request['minimum_trust'], 'active-superseders': []})
                required = ('authorized-scope', 'applicable', 'active', 'valid-time', 'sufficient-trust')
                if not all(selection['gates'][key] for key in required):
                    reasons[path] = selection['reasons'] + applicable['reasons']; continue
                for item in metadata['supersedes'] + metadata['corrects']:
                    superseders.setdefault((item['id'], item['version']), []).append(metadata['id'] + '/' + str(metadata['version']))
            if self.scan[path]:
                reasons[path] = ['security:' + reason for reason in self.scan[path]]; continue
            pool.append((path, metadata))
        eligible = []
        for path, metadata in pool:
            if metadata is not None:
                replacing = sorted(set(superseders.get((metadata['id'], metadata['version']), [])))
                selection = n.lifecycle.current(metadata, {'authorized-scope': True, 'applicable': True, 'now': now,
                    'minimum-trust': request['minimum_trust'], 'active-superseders': replacing})
                if not selection['current']:
                    reasons[path] = selection['reasons']; continue
                if metadata['kind'] not in request['kinds']:
                    reasons[path] = ['memory-type']; continue
            if path in deferred:
                reasons[path] = ['pending-semantic-conflict']; continue
            eligible.append(path)
        return eligible, reasons

    def native_crosscheck(self, request, context, now, deferred, eligible, reasons):
        key = ('crosscheck',) + self.key(request, context, deferred)
        if key in self.memo:
            return self.memo[key]
        req = {**request, 'query': '', 'ids': [], 'paths': [], 'limit': 64}
        results, excluded, *_ = self.native.retrieval.evaluate_sources(None, None, self.pid, self.entries,
            lambda path: self.raw[path], lambda path, text: self.parsed[path], self.hashes, req, req, context, now,
            frozenset(deferred), None)
        mine = Counter(reason for values in reasons.values() for reason in values)
        selected = sorted(item['path'] for item in results)
        self.memo[key] = (dict(excluded) == dict(mine) and selected == sorted(eligible), dict(excluded), dict(mine))
        return self.memo[key]


def mutate(native, lrf, snf, files, rows, spec, pid):
    files, rows = dict(files), copy.deepcopy(rows)
    target = next(row for row in rows if row['label'] == spec['target'][4:])
    meta = target['metadata']
    index = next(path for path, value in files.items() if path.startswith('topics/n/') and '[' + target['path'] + ']' in value)
    lines = files[index].split('\n')
    at = next(i for i, line in enumerate(lines) if line.startswith('| [' + target['path'] + ']'))

    def row_text(path, purpose, scope, state):
        return snf.route_table([(path, purpose, scope, state, 'When this subject applies')], index).split('\n')[2]
    title = target['body'].splitlines()[0].lstrip('# ').strip()
    if spec['op'] == 'add-correction':
        new = copy.deepcopy(target)
        nm = new['metadata']
        nm.update({'version': meta['version'] + 1, 'corrects': [{'id': meta['id'], 'version': meta['version']}],
                   'recorded-at': '2026-11-05T00:00:00Z', 'updated': '2026-11-05T00:00:00Z'})
        new.update(label=spec['new_ref'][4:], body=spec['body'], path='topics/r/' + meta['id'] + '-v' + str(nm['version']) + '.md')
        meta.update({'status': 'superseded', 'state-reason': spec['state_reason'],
                     'superseded-by': [{'id': meta['id'], 'version': nm['version']}]})
        files[target['path']] = lrf.header(meta) + target['body']
        files[new['path']] = lrf.header(nm) + new['body']
        lines[at] = row_text(target['path'], title, meta['scope'], 'superseded')
        lines.insert(at + 1, row_text(new['path'], spec['body'].splitlines()[0].lstrip('# ').strip(), nm['scope'], 'active'))
        rows.append(new)
    elif spec['op'] == 'revoke':
        meta.update({'status': 'revoked', 'state-reason': spec['state_reason']})
        files[target['path']] = lrf.header(meta) + target['body']
        lines[at] = row_text(target['path'], title, meta['scope'], 'revoked')
    elif spec['op'] == 'delete':
        stone = native.tombstones.render({'id': meta['id'], 'scope': pid, 'deleted-at': spec['deleted_at'],
            'deletion-authority': lrf.identity('g1-deletion-authority'), 'reason-class': spec['reason_class']})
        del files[target['path']]
        files[stone['path']] = stone['content']
        lines[at] = row_text(stone['path'], 'Deleted record marker', 'repository', 'deleted')
        rows.remove(target)
    else:
        raise ValueError('Unknown mutation op')
    files[index] = '\n'.join(lines)
    return files, rows


def recorded(meta_of, a, rel, b):
    """Directed canonical assertion check; superseded-by(a,b) is the inverse of supersedes/corrects(b,a)."""
    ma = meta_of(a)
    if ma is None:
        return False
    if rel in {'corrects', 'supersedes', 'superseded-by', 'derived-from'}:
        mb = meta_of(b)
        key = {'id': mb['id'], 'version': mb['version']} if mb else None
        if b.startswith('missing:'):
            key = b  # resolved by caller
        return key in ma.get(rel, []) if isinstance(key, dict) else False
    if rel == 'evidence':
        return b[3:] in ma['evidence']
    if rel == 'scope':
        return ma['scope'] == b[6:]
    if rel == 'condition':
        k, v = b[5:].split('=', 1)
        return k in ma['conditions'] and (ma['conditions'][k] if ma['conditions'][k] is not None else 'unknown') == v
    return False


def validate(am_root, suite, sizes, crosscheck_sizes, emit, only=None):
    problems = Problems()
    native = Native(am_root)
    lrf, snf, template = overlay.load(am_root)
    src = suite['sources']
    for path, expected in src['pinned_files_sha256'].items():
        problems.check(sha256((am_root / path).read_bytes()) == expected, 'source', 'pinned hash mismatch ' + path)
    rows_digest = []
    for top in src['native_package_tree']['roots']:
        for path in sorted((am_root / top).rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts:
                rows_digest.append(path.relative_to(am_root).as_posix() + ' ' + sha256(path.read_bytes()))
    problems.check(sha256('\n'.join(rows_digest)) == src['native_package_tree']['sha256'], 'source', 'native package tree drifted')
    _, _, literal_cases = lrf.corpus(template, 128)
    problems.check(sha256(json.dumps(literal_cases, sort_keys=True, separators=(',', ':'))) == src['literal_24_cases_sha256'],
                   'source', 'literal 24-case labels changed')
    vocab = suite['vocabulary']
    pid = suite['fixture']['project_id']
    problems.check(pid == lrf.PID, 'fixture', 'project id differs from fixture PID')
    cases = suite['cases']
    ids = [c['case_id'] for c in cases]
    problems.check(len(ids) == len(set(ids)), 'suite', 'duplicate case ids')
    records = suite['records']
    now = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')
    report = {'sizes': {}, 'cases': len(cases), 'records': len(records)}
    oracles = {}

    # Static vocabulary and coverage checks.
    tags = set()
    for key, item in suite['coverage'].items():
        hits = sorted(c['case_id'] for c in cases if set(item['tags']) & set(c['covers']))
        problems.check(hits == item['cases'] and hits, 'coverage', key + ' case list stale or empty')
        tags |= set(item['tags'])
    for c in cases:
        where = c['case_id']
        problems.check(set(c['covers']) & tags, where, 'case maps to no coverage requirement')
        av = c['availability']
        problems.check(av['outcome'] in vocab['outcomes'], where, 'unknown outcome')
        problems.check(set(av['exit_any_of']) <= EXIT.get(av['outcome'], set()), where, 'exit codes inconsistent with outcome')
        problems.check(av['no_match_claim'] in {'permitted', 'forbidden'}, where, 'bad no_match_claim')
        problems.check(c['split'] in vocab['splits'] and c['journey'] in {'coding', 'investigation', 'orientation'}, where, 'bad split/journey')
        problems.check(c['rationale'] and len(c['rationale']) > 30, where, 'missing authored rationale')
        if av['outcome'] in {'withheld-replacement', 'qualified-empty-unresolved'} or av['outcome'] == 'non-success':
            problems.check(av['no_match_claim'] == 'forbidden', where, 'withheld/unresolved/non-success must forbid no-match')
        if av['outcome'].startswith('qualified-empty') or av['outcome'] in {'withheld-replacement', 'non-success', 'invalid-request'}:
            problems.check(not c['primary']['required'], where, 'empty outcome cannot require primary guidance')
        if av['outcome'] == 'qualified-guidance':
            problems.check(c['primary']['required'] or c['relationships']['required'] or c['relationships']['allowed']
                           or any(p['tier'] == 'required' for p in c['relationships']['route_pointers']),
                           where, 'qualified-guidance needs a required primary, path or route')
        for path in c['relationships']['disallowed']:
            problems.check(path['reason'] in vocab['disallowed_reasons'], where, 'unknown disallowed reason')
        for path in c['relationships']['required'] + c['relationships']['allowed'] + c['relationships']['disallowed']:
            problems.check(path['rel'] in vocab['relations'], where, 'unknown relation ' + path['rel'])
        if c['operation'].startswith('template:'):
            problems.check(c['template'] and c['template']['seed'] in records, where, 'template seed is not a canonical record')
        if c['context_truth']['evaluation_time_after']:
            problems.check(now > c['context_truth']['evaluation_time_after'], where, 'evaluation time precondition not met now')
        request = {k: v for k, v in c['request'].items() if k != 'ranking'}
        try:
            native.retrieval.request_value(request)
            refused = False
        except native.ContractError:
            refused = True
        if av['outcome'] == 'invalid-request' and not c['context_truth']['supplied'].get('branch'):
            problems.check(refused and native.security.scan(c['request']['query'])['reasons'], where,
                           'native request validation must refuse this unsafe query')
        else:
            problems.check(not refused, where, 'native request validation refused a request expected to run')
        problems.check(av['outcome'] != 'literal-unchanged' or 'ranking' not in c['request'], where, 'literal control has ranking')

    variants = {'base': False, 'overlay': True}
    for size in sizes:
        started = time.perf_counter()
        size_report = {}
        base_files, base_rows, base_info = overlay.bank(am_root, size, overlay=False)
        for variant, with_overlay in variants.items():
            files, rows, info = base_files, base_rows, base_info
            if with_overlay:
                files, rows = overlay.apply(base_files, base_rows, lrf, snf, template)
                info = {'canonical_files': len(files), 'canonical_bytes': sum(len(v.encode()) for v in files.values())}
            bank = Bank(native, files, rows, pid)
            vr = {'canonical_files': info['canonical_files'], 'canonical_bytes': info['canonical_bytes'], 'entries': len(bank.entries),
                  'crosschecked_cases': 0, 'eligible_extras': {}}
            # Canonical identity checks.
            literal = {} if with_overlay else {row['label']: row for row in lrf.corpus(template, 128)[1]}
            for ref, rec in records.items():
                if ref.startswith('mut:') or (rec['partition'] == 'overlay' and not with_overlay):
                    continue
                row = bank.by_label.get(rec['label'])
                if not problems.check(row is not None, ref, 'label absent at size %d %s' % (size, variant)):
                    continue
                m = row['metadata']
                problems.check((m['id'], m['version']) == (rec['id'], rec['version']), ref, 'id/version mismatch')
                problems.check(row['path'] == rec['semantic_navigation_path'], ref, 'path mismatch')
                problems.check(sha256(files[row['path']]) == rec['file_sha256'], ref, 'canonical file hash mismatch')
                problems.check(all(m[f] == rec[f] for f in RECORD_FIELDS), ref, 'canonical metadata mismatch')
                problems.check(bank.parsed[row['path']][0] == 'standalone-record', ref, 'not a native standalone record')
                if literal and rec['labeled_recall_path']:
                    lr = literal[rec['label']]
                    problems.check(lr['path'] == rec['labeled_recall_path'] and lr['metadata'] == m and lr['body'] == row['body'],
                                   ref, 'labeled_recall_fixture copy differs')

            def meta_of(ref, current=bank):
                if ref.startswith('missing:'):
                    return None
                row = current.by_label.get(ref[4:])
                return row['metadata'] if row else None

            # Global invariants.
            gi = suite['global_invariants']
            flagged = sorted('rec:' + bank.label_of[p] for p, r in bank.scan.items() if r and p in bank.label_of)
            hidden = sorted('rec:' + l for l, row in bank.by_label.items() if row['metadata'].get('visibility') == 'hidden')
            derived = sorted(['rec:' + l, 'rec:' + t] for l, row in bank.by_label.items() for d in row['metadata']['derived-from']
                             for t, other in bank.by_label.items() if (other['metadata']['id'], other['metadata']['version']) == (d['id'], d['version']))
            expect = lambda key: sorted(gi[key]['base'] + (gi[key]['overlay'] if with_overlay else []))  # noqa: E731
            problems.check(flagged == expect('native_security_flagged'), variant, 'security-flagged set differs: %s' % flagged)
            problems.check(hidden == expect('hidden_visibility'), variant, 'hidden set differs: %s' % hidden)
            problems.check(derived == sorted(gi['recorded_derived_from']['base'] + (gi['recorded_derived_from']['overlay'] if with_overlay else [])),
                           variant, 'recorded derived-from set differs: %s' % derived)
            evidence = Counter(e for row in bank.by_label.values() if not row['label'].startswith('g1-') for e in row['metadata']['evidence'])
            problems.check(all(v == 1 for v in evidence.values()) and all(len(row['metadata']['evidence']) == 1 for row in bank.by_label.values()
                           if not row['label'].startswith('g1-')), variant, 'base evidence not unique')
            root_routes = {route['path'] for route in bank.routes}

            for c in cases:
                if (c['partition'] == 'overlay' and not with_overlay) or (only and c['case_id'] not in only):
                    continue
                where = '%s@%d/%s' % (c['case_id'], size, variant)
                truth = c['context_truth']
                current = bank
                if truth['mutation'] and truth['stage'] == 'after-rebuild':
                    mfiles, mrows = mutate(native, lrf, snf, files, rows, suite['mutations'][truth['mutation']], pid)
                    current = Bank(native, mfiles, mrows, pid)
                    for ref, rec in records.items():
                        if ref.startswith('mut:') and rec['exists_after_mutation'] == truth['mutation']:
                            row = current.by_label.get(rec['label'])
                            problems.check(row and row['path'] == rec['semantic_navigation_path'] and
                                           (row['metadata']['id'], row['metadata']['version']) == (rec['id'], rec['version']),
                                           where, 'mutation record identity mismatch')
                request = {k: v for k, v in c['request'].items() if k != 'ranking'}
                try:
                    context = native.observed(truth['supplied'], truth['observed_git_branch'])
                except native.ContractError:
                    problems.check(c['availability']['outcome'] == 'invalid-request', where, 'native refused branch substitution unexpectedly')
                    continue
                problems.check(c['availability']['outcome'] != 'invalid-request' or not truth['supplied'].get('branch'), where,
                               'branch substitution was not refused natively')
                if c['availability']['outcome'] == 'invalid-request':
                    continue

                def path_of(ref):
                    if ref.startswith('ctx:'):
                        return ref[4:]
                    row = current.by_label.get(ref[4:])
                    return row['path'] if row else None
                deferred = frozenset(path_of(ref) for ref in truth['deferred'])
                eligible, reasons = current.eligibility(request, context, now, deferred)
                if size in crosscheck_sizes:
                    ok, nat, mine = current.native_crosscheck(request, context, now, deferred, eligible, reasons)
                    problems.check(ok, where, 'per-record eligibility differs from native evaluate_sources: %s vs %s' % (nat, mine))
                    vr['crosschecked_cases'] += 1
                eset = set(eligible)
                prim = c['primary']
                for ref in prim['required'] + prim['tolerated']:
                    problems.check(path_of(ref) in eset, where, ref + ' required/tolerated but natively ineligible: %s'
                                   % reasons.get(path_of(ref)))
                expanded_forbidden = []
                for item in prim['forbidden']:
                    if 'glob' in item:
                        matched = [l for l in current.by_label if fnmatch.fnmatch(l, item['glob']) and 'rec:' + l not in item['except']]
                        problems.check(matched or (item['glob'] == 'g1-*' and not with_overlay), where, 'forbidden glob matched nothing')
                        expanded_forbidden += [('rec:' + l, item['reason']) for l in matched]
                    else:
                        expanded_forbidden.append((item['ref'], item['reason']))
                # A reason may be one native exclusion reason or a list of acceptable ones.
                expanded_forbidden = [(ref, [reason] if isinstance(reason, str) else reason) for ref, reason in expanded_forbidden]
                for ref, reason in expanded_forbidden:
                    path = path_of(ref)
                    problems.check(path not in eset and set(reason) & set(reasons.get(path, [])), where,
                                   '%s forbidden for %s but native reasons are %s' % (ref, reason, reasons.get(path)))
                if c['partition'] == 'base' and with_overlay:
                    leaked = [p for p in eligible if current.label_of.get(p, '').startswith('g1-')]
                    problems.check(not leaked, where, 'overlay record eligible in base case')
                outcome = c['availability']['outcome']
                if outcome == 'withheld-replacement':
                    problems.check(any('no-active-superseder' in r for _, r in expanded_forbidden), where, 'withheld outcome without superseded obsolete record')
                if outcome == 'qualified-empty-unresolved':
                    problems.check(any(x.startswith(UNRESOLVED) for _, r in expanded_forbidden for x in r) or c['mandatory_caveats'], where,
                                   'unresolved outcome without a withheld/unknown source')
                if outcome in {'literal-unchanged', 'literal-fallback'}:
                    selected = {current.label_of.get(p, p) for p in eligible
                                if native.retrieval.match(request, {'path': p, 'body': current.parsed[p][2] if p in current.parsed else current.files[p],
                                                                    'record': current.parsed.get(p, (None, {}, None))[1]})}
                    problems.check({'rec:' + l for l in selected} == set(prim['required']), where,
                                   'native literal selection %s differs from label' % sorted(selected))
                if request['ids'] or request['paths']:
                    chosen = {p for p in eligible if native.retrieval.match(request, {'path': p, 'body': '',
                              'record': current.parsed[p][1] if p in current.parsed else {}})}
                    problems.check(chosen == {path_of(r) for r in prim['required']}, where,
                                   'exact selector domain %s differs from required' % sorted(chosen))
                extras = sorted(current.label_of.get(p, p) for p in eligible
                                if not any(path_of(r) == p for r in prim['required'] + prim['tolerated']))
                vr['eligible_extras'][c['case_id']] = len(extras)
                # Paraphrase class.
                qtokens = tokens(c['request']['query'])
                texts = {ref: current.files[path_of(ref)].split('\n---\n', 1)[-1] if ref.startswith('rec:') or ref.startswith('mut:')
                         else current.files[path_of(ref)] for ref in prim['required']}
                overlap = {ref: sorted(qtokens & tokens(text)) for ref, text in texts.items()}
                if c['paraphrase_class'] == 'no-content-overlap':
                    problems.check(all(not v for v in overlap.values()), where, 'claimed no overlap but shares %s' % overlap)
                elif c['paraphrase_class'] == 'partial-overlap':
                    literal_hit = [ref for ref in prim['required'] if native.retrieval.match(request, {'path': path_of(ref), 'body': texts[ref]})]
                    problems.check(any(overlap.values()) and len(literal_hit) < len(prim['required']), where,
                                   'partial-overlap claim inconsistent (overlap %s, literal %s)' % (overlap, literal_hit))
                # Mandatory caveats.
                for cav in c['mandatory_caveats']:
                    t = cav['type']
                    if t == 'condition':
                        m = meta_of(cav['ref'], current)
                        problems.check(m and m['conditions'].get(cav['key']) == cav['value'] and cav['ref'] in prim['required'], where, 'condition caveat invalid')
                    elif t == 'scope':
                        problems.check(meta_of(cav['ref'], current)['scope'] == cav['scope'], where, 'scope caveat invalid')
                    elif t == 'correction':
                        problems.check(recorded(lambda r: meta_of(r, current), cav['ref'], 'corrects', cav['corrects'])
                                       and cav['ref'] in prim['required'], where, 'correction caveat not recorded')
                    elif t == 'conflict-group':
                        a, b = cav['refs']
                        ma, mb = meta_of(a, current), meta_of(b, current)
                        same_title = current.by_label[a[4:]]['body'].splitlines()[0] == current.by_label[b[4:]]['body'].splitlines()[0]
                        linked = any(recorded(lambda r: meta_of(r, current), x, rel, y) for x, y in ((a, b), (b, a))
                                     for rel in ('corrects', 'supersedes', 'superseded-by', 'derived-from'))
                        problems.check(set(cav['refs']) <= set(prim['required']) and same_title and not linked and ma and mb, where, 'conflict group invalid')
                    elif t == 'independent-evidence':
                        sets = [set(meta_of(r, current)['evidence']) for r in cav['refs']]
                        problems.check(not set.intersection(*sets) and set(cav['refs']) <= set(prim['required']), where, 'evidence not independent')
                    elif t == 'in-body':
                        problems.check(cav['text'] in current.by_label[cav['ref'][4:]]['body'] and path_of(cav['ref']) in eset, where, 'in-body caveat absent')
                    elif t == 'orientation-only':
                        problems.check(current.parsed.get(path_of(cav['ref'])) is None and path_of(cav['ref']) in eset, where, 'orientation caveat invalid')
                    elif t == 'unresolved-replacement':
                        label, version = cav['missing'][8:].rsplit('@', 1)
                        m = meta_of(cav['ref'], current)
                        target = {'id': current.by_label[label]['metadata']['id'], 'version': int(version)}
                        exists = any((r['metadata']['id'], r['metadata']['version']) == (target['id'], target['version']) for r in current.by_label.values())
                        problems.check(target in m.get('superseded-by', []) and not exists, where, 'unresolved replacement invalid')
                    elif t != 'assurance':
                        problems.check(False, where, 'unknown caveat type')
                # Relationships.
                for hop in c['relationships']['required'] + c['relationships']['allowed']:
                    a, rel, b = hop['from'], hop['rel'], hop['to']
                    if rel == 'co-routed':
                        ok = bool(current.parents.get(path_of(a), set()) & current.parents.get(path_of(b), set())) and path_of(b) in eset
                    elif b.startswith('missing:'):
                        label, version = b[8:].rsplit('@', 1)
                        target = {'id': current.by_label[label]['metadata']['id'], 'version': int(version)}
                        ok = target in meta_of(a, current).get(rel, [])
                    else:
                        ok = recorded(lambda r: meta_of(r, current), a, rel, b)
                    problems.check(ok and hop.get('asserted_by', a) == a, where, 'allowed/required hop not canonical: %s' % hop)
                for hop in c['relationships']['disallowed']:
                    a, rel, b, why = hop['from'], hop['rel'], hop['to'], hop['reason']
                    mb = meta_of(b, current) if b.startswith(('rec:', 'mut:')) else None
                    pb = path_of(b) if b.startswith(('rec:', 'mut:')) else None
                    fwd = rel in {'corrects', 'supersedes', 'superseded-by', 'derived-from', 'evidence'} and recorded(lambda r: meta_of(r, current), a, rel, b)
                    if why == 'not-recorded' or why == 'inferred-as-recorded':
                        ok = not fwd and (rel != 'inferred-similarity' or hop.get('channel') == 'recorded')
                    elif why == 'wrong-direction':
                        ok = not fwd and recorded(lambda r: meta_of(r, current), b, rel, a)
                    elif why == 'endpoint-ineligible':
                        ok = pb not in eset and pb is not None
                    elif why == 'endpoint-hidden':
                        ok = any((m or {}).get('visibility') == 'hidden' for m in (meta_of(a, current), mb))
                    elif why == 'endpoint-unsafe':
                        ok = any(bool(current.scan.get(p)) for p in (path_of(a), pb) if p)
                    elif why == 'missing-endpoint':
                        label, version = b[8:].rsplit('@', 1)
                        ok = not any((r['metadata']['id'], r['metadata']['version']) == (current.by_label[label]['metadata']['id'], int(version))
                                     for r in current.by_label.values())
                    elif why == 'inactive-as-current':
                        ok = pb not in eset and any(x in reasons.get(pb, []) for x in ('active', 'no-active-superseder'))
                    elif why == 'shared-provenance-not-relevance':
                        field, value = hop['via'][7:].split('=', 1)
                        ok = meta_of(a, current)['origin'].get(field) == value == mb['origin'].get(field)
                    else:
                        ok = False
                    problems.check(ok, where, 'disallowed hop reason not borne out: %s' % hop)
                if c['operation'] == 'template:component-attempts' and (c.get('availability', {}).get('no_attempt_records_in_component')):
                    comp = c['availability']['no_attempt_records_in_component']
                    attempts = [l for l, r in current.by_label.items() if r['metadata']['kind'] == 'episodic-attempt' and r['metadata']['scope'] == 'component:' + comp]
                    problems.check(not attempts, where, 'component unexpectedly has attempt records')
                for pointer in c['relationships']['route_pointers']:
                    problems.check(pointer['ref'][6:] in root_routes, where, 'route pointer is not an actual root route')
                # Disclosure consistency.
                disc = c['disclosure']
                permitted = set(prim['required'] + prim['tolerated'])
                for ref in disc['forbidden_all'] + disc['forbidden_body']:
                    problems.check(ref not in permitted, where, ref + ' both permitted and forbidden')
                for ref in disc['forbidden_all']:
                    path = path_of(ref)
                    problems.check(path is None or path not in eset, where, ref + ' forbidden_all but eligible')
                sendable = set(expect('native_security_flagged')) | set(expect('hidden_visibility'))
                if truth['mutation'] == 'delete-receipt-b':
                    sendable.add(suite['mutations']['delete-receipt-b']['target'])
                problems.check(set(disc['provider_send_forbidden']) <= sendable, where, 'provider_send_forbidden not grounded in native flags')
                if emit is not None and variant == 'overlay':
                    def strings(ref, body_only=False):
                        row = None
                        if ref.startswith(('rec:', 'mut:')):
                            row = current.by_label.get(ref[4:]) or bank.by_label.get(ref[4:])
                        if row is None:
                            return []
                        body = [s.strip() for s in re.split(r'(?<=[.!?])\s+|\n', row['body']) if len(s.strip()) > 12 and not s.startswith('#')]
                        if body_only:
                            return body
                        shared = any(r['metadata']['id'] == row['metadata']['id'] and 'rec:' + l in permitted for l, r in current.by_label.items())
                        return body + [row['path'], row['body'].splitlines()[0].lstrip('# ').strip()] + row['metadata']['evidence'] + ([] if shared else [row['metadata']['id']])
                    oracles.setdefault(str(size), {})[c['case_id']] = {
                        'required': [{'ref': r, 'path': path_of(r), 'id': (meta_of(r, current) or {}).get('id'),
                                      'version': (meta_of(r, current) or {}).get('version')} for r in prim['required']],
                        'tolerated_paths': [path_of(r) for r in prim['tolerated']],
                        'forbidden_primary_paths': sorted({path_of(r) for r, _ in expanded_forbidden if path_of(r)}),
                        'forbidden_strings': sorted({s for r in disc['forbidden_all'] for s in strings(r)}),
                        'forbidden_body_strings': sorted({s for r in disc['forbidden_body'] for s in strings(r, True)}),
                        'eligible_domain_size': len(eligible)}
            size_report[variant] = vr
        size_report['seconds'] = round(time.perf_counter() - started, 3)
        report['sizes'][str(size)] = size_report
    report['problems'] = list(problems)
    report['valid'] = not problems
    return report, oracles


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--am-root', required=True)
    parser.add_argument('--suite', default=str(HERE / 'suite.json'))
    parser.add_argument('--sizes', nargs='+', type=int, default=[1000])
    parser.add_argument('--crosscheck-sizes', nargs='*', type=int, default=[1000])
    parser.add_argument('--report')
    parser.add_argument('--emit-oracles')
    parser.add_argument('--cases', nargs='*', help='Optional case-id subset for targeted large-size checks')
    args = parser.parse_args()
    suite_bytes = Path(args.suite).read_bytes()
    suite = json.loads(suite_bytes)
    report, oracles = validate(Path(args.am_root).resolve(), suite, args.sizes, set(args.crosscheck_sizes),
                               {} if args.emit_oracles else None, set(args.cases or ()))
    report = {'schema': 'agent-memory/g1-independent-suite-validation-v1', 'suite_sha256': sha256(suite_bytes),
              'validator_sha256': sha256(Path(__file__).read_bytes()),
              'overlay_sha256': sha256((HERE / 'g1_relationship_overlay.py').read_bytes()),
              'am_root': str(Path(args.am_root).resolve()), 'scope': 'light canonical/native-eligibility validation; no retrieval ranking, graph, embedding or model execution',
              **report}
    text = json.dumps(report, indent=1) + '\n'
    if args.report:
        Path(args.report).write_text(text)
    if args.emit_oracles:
        Path(args.emit_oracles).write_text(json.dumps({'suite_sha256': report['suite_sha256'], 'oracles': oracles}, indent=1) + '\n')
    print(json.dumps({k: report[k] for k in ('valid', 'cases', 'records', 'sizes')} | {'problems': report['problems'][:40]}, indent=1))
    return 0 if report['valid'] else 1


if __name__ == '__main__':
    sys.exit(main())
