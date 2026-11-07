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
ROLES = {'user', 'developer', 'assistant', 'toolResult', 'bashExecution', 'pythonExecution', 'fileMention', 'custom', 'hookMessage'}


FORBIDDEN_KEYS = {'providerPayload', 'provider_payload', 'thinking', 'reasoning',
                  'thinkingSignature', 'textSignature', 'signature', 'auth',
                  'credentials', 'credential', 'config', 'contextSnapshot'}


class UnsafeSource(ContractError):
    pass


def _safe_tree(value):
    """Reject opaque payloads instead of copying them through arbitrary details."""
    if value is None or type(value) in (bool, int, float):
        return
    if isinstance(value, str):
        if any(re.search(pattern, value) for _, pattern in (*security.SECRETS, *security.SUSPICIOUS)):
            raise UnsafeSource('Unsafe native source')
        return
    if isinstance(value, list):
        for child in value:
            _safe_tree(child)
        return
    if isinstance(value, dict):
        if value.get('type') in ('image', 'image_url', 'input_image', 'reasoning', 'thinking', 'unavailable'):
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
        elif kind in ('thinking', 'reasoning', 'redactedThinking'):
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
            if role in {'bashExecution', 'pythonExecution'}:
                command_key = 'command' if role == 'bashExecution' else 'code'
                result[command_key] = _text(message.get(command_key))
                result['output'] = _text(message.get('output'))
                gap = bool(message.get('truncated')) or bool(message.get('images'))
                for key in ('cancelled', 'truncated', 'excludeFromContext'):
                    if key in message:
                        if type(message[key]) is not bool:
                            raise ContractError('Invalid native execution flag')
                        result[key] = message[key]
                code = message.get('exitCode')
                if code is not None and type(code) is not int:
                    raise ContractError('Invalid native exit code')
                result['exitCode'] = code
            elif role == 'fileMention':
                files = message.get('files')
                if not isinstance(files, list) or len(files) > 1024:
                    raise ContractError('Unavailable native file mention')
                result['files'] = []
                gap = False
                for item in files:
                    if not isinstance(item, dict) or item.get('skippedReason') or item.get('image'):
                        gap = True
                        continue
                    result['files'].append({'path': _text(item.get('path')), 'content': _text(item.get('content'))})
            else:
                content, gap = _blocks(message.get('content'))
                result['content'] = content
                if role == 'toolResult' and 'details' in message:
                    result['details'] = message['details']
                if role in {'custom', 'hookMessage'}:
                    result['customType'] = _text(message.get('customType'))
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
    except UnsafeSource:
        return {'gap': 'unsafe-representation'}
    except (ContractError, TypeError, ValueError, UnicodeError, RecursionError):
        return {'gap': 'missing-payload'}


def _valid_instruction_context(snapshot):
    """Validate a historical metadata-only manual assertion, if present."""
    if not isinstance(snapshot, dict):
        return False
    if 'instruction_context' not in snapshot:
        return True
    try:
        required = {'carrier', 'native_entry_id', 'native_parent_id', 'role',
                    'toolCallId', 'toolName', 'isError', 'content', 'instruction_context'}
        if not required <= snapshot.keys() or set(snapshot) - required - {'attribution'}:
            return False
        context = snapshot['instruction_context']
        fields = {'schema', 'kind', 'installation_id', 'observed_receipt_sha256',
                  'artifact_digest', 'manual_file_sha256', 'manual_relative_path',
                  'renderer', 'invocation_native_entry_id', 'tool_call_id'}
        if not isinstance(context, dict) or set(context) != fields:
            return False
        if (snapshot['carrier'] != 'message' or snapshot['role'] != 'toolResult'
                or snapshot['toolName'] != 'read' or snapshot['isError'] is not False
                or snapshot['content'] != [] or type(snapshot['content']) is not list
                or context['schema'] != 'agent-memory/owned-instruction-context-v1'
                or context['kind'] != 'installed-am-manual'
                or context['manual_relative_path'] != 'SKILL.md'
                or context['renderer'] != 'omp-18.16-full-skill-v1'
                or context['tool_call_id'] != snapshot['toolCallId']):
            return False
        for value in (snapshot['native_entry_id'], snapshot['toolCallId'],
                      context['installation_id'], context['invocation_native_entry_id']):
            if not isinstance(value, str) or not value:
                return False
        if snapshot['native_parent_id'] is not None and not isinstance(snapshot['native_parent_id'], str):
            return False
        if 'attribution' in snapshot and not isinstance(snapshot['attribution'], str):
            return False
        for key in ('observed_receipt_sha256', 'artifact_digest', 'manual_file_sha256'):
            if not isinstance(context[key], str) or not re.fullmatch(r'[0-9a-f]{64}', context[key]):
                return False
        _safe_tree(snapshot)
        raw = json.dumps(snapshot, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False)
        return (len(raw.encode('utf-8')) <= MAX_SOURCE
                and not any(re.search(pattern, raw) for _, pattern in (*security.SECRETS, *security.SUSPICIOUS)))
    except (ContractError, TypeError, ValueError, UnicodeError, RecursionError):
        return False


