"""Publish reviewed memory independently of feature code; verify actual authority trees."""
from __future__ import annotations

import argparse
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import quote, urlsplit, parse_qs
import uuid

from . import bank_contract as bank, bank_transactions as tx, bank_writes as writes, local_registry as storage
from . import scope_binding, semantic_sharing as semantic, sharing_recovery as recovery
from .developer_identity import digest, encoded
from .frontmatter import ContractError
from .sharing_holds import HoldFailure
from .identity_git import git
from . import sharing_limits as limits

SCHEMA = 'agent-memory/publication-request-v1'
FIELDS = {'schema', 'repo_root', 'project_id', 'base', 'candidate', 'proof'}
LIMIT = 1048576
API_VERSION = '2026-03-10'


def environment():
    allowed = {'GIT_CONFIG_NOSYSTEM', 'GIT_CONFIG_SYSTEM', 'GIT_CONFIG_GLOBAL',
               'GIT_TERMINAL_PROMPT', 'GIT_ASKPASS', 'GIT_ALLOW_PROTOCOL',
               'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL'}
    env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_') or k in allowed}
    env.update(GIT_NO_LAZY_FETCH='1', GIT_OPTIONAL_LOCKS='0', GIT_TERMINAL_PROMPT='0',
               GIT_TRACE='0', GIT_TRACE2='0', GIT_TRACE2_EVENT='0', GIT_TRACE2_PERF='0')
    # Never emit credentials or response bodies through gh debug logging.
    env.pop('GH_DEBUG', None)
    return env


def command(argv, *, env=None, payload=None, timeout=30):
    with tempfile.TemporaryFile() as output:
        try:
            p = subprocess.run(argv, env=env or environment(), input=payload, stdout=output,
                               stderr=subprocess.DEVNULL, timeout=timeout)
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise ContractError('Publication transport unavailable or interrupted; inventory before retry') from exc
        output.seek(0); raw = output.read(LIMIT + 1)
    if len(raw) > LIMIT:
        raise ContractError('Publication response exceeds its complete bounded reader')
    return p.returncode, raw


def git_write(root, *args, payload=None, index=None, network=False, optional=False):
    env = environment()
    if index is not None:
        env['GIT_INDEX_FILE'] = str(index)
    if not network:
        env['GIT_ALLOW_PROTOCOL'] = ''
    # A normal push retains the repository's pre-push hook. No --force, admin,
    # lease override, filter, checkout, reset or original-index mutation exists.
    argv = ['git', '--no-pager', '--no-replace-objects', '--literal-pathspecs',
            '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false', '-C', str(root), *args]
    code, raw = command(argv, env=env, payload=payload)
    if code and not optional:
        raise ContractError('Memory publication Git operation failed; retain local learning and inventory')
    return code, raw.decode('utf-8')


def api(endpoint, *, method='GET', body=None, allow_missing=False, with_headers=False):
    # --include distinguishes an observed 404 from auth/permission/network failure.
    argv = ['gh', 'api', '--hostname', 'github.com', '--include', '--method', method,
            '-H', 'Accept: application/vnd.github+json', '-H', 'X-GitHub-Api-Version: '+API_VERSION, endpoint]
    if body is not None:
        argv += ['--input', '-']
    code, raw = command(argv, payload=encoded(body) if body is not None else None)
    text = raw.decode('utf-8').replace('\r\n', '\n')
    header, separator, content = text.partition('\n\n')
    match = re.match(r'HTTP/[0-9.]+ ([0-9]{3})\b', header)
    status = int(match[1]) if match else None
    if status == 404 and allow_missing:
        return None
    if code or status not in ({200} if method == 'GET' else {200, 201}) or not separator:
        raise ContractError('GitHub rules or permission observation unavailable; publication remains pending')
    try:
        value = json.loads(content)
        headers = {k.lower(): v.strip() for line in header.splitlines()[1:] if ':' in line
                   for k, v in [line.split(':', 1)]}
        return (value, headers) if with_headers else value
    except (ValueError, UnicodeError) as exc:
        raise ContractError('Incomplete GitHub policy response; publication remains pending') from exc


