"""Historical receipt reads bind the existing checkout, not current bank health."""
from pathlib import Path
from . import layout_recovery_state as state, local_registry as storage, project_identity, scope_binding
from .developer_identity import digest, encoded
from .frontmatter import ContractError
from .identity_git import git


def check(repo, scope, project_id):
    root, _, _, _ = state.paths(repo)
    # Reuse the real principal and validated existing project registry. No bank
    # prose is interpreted, no project is created, and no write is authorized.
    scope_binding.principal(root)
    if storage.paths('projects')[2].exists():
        raise ContractError('Historical project registry is locked')
    registry, _ = project_identity.observe()
    common = git(root, 'rev-parse', '--git-common-dir', optional=True)
    location = (root / common).resolve() if common is not None else root
    info = location.stat(); key = f'{info.st_dev}:{info.st_ino}'
    known = registry['checkouts'].get(key)
    if (key != scope['checkout_key'] or known is None or known['project_id'] != project_id
            or str(location) not in known['locations'] or project_id not in registry['projects']):
        raise ContractError('Historical receipt belongs to a different or unregistered checkout')
    return root


def recovery_receipt(directory, preview, canonical):
    return {'schema': 'agent-memory/layout-known-recovery-receipt-v1', 'operation_id': preview['operation_id'],
            'recovery_id': preview['recovery_id'], 'recovery_sha256': digest(encoded(preview)),
            'canonical_receipt_sha256': digest(encoded(canonical)), 'outcome': canonical['outcome'],
            'verified': True, 'readback_at': storage.now()}


def completed(directory, preview):
    path = directory / 'recovery' / (preview['recovery_id'] + '.receipt.json')
    receipt = state.read(path, missing=True)
    if receipt is None:
        return None
    canonical = state.read(directory / 'receipt.json')
    if (receipt.get('schema') != 'agent-memory/layout-known-recovery-receipt-v1'
            or receipt.get('operation_id') != preview['operation_id'] or receipt.get('recovery_id') != preview['recovery_id']
            or receipt.get('recovery_sha256') != digest(encoded(preview))
            or receipt.get('canonical_receipt_sha256') != digest(encoded(canonical))
            or receipt.get('verified') is not True or canonical.get('verified') is not True):
        raise ContractError('Known recovery historical receipt is corrupt or request ID was reused')
    return canonical