def _project_owned_manual(entry, invocation, witness):
    """Project only an exact native full read backed by an installation witness."""
    from . import omp_installation

    try:
        if type(witness) is not omp_installation._OwnedManual:
            return None
        verified = witness._verify()
        if verified is None:
            return None
        text, path, installation_id, receipt_hash, artifact_hash, manual_hash = verified
        if (not isinstance(entry, dict) or set(entry) != {'type', 'id', 'parentId', 'message'}
                or entry['type'] != 'message' or not isinstance(entry['message'], dict)):
            return None
        message = entry['message']
        required = {'role', 'toolCallId', 'toolName', 'isError', 'content', 'details'}
        if (not required <= message.keys() or set(message) - required - {'attribution'}
                or message['role'] != 'toolResult' or message['toolName'] != 'read'
                or message['isError'] is not False):
            return None
        call_id = _text(message['toolCallId'])
        if (not isinstance(invocation, dict) or invocation.get('type') != 'message'
                or not isinstance(invocation.get('message'), dict)
                or invocation['message'].get('role') != 'assistant'
                or not isinstance(invocation['message'].get('content'), list)):
            return None
        calls = [block for block in invocation['message']['content']
                 if isinstance(block, dict) and block.get('type') == 'toolCall' and block.get('id') == call_id]
        if len(calls) != 1 or set(calls[0]) != {'type', 'id', 'name', 'arguments'} or calls[0]['name'] != 'read':
            return None
        arguments = calls[0]['arguments']
        if (not isinstance(arguments, dict) or set(arguments) not in ({'path'}, {'i', 'path'})
                or arguments['path'] != 'skill://agent-memory'):
            return None
        if 'i' in arguments:
            _text(arguments['i'])
            _safe_tree(arguments['i'])
            raw_i = json.dumps(arguments['i'], ensure_ascii=False, allow_nan=False)
            if len(raw_i.encode('utf-8')) > MAX_SOURCE or any(
                    re.search(pattern, raw_i) for _, pattern in (*security.SECRETS, *security.SUSPICIOUS)):
                return None
        body = text.rstrip('\n')
        lines = len(body.split('\n'))
        if (message['content'] != [{'type': 'text', 'text': '[Skill file: ' + path + ']\n' + body}]
                or message['details'] != {
                    'contentType': 'text/markdown', 'resolvedPath': path, 'rowPrefixes': False,
                    'totalLines': lines, 'displayContent': {'lineNumbers': list(range(1, lines + 1)),
                                                            'startLine': 1, 'text': body},
                    'meta': {'source': {'type': 'internal', 'value': 'skill://agent-memory'}}}):
            return None
        # Python equality otherwise treats booleans and integers as interchangeable.
        details = message['details']
        if (type(details['rowPrefixes']) is not bool or type(details['totalLines']) is not int
                or type(details['displayContent']['startLine']) is not int
                or any(type(n) is not int for n in details['displayContent']['lineNumbers'])):
            return None
        snapshot = {'carrier': 'message', 'native_entry_id': _text(entry['id']),
                    'native_parent_id': entry['parentId'], 'role': 'toolResult',
                    'toolCallId': call_id, 'toolName': 'read', 'isError': False, 'content': [],
                    'instruction_context': {
                        'schema': 'agent-memory/owned-instruction-context-v1',
                        'kind': 'installed-am-manual', 'installation_id': installation_id,
                        'observed_receipt_sha256': receipt_hash, 'artifact_digest': artifact_hash,
                        'manual_file_sha256': manual_hash, 'manual_relative_path': 'SKILL.md',
                        'renderer': 'omp-18.16-full-skill-v1',
                        'invocation_native_entry_id': _text(invocation.get('id')), 'tool_call_id': call_id}}
        if 'attribution' in message:
            snapshot['attribution'] = _text(message['attribution'])
        if not _valid_instruction_context(snapshot):
            return None
        return {'snapshot': snapshot, 'exclusions': list(EXCLUSIONS)}
    except (ContractError, TypeError, ValueError, UnicodeError, RecursionError, KeyError):
        return None
