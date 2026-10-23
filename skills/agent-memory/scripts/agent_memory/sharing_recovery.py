"""Fresh Git observations, publication replay readback and deletion-aware merge plans.

These are recovery primitives. A merge plan is not semantic approval, a write,
publication or a claim that feature code matches shared authority.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import uuid

from . import bank_contract as bank, bank_layout as layout, bank_transactions as transactions
from . import bank_writes as writes, repository_config, scope_binding, sharing_boundary, security
from .developer_identity import digest, encoded, load
from .frontmatter import ContractError, read, write
from .identity_git import git

MAX_FILES = 256
MAX_BYTES = 1048576
MAX_FILE = 16384
MISSING = object()


def command(repo, *args, network=False, limit=MAX_BYTES):
    # Never let ambient worktree/index/object redirection alter the repository.
    allowed = {'GIT_CONFIG_NOSYSTEM', 'GIT_CONFIG_SYSTEM', 'GIT_CONFIG_GLOBAL',
               'GIT_TERMINAL_PROMPT', 'GIT_ASKPASS', 'GIT_ALLOW_PROTOCOL'}
    env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_') or k in allowed}
    env.update(GIT_NO_LAZY_FETCH='1', GIT_OPTIONAL_LOCKS='0', GIT_TRACE='0',
               GIT_TRACE2='0', GIT_TRACE2_EVENT='0', GIT_TRACE2_PERF='0', GIT_TERMINAL_PROMPT='0')
    if not network:
        env['GIT_ALLOW_PROTOCOL'] = ''
    argv = ['git', '--no-pager', '--no-replace-objects', '--literal-pathspecs',
            '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false',
            '-c', 'core.hooksPath=/dev/null', '-C', str(repo), *args]
    # Spool output instead of allocating an unbounded remote/tree response.
    with tempfile.TemporaryFile() as output:
        try:
            result = subprocess.run(argv, env=env, stdout=output, stderr=subprocess.DEVNULL,
                                    timeout=30 if network else 10)
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise ContractError('Git sharing observation failed; retain local memory and retry') from exc
        output.seek(0); raw = output.read(limit + 1)
    if result.returncode or len(raw) > limit:
        raise ContractError('Git sharing observation failed or exceeded its bound; retain local memory')
    return raw.decode('utf-8')


def oid(value):
    if not isinstance(value, str) or not re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}', value):
        raise ContractError('An immutable full Git object ID is required')
    return value


def commit(repo, value):
    oid(value)
    if command(repo, 'cat-file', '-t', value).strip() != 'commit':
        raise ContractError('Sharing snapshots must name commits')
    return value


def bounded(files):
    if (not isinstance(files, dict) or len(files) > MAX_FILES or
            any(not isinstance(v, str) or len(v.encode()) > MAX_FILE for v in files.values()) or
            sum(len(v.encode()) for v in files.values()) > MAX_BYTES):
        raise ContractError('Complete memory snapshot exceeds recovery bounds; review without truncation')
    checked = bank.validate_files(files)
    for path, body in files.items():
        if security.scan(path + '\n' + body)['reasons']:
            raise ContractError('Unsafe or unreviewable shared memory; inspect without echoing content')
    return checked


def tree(repo, revision, prefix):
    commit(repo, revision)
    output = command(repo, 'ls-tree', '-rz', '--full-tree', revision, '--', prefix)
    files = {}
    for item in output.split('\0'):
        if not item:
            continue
        header, path = item.split('\t', 1)
        mode, kind, blob = header.split()
        if not path.startswith(prefix + '/') or mode != '100644' or kind != 'blob':
            raise ContractError('Nonordinary shared bank tree requires review')
        path = path[len(prefix) + 1:]
        if path != 'MEMORY.md':
            bank.path_value(path)
        if bank.runtime_path(path) or len(files) >= MAX_FILES:
            raise ContractError('Excluded runtime artifacts or oversized bank tree require review')
        size = int(command(repo, 'cat-file', '-s', blob).strip())
        if size > MAX_FILE:
            raise ContractError('Shared memory object exceeds bounded review')
        files[path] = command(repo, 'cat-file', 'blob', blob, limit=MAX_FILE)
    bounded(files)
    return files


def server(repo, remote, branch):
    response = command(repo, 'ls-remote', '--symref', '--', remote, 'HEAD', 'refs/heads/' + branch,
                       network=True, limit=65536)
    default, observed = None, None
    for line in response.splitlines():
        left, ref = line.split('\t', 1)
        if ref == 'HEAD' and left.startswith('ref: refs/heads/'):
            default = left[len('ref: refs/heads/'):]
        elif ref == 'refs/heads/' + branch:
            observed = oid(left)
    if default != branch or observed is None:
        raise ContractError('Configured authority differs from the server default or is unresolved')
    return observed


def observe(repo):
    root, _ = writes.bank_path(repo)
    context = scope_binding.require(root)
    boundary = sharing_boundary.inspect(root)
    if set(boundary['blockers']) - {'canonical-memory-is-not-tracked'}:
        raise ContractError('Resolve the listed local sharing boundary blockers before exchange')
    local_head = git(root, 'rev-parse', '--verify', 'HEAD')
    remote, branch = boundary['authority_remote'], boundary['authority_branch']
    head = server(root, remote, branch)
    # Unique disposable ref: do not overwrite FETCH_HEAD, remote tracking refs,
    # the caller's branch/index or another session's observation.
    ref = 'refs/agent-memory/observations/' + str(uuid.uuid4())
    try:
        command(root, 'fetch', '--no-tags', '--no-write-fetch-head', '--no-recurse-submodules',
                '--', remote, 'refs/heads/' + branch + ':' + ref, network=True, limit=65536)
        writes.fault('sharing-after-fetch')
        fetched = git(root, 'rev-parse', '--verify', ref)
        if fetched != head or server(root, remote, branch) != head:
            raise ContractError('Authority advanced during observation; reconcile against a new observation')
        files = tree(root, head, boundary['bank_root'])
        local_router = bank.parse_router((root / boundary['bank_root'] / 'MEMORY.md').read_text())[0]
        shared_router = bank.parse_router(files['MEMORY.md'])[0]
        if (bounded(files)['project_id'] != context['project_id'] or
                shared_router.get('canonical-remote') != local_router.get('canonical-remote')):
            raise ContractError('Shared authority belongs to a different project')
        scope_binding.require(root, context['project_id'], expected=context)
        if sharing_boundary.inspect(root) != boundary or git(root, 'rev-parse', '--verify', 'HEAD') != local_head:
            raise ContractError('Local memory or boundary changed during remote observation')
        return {'repo_root': str(root), 'project_id': context['project_id'], 'bank_root': boundary['bank_root'],
                'authority_remote': remote, 'authority_branch': branch, 'authority_head': head,
                'local_head': local_head, 'scope_context': context,
                'local_bank_sha256': boundary['bank_observation_sha256'],
                'landed_hashes': transactions.file_hashes(files),
                'observed_at': datetime.now(timezone.utc).isoformat(), 'remote_freshness_verified': True}
    finally:
        # Objects remain addressable by immutable IDs, even if the cursor/ref is lost.
        command(root, 'update-ref', '-d', ref)


def result(operation, **values):
    return {'schema': 'agent-memory/sharing-recovery-v1', 'operation': operation,
            'writes_performed': False, 'git_metadata_writes_performed': False, 'canonical_writes_performed': False,
            'publication_verified': False, 'feature_code_freshness_verified': False, **values}


def inspect(repo, fetch=False):
    boundary = sharing_boundary.inspect(repo)
    if not fetch:
        return result('inspect', sharing_state='pending-stale', remote_freshness_verified=False,
                      local_memory_available=True, boundary=boundary)
    try:
        observation = observe(repo)
    except ContractError:
        return result('inspect', writes_performed=None, git_metadata_writes_performed=None, sharing_state='pending-stale', remote_freshness_verified=False,
                      local_memory_available=True, boundary=boundary,
                      reason='Remote unavailable, changed or unverified; retain local capture and retry')
    return result('inspect', writes_performed=True, git_metadata_writes_performed=True, sharing_state='observed', observation=observation,
                  remote_freshness_verified=True, local_memory_available=True)


def choose(base, local, landed):
    if local == landed:
        return local
    if local == base:
        return landed
    if landed == base:
        return local
    raise ContractError('Concurrent changes require semantic reconciliation by the detecting session')


def mapping(base, local, landed):
    merged = {}
    for key in sorted(set(base) | set(local) | set(landed)):
        value = choose(base.get(key, MISSING), local.get(key, MISSING), landed.get(key, MISSING))
        if value is not MISSING:
            merged[key] = value
    return merged


def router_parts(text):
    meta, routes, omitted = bank.parse_router(text)
    _, body = read(text, allowed=meta)
    return meta, {r['path']: r for r in routes}, omitted, body.split('\n## Routes\n', 1)[0]


def index_parts(path, text):
    keys = set(layout.SPEC['properties'])
    meta, body = read(text, allowed=keys)
    return meta, {r['path']: r for r in layout.parse_index(path, meta, body)}, body.split('\n## Routes\n', 1)[0]


def merge_files(base, local, landed):
    checks = [bounded(x) for x in (base, local, landed)]
    if len({x['project_id'] for x in checks}) != 1:
        raise ContractError('A reconciliation cannot transfer memory between projects')
    # Deletion guards are monotonic even if a branch deletes an older marker.
    tombs, removed = {}, set()
    for files, checked in zip((base, local, landed), checks):
        for entry in checked['entries']:
            if entry['format'] == 'tombstone':
                path = entry['path']
                if path in tombs and files[path] != tombs[path]:
                    raise ContractError('Different deletion authorities require explicit reconciliation')
                tombs[path] = files[path];removed.add(entry['id'])
    drops, maps = set(), []
    for files, checked in zip((base, local, landed), checks):
        current = dict(files)
        for entry in checked['entries']:
            if entry['format'] != 'standalone-record':
                continue
            path = entry['path'];metadata, _ = read(files[path], allowed=layout.records.MODEL['properties'])
            if entry['id'] in removed:
                drops.add(path);current.pop(path)
            elif any(ref['id'] in removed for ref in metadata.get('derived-from', [])):
                raise ContractError('A derived observation of deleted learning requires explicit erasure review')
        maps.append(current)
    # Route tables are structured maps. Never use a text merge to choose truth.
    structural = {'MEMORY.md'}
    for checked in checks:
        structural.update(e['path'] for e in checked['entries'] if e['format'] == 'topic-index')
    payloads = [{p: v for p, v in f.items() if p not in structural and p not in tombs} for f in maps]
    merged = mapping(*payloads);merged.update(tombs)
    ordered, visiting, visited = [], set(), set()
    def order(path):
        if path in visiting:raise ContractError('Combined index topology requires explicit reconciliation')
        if path in visited:return
        visiting.add(path)
        for files in maps:
            if path in files:
                for child in index_parts(path, files[path])[1]:
                    if child in structural and child != 'MEMORY.md':order(child)
        visiting.remove(path);visited.add(path);ordered.append(path)
    for path in sorted(structural - {'MEMORY.md'}):order(path)
    for path in ordered:
        values = [f.get(path, MISSING) for f in maps]
        if any(v is MISSING for v in values):
            selected = choose(*values)
            if selected is not MISSING:
                meta, routes, prefix = index_parts(path, selected)
            else:
                continue
        else:
            parts = [index_parts(path, v) for v in values]
            meta = mapping(*[{k:v for k,v in p[0].items() if k != 'updated'} for p in parts])
            meta['updated'] = max(p[0]['updated'] for p in parts)
            routes = mapping(*[{k:v for k,v in p[1].items() if k not in drops} for p in parts]);prefix = choose(*[p[2] for p in parts])
        routes = {p:r for p,r in routes.items() if p in merged}
        if not routes:
            # Preserve custom introductions rather than silently dropping them.
            standard = '# Topic index\n\nLabels name bank-relative files; links are relative to this index.\n'
            if prefix != standard:
                raise ContractError('An empty custom index needs explicit preservation before removal')
            drops.add(path);continue
        merged[path] = write(meta) + prefix + '\n## Routes\n\n' + bank.route_table(list(routes.values()), path)
    parts = [router_parts(f['MEMORY.md']) for f in maps]
    meta = mapping(*[{k:v for k,v in p[0].items() if k != 'router-updated'} for p in parts])
    meta['router-updated'] = max(p[0]['router-updated'] for p in parts)
    routes = mapping(*[{k:v for k,v in p[1].items() if k not in drops} for p in parts]);prefix = choose(*[p[3] for p in parts])
    routes = {p:r for p,r in routes.items() if p in merged}
    # Keep pre-existing tombstone routes even when a branch removed the marker.
    for path in tombs:
        for part in parts:
            if path in part[1]:
                routes[path] = part[1][path];break
    omissions = {}
    for path in bank.CONTEXTS:
        if path not in merged:
            values = [p[2].get(path, MISSING) for p in parts]
            reason = choose(*values)
            if reason is MISSING:
                raise ContractError('A removed ordinary context requires its omission explanation')
            omissions[path] = reason
    merged['MEMORY.md'] = (write(meta) + prefix + '\n## Routes\n\n' + bank.route_table(list(routes.values()))
        + '\n## Omitted contexts\n\n' + (''.join('- `' + p + '`: ' + omissions[p] + '\n' for p in bank.CONTEXTS if p in omissions) or 'None.\n'))
    final = bounded(merged)
    inactive = {}
    for checked in checks:
        for entry in checked['entries']:
            if entry['format'] == 'standalone-record' and entry['lifecycle'] != 'active':
                inactive[entry['id']] = max(inactive.get(entry['id'], 0), entry['version'])
    for entry in final['entries']:
        if (entry['format'] == 'standalone-record' and entry['lifecycle'] == 'active'
                and entry['version'] <= inactive.get(entry['id'], 0)):
            raise ContractError('Inactive guidance cannot reactivate through an old version; review fresh lifecycle evidence')
    return merged


def ancestor(repo, older, newer):
    # rev-list returns nothing only when older is reachable from newer.
    return command(repo, 'rev-list', '--max-count=1', older, '^' + newer).strip() == ''


def reconcile(repo, base=None):
    observation = observe(repo)
    root = Path(observation['repo_root']);prefix = observation['bank_root'];head = observation['authority_head']
    if base is None:
        ancestors = command(root, 'merge-base', '--all', observation['local_head'], head).splitlines()
        if len(ancestors) != 1:
            raise ContractError('Lost cursor has no unique common base; inspect instead of guessing')
        base = ancestors[0]
    commit(root, base)
    if not ancestor(root, base, head) or not ancestor(root, base, observation['local_head']):
        raise ContractError('Observed base is outside current histories; explicit reconciliation required')
    _, hashes = writes.snapshot(root)
    local = transactions.files_at(root / prefix)
    if transactions.file_hashes(local) != hashes:
        raise ContractError('Local memory changed while preparing reconciliation')
    base_files, landed = tree(root, base, prefix), tree(root, head, prefix)
    merged = merge_files(base_files, local, landed)
    if server(root, observation['authority_remote'], observation['authority_branch']) != head or writes.snapshot(root)[1] != hashes or git(root, 'rev-parse', '--verify', 'HEAD') != observation['local_head']:
        raise ContractError('Shared head or local memory advanced; reconcile again')
    return result('reconcile', writes_performed=True, git_metadata_writes_performed=True, observation=observation, base=base, local_hashes=hashes,
                  candidate_files=merged, candidate_hashes=transactions.file_hashes(merged),
                  changed_memory_paths=[prefix + '/' + p for p in sorted(set(local) | set(merged)) if local.get(p) != merged.get(p)],
                  semantic_review_required=True, semantic_approval_verified=False,
                  remote_freshness_verified=True, local_memory_available=True)


def replay(repo, request):
    if not isinstance(request, dict) or set(request) != {'base', 'candidate'}:
        raise ContractError('Replay requires exact base and memory-only candidate commit IDs')
    observation = observe(repo);root = Path(observation['repo_root']);prefix = observation['bank_root']
    base, candidate = (commit(root, request[k]) for k in ('base', 'candidate'))
    parents = command(root, 'rev-list', '--parents', '-n', '1', candidate).strip().split()[1:]
    if parents != [base]:
        raise ContractError('Publication candidate must have exactly the observed authority base as parent')
    changed = command(root, 'diff-tree', '--no-commit-id', '--no-renames', '--name-only', '-r', '-z', base, candidate).split('\0')
    changed = [p for p in changed if p]
    if not changed or any(not p.startswith(prefix + '/') for p in changed):
        raise ContractError('Replay candidate contains no memory delta or imports unrelated code')
    before, after = tree(root, base, prefix), tree(root, candidate, prefix)
    if any(bounded(f)['project_id'] != observation['project_id'] for f in (before, after)):
        raise ContractError('Replay cannot change project identity')
    delta = {p: after.get(p) for p in set(before) | set(after) if before.get(p) != after.get(p)}
    head = observation['authority_head']
    if not ancestor(root, base, head):
        raise ContractError('Authority history changed; preserve candidate and reconcile instead of retrying')
    # Bound historical readback; exceeding it requires explicit inspection, never
    # an assumption that a prior publication did not happen.
    history = command(root, 'rev-list', '--max-count=129', head, '^' + base).splitlines()
    if len(history) > 128:
        raise ContractError('Publication history exceeds bounded replay inventory; explicit readback required')
    witnessed = None
    for revision in history:
        files = tree(root, revision, prefix)
        if all(files.get(p) == body for p, body in delta.items()):
            witnessed = revision;break
    current = tree(root, head, prefix)
    if server(root, observation['authority_remote'], observation['authority_branch']) != head:
        raise ContractError('Authority advanced during replay readback; inventory again')
    return result('replay', writes_performed=True, git_metadata_writes_performed=True, observation=observation, publication_verified=witnessed is not None,
                  intended_delta_landed=witnessed is not None, witnessed_commit=witnessed,
                  intended_delta_current=all(current.get(p) == body for p, body in delta.items()),
                  retry_state='already-landed' if witnessed else 'reconciliation-required',
                  remote_freshness_verified=True, safe_to_retry_without_reconciliation=False)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='operation', required=True)
    for name in ('inspect', 'reconcile', 'replay'):
        p = sub.add_parser(name);p.add_argument('--repo', required=True)
        if name == 'inspect':p.add_argument('--fetch', action='store_true')
        if name == 'reconcile':p.add_argument('--base')
        if name == 'replay':p.add_argument('--request', required=True)
    args = parser.parse_args(argv)
    try:
        if args.operation == 'inspect':value = inspect(args.repo, args.fetch)
        elif args.operation == 'reconcile':value = reconcile(args.repo, args.base)
        else:
            path = Path(args.request)
            with path.open('rb') as stream:raw = stream.read(65537)
            if len(raw) > 65536:raise ContractError('Replay request exceeds bound')
            value = replay(args.repo, load(raw))
        print(json.dumps(value, indent=2));return 0
    except (ContractError, OSError, ValueError, TypeError, KeyError, AttributeError, UnicodeError, RecursionError):
        print(json.dumps(result(args.operation, writes_performed=None if args.operation != 'inspect' or args.fetch else False, git_metadata_writes_performed=None if args.operation != 'inspect' or args.fetch else False, remote_freshness_verified=False, sharing_state='pending-stale',
              error='Unresolved, changed or unsafe sharing state; retain local work and inspect before retry')))
        return 1


if __name__ == '__main__':
    sys.exit(main())
