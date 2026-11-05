"""Private, source-preserving publication proofs for invalid authority layouts.

This deliberately does not use semantic-sharing-v2 as a proof of an invalid tree.
The existing publication policy/PR/check machinery remains the landing authority.
"""
from __future__ import annotations

from datetime import datetime, timezone
import hashlib
from pathlib import Path, PurePosixPath
import tempfile
import resource
import subprocess
import unicodedata
import uuid

from . import bank_contract as bank, bank_observation, bank_transactions as tx
from . import bank_writes as writes, local_registry as storage, repository_config
from . import scope_binding, security, sharing_recovery as git, memory_publication as publication
from . import layout_recovery_state as state, recovery_frontier as frontier
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError

from . import layout_recovery_observation as raw_sources

MAX_FILE = raw_sources.MAX_FILE
MAX_BYTES = raw_sources.MAX_BYTES
MAX_FILES = raw_sources.MAX_FILES
MAX_ENTRIES = raw_sources.MAX_ENTRIES
MAX_DEPTH = raw_sources.MAX_DEPTH
OBSERVATION = 'agent-memory/layout-publication-observation-v1'
REVIEW = 'agent-memory/layout-publication-review-v1'
REQUEST = 'agent-memory/layout-publication-request-v1'
TERMINAL = {'admitted', 'governed-terminal', 'reviewed-non-guidance-discard'}


def require(condition, message):
    if not condition:
        raise ContractError(message)


def exact(value, fields, schema=None):
    require(isinstance(value, dict) and set(value) == set(fields)
            and (schema is None or value.get('schema') == schema),
            'Use the exact typed private recovery publication artifact')
    return value


def text(value, maximum=4096):
    require(isinstance(value, str) and bool(value.strip()) and len(value.encode()) <= maximum,
            'A bounded explicit source-preservation judgment is required')
    return value


def result(operation, operation_id, status, **values):
    return {'schema': 'agent-memory/layout-recovery-result-v1', 'operation': operation,
            'operation_id': operation_id, 'state': status, 'publication_verified': False,
            'verified': not status.startswith('pending'), 'canonical_eligibility': 'unavailable',
            'semantic_recovery_complete': False, 'pending_unit_ids': [], 'pending_units': 0,
            'required_actions': [], 'head_writes_performed': False, 'feature_writes_performed': False,
            'canonical_writes_performed': False, 'original_index_writes_performed': False,
            'history_writes_performed': False, 'private_artifact_writes_performed': False,
            'feature_code_freshness_verified': False, 'source_assertions_verified': False,
            'other_machine_import_verified': False, **values}


def operation_evidence(repo, operation_id, *, owner=None):
    """Check the real terminal ledger, not a historical layout-completion boolean."""
    require(uuid_valid(operation_id), 'Use the original recovery operation UUID')
    root, bank_path, base, directory = state.paths(repo, operation_id)
    token = frontier.observe(root, owner=owner)
    require(token.get('coverage_complete') is True and not token.get('unit_ids'),
            'Publication/retention is pending while any recovery source remains unresolved')
    index = state.index(root)
    require(operation_id in index['operations'], 'Unknown recovery operation; retain all evidence')
    receipt = state.read(directory / 'receipt.json')
    require(receipt.get('schema') == 'agent-memory/layout-recovery-receipt-v1'
            and receipt.get('operation_id') == operation_id and receipt.get('verified') is True
            and receipt.get('outcome') in {'layout-restored', 'rolled-back'},
            'Recovery needs its immutable, completed layout receipt')
    directory, head, ledger = frontier.ledger(root, operation_id, index['operations'][operation_id])
    manifest, proposal = frontier.evidence(directory, ledger, index['operations'][operation_id])
    require(receipt.get('request_sha256') == digest(encoded(proposal))
            and receipt.get('proposal_id') == proposal['proposal_id'],
            'Layout receipt is not the original exact reviewed operation')
    for source in manifest['sources']:
        require(uuid_valid(source['source_id']), 'Invalid retained original source identity')
        raw = state.read_bytes(directory/'originals'/(source['source_id']+'.bin'), MAX_FILE)
        require(len(raw) == source['byte_count'] and digest(raw) == source['sha256'],
                'Missing or damaged original backup; publication/retention remains pending')
    units = {u['unit_id']: u for u in ledger['units']}
    for unit in units.values():
        if unit.get('state') == 'superseded-by-revision':
            children = unit.get('child_unit_ids')
            require(isinstance(children, list) and bool(children)
                    and all(child in units and units[child].get('state') in TERMINAL for child in children),
                    'A revised recovery unit is not terminal')
        else:
            require(unit.get('state') in TERMINAL and bool(unit.get('terminal_receipts')),
                    'A recovery unit lacks its actual reviewed terminal disposition')
    require(ledger.get('project_id') == token.get('project_id') == receipt.get('project_id'),
            'Recovery evidence belongs to another project')
    return root, bank_path, base, directory, {
        'frontier_token': token, 'ledger_head': head, 'receipt_sha256': digest(encoded(receipt)),
        'index_sha256': digest(encoded(index)), 'project_id': ledger['project_id']}


def _path(path):
    raw_sources.path_value(path)
    if len(PurePosixPath(path).parts) > MAX_DEPTH:
        raise raw_sources.RawCapacityError('depth', MAX_DEPTH, len(PurePosixPath(path).parts), 'exact')
    return path


