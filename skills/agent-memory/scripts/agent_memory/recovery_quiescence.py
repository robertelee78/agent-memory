"""Retained independently reviewed same-bank invocation/runtime renewals."""
from __future__ import annotations
import uuid
from pathlib import Path

from . import artifact, layout_recovery_state as state, local_registry as storage, recovery_frontier as frontier
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError

ATTESTATION_STATEMENT = 'All authorized legacy invocation routes and in-flight sessions are disabled or quiesced until terminal recovery retirement, or use the independently verified capable runtime and bound private state.'


def attestation_value(value, manifest, runtime):
    names = {'schema', 'attestation_id', 'operation_id', 'project_id', 'repo_root', 'bank_path', 'checkout_key',
        'state_root', 'manifest_sha256', 'runtime_artifact_digest', 'operator_developer_id', 'issued_at',
        'declared_principals', 'declared_invocation_routes', 'stopped_sessions', 'statement', 'review_id'}
    if (not isinstance(value, dict) or set(value) != names
            or value['schema'] != 'agent-memory/layout-quiescence-attestation-v1'
            or not uuid_valid(value['attestation_id']) or not uuid_valid(value['review_id'])
            or value['operation_id'] != manifest['operation_id'] or value['project_id'] != manifest['project_id']
            or value['repo_root'] != manifest['repo_root'] or value['bank_path'] != manifest['bank_path']
            or value['checkout_key'] != manifest['identity']['scope_context']['checkout_key']
            or value['manifest_sha256'] != digest(encoded(manifest))
            or value['runtime_artifact_digest'] != runtime['artifact_digest']
            or value['operator_developer_id'] != manifest['identity']['scope_context']['developer_id']
            or value['statement'] != ATTESTATION_STATEMENT
            or 'layout-frontier-v1' not in runtime['compatibility'].get('private_state_capabilities', [])):
        raise ContractError('pending-runtime-compatibility')
    from . import bank_writes
    operations, _ = bank_writes.state_paths(Path(manifest['repo_root']), Path(manifest['bank_path']), manifest['operation_id'])
    if value['state_root'] != str(operations.parents[2]):
        raise ContractError('pending-runtime-compatibility')
    for name in ('declared_principals', 'declared_invocation_routes', 'stopped_sessions'):
        values = value[name]
        if (not isinstance(values, list) or (name != 'stopped_sessions' and not values)
                or len(values) > 1024 or len(set(values)) != len(values)
                or any(not isinstance(item, str) or not item.strip() or len(item.encode()) > 4096 for item in values)):
            raise ContractError('pending-runtime-compatibility')
    storage.time_value(value['issued_at'])
    return value

def selected_attestation(repo, operation_id, proposal, runtime_digest):
    directory = state.paths(repo, operation_id)[3]
    return _selected_attestation(directory, proposal, runtime_digest)


def _selected_attestation(directory, proposal, runtime_digest):
    heads = state.read(directory / 'quiescence-attestations.json', missing=True) or {}
    row = heads.get(runtime_digest)
    if row is None:
        attestation = proposal.get('quiescence_attestation')
        if (attestation is None or attestation.get('runtime_artifact_digest') != runtime_digest
                or digest(encoded(attestation)) != proposal.get('quiescence_attestation_sha256')):
            return None
        return attestation
    attestation = state.read(directory / 'attestations' / (row['attestation_id'] + '.json'))
    review = state.read(directory / 'quiescence-reviews' / (row['review_id'] + '.json'))
    if (digest(encoded(attestation)) != row['sha256'] or digest(encoded(review)) != row['review_sha256']
            or review['attestation_sha256'] != row['sha256'] or review['decision'] != 'approve'
            or attestation['runtime_artifact_digest'] != runtime_digest):
        raise ContractError('Renewed runtime quiescence attestation is missing, corrupt or mismatched')
    return attestation


