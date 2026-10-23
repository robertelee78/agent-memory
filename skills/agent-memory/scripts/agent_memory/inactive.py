"""Minimal inactive representations and canonical invalidation of derived input.

These helpers do not choose a lifecycle change, admit learning or return recalled
text. They keep stale projection bodies from supplying their own replacement.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import uuid

from . import bank_writes as writes
from . import records, scope_binding, security, tombstones
from .developer_identity import digest, load, uuid_valid
from .frontmatter import ContractError, write

INACTIVE = {'superseded', 'revoked', 'expired'}
MAX_INPUT = 65536


def bounded_file(path):
    with Path(path).open('rb') as stream:
        data = stream.read(MAX_INPUT + 1)
    if len(data) > MAX_INPUT:
        raise ContractError('Inactive representation input exceeds its bounded reader')
    return data


def compact(content):
    kind, metadata, _ = records.parse(content)
    if kind == 'tombstone':
        rendered = tombstones.render(metadata)
        return {'schema': 'agent-memory/inactive-result-v1', 'operation': 'compact',
                'format': 'tombstone', 'content': rendered['content'],
                'writes_performed': False, 'lifecycle_change_performed': False,
                'validation_scope': 'safe-representation-only'}
    if kind != 'standalone-record' or metadata['status'] not in INACTIVE:
        raise ContractError('Compaction requires an already inactive record or exact terminal tombstone')
    # Original bodies and optional explanatory/admission fields are not carried
    # forward. Preserve the required conditional evidence and original identity.
    kept = {key: value for key, value in metadata.items()
            if key in records.MODEL['required'] or key in {'state-reason', 'superseded-by'}}
    body = '# Inactive memory\n\nThis version is ' + metadata['status'] + '; it is excluded from current guidance.\n'
    rendered = write(kept) + body
    records.parse(rendered)
    # Unsafe evidence/conditions/provenance cannot be silently discarded or
    # replaced with invented safe facts. Leave that decision to governed erasure.
    security.ensure_safe({'inactive-summary': rendered})
    return {'schema': 'agent-memory/inactive-result-v1', 'operation': 'compact',
            'format': 'standalone-record', 'content': rendered,
            'writes_performed': False, 'lifecycle_change_performed': False,
            'validation_scope': 'safe-representation-only'}


def filter_projection(repo, request):
    if (not isinstance(request, dict) or set(request) != {'project_id', 'records'}
            or not uuid_valid(request['project_id']) or not isinstance(request['records'], list)
            or len(request['records']) > 64):
        raise ContractError('Derived input needs an explicit project and at most 64 versioned record references')
    seen = set()
    for item in request['records']:
        if (not isinstance(item, dict) or not uuid_valid(item.get('id'))
                or type(item.get('version')) is not int or item['version'] < 1
                or (item['id'], item['version']) in seen):
            raise ContractError('Derived records need unique stable IDs and positive versions')
        seen.add((item['id'], item['version']))
    root, bank = writes.bank_path(repo)
    context = scope_binding.require(root, request['project_id'])
    checked, observed = writes.snapshot(root)
    project, deletion_hashes, deleted = tombstones.history(root)
    if deletion_hashes != observed or project != request['project_id']:
        raise ContractError('Canonical bank changed before derived-input invalidation')
    canonical = {}
    for entry in checked['entries']:
        if entry['format'] != 'standalone-record':
            continue
        raw = (bank / entry['path']).read_bytes()
        if digest(raw) != observed[entry['path']]:
            raise ContractError('Canonical source changed during derived-input invalidation')
        _, metadata, _ = records.parse(raw.decode('utf-8'))
        canonical[(metadata['id'], metadata['version'])] = (metadata, entry['path'])
    remaining = []
    for item in request['records']:
        if item['id'] in deleted:
            continue
        current = canonical.get((item['id'], item['version']))
        if current is not None and current[0]['status'] == 'active':
            # Do not forward cached prose, hashes, titles, conditions, roles or
            # arbitrary metadata even for a still-active identity/version.
            remaining.append({'id': item['id'], 'version': item['version'], 'path': current[1]})
    scope_binding.require(root, project, expected=context)
    if writes.snapshot(root)[1] != observed:
        raise ContractError('Canonical bank changed; repeat invalidation before rebuilding or consolidation')
    audit = security.audit(root, str(uuid.uuid4()), 'retrieval', 'checked', context,
        ['external'], record_ids=sorted({item['id'] for item in request['records']}))
    return {'schema': 'agent-memory/inactive-result-v1', 'operation': 'filter',
            'project_id': project, 'remaining_references': remaining,
            'discarded': len(request['records']) - len(remaining), 'audit': audit,
            'cached_payloads_returned': False, 'current_guidance_verified': False,
            'canonical_writes_performed': False, 'audit_writes_performed': True,
            'validation_scope': 'erasure-invalidation-only'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    render = commands.add_parser('compact')
    render.add_argument('--file', required=True)
    guard = commands.add_parser('filter')
    guard.add_argument('--repo', required=True)
    guard.add_argument('--input', required=True)
    args = parser.parse_args(argv)
    try:
        output = (compact(bounded_file(args.file).decode('utf-8')) if args.command == 'compact'
                  else filter_projection(args.repo, load(bounded_file(args.input))))
        print(json.dumps(output, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError):
        # Invalid metadata, paths and even parser exceptions may carry deleted
        # content. Refusals never quote it or compute a content hash for output.
        print(json.dumps({'schema': 'agent-memory/inactive-result-v1', 'operation': args.command,
            'error': 'Invalid or unsafe inactive representation, scope, canonical state or derived input; preserve original state for inspection',
            'verified': False, **({'writes_performed': False} if args.command == 'compact' else {}),
            'canonical_writes_performed': False}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