def collection(endpoint, *, key=None):
    """Read every page within a finite bound; never assume the first page is all rules."""
    found = []
    for page in range(1, 5):
        value, headers = api(endpoint+('&' if '?' in endpoint else '?')+'per_page=100&page='+str(page), with_headers=True)
        rows = value.get(key) if key and isinstance(value, dict) else value
        if not isinstance(rows, list) or len(rows) > 100:
            raise ContractError('Incomplete paginated GitHub policy/check inventory')
        found.extend(rows)
        if not re.search(r'rel="next"', headers.get('link', '')):
            return found
    raise ContractError('Complete GitHub rule/check inventory exceeds its review bound')


def repository(root):
    evidence = scope_binding.project.status(root)['evidence']
    identity = evidence['canonical_remote']
    match = re.fullmatch(r'repo://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)', identity or '')
    if not match:
        raise ContractError('Automatic landing requires observable GitHub rules; retain local memory for other hosts')
    return match[1]+'/'+match[2]


def restricted_push_actor(endpoint, restrictions):
    """Observe an explicit user grant; API eligibility never bypasses Git policy."""
    repair = ('Retain local learning and pending sources; inspect the authenticated '
              'GitHub user and branch allowed users. Team/App-only grants require '
              'a supported repository workflow; do not loosen protection or switch credentials automatically.')
    try:
        if (not isinstance(restrictions, dict)
                or any(not isinstance(restrictions.get(key), list) for key in ('users', 'teams', 'apps'))):
            raise ContractError('Classic push-actor restriction is incomplete or malformed')
        actor = api('user')
        def user(value):
            if (not isinstance(value, dict) or type(value.get('id')) is not int
                    or value['id'] <= 0 or value.get('type') != 'User'
                    or not isinstance(value.get('login'), str)
                    or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}', value['login'])):
                raise ContractError('Push-actor user identity is incomplete or malformed')
            return {'id': value['id'], 'login': value['login'].lower(), 'type': 'User'}
        actor = user(actor)
        users, seen = [], set()
        for page in range(1, 5):
            rows, headers = api(endpoint+'?per_page=100&page='+str(page), with_headers=True)
            if not isinstance(rows, list) or len(rows) > 100:
                raise ContractError('Allowed push-user inventory is incomplete or malformed')
            for row in rows:
                value = user(row)
                if value['id'] in seen:
                    raise ContractError('Allowed push-user inventory has duplicate or conflicting IDs')
                seen.add(value['id']); users.append(value)
            link = headers.get('link', '')
            if link and not all(re.fullmatch(r'\s*<[^>]+>;\s*rel="(?:next|prev|first|last)"\s*', part) for part in link.split(',')):
                raise ContractError('Allowed push-user pagination header is malformed')
            if 'rel="next"' not in link:
                break
            matches = re.findall(r'<([^>]+)>;\s*rel="next"', link)
            if len(matches) != 1:
                raise ContractError('Allowed push-user pagination chain is malformed')
            parsed = urlsplit(matches[0])
            if (parsed.scheme != 'https' or parsed.netloc != 'api.github.com'
                    or parsed.path != '/'+endpoint or parsed.fragment
                    or parse_qs(parsed.query) != {'per_page': ['100'], 'page': [str(page+1)]}):
                raise ContractError('Allowed push-user pagination chain changed or is incomplete')
        else:
            raise ContractError('Allowed push-user inventory exceeds four complete pages')
        observation = {'actor': actor, 'users': sorted(users, key=lambda value: value['id'])}
        if actor not in users:
            return {**observation, 'eligible': False,
                    'reason': 'Authenticated GitHub user is not an explicitly allowed push user.', 'repair': repair}
        return {**observation, 'eligible': True}
    except ContractError as exc:
        return {'eligible': False, 'reason': 'Explicit push-user eligibility is unverified: '+str(exc), 'repair': repair}