def validate(repo, value, *, runtime_artifact_digest, required_routes=(), publish=False, owner=None):
    """A caller supplies a measured target digest, never a boolean bypass."""
    from . import lifecycle, comparison
    root, bank, _, _ = state.paths(repo)
    if not isinstance(value, dict) or set(value) != {'attestation', 'review'}:
        raise ContractError('New invocation routes require the exact private attestation and independent review envelope')
    attestation, review = value['attestation'], value['review']
    operation_id = attestation.get('operation_id')
    row = state.index(root)['operations'].get(operation_id)
    if row is None or row['state'] != 'active':
        raise ContractError('Quiescence renewal must bind the actual active recovery operation')
    directory, _, current = frontier.ledger(root, operation_id, row)
    manifest, proposal = frontier.evidence(directory, current, row)
    runtime = artifact.inspect(artifact.PACK)
    runtime['artifact_digest'] = runtime_artifact_digest
    attestation_value(attestation, manifest, runtime)
    if not set(required_routes).issubset(attestation['declared_invocation_routes']):
        raise ContractError('Renewed quiescence omits a newly enabled same-bank invocation route')
    fields = {'schema', 'review_id', 'operation_id', 'attestation_sha256', 'reviewer_developer_id',
        'reviewer_harness', 'reviewer_session', 'review_time', 'decision', 'rationale'}
    if (not isinstance(review, dict) or set(review) != fields
            or review['schema'] != 'agent-memory/layout-quiescence-review-v1'
            or review['review_id'] != attestation['review_id'] or not uuid_valid(review['review_id'])
            or not uuid_valid(review['reviewer_developer_id']) or review['operation_id'] != operation_id
            or review['attestation_sha256'] != digest(encoded(attestation)) or review['decision'] != 'approve'):
        raise ContractError('Quiescence renewal needs retained independent review of the exact complete declaration')
    author = proposal.get('author', {})
    if (review['reviewer_harness'], review['reviewer_session']) == (author.get('harness'), author.get('session')):
        raise ContractError('Quiescence review must come from a different actual agent/session')
    lifecycle.time_value(review['review_time'])
    comparison.safe_text(review['rationale'], 4096)
    comparison.safe_text(review['reviewer_harness'], 256)
    comparison.safe_text(review['reviewer_session'], 256)
    from . import bank_observation, scope_binding
    live = frontier.observe(root, owner)
    marker = frontier.marker_bytes(bank)
    marker_value = frontier.marker_value(marker) if marker is not None else None
    if marker_value is None or marker_value['schema'] != frontier.HOLD or marker_value['operation_id'] != operation_id:
        raise ContractError('Quiescence renewal requires the exact persistent hold, not an interrupted transaction')
    bank_observation.markers(bank, owner, marker)
    scope_binding.require(root, manifest['project_id'], expected=manifest['identity']['scope_context'],
        owned_lock=owner, owned_transaction=marker)
    executing = artifact.inspect(artifact.PACK)
    if (selected_attestation(root, operation_id, proposal, executing['artifact_digest']) is None
            and executing['artifact_digest'] != runtime_artifact_digest):
        raise ContractError('The executing verifier is neither the reviewed existing runtime nor the reviewed replacement')
    frontier.check(live, root, owner)
    if publish:
        if owner is None:
            with storage.exclusive(bank / '.agent-memory.lock', str(uuid.uuid4()), 'bank-recover') as owned:
                return validate(root, value, runtime_artifact_digest=runtime_artifact_digest,
                    required_routes=required_routes, publish=True, owner=owned)
        state.write_once(directory / 'attestations' / (attestation['attestation_id'] + '.json'), attestation)
        state.write_once(directory / 'quiescence-reviews' / (review['review_id'] + '.json'), review)
        heads_path = directory / 'quiescence-attestations.json'
        heads = state.read(heads_path, missing=True) or {}
        old = digest(state.read_bytes(heads_path, missing=True))
        next_head = {'attestation_id': attestation['attestation_id'],
            'sha256': digest(encoded(attestation)), 'review_id': review['review_id'], 'review_sha256': digest(encoded(review))}
        if heads.get(runtime_artifact_digest) != next_head:
            heads[runtime_artifact_digest] = next_head
            state.replace(heads_path, heads, old)
            frontier.invalidate(root)
    return {'operation_id': operation_id, 'runtime_artifact_digest': runtime_artifact_digest,
        'attestation_sha256': digest(encoded(attestation)), 'invocation_routes': attestation['declared_invocation_routes']}