def _objects(repo, objects):
    """Two bounded Git children; disclose one source at a time in 8KiB chunks."""
    if not objects:
        return
    payload = ('\n'.join(oid for _, oid in objects)+'\n').encode('ascii')
    metadata = git.raw_command(repo, 'cat-file', '--batch-check', payload=payload, limit=len(objects)*128)
    rows = metadata.decode('ascii').splitlines()
    require(len(rows) == len(objects), 'Incomplete raw object size inventory')
    sizes, total = [], 0
    for (_, oid), row in zip(objects, rows):
        fields = row.split()
        require(len(fields) == 3 and fields[:2] == [oid, 'blob'] and fields[2].isdigit(),
                'Raw object size inventory substituted a source')
        size = int(fields[2])
        total += size
        if size > MAX_FILE:
            raise raw_sources.RawCapacityError('source-bytes', MAX_FILE, size, 'exact')
        if total > MAX_BYTES:
            raise raw_sources.RawCapacityError('aggregate-bytes', MAX_BYTES, total)
        sizes.append(size)
    _, _, base, _ = state.paths(repo)
    state.private_parent(base)
    limit = total+len(objects)*128
    def bound_output():
        previous = resource.getrlimit(resource.RLIMIT_FSIZE)
        cap = min([limit+1, *[v for v in previous if v != resource.RLIM_INFINITY]])
        resource.setrlimit(resource.RLIMIT_FSIZE, (cap, cap))
    environment = publication.environment()
    environment['GIT_ALLOW_PROTOCOL'] = ''
    argv = ['git', '--no-pager', '--no-replace-objects', '--literal-pathspecs',
            '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false',
            '-c', 'core.hooksPath=/dev/null', '-C', str(repo), 'cat-file', '--batch']
    with tempfile.TemporaryFile(dir=base) as spool:
        try:
            process = subprocess.run(argv, env=environment, input=payload, stdout=spool,
                                     stderr=subprocess.DEVNULL, timeout=30, preexec_fn=bound_output)
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise ContractError('Raw Git object observation unavailable; retain private evidence') from exc
        require(process.returncode == 0 and spool.tell() <= limit, 'Raw object output exceeded its complete bound')
        spool.seek(0)
        for (path, oid), size in zip(objects, sizes):
            header = spool.readline(128)
            require(header == (oid+' blob '+str(size)+'\n').encode(), 'Raw object framing/identity changed')
            remaining, chunks = size, []
            algorithm = hashlib.sha1 if len(oid) == 40 else hashlib.sha256
            hashed = algorithm(b'blob '+str(size).encode()+b'\0')
            while remaining:
                chunk = spool.read(min(8192, remaining))
                require(bool(chunk), 'Incomplete raw object source bytes')
                chunks.append(chunk)
                hashed.update(chunk)
                remaining -= len(chunk)
            require(spool.read(1) == b'\n' and hashed.hexdigest() == oid,
                    'Raw Git source bytes do not match their exact immutable object')
            yield path, oid, b''.join(chunks)
        require(not spool.read(1), 'Unexpected trailing raw object bytes')


def observe_tree(repo, revision, prefix, directory=None, *, expected=None):
    """Inventory all raw bank objects, including unsupported ordinary filenames.

    Git tree objects are immutable; unlike a working tree they have no Unix
    ownership metadata. Modes and object identities are the physical facts here.
    Raw bytes never enter a normal snapshot, stdout or shared proof trailer.
    """
    git.commit(repo, revision)
    raw = git.raw_command(repo, 'ls-tree', '-rtz', '--full-tree', revision, '--', prefix,
                          limit=git.MAX_LOCATOR_TREE_BYTES)
    physical, sources, names, objects, total = [], [], {}, [], 0
    prior = {s['path']: s for s in expected['sources']} if expected is not None else {}
    for item in raw.split(b'\0'):
        if not item:
            continue
        header, name = item.split(b'\t', 1)
        mode, kind, oid = header.decode('ascii').split()
        full = name.decode('utf-8')
        if full == prefix and kind == 'tree':
            continue
        require(full.startswith(prefix + '/'), 'Raw inventory escaped the selected bank')
        path = _path(full[len(prefix) + 1:])
        alias = unicodedata.normalize('NFC', path).casefold()
        require(alias not in names, 'Raw source aliases require explicit review')
        if len(physical) >= MAX_ENTRIES:
            raise raw_sources.RawCapacityError('physical-entries', MAX_ENTRIES, len(physical)+1)
        names[alias] = path
        require((kind, mode) in {('tree', '040000'), ('blob', '100644')},
                'Redirect, executable, submodule or nonordinary raw bank object requires review')
        require(not bank.runtime_path(path), 'A committed recovery/runtime marker is not portable publication evidence')
        entry = {'path': path, 'type': kind, 'mode': mode, 'git_oid': git.oid(oid)}
        physical.append(entry)
        if kind == 'tree':
            continue
        if len(objects) >= MAX_FILES:
            raise raw_sources.RawCapacityError('source-files', MAX_FILES, len(objects)+1)
        objects.append((path, oid))
    for path, oid, body in _objects(repo, objects):
        count = len(body)
        total += count
        source_id = prior[path]['source_id'] if path in prior else str(uuid.uuid4())
        source = {'source_id': source_id, 'path': path, 'byte_count': count,
                  'sha256': digest(body), 'git_blob': oid, 'blob': 'originals/'+source_id+'.bin'}
        if directory is not None:
            state.put_bytes(directory / source['blob'], body)
        sources.append(source)
    manifest = {'schema': 'agent-memory/layout-publication-tree-v1', 'revision': revision,
                'bank_root': prefix, 'observation_complete': True, 'physical_entries': physical,
                'sources': sources, 'source_bytes': total}
    if expected is not None:
        require(manifest == expected, 'Raw authority source inventory changed; obtain a new independent review')
    return manifest


