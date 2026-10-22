"""Materialize typed test memory; these assertions are disposable fixture data."""
import json
from pathlib import Path
import uuid

ROOT = Path(__file__).resolve().parents[1]


def record_text(body, project_id, scope='repository', *, kind='semantic-fact', status='active'):
    proposal = json.loads((ROOT / 'skills/agent-memory/templates/record-proposal.example.json').read_bytes())
    metadata = proposal['metadata']
    metadata.update({'id': str(uuid.uuid4()), 'project-id': project_id, 'kind': kind, 'status': status,
                     'scope': scope, 'conditions': {}, 'trust': 'reviewed', 'visibility': 'visible',
                     'admission-reason': 'Disposable structural fixture; not real admission evidence'})
    metadata['origin'].update({'developer-id': str(uuid.uuid4()), 'code-state': 'not-code'})
    if status in {'quarantined', 'superseded', 'revoked', 'expired'}:
        metadata['state-reason'] = 'Historical fixture; excluded from current guidance'
    if status == 'superseded':
        metadata['superseded-by'] = [{'id': metadata['id'], 'version': 2}]
    return '---\n' + ''.join(f'{key}: {json.dumps(value)}\n' for key, value in metadata.items()) + '---\n' + body


def project_id(router):
    return json.loads(next(line.split(': ', 1)[1] for line in router.splitlines() if line.startswith('project-id: ')))
