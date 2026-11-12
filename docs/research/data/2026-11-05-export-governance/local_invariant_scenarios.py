#!/usr/bin/env python3
"""Disposable #247 research scenarios against CURRENT local invariants.

Research evidence only. This is not a product test, federation prototype, export
engine or cross-project transfer. Every bank, project UUID, developer and body is
synthetic and lives in a throwaway tests/world.py World under /tmp. No real or
private bank is read, nothing is pushed, and no network or provider is used.

Part A drives the real portable helpers (retrieve, inactive_records,
projection, tombstones) to observe the local guarantees that any future export
policy must not weaken. Part B is an executable model of the PROPOSED #247
authority/precedence/receipt rules (hypothetical policy, not shipped code) used
only to check the brief's decision tables for internal consistency.

Run from the repository root:  python3 -I <this file>  (prints one JSON report)
"""
import itertools
import json
from pathlib import Path
import shutil
import sys
import uuid

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'tests'))
from world import World  # noqa: E402
from identity_fixtures import bind  # noqa: E402
from record_fixtures import record_text  # noqa: E402

CANARY = 'EXPORT_RESEARCH_CANARY_7f3c'
SECRET = 'api_key = SYNTHETICVALUE9876543210'


def unpack(text):
    header, body = text[4:].split('\n---\n', 1)
    return {k: json.loads(v) for k, v in (line.split(': ', 1) for line in header.splitlines())}, body


def render(metadata, body):
    return '---\n' + ''.join(k + ': ' + json.dumps(v) + '\n' for k, v in metadata.items()) + '---\n' + body


class Lab:
    def __init__(self, w):
        self.w = w
        self.pack = w.inside('portable pack')
        shutil.copytree(ROOT / 'skills/agent-memory', self.pack, ignore=shutil.ignore_patterns('__pycache__'))
        self.n = 0

    def run(self, helper, *args):
        r = self.w.run([sys.executable, '-I', self.pack / 'scripts' / helper, *map(str, args)], check=False)
        try:
            value = json.loads(r.stdout) if r.stdout else None
        except ValueError:
            value = None
        return r.returncode, value, r.stdout

    def save(self, value):
        self.n += 1
        path = self.w.inside('input-%d.json' % self.n)
        path.write_text(json.dumps(value))
        return path

    def project(self, name):
        repo = self.w.repository(name)
        (repo / 'README.md').write_text('# Synthetic ' + name + '\n')
        self.w.git('add', '.', cwd=repo)
        self.w.git('commit', '-m', 'Disposable synthetic project', cwd=repo)
        project = str(uuid.uuid4())
        proposal = json.loads((self.pack / 'templates/router-proposal.example.json').read_text())
        proposal['project-id'] = project
        code, rendered, _ = self.run('bank_contract.py', 'render', '--input', self.save(proposal))
        assert code == 0, rendered
        bank = repo / 'memory-bank'
        bank.mkdir()
        for path, body in rendered['files'].items():
            (bank / path).write_text(body)
        actor = bind(self.w, repo)['developer_id']
        return repo, bank, project, actor

    def add(self, bank, project, actor, name, body, *, status='active', **updates):
        metadata, _ = unpack(record_text(body, project, status=status))
        metadata['origin'].update({'developer-id': actor, 'harness': 'research-fixture', 'session': 'synthetic-session'})
        metadata.update(updates)
        path = 'topics/' + name + '.md'
        (bank / 'topics').mkdir(exist_ok=True)
        (bank / path).write_text(render(metadata, body))
        row = '| [' + path + '](<' + path + '>) | Synthetic lesson | ' + metadata['scope'] + ' | ' + status + ' | Before qualified recall |\n'
        router = bank / 'MEMORY.md'
        router.write_text(router.read_text().replace('\n## Omitted contexts', row + '\n## Omitted contexts'))
        return metadata, path

    def retrieve(self, repo, project, query='retry'):
        request = {'project_id': project, 'query': query, 'ids': [], 'paths': [], 'context': {},
                   'kinds': ['semantic-fact'], 'minimum_trust': 'reviewed', 'limit': 16, 'include_context': False}
        return self.run('retrieve.py', '--repo', repo, '--request', self.save(request))