def policy(root, binding):
    name = repository(root); endpoint = 'repos/'+name
    repo = api(endpoint)
    if (not isinstance(repo, dict) or repo.get('full_name', '').lower() != name.lower()
            or repo.get('default_branch') != binding['authority_branch']
            or repo.get('permissions', {}).get('push') is not True
            or repo.get('archived') is not False or repo.get('disabled') is not False):
        raise ContractError('Current repository identity/default/permission does not permit publication')
    branch = quote(binding['authority_branch'], safe='')
    branch_data = api(endpoint+'/branches/'+branch)
    rules = collection(endpoint+'/rules/branches/'+branch)
    if (not isinstance(branch_data, dict) or type(branch_data.get('protected')) is not bool
            or branch_data.get('name') != binding['authority_branch'] or not isinstance(rules, list)
            or any(not isinstance(r, dict) or not isinstance(r.get('type'), str)
                   or not re.fullmatch('[a-z_]{1,64}', r['type']) for r in rules)):
        raise ContractError('Incomplete active branch rule inventory; publication remains pending')
    known = {'pull_request', 'required_status_checks', 'non_fast_forward', 'deletion'}
    unsupported = sorted({r['type'] for r in rules} - known)
    classic = None
    push_actor = None
    if branch_data['protected']:
        classic = api(endpoint+'/branches/'+branch+'/protection', allow_missing=True)
        if classic is None and not rules:
            raise ContractError('Protected branch has no complete readable rule inventory')
        if classic is not None:
            if not isinstance(classic, dict):
                raise ContractError('Incomplete classic branch protection response')
            for key in ('required_linear_history', 'required_signatures', 'lock_branch'):
                if classic.get(key, {}).get('enabled') is True:
                    unsupported.append(key)
            if classic.get('restrictions') is not None:
                push_actor = restricted_push_actor(endpoint+'/branches/'+branch+'/protection/restrictions/users', classic['restrictions'])
                if not push_actor['eligible']:
                    unsupported.append('restricted-push-actor-unverified')
    requirements = (classic or {}).get('required_pull_request_reviews') or {}
    review = (requirements.get('required_approving_review_count', 0) > 0
              or requirements.get('require_code_owner_reviews') is True
              or requirements.get('require_last_push_approval') is True)
    checks = set()
    def add_check(context, app=None):
        if (not isinstance(context, str) or not context or len(context.encode()) > 256
                or (app is not None and (type(app) is not int or app < -1))):
            raise ContractError('Unknown required status check identity')
        checks.add((context, None if app in {None, -1} else app))
    required = (classic or {}).get('required_status_checks') or {}
    for context in required.get('contexts', []):
        add_check(context)
    for check in required.get('checks', []):
        add_check(check['context'], check.get('app_id'))
    for rule in rules:
        if rule['type'] == 'pull_request':
            parameters = rule.get('parameters', {})
            if 'allowed_merge_methods' in parameters and 'merge' not in parameters['allowed_merge_methods']:
                unsupported.append('pull-request-merge-method')
            review |= (parameters.get('required_approving_review_count', 0) > 0
                       or parameters.get('require_code_owner_review') is True
                       or parameters.get('require_last_push_approval') is True
                       or any(r['minimum_approvals'] > 0 for r in parameters.get('required_reviewers', [])))
        elif rule['type'] == 'required_status_checks':
            for check in rule['parameters']['required_status_checks']:
                add_check(check['context'], check.get('integration_id'))
    protected = branch_data['protected'] or any(r['type'] not in {'non_fast_forward', 'deletion'} for r in rules)
    if protected and repo.get('allow_merge_commit') is not True:
        unsupported.append('merge-commit-method-disabled')
    return {'repository': name, 'protected': protected, 'review_required': review,
            'push_actor': push_actor,
            'required_checks': [{'context': c, 'app_id': a} for c, a in sorted(checks, key=lambda x: (x[0], x[1] or 0))],
            'conversation_resolution_required': bool(classic and classic.get('required_conversation_resolution', {}).get('enabled')),
            'unsupported_rules': sorted(set(unsupported)), 'observation_sha256': digest(encoded({
                'repo': {k: repo.get(k) for k in ('full_name', 'default_branch', 'permissions', 'archived', 'disabled', 'allow_merge_commit')},
                'branch': {k: branch_data[k] for k in ('name', 'protected')},
                'rules': sorted(rules, key=lambda r: encoded(r)), 'classic': classic, 'push_actor': push_actor}))}


def result(operation, state, **values):
    return {'schema': 'agent-memory/publication-result-v1', 'operation': operation,
            'sharing_state': state, 'publication_verified': False, 'canonical_writes_performed': False,
            'original_index_writes_performed': False, 'feature_code_freshness_verified': False,
            'local_memory_available': True, **values}


def index_bytes(root):
    path = Path(git(root, 'rev-parse', '--path-format=absolute', '--git-path', 'index'))
    if path.is_symlink():
        raise ContractError('Publication refuses a redirected original index')
    return digest(path.read_bytes() if path.exists() else None)


