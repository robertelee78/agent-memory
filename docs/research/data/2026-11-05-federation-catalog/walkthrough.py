"""Disposable #246 catalog-research walkthrough.

Part 1 exercises CURRENT public identity/scope/retrieval helpers in a disposable
tests/world.py World with synthetic projects (no network, no real banks, no
transfer). Part 2 is arithmetic over the PROPOSED authority rule from
docs/research/v0.0.15-federation-catalog.md; it evaluates research logic only
and is not a runtime feature pass. Run from the repository root:

    python3 docs/research/data/2026-11-05-federation-catalog/walkthrough.py
"""
import itertools
import json
from pathlib import Path
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'tests'))
from world import World  # noqa: E402

SCRIPTS = ROOT / 'scripts'
LABELS = {}


def label(value):
    if isinstance(value, str) and value in LABELS:
        return LABELS[value]
    return value


def scrub(value):
    """Replace random IDs and disposable paths with stable labels."""
    if isinstance(value, dict):
        return {k: scrub(v) for k, v in value.items()}
    if isinstance(value, list):
        return [scrub(v) for v in value]
    if isinstance(value, str):
        for raw, name in LABELS.items():
            value = value.replace(raw, name)
    return value


def cmd(w, name, *args):
    r = w.run([sys.executable, '-I', SCRIPTS / (name + '.py'), *args], check=False)
    try:
        value = json.loads(r.stdout) if r.stdout else None
    except ValueError:
        value = None
    return r.returncode, value, r.stdout


def save(w, data, name):
    path = w.inside(name)
    path.write_text(json.dumps(data))
    return path


def good(w, name, *args):
    rc, value, raw = cmd(w, name, *args)
    if rc != 0:
        raise RuntimeError(f'{name} {args[:2]} failed: {raw[:400]}')
    return value


def claim(ref, canary):
    return {'context': 'projectbrief.md', 'fact': 'intent', 'fact_key': 'retry-limit',
            'summary': f'Reviewed intent retry limit: 5 ({canary}).', 'value': 5, 'scope': 'repository',
            'conditions': {}, 'evidence': [ref], 'current': True, 'applicable': True, 'qualified': True,
            'authority_kind': 'approved-planning'}


def project(w, name, remote, canary):
    repo = w.repository(name)
    (repo / 'README.md').write_text(f'# {name}\n')
    (repo / 'docs/adr').mkdir(parents=True)
    (repo / 'docs/adr/one.md').write_text(f'---\nstatus: accepted\n---\n# Retry intent\n\nFive retries. {canary}\n')
    (repo / 'src').mkdir()
    (repo / 'src/app.py').write_text('RETRY_LIMIT = 5\n')
    if remote:
        w.git('remote', 'add', 'origin', remote, cwd=repo)
    p = good(w, 'scope_binding', 'preview', '--repo', repo, '--action', 'create', '--reason', 'Synthetic research project')
    pid = good(w, 'scope_binding', 'apply', '--proposal', save(w, p, name + '-bind.json'))['project_id']
    LABELS[pid] = 'PROJECT_' + name.upper().replace('-', '_')
    observed = good(w, 'initialize', 'inspect', '--repo', repo)
    review = {'inventory': observed, 'existing_memory_reviewed': False,
              'claims': [claim('docs/adr/one.md', canary)], 'questions': []}
    proposal = good(w, 'initialize', 'preview', '--repo', repo, '--review', save(w, review, name + '-review.json'))
    good(w, 'initialize', 'apply', '--proposal', save(w, proposal, name + '-init.json'))
    w.git('add', '.', cwd=repo)
    w.git('commit', '-m', 'Synthetic source and bank', cwd=repo)
    return repo, pid


def request(pid, query):
    return {'project_id': pid, 'query': query, 'ids': [], 'paths': [], 'context': {'branch': 'main'},
            'kinds': [], 'minimum_trust': 'reviewed', 'limit': 64, 'include_context': True}


