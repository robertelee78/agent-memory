"""One fail-closed union of bank-local private runtime requirements."""
from __future__ import annotations

from contextlib import ExitStack, contextmanager
from contextvars import ContextVar
from pathlib import Path
import os

from . import artifact, layout_recovery_state as state, project_identity, repository_config
from .developer_identity import digest, encoded
from .frontmatter import ContractError
from .identity_git import git

TOKENS = {'layout-frontier-v1', 'omp-harness-v1'}


_BARRIERS = ContextVar('private-state-lifecycle-barriers', default=None)
def compatibility(target):
    if target is None:
        return set()
    value = target.get('compatibility', target)
    tokens = value.get('private_state_capabilities', [])
    if not artifact.strings(tokens, allow_empty=True) or not set(tokens).issubset(TOKENS):
        raise ContractError('Target runtime has an unknown private-state compatibility declaration')
    return set(tokens)


def repository_inventory(installation=None):
    """Discovery is a union, not an intersection with currently available banks."""
    roots, unknown = set(), []
    registry, registry_sha = project_identity.observe()
    for entry in registry['checkouts'].values():
        for location in entry['locations']:
            path = Path(location)
            if not path.is_dir():
                unknown.append({'source': 'project-registry', 'location': location})
                continue
            listed = git(path, 'worktree', 'list', '--porcelain', optional=True)
            if listed is None:
                if (path / '.git').exists() or path.name == '.git':
                    unknown.append({'source': 'project-registry', 'location': location})
                else:
                    roots.add(path)
                continue
            for line in listed.splitlines():
                if line.startswith('worktree '):
                    roots.add(Path(line[9:]))
    bindings = state.bindings()
    for row in bindings['bindings'].values():
        if row['lifecycle_state'] in {'active', 'interrupted'}:
            roots.add(Path(row['repo_root']))
    if installation:
        for key in ('rollback_repositories', 'observed_repositories'):
            values = installation.get(key, [])
            if not isinstance(values, list) or any(not isinstance(value, str) for value in values):
                raise ContractError('Saved runtime bank inventory is malformed')
            roots.update(Path(value) for value in values)
        for row in installation.get('banks', []):
            roots.add(Path(row['repository']))
    try:
        from . import omp_boundary
    except ImportError:
        omp_boundary = None
    producer = getattr(omp_boundary, 'source_capability_repositories', None)
    if producer is not None:
        roots.update(Path(value) for value in producer(installation))
    elif installation and ('omp' in installation.get('presets', [])
            or any(row.get('harness') == 'omp' for row in installation.get('source_bindings', []))):
        unknown.append({'source': 'omp-bindings', 'reason': 'complete enrolled-bank discovery is unavailable'})
    if len(roots) > 1024:
        raise ContractError('Private-state bank inventory exceeds its complete finite bound')
    pins = {'project_registry_sha256': registry_sha, 'layout_bindings_sha256': digest(encoded(bindings))}
    return roots, unknown, pins


