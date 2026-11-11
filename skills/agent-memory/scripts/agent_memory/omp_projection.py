"""Project native stored entries without copying display events or provider state.

The caller must supply a detached SessionManager stored entry, never a native
message event. A returned gap is not assessable source coverage.
"""
import json
import re
from pathlib import Path
from datetime import datetime

from . import security
from .frontmatter import ContractError

MAX_SOURCE = 16 * 1024 * 1024
EXCLUSIONS = ('provider-metadata', 'reasoning-and-signatures')
ROLES = {'user', 'developer', 'assistant', 'toolResult', 'bashExecution', 'pythonExecution', 'fileMention', 'custom', 'hookMessage'}


FORBIDDEN_KEYS = {'providerPayload', 'provider_payload', 'thinking', 'reasoning',
                  'thinkingSignature', 'textSignature', 'signature', 'auth',
                  'credentials', 'credential', 'config', 'contextSnapshot'}
PERSISTENCE_TRUNCATION_NOTICE = '\n\n[Session persistence truncated large content]'


def _instruction_uri(relative):
    """Return only the canonical native URI for a complete instruction file."""
    if not isinstance(relative, str):
        return None
    if relative == 'SKILL.md':
        return 'skill://agent-memory'
    if re.fullmatch(r'(?:[a-z][a-z0-9-]*/)+GUIDE\.md', relative):
        return 'skill://agent-memory/' + relative
    return None


def _instruction_relative(uri):
    """Invert canonical instruction URIs without normalizing aliases."""
    if not isinstance(uri, str):
        return None
    if uri == 'skill://agent-memory':
        return 'SKILL.md'
    prefix = 'skill://agent-memory/'
    if uri.startswith(prefix):
        relative = uri[len(prefix):]
        if _instruction_uri(relative) == uri:
            return relative
    return None