def observations():
    out = {}
    with World() as w:
        dev = good(w, 'developer_identity', 'preview', '--create', '--label', 'Research developer')
        dev_id = good(w, 'developer_identity', 'apply', '--proposal', save(w, dev, 'dev.json'))['developer_id']
        LABELS[dev_id] = 'DEVELOPER_1'
        LABELS[str(w.root)] = '$WORLD'
        alpha, a = project(w, 'alpha', 'https://Git.Example.invalid/example-org/alpha-sdk.git', 'ALPHA_BODY_CANARY')
        beta, b = project(w, 'beta', 'git@git.example.invalid:private-org/beta-deploy.git', 'BETA_BODY_CANARY')

        # O1 stable random identity, not a remote/basename derivative.
        out['O1_distinct_random_project_ids'] = {'alpha_ne_beta': a != b,
                                                 'alpha_is_uuid4': uuid.UUID(a).version == 4,
                                                 'beta_is_uuid4': uuid.UUID(b).version == 4}

        # O2 remote normalization: syntax-equivalent forms unify, hosts/owners/forks stay distinct.
        forms = ['https://user:secret@git.example.invalid/example-org/alpha-sdk.git/',
                 'ssh://git@git.example.invalid/example-org/alpha-sdk',
                 'git@git.example.invalid:example-org/alpha-sdk.git',
                 'https://GitHub.com/Example-Org/Alpha-SDK.git',
                 'https://git.example.invalid/example-org/alpha-sdk-fork',
                 'https://other.example.invalid/example-org/alpha-sdk']
        norm = {}
        for form in forms:
            rc, value, raw = cmd(w, 'project_identity', 'normalize', form)
            norm[form.replace('user:secret@', 'user:***@')] = value.get('canonical_remote') if rc == 0 and value else ('error', raw[:160])
        out['O2_remote_normalization'] = norm

        # O3 linked worktree resolves to the same project.
        w.git('worktree', 'add', '-b', 'wt', str(w.inside('alpha-worktree')), cwd=alpha)
        st = good(w, 'project_identity', 'status', '--repo', w.inside('alpha-worktree'))
        out['O3_linked_worktree'] = {'project_id': label(st['project_id']), 'resolved': st['resolved']}

        # O4 independent clone with the same authority: same bank ID, but explicit link required.
        clone = w.clone('alpha', 'alpha-clone')
        w.git('remote', 'set-url', 'origin', 'https://git.example.invalid/example-org/alpha-sdk.git', cwd=clone)
        st = good(w, 'project_identity', 'status', '--repo', clone)
        rc, link, _ = cmd(w, 'project_identity', 'preview', '--repo', clone, '--action', 'link', '--reason', 'Research clone')
        out['O4_independent_clone'] = {'project_id': label(st['project_id']), 'resolved': st['resolved'],
                                       'issues': scrub(st['issues']), 'link_preview_rc': rc,
                                       'link_writes_performed': (link or {}).get('writes_performed')}

        # O5 fork/host migration: remote change stops; fork yields a new random ID.
        w.git('remote', 'set-url', 'origin', 'https://git.example.invalid/someone/alpha-sdk-fork.git', cwd=clone)
        st = good(w, 'project_identity', 'status', '--repo', clone)
        rc, fork, raw = cmd(w, 'project_identity', 'preview', '--repo', clone, '--action', 'fork', '--reason', 'Research fork')
        fork_id = (fork or {}).get('project_id')
        out['O5_fork_or_migration'] = {'resolved': st['resolved'], 'issues': scrub(st['issues']), 'fork_preview_rc': rc,
                                       'fork_id_is_new': fork_id not in (None, a, b),
                                       'fork_writes_performed': (fork or {}).get('writes_performed')}

        # O6 identical basenames without remotes are distinct projects.
        ids = []
        for parent in ('team-one', 'team-two'):
            repo = w.repository(parent + '/tool')
            p = good(w, 'scope_binding', 'preview', '--repo', repo, '--action', 'create', '--reason', 'Basename collision')
            ids.append(good(w, 'scope_binding', 'apply', '--proposal', save(w, p, parent + '.json'))['project_id'])
        out['O6_same_basename_no_remote'] = {'distinct': ids[0] != ids[1]}

        # O7 current scope policy: another project is never applicable; no federation option exists.
        rc, value, _ = cmd(w, 'scope_policy', 'decide', '--input', save(w, {'bank_project_id': b, 'project_id': a}, 'd1.json'))
        out['O7_scope_decide_cross_project'] = scrub({'rc': rc, 'applicable': value['applicable'], 'reasons': value['reasons'],
                                                      'identity_authenticated': value['identity_authenticated']})
        invalid = {}
        for variant in ({'scope': 'project:' + b}, {'federation_consent': True}, {'scope': 'personal'}):
            rc, value, _ = cmd(w, 'scope_policy', 'decide', '--input', save(w, {'bank_project_id': a, 'project_id': a, **variant}, 'd2.json'))
            invalid[scrub(json.dumps(variant))] = {'rc': rc, 'valid': (value or {}).get('valid')}
        out['O7_federation_scope_variants_rejected'] = invalid

        # O8 retrieval: own project works; naming another project against a bank fails without its bodies.
        rc, own, raw = cmd(w, 'retrieve', '--repo', alpha, '--request', save(w, request(a, 'retry'), 'r1.json'))
        out['O8_own_project_retrieval'] = {'rc': rc, 'result_count': len((own or {}).get('results', [])),
                                           'only_alpha_canary': 'ALPHA_BODY_CANARY' in raw and 'BETA_BODY_CANARY' not in raw,
                                           'result_envelope_keys': sorted(own or {}),
                                           'excluded_counts_keys': sorted((own or {}).get('excluded_counts', {})),
                                           'metrics_keys': sorted((own or {}).get('metrics', {}))}
        for key, req in (('without_discovery', request(a, 'retry')), ('with_discovery', {**request(a, 'retry'), 'discovery': {}})):
            rc, value, raw = cmd(w, 'retrieve', '--repo', beta, '--request', save(w, req, 'r2.json'))
            out['O8_foreign_project_id_against_beta_' + key] = {
                'rc': rc, 'error': scrub((value or {}).get('error', raw[:200])),
                'beta_body_disclosed': 'BETA_BODY_CANARY' in raw, 'beta_id_disclosed': b in raw}

        # O9 shared developer identity across projects does not merge banks or grant access.
        sa = good(w, 'scope_binding', 'status', '--repo', alpha)
        sb = good(w, 'scope_binding', 'status', '--repo', beta)
        out['O9_same_developer_two_projects'] = scrub({
            'same_developer': sa['developer']['developer_id'] == sb['developer']['developer_id'],
            'projects_distinct': sa['project']['project_id'] != sb['project']['project_id'],
            'identity_authenticated': [sa['identity_authenticated'], sb['identity_authenticated']],
            'security_boundary': sa['security_boundary']})

        # O10 bank router publishes the project UUID and sanitized remote to every bank reader.
        router = (alpha / 'memory-bank/MEMORY.md').read_text()
        out['O10_router_identity_disclosure'] = {'project_id_in_router': a in router,
                                                 'canonical_remote_in_router': 'repo://git.example.invalid/example-org/alpha-sdk' in router}
    # O11 no expansion-policy or federation surface exists in this source tree.
    hits = [str(p.relative_to(ROOT)) for p in (ROOT / 'skills').rglob('*.py') if 'recall-policy' in p.read_text(errors='ignore')]
    out['O11_recall_policy_command_in_source'] = hits
    return out


