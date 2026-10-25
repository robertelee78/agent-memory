"""Inspect canonical current files and optional local Git facts without runtime state."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

from . import repository_config
from . import bank_contract as contract
from . import bank_writes as writes
from .frontmatter import ContractError


def git_context(root, observed):
    if shutil.which('git') is None:
        return {'status': 'unavailable', 'reason': 'Git is not installed; current Markdown remains readable'}
    env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
    env.update(GIT_NO_LAZY_FETCH='1', GIT_ALLOW_PROTOCOL='', GIT_OPTIONAL_LOCKS='0',
               GIT_TRACE='0', GIT_TRACE2='0', GIT_TRACE2_EVENT='0', GIT_TRACE2_PERF='0')
    command = ['git', '--no-pager', '--no-replace-objects', '--literal-pathspecs',
               '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false',
               '-C', str(root)]

    def run(*args):
        return subprocess.run([*command, *args], env=env, capture_output=True, timeout=10)

    try:
        top = run('rev-parse', '--show-toplevel')
        if top.returncode:
            return {'status': 'not-discovered', 'reason': 'No usable Git checkout was discovered; no Git audit claim'}
        if Path(os.fsdecode(top.stdout).removesuffix('\n')).resolve() != root:
            return {'status': 'different-root', 'reason': 'Selected bank root is not the Git checkout root; resolve explicitly'}
        head = run('rev-parse', '--verify', 'HEAD')
        oid = head.stdout.decode('ascii').strip() if head.returncode == 0 else None
        if oid is not None and re.fullmatch(r'(?:[0-9a-f]{40}|[0-9a-f]{64})', oid) is None:
            raise ContractError('Invalid Git context reference')
        index = run('ls-files', '--stage', '-z', '--', repository_config.relative(root))
        if index.returncode:
            return {'status': 'unavailable', 'reason': 'Git index could not be inspected; no Git audit claim'}
        prefix = repository_config.relative(root) + '/'
        entries = []
        for record in index.stdout.split(b'\0'):
            if not record:
                continue
            metadata, path = record.split(b'\t', 1)
            mode, object_id, stage = metadata.decode('ascii').split()
            path = os.fsdecode(path)
            if path.startswith(prefix) and path[len(prefix):] in observed:
                entries.append({'path': path[len(prefix):], 'mode': mode, 'object_id': object_id, 'stage': int(stage)})
        indexed = {entry['path'] for entry in entries if entry['stage'] == 0}
        return {'status': 'checkout', 'context_head': oid, 'index_entries': entries,
                'indexed_paths': sorted(indexed), 'not_indexed_paths': sorted(set(observed) - indexed),
                'unmerged_paths': sorted({entry['path'] for entry in entries if entry['stage'] != 0}),
                'current_bytes_committed': 'not-assessed'}
    except (OSError, UnicodeError, ValueError, subprocess.TimeoutExpired):
        return {'status': 'unavailable', 'reason': 'Local Git inspection unavailable; current Markdown remains the source'}


def inspect(repo, path=None):
    root, bank = writes.bank_path(repo)
    checked, observed = writes.snapshot(root)
    if path is not None:
        return inspect_context(root, bank, checked, observed, path)
    git = git_context(root, observed)
    latest, hashes = writes.snapshot(root)
    if latest != checked or hashes != observed or git_context(root, hashes) != git:
        raise ContractError('Bank or local Git context changed during inspection; reread before use')
    routes = {route['path']: route for route in checked['routes']}
    contexts = {name: {'present': name in observed, 'route': routes.get(name),
                       'omission_reason': checked['omitted'].get(name)} for name in contract.CONTEXTS}
    return {'schema': 'agent-memory/current-bank-view-v1', 'operation': 'inspect',
            'project_id': checked['project_id'], 'canonical_source': 'Markdown',
            'contexts': contexts, 'routes': checked['routes'], 'source_hashes': observed, 'git': git,
            'sharing': {'state': 'local-or-pending-sharing', 'publication_verified': False, 'freshness_verified': False},
            'reads_memory_generated_state': False, 'writes_performed': False,
            'validation_scope': 'current-file-inventory-and-local-git-context'}


def inspect_context(root, bank, checked, observed, path):
    from . import scope_binding, security, handoff
    from .retrieval import observed_context
    from .developer_identity import digest
    if path not in contract.CONTEXTS or path not in observed:
        raise ContractError('Explicit historical inspection selects an existing ordinary context file only')
    scope = scope_binding.require(root, checked['project_id'])
    context, _ = observed_context(root, {})
    with (bank / path).open('rb') as stream:
        raw = stream.read(16385)
    if len(raw) > 16384 or digest(raw) != observed[path]:
        raise ContractError('Context is oversized or changed during inspection')
    body = raw.decode('utf-8')
    scanned = security.scan(body)
    value = {'schema': 'agent-memory/context-inspection-v1', 'operation': 'inspect',
             'project_id': checked['project_id'], 'inspection_class': 'historical-orientation',
             'assignment': False, 'usable_as_current_guidance': False,
             'source_assertions_verified': False, 'identity_authenticated': False,
             'publication_verified': False, 'writes_performed': False}
    if scanned['reasons']:
        value.update(withheld=True, reasons=scanned['reasons'])
    else:
        assessment = handoff.assess(body, checked['project_id'], context, root, bank, inspect_only=True) if path == 'activeContext.md' else None
        value.update(path=path, body=body, source_sha256=observed[path],
                     untrusted_data=True, trust=None, context_assessment=assessment)
    if writes.snapshot(root)[1] != observed or observed_context(root, {})[0] != context:
        raise ContractError('Bank or checkout changed during historical inspection')
    scope_binding.require(root, checked['project_id'], expected=scope)
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    current = sub.add_parser('inspect')
    current.add_argument('--repo', default='.')
    current.add_argument('--path')
    args = parser.parse_args(argv)
    try:
        print(json.dumps(inspect(args.repo, args.path), indent=2))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps({'schema': 'agent-memory/current-bank-view-v1', 'valid': False,
                          'writes_performed': False, 'error': str(exc)}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