def state(bank):
    return {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}


def part_a():
    out = {}
    with World() as w:
        lab = Lab(w)
        s_repo, s_bank, s_proj, actor = lab.project('source-s')
        d_repo, d_bank, d_proj, actor_d = lab.project('destination-d')
        body = '# Retry lesson\n\nSynthetic retry guidance ' + CANARY + '.\n'
        classes = {}
        classes['active-reviewed-visible'] = lab.add(s_bank, s_proj, actor, 'active', body)
        classes['secret-bearing-active'] = lab.add(s_bank, s_proj, actor, 'secret', '# Retry lesson\n\nretry ' + SECRET + '\n')
        classes['hidden-active'] = lab.add(s_bank, s_proj, actor, 'hidden', body, visibility='hidden')
        classes['unknown-applicability'] = lab.add(s_bank, s_proj, actor, 'unknown', body, applicability='unknown')
        classes['branch-conditional-other-branch'] = lab.add(s_bank, s_proj, actor, 'branch', body, conditions={'branch': 'feature-x'})
        classes['revoked'] = lab.add(s_bank, s_proj, actor, 'revoked', body, status='revoked')
        classes['expired'] = lab.add(s_bank, s_proj, actor, 'expired', body, status='expired')
        classes['untrusted-active'] = lab.add(s_bank, s_proj, actor, 'untrusted', body, trust='untrusted')
        d_meta, _ = lab.add(d_bank, d_proj, actor_d, 'local', '# Retry lesson\n\nDestination-local retry note.\n')

        # A1: which classes does CURRENT local recall return at all?
        code, v, raw = lab.retrieve(s_repo, s_proj)
        returned = {item['record']['id'] for item in (v or {}).get('results', []) if 'record' in item}
        out['A1_local_eligibility'] = {
            'exit': code,
            'returned_by_class': {k: (m['id'] in returned) for k, (m, _) in classes.items()},
            'excluded_count_keys': sorted((v or {}).get('excluded_counts', {})),
            'excluded_counts': (v or {}).get('excluded_counts'),
            'metrics_inventory_files_present': 'inventory_files' in (v or {}).get('metrics', {}),
            'secret_value_echoed': 'SYNTHETICVALUE9876543210' in raw,
        }

        # A2: project isolation; a foreign project UUID is refused by both banks.
        foreign = {}
        for label, repo, project in (('source-bank-with-destination-id', s_repo, d_proj),
                                     ('destination-bank-with-source-id', d_repo, s_proj)):
            code, v, raw = lab.retrieve(repo, project)
            foreign[label] = {'exit': code, 'results': None if v is None else v.get('results'), 'canary_echoed': CANARY in raw}
        ref = {'id': classes['active-reviewed-visible'][0]['id'], 'version': 1}
        code, v, raw = lab.run('inactive_records.py', 'filter', '--repo', d_repo, '--input', lab.save({'project_id': s_proj, 'records': [ref]}))
        foreign['destination-guard-with-source-reference'] = {'exit': code, 'canary_echoed': CANARY in raw}
        code, v, raw = lab.run('inactive_records.py', 'filter', '--repo', d_repo, '--input', lab.save({'project_id': d_proj, 'records': [ref]}))
        foreign['destination-guard-foreign-id-under-destination-project'] = {
            'exit': code, 'remaining': None if v is None else v.get('remaining_references'),
            'discarded': None if v is None else v.get('discarded')}
        out['A2_project_isolation'] = foreign

        # A3: version-bound reference, then a direct canonical revoke edit (simulates
        # a governed revoke; the governance command itself is not exercised here).
        a_meta, a_path = classes['active-reviewed-visible']
        code, before, _ = lab.run('inactive_records.py', 'filter', '--repo', s_repo,
                                  '--input', lab.save({'project_id': s_proj, 'records': [ref, {'id': ref['id'], 'version': 2}]}))
        out['A3_version_binding'] = {'exit': code, 'error': before and before.get('error'),
                                     'remaining': before and before.get('remaining_references'),
                                     'discarded': before and before.get('discarded')}

        # A4: projection built under "grant v1", then deletion, then stale commit.
        code, built, _ = lab.run('projection.py', 'build', '--repo', s_repo, '--input', lab.save({'project_id': s_proj, 'records': [ref]}))
        assert code == 0, built
        recipient_copy = w.inside('recipient-retained-copy.json')
        recipient_copy.write_text(json.dumps({'disclosed_body': (s_bank / a_path).read_text()}))
        stale_projection = {'project_id': s_proj, 'records': [{'id': ref['id'], 'body': (s_bank / a_path).read_text(), 'conditions': ['branch:old']},
                                                            {'id': classes['revoked'][0]['id'], 'body': 'surviving id only'}]}
        (s_bank / a_path).unlink()
        tomb = {'id': ref['id'], 'scope': s_proj, 'deleted-at': '2026-11-05T00:00:00Z',
                'deletion-authority': str(uuid.uuid4()), 'reason-class': 'user-request'}
        code, rendered, _ = lab.run('tombstones.py', 'render', '--input', lab.save(tomb))
        assert code == 0, rendered
        (s_bank / rendered['path']).parent.mkdir(parents=True, exist_ok=True)
        (s_bank / rendered['path']).write_text(rendered['content'])
        router = s_bank / 'MEMORY.md'
        lines = [l for l in router.read_text().split('\n') if a_path not in l]
        text = '\n'.join(lines).replace('\n## Omitted contexts', '| [' + rendered['path'] + '](<' + rendered['path'] + '>) | Deletion guard | repository | deleted | Before rebuilding projections |\n\n## Omitted contexts')
        router.write_text(text)
        code_check, check, raw_check = lab.run('projection.py', 'check', '--repo', s_repo, '--input', lab.save(built['projection']))
        rebuilds = []
        for conditions in (['branch:old'], ['repository'], ['component:other']):
            stale_projection['records'][0]['conditions'] = conditions
            c, g, rawg = lab.run('tombstones.py', 'filter', '--repo', s_repo, '--input', lab.save(stale_projection))
            rebuilds.append({'conditions': conditions, 'exit': c, 'suppressed': g and g.get('suppressed'),
                             'kept_ids': [r['id'] for r in (g or {}).get('records', [])], 'canary_echoed': CANARY in rawg})
        c, g, _ = lab.run('inactive_records.py', 'filter', '--repo', s_repo, '--input', lab.save({'project_id': s_proj, 'records': [ref]}))
        code_r, rv, raw_r = lab.retrieve(s_repo, s_proj)
        out['A4_delete_stale_builder_non_resurrection'] = {
            'stale_projection_check_exit': code_check, 'stale_projection_check_error': check and check.get('error'),
            'stale_check_canary_echoed': CANARY in raw_check,
            'tombstone_rebuilds_under_changed_conditions': rebuilds,
            'inactive_filter_after_delete': {'exit': c, 'remaining': g and g['remaining_references'], 'discarded': g and g['discarded']},
            'recall_after_delete_returns_deleted_id': ref['id'] in {i['record']['id'] for i in (rv or {}).get('results', []) if 'record' in i},
            'tombstone_fields': sorted(rendered['metadata']), 'tombstone_contains_canary': CANARY in rendered['content'],
            'recipient_retained_copy_still_contains_canary': CANARY in recipient_copy.read_text(),
        }

        # A5: tombstone refuses content-bearing or fingerprint fields without echo.
        refusals = {}
        for extra in ('body', 'content-sha256', 'title', 'source-project-name'):
            c, v, raw = lab.run('tombstones.py', 'render', '--input', lab.save({**tomb, 'id': str(uuid.uuid4()), extra: CANARY}))
            refusals[extra] = {'exit': c, 'canary_echoed': CANARY in raw}
        out['A5_tombstone_content_free'] = refusals

        # A6: unavailable or interrupted source is an error, never a clean empty success.
        unavailable = {}
        marker = s_bank / '.agent-memory.lock'
        marker.write_text('synthetic interruption')
        c, v, raw = lab.retrieve(s_repo, s_proj)
        unavailable['interrupted-source-lock-recall'] = {'exit': c, 'results_key_present': bool(v) and 'results' in v}
        c, v, raw = lab.run('tombstones.py', 'filter', '--repo', s_repo, '--input', lab.save(stale_projection))
        unavailable['interrupted-source-lock-rebuild-guard'] = {'exit': c, 'canary_echoed': CANARY in raw}
        marker.unlink()
        d_before = state(d_bank)
        moved = w.inside('source-s-offline')
        s_repo.rename(moved)
        c, v, raw = lab.retrieve(s_repo, s_proj)
        unavailable['missing-source-repository-recall'] = {'exit': c, 'results_key_present': bool(v) and 'results' in v}
        c, v, raw = lab.retrieve(d_repo, d_proj)
        unavailable['destination-local-recall-while-source-offline'] = {
            'exit': c, 'returns_destination_record': d_meta['id'] in {i['record']['id'] for i in (v or {}).get('results', []) if 'record' in i},
            'optional_indexes_used': v and v.get('optional_indexes_used')}
        unavailable['destination-bank-bytes-unchanged'] = state(d_bank) == d_before
        moved.rename(s_repo)
        out['A6_outage_and_local_independence'] = unavailable

    # A7: a foreign identity written into canonical derived-from (the deletion
    # closure edge) in a separate disposable world: does local recall survive?
    with World() as w:
        lab = Lab(w)
        d_repo, d_bank, d_proj, actor = lab.project('destination-d')
        missing_foreign = {'id': str(uuid.uuid4()), 'version': 1}
        meta, _ = lab.add(d_bank, d_proj, actor, 'dangling', '# Retry lesson\n\nretry note citing a foreign record.\n',
                          **{'derived-from': [missing_foreign]})
        plain, _ = lab.add(d_bank, d_proj, actor, 'plain', '# Retry lesson\n\nretry note without relations.\n')
        c, v, raw = lab.retrieve(d_repo, d_proj)
        ids = {i['record']['id'] for i in (v or {}).get('results', []) if 'record' in i}
        out['A7_foreign_id_in_canonical_derived_from'] = {
            'exit': c, 'error': None if v is None else v.get('error'),
            'dangling_record_returned': meta['id'] in ids, 'plain_record_returned': plain['id'] in ids,
            'result_reports_relation_unavailable': 'unavailable' in raw.lower()}

    # A8: unadmitted material has no canonical route at all. Routing a candidate or
    # quarantined record into the shared bank invalidates the whole bank (fail closed).
    unadmitted = {}
    for status in ('candidate', 'quarantined'):
        with World() as w:
            lab = Lab(w)
            repo, bank, proj, actor = lab.project('source-s')
            good, _ = lab.add(bank, proj, actor, 'active', '# Retry lesson\n\nretry ok.\n')
            lab.add(bank, proj, actor, status, '# Retry lesson\n\nretry ' + CANARY + '\n', status=status)
            c, v, raw = lab.retrieve(repo, proj)
            c2, v2, raw2 = lab.run('inactive_records.py', 'filter', '--repo', repo,
                                   '--input', lab.save({'project_id': proj, 'records': [{'id': good['id'], 'version': 1}]}))
            unadmitted[status] = {'recall_exit': c, 'recall_has_results': bool(v) and 'results' in v,
                                  'derived_guard_exit': c2, 'canary_echoed': CANARY in raw + raw2}
    out['A8_unadmitted_routed_into_canonical_bank'] = unadmitted

    # A9: naive import of a verbatim foreign record into a destination's canonical
    # bank (the anti-pattern a federation must avoid).
    with World() as w:
        lab = Lab(w)
        s_repo, s_bank, s_proj, actor = lab.project('source-s')
        d_repo, d_bank, d_proj, actor_d = lab.project('destination-d')
        foreign_meta, foreign_path = lab.add(s_bank, s_proj, actor, 'exported', '# Retry lesson\n\nretry ' + CANARY + '\n')
        local_meta, _ = lab.add(d_bank, d_proj, actor_d, 'local', '# Retry lesson\n\nretry local.\n')
        c0, v0, _ = lab.retrieve(d_repo, d_proj)
        (d_bank / 'topics/imported.md').write_bytes((s_bank / foreign_path).read_bytes())
        router = d_bank / 'MEMORY.md'
        router.write_text(router.read_text().replace('\n## Omitted contexts',
            '| [topics/imported.md](<topics/imported.md>) | Imported foreign lesson | repository | active | Before qualified recall |\n\n## Omitted contexts'))
        c1, v1, raw1 = lab.retrieve(d_repo, d_proj)
        out['A9_verbatim_foreign_record_in_destination_bank'] = {
            'destination_recall_exit_before': c0, 'destination_recall_exit_after': c1,
            'destination_local_record_still_returned': local_meta['id'] in {i['record']['id'] for i in (v1 or {}).get('results', []) if 'record' in i},
            'canary_echoed': CANARY in raw1}
    return out


