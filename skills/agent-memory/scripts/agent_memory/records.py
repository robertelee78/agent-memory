"""Read-only standalone Markdown record and optional history format contract."""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import sys

from .developer_identity import load, uuid_valid
from .frontmatter import ContractError, read, write
from .lifecycle import time_value
from . import scopes

PACK = Path(__file__).resolve().parents[2]
MODEL = json.loads((PACK / 'schemas/record.json').read_bytes())
EVENT = json.loads((PACK / 'schemas/history-event.json').read_bytes())
TOMBSTONE = json.loads((PACK / 'schemas/tombstone.json').read_bytes())
POLICY_FIELDS = MODEL['x-policy-fields']


def text(value):
    if (not isinstance(value, str) or not value.strip() or value != value.strip()
            or any(ord(c) < 32 or ord(c) == 127 for c in value)):
        raise ContractError('A nonempty explicit metadata string is required')
    return value


def fields(value, required, allowed):
    if not isinstance(value, dict) or not set(required).issubset(value) or not set(value).issubset(allowed):
        raise ContractError('Metadata requires the known fields and no extras')


def identity(value):
    if not uuid_valid(value):
        raise ContractError('Canonical random UUIDv4 identity required')


def reference(value):
    fields(value, {'id', 'version'}, {'id', 'version'})
    identity(value['id'])
    if type(value['version']) is not int or value['version'] < 1:
        raise ContractError('An exact positive record version is required')
    return value


def captured_source_path(capture_id, source_id):
    identity(capture_id);identity(source_id)
    return 'history/sources/' + capture_id + '/' + source_id + '.md'


def source_capture_id(path, metadata):
    """Original proposal identity survives inactive-body compaction and erasure."""
    parts = path.split('/')
    if (len(parts) == 4 and parts[:2] == ['history', 'sources'] and uuid_valid(parts[2])
            and parts[3] == metadata['id'] + '.md'
            and {'id': parts[2], 'version': 1} in metadata['derived-from']):
        return parts[2]
    return None


def references(values, record, *, replacement=False, correction=False):
    if not isinstance(values, list):
        raise ContractError('Version links must be a list')
    seen = set()
    for item in values:
        reference(item)
        key = (item['id'], item['version'])
        if key in seen:
            raise ContractError('Duplicate version link')
        seen.add(key)
        if item['id'] == record['id']:
            if (replacement and item['version'] <= record['version']) or (not replacement and item['version'] >= record['version']):
                raise ContractError('Self-version links must point to the proper earlier or replacement version')
        if correction and (item['id'] != record['id'] or item['version'] >= record['version']):
            raise ContractError('A correction must retain the record ID and name an earlier version')


def evidence(values):
    if not isinstance(values, list) or not values:
        raise ContractError('At least one evidence reference is required')
    for item in values:
        text(item)
    if len(values) != len(set(values)):
        raise ContractError('Duplicate evidence reference')


def record(metadata, required_metadata=()):
    fields(metadata, MODEL['required'], MODEL['properties'])
    if metadata['schema'] != MODEL['$id']:
        raise ContractError('Unknown record schema')
    reference({'id': metadata['id'], 'version': metadata['version']})
    identity(metadata['project-id'])
    for key in ('kind', 'status', 'applicability'):
        if metadata[key] not in MODEL['properties'][key]['enum']:
            raise ContractError('Unknown record kind, lifecycle or applicability')
    parsed_scope = scopes.parse_scope(metadata['scope'])
    conditions = scopes.conditions(metadata['conditions'], allow_unknown=True)
    source = metadata['source']
    fields(source, {'actor', 'kind'}, {'actor', 'kind'})
    text(source['actor'])
    text(source['kind'])
    evidence(metadata['evidence'])
    origin = metadata['origin']
    spec = MODEL['properties']['origin']
    fields(origin, spec['required'], spec['properties'])
    identity(origin['developer-id'])
    text(origin['harness'])
    text(origin['session'])
    if origin['code-state'] not in spec['properties']['code-state']['enum']:
        raise ContractError('Explicit originating code state required')
    # This is a minimum format guard, not a claim that these conditions suffice
    # semantically or that the invoking branch/revision was actually inspected.
    conditional = ((parsed_scope is not None and parsed_scope[0] == 'branch')
                   or any(conditions.get(k) is not None for k in ('branch', 'revision')))
    unknown = metadata['applicability'] == 'unknown' or any(v is None for v in conditions.values())
    if origin['code-state'] == 'unmerged' and not conditional and not unknown:
        raise ContractError('Unmerged observations require explicit branch/revision conditions or unknown applicability')
    if time_value(metadata['updated']) < time_value(metadata['recorded-at']):
        raise ContractError('Updated time cannot precede recording')
    start = time_value(metadata['valid-from']) if metadata['valid-from'] is not None else None
    end = time_value(metadata['valid-until']) if metadata['valid-until'] is not None else None
    if start is not None and end is not None and end <= start:
        raise ContractError('Validity must have a strictly later exclusive end')
    for key in ('derived-from', 'corrects', 'supersedes'):
        references(metadata[key], metadata, correction=key == 'corrects')
    required = set(required_metadata)
    if not required.issubset(POLICY_FIELDS):
        raise ContractError('Unknown policy-required metadata field')
    if metadata['status'] == 'active':
        required.update(('admission-reason', 'trust', 'visibility'))
    if not required.issubset(metadata):
        raise ContractError('Policy-required metadata is missing')
    for key in ('admission-reason', 'redaction', 'state-reason'):
        if key in metadata:
            text(metadata[key])
    for key in ('trust', 'visibility'):
        if key in metadata and metadata[key] not in MODEL['properties'][key]['enum']:
            raise ContractError('Unknown trust or visibility')
    confidence = metadata.get('confidence')
    if confidence is not None and (type(confidence) not in {int, float} or not 0 <= confidence <= 1 or not math.isfinite(confidence)):
        raise ContractError('Confidence must be null or finite in the range zero to one')
    if metadata['status'] in {'quarantined', 'superseded', 'revoked', 'expired'} and 'state-reason' not in metadata:
        raise ContractError('Non-current lifecycle needs a visible reason')
    if metadata['status'] == 'superseded' and not metadata.get('superseded-by'):
        raise ContractError('Superseded records must name the replacement')
    if 'superseded-by' in metadata:
        if not metadata['superseded-by']:
            raise ContractError('Replacement links cannot be empty')
        references(metadata['superseded-by'], metadata, replacement=True)
    return metadata