def _files(observation):
    return {p: raw.decode('utf-8') for p, raw in observation.sources.items()}


def _raw_body(directory, source):
    raw = state.read_bytes(directory / source['blob'], maximum=MAX_FILE)
    require(len(raw) == source['byte_count'] and digest(raw) == source['sha256'],
            'Missing or damaged raw publication backup; retain the operation')
    return raw


def _fetch(root, remote, branch):
    advertised = git.server(root, remote, branch)
    ref = 'refs/agent-memory/observations/'+str(uuid.uuid4())
    try:
        git.command(root, 'fetch', '--no-tags', '--no-write-fetch-head', '--refmap=', '--no-recurse-submodules',
                    '--', remote, 'refs/heads/'+branch+':'+ref, network=True, limit=65536)
        require(git.command(root, 'rev-parse', '--verify', ref).strip() == advertised
                and git.server(root, remote, branch) == advertised,
                'Authority advanced during recovery publication observation')
        return advertised
    finally:
        git.command(root, 'update-ref', '-d', ref)


def _local_boundary(root, observed, manifest):
    """Keep normal index/ignore rules, proving invalid-base deletions from raw pins."""
    prefix = repository_config.relative(root)
    original = {s['path']: s for s in manifest['sources']}
    indexed = git.command(root, 'ls-files', '--stage', '-z', '--', prefix, limit=git.MAX_LOCATOR_TREE_BYTES)
    for item in indexed.split('\0'):
        if not item:
            continue
        header, full = item.split('\t', 1)
        mode, blob, stage = header.split()
        require(full.startswith(prefix+'/') and stage == '0' and mode == '100644',
                'Unmerged/nonordinary original memory index requires inspection')
        path = full[len(prefix)+1:]
        if path not in observed:
            source = original.get(path)
            require(source is not None and not bank.runtime_path(path),
                    'Unknown indexed artifact cannot be omitted by recovery publication')
            body = git.raw_command(root, 'cat-file', 'blob', git.oid(blob), limit=MAX_FILE)
            require(digest(body) == source['sha256'], 'Indexed deletion is not the inspected recovery source')
    ignored = git.command(root, 'ls-files', '--cached', '--others', '--ignored', '--exclude-standard', '-z',
                          '--', prefix, limit=git.MAX_LOCATOR_TREE_BYTES)
    require(not any(p[len(prefix)+1:] in observed for p in ignored.split('\0') if p),
            'Ignored canonical memory requires explicit boundary repair')
    return digest(indexed.encode())