def required_private_state_capabilities(installation, target_artifact, operation):
    """Observe every producer under source barriers; never infer missing = capable.

    Lifecycle callers already hold their runtime lock. This function takes only
    the suffix source barrier and never invokes a lifecycle helper underneath it.
    The returned pins must be re-observed at each activation/readback boundary.
    """
    supported = compatibility(target_artifact)
    roots, unknown, inventory_pins = repository_inventory(installation)
    requirements, pins = {}, []
    target_compatibility = target_artifact.get('compatibility', target_artifact) if target_artifact else {}
    receipt_schema = installation.get('schema') if installation else None
    readable_receipts = target_compatibility.get('installation_receipt_schemas', ['agent-memory/install-receipt-v1'])
    if receipt_schema == 'agent-memory/install-receipt-v2' and receipt_schema not in readable_receipts:
        unknown.append({'source': 'installation-receipt', 'reason': 'target cannot read retained v2 ownership receipt'})
    from . import recovery_frontier, bank_writes
    try:
        from . import omp_boundary
    except ImportError:
        omp_boundary = None
    with ExitStack() as stack:
        # Stable order avoids same-family cross-bank inversion.
        for root in sorted(roots, key=str):
            if not root.is_absolute() or root.is_symlink() or not root.is_dir():
                unknown.append({'source': 'bank-inventory', 'location': str(root)})
                continue
            try:
                settings = repository_config.settings(root)
                bank = root / settings['bank_root']
                if not bank.exists():
                    bound = any(row['repo_root'] == str(root) and row['lifecycle_state'] in {'active', 'interrupted'}
                        for row in state.bindings()['bindings'].values())
                    if bound:
                        raise ContractError('Bound recovery bank is unavailable')
                    continue
                capabilities = set()
                operations, _ = bank_writes.state_paths(root, bank, '00000000-0000-4000-8000-000000000000')
                source_marker = operations.parent / 'turn-sources-compat-v1.json'
                source_tree = operations.parent / 'omp-source-v1'
                held = _BARRIERS.get()
                if held is not None and str(root) not in held:
                    raise ContractError('A bank joined the private-state inventory after lifecycle quiescence')
                if omp_boundary is not None and hasattr(omp_boundary, 'source_barrier'):
                    if held is None:
                        stack.enter_context(omp_boundary.source_barrier(root, exclusive=True))
                    producer = getattr(omp_boundary, 'required_source_capabilities', None)
                    if producer is None:
                        if os.path.lexists(source_marker) or os.path.lexists(source_tree):
                            raise ContractError('Private source requirement cannot be completely observed')
                    else:
                        capabilities.update(producer(root))
                elif os.path.lexists(source_marker) or os.path.lexists(source_tree):
                    raise ContractError('Private native source state requires its capable source observer')
                token = recovery_frontier.observe(root)
                indexed = state.index(root)
                layout_required = token['hold_sha256'] is not None or any(
                    row['state'] in {'active', 'interrupted'} for row in indexed['operations'].values())
                if layout_required:
                    recovery_frontier.verified_hold(root)
                    capabilities.add('layout-frontier-v1')
                    target_digest = target_artifact.get('artifact_digest') if target_artifact else None
                    if target_digest is not None:
                        from .recovery_quiescence import selected_attestation
                        frontier_operation = next(operation_id for operation_id, indexed_row in indexed['operations'].items()
                            if indexed_row['state'] in {'active', 'interrupted'})
                        directory, _, ledger_value = recovery_frontier.ledger(root, frontier_operation, indexed['operations'][frontier_operation])
                        _, proposal = recovery_frontier.evidence(directory, ledger_value, indexed['operations'][frontier_operation])
                        if selected_attestation(root, frontier_operation, proposal, target_digest) is None:
                            raise ContractError('Target capable runtime has no renewed bank-bound quiescence attestation')
                if not capabilities.issubset(TOKENS):
                    raise ContractError('A private-state producer returned an unknown capability')
                requirements[str(bank)] = sorted(capabilities)
                pins.append({'repo_root': str(root), 'bank_path': str(bank), 'recovery_frontier': token,
                    'requirements': sorted(capabilities)})
            except (ContractError, OSError, ValueError, TypeError, KeyError) as exc:
                unknown.append({'source': 'private-state', 'location': str(root),
                    'reason': 'private state is missing, inaccessible, interrupted, malformed or changed'})
    required = set().union(*(set(values) for values in requirements.values())) if requirements else set()
    missing = required - supported
    if operation == 'uninstall' and required:
        # An explicit equivalent already-active runtime must be passed as target.
        equivalent = isinstance(target_artifact, dict) and target_artifact.get('already_active_verified') is True
        if not equivalent:
            missing.update(required)
    return {'requirements': requirements, 'required_capabilities': sorted(required),
        'target_capabilities': sorted(supported), 'missing_capabilities': sorted(missing),
        'unknown_state': unknown, 'evidence_pins': {'inventory': inventory_pins, 'banks': pins},
        'decision': 'compatible' if not unknown and not missing else 'pending',
        'operation': operation, 'private_evidence_preserved': True}


def require_compatible(installation, target_artifact, operation, expected=None):
    current = required_private_state_capabilities(installation, target_artifact, operation)
    if current['decision'] != 'compatible':
        raise ContractError('Private-state runtime compatibility is pending; preserve the capable active runtime and evidence')
    if expected is not None and current != expected:
        raise ContractError('Private-state inventory changed before runtime activation; quiesce and inspect again')
    return current


@contextmanager
def activation_guard(installation, target_artifact, operation, transition=None):
    """Retain the common source barriers through activation and exact rollback."""
    existing = _BARRIERS.get()
    if existing is not None:
        if transition is not None:
            transition()
        proof = require_compatible(installation, target_artifact, operation)
        yield proof
        require_compatible(installation, target_artifact, operation, expected=proof)
        return
    roots, unknown, _ = repository_inventory(installation)
    if unknown:
        raise ContractError('Private-state bank inventory is unavailable; preserve the active runtime')
    try:
        from . import omp_boundary
    except ImportError:
        omp_boundary = None
    with ExitStack() as stack:
        for root in sorted(roots, key=str):
            if omp_boundary is not None and hasattr(omp_boundary, 'source_barrier'):
                stack.enter_context(omp_boundary.source_barrier(root, exclusive=True))
        bound = _BARRIERS.set({str(root) for root in roots})
        try:
            if transition is not None:
                transition()
            proof = require_compatible(installation, target_artifact, operation)
            yield proof
            require_compatible(installation, target_artifact, operation, expected=proof)
        finally:
            _BARRIERS.reset(bound)
