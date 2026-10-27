"""Validate agent-authored no-learning assessments, not semantic truth."""
import json
from .frontmatter import ContractError

def validate(value, source):
    if (not isinstance(value, dict) or set(value) != {'source_id','source_review','candidates'}
            or value['source_id'] != source or not isinstance(value['source_review'], str)
            or not value['source_review'].strip() or len(value['source_review'].encode()) > 2048
            or not isinstance(value['candidates'], list) or len(value['candidates']) > 32):
        raise ContractError('Use a bounded source-bound no-learning assessment')
    if len(json.dumps(value, ensure_ascii=False).encode()) > 2048:
        raise ContractError('No-learning assessment exceeds its bound')
    for candidate in value['candidates']:
        if (not isinstance(candidate, dict) or set(candidate) != {'claim','disposition','reason'}
                or candidate['disposition'] != 'not-durable'
                or any(not isinstance(candidate[k], str) or not candidate[k].strip()
                       or len(candidate[k].encode()) > 1024 for k in ('claim','reason'))):
            raise ContractError('Eligible, ambiguous or unresolved learning remains pending')
    return value