def _observe(repo, operation_id, observation_id=None):
    root, bank_path, _, operation, terminal = operation_evidence(repo, operation_id)
    require(state.read(operation/'receipt.json').get('outcome') != 'rolled-back',
            'A rolled-back invalid layout is not a publication result')
    manifest = state.read(operation/'manifest.json')
    require(manifest.get('observation_complete') is True, 'Original source observation is incomplete')
    local = bank_observation.observe(root)
    files = _files(local)
    require(not security.complete_refresh_reasons(files), 'Unsafe/unadmitted publication result requires governed review')
    checked = bank.validate_files(files)
    require(checked['project_id'] == terminal['project_id']
            and all(e['lifecycle'] not in {'candidate', 'quarantined'} for e in checked['entries']),
            'The complete local result must be admitted, same-project native memory')
    context = scope_binding.require(root, terminal['project_id'], expected=manifest['identity']['scope_context'])
    project = scope_binding.project.status(root)['evidence']
    settings = repository_config.settings(root)
    remote = project['authority_name']
    require(remote, 'Recovery publication requires the existing verified authority')
    repository_config.safe_label(remote)
    branch = settings['authority_branch']
    if not branch:
        remote_head = git.command(root, 'symbolic-ref', '--quiet', 'refs/remotes/'+remote+'/HEAD').strip()
        prefix = 'refs/remotes/'+remote+'/'
        require(remote_head.startswith(prefix), 'Explicit/default authority branch observation is required')
        branch = remote_head[len(prefix):]
    repository_config.validate_branch(branch)
    index = publication.index_bytes(root)
    head = git.command(root, 'rev-parse', '--verify', 'HEAD').strip()
    authority = _fetch(root, remote, branch)
    base = git.command(root, 'merge-base', head, authority).strip()
    git.commit(root, base)
    git.authority_bank_location(root, authority, settings['bank_root'])
    observation_id = observation_id or str(uuid.uuid4())
    directory = operation/'publication'/observation_id
    existing = state.read(directory/'observation.json', missing=True)
    variants = {}
    for side, revision in (('base', base), ('landed', authority)):
        expected = existing['variants'][side] if existing else None
        variants[side] = observe_tree(root, revision, settings['bank_root'], directory, expected=expected)
    if not variants['landed']['sources']:
        git.first_bank_history(root, authority)
    prior = existing['variants']['local']['sources'] if existing else None
    local_physical, local_sources, local_totals = raw_sources.inventory(
        bank_path, retain_directory=directory, prior_sources=prior)
    raw_hashes = {source['path']: source['sha256'] for source in local_sources}
    require(all(raw_hashes.get(path) == sha for path, sha in local.hashes.items()),
            'Canonical and complete raw local observations disagree')
    variants['local'] = {'schema': 'agent-memory/layout-publication-local-v1', 'revision': head,
                         'observation_complete': True, 'sources': local_sources,
                         'physical_entries': local_physical, 'totals': local_totals}
    indexed = _local_boundary(root, local.hashes, manifest)
    binding = {'repo_root': str(root), 'project_id': checked['project_id'], 'bank_root': settings['bank_root'],
               'authority_remote': remote, 'authority_branch': branch, 'authority_head': authority,
               'local_head': head, 'base': base, 'scope_context': context}
    observed_policy = publication.policy(root, binding)
    value = {'schema': OBSERVATION, 'observation_id': observation_id, 'operation_id': operation_id,
             'binding': binding, 'terminal': terminal, 'original_manifest_sha256': digest(encoded(manifest)),
             'variants': variants, 'local_hashes': local.hashes, 'original_index_sha256': index,
             'index_entries_sha256': indexed, 'repository_policy': observed_policy,
             'runtime_artifact_digest': _runtime_digest()}
    local.recheck()
    raw_readback = raw_sources.inventory(bank_path, prior_sources=local_sources)
    require(raw_readback == (local_physical, local_sources, local_totals),
            'Complete local raw bytes, occupants or physical metadata changed during publication observation')
    frontier.check(terminal['frontier_token'], root)
    scope_binding.require(root, checked['project_id'], expected=context)
    require(publication.index_bytes(root) == index and git.server(root, remote, branch) == authority,
            'Local index or advertised authority changed during publication observation')
    state.write_once(directory/'observation.json', value)
    return root, directory, value, files


def _preservable_source(observation, source, publication_unit):
    """Unsupported source names do not acquire admission merely by relocation."""
    if source['path'] == 'MEMORY.md':
        return
    try:
        bank.path_value(source['path'])
        return
    except ContractError:
        pass
    root = observation['binding']['repo_root']
    operation_id = observation['operation_id']
    row = state.index(root)['operations'][operation_id]
    directory, _, ledger = frontier.ledger(root, operation_id, row)
    manifest, _ = frontier.evidence(directory, ledger, row)
    original = next((s for s in manifest['sources'] if s['path'] == source['path']
                     and s['sha256'] == source['sha256'] and s['byte_count'] == source['byte_count']), None)
    require(original is not None,
            'New unsupported authority/local source needs explicit recovery/admission review before publication')
    unit_map = state.read(directory/'unit-map.json')
    entitled = [unit for unit in unit_map['units'] if unit['disposition'] == 'preserve'
                and unit.get('semantic_kind') != 'unadmitted-source']
    # Revised pending parents remain historical. Actual admitted terminal child
    # ranges, not a superseded parent assertion, establish their entitlement.
    entitled.extend(unit for unit in ledger['units'] if unit['state'] in {'admitted', 'governed-terminal'})
    spans = sorted((max(r['start'], publication_unit['start']), min(r['end'], publication_unit['end']))
                   for unit in entitled for r in unit['source_ranges']
                   if r['source_id'] == original['source_id']
                   and r['end'] > publication_unit['start'] and r['start'] < publication_unit['end'])
    cursor = publication_unit['start']
    for start, end in spans:
        require(start <= cursor, 'A publication preservation range contains unadmitted source material')
        cursor = max(cursor, end)
    require(bool(spans) and cursor == publication_unit['end'],
        'Retained opaque, unknown or unconfirmed source is not admitted guidance by a publication rename')


def _runtime_digest():
    # The package inventory binds executing source, schema and guide bytes without
    # importing a candidate payload or granting permission from its declaration.
    from . import pack_validation
    inventory = pack_validation.inventory(Path(__file__).resolve().parents[2])
    return digest(encoded({p: {'sha256': digest(raw), 'mode': mode}
                           for p, (raw, mode) in sorted(inventory.items())}))


def inspect(repo, operation_id, input=None):
    require(input is None, 'First publication inspection takes no caller-supplied authority pins')
    _, directory, observation, _ = _observe(repo, operation_id)
    return result('publish-inspect', operation_id, 'publication-observed', observation=observation,
                  observation_path=str(directory/'observation.json'), canonical_eligibility=True,
                  semantic_recovery_complete=True, private_artifact_writes_performed=True,
                  git_metadata_writes_performed=True, remote_freshness_verified=True)


