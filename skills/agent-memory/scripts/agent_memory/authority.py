"""Read-only authority triage of explicitly reviewed, comparable assertions."""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
import sys

from .developer_identity import load, uuid_valid
from .frontmatter import ContractError
from .scopes import conditions, parse_scope

MODEL = json.loads((Path(__file__).resolve().parents[2] / 'schemas/authority-policy.json').read_bytes())
DOMAINS = {row['fact']: row for row in MODEL['domains']}
KINDS = {'memory'} | {kind for row in DOMAINS.values() for kind in row['source-kinds']}


def text(value):
    return isinstance(value, str) and bool(value.strip()) and not any(ord(c) < 32 for c in value)


def comparable(value):
    # Exact typed JSON comparisons, not semantic equivalence of arbitrary prose.
    return json.dumps(value, sort_keys=True, allow_nan=False, ensure_ascii=False, separators=(',', ':'))


def assess(request):
    required = {'fact', 'fact_key', 'project_id', 'memory', 'sources', 'detecting_developer_id'}
    if (not isinstance(request, dict) or not required.issubset(request)
            or not set(request).issubset(required | {'remedy'}) or request['fact'] not in DOMAINS
            or not text(request['fact_key']) or not uuid_valid(request['project_id'])
            or not uuid_valid(request['detecting_developer_id']) or request.get('remedy', 'correct') not in {'correct', 'revoke'}):
        raise ContractError('Authority triage requires a known fact, explicit project/detector and supported remedy')
    memory = request['memory']
    fields = {'id', 'version', 'project_id', 'value', 'scope', 'conditions', 'developer_id', 'evidence'}
    if (not isinstance(memory, dict) or set(memory) != fields or not uuid_valid(memory['id'])
            or memory['project_id'] != request['project_id'] or not uuid_valid(memory['developer_id'])
            or type(memory['version']) is not int or memory['version'] < 1
            or not isinstance(memory['evidence'], list) or any(not text(ref) for ref in memory['evidence'])):
        raise ContractError('A correction candidate must name the existing project record, version and provenance')
    parse_scope(memory['scope'])
    conditions(memory['conditions'])
    remembered = comparable(memory['value'])
    if not isinstance(request['sources'], list):
        raise ContractError('Reviewed source assertions must be a list')
    selected = []
    observed = []
    seen = set()
    for source in request['sources']:
        fields = {'kind', 'ref', 'project_id', 'fact_key', 'current', 'applicable', 'qualified', 'value'}
        if (not isinstance(source, dict) or set(source) != fields or source['kind'] not in KINDS
                or not text(source['ref']) or source['ref'] in seen or source['project_id'] != request['project_id']
                or source['fact_key'] != request['fact_key']
                or any(type(source[key]) is not bool for key in ('current', 'applicable', 'qualified'))):
            raise ContractError('Sources require unique references and explicit same-project, same-fact review assertions')
        seen.add(source['ref'])
        comparable(source['value'])
        observed.append(source['ref'])
        if (source['kind'] in DOMAINS[request['fact']]['source-kinds']
                and source['current'] and source['applicable'] and source['qualified']):
            selected.append(source)
    result = {'schema': 'agent-memory/authority-assessment-v1', 'fact': request['fact'],
              'authority': DOMAINS[request['fact']]['authority'], 'source_refs': [s['ref'] for s in selected],
              'observed_refs': observed, 'memory_is_proof': False, 'source_assertions_verified': False, 'identity_authenticated': False,
              'writes_performed': False, 'validation_scope': 'reviewed-assertion-triage',
              'correction_candidate': None, 'clarification': None}
    if not selected:
        result['action'] = 'inspect-current-authority'
    elif len({comparable(s['value']) for s in selected}) != 1:
        result['action'] = 'clarify'
        result['clarification'] = {'detecting_developer_id': request['detecting_developer_id'],
                                  'known_other_developer_ids': [] if memory['developer_id'] == request['detecting_developer_id'] else [memory['developer_id']],
                                  'question': 'Which applicable current source resolves this project fact?'}
    elif remembered == comparable(selected[0]['value']):
        result['action'] = 'use-current-authority'
    else:
        result['action'] = 'propose-' + request.get('remedy', 'correct')
        candidate = {'operation': request.get('remedy', 'correct'), 'target': copy.deepcopy(memory),
                     'project_id': request['project_id'], 'fact_key': request['fact_key'],
                     'evidence': [s['ref'] for s in selected], 'detected_by': request['detecting_developer_id'],
                     'reason': 'Remembered claim disagrees with applicable current authority'}
        if candidate['operation'] == 'correct':
            candidate['proposed_value'] = copy.deepcopy(selected[0]['value'])
        result['correction_candidate'] = candidate
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('model')
    assessment = sub.add_parser('assess')
    assessment.add_argument('--input', required=True)
    args = parser.parse_args(argv)
    try:
        output = {'model': MODEL, 'writes_performed': False} if args.command == 'model' else assess(load(Path(args.input).read_bytes()))
        print(json.dumps(output, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps({'schema': 'agent-memory/authority-assessment-v1', 'valid': False,
                          'writes_performed': False, 'error': str(exc)}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
