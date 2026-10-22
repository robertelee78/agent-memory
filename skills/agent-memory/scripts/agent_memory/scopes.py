"""Shared repository namespace and applicability decisions, never an access ACL."""
from __future__ import annotations

import argparse
import json
from pathlib import Path, PurePosixPath
import sys

from .developer_identity import load, uuid_valid
from .frontmatter import ContractError

MODEL = json.loads((Path(__file__).resolve().parents[2] / 'schemas/scope-policy.json').read_bytes())
KINDS = MODEL['condition-kinds']


def condition_value(kind, value, *, allow_unknown=False):
    if allow_unknown and kind in KINDS and value is None:
        return None
    if (kind not in KINDS or not isinstance(value, str) or not value or value != value.strip()
            or any(ord(c) < 32 or ord(c) == 127 or c in '|\\' for c in value)):
        raise ContractError('Applicability requires an explicit known condition and nonempty value')
    if kind == 'component':
        path = PurePosixPath(value)
        if (path.is_absolute() or str(path) != value or value == '.' or ':' in value
                or any(part in {'.', '..'} for part in path.parts)):
            raise ContractError('Component conditions require canonical repository-relative paths')
    return value


def parse_scope(scope):
    if scope == MODEL['default-scope']:
        return None
    if not isinstance(scope, str) or ':' not in scope:
        raise ContractError('Scope must be repository or an explicit applicability condition')
    kind, value = scope.split(':', 1)
    if kind not in MODEL['route-kinds']:
        raise ContractError('Scope requires a known route kind')
    return kind, condition_value(kind, value)


def conditions(value, *, allow_unknown=False):
    if not isinstance(value, dict):
        raise ContractError('Conditions and observed context must be objects')
    return {key: condition_value(key, item, allow_unknown=allow_unknown) for key, item in value.items()}


def decide(request):
    required = {'bank_project_id', 'project_id'}
    allowed = required | {'scope', 'conditions', 'context', 'developer_id'}
    if (not isinstance(request, dict) or not required.issubset(request) or not set(request).issubset(allowed)
            or any(not uuid_valid(request[key]) for key in required)
            or ('developer_id' in request and not uuid_valid(request['developer_id']))):
        raise ContractError('Scope decisions require explicit canonical project IDs and known fields')
    scope = request.get('scope', MODEL['default-scope'])
    parsed = parse_scope(scope)
    requirements = list(conditions(request.get('conditions', {}), allow_unknown=True).items())
    if parsed is not None:
        requirements.insert(0, parsed)
    context = conditions(request.get('context', {}))
    same_project = request['bank_project_id'] == request['project_id']
    reasons = [] if same_project else ['different-project']
    for kind, expected in requirements:
        actual = context.get(kind)
        if expected is None:
            reasons.append('unknown-condition:' + kind)
        elif actual is None:
            reasons.append('missing-context:' + kind)
        elif kind == 'component':
            wanted = PurePosixPath(expected)
            observed = PurePosixPath(actual)
            if observed != wanted and wanted not in observed.parents:
                reasons.append('different-context:' + kind)
        elif expected != actual:
            reasons.append('different-context:' + kind)
    return {'schema': 'agent-memory/scope-decision-v1', 'same_project': same_project,
            'scope': scope, 'applicable': not reasons, 'reasons': list(dict.fromkeys(reasons)),
            'validation_scope': 'repository-applicability-only', 'identity_authenticated': False,
            'writes_performed': False}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('model')
    select = sub.add_parser('decide')
    select.add_argument('--input', required=True)
    args = parser.parse_args(argv)
    try:
        output = {'model': MODEL, 'writes_performed': False} if args.command == 'model' else decide(load(Path(args.input).read_bytes()))
        print(json.dumps(output, indent=2))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps({'schema': 'agent-memory/scope-decision-v1', 'valid': False,
                          'writes_performed': False, 'error': str(exc)}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
