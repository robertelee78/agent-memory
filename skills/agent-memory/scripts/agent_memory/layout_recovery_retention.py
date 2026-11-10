"""Separately reviewed, exact-file cleanup of terminal private recovery evidence.

No recursive deletion, age-based collection, pending-unit disposal, or implicit
cleanup by bank purge/runtime uninstall is provided here. Directories remain.
"""
from __future__ import annotations

from datetime import datetime, timezone
import os
from pathlib import Path, PurePosixPath
import re
import stat
import uuid

from . import bank_writes as writes, local_registry as storage, scope_binding
from . import layout_recovery_state as state, recovery_frontier as frontier
from . import layout_recovery_publication as publication
from . import layout_recovery_observation as observation, sharing_limits
from . import layout_recovery_temps as temps
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError
from . import trust_paths

SCHEMA = 'agent-memory/layout-retention-proposal-v1'
REVIEW = 'agent-memory/layout-retention-review-v1'
MAX_ENTRIES = 262144
MAX_TOTAL = 512 * 1024 * 1024
ROOT_FILES = {'manifest.json', 'manifest.sha256', 'unit-map.json', 'journal.json', 'receipt.json',
              'ledger-head.json', 'tombstone.json', 'retention-requirements.json',
              'rollback-plan.json', 'child-rollback-plan.json', 'quiescence-attestations.json', 'staging.json',
              'retirement.json', 'reference-map.json', 'source-context.json'}
UUID = r'[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}'
OWNED = re.compile(
    r'(?:originals|intended)/'+UUID+r'\.bin|'
    r'(?:proposals|reviews|attestations|resolutions|resolution-journals|resolution-completions|union-reviews|discard-reviews|quiescence-reviews)/'+UUID+r'\.json|'
    r'(?:coverage-reviews|unit-maps)/[0-9a-f]{64}\.json|'
    r'(?:ledger|progress|ledger-transitions)/[0-9]{8}\.json|'
    r'progress/(?:apply|finish|rollback)-[0-9]{8}\.json|'
    r'recovery/'+UUID+r'(?:\.receipt)?\.json|'
    r'revisions/'+UUID+r'/(?:proposal|review|receipt|journal|unit-map)\.json|'
    r'nested/'+UUID+r'/(?:journal|receipt|proposal|resolution|completion|rollback-plan)\.json|'
    r'nested/'+UUID+r'/(?:progress|rollback-progress)/[0-9]{8}\.json|'
    r'publication/'+UUID+r'/(?:observation|request|receipt)\.json|'
    r'publication/'+UUID+r'/originals/'+UUID+r'\.bin|'
    r'metadata-probe-attempts/'+UUID+r'/(?:plan|completion)\.json|'
    r'metadata-probe-attempts/'+UUID+r'/pins/\.agent-memory-probe-'+UUID+r'\.json|'
    r'retention/'+UUID+r'/(?:intent|proposal|receipt|progress/[0-9]{8})\.json\Z')
OWNED_DIRECTORY = re.compile(
    r'(?:originals|intended|proposals|reviews|attestations|resolutions|resolution-journals|resolution-completions|ledger|progress|recovery|unit-maps|revisions|nested|publication|retention|coverage-reviews|union-reviews|discard-reviews|ledger-transitions|quiescence-reviews|metadata-probe-attempts)|'
    r'(?:revisions|nested|publication|retention|metadata-probe-attempts)/'+UUID+r'|'
    r'publication/'+UUID+r'/originals|retention/'+UUID+r'/progress|nested/'+UUID+r'/(?:progress|rollback-progress)|'
    r'metadata-probe-attempts/'+UUID+r'/pins')


def require(condition, message):
    if not condition:
        raise ContractError(message)


def _signature(info):
    return {field: getattr(info, 'st_'+field) for field in
            ('dev', 'ino', 'mode', 'uid', 'gid', 'nlink', 'size', 'mtime_ns', 'ctime_ns')}


