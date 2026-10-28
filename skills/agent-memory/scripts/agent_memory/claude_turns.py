"""Thin native Claude mapping onto the shared source-observation protocol."""
import re
from . import turn_sources
from .frontmatter import ContractError


def complete(repo, value):
    fields = {'session', 'prompt', 'observation_id', 'source_sha256', 'outcome', 'assessment'}
    if not isinstance(value, dict) or set(value) not in (fields, fields | {'publication_request'}):
        raise ContractError('Use an exact native Claude observation assessment')
    if not isinstance(value['observation_id'], str) or not re.fullmatch(r'[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}', value['observation_id']):
        raise ContractError('observation_id must be one local observation UUID, not a shell word list')
    if not isinstance(value['source_sha256'], str) or not re.fullmatch('[0-9a-f]{64}', value['source_sha256']):
        raise ContractError('source_sha256 must be the exact 64-character lowercase prompt digest')
    observed = turn_sources.get(repo, 'claude', value['session'], value['prompt'], value['observation_id'])
    if observed['source_sha256'] != value['source_sha256'] or observed['native_source_id'] != value['prompt']:
        raise ContractError('Claude source differs from its retained observation')
    verified = turn_sources.verify_assessment(repo, value, value['observation_id'])
    result = turn_sources.complete(repo, 'claude', value['session'], value['prompt'],
                                   value['observation_id'], value['source_sha256'], verified)
    return result


def pending(repo, session=None, prompt=None):
    if (session is None) != (prompt is None):
        raise ContractError('Select both native session and prompt, or neither')
    if session is None:
        return turn_sources.backlog(repo, harness='claude')
    state = turn_sources.inspect(repo, 'claude', session, prompt)
    return {'verified': True, 'pending': [{k: x[k] for k in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
                                        for x in state['pending']], 'faults': state['faults'],
            'canonical_writes_performed': False}