def event(metadata):
    fields(metadata, EVENT['required'], EVENT['properties'])
    if metadata['schema'] != EVENT['$id'] or metadata['kind'] not in EVENT['properties']['kind']['enum']:
        raise ContractError('Unknown historical event; deletion uses only a safe tombstone')
    identity(metadata['id'])
    identity(metadata['project-id'])
    reference(metadata['record'])
    text(metadata['actor'])
    time_value(metadata['recorded-at'])
    evidence(metadata['evidence'])
    return metadata


def validate(metadata, body, required_metadata=()):
    if not isinstance(metadata, dict) or not isinstance(body, str):
        raise ContractError('Explicit metadata and Markdown body are required')
    body.encode('utf-8')
    if set(metadata) == set(TOMBSTONE['required']):
        if required_metadata:
            raise ContractError('Tombstones must not retain policy or provenance fields')
        # Lazy import avoids the bank-reader/record-reader dependency cycle when
        # the full optional-record inventory consumes this contract.
        from .tombstones import parse
        return 'tombstone', parse(write(metadata) + body)
    if metadata.get('schema') == EVENT['$id']:
        if body.strip() or required_metadata:
            raise ContractError('Optional history events have empty bodies and their own format')
        return 'optional-history-event', event(metadata)
    if not body.strip():
        raise ContractError('Standalone current memory requires a readable Markdown body')
    return 'standalone-record', record(metadata, required_metadata)


def parse(content, required_metadata=()):
    allowed = set(MODEL['properties']) | set(EVENT['properties']) | set(TOMBSTONE['required'])
    metadata, body = read(content, allowed=allowed)
    kind, metadata = validate(metadata, body, required_metadata)
    return kind, metadata, body


def applicability(metadata, request):
    fields(request, {'bank_project_id', 'context'}, {'bank_project_id', 'context'})
    record(metadata)
    result = scopes.decide({'bank_project_id': request['bank_project_id'], 'project_id': metadata['project-id'],
                            'scope': metadata['scope'], 'conditions': metadata['conditions'], 'context': request['context']})
    if metadata['applicability'] == 'unknown':
        result['applicable'] = False
        result['reasons'].append('unknown-applicability')
    result['validation_scope'] = 'record-applicability-only; not lifecycle-currentness-or-admission'
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('model')
    render = sub.add_parser('render')
    render.add_argument('--input', required=True)
    check = sub.add_parser('validate')
    check.add_argument('--file', required=True)
    applicable = sub.add_parser('applicability')
    applicable.add_argument('--file', required=True)
    applicable.add_argument('--input', required=True)
    for command in (render, check, applicable):
        command.add_argument('--require', action='append', default=[], choices=POLICY_FIELDS)
    args = parser.parse_args(argv)
    try:
        if args.command == 'model':
            output = {'record': MODEL, 'optional_history_event': EVENT, 'tombstone': TOMBSTONE, 'writes_performed': False}
        else:
            if args.command == 'render':
                request = load(Path(args.input).read_bytes())
                fields(request, {'metadata', 'body'}, {'metadata', 'body'})
                kind, metadata = validate(request['metadata'], request['body'], args.require)
                body = request['body']
            else:
                kind, metadata, body = parse(Path(args.file).read_bytes().decode('utf-8'), args.require)
            if args.command == 'applicability':
                if kind != 'standalone-record':
                    raise ContractError('Only standalone records have conditional applicability')
                output = applicability(metadata, load(Path(args.input).read_bytes()))
            else:
                output = {'schema': 'agent-memory/record-result-v1', 'format': kind, 'metadata': metadata,
                          'body': body, 'valid': True, 'writes_performed': False,
                          'validation_scope': 'format-and-local-invariants', 'identity_authenticated': False,
                          'sources_verified': False, 'admission_verified': False}
                if args.command == 'render':
                    output['content'] = write(metadata) + body
        print(json.dumps(output, indent=2, ensure_ascii=False, allow_nan=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError):
        # Invalid inputs may be malformed deletion markers containing secrets.
        print(json.dumps({'schema': 'agent-memory/record-result-v1', 'valid': False,
                          'writes_performed': False, 'error': 'Invalid record, history event, tombstone or applicability request; inspect the original input'}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