def _inventory(directory):
    """Pin every named private entry; never follow redirects or ignore unknowns."""
    entries, directories, total = {}, {}, 0

    def visit(path, relative=''):
        nonlocal total
        trust_paths.state(path)
        fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        before = os.fstat(fd)
        require(before.st_uid == os.geteuid() and stat.S_IMODE(before.st_mode) == 0o700,
                'Recovery retention requires the existing checked owner-only directory')
        directories[relative] = {'dev': before.st_dev, 'ino': before.st_ino, 'mode': before.st_mode,
                                 'uid': before.st_uid, 'gid': before.st_gid}
        try:
            names = sorted(os.listdir(fd))
            for name in names:
                require(name not in {'.', '..'} and '/' not in name and '\\' not in name
                        and not any(ord(c) < 32 or ord(c) == 127 for c in name), 'Unsafe private artifact name')
                key = relative+'/'+name if relative else name
                require(len(entries)+len(directories) < MAX_ENTRIES and len(PurePosixPath(key).parts) <= 64,
                        'Recovery retention inventory exceeds its explicit complete bound')
                info = os.stat(name, dir_fd=fd, follow_symlinks=False)
                if stat.S_ISDIR(info.st_mode):
                    require(OWNED_DIRECTORY.fullmatch(key) is not None,
                            'Unknown private backup/directory is not owned cleanup evidence')
                    visit(path/name, key)
                else:
                    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1
                            and info.st_uid == os.geteuid() and stat.S_IMODE(info.st_mode) == 0o600,
                            'Unknown linked, redirected or unsafe recovery artifact; no deletion is authorized')
                    require(key in ROOT_FILES or OWNED.fullmatch(key) is not None,
                            'Unknown private recovery artifact; dependency inventory remains pending')
                    raw = state.read_bytes(path/name)
                    if key == 'staging.json':
                        staging = load(raw)
                        publication.exact(staging, {'schema', 'operation_id', 'repo_root', 'bank_path',
                                                    'project_id', 'identity', 'observation_complete'},
                                          'agent-memory/layout-recovery-staging-v1')
                        require(staging['observation_complete'] is False
                                and uuid_valid(staging['operation_id']) and uuid_valid(staging['project_id'])
                                and isinstance(staging['identity'], dict),
                                'Unknown/inconsistent partial staging evidence must remain pending')
                    total += len(raw)
                    require(total <= MAX_TOTAL, 'Recovery cleanup inventory exceeds the private review allowance')
                    after = os.stat(name, dir_fd=fd, follow_symlinks=False)
                    require(_signature(info) == _signature(after), 'Private artifact changed during retention inventory')
                    entries[key] = {'physical': _signature(info), 'sha256': digest(raw), 'bytes': len(raw)}
            require(sorted(os.listdir(fd)) == names, 'Private directory contents changed during retention inventory')
            named = path.lstat()
            require((before.st_dev, before.st_ino) == (named.st_dev, named.st_ino),
                    'Private directory binding changed during retention inventory')
        finally:
            os.close(fd)
    visit(directory)
    return {'files': entries, 'directories': directories, 'bytes': total}


def _references(value, operation_id, directory):
    if isinstance(value, dict):
        return any((key in {'parent_operation_id', 'recovery_operation_id', 'source_operation_id', 'restore_operation_id'}
                    and item == operation_id) or _references(item, operation_id, directory)
                   for key, item in value.items())
    if isinstance(value, list):
        return any(_references(item, operation_id, directory) for item in value)
    return isinstance(value, str) and (value == str(directory) or value.startswith(str(directory)+'/'))


