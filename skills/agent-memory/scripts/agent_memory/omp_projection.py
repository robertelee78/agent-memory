"""Project native stored entries without copying display events or provider state.

The caller must supply a detached SessionManager stored entry, never a native
message event. A returned gap is not assessable source coverage.
"""
import json
import re

from . import security
from .frontmatter import ContractError

MAX_SOURCE = 16 * 1024 * 1024
EXCLUSIONS = ('provider-metadata', 'reasoning-and-signatures')
ROLES = {'user', 'developer', 'assistant', 'toolResult', 'bashExecution', 'pythonExecution', 'fileMention'}


FORBIDDEN_KEYS = {'providerPayload', 'provider_payload', 'thinking', 'reasoning',
                  'thinkingSignature', 'textSignature', 'signature', 'auth',
                  'credentials', 'credential', 'config', 'contextSnapshot'}


def _safe_tree(value):
    """Reject opaque payloads instead of copying them through arbitrary details."""
    if value is None or type(value) in (bool, int, float):
        return
    if isinstance(value, str):
        if any(re.search(pattern, value) for _, pattern in (*security.SECRETS, *security.SUSPICIOUS)):
            raise ContractError('Unsafe native source')
        return
    if isinstance(value, list):
        for child in value:
            _safe_tree(child)
        return
    if isinstance(value, dict):
        if value.get('type') in ('image', 'image_url', 'input_image', 'reasoning', 'thinking'):
            raise ContractError('Opaque native source')
        for key, child in value.items():
            if not isinstance(key, str) or key in FORBIDDEN_KEYS:
                raise ContractError('Opaque native source')
            _safe_tree(key)
            _safe_tree(child)
        return
    raise ContractError('Opaque native source')


def _text(value):
    if not isinstance(value, str):
        raise ContractError('Native source has an unavailable payload')
    return value


def _blocks(value):
    if isinstance(value, str):
        return value, False
    if not isinstance(value, list):
        raise ContractError('Native source has an unavailable payload')
    blocks = []
    gap = False
    for item in value:
        if not isinstance(item, dict):
            raise ContractError('Native source has an unavailable payload')
        kind = item.get('type')
        if kind == 'text':
            blocks.append({'type': 'text', 'text': _text(item.get('text'))})
        elif kind == 'toolCall':
            call = {'type': 'toolCall', 'id': _text(item.get('id')), 'name': _text(item.get('name'))}
            arguments = item.get('arguments')
            if not isinstance(arguments, dict):
                raise ContractError('Native tool arguments are unavailable')
            call['arguments'] = arguments
            blocks.append(call)
        elif kind in ('thinking', 'reasoning'):
            continue
        else:
            # Images, references and unknown blocks need independently checked
            # payload evidence. Do not serialize inline bytes or infer coverage.
            gap = True
    return blocks, gap


def project(entry):
    """Return a projected entry or a fixed, content-free coverage gap.

    This operation does not perform enrollment, canonical writes, or complete
    assessments. Unknown fields are deliberately excluded rather than copied.
    """
    try:
        if not isinstance(entry, dict) or entry.get('type') not in ('message', 'custom_message'):
            return {'gap': 'missing-stored-source'}
        native_id = _text(entry.get('id'))
        parent = entry.get('parentId')
        if parent is not None:
            parent = _text(parent)
        if entry['type'] == 'message':
            message = entry.get('message')
            if not isinstance(message, dict) or message.get('role') not in ROLES:
                return {'gap': 'missing-stored-source'}
            role = message['role']
            result = {'carrier': 'message', 'native_entry_id': native_id,
                      'native_parent_id': parent, 'role': role}
            for key in ('attribution', 'toolCallId', 'toolName', 'isError'):
                if key in message:
                    value = message[key]
                    if key == 'isError':
                        if type(value) is not bool:
                            raise ContractError('Invalid native result flag')
                    else:
                        _text(value)
                    result[key] = value
            content, gap = _blocks(message.get('content'))
            result['content'] = content
            if role == 'toolResult' and 'details' in message:
                result['details'] = message['details']
        else:
            result = {'carrier': 'custom_message', 'native_entry_id': native_id,
                      'native_parent_id': parent, 'customType': _text(entry.get('customType'))}
            if 'attribution' in entry:
                result['attribution'] = _text(entry['attribution'])
            result['content'], gap = _blocks(entry.get('content'))
        _safe_tree(result)
        raw = json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False)
        if len(raw.encode('utf-8')) > MAX_SOURCE:
            return {'gap': 'oversized-source'}
        # Apply the shared patterns to the complete projection, including nested
        # tool arguments/details, before retaining it. Never return matched text.
        if any(re.search(pattern, raw) for _, pattern in (*security.SECRETS, *security.SUSPICIOUS)):
            return {'gap': 'unsafe-representation'}
        if gap:
            return {'gap': 'missing-payload'}
        return {'snapshot': result, 'exclusions': list(EXCLUSIONS)}
    except (ContractError, TypeError, ValueError, UnicodeError, RecursionError):
        return {'gap': 'missing-payload'}
