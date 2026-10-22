"""Pure optional-record inventory and bounded topic-index contract."""
from __future__ import annotations

import argparse
import json
from pathlib import Path, PurePosixPath
import sys

from . import bank_contract as bank
from . import records, scopes
from .developer_identity import load, uuid_valid
from .frontmatter import ContractError, read, write
from .lifecycle import time_value

SPEC = json.loads((Path(__file__).resolve().parents[2] / 'schemas/topic-index.json').read_bytes())


def index_header(metadata, path):
    bank.path_value(path)
    if not path.startswith('topics/'):
        raise ContractError('Topic indices must remain in topics/')
    records.fields(metadata, SPEC['required'], SPEC['properties'])
    if metadata['schema'] != SPEC['$id'] or not uuid_valid(metadata['project-id']):
        raise ContractError('Unknown topic index schema or project identity')
    time_value(metadata['updated'])


def index_text(path, metadata, routes):
    index_header(metadata, path)
    if not isinstance(routes, list) or not routes or len(routes) > SPEC['x-max-routes']:
        raise ContractError('A useful bounded topic index requires explicit routes')
    if any(route['path'] in bank.CONTEXTS for route in routes):
        raise ContractError('The six context files must be routed directly from MEMORY.md')
    # Reparse exactly the rendered bytes so duplicate targets and link framing
    # cannot be accepted only by the writer.
    content = (write(metadata) + '# Topic index\n\nLabels name bank-relative files; links are relative to this index.\n\n## Routes\n\n'
               + bank.route_table(routes, path))
    parse_index(path, metadata, content.split('\n---\n', 1)[1])
    return content


def parse_index(path, metadata, body):
    index_header(metadata, path)
    if len((write(metadata) + body).encode('utf-8')) > SPEC['x-max-bytes'] or body.count('\n## Routes\n') != 1:
        raise ContractError('Topic index requires one bounded route table')
    routes = bank.parse_table(body.split('\n## Routes\n', 1)[1], path)
    if not routes or any(route['path'] in bank.CONTEXTS for route in routes):
        raise ContractError('Topic indices require useful optional-record routes')
    return routes


def constraints(metadata):
    """Resolve declared constraints for structural inheritance, not real context."""
    values = {key: value for key, value in metadata['conditions'].items() if value is not None}
    parsed = scopes.parse_scope(metadata['scope'])
    if parsed:
        kind, value = parsed
        old = values.get(kind)
        if old is not None and old != value:
            if kind != 'component':
                raise ContractError('Record scope contradicts its conditions')
            a, b = PurePosixPath(old), PurePosixPath(value)
            if a in b.parents:
                old = value
            elif b not in a.parents:
                raise ContractError('Record component conditions have no intersection')
            values[kind] = old
        else:
            values[kind] = value
    return values


def inherited(metadata, ancestors, project_id):
    context = constraints(metadata)
    for scope in ancestors:
        result = scopes.decide({'bank_project_id': project_id, 'project_id': project_id,
                                'scope': scope, 'context': context})
        if not result['applicable']:
            raise ContractError('Record must retain inherited index conditions in its own metadata')