def _child_info(root, operation_id, base, index, inventory, *, complete=False):
    """A child restore has real parent evidence, never a fabricated ledger."""
    from . import layout_restore
    directory = base/operation_id
    candidates = [state.read(directory/name) for name in inventory['files']
                  if name.startswith('proposals/') and name.endswith('.json')]
    candidates = [p for p in candidates if p.get('schema') == layout_restore.SCHEMA]
    require(len(candidates) == 1, 'Unindexed operation is not one known typed child restore')
    proposal = layout_restore.value(root, candidates[0])
    require(proposal['operation_id'] == operation_id, 'Child restore identity was substituted')
    parent_id = proposal['parent_operation_id']
    require(parent_id in index['operations'], 'Child restore has no real indexed parent')
    parent_row = index['operations'][parent_id]
    parent_directory = base/parent_id
    parent_receipt = state.read(parent_directory/'receipt.json')
    require(parent_receipt.get('schema') == 'agent-memory/layout-recovery-receipt-v1'
            and parent_receipt.get('verified') is True and parent_receipt.get('operation_id') == parent_id
            and parent_receipt.get('outcome') == 'layout-restored'
            and digest(encoded(parent_receipt)) == proposal['parent_receipt_sha256'],
            'Child restore parent receipt is missing, changed or not a real completed repair')
    parent_proposal = state.read(parent_directory/'proposals'/(parent_receipt['proposal_id']+'.json'))
    require(digest(encoded(parent_proposal)) == proposal['parent_proposal_sha256']
            and parent_receipt['request_sha256'] == digest(encoded(parent_proposal))
            and parent_row['project_id'] == proposal['project_id'] == parent_receipt['project_id']
            and proposal['parent_index_row']['ledger_head_sha256'] == proposal['parent_ledger_head_sha256']
            and proposal['parent_index_row']['proposal_id'] == parent_proposal['proposal_id'],
            'Child restore does not bind the original parent proposal and reviewed ledger head')
    # The existing child contract only restores generation zero. Its actual
    # completed receipt binds that checked head even after later parent cleanup.
    original_ledger = layout_restore.recovery.ledger_value(parent_proposal)
    original_head = {'schema': 'agent-memory/layout-ledger-head-v1', 'operation_id': parent_id,
                     'generation': 0, 'sha256': digest(encoded(original_ledger))}
    require(digest(encoded(original_head)) == proposal['parent_ledger_head_sha256'],
            'Child restore parent head is not the original actual layout generation')
    binding = state.bindings()['bindings'].get(operation_id)
    receipt = state.read(directory/'receipt.json', missing=True)
    completed = receipt is not None
    if completed:
        require(receipt.get('schema') == 'agent-memory/layout-recovery-receipt-v1'
                and receipt.get('operation_id') == operation_id and receipt.get('proposal_id') == proposal['proposal_id']
                and receipt.get('project_id') == proposal['project_id'] and receipt.get('verified') is True
                and receipt.get('request_sha256') == digest(encoded(proposal))
                and receipt.get('parent_operation_id') == parent_id
                and receipt.get('parent_receipt_sha256') == proposal['parent_receipt_sha256']
                and (receipt.get('restore_mode'), receipt.get('outcome')) in {
                    ('finish', 'rolled-back'), ('rollback', 'layout-restored')},
                'Unknown or incomplete child restoration receipt')
        require(binding is not None and binding['lifecycle_state'] in {'terminal', 'cleaned'}
                and binding['project_id'] == proposal['project_id']
                and binding['repo_root'] == str(root) and binding['bank_path'] == index['bank_path']
                and binding['checkout_key'] == proposal['scope_context']['checkout_key'],
                'Child restore still has an interrupted private binding')
        journal = state.read(directory/'journal.json', missing=True)
        if journal is not None:
            require(journal.get('schema') == 'agent-memory/layout-recovery-journal-v1'
                    and journal.get('operation_id') == operation_id
                    and journal.get('proposal_sha256') == digest(encoded(proposal))
                    and journal.get('parent_operation_id') == parent_id
                    and journal.get('parent_receipt_sha256') == proposal['parent_receipt_sha256'],
                    'Child restore journal is not its actual parent-bound operation')
        else:
            require(binding['lifecycle_state'] == 'cleaned'
                    and state.read(directory/'tombstone.json')['operation_id'] == operation_id,
                    'Child restore journal disappeared without reviewed cleanup')
    if complete:
        require(completed and parent_row['state'] in {'terminal', 'rolled-back', 'cleaned'},
                'Unfinished child or active parent still depends on restoration evidence')
    return {'kind': 'child-restore', 'state': binding['lifecycle_state'] if binding else 'interrupted',
            'project_id': proposal['project_id'], 'parent_operation_id': parent_id,
            'completed': completed, 'receipt_sha256': digest(encoded(receipt)) if completed else None}, proposal


def _operation_evidence(repo, operation_id, *, owner=None):
    root, bank, base, directory = state.paths(repo, operation_id)
    index = state.index(root)
    if operation_id in index['operations']:
        return publication.operation_evidence(root, operation_id, owner=owner)
    inventory = _inventory(directory)
    child, proposal = _child_info(root, operation_id, base, index, inventory, complete=True)
    token = frontier.observe(root, owner=owner)
    require(token['coverage_complete'] and not token['unit_ids'], 'Pending recovery frontier prevents cleanup')
    manifest = state.read(directory/'manifest.json')
    require(manifest.get('observation_complete') is True and manifest.get('operation_id') == operation_id
            and digest(encoded(manifest)) == proposal['manifest_sha256'], 'Child original observation is incomplete or corrupt')
    for source in manifest['sources']:
        require(uuid_valid(source['source_id']) and source['blob'] == 'originals/'+source['source_id']+'.bin',
                'Child original source locator is not owned by this operation')
        body = state.read_bytes(directory/'originals'/(source['source_id']+'.bin'), publication.MAX_FILE)
        require(len(body) == source['byte_count'] and digest(body) == source['sha256'],
                'Child restore original backup is missing or corrupt')
    return root, bank, base, directory, {
        'kind': 'child-restore', 'frontier_token': token, 'project_id': proposal['project_id'],
        'receipt_sha256': child['receipt_sha256'], 'index_sha256': digest(encoded(index)),
        'parent_operation_id': proposal['parent_operation_id'],
        'parent_receipt_sha256': proposal['parent_receipt_sha256'],
        'parent_ledger_head_sha256': proposal['parent_ledger_head_sha256']}



