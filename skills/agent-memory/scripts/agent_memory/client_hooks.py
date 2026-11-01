"""Merge only one owned command group into a client hook configuration.

The caller owns lifecycle locking, preflight, transactional activation and
readback. A valid merge is neither a trusted client hook nor proof of invocation.
"""
from __future__ import annotations

import copy
import json

from .frontmatter import ContractError, unique_object

EVENT = 'UserPromptSubmit'
MAX_CONFIG = 1024 * 1024


class HookError(ContractError):
    """Fixed hook verification classification; never retain configuration text."""
    def __init__(self, code):
        self.code = code
        super().__init__(code)


def parse(raw):
    if not isinstance(raw, bytes) or len(raw) > MAX_CONFIG:
        raise HookError('config_unavailable')
    try:
        value = json.loads(raw.decode('utf-8'), object_pairs_hook=unique_object) if raw else {}
    except ContractError:
        raise HookError('duplicate_json_key') from None
    except (UnicodeError, ValueError):
        raise HookError('malformed_json') from None
    if not isinstance(value, dict):
        raise HookError('unsupported_shape')
    hooks = value.get('hooks', {})
    if (not isinstance(hooks, dict) or not isinstance(hooks.get(EVENT, []), list)
            or not isinstance(hooks.get('Stop', []), list)):
        raise HookError('unsupported_shape')
    return value


def encode(value):
    raw = (json.dumps(value, indent=2, ensure_ascii=False) + '\n').encode('utf-8')
    if len(raw) > MAX_CONFIG:
        raise ContractError('Updated client hook configuration exceeds its bound')
    return raw


def entry(command):
    if not isinstance(command, str) or not command.strip() or len(command.encode('utf-8')) > 4096:
        raise ContractError('Invalid owned hook command')
    return {'hooks': [{'type': 'command', 'command': command}]}


def equal(left, right):
    """Compare JSON without Python's boolean/number coercion."""
    return (json.dumps(left, sort_keys=True, ensure_ascii=False)
            == json.dumps(right, sort_keys=True, ensure_ascii=False))


def locate(document, owned, event=EVENT):
    if (not isinstance(owned, dict) or set(owned) != {'hooks'}
            or not isinstance(owned['hooks'], list) or not owned['hooks']
            or any(not isinstance(item, dict) or item.get('type') != 'command'
                   or not isinstance(item.get('command'), str) or not item['command'].strip()
                   or len(item['command'].encode('utf-8')) > 4096 for item in owned['hooks'])):
        raise HookError('insufficient_receipt')
    groups = document.get('hooks', {}).get(event, [])
    if not isinstance(groups, list):
        raise HookError('unsupported_shape')
    expected = json.dumps(owned, sort_keys=True, ensure_ascii=False)
    matches = [i for i, item in enumerate(groups)
               if json.dumps(item, sort_keys=True, ensure_ascii=False) == expected]
    if not matches:
        raise HookError('owned_group_missing_or_changed')
    if len(matches) != 1:
        raise HookError('owned_group_duplicate')
    return matches[0]


def merge(raw, owned, desired, event=EVENT):
    document = parse(raw)
    groups = document.setdefault('hooks', {}).setdefault(event, [])
    if not isinstance(groups, list):
        raise ContractError('Existing client hook configuration has an unsupported shape')
    if owned is None:
        if desired in groups:
            raise ContractError('Identical unowned hook already exists; preserve client configuration')
        groups.append(copy.deepcopy(desired))
    else:
        position = locate(document, owned, event)
        if desired != owned and desired in groups:
            raise ContractError('Desired hook collides with an existing client hook')
        groups[position] = copy.deepcopy(desired)
    return encode(document)


def remove(raw, owned, originally_absent=False, original_shape=None, event=EVENT):
    document = parse(raw)
    position = locate(document, owned, event)
    groups = document['hooks'][event]
    del groups[position]
    original_shape = original_shape or {}
    if not groups and not original_shape.get('event_present'):
        del document['hooks'][event]
    if not document['hooks'] and not original_shape.get('hooks_present'):
        del document['hooks']
    if originally_absent and not document:
        return None
    return encode(document)
