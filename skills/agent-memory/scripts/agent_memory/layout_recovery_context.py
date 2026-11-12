"""Internal recovery authorization: exact private proofs plus live read-set pins."""
from __future__ import annotations
import json
from pathlib import Path
from . import artifact, bank_observation, layout_recovery_state as state, recovery_frontier as frontier
from . import recovery_quiescence
from .developer_identity import digest, encoded
from .frontmatter import ContractError

_seal = object()


def runtime_guard(repo, manifest, proposal):
    measured = artifact.inspect(artifact.PACK)
    if 'layout-frontier-v1' not in measured['compatibility'].get('private_state_capabilities', []):
        raise ContractError('pending-runtime-compatibility')
    if proposal.get('pending_units') or proposal.get('inherited_unit_ids'):
        attestation = recovery_quiescence.selected_attestation(repo, proposal['operation_id'], proposal, measured['artifact_digest'])
        if attestation is None:
            raise ContractError('pending-runtime-compatibility')
        recovery_quiescence.attestation_value(attestation, manifest, measured)
    return measured['artifact_digest']


def _pin_json(pins, operation_id, relative, value):
    pins.append({'operation_id': operation_id, 'path': relative, 'sha256': digest(encoded(value))})


def evidence(repo, manifest, proposal, owner, transaction):
    from . import layout_recovery as recovery
    marker = frontier.marker_value(transaction)
    if (marker['schema'] != frontier.TRANSACTION or marker['operation_id'] != proposal['operation_id']
            or proposal['operation_id'] != manifest['operation_id'] or marker['project_id'] != manifest['project_id']):
        raise ContractError('Recovery observer requires the exact internal typed transaction')
    root, _, _, directory = state.paths(repo, proposal['operation_id'])
    journal = state.read(directory / 'journal.json')
    if (journal.get('schema') != recovery.JOURNAL or journal.get('operation_id') != proposal['operation_id']
            or journal.get('proposal_sha256') != digest(encoded(proposal))
            or journal.get('manifest_sha256') != digest(encoded(manifest))
            or state.read(directory / 'proposals' / (proposal['proposal_id'] + '.json')) != proposal):
        raise ContractError('Recovery private journal/proposal/manifest changed')
    pins = []
    _pin_json(pins, proposal['operation_id'], 'manifest.json', manifest)
    _pin_json(pins, proposal['operation_id'], 'proposals/' + proposal['proposal_id'] + '.json', proposal)
    _pin_json(pins, proposal['operation_id'], 'journal.json', journal)
    originals = [(proposal['operation_id'], manifest)]
    if proposal['schema'] == 'agent-memory/layout-child-restore-v1':
        from . import layout_restore
        layout_restore.value(root, proposal)
        _, _, _, parent_dir, parent_manifest, parent, _ = recovery.recovery_source(root, proposal['parent_operation_id'])
        receipt = recovery.receipt_value(parent_dir, parent)
        head = state.read(parent_dir / 'ledger-head.json')
        if (digest(encoded(parent)) != proposal['parent_proposal_sha256']
                or digest(encoded(receipt)) != proposal['parent_receipt_sha256']
                or digest(encoded(head)) != proposal['parent_ledger_head_sha256']):
            raise ContractError('Child restore parent evidence/dependency changed')
        _pin_json(pins, parent['operation_id'], 'manifest.json', parent_manifest)
        _pin_json(pins, parent['operation_id'], 'proposals/' + parent['proposal_id'] + '.json', parent)
        _pin_json(pins, parent['operation_id'], 'receipt.json', receipt)
        _pin_json(pins, parent['operation_id'], 'ledger-head.json', head)
        originals.append((parent['operation_id'], parent_manifest))
        actual_frontier = frontier.observe(root, owner, transaction)
        if any(actual_frontier[key] != proposal['frontier'][key]
               for key in ('ledger_heads', 'unit_ids', 'reference_map_sha256', 'frontier_digest')):
            raise ContractError('Child restore would hide a later same-bank recovery dependency')
        guard_manifest, guard_proposal = parent_manifest, parent
    else:
        recovery.proposal_value(root, proposal)
        review = state.read(directory / 'reviews' / (proposal['review_id'] + '.json'))
        unit_map = state.read(directory / 'unit-map.json')
        if digest(encoded(review)) != proposal['review_sha256'] or digest(encoded(unit_map)) != proposal['unit_map_sha256']:
            raise ContractError('Recovery independent review/unit map changed')
        _pin_json(pins, proposal['operation_id'], 'reviews/' + proposal['review_id'] + '.json', review)
        _pin_json(pins, proposal['operation_id'], 'unit-map.json', unit_map)
        head = state.read(directory / 'ledger-head.json')
        if head['generation'] != 0 or head['sha256'] != journal['initial_ledger_sha256']:
            raise ContractError('Layout recovery cannot replay over a later ledger generation')
        guard_manifest, guard_proposal = manifest, proposal
    for operation_id, original in originals:
        for source in original['sources']:
            pins.append({'operation_id': operation_id, 'path': source['blob'], 'sha256': source['sha256']})
    return pins, runtime_guard(root, guard_manifest, guard_proposal)


