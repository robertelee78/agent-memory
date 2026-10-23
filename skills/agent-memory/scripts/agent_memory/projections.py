"""Disposable canonical-reference projections; no independent memory bodies."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

from . import bank_writes as writes
from . import inactive, scope_binding
from .developer_identity import encoded, load, uuid_valid
from .frontmatter import ContractError

SCHEMA = 'agent-memory/reference-projection-v1'
MAX_BYTES = 2 * 1024 * 1024


def read_input(path, maximum):
    with Path(path).open('rb') as stream:
        raw = stream.read(maximum + 1)
    if len(raw) > maximum:
        raise ContractError('Projection input exceeds its bounded reader')
    return load(raw)


def value(projection):
    if (not isinstance(projection, dict)
            or set(projection) != {'schema', 'project_id', 'source_hashes', 'records'}
            or projection['schema'] != SCHEMA or not uuid_valid(projection['project_id'])
            or not isinstance(projection['source_hashes'], dict)
            or not projection['source_hashes'] or len(projection['source_hashes']) > 8192
            or any(not isinstance(path, str) or not isinstance(sha, str)
                   or re.fullmatch('[0-9a-f]{64}', sha) is None
                   for path, sha in projection['source_hashes'].items())
            or not isinstance(projection['records'], list) or len(projection['records']) > 64):
        raise ContractError('Projection requires explicit project, complete source hashes and bounded references only')
    seen = set()
    for record in projection['records']:
        if (not isinstance(record, dict) or set(record) != {'id', 'version', 'path'}
                or not uuid_valid(record['id']) or type(record['version']) is not int
                or record['version'] < 1 or not isinstance(record['path'], str)
                or record['path'] not in projection['source_hashes']
                or (record['id'], record['version']) in seen):
            raise ContractError('Projection records must be exact canonical ID/version/path references')
        seen.add((record['id'], record['version']))
    if len(encoded(projection)) > MAX_BYTES:
        raise ContractError('Projection exceeds its bounded representation')
    return projection


def build(repo, request):
    if not isinstance(request, dict) or not uuid_valid(request.get('project_id')):
        raise ContractError('Projection build requires explicit project selection')
    root, _ = writes.bank_path(repo)
    context = scope_binding.require(root, request['project_id'])
    _, before = writes.snapshot(root)
    guarded = inactive.filter_projection(root, request)
    scope_binding.require(root, request['project_id'], expected=context)
    if writes.snapshot(root)[1] != before:
        raise ContractError('Bank changed during projection build; rebuild from canonical input')
    projection = value({'schema': SCHEMA, 'project_id': request['project_id'],
                        'source_hashes': before, 'records': guarded['remaining_references']})
    return {'schema': 'agent-memory/projection-result-v1', 'operation': 'build',
            'projection': projection, 'discarded': guarded['discarded'], 'audit': guarded['audit'],
            'canonical_writes_performed': False, 'audit_writes_performed': True,
            'admission_verified': False, 'current_guidance_verified': False,
            'validation_scope': 'canonical-reference-projection-only'}


def check(repo, projection):
    projection = value(projection)
    root, _ = writes.bank_path(repo)
    context = scope_binding.require(root, projection['project_id'])
    _, observed = writes.snapshot(root)
    if projection['source_hashes'] != observed:
        # Never echo old hashes, record paths or other cached data after source
        # invalidation: those may describe a now-deleted record.
        raise ContractError('Projection sources changed; discard and rebuild before use')
    guarded = inactive.filter_projection(root, {'project_id': projection['project_id'],
                                               'records': projection['records']})
    if guarded['remaining_references'] != projection['records']:
        raise ContractError('Projection contains noncanonical, inactive or deleted references')
    scope_binding.require(root, projection['project_id'], expected=context)
    if writes.snapshot(root)[1] != observed:
        raise ContractError('Bank changed during projection check; discard and rebuild before use')
    return {'schema': 'agent-memory/projection-result-v1', 'operation': 'check', 'valid': True,
            'project_id': projection['project_id'], 'references': len(projection['records']),
            'audit': guarded['audit'], 'canonical_writes_performed': False,
            'audit_writes_performed': True, 'admission_verified': False,
            'current_guidance_verified': False,
            'validation_scope': 'canonical-reference-projection-only'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('build', 'check'):
        command = commands.add_parser(name)
        command.add_argument('--repo', required=True)
        command.add_argument('--input', required=True)
    args = parser.parse_args(argv)
    try:
        request = read_input(args.input, inactive.MAX_INPUT if args.command == 'build' else MAX_BYTES)
        result = build(args.repo, request) if args.command == 'build' else check(args.repo, request)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError, RecursionError):
        print(json.dumps({'schema': 'agent-memory/projection-result-v1', 'operation': args.command,
            'valid': False, 'canonical_writes_performed': False,
            'error': 'Invalid, stale or unauthorized projection/input; preserve canonical memory and rebuild before use'}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