def candidate(root, binding, files, *, quarantine_records=False):
    before = recovery.tree(root, binding['authority_head'], binding['bank_root'],quarantine_records=quarantine_records)
    if files == before:
        return None
    # This index contains the fresh authority tree, not the feature HEAD/index.
    # Desired blobs are read from the exact reviewed result, never git add -A.
    temporary_root, _ = writes.state_paths(root, root/binding['bank_root'], str(uuid.uuid4()))
    storage.private_mkdir(temporary_root)
    with tempfile.TemporaryDirectory(prefix='publication-', dir=temporary_root) as directory:
        index = Path(directory)/'index'
        git_write(root, 'read-tree', binding['authority_head'], index=index)
        for path in sorted(set(before) | set(files)):
            relative = binding['bank_root']+'/'+path
            if path not in files:
                git_write(root, 'update-index', '--force-remove', '--', relative, index=index)
            elif files[path] != before.get(path):
                _, blob = git_write(root, 'hash-object', '-w', '--stdin', payload=files[path].encode())
                git_write(root, 'update-index', '--add', '--cacheinfo', '100644,'+blob.strip()+','+relative, index=index)
        _, tree = git_write(root, 'write-tree', index=index)
        signing = git(root, 'config', '--bool', 'commit.gpgsign', optional=True)
        _, commit = git_write(root, 'commit-tree', tree.strip(), '-p', binding['authority_head'],
                              *(['-S'] if signing == 'true' else []),
                              '-m', 'Publish reviewed project memory\n\nSemantic-proof: '+digest(encoded(files)))
    return recovery.oid(commit.strip())


def prepare(repo, proof):
    reviewed = semantic.check(repo, proof)
    binding = proof['judgment']['binding']; root = Path(binding['repo_root'])
    original_index = index_bytes(root)
    observed_policy = policy(root, binding)
    if observed_policy['unsupported_rules']:
        return result('prepare', 'pending-policy', reason='Repository rules need an explicit supported landing method',
                      unsupported_rules=observed_policy['unsupported_rules'],
                      policy_failure=observed_policy.get('push_actor'), git_metadata_writes_performed=True)
    proposed = candidate(root, binding, reviewed['publish_files'],quarantine_records=proof['schema']=='agent-memory/semantic-sharing-proof-v2')
    semantic.final_sources(root, binding, binding)
    if index_bytes(root) != original_index:
        raise ContractError('Original index changed during publication preparation; inspect without overwriting it')
    if proposed is None:
        return result('prepare', 'no-publishable-delta', conflicts=reviewed['conflicts'],
                      blocked_paths=reviewed['blocked_paths'], git_metadata_writes_performed=True)
    request = {'schema': SCHEMA, 'repo_root': str(root), 'project_id': binding['project_id'],
               'base': binding['authority_head'], 'candidate': proposed, 'proof': proof}
    return result('prepare', 'prepared', request=request, repository_policy=observed_policy,
                  conflicts=reviewed['conflicts'], blocked_paths=reviewed['blocked_paths'],
                  git_metadata_writes_performed=True, remote_freshness_verified=True)


def request_value(root, value):
    if (not isinstance(value, dict) or set(value) != FIELDS or value.get('schema') != SCHEMA
            or value.get('repo_root') != str(root)):
        raise ContractError('Use the exact private publication request in its original repository')
    proof = value['proof']; binding = proof['judgment']['binding']
    if (value['project_id'] != binding['project_id'] or value['base'] != binding['authority_head']
            or binding['repo_root'] != str(root)):
        raise ContractError('Publication request and source proof do not share the same project/base')
    recovery.commit(root, value['candidate']); recovery.commit(root, value['base'])
    parents = recovery.command(root, 'rev-list', '--parents', '-n', '1', value['candidate']).strip().split()[1:]
    if parents != [value['base']]:
        raise ContractError('Memory candidate must have only the freshly reviewed authority parent')
    after = recovery.tree(root, value['candidate'], binding['bank_root'],quarantine_records=proof['schema']=='agent-memory/semantic-sharing-proof-v2')
    if (bank.validate_files(after)['project_id'] != value['project_id']
            or tx.file_hashes(after) != proof['publish_hashes']):
        raise ContractError('Actual candidate bank does not equal the exact reviewed publication result')
    changed = recovery.command(root, 'diff-tree', '--no-commit-id', '--no-renames', '--name-only', '-r', '-z',
                               value['base'], value['candidate']).split('\0')
    if not any(changed) or any(not path.startswith(binding['bank_root']+'/') for path in changed if path):
        raise ContractError('Publication contains feature code or no actual memory delta')
    scope_binding.require(root, value['project_id'], expected=binding['scope_context'])
    return binding