def _review(observation, review, files, directory):
    exact(review, {'schema', 'review_id', 'observation_sha256', 'intended_files_sha256',
                   'author', 'reviewer', 'reviewed_at', 'source_units', 'added_paths',
                   'identity_decisions', 'semantic_preservation', 'security', 'decision'}, REVIEW)
    require(uuid_valid(review['review_id']) and review['observation_sha256'] == digest(encoded(observation))
            and review['intended_files_sha256'] == digest(encoded(files)) and review['decision'] == 'approve'
            and review['semantic_preservation'] is True and review['security'] is True,
            'Independent publication review must bind all exact sources and the complete intended result')
    for principal in (review['author'], review['reviewer']):
        exact(principal, {'developer_id', 'harness', 'session', 'evidence'})
        require(uuid_valid(principal['developer_id']), 'Review requires real developer attribution')
        for field in ('harness', 'session', 'evidence'):
            text(principal[field])
    require((review['author']['harness'], review['author']['session']) !=
            (review['reviewer']['harness'], review['reviewer']['session']),
            'Proposal author cannot independently approve their own publication')
    from .lifecycle import time_value
    time_value(review['reviewed_at'])
    require(review['author']['developer_id'] == observation['binding']['scope_context']['developer_id'],
            'Publication author differs from the actual detecting developer')
    sources = {(side, s['source_id']): s for side, variant in observation['variants'].items() for s in variant['sources']}
    coverage = {key: [] for key in sources}
    require(isinstance(review['source_units'], list), 'Every all-side raw source needs a reviewed primary mapping')
    targets = set()
    for unit in review['source_units']:
        exact(unit, {'unit_id', 'side', 'source_id', 'start', 'end', 'disposition', 'targets', 'rationale', 'terminal_evidence'})
        require(uuid_valid(unit['unit_id']) and (unit['side'], unit['source_id']) in sources,
                'Publication review substituted an unknown raw source')
        source = sources[(unit['side'], unit['source_id'])]
        start, end = unit['start'], unit['end']
        require(type(start) is int and type(end) is int and 0 <= start <= end <= source['byte_count']
                and (start < end or source['byte_count'] == 0), 'Invalid complete source byte range')
        coverage[(unit['side'], unit['source_id'])].append((start, end))
        text(unit['rationale'])
        require(isinstance(unit['targets'], list), 'Source preservation needs exact destination references')
        for target in unit['targets']:
            exact(target, {'path', 'sha256'})
            require(target['path'] in files and digest(files[target['path']].encode()) == target['sha256'],
                    'Source mapping target differs from the intended complete result')
            targets.add(target['path'])
        disposition = unit['disposition']
        require(disposition in {'preserve', 'governed-terminal', 'reviewed-non-guidance-discard'},
                'New authority sources needing admission/clarification must remain pending, not publish')
        if disposition == 'preserve':
            require(bool(unit['targets']) and unit['terminal_evidence'] is None,
                    'Preserved knowledge must remain available, not merely backed up')
            _preservable_source(observation, source, unit)
        elif disposition == 'governed-terminal':
            _governed_evidence(observation, unit, source, files, directory)
        else:
            evidence = unit['terminal_evidence']
            exact(evidence, {'authority', 'reason', 'non_guidance'})
            require(evidence['non_guidance'] is True and not unit['targets'],
                    'Only explicitly reviewed non-guidance may leave current knowledge')
            text(evidence['authority']); text(evidence['reason'])
            # A typed record/history can never be relabelled disposable framing.
            from . import records
            try:
                kind, _, _ = records.parse(_raw_body(directory, source).decode('utf-8'))
            except (ContractError, UnicodeError):
                kind = None
            require(kind not in {'standalone-record', 'tombstone', 'optional-history-event'},
                    'Existing admitted records/history require actual governed terminal evidence')
    for key, spans in coverage.items():
        cursor = 0
        for start, end in sorted(spans):
            require(start == cursor, 'All raw source bytes must be mapped exactly once without gaps or overlap')
            cursor = end
        require(bool(spans) and cursor == sources[key]['byte_count'], 'Unread raw source remainder cannot be omitted')
    require(len({u['unit_id'] for u in review['source_units']}) == len(review['source_units']), 'Duplicate publication unit ID')
    require(isinstance(review['added_paths'], list) and len(set(review['added_paths'])) == len(review['added_paths'])
            and set(review['added_paths']) == set(files)-targets,
            'Independent review must account for every intended output, including newly admitted additions')
    decisions = review['identity_decisions']
    require(isinstance(decisions, dict) and set(decisions) == {'base', 'local', 'landed'}, 'Review all three project/authority identities')
    for side, variant in observation['variants'].items():
        router = next((s for s in variant['sources'] if s['path'] == 'MEMORY.md'), None)
        intended = bank.parse_router(files['MEMORY.md'])[0]
        if not variant['sources'] and side != 'local':
            require(not git.command(observation['binding']['repo_root'], 'rev-list', '--max-count=1',
                                    variant['revision'], '--', observation['binding']['bank_root']).strip(),
                    'A previously occupied authority bank cannot be treated as empty')
            text(decisions[side])
            continue
        require(router is not None, 'Absent/ambiguous authority router requires explicit identity repair')
        try:
            metadata = bank.parse_router(_raw_body(directory, router).decode('utf-8'))[0]
        except (ContractError, UnicodeError):
            _identity_review(observation, variant, router, decisions[side], intended)
            continue
        require(metadata['project-id'] == intended['project-id'] == observation['binding']['project_id']
                and metadata.get('canonical-remote') == intended.get('canonical-remote'),
                'Recovery publication cannot choose between conflicting project/authority identities')
        text(decisions[side])
    _preserve_records(observation, files, review, directory)


