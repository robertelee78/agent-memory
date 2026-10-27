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


def parse(raw):
    if not isinstance(raw, bytes) or len(raw) > MAX_CONFIG:
        raise ContractError('Client hook configuration is unavailable or too large')
    try:
        value = json.loads(raw.decode('utf-8'), object_pairs_hook=unique_object) if raw else {}
    except (UnicodeError, ValueError) as exc:
        raise ContractError('Client hook configuration must be a unique-key JSON object') from exc
    if not isinstance(value, dict):
        raise ContractError('Client hook configuration must be a JSON object')
    hooks = value.get('hooks', {})
    if not isinstance(hooks, dict) or not isinstance(hooks.get(EVENT, []), list):
        raise ContractError('Existing client hook configuration has an unsupported shape')
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


def locate(document, owned):
    if not isinstance(owned, dict) or set(owned) != {'hooks'}:
        raise ContractError('Invalid owned hook receipt')
    groups = document.get('hooks', {}).get(EVENT, [])
    matches = [i for i, item in enumerate(groups) if item == owned]
    if len(matches) != 1:
        raise ContractError('Owned hook is missing, duplicated or changed; preserve client configuration')
    return matches[0]


def merge(raw, owned, desired):
    document = parse(raw)
    groups = document.setdefault('hooks', {}).setdefault(EVENT, [])
    if owned is None:
        if desired in groups:
            raise ContractError('Identical unowned hook already exists; preserve client configuration')
        groups.append(copy.deepcopy(desired))
    else:
        position = locate(document, owned)
        if desired != owned and desired in groups:
            raise ContractError('Desired hook collides with an existing client hook')
        groups[position] = copy.deepcopy(desired)
    return encode(document)


def remove(raw, owned, originally_absent=False, original_shape=None):
    document = parse(raw)
    position = locate(document, owned)
    groups = document['hooks'][EVENT]
    del groups[position]
    original_shape = original_shape or {}
    if not groups and not original_shape.get('event_present'):
        del document['hooks'][EVENT]
    if not document['hooks'] and not original_shape.get('hooks_present'):
        del document['hooks']
    if originally_absent and not document:
        return None
    return encode(document)