def _probe_dependencies(bank, other, operation_id, inventory):
    attempts = sorted(key.split('/')[1] for key in inventory['directories']
                      if key.startswith('metadata-probe-attempts/') and len(key.split('/')) == 2)
    probe_dirs = [key for key in inventory['directories']
                  if key == 'metadata-probe-attempts' or key.startswith('metadata-probe-attempts/')]
    probe_files = [pin for key, pin in inventory['files'].items() if key.startswith('metadata-probe-attempts/')]
    require(len(probe_dirs) + len(probe_files) <= observation.MAX_ENTRIES
            and sum(pin['bytes'] for pin in probe_files) <= sharing_limits.MAX_PRIVATE,
            'Metadata probe evidence exceeds its complete aggregate bound')
    if not attempts:
        return []
    journal = state.read(other/'journal.json', missing=True)
    require(isinstance(journal, dict) and isinstance(journal.get('metadata_probes'), dict),
            'Missing parent journal for metadata probe evidence')
    require(journal['metadata_probes'].get('operation_id') == operation_id,
            'Metadata probe plan belongs to a different operation')
    live = temps.probe_entries(bank, journal['metadata_probes'], other)
    blockers = ([{'kind': 'live-metadata-probe', 'operation_id': operation_id}] if live else [])
    for attempt in attempts:
        prefix = 'metadata-probe-attempts/'+attempt+'/'
        require(prefix+'plan.json' in inventory['files'], 'Metadata probe attempt has no owned plan')
        plan = state.read(other/prefix/'plan.json')
        # probe_entries has already validated every complete attempt and pin
        # against this exact journal, without deleting any occupied path.
        completion = state.read(other/prefix/'completion.json', missing=True)
        expected = {'schema': 'agent-memory/layout-metadata-probe-completion-v1',
                    'operation_id': operation_id, 'attempt_id': attempt, 'plan_sha256': digest(encoded(plan))}
        if completion != expected:
            blockers.append({'kind': 'unfinished-metadata-probe-attempt',
                             'operation_id': operation_id, 'attempt_id': attempt})
    return blockers