def _identity_review(observation, variant, router, decision, intended):
    exact(decision, {'schema', 'project_id', 'raw_router_sha256', 'project_registry_sha256',
                     'checkout_key', 'authority_revision', 'authority_router_path',
                     'authority_router_sha256', 'rationale'}, 'agent-memory/layout-publication-identity-review-v1')
    binding = observation['binding']
    require(decision['project_id'] == binding['project_id']
            and decision['raw_router_sha256'] == router['sha256']
            and decision['project_registry_sha256'] == binding['scope_context']['project_sha256']
            and decision['checkout_key'] == binding['scope_context']['checkout_key'],
            'Malformed raw router needs exact independent existing-project/checkout review')
    revision = git.commit(binding['repo_root'], decision['authority_revision'])
    require(git.ancestor(binding['repo_root'], revision, variant['revision']),
            'Stronger identity evidence must be checked authority ancestry, not a path assertion')
    path = decision['authority_router_path']
    require(path == binding['bank_root']+'/MEMORY.md', 'Identity evidence must name the independently discovered bank')
    raw = repository_config.object_router_bytes(binding['repo_root'], revision+':'+path, {})
    metadata = bank.parse_router(raw.decode('utf-8'))[0]
    require(digest(raw) == decision['authority_router_sha256']
            and metadata['project-id'] == intended['project-id'] == binding['project_id']
            and metadata.get('canonical-remote') == intended.get('canonical-remote'),
            'Stronger existing authority evidence does not uniquely establish this project')
    text(decision['rationale'])


def _governed_evidence(observation, unit, source, files, directory):
    evidence = unit['terminal_evidence']
    exact(evidence, {'operation_id', 'receipt_sha256', 'proposal'})
    proposal = tx.proposal_value(evidence['proposal'])
    root = Path(observation['binding']['repo_root'])
    require(proposal['operation_id'] == evidence['operation_id'] and proposal['repo_root'] == str(root)
            and proposal['project_id'] == observation['binding']['project_id']
            and 'governance_evidence' in proposal and proposal['observed_hashes'].get(source['path']) == source['sha256'],
            'Governed source removal requires the actual same-source normal governance proposal')
    _, receipt_path = writes.state_paths(root, root/observation['binding']['bank_root'], evidence['operation_id'])
    receipt = tx.receipt_value(receipt_path, proposal)
    require(receipt is not None and receipt['outcome'] == 'finished'
            and digest(encoded(receipt)) == evidence['receipt_sha256']
            and all(files.get(path) == proposal['files'].get(path) for path in tx.changed(proposal)),
            'Proposed, partial or mismatched governance is not a terminal publication disposition')


def _preserve_records(observation, files, review, directory):
    from . import records
    output = {}
    for path, body in files.items():
        try:
            kind, metadata, _ = records.parse(body)
        except ContractError:
            continue
        if kind in {'standalone-record', 'tombstone', 'optional-history-event'}:
            output[(metadata.get('id'), metadata.get('version'))] = (metadata, body)
    for side, variant in observation['variants'].items():
        for source in variant['sources']:
            try:
                kind, metadata, body = records.parse(_raw_body(directory, source).decode('utf-8'))
            except (ContractError, UnicodeError):
                continue
            if kind not in {'standalone-record', 'tombstone', 'optional-history-event'}:
                continue
            units = [u for u in review['source_units'] if u['side'] == side and u['source_id'] == source['source_id']]
            if any(u['disposition'] == 'governed-terminal' for u in units):
                continue
            target = output.get((metadata.get('id'), metadata.get('version')))
            require(target is not None and target[0] == metadata and records.parse(target[1])[2] == body,
                    'Existing typed identities, provenance, conditions and history cannot be rewritten by layout publication')


def _candidate(root, binding, before, files):
    before_by_path = {s['path']: s for s in before['sources']}
    operations, _ = writes.state_paths(root, root/binding['bank_root'], str(uuid.uuid4()))
    storage.private_mkdir(operations)
    with tempfile.TemporaryDirectory(prefix='layout-publication-', dir=operations) as directory:
        index = Path(directory)/'index'
        publication.git_write(root, 'read-tree', binding['authority_head'], index=index)
        for path in sorted(set(before_by_path) | set(files)):
            full = binding['bank_root']+'/'+path
            if path not in files:
                publication.git_write(root, 'update-index', '--force-remove', '--', full, index=index)
            else:
                _, oid = publication.git_write(root, 'hash-object', '-w', '--stdin', payload=files[path].encode())
                publication.git_write(root, 'update-index', '--add', '--cacheinfo', '100644,'+oid.strip()+','+full, index=index)
        _, tree = publication.git_write(root, 'write-tree', index=index)
        signing = publication.git(root, 'config', '--bool', 'commit.gpgsign', optional=True)
        _, commit = publication.git_write(root, 'commit-tree', tree.strip(), '-p', binding['authority_head'],
                                         *(['-S'] if signing == 'true' else []),
                                         '-m', 'Publish source-preserving project memory recovery')
    return git.oid(commit.strip())