def publication_pr(root, request, observed_policy):
    """Create/reuse a memory-only PR; never approve it or enable an unpinned merge."""
    binding = request['proof']['judgment']['binding']; name = observed_policy['repository']; owner = name.split('/')[0]
    branch = 'agent-memory/'+request['project_id']+'/'+request['candidate']
    remote = binding['authority_remote']; endpoint = 'repos/'+name+'/pulls'
    existing = api(endpoint+'?state=all&base='+quote(binding['authority_branch'], safe='')+
                   '&head='+quote(owner+':'+branch, safe='')+'&per_page=2')
    if not isinstance(existing, list) or len(existing) > 1:
        raise ContractError('Publication PR identity is ambiguous; inspect before retry')
    if not existing:
        git_write(root, 'push', '--porcelain', '--no-follow-tags', '--recurse-submodules=no', '--',
                  remote, request['candidate']+':refs/heads/'+branch, network=True)
        semantic.final_sources(root, binding, binding)
        # A lost create response is recovered by the exact same head/base query.
        value = api(endpoint, method='POST', body={
            'title': 'Maintain shared project memory', 'head': branch, 'base': binding['authority_branch'],
            'body': 'Memory-only learning prepared from the current authority tree. Conditions, evidence and provenance remain in the Markdown bank. Required reviews and checks must complete; no bypass or automatic approval is requested.'})
    else:
        value = existing[0]
    if (not isinstance(value, dict) or type(value.get('number')) is not int
            or value.get('head', {}).get('sha') != request['candidate']
            or value.get('head', {}).get('ref') != branch
            or value.get('base', {}).get('ref') != binding['authority_branch']
            or value.get('base', {}).get('repo', {}).get('full_name', '').lower() != name.lower()
            or value.get('head', {}).get('repo', {}).get('full_name', '').lower() != name.lower()):
        raise ContractError('Returned PR is not this repository memory-only candidate')
    return value


def check_inventory(name, sha):
    endpoint = 'repos/'+name+'/commits/'+recovery.oid(sha)
    statuses = collection(endpoint+'/statuses')
    checks = collection(endpoint+'/check-runs?filter=latest', key='check_runs')
    if (any(not isinstance(s, dict) or not isinstance(s.get('context'), str) or type(s.get('id')) is not int for s in statuses)
            or any(not isinstance(c, dict) or c.get('head_sha') != sha or not isinstance(c.get('name'), str) for c in checks)):
        raise ContractError('Incomplete or wrong-commit required check evidence')
    latest = {}
    for status in statuses:
        if status['context'] not in latest or status['id'] > latest[status['context']]['id']:
            latest[status['context']] = status
    return latest, checks


def required_checks_pass(name, head, merged, requirements):
    if not requirements:
        return True
    merge_statuses, merge_checks = check_inventory(name, merged)
    statuses, checks = ((merge_statuses, merge_checks) if merge_statuses or merge_checks else check_inventory(name, head))
    def recent(value):
        if not isinstance(value, str):
            return False
        try:
            at = datetime.fromisoformat(value.replace('Z', '+00:00'))
            now = datetime.now(timezone.utc)
            return at.tzinfo is not None and now-timedelta(days=7) <= at <= now+timedelta(minutes=5)
        except (ValueError, TypeError):
            return False
    for required in requirements:
        name, app = required['context'], required['app_id']
        matching = [c for c in checks if c['name'] == name and (app is None or c.get('app', {}).get('id') == app)]
        status = statuses.get(name)
        # A plain status does not establish a configured GitHub App identity.
        if not matching and (status is None or app is not None):
            return False
        if status is not None and (status.get('state') != 'success' or not recent(status.get('created_at'))):
            return False
        if any(c.get('status') != 'completed' or c.get('conclusion') not in {'success', 'neutral', 'skipped'}
               or not recent(c.get('completed_at')) for c in matching):
            return False
    return True