def _dependencies(root, operation_id, base, directory, *, cleanup_id=None):
    index = state.index(root)
    # Union registry and physical discovery; a typed child need not invent an
    # index row/ledger, but no missing bound operation is considered absent.
    bindings = state.bindings()
    bound = {key: row for key, row in bindings['bindings'].items()
             if row['repo_root'] == str(root) or row['bank_path'] == index['bank_path']}
    trust_paths.state(base)
    names = sorted(os.listdir(base))
    require(all(name == 'index.json' or uuid_valid(name) for name in names),
            'Unknown recovery workspace occupant prevents a complete dependency proof')
    physical = {name for name in names if name != 'index.json'}
    require(set(index['operations']) | set(bound) <= physical,
            'Missing indexed/bound recovery operation; retain all private evidence')
    inventories = {key: _inventory(base/key) for key in sorted(physical)}
    rows = dict(index['operations'])
    for key in sorted(physical-set(rows)):
        rows[key], _ = _child_info(root, key, base, index, inventories[key])
    pins, blockers = {}, []
    for other_id in sorted(physical):
        other = base/other_id
        inventory = inventories[other_id]
        row = rows[other_id]
        blockers.extend(_probe_dependencies(Path(index['bank_path']), other, other_id, inventory))
        if row.get('kind') == 'child-restore' and not row['completed']:
            blockers.append({'kind': 'unfinished-child-restore', 'operation_id': other_id})
        for name, pin in inventory['files'].items():
            if other_id == operation_id and (name.startswith('retention/') or name == 'tombstone.json'):
                continue
            if name.endswith('.json'):
                value = state.read(other/name)
                if name == 'staging.json':
                    require(value['operation_id'] == other_id and value['project_id'] == row['project_id']
                            and value['repo_root'] == str(root) and value['bank_path'] == index['bank_path'],
                            'Partial staging evidence belongs to another operation or checkout')
                if other_id != operation_id and _references(value, operation_id, directory):
                    # A cleaned child has surrendered rollback and keeps only
                    # historical receipts; a live child still owns a dependency.
                    if row['state'] != 'cleaned' and not (row.get('kind') == 'child-restore' and row['completed']):
                        blockers.append({'kind': 'child-or-referencing-operation', 'operation_id': other_id, 'artifact': name})
                if other_id == operation_id and name.startswith('nested/') and name.endswith('/journal.json'):
                    from . import bank_transactions, recovery_transactions
                    nested_id = PurePosixPath(name).parts[1]
                    completion_name = name[:-len('journal.json')]+'completion.json'
                    completion = state.read(other/completion_name, missing=True)
                    complete = (completion is not None and completion.get('schema') == 'agent-memory/layout-nested-completion-v1'
                        and completion.get('operation_id') == other_id and completion.get('nested_id') == nested_id
                        and completion.get('verified') is True)
                    if complete and completion.get('outcome') == 'rolled-back':
                        complete = completion.keys() == {'schema', 'operation_id', 'nested_id', 'outcome', 'verified'}
                    elif complete:
                        _, canonical_path = bank_transactions.locations(root, Path(index['bank_path']), nested_id)
                        receipt = recovery_transactions.nested_receipt(value, canonical_path)
                        complete = (receipt is not None and completion.get('canonical_receipt_sha256') == digest(encoded(receipt))
                            and receipt['operation_id'] == nested_id and receipt['project_id'] == row['project_id'])
                    if not complete:
                        blockers.append({'kind': 'unfinished-nested-transaction', 'artifact': name})
                if other_id == operation_id and name.startswith('resolution-journals/'):
                    resolution_id = PurePosixPath(name).stem
                    completion = state.read(other / 'resolution-completions' / (resolution_id + '.json'), missing=True)
                    receipt = state.read(other / 'resolutions' / (resolution_id + '.json'), missing=True)
                    expected = {'schema': 'agent-memory/layout-resolution-completion-v1',
                        'resolution_id': resolution_id, 'receipt_sha256': digest(encoded(receipt)),
                        'journal_sha256': digest(encoded(value)), 'verified': True}
                    if (receipt is None or completion != expected or value.get('proposal', {}).get('operation_id') != operation_id
                            or receipt.get('request_sha256') != digest(encoded(value['proposal']))):
                        blockers.append({'kind': 'unfinished-resolution', 'artifact': name})
                if other_id == operation_id and name.startswith('publication/') and name.endswith('/request.json'):
                    receipt_name = name[:-len('request.json')]+'receipt.json'
                    receipt = state.read(other/receipt_name, missing=True)
                    if (receipt is None or receipt.get('publication_verified') is not True
                            or receipt.get('request_sha256') != digest(encoded(value))):
                        blockers.append({'kind': 'unfinished-publication', 'artifact': name})
                if other_id == operation_id and name == 'retention-requirements.json':
                    require(isinstance(value, dict) and set(value) == {'schema', 'operation_id', 'requirements'}
                            and value['schema'] == 'agent-memory/layout-retention-requirements-v1'
                            and value['operation_id'] == operation_id and isinstance(value['requirements'], list),
                            'Unknown operator retention requirements are unresolved dependencies')
                    if value['requirements']:
                        blockers.append({'kind': 'operator-retention-requirement', 'count': len(value['requirements'])})
            if other_id != operation_id:
                pins[other_id+'/'+name] = pin
    require(state.index(root) == index and state.bindings() == bindings and sorted(os.listdir(base)) == names,
            'Recovery dependency discovery changed during its complete inventory')
    return {'schema': 'agent-memory/layout-retention-dependencies-v1',
            'operations': {k: v for k, v in rows.items() if k != operation_id},
            'bindings': {k: v for k, v in bound.items() if k != operation_id},
            'artifact_pins': pins, 'blockers': blockers}


def _retained(name):
    return (name in {'receipt.json', 'journal.json', 'retention-requirements.json', 'quiescence-attestations.json', 'staging.json',
                    'retirement.json', 'reference-map.json'}
            or name.startswith(('retention/', 'resolutions/', 'resolution-journals/', 'resolution-completions/',
                                'nested/', 'proposals/', 'recovery/', 'revisions/',
                                'coverage-reviews/', 'union-reviews/', 'discard-reviews/', 'ledger-transitions/',
                                'metadata-probe-attempts/',
                                'quiescence-reviews/', 'attestations/', 'reviews/'))
            or (name.startswith('publication/') and '/originals/' not in name))