# ---------------------------------------------------------------------------
# Part B: model of the PROPOSED policy (hypothetical; checks table consistency).
AUTH = ('M', 'D', 'R', 'X', 'P')


def effective_mode(session, project):
    """ADR-0023 D4: session overrides project; absent means ask."""
    if session is not None:
        return session
    return project if project is not None else 'ask'


def expansion_allowed(grants, mode, receipt_valid, needs_provider):
    base = all(grants[a] is True for a in ('M', 'D', 'R', 'X'))
    provider = grants['P'] is True if needs_provider else True
    e = receipt_valid or mode == 'allow'
    return base and provider and e


def receipt_valid(receipt, request):
    return receipt is not None and all(receipt[k] == request[k] for k in (
        'session', 'project', 'developer', 'origin', 'destinations', 'query_digest', 'operation',
        'policy_version', 'authority_versions')) and request['page'] <= receipt['page_bound'] and not request['after_deny_or_cancel'] and request['now'] < receipt['expiry']


def part_b():
    values = (True, False, None)  # None = unknown/unavailable
    rows = violations = override_only_e = 0
    for combo in itertools.product(values, repeat=len(AUTH)):
        grants = dict(zip(AUTH, combo))
        for mode in ('ask', 'allow'):
            for receipt in (True, False):
                for provider in (True, False):
                    rows += 1
                    allowed = expansion_allowed(grants, mode, receipt, provider)
                    needed = ('M', 'D', 'R', 'X') + (('P',) if provider else ())
                    if allowed and not all(grants[a] is True for a in needed):
                        violations += 1
                    if mode == 'allow' and not receipt and allowed:
                        override_only_e += 1
    precedence = {}
    for s in (None, 'ask', 'allow'):
        for p in (None, 'ask', 'allow'):
            precedence['session=%s,project=%s' % (s, p)] = effective_mode(s, p)
    base = {'session': 's1', 'project': 'p1', 'developer': 'd1', 'origin': 'p1', 'destinations': ('p2',),
            'query_digest': 'q1', 'operation': 'explore', 'policy_version': 3, 'authority_versions': (1, 1, 1, 1, 1),
            'page': 1, 'after_deny_or_cancel': False, 'now': 10}
    receipt = {**base, 'page_bound': 3, 'expiry': 20}
    changes = {'unchanged-page-2-retry': {'page': 2}, 'page-beyond-bound': {'page': 4},
               'changed-query': {'query_digest': 'q2'}, 'added-destination': {'destinations': ('p2', 'p3')},
               'policy-revision': {'policy_version': 4}, 'grant-revoked-version-bump': {'authority_versions': (1, 1, 1, 2, 1)},
               'other-parallel-session': {'session': 's2'}, 'new-session-after-reset': {'session': 's3'},
               'other-developer-same-project': {'developer': 'd2'}, 'other-origin-project': {'project': 'p9', 'origin': 'p9'},
               'retry-after-denial': {'after_deny_or_cancel': True}, 'expired': {'now': 25}}
    reuse = {name: receipt_valid(receipt, {**base, **delta}) for name, delta in changes.items()}
    return {'authority_rows_evaluated': rows, 'rows_allowed_without_all_required_authorities': violations,
            'rows_allowed_by_override_without_receipt': override_only_e,
            'override_rows_expected_formula': 'count of rows where M,D,R,X true, P true-or-not-needed, mode=allow, receipt=False',
            'precedence': precedence, 'receipt_reuse': reuse}


if __name__ == '__main__':
    print(json.dumps({'schema': 'am-research/247-local-invariant-scenarios-v1', 'part_a_current_source': part_a(),
                      'part_b_proposed_policy_model': part_b()}, indent=2, sort_keys=True))