# Part 2: proposed conjunctive authority rule (research arithmetic only).
AUTH = ('M', 'D', 'R', 'X', 'E', 'P')
DIRECTIONS = ('public->public', 'public->private', 'private->private', 'private->public')


STATES = ('verified-current', 'stale-local-version', 'offline-unknown', 'revocation-observed')


def proposed(grants, direction, authority_state, provider_remote):
    """Return private internal (decision, foreign_reads, disclosed) for research arithmetic.

    Local manifest/grant/receipt checks run first; a foreign bank/index/graph is
    opened only when every local check passes and authority is currently verified.
    Repository visibility (direction) is deliberately NOT an input to allow.
    """
    local = all(grants[k] for k in ('M', 'D', 'R', 'X', 'E'))
    if not local or authority_state == 'revocation-observed':
        return 'denied', 0, False
    if authority_state != 'verified-current':
        return 'unavailable', 0, False
    if provider_remote and not grants['P']:
        # Exact/literal foreign recall may proceed; semantic ranking that would send foreign text is refused.
        return 'allowed-literal-only', 1, True
    return 'allowed', 1, True


def public_outcome(grants, authority_state):
    """One request-level envelope; no destination IDs or distinguishing reasons."""
    if authority_state != 'verified-current' or not all(grants[k] for k in ('M', 'D', 'R', 'X')):
        return 'unavailable'
    if not grants['E']:
        return 'approval-required'
    return 'available'