def stored_timestamp(value):
    if value is None:
        return None
    if not isinstance(value, str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?Z', value):
        raise ContractError('Invalid native stored timestamp')
    try:
        return datetime.fromisoformat(value.replace('Z', '+00:00'))
    except ValueError as exc:
        raise ContractError('Invalid native stored timestamp') from exc


def checked_view(repo, session, payload):
    """Return the validated detached stored view and its pre-guard Git root."""
    from . import turn_sources
    view = payload.get('view')
    fields = {'session', 'cwd', 'file', 'header', 'leaf_id', 'native_parents', 'source_kind', 'tail_status', 'fork_status'}
    if not isinstance(view, dict) or set(view) != fields or view['session'] != session:
        raise ContractError('Invalid checked native source view')
    header = view['header']
    if (not isinstance(header, dict) or set(header) != {'id', 'cwd', 'version', 'parent_session', 'timestamp'}
            or header['id'] != session or type(header['version']) is not int
            or header['version'] != 3 or view['source_kind'] not in {'session-manager', 'retired-file'}):
        raise ContractError('Invalid checked native source header')
    if view['tail_status'] not in {'available', 'active-unavailable'}:
        raise ContractError('Invalid checked native tail qualification')
    for cwd in (view['cwd'], header['cwd']):
        if not isinstance(cwd, str) or not Path(cwd).is_absolute():
            raise ContractError('Invalid checked native working directory')
    expected_root = Path(repo).resolve()
    from .identity_git import git
    actual_top = git(Path(view['cwd']), 'rev-parse', '--show-toplevel', optional=True)
    if actual_top is None:
        raise ContractError('Checked native source Git root is unavailable')
    if Path(actual_top).resolve() != expected_root:
        raise ContractError('Checked native source belongs to another bank scope')
    if view['file'] is not None and (not isinstance(view['file'], str) or not Path(view['file']).is_absolute()):
        raise ContractError('Invalid checked native source file')
    if view['source_kind'] == 'retired-file' and (view['file'] is None or view['tail_status'] != 'available'):
        raise ContractError('Retired native source requires its exact available file')
    if header['parent_session'] is not None:
        turn_sources.bounded(header['parent_session'], 8192)
    parents = view['native_parents']
    if not isinstance(parents, list) or len(parents) > 16384:
        raise ContractError('Checked native source inventory exceeds bound')
    identities = {}
    for entry in parents:
        if not isinstance(entry, dict) or set(entry) != {'id', 'parent_id', 'timestamp'}:
            raise ContractError('Invalid checked native parent identity')
        native = turn_sources.bounded(entry['id'])
        if native.startswith(('event:', 'tool:')):
            raise ContractError('Native identity uses a reserved gap prefix')
        parent = entry['parent_id']
        if parent is not None:
            turn_sources.bounded(parent)
        if native in identities:
            raise ContractError('Duplicate checked native identity')
        identities[native] = parent
    finished = set()
    for native in identities:
        chain = set()
        current = native
        while current is not None and current not in finished:
            if current not in identities or current in chain:
                raise ContractError('Incomplete or cyclic checked native source tree')
            chain.add(current)
            current = identities[current]
        finished.update(chain)
    leaf = view['leaf_id']
    if leaf is not None and leaf not in identities:
        raise ContractError('Checked native leaf is unavailable')
    branch = []
    current = leaf
    while current is not None:
        branch.append(current)
        current = identities[current]
    branch.reverse()
    if payload.get('branch_ids') != branch:
        raise ContractError('Checked native branch differs from its parent identities')
    entries = payload.get('entries')
    if not isinstance(entries, list) or len(entries) > 16384:
        raise ContractError('Checked native source carriers exceed bound')
    seen = set()
    for entry in entries:
        if (not isinstance(entry, dict) or entry.get('id') not in identities
                or entry.get('parentId') != identities[entry['id']] or entry['id'] in seen):
            raise ContractError('Checked native carrier identity differs')
        if entry.get('type') == 'message' and not isinstance(entry.get('message'), dict):
            raise ContractError('Checked native message carrier is unavailable')
        seen.add(entry['id'])
    if set(identities) != seen:
        raise ContractError('Checked native inventory omits stored carriers')
    fork_time = stored_timestamp(header['timestamp'])
    times = {entry['id']: stored_timestamp(entry['timestamp']) for entry in parents}
    expected_fork = ('none' if header['parent_session'] is None else
                     'verified' if fork_time is not None and all(time is not None for time in times.values()) else 'unavailable')
    if view['fork_status'] != expected_fork:
        raise ContractError('Native fork evidence qualification differs')
    inherited = sorted(native for native, time in times.items()
                       if expected_fork == 'verified' and time <= fork_time)
    if sorted(payload.get('inherited_ids', [])) != inherited:
        raise ContractError('Inherited native IDs differ from stored fork evidence')
    return view, expected_root


def _persistence_truncated(value):
    if isinstance(value, str):
        return value.endswith(PERSISTENCE_TRUNCATION_NOTICE)
    if isinstance(value, list):
        return any(_persistence_truncated(child) for child in value)
    if isinstance(value, dict):
        return any(_persistence_truncated(child) for child in value.values())
    return False


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


def project(entry, *, persisted=False):
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
        if _persistence_truncated(result):
            return {'gap': 'oversized-source'}
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
                or _instruction_uri(context['manual_relative_path']) is None
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


def _project_owned_manual(entry, invocation, witness, verified):
    """Project only an exact native full read backed by an installation witness."""
    from . import omp_installation

    try:
        if (type(witness) is not omp_installation._OwnedManual
                or verified != witness._comparison):
            return None
        text, path, installation_id, receipt_hash, artifact_hash, manual_hash = verified
        relative = witness.relative_path
        uri = _instruction_uri(relative)
        if uri is None:
            return None
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
                or arguments['path'] != uri):
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
                    'meta': {'source': {'type': 'internal', 'value': uri}}}):
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
                        'manual_file_sha256': manual_hash, 'manual_relative_path': relative,
                        'renderer': 'omp-18.16-full-skill-v1',
                        'invocation_native_entry_id': _text(invocation.get('id')), 'tool_call_id': call_id}}
        if 'attribution' in message:
            snapshot['attribution'] = _text(message['attribution'])
        if not _valid_instruction_context(snapshot):
            return None
        return {'snapshot': snapshot, 'exclusions': list(EXCLUSIONS)}
    except (ContractError, TypeError, ValueError, UnicodeError, RecursionError, KeyError):
        return None