def _preview(root, operation_id, cleanup_id, *, owner=None):
    root, bank, base, directory, terminal = _operation_evidence(root, operation_id, owner=owner)
    if terminal.get('kind') != 'child-restore':
        require(state.index(root)['operations'][operation_id]['state'] in {'terminal', 'rolled-back'},
                'Cleanup cannot retire an active/interrupted recovery operation')
    context = scope_binding.require(root, terminal['project_id'], owned_lock=owner)
    inventory = _inventory(directory)
    dependencies = _dependencies(root, operation_id, base, directory, cleanup_id=cleanup_id)
    require(not dependencies['blockers'], 'Recovery rollback, resolution, publication or operator retention dependencies remain')
    delete = {name: pin for name, pin in inventory['files'].items() if not _retained(name) and name != 'tombstone.json'}
    retain = {name: pin for name, pin in inventory['files'].items() if _retained(name) and not name.startswith('retention/')}
    return {'schema': SCHEMA, 'cleanup_id': cleanup_id, 'operation_id': operation_id,
            'repo_root': str(root), 'project_id': terminal['project_id'], 'scope_context': context,
            'terminal': terminal, 'dependencies': dependencies, 'delete': delete, 'retain': retain,
            'directories': inventory['directories'], 'rollback_capability_surrendered': True,
            'review': None}


def inspect(repo, operation_id, input=None):
    require(input is None or (isinstance(input, dict) and set(input) == {'cleanup_id', 'review'}),
            'Retention inspection accepts only an exact cleanup ID and separate independent review')
    cleanup_id = str(uuid.uuid4()) if input is None else input['cleanup_id']
    require(uuid_valid(cleanup_id), 'Cleanup requires a new stable canonical UUID')
    proposal = _preview(repo, operation_id, cleanup_id)
    if input is not None:
        proposal['review'] = input['review']
        _review(proposal)
    return publication.result('retention-inspect', operation_id, 'retention-preview', proposal=proposal,
                              cleanup_id=cleanup_id, deletion_bytes=sum(p['bytes'] for p in proposal['delete'].values()),
                              rollback_capability_surrendered=True, canonical_eligibility='unavailable',
                              semantic_recovery_complete=False)


def _review(proposal):
    review = proposal['review']
    publication.exact(review, {'schema', 'review_id', 'cleanup_id', 'preview_sha256', 'reviewer',
                               'reviewed_at', 'decision', 'dependency_review_complete',
                               'rollback_loss_acknowledged', 'operator_authorization', 'rationale'}, REVIEW)
    preview = {**proposal, 'review': None}
    require(uuid_valid(review['review_id']) and review['cleanup_id'] == proposal['cleanup_id']
            and review['preview_sha256'] == digest(encoded(preview)) and review['decision'] == 'approve'
            and review['dependency_review_complete'] is True and review['rollback_loss_acknowledged'] is True,
            'Cleanup needs separate independent review of this exact dependency/file preview and rollback loss')
    reviewer = review['reviewer']
    publication.exact(reviewer, {'developer_id', 'harness', 'session', 'evidence'})
    require(uuid_valid(reviewer['developer_id']), 'Retention reviewer requires actual developer attribution')
    for field in ('harness', 'session', 'evidence'):
        publication.text(reviewer[field])
    # Developer UUID is attribution, not authentication. Independence is a real
    # separate session, bound explicitly rather than guessed from source prose.
    authorization = review['operator_authorization']
    publication.exact(authorization, {'developer_id', 'author_harness', 'author_session', 'evidence', 'statement'})
    require(authorization['developer_id'] == proposal['scope_context']['developer_id']
            and (reviewer['harness'], reviewer['session']) != (authorization['author_harness'], authorization['author_session']),
            'Cleanup author cannot self-approve the dependency review')
    for field in ('author_harness', 'author_session', 'evidence', 'statement'):
        publication.text(authorization[field])
    publication.text(review['rationale'])
    from .lifecycle import time_value
    time_value(review['reviewed_at'])


def _safe_tombstone(proposal, status):
    return {'schema': 'agent-memory/layout-retention-tombstone-v1', 'operation_id': proposal['operation_id'],
            'project_id': proposal['project_id'], 'cleanup_id': proposal['cleanup_id'], 'state': status,
            'original_bytes_available': False, 'rollback_available': False,
            'historical_receipt_retained': True, 'cleanup_complete': status == 'cleaned',
            'retained_receipts': ['receipt.json'], 'private_content_included': False}