class RecoveryReadSet:
    """Not serializable and never accepted from a proposal or ordinary writer."""
    __slots__ = ('root', 'manifest', 'proposal', 'owner', 'transaction', 'context', 'context_bytes', '_seal')

    def __init__(self, *args, **kwargs):
        raise ContractError('Recovery authorization must come from actual locked observations')

    @classmethod
    def observe(cls, repo, manifest, proposal, owner, transaction):
        from . import layout_recovery as recovery, layout_restore
        root, bank, _, directory = state.paths(repo, proposal['operation_id'])
        bank_observation.markers(bank, owner, transaction)
        pins, runtime = evidence(root, manifest, proposal, owner, transaction)
        context = state.read(directory / 'source-context.json', missing=True)
        if context is None:
            current = recovery.view(bank)
            boundary = (layout_restore.boundary(proposal, current) if proposal['schema'] == 'agent-memory/layout-child-restore-v1'
                        else recovery.partial_boundary(proposal, manifest, current))
            if boundary != 0:
                raise ContractError('Missing original recovery read-set evidence after canonical mutation')
            context = {'schema': 'agent-memory/layout-recovery-read-set-v1', 'operation_id': proposal['operation_id'],
                       'project_id': proposal['project_id'], 'proposal_sha256': digest(encoded(proposal)),
                       'manifest_sha256': digest(encoded(manifest)), 'private_sha256': pins,
                       'runtime_artifact_digest': runtime, 'frontier': frontier.observe(root, owner, transaction)}
            state.write_once(directory / 'source-context.json', context)
        expected = {'schema', 'operation_id', 'project_id', 'proposal_sha256', 'manifest_sha256',
                    'private_sha256', 'runtime_artifact_digest', 'frontier'}
        if (not isinstance(context, dict) or set(context) != expected or context['schema'] != 'agent-memory/layout-recovery-read-set-v1'
                or context['operation_id'] != proposal['operation_id'] or context['project_id'] != proposal['project_id']
                or context['proposal_sha256'] != digest(encoded(proposal)) or context['manifest_sha256'] != digest(encoded(manifest))
                or context['private_sha256'] != pins or context['runtime_artifact_digest'] != runtime):
            raise ContractError('Recovery read-set evidence changed or belongs to another operation')
        result = object.__new__(cls)
        result.root, result.manifest, result.proposal = root, manifest, proposal
        result.owner, result.transaction = owner, transaction
        result.context, result.context_bytes, result._seal = context, encoded(context), _seal
        result.check()
        return result

    def check(self):
        from . import layout_recovery as recovery
        if type(self) is not RecoveryReadSet or self._seal is not _seal:
            raise ContractError('Only an internally constructed recovery observer can authorize replacement')
        _, bank, _, directory = state.paths(self.root, self.proposal['operation_id'])
        bank_observation.markers(bank, self.owner, self.transaction)
        if state.read_bytes(directory / 'source-context.json') != self.context_bytes:
            raise ContractError('Recovery source-context journal changed')
        pins, runtime = evidence(self.root, self.manifest, self.proposal, self.owner, self.transaction)
        if pins != self.context['private_sha256'] or runtime != self.context['runtime_artifact_digest']:
            raise ContractError('Recovery proof/runtime pins changed')
        for pin in pins:
            location = state.paths(self.root, pin['operation_id'])[3] / pin['path']
            if digest(state.read_bytes(location)) != pin['sha256']:
                raise ContractError('Recovery original/proposal/review evidence changed')
        frontier.check(self.context['frontier'], self.root, self.owner, self.transaction)
        recovery.rebind(self.root, self.manifest, self.owner, self.transaction)
        return True