def _delta(root, base, candidate, prefix, before, files):
    require(git.command(root, 'rev-list', '--parents', '-n', '1', candidate).strip().split()[1:] == [base],
            'Recovery candidate must have exactly the reviewed authority parent')
    raw = git.command(root, 'diff-tree', '--no-commit-id', '--no-renames', '--raw', '-r', '-z', base, candidate,
                      limit=git.MAX_LOCATOR_TREE_BYTES).split('\0')
    changed = {}
    for i in range(0, len(raw)-1, 2):
        header, path = raw[i], raw[i+1]
        fields = header.split()
        require(len(fields) == 5 and path.startswith(prefix+'/') and fields[4] in {'A', 'M', 'D'}
                and fields[0] in {':000000', ':100644'} and fields[1] in {'000000', '100644'},
                'Entire authority delta contains an unreviewed code, mode, runtime or topology change')
        relative = path[len(prefix)+1:]
        require(not bank.runtime_path(relative), 'Runtime recovery artifacts must never be published')
        changed[relative] = {'before': fields[2], 'after': fields[3], 'status': fields[4]}
    expected = {p for p in set(before)|set(files) if before.get(p) != (digest(files[p].encode()) if p in files else None)}
    require(set(changed) == expected and bool(changed), 'Full candidate delta differs from the complete intended authority transformation')
    candidate_files = git.tree(root, candidate, prefix)
    require(candidate_files == files, 'Candidate normal readback differs from exact intended native files')
    return changed


def prepare(repo, operation_id, input):
    exact(input, {'schema', 'observation_id', 'intended_files', 'review'}, 'agent-memory/layout-publication-prepare-v1')
    require(uuid_valid(input['observation_id']), 'Use the inspected publication observation UUID')
    root, directory, observation, local_files = _observe(repo, operation_id, input['observation_id'])
    files = input['intended_files']
    require(isinstance(files, dict) and bank.validate_files(files)['project_id'] == observation['binding']['project_id']
            and not security.complete_refresh_reasons(files), 'Complete publication result must pass normal eligibility/security')
    require(all(e['lifecycle'] not in {'candidate', 'quarantined'} for e in bank.validate_files(files)['entries']),
            'Publication cannot automatically admit newly proposed records')
    _review(observation, input['review'], files, directory)
    policy = observation['repository_policy']
    if policy['unsupported_rules']:
        return result('publish-prepare', operation_id, 'pending-policy', repository_policy=policy,
                      git_metadata_writes_performed=True, private_artifact_writes_performed=True)
    binding = observation['binding']
    before = {s['path']: s['sha256'] for s in observation['variants']['landed']['sources']}
    bank.file_topology(set(before) | set(files))
    if before == tx.file_hashes(files):
        return result('publish-prepare', operation_id, 'no-publishable-delta', git_metadata_writes_performed=True)
    candidate = _candidate(root, binding, observation['variants']['landed'], files)
    delta = _delta(root, binding['authority_head'], candidate, binding['bank_root'], before, files)
    request = {'schema': REQUEST, 'operation_id': operation_id, 'repo_root': str(root),
               'project_id': binding['project_id'], 'base': binding['authority_head'], 'candidate': candidate,
               'observation': observation, 'intended_files': files, 'review': input['review'], 'delta': delta}
    _check_current(root, request)
    state.write_once(directory/'request.json', request)
    return result('publish-prepare', operation_id, 'prepared', request=request,
                  private_artifact_writes_performed=True, git_metadata_writes_performed=True,
                  canonical_eligibility=True, semantic_recovery_complete=True)


def _request(repo, operation_id, request):
    exact(request, {'schema', 'operation_id', 'repo_root', 'project_id', 'base', 'candidate',
                    'observation', 'intended_files', 'review', 'delta'}, REQUEST)
    root, _, _, operation = state.paths(repo, operation_id)
    observation = request['observation']
    require(request['operation_id'] == operation_id and request['repo_root'] == str(root)
            and observation['operation_id'] == operation_id and uuid_valid(observation['observation_id'])
            and request['project_id'] == observation['binding']['project_id']
            and request['base'] == observation['binding']['authority_head'], 'Publication request binding changed')
    directory = operation/'publication'/observation['observation_id']
    require(state.read(directory/'request.json') == request, 'Use the exact immutable prepared publication request')
    return root, directory


def _check_current(root, request):
    _, directory, current, _ = _observe(root, request['operation_id'], request['observation']['observation_id'])
    require(current == request['observation'], 'Authority, policy, runtime, source or terminal frontier changed')
    _review(current, request['review'], request['intended_files'], directory)
    before = {s['path']: s['sha256'] for s in current['variants']['landed']['sources']}
    require(_delta(root, request['base'], request['candidate'], current['binding']['bank_root'], before,
                   request['intended_files']) == request['delta'], 'Actual prepared authority delta changed')


class RecoveryPublicationContext:
    """An internal checked source guard; never deserialized from a request."""
    def __init__(self, root, request):
        self.root, self.request = root, request
        _check_current(root, request)

    def check(self, root, binding):
        require(root == self.root and binding == self.request['observation']['binding'],
                'Recovery publication context does not match the existing landing operation')
        _check_current(root, self.request)