def _remaining(directory, proposal, progress):
    inventory = _inventory(directory)
    actual = {name: pin for name, pin in inventory['files'].items()
              if not name.startswith('retention/') and name != 'tombstone.json'}
    allowed = set(proposal['delete']) | set(proposal['retain'])
    require(set(actual) <= allowed and set(proposal['retain']) <= set(actual),
            'Unknown new or missing retained artifact; interrupted cleanup remains pending')
    for name, pin in actual.items():
        require(pin == proposal['delete'].get(name, proposal['retain'].get(name)),
                'Changed private artifact is not owned by this exact cleanup preview')
    require(all(name not in actual for name in progress), 'A deleted artifact reappeared; do not delete its replacement')
    missing = set(proposal['delete'])-set(actual)
    # One missing current target is allowed for unlink-before-progress crash;
    # the exact durable intent is already present before any deletion.
    ordered = sorted(proposal['delete'])
    count = len(progress)
    require(progress == ordered[:count] and missing in (set(progress), set(ordered[:count+1])),
            'Missing artifacts are not an attainable ordered cleanup boundary')
    return inventory, missing


def _remove(directory, name, pin, directories):
    target = directory/name
    require(PurePosixPath(name).as_posix() == name and not PurePosixPath(name).is_absolute()
            and '..' not in PurePosixPath(name).parts, 'Cleanup path escaped its operation')
    parent = target.parent
    key = parent.relative_to(directory).as_posix()
    key = '' if key == '.' else key
    expected_parent = directories[key]
    trust_paths.state(parent)
    fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        opened = os.fstat(fd)
        require(all(getattr(opened, 'st_'+k) == v for k, v in expected_parent.items()),
                'Cleanup parent changed; preserve the replacement')
        raw = state.read_bytes(target)
        named = os.stat(target.name, dir_fd=fd, follow_symlinks=False)
        require(_signature(named) == pin['physical'] and len(raw) == pin['bytes'] and digest(raw) == pin['sha256'],
                'Cleanup artifact changed immediately before removal')
        current = parent.lstat()
        require((opened.st_dev, opened.st_ino) == (current.st_dev, current.st_ino), 'Cleanup parent binding changed')
        os.unlink(target.name, dir_fd=fd)
        os.fsync(fd)
    finally:
        os.close(fd)