def protected_candidate(root, request, observed_policy):
    pr = publication_pr(root, request, observed_policy); number = pr['number']
    name = observed_policy['repository']; owner, repo_name = name.split('/')
    query = '''query($owner:String!,$repo:String!,$number:Int!){repository(owner:$owner,name:$repo){pullRequest(number:$number){state isDraft headRefOid baseRefOid reviewDecision mergeStateStatus commits(last:1){nodes{commit{oid statusCheckRollup{state}}}} reviewThreads(first:100){pageInfo{hasNextPage} nodes{isResolved}}}}}'''
    data = api('graphql', method='POST', body={'query': query, 'variables': {'owner': owner, 'repo': repo_name, 'number': number}})
    if data.get('errors'):
        raise ContractError('Incomplete pull request rule evaluation')
    current = data['data']['repository']['pullRequest']; binding = request['proof']['judgment']['binding']
    url = 'https://github.com/'+name+'/pull/'+str(number)
    if (current['state'] != 'OPEN' or current['isDraft'] or current['headRefOid'] != request['candidate']
            or current['baseRefOid'] != request['base']):
        return None, url, 'PR source/base changed or is not open; reconcile current authority'
    if current['reviewDecision'] in {'REVIEW_REQUIRED', 'CHANGES_REQUESTED'} or (
            observed_policy['review_required'] and current['reviewDecision'] != 'APPROVED'):
        return None, url, 'Required review remains pending; ask the detecting developer through the normal repository workflow'
    threads = current['reviewThreads']
    checks = current['commits']['nodes']
    if (current['mergeStateStatus'] != 'CLEAN' or len(checks) != 1 or checks[0]['commit']['oid'] != request['candidate']
            or threads['pageInfo']['hasNextPage'] or any(not row['isResolved'] for row in threads['nodes'])):
        return None, url, 'Required checks, conversations or merge requirements remain pending'
    # GitHub's generated merge must have this exact reviewed base and PR head.
    # A regular fast-forward push then rejects any competing authority advance.
    ref = 'refs/agent-memory/observations/'+str(uuid.uuid4())
    try:
        recovery.command(root, 'fetch', '--no-tags', '--no-write-fetch-head', '--no-recurse-submodules', '--',
                         binding['authority_remote'], 'refs/pull/'+str(number)+'/merge:'+ref, network=True)
        merged = git(root, 'rev-parse', '--verify', ref)
        parents = recovery.command(root, 'rev-list', '--parents', '-n', '1', merged).strip().split()[1:]
        if parents != [request['base'], request['candidate']]:
            raise ContractError('Generated merge base/head advanced; repeat semantic reconciliation')
        if recovery.command(root, 'rev-parse', merged+'^{tree}').strip() != recovery.command(root, 'rev-parse', request['candidate']+'^{tree}').strip():
            raise ContractError('Generated merge changes more than the exact reviewed candidate tree')
        if not required_checks_pass(name, request['candidate'], merged, observed_policy['required_checks']):
            return None, url, 'Exact required status/check results or their configured App identities remain pending'
        return merged, url, None
    finally:
        recovery.command(root, 'update-ref', '-d', ref)