def _replay(root, request):
    binding = request['observation']['binding']
    head = _fetch(root, binding['authority_remote'], binding['authority_branch'])
    lineage = git.command(root, 'rev-list', '--first-parent', '--max-count=130', head).splitlines()
    require(request['base'] in lineage and lineage.index(request['base']) <= 128,
            'Recovery publication mainline witness is unavailable; do not blindly retry a push')
    delta = {path: request['intended_files'].get(path) for path in request['delta']}
    witness = None
    candidate_tree = git.command(root, 'rev-parse', request['candidate']+'^{tree}').strip()
    for revision in lineage[:lineage.index(request['base'])]:
        if git.command(root, 'rev-parse', revision+'^{tree}').strip() == candidate_tree:
            # An exact whole authority tree, not reachability or a subset of
            # memory paths, witnesses this independently reviewed candidate.
            require(git.tree(root, revision, binding['bank_root']) == request['intended_files'],
                    'Historical repaired authority tree differs from the complete intended result')
            witness = revision
            break
    try:
        current = observe_tree(root, head, binding['bank_root'])
        hashes = {s['path']: s['sha256'] for s in current['sources']}
        still_current = all(hashes.get(p) == (digest(body.encode()) if body is not None else None)
                            for p, body in delta.items())
    except ContractError:
        if witness is None:
            raise
        still_current = 'unavailable'
    require(git.server(root, binding['authority_remote'], binding['authority_branch']) == head,
            'Authority advanced during historical publication readback')
    return {'witnessed_commit': witness, 'intended_delta_current': still_current}


def land(repo, operation_id, input):
    root, directory = _request(repo, operation_id, input)
    request = input
    # Historical success is checked before demanding old local sources reappear.
    # It never replays bytes over later canonical/authority corrections.
    scope_binding.require(root, request['project_id'])
    original_index = publication.index_bytes(root)
    readback = _replay(root, request)
    if readback['witnessed_commit'] is not None:
        require(publication.index_bytes(root) == original_index, 'Original feature index changed during historical readback')
        receipt = state.read(directory/'receipt.json', missing=True)
        created = receipt is None
        if created:
            receipt = {'schema': 'agent-memory/layout-publication-receipt-v1', 'operation_id': operation_id,
                       'request_sha256': digest(encoded(request)), 'publication_verified': True,
                       'readback_at': datetime.now(timezone.utc).isoformat(), **readback}
            state.write_once(directory/'receipt.json', receipt)
        require(receipt.get('request_sha256') == digest(encoded(request))
                and receipt.get('publication_verified') is True,
                'Historical publication receipt does not match this exact request')
        return result('publish-land', operation_id, 'already-landed', publication_verified=True,
                      receipt_is_historical=True, already_applied=True, git_metadata_writes_performed=True,
                      receipt=receipt, private_artifact_writes_performed=created, **readback)
    context = RecoveryPublicationContext(root, request)
    binding = request['observation']['binding']
    observed_policy = publication.policy(root, binding)
    if observed_policy['unsupported_rules']:
        return result('publish-land', operation_id, 'pending-policy', repository_policy=observed_policy,
                      git_metadata_writes_performed=True)
    # Adapter supplies only the binding expected by the existing policy engine;
    # it is never passed to semantic.check or claimed as a sharing-v2 proof.
    landing = {'base': request['base'], 'candidate': request['candidate'], 'project_id': request['project_id'],
               'proof': {'judgment': {'binding': binding}}}
    proposed, url = request['candidate'], None
    if observed_policy['protected']:
        proposed, url, reason = publication.protected_candidate(root, landing, observed_policy, recovery_context=context)
        if proposed is None:
            return result('publish-land', operation_id, 'pending-repository-rules', reason=reason,
                          pull_request=url, git_metadata_writes_performed=True)
    context.check(root, binding)
    require(publication.policy(root, binding) == observed_policy
            and publication.index_bytes(root) == original_index, 'Policy/original index changed before recovery landing')
    if observed_policy['protected']:
        proposed, url, reason = publication.protected_candidate(root, landing, observed_policy, recovery_context=context)
        if proposed is None:
            return result('publish-land', operation_id, 'pending-repository-rules', reason=reason,
                          pull_request=url, git_metadata_writes_performed=True)
    context.check(root, binding)
    writes.fault('layout-recovery-publication-before-push')
    code, _ = publication.git_write(root, 'push', '--porcelain', '--no-follow-tags', '--recurse-submodules=no', '--',
                                    binding['authority_remote'], proposed+':refs/heads/'+binding['authority_branch'],
                                    network=True, optional=True)
    writes.fault('layout-recovery-publication-after-push')
    readback = _replay(root, request)
    require(publication.index_bytes(root) == original_index, 'Original feature index changed externally; preserve it')
    if readback['witnessed_commit'] is None:
        return result('publish-land', operation_id, 'pending-reconciliation', transport_exit=code,
                      pull_request=url, git_metadata_writes_performed=True)
    receipt = {'schema': 'agent-memory/layout-publication-receipt-v1', 'operation_id': operation_id,
               'request_sha256': digest(encoded(request)), 'publication_verified': True,
               'readback_at': datetime.now(timezone.utc).isoformat(), **readback}
    state.write_once(directory/'receipt.json', receipt)
    return result('publish-land', operation_id, 'landed', publication_verified=True,
                  receipt=receipt, pull_request=url, private_artifact_writes_performed=True,
                  git_metadata_writes_performed=True, history_writes_performed=True, **readback)