def inventory(files, root_metadata, root_routes):
    project_id = root_metadata['project-id']
    entries, visited, versions, active, deleted, events = [], set(), set(), set(), set(), set()
    allowed = (set(records.MODEL['properties']) | set(records.EVENT['properties'])
               | set(records.TOMBSTONE['required']) | set(SPEC['properties']))

    def visit(route, ancestors, index_depth, stack):
        path = route['path']
        if path in stack:
            raise ContractError('Topic index cycle; preserve the bank for clarification')
        if path in visited:
            raise ContractError('A file must have one unambiguous route')
        if path not in files:
            raise ContractError('A routed optional file is missing')
        visited.add(path)
        chain = [*ancestors, route['scope']]
        if path in bank.CONTEXTS:
            entries.append({'path': path, 'format': 'context-prose', 'scope_chain': chain,
                            'lifecycle': route['lifecycle']})
            return
        try:
            metadata, body = read(files[path], allowed=allowed)
            if metadata.get('schema') == SPEC['$id']:
                if len(files[path].encode('utf-8')) > SPEC['x-max-bytes']:
                    raise ContractError('Topic index byte limit exceeded')
                if index_depth >= SPEC['x-max-nested-indexes']:
                    raise ContractError('Topic index nesting limit exceeded')
                if metadata['project-id'] != project_id or route['lifecycle'] != 'active':
                    raise ContractError('Topic index project/state does not match the bank')
                children = parse_index(path, metadata, body)
                entries.append({'path': path, 'format': 'topic-index', 'scope_chain': chain,
                                'lifecycle': route['lifecycle']})
                for child in children:
                    visit(child, chain, index_depth + 1, [*stack, path])
                return
            kind, metadata = records.validate(metadata, body)
        except (ContractError, ValueError, TypeError, KeyError, AttributeError):
            # Malformed tombstones can contain secrets in keys or values.
            raise ContractError('Invalid optional record, index or history; inspect the original file') from None
        entry = {'path': path, 'format': kind, 'scope_chain': chain, 'lifecycle': route['lifecycle']}
        if kind == 'tombstone':
            if (path != 'history/tombstones/' + metadata['id'] + '.md' or metadata['scope'] != project_id
                    or route['scope'] != 'repository' or route['lifecycle'] != 'deleted' or metadata['id'] in deleted):
                raise ContractError('Safe tombstone path, project or route does not match')
            # Deletion is repository-wide for the stable ID, not conditional on
            # an index's branch scope.
            if any(scope != 'repository' for scope in ancestors):
                raise ContractError('Tombstones must not inherit conditional index scopes')
            deleted.add(metadata['id'])
            entry['id'] = metadata['id']
        elif kind == 'optional-history-event':
            if (not path.startswith('history/') or path.startswith('history/tombstones/') or metadata['project-id'] != project_id
                    or route['scope'] != 'repository' or route['lifecycle'] != 'active' or metadata['id'] in events):
                raise ContractError('Optional historical events belong to this project history')
            events.add(metadata['id'])
            entry.update(id=metadata['id'], current_guidance=False)
        else:
            if (metadata['project-id'] != project_id or route['scope'] != metadata['scope']
                    or route['lifecycle'] != metadata['status'] or metadata['status'] in {'candidate', 'quarantined'}
                    or path.startswith('history/tombstones/')):
                raise ContractError('Standalone project record must match its admitted or historical route')
            if path.startswith('procedures/') and metadata['kind'] != 'procedural-candidate':
                raise ContractError('Procedure files must remain non-binding procedural candidates')
            if path.startswith('history/') and metadata['status'] == 'active':
                raise ContractError('Optional history cannot contain the only current record')
            inherited(metadata, ancestors, project_id)
            key = (metadata['id'], metadata['version'])
            if key in versions or (metadata['status'] == 'active' and metadata['id'] in active):
                raise ContractError('Duplicate record version or ambiguous active versions')
            versions.add(key)
            if metadata['status'] == 'active':
                active.add(metadata['id'])
            entry.update(id=metadata['id'], version=metadata['version'], kind=metadata['kind'],
                         applicability=metadata['applicability'], conditions=metadata['conditions'],
                         binding_procedure=False)
        entries.append(entry)

    for route in root_routes:
        visit(route, [], 0, [])
    if set(files) != {'MEMORY.md', *visited}:
        raise ContractError('Project Markdown must be reachable from the bounded route map')
    if deleted & {identity for identity, _ in versions}:
        raise ContractError('Deleted record content remains in the canonical bank; reconcile erasure')
    return entries


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('model')
    render = sub.add_parser('render-index')
    render.add_argument('--input', required=True)
    check = sub.add_parser('validate-index')
    check.add_argument('--file', required=True)
    check.add_argument('--path', required=True, help='Canonical bank-relative location of this index')
    inspect = sub.add_parser('inspect')
    inspect.add_argument('--repo', default='.')
    args = parser.parse_args(argv)
    try:
        if args.command == 'model':
            output = {'model': SPEC, 'writes_performed': False}
        elif args.command == 'inspect':
            from .bank_writes import snapshot
            checked, hashes = snapshot(args.repo)
            output = {'schema': 'agent-memory/layout-result-v1', 'project_id': checked['project_id'],
                      'entries': checked['entries'], 'source_hashes': hashes, 'writes_performed': False,
                      'validation_scope': 'routed-layout-and-record-format', 'admission_verified': False,
                      'sources_verified': False, 'identity_authenticated': False}
        elif args.command == 'render-index':
            request = load(Path(args.input).read_bytes())
            records.fields(request, {'path', 'project-id', 'updated', 'routes'}, {'path', 'project-id', 'updated', 'routes'})
            metadata = {'schema': SPEC['$id'], 'project-id': request['project-id'], 'updated': request['updated']}
            output = {'schema': 'agent-memory/layout-result-v1', 'path': request['path'],
                      'content': index_text(request['path'], metadata, request['routes']), 'writes_performed': False}
        else:
            raw = Path(args.file).read_bytes()
            if len(raw) > SPEC['x-max-bytes']:
                raise ContractError('Topic index byte limit exceeded')
            metadata, body = read(raw.decode('utf-8'), allowed=SPEC['properties'])
            output = {'schema': 'agent-memory/layout-result-v1', 'path': args.path,
                      'routes': parse_index(args.path, metadata, body), 'valid': True, 'writes_performed': False}
        print(json.dumps(output, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError):
        print(json.dumps({'schema': 'agent-memory/layout-result-v1', 'valid': False, 'writes_performed': False,
                          'error': 'Invalid optional record layout, index or canonical bank; inspect original files'}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