def land(repo, request):
    root, _ = writes.bank_path(repo); binding = request_value(root, request)
    original_index = index_bytes(root)
    # Inventory before any retry. A prior success stays historical if later
    # corrected; neither a new proof nor a duplicate push is needed for readback.
    readback = recovery.replay(root, {key: request[key] for key in ('base', 'candidate')},
        quarantine_records=request['proof']['schema']=='agent-memory/semantic-sharing-proof-v2')
    if readback['intended_delta_landed']:
        return result('land', 'already-landed', publication_verified=True,
                      intended_delta_current=readback['intended_delta_current'], witnessed_commit=readback['witnessed_commit'],
                      remote_freshness_verified=True, git_metadata_writes_performed=True)
    try:
        reviewed = semantic.check(root, request['proof'])
    except HoldFailure:
        raise
    except ContractError:
        return result('land', 'pending-reconciliation', reason='Sources or authority changed; inventory, deduplicate and review the new source set',
                      git_metadata_writes_performed=True)
    observed_policy = policy(root, binding)
    if observed_policy['unsupported_rules']:
        return result('land', 'pending-policy', unsupported_rules=observed_policy['unsupported_rules'],
                      reason='Repository requires an explicitly supported landing method',
                      policy_failure=observed_policy.get('push_actor'), git_metadata_writes_performed=True)
    proposed, pr_url = request['candidate'], None
    if observed_policy['protected']:
        proposed, pr_url, reason = protected_candidate(root, request, observed_policy)
        if proposed is None:
            return result('land', 'pending-repository-rules', reason=reason, pull_request=pr_url, git_metadata_writes_performed=True)
    writes.fault('publication-before-final-review')
    # Repeat semantic comparison/deduplication and policy immediately before push.
    semantic.check(root, request['proof'])
    if policy(root, binding) != observed_policy or index_bytes(root) != original_index:
        raise ContractError('Repository rules or original index changed; inspect before publication')
    if observed_policy['protected']:
        proposed, pr_url, reason = protected_candidate(root, request, observed_policy)
        if proposed is None:
            return result('land', 'pending-repository-rules', reason=reason, pull_request=pr_url, git_metadata_writes_performed=True)
    semantic.final_sources(root, binding, binding)
    writes.fault('publication-before-push')
    code, _ = git_write(root, 'push', '--porcelain', '--no-follow-tags', '--recurse-submodules=no', '--', binding['authority_remote'],
                        proposed+':refs/heads/'+binding['authority_branch'], network=True, optional=True)
    writes.fault('publication-after-push')
    readback = recovery.replay(root, {key: request[key] for key in ('base', 'candidate')},
        quarantine_records=request['proof']['schema']=='agent-memory/semantic-sharing-proof-v2')
    if index_bytes(root) != original_index:
        raise ContractError('Original feature index changed externally during publication; preserve and inspect it')
    if not readback['intended_delta_landed']:
        return result('land', 'pending-reconciliation', reason=('Push was rejected; API and Git transport may use different credentials, or repository rules/authority changed. Inspect both identities and current rules; preserve the candidate and local learning before retrying.' if code else
                              'Authority advanced or landing readback is unavailable; preserve the candidate and reconcile before retrying'),
                      transport_exit=code, pull_request=pr_url, git_metadata_writes_performed=True)
    return result('land', 'landed', publication_verified=True, witnessed_commit=readback['witnessed_commit'],
                  intended_delta_current=readback['intended_delta_current'], pull_request=pr_url,
                  remote_freshness_verified=True, git_metadata_writes_performed=True,
                  blocked_paths=reviewed['blocked_paths'], conflicts=reviewed['conflicts'])


from . import validation_memo


@validation_memo.operation()
def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['prepare', 'land'])
    parser.add_argument('--repo', required=True)
    parser.add_argument('--input', required=True)
    parser.add_argument('--output')
    parser.add_argument('--v2',action='store_true',help='Read an explicitly v2 proof/request with the complete private-operation budget')
    args = parser.parse_args(argv)
    try:
        if args.output:
            limits.preflight_private_output(args.output, args.repo)
        value = semantic.read_input(args.input,version2=args.v2)
        outcome = prepare(args.repo, value) if args.operation == 'prepare' else land(args.repo, value)
        if args.output:
            outcome=limits.write_private(args.output,outcome,args.repo)
        print(json.dumps(outcome, indent=2)); return 0
    except HoldFailure as exc:
        print(json.dumps(result(args.operation, 'pending-inspection', reason=str(exc),
                                failure_component='known-dispute-state', failure_code=exc.code,
                                repair=exc.repair, git_metadata_writes_performed=None,
                                remote_freshness_verified=False)))
        return 1
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError, RecursionError) as exc:
        output_failed = isinstance(exc, limits.PrivateOutputError)
        print(json.dumps(result(args.operation, 'pending-private-output' if output_failed else 'pending-inspection',
                                failure_code='private-output-invalid' if output_failed else 'publication-unverified',
                                reason=str(exc) if output_failed else 'Unresolved, unsafe, offline, changed or interrupted publication; retain local memory and inspect before retry',
                                repair=str(exc) if output_failed else 'Preserve local memory and original pending evidence; inspect before retrying.',
                                git_metadata_writes_performed=False if output_failed else None, remote_freshness_verified=False)))
        return 1


if __name__ == '__main__':
    sys.exit(main())