def truth_table():
    decisions, public_decisions, per_direction = {}, {}, {d: 0 for d in DIRECTIONS}
    cells = allowed_missing = denied_with_reads = 0
    for bits in itertools.product((False, True), repeat=len(AUTH)):
        grants = dict(zip(AUTH, bits))
        for direction in DIRECTIONS:
            for state in STATES:
                for provider_remote in (False, True):
                    cells += 1
                    decision, reads, disclosed = proposed(grants, direction, state, provider_remote)
                    decisions[decision] = decisions.get(decision, 0) + 1
                    public = public_outcome(grants, state)
                    public_decisions[public] = public_decisions.get(public, 0) + 1
                    if decision.startswith('allowed'):
                        per_direction[direction] += 1
                        allowed_missing += not all(grants[k] for k in ('M', 'D', 'R', 'X', 'E'))
                    else:
                        denied_with_reads += bool(reads or disclosed)
    invariant = all(
        proposed(dict(zip(AUTH, bits)), DIRECTIONS[0], s, p) == proposed(dict(zip(AUTH, bits)), d, s, p)
        for bits in itertools.product((False, True), repeat=len(AUTH)) for d in DIRECTIONS
        for s in STATES for p in (False, True))
    return {'cells': cells, 'decisions': decisions, 'allowed_per_direction': per_direction,
            'public_request_outcomes': public_decisions,
            'allowed_cells_missing_any_of_MDRXE': allowed_missing,
            'denied_or_unavailable_cells_with_foreign_read_or_disclosure': denied_with_reads,
            'decision_independent_of_repository_visibility': invariant,
            'expected_allowed_cells': len(DIRECTIONS) * 2 * 2,
            'scope_note': 'Consistency check of the brief proposal encoding; not a runtime, security or feature pass.'}


ALL = dict.fromkeys(AUTH, True)
# Named use-case walkthroughs over the proposal (synthetic; see brief section 3/7).
SCENARIOS = [
    ('UC-a public SDK asks public server compatibility notes', 'public->public', ALL, 'verified-current', False, 'allowed'),
    ('UC-a same, server never exported the record (X absent)', 'public->public', {**ALL, 'X': False}, 'verified-current', False, 'denied'),
    ('UC-b private app asks private deploy repo ownership contract', 'private->private', ALL, 'verified-current', False, 'allowed'),
    ('UC-c public library expands toward private consumer it cannot see (D absent)', 'private->public', {**ALL, 'D': False, 'R': False}, 'verified-current', False, 'denied'),
    ('UC-c private consumer reads public library export', 'public->private', ALL, 'verified-current', False, 'allowed'),
    ('UC-d second worktree of same project (not foreign; local scope)', 'local', None, 'verified-current', False, 'local-only'),
    ('UC-e fork without explicit membership of new UUID (M absent)', 'public->public', {**ALL, 'M': False}, 'verified-current', False, 'denied'),
    ('UC-f project removed from federation (revocation observed)', 'private->private', ALL, 'revocation-observed', False, 'denied'),
    ('UC-f offline, current membership version unknown', 'private->private', ALL, 'offline-unknown', False, 'unavailable'),
    ('UC-f cached grant version older than known current', 'private->private', ALL, 'stale-local-version', False, 'unavailable'),
    ('D4 operator denied or cancelled expansion prompt (E absent)', 'private->private', {**ALL, 'E': False}, 'verified-current', False, 'denied'),
    ('D4 allow-without-asking but destination not a member (M absent)', 'private->private', {**ALL, 'M': False}, 'verified-current', False, 'denied'),
    ('R3 remote embedding endpoint without provider approval (P absent)', 'private->private', {**ALL, 'P': False}, 'verified-current', True, 'allowed-literal-only'),
    ('Reader in destination lacks read/acceptance (R absent)', 'public->private', {**ALL, 'R': False}, 'verified-current', False, 'denied'),
]


def scenarios():
    rows = []
    for name, direction, grants, state, remote, expected in SCENARIOS:
        if grants is None:
            rows.append({'scenario': name, 'decision': 'local-only', 'foreign_reads': 0, 'matches_expected': expected == 'local-only'})
            continue
        decision, reads, disclosed = proposed(grants, direction, state, remote)
        rows.append({'scenario': name, 'direction': direction, 'decision': decision, 'foreign_reads': reads,
                     'public_request_outcome': public_outcome(grants, state),
                     'foreign_metadata_disclosed': disclosed, 'matches_expected': decision == expected})
    return rows


if __name__ == '__main__':
    head = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    print(json.dumps({'schema': 'am-v015-246-walkthrough-v1', 'source_revision': head,
                      'part1_current_behavior': scrub(observations()),
                      'part2_proposed_rule_arithmetic': truth_table(),
                      'part3_proposed_scenarios': scenarios()}, indent=2, sort_keys=True))
