"""Recognize only journal-derived temporary names and exact reviewed payloads."""
from pathlib import Path
import json
import os
import stat
from . import layout_recovery_observation as raw, bank_observation
from .developer_identity import digest
from .frontmatter import ContractError


def source_name(operation_id, path):
    return str(Path(path).parent / ('.agent-memory-layout-' + operation_id + '-' + digest(path.encode())[:16] + '.tmp'))


def entries(bank, operation_id, actions, base_view, final_facts, marker_payloads):
    base = {row['path']: row for row in base_view}
    candidates = {}
    for action in actions:
        path = action['path']
        values = []
        for sha, facts in ((action.get('before'), base.get(path)), (action.get('after'), final_facts.get(path))):
            if sha is not None and facts is not None:
                values.append((sha, facts['uid'], facts['gid'], stat.S_IMODE(facts['mode'])))
        candidates[source_name(operation_id, path)] = values
    for payload in marker_payloads:
        identity = json.loads(payload)['operation_id']
        path = '.agent-memory-layout-marker-' + identity + '.tmp'
        inherited_gid = bank.stat().st_gid if bank.stat().st_mode & stat.S_ISGID else os.getegid()
        candidates.setdefault(path, []).append((digest(payload), os.geteuid(), inherited_gid, 0o600))
    result = []
    for path, allowed in sorted(candidates.items()):
        target = bank / path
        if not target.exists() and not target.is_symlink():
            continue
        body, facts = raw.read_source(bank, path)
        if (digest(body), facts['uid'], facts['gid'], stat.S_IMODE(facts['mode'])) not in allowed:
            raise ContractError('Unknown, partial or changed transaction temporary; preserve it for explicit inspection')
        raw.metadata_supported(target)
        result.append({'path': path, 'sha256': digest(body), 'facts': facts})
    return result


def clean(bank, pins, owner, transaction):
    for pin in pins:
        bank_observation.markers(bank, owner, transaction)
        body, facts = raw.read_source(bank, pin['path'])
        if digest(body) != pin['sha256'] or facts != pin['facts']:
            raise ContractError('Known recovery temporary changed after preview')
        descriptors = [os.open(bank, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)]
        try:
            parts = Path(pin['path']).parts
            for part in parts[:-1]:
                descriptors.append(os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptors[-1]))
            named = os.stat(parts[-1], dir_fd=descriptors[-1], follow_symlinks=False)
            if raw.signature(named) != facts:
                raise ContractError('Known temporary binding changed immediately before cleanup')
            os.unlink(parts[-1], dir_fd=descriptors[-1]); os.fsync(descriptors[-1])
        finally:
            for descriptor in reversed(descriptors):
                os.close(descriptor)