def apply(repo, proposal):
    proposal = proposal.get('proposal', proposal) if isinstance(proposal, dict) else proposal
    fields = {'schema', 'cleanup_id', 'operation_id', 'repo_root', 'project_id', 'scope_context', 'terminal',
              'dependencies', 'delete', 'retain', 'directories', 'rollback_capability_surrendered', 'review'}
    publication.exact(proposal, fields, SCHEMA)
    require(uuid_valid(proposal['cleanup_id']) and uuid_valid(proposal['operation_id'])
            and proposal['rollback_capability_surrendered'] is True, 'Use the exact reviewed cleanup proposal')
    _review(proposal)
    operation_id, cleanup_id = proposal['operation_id'], proposal['cleanup_id']
    root, bank, base, directory = state.paths(repo, operation_id)
    require(proposal['repo_root'] == str(root), 'Cleanup proposal belongs to another checkout')
    cleanup = directory/'retention'/cleanup_id
    known = state.read(cleanup/'receipt.json', missing=True)
    if known is not None:
        require(known.get('request_sha256') == digest(encoded(proposal))
                and state.read(cleanup/'proposal.json') == proposal, 'Cleanup UUID was reused with another request')
        return publication.result('retention-apply', operation_id, 'retention-cleaned', cleanup_id=cleanup_id,
                                  receipt=known, already_applied=True, receipt_is_historical=True,
                                  original_bytes_available=False, rollback_available=False)
    from .turn_sources import canonical_writer_barrier
    with canonical_writer_barrier(root), storage.exclusive(bank/'.agent-memory.lock', cleanup_id, 'bank-recover') as owner:
        scope_binding.require(root, proposal['project_id'], expected=proposal['scope_context'], owned_lock=owner)
        intent = state.read(cleanup/'intent.json', missing=True)
        if intent is None:
            current = _preview(root, operation_id, cleanup_id, owner=owner)
            require({**proposal, 'review': None} == current, 'Retention files, terminal ledger or dependencies changed since review')
            state.write_once(cleanup/'proposal.json', proposal)
            intent = {'schema': 'agent-memory/layout-retention-intent-v1', 'cleanup_id': cleanup_id,
                      'operation_id': operation_id, 'request_sha256': digest(encoded(proposal)),
                      'historical_receipt_sha256': proposal['terminal']['receipt_sha256']}
            state.write_once(cleanup/'intent.json', intent)
            state.write_once(directory/'tombstone.json', _safe_tombstone(proposal, 'cleanup-interrupted'))
            writes.fault('layout-recovery-retention-intent')
        require(intent.get('request_sha256') == digest(encoded(proposal))
                and state.read(cleanup/'proposal.json') == proposal, 'Interrupted cleanup does not match this exact request')
        dependencies = _dependencies(root, operation_id, base, directory, cleanup_id=cleanup_id)
        require(dependencies == proposal['dependencies'], 'Cleanup dependency graph changed; preserve remaining artifacts')
        index = state.index(root)
        row = index['operations'].get(operation_id)
        require(row is None or row['state'] in {'terminal', 'rolled-back', 'cleaned'},
                'A pending operation can never be cleaned')
        if row is None:
            _child_info(root, operation_id, base, index, _inventory(directory), complete=True)
        elif row['state'] != 'cleaned':
            # Durable tombstone and historical receipt precede the state change.
            # Readers must stop expecting deleted raw evidence before unlink.
            index['operations'][operation_id]['state'] = 'cleaned'
            state.set_index(root, index, digest(state.read_bytes(base/'index.json')))
        state.bind(root, operation_id, proposal['project_id'], proposal['scope_context']['checkout_key'], 'cleaned')
        cache_state = frontier.invalidate(root)
        require(frontier.marker_bytes(bank) is None
                and frontier.marker_bytes(bank, '.agent-memory-recovery.md') is None,
                'A live recovery marker must never be removed by retention cleanup')
        progress = []
        for sequence in range(len(proposal['delete'])):
            event = state.read(cleanup/'progress'/('%08d.json' % sequence), missing=True)
            if event is None:
                break
            require(event == {'schema': 'agent-memory/layout-retention-progress-v1', 'cleanup_id': cleanup_id,
                              'sequence': sequence, 'path': sorted(proposal['delete'])[sequence]}, 'Corrupt cleanup progress')
            progress.append(event['path'])
        _, missing = _remaining(directory, proposal, progress)
        for sequence, name in enumerate(sorted(proposal['delete'])):
            if sequence < len(progress):
                continue
            require(_dependencies(root, operation_id, base, directory, cleanup_id=cleanup_id) == proposal['dependencies'],
                    'A new dependency appeared during cleanup; retain all remaining evidence')
            if name not in missing:
                _remove(directory, name, proposal['delete'][name], proposal['directories'])
            writes.fault('layout-recovery-retention-after-delete')
            writes.fault('layout-recovery-retention-after-delete-'+str(sequence))
            state.write_once(cleanup/'progress'/('%08d.json' % sequence), {
                'schema': 'agent-memory/layout-retention-progress-v1', 'cleanup_id': cleanup_id,
                'sequence': sequence, 'path': name})
            writes.fault('layout-recovery-retention-after-progress')
        _remaining(directory, proposal, sorted(proposal['delete']))
        tombstone = _safe_tombstone(proposal, 'cleaned')
        state.replace(directory/'tombstone.json', tombstone, digest(state.read_bytes(directory/'tombstone.json')))
        writes.fault('layout-recovery-retention-final-tombstone')
        receipt = {'schema': 'agent-memory/layout-retention-receipt-v1', 'cleanup_id': cleanup_id,
                   'operation_id': operation_id, 'request_sha256': digest(encoded(proposal)), 'verified': True,
                   'outcome': 'cleaned', 'deleted_file_count': len(proposal['delete']),
                   'historical_receipt_retained': True, 'original_bytes_available': False,
                   'rollback_available': False, 'readback_at': datetime.now(timezone.utc).isoformat()}
        state.write_once(cleanup/'receipt.json', receipt)
    return publication.result('retention-apply', operation_id, 'retention-cleaned', cleanup_id=cleanup_id,
                              receipt=receipt, private_artifact_writes_performed=True,
                              private_artifact_deletions_performed=True, original_bytes_available=False,
                              rollback_available=False, semantic_recovery_complete=False, derived_cache_state=cache_state)
