"""Native omp boundary CLI and conservative bank-wide lifecycle coordination."""
from contextlib import ExitStack, nullcontext
import argparse
import hashlib
import os
from pathlib import Path
import re
import sys
import uuid

from . import bank_writes, local_registry, project_identity, trust_paths, turn_sources
from .developer_identity import load
from .frontmatter import ContractError
from .turn_sources import private_lock, source_barrier

CAPABILITY = 'omp-harness-v1'
MARKER = {'schema': 'agent-memory/turn-sources-compat-v1',
          'minimum_reader': CAPABILITY, 'harnesses': ['omp']}


def _base(repo):
    root, bank = bank_writes.bank_path(repo)
    operations, _ = bank_writes.state_paths(root, bank, str(uuid.uuid4()))
    return operations.parent




def _marker(base):
    try:
        value = load(turn_sources.read_bytes(base / 'turn-sources-compat-v1.json', 4096))
    except FileNotFoundError:
        return False
    if value != MARKER:
        raise ContractError('Unknown or malformed source compatibility marker')
    return True


def ensure_marker(repo):
    from . import scope_binding
    root, bank = bank_writes.bank_path(repo)
    base = _base(root)
    record_bank(root)
    context = scope_binding.private_enrollment_context(root)
    binding = {'schema': 'agent-memory/omp-bank-binding-v1', 'repository': str(root), 'bank_path': str(bank),
               'private_base': str(base), 'project_id': context.project_id}
    try:
        turn_sources.exclusive(base / 'omp-bank-binding-v1.json', binding, 4096)
    except FileExistsError:
        if _bank_binding(base) != binding:
            raise ContractError('Native source bank/private-state binding changed')
    if not _marker(base):
        try:
            turn_sources.exclusive(base / 'turn-sources-compat-v1.json', MARKER, 4096)
        except FileExistsError:
            if not _marker(base):
                raise ContractError('Source compatibility marker changed')
    scope_binding.recheck_private_enrollment(context)


def _registry():
    _, path, _ = local_registry.paths('omp-banks')
    try:
        value = load(turn_sources.read_bytes(path, 1024 * 1024))
    except FileNotFoundError:
        return {'schema': 'agent-memory/omp-bank-registry-v1', 'repositories': [], 'private_bases': {}}
    if (not isinstance(value, dict) or set(value) != {'schema', 'repositories', 'private_bases'}
            or value['schema'] != 'agent-memory/omp-bank-registry-v1'
            or not isinstance(value['repositories'], list) or len(value['repositories']) > 1024
            or value['repositories'] != sorted(set(value['repositories']))
            or any(not isinstance(p, str) or not Path(p).is_absolute() or '..' in Path(p).parts
                   for p in value['repositories'])):
        raise ContractError('Unverifiable omp bank registry; preserve private state')
    if (not isinstance(value['private_bases'], dict) or set(value['private_bases']) != set(value['repositories'])
            or any(not isinstance(p, str) or not Path(p).is_absolute() or '..' in Path(p).parts for p in value['private_bases'].values())):
        raise ContractError('Unverifiable omp bank registry; preserve private state')
    return value


def record_bank(repo):
    root = str(Path(repo).resolve())
    private_base = str(_base(repo))
    value = _registry()
    if root in value['repositories']:
        if value['private_bases'][root] != private_base:
            raise ContractError('Native protected private-state location changed; preserve its original binding')
        return
    directory, registry, lock = local_registry.paths('omp-banks')
    local_registry.private_mkdir(directory)
    with local_registry.exclusive(lock, str(uuid.uuid4()), 'omp-enrollment'):
        value = _registry()
        observed = hashlib.sha256(registry.read_bytes()).hexdigest() if registry.exists() else None
        value['repositories'] = sorted(set(value['repositories']) | {root})
        value['private_bases'][root] = private_base
        if len(value['repositories']) > 1024:
            raise ContractError('omp bank registry capacity reached')
        local_registry.replace(registry, turn_sources.encoded(value), observed)


def _bank_binding(base):
    value = load(turn_sources.read_bytes(base / 'omp-bank-binding-v1.json', 4096))
    fields = {'schema', 'repository', 'bank_path', 'private_base', 'project_id'}
    if (not isinstance(value, dict) or set(value) != fields or value['schema'] != 'agent-memory/omp-bank-binding-v1'
            or any(not isinstance(value[name], str) for name in fields)):
        raise ContractError('Unknown native bank binding')
    for name in ('repository', 'bank_path', 'private_base'):
        path = Path(value[name])
        if not path.is_absolute() or '..' in path.parts or path.resolve() != path:
            raise ContractError('Native bank binding has a redirected location')
    if (value['private_base'] != str(base)
            or hashlib.sha256(value['bank_path'].encode()).hexdigest() != base.name
            or not Path(value['bank_path']).is_relative_to(Path(value['repository']))
            or str(uuid.UUID(value['project_id'])) != value['project_id']):
        raise ContractError('Native bank binding differs from its protected namespace')
    return value


def _source_state_bindings(installation, registered):
    state_roots = {bank_writes.state_root()}
    state_roots.update(Path(path).parents[1] for path in registered['private_bases'].values())
    if installation and installation.get('runtime_state'):
        state_roots.add(Path(installation['runtime_state']))
    roots = set()
    for state_root in sorted(state_roots):
        trust_paths.inspect(state_root)
        if not state_root.exists():
            if any(Path(path).is_relative_to(state_root) for path in registered['private_bases'].values()):
                raise ContractError('A saved native private-state root is unavailable')
            continue
        writes = state_root / 'writes'
        if not os.path.lexists(writes):
            continue
        trust_paths.state(writes)
        with os.scandir(writes) as scan:
            entries = sorted(list(scan), key=lambda entry: entry.name)
        if len(entries) > 65536:
            raise ContractError('Private bank inventory exceeds its complete bound')
        for entry in entries:
            if not re.fullmatch(r'[0-9a-f]{64}', entry.name):
                raise ContractError('Unknown private bank namespace')
            base = Path(entry.path)
            trust_paths.state(base)
            binding_path = base / 'omp-bank-binding-v1.json'
            if os.path.lexists(binding_path):
                binding = _bank_binding(base)
                roots.add(Path(binding['repository']))
                expected = bank_writes.state_root() / 'writes' / base.name
                if base != expected:
                    raise ContractError('Native bank remains bound to another protected private-state root')
                continue
            if any(os.path.lexists(base / name) for name in ('turn-sources-compat-v1.json', 'omp-source-v1')):
                raise ContractError('Native marker/source inventory lacks its bound repository')
            pending = base / 'turn-sources-v2' / 'pending'
            if os.path.lexists(pending):
                trust_paths.state(pending)
                with os.scandir(pending) as scan:
                    for number, item in enumerate(scan):
                        if number >= turn_sources.MAX_TURNS:
                            raise ContractError('Private pending source inventory exceeds bound')
                        value = load(turn_sources.read_bytes(Path(item.path), 4096))
                        if not isinstance(value, dict) or value.get('schema') != 'agent-memory/turn-reference-v2':
                            raise ContractError('Private pending source inventory is unverifiable')
                        if value.get('harness') == 'omp':
                            raise ContractError('Orphan indexed native source lacks its bound repository')
    for repo, private_base in registered['private_bases'].items():
        base = Path(private_base)
        if not base.is_dir() or _bank_binding(base)['repository'] != repo:
            raise ContractError('Registered native source binding is unavailable')
    return roots


def source_capability_repositories(installation=None):
    """Union discovery retains unavailable paths so lifecycle cannot call them empty."""
    registered = _registry()
    roots = {Path(p) for p in registered['repositories']}
    roots.update(_source_state_bindings(installation, registered))
    registry, _ = project_identity.observe()
    for checkout in registry['checkouts'].values():
        roots.update(Path(p) for p in checkout['locations'])
    if installation:
        for key in ('rollback_repositories', 'observed_repositories'):
            roots.update(Path(p) for p in installation.get(key, []))
        for binding in installation.get('bank_bindings', []):
            if not isinstance(binding, dict) or not isinstance(binding.get('repository'), str):
                raise ContractError('Unverifiable saved bank binding')
            roots.add(Path(binding['repository']))
    return roots


def _omp_index(repo):
    from . import scope_binding
    directory = _base(repo) / 'turn-sources-v2' / 'pending'
    if not os.path.lexists(directory):
        return []
    trust_paths.state(directory)
    context = scope_binding.private_enrollment_context(repo)
    result = []
    with os.scandir(directory) as entries:
        for number, entry in enumerate(entries):
            if (number >= turn_sources.MAX_TURNS or not re.fullmatch(r'(?:0|[1-9][0-9]*)\.json', entry.name)
                    or int(entry.name[:-5]) >= turn_sources.MAX_TURNS):
                raise ContractError('Unverifiable shared source index inventory')
            path = Path(entry.path)
            reference = turn_sources.validate_reference(load(turn_sources.read_bytes(path, 4096)),
                                                        context.root, context.project_id)
            if reference['harness'] == 'omp':
                result.append((path, reference))
    scope_binding.recheck_private_enrollment(context)
    return result


def _drain_inventory(repo):
    """Bound complete evidence enumeration, including orphan shared namespaces."""
    from . import omp_transport
    base = _base(repo)
    rows, faults = omp_transport.pending_inventory(repo)
    pins = []
    binding_path = base / 'omp-bank-binding-v1.json'
    if os.path.lexists(binding_path):
        _bank_binding(base)
        pins.append([binding_path.name, hashlib.sha256(turn_sources.read_bytes(binding_path, 4096)).hexdigest()])
    for path, reference in _omp_index(repo):
        pins.append([str(path.relative_to(base)), hashlib.sha256(turn_sources.read_bytes(path, 4096)).hexdigest()])
        state = turn_sources.inspect(repo, 'omp', reference['session'], reference['lifecycle'])
        if not state['complete']:
            rows.append({'session': reference['session'], 'lifecycle': reference['lifecycle'],
                         'source_reference': 'omp:index:' + reference['turn'], 'status': 'indexed-pending'})
    count = 0
    def unreadable(_error):
        raise ContractError('Source drain inventory is unreadable; preserve all private evidence')
    roots = [base / 'omp-source-v1']
    for directory, cursor in omp_transport.all_segments(repo):
        roots.append(turn_sources.private_namespace(repo, 'omp', cursor['session'], cursor['lifecycle'])[3])
    # Orphan shared namespaces still participate even without a transport receipt.
    turns = base / 'turn-sources-v2' / 'turns'
    for directory in omp_transport.directories(turns, r'[0-9a-f]{64}'):
        for i in range(turn_sources.MAX_SOURCES):
            try:
                candidate = load(turn_sources.read_bytes(directory / ('source-' + str(i) + '.json'), 4096))
            except FileNotFoundError:
                continue
            if not isinstance(candidate, dict):
                raise ContractError('Unverifiable drain source namespace')
            if candidate.get('harness') == 'omp':
                roots.append(directory)
                break
    for root in sorted(set(roots)):
        if not os.path.lexists(root):
            continue
        trust_paths.state(root)
        for directory, names, files in os.walk(root, followlinks=False, onerror=unreadable):
            trust_paths.state(Path(directory))
            names.sort(); files.sort()
            for name in names:
                trust_paths.state(Path(directory) / name)
            for name in files:
                count += 1
                if count > 262144:
                    raise ContractError('Source drain inventory exceeds bound')
                if name.endswith('.lock'):
                    # Lock files are metadata, never evidence of completion.
                    continue
                path = Path(directory) / name
                raw = turn_sources.read_bytes(path, omp_transport.MAX_PAYLOAD)
                pins.append([str(path.relative_to(base)), hashlib.sha256(raw).hexdigest()])
    pending = [row for row in rows if row.get('status') != 'completed']
    binding = {'repository': str(Path(repo).resolve()), 'private_state': str(base),
               'pins': pins, 'pending': pending, 'faults': faults}
    return binding, hashlib.sha256(turn_sources.encoded(binding)).hexdigest()


def required_source_capabilities(repo):
    """A missing marker needs a real bound drain proof, never an empty fallback."""
    root, bank = bank_writes.bank_path(repo)
    base = _base(root)
    registered = _registry()
    saved_base = registered['private_bases'].get(str(root))
    if saved_base is not None and saved_base != str(base):
        raise ContractError('Native protected private-state location differs from its saved binding')
    binding_path = base / 'omp-bank-binding-v1.json'
    has_binding = os.path.lexists(binding_path)
    if has_binding:
        binding = _bank_binding(base)
        if binding['repository'] != str(root) or binding['bank_path'] != str(bank):
            raise ContractError('Native private source binding differs from the current bank')
    elif saved_base is not None:
        raise ContractError('Saved native bank lacks its protected private-state binding')
    if _marker(base):
        if not has_binding:
            raise ContractError('Native compatibility marker lacks its protected bank binding')
        return {CAPABILITY}
    has_evidence = has_binding or os.path.lexists(base / 'omp-source-v1')
    if _omp_index(repo):
        raise ContractError('Markerless state still contains indexed omp references')
    shared = base / 'turn-sources-v2' / 'turns'
    if os.path.lexists(shared):
        trust_paths.state(shared)
        with os.scandir(shared) as entries:
            for number, entry in enumerate(entries):
                if number >= 65536:
                    raise ContractError('Source namespace inventory exceeds bound')
                directory = Path(entry.path)
                trust_paths.state(directory)
                for i in range(turn_sources.MAX_SOURCES):
                    try:
                        source = load(turn_sources.read_bytes(directory / ('source-' + str(i) + '.json'), 4096))
                    except FileNotFoundError:
                        continue
                    if not isinstance(source, dict):
                        raise ContractError('Unverifiable source inventory')
                    has_evidence |= source.get('harness') == 'omp'
    if not has_evidence:
        return set()
    with private_lock(base / 'omp-transport.lock'):
        binding, digest = _drain_inventory(repo)
        try:
            proof = load(turn_sources.read_bytes(base / 'omp-drains' / (digest + '.json'), 4096))
        except FileNotFoundError:
            raise ContractError('Retained omp evidence lacks its bound lifecycle drain proof') from None
        expected = {'schema': 'agent-memory/omp-drain-v1', 'repository': binding['repository'],
                    'private_state': str(base), 'inventory_sha256': digest, 'writers_quiesced': True}
        if proof != expected or binding['pending'] or binding['faults']:
            raise ContractError('Retained omp drain proof differs from current private evidence')
    return set()


def drain_source_capabilities(repo, *, quiesced=False, check=False):
    """Aware lifecycle only, after binding disable and explicit writer quiescence."""
    if not quiesced:
        raise ContractError('Source drain requires disabled bindings and writer quiescence')
    base = _base(repo)
    with source_barrier(repo, exclusive=True), private_lock(base / 'omp-transport.lock'):
        if not check:
            namespaces = {}
            for _, reference in _omp_index(repo):
                root, _, shared_base, directory, identity = turn_sources.private_namespace(
                    repo, 'omp', reference['session'], reference['lifecycle'])
                namespaces[str(directory)] = (root, shared_base, directory, identity)
            with ExitStack() as completions:
                for key in sorted(namespaces):
                    root, shared_base, directory, identity = namespaces[key]
                    completions.enter_context(turn_sources.locked(directory))
                    if turn_sources.inventory(directory, identity)['complete']:
                        turn_sources.cleanup(shared_base, directory, identity, root)
        binding, digest = _drain_inventory(repo)
        result = {'capability': CAPABILITY, 'pending': binding['pending'], 'faults': binding['faults'],
                  'inventory_sha256': digest, 'drainable': not binding['pending'] and not binding['faults'],
                  'marker_cleared': False, 'canonical_writes_performed': False}
        if check or not result['drainable']:
            return result
        proof = {'schema': 'agent-memory/omp-drain-v1', 'repository': binding['repository'],
                 'private_state': str(base), 'inventory_sha256': digest, 'writers_quiesced': True}
        directory = base / 'omp-drains'
        turn_sources.private_mkdir(directory)
        target = directory / (digest + '.json')
        try:
            turn_sources.exclusive(target, proof, 4096)
        except FileExistsError:
            if load(turn_sources.read_bytes(target, 4096)) != proof:
                raise ContractError('Conflicting retained source drain proof')
        if _marker(base):
            (base / 'turn-sources-compat-v1.json').unlink()
            turn_sources.sync(base)
            if os.path.lexists(base / 'turn-sources-compat-v1.json'):
                raise ContractError('Source marker removal could not be verified')
        result['marker_cleared'] = True
        return result


def _enrolled_repositories(installation):
    from . import repository_config
    result = []
    enrolled = set(_registry()['repositories'])
    for repo in sorted(source_capability_repositories(installation)):
        if not repo.is_dir():
            raise ContractError('Enrolled native bank is unavailable; compatibility remains unknown')
        if not repository_config.bank_path(repo).is_dir():
            if str(repo) in enrolled:
                raise ContractError('Enrolled native bank has disappeared; compatibility remains unknown')
            continue
        result.append(repo)
    return result


def native_activation(plan, check=False):
    """Refresh reviewed layout quiescence for the newly enabled native route."""
    if not plan.get('omp_binding'):
        return []
    old = plan.get('old') or {}
    fresh_route = not old.get('omp_binding')
    if not fresh_route and old.get('source', {}).get('artifact_digest') == plan['source']['artifact_digest']:
        return []
    from . import recovery_frontier, scope_binding
    route = plan['omp_binding']['extension_target']
    values = []
    proof_paths = plan.get('omp_layout_quiescence') or []
    if isinstance(proof_paths, str):
        proof_paths = [proof_paths]
    for proof_path in [*proof_paths, *(plan.get('layout_quiescence') or [])]:
        path = Path(proof_path)
        trust_paths.record(path)
        values.append(load(turn_sources.read_bytes(path, 65536)))
    results = []
    for repo in _enrolled_repositories(old):
        with (nullcontext() if check else source_barrier(repo, exclusive=True)):
            context = scope_binding.private_enrollment_context(repo)
            if context.held:
                matching = [item for item in values if isinstance(item, dict) and isinstance(item.get('attestation'), dict)
                            and item['attestation'].get('repo_root') == str(repo)]
                if len(matching) != 1:
                    raise ContractError('Native activation under layout hold requires independently reviewed --omp-layout-quiescence covering the new route')
                results.append(recovery_frontier.validate_quiescence(
                    repo, matching[0], runtime_artifact_digest=plan['source']['artifact_digest'],
                    required_routes=(route,), publish=not check))
    return results


def native_transition(plan, check=False):
    """Disable only the saved executable binding, journal it, then prove drain."""
    from . import installation as install
    old = plan.get('old')
    selected = plan.get('omp_transition')
    if (not old or not isinstance(selected, dict)
            or selected != {'writers_quiesced': True}):
        raise ContractError('Native deactivation requires saved ownership and explicit writer quiescence')
    from .omp_installation import validate_receipt
    validate_receipt(old)
    repositories = _enrolled_repositories(old)
    if not old.get('omp_binding'):
        # A previously removed binding can leave safely retained snapshots and a
        # separately owned capable reader, without any executable path to guess.
        with ExitStack() as locks:
            for repo in repositories:
                locks.enter_context(source_barrier(repo, exclusive=True))
            results = [drain_source_capabilities(repo, quiesced=True, check=check) for repo in repositories]
        if not check and any(not result['drainable'] for result in results):
            raise ContractError('Historical native sources remain pending; preserve their capable reader')
        return {'binding_absent_in_saved_ownership': True, 'drains': results, 'canonical_writes_performed': False}
    target = Path(old['omp_binding']['extension_target'])
    owned = next(t for t in old['targets'] if t['path'] == str(target))
    state = Path(plan['state'])
    journal_path = state / 'omp-transition.json'
    identity = install.fingerprint(target)
    backup = target.parent / ('.agent-memory-disabled-' + old['installation_id'] + '-' + str(uuid.uuid4()) + '.ts.disabled')
    journal = {'schema': 'agent-memory/omp-native-transition-v1', 'installation_id': old['installation_id'],
               'source': old['source'], 'extension': str(target), 'identity': owned['identity'],
               'disabled_path': str(backup), 'status': 'prepared'}
    if journal_path.exists():
        retained = install.load(journal_path)
        if retained.get('status') in {'prepared', 'disabled'}:
            backup = Path(retained.get('disabled_path', ''))
            if backup.parent != target.parent or not re.fullmatch(r'\.agent-memory-disabled-' + re.escape(old['installation_id']) + r'-[0-9a-f-]{36}\.ts\.disabled', backup.name):
                raise ContractError('Foreign native disabled-binding journal path')
            expected = {**journal, 'disabled_path': str(backup), 'status': retained['status']}
            if retained != expected:
                raise ContractError('Native disabled-binding journal differs from saved ownership')
            journal = retained
    if identity != owned['identity'] and not (identity is None and install.fingerprint(backup) == owned['identity']):
        raise ContractError('Native binding is modified/unowned; preserve it and refuse compatibility drain')
    results = []
    with ExitStack() as locks:
        for repo in repositories:
            if not repo.is_dir():
                raise ContractError('Enrolled native bank is unavailable; compatibility remains unknown')
            locks.enter_context(source_barrier(repo, exclusive=True))
        if check:
            for repo in repositories:
                results.append(drain_source_capabilities(repo, quiesced=True, check=True))
            return {'binding_disable_planned': True, 'drains': results,
                    'installation_writes_performed': False, 'canonical_writes_performed': False}
        install.replace_record(journal_path, journal)
        if identity is not None:
            if os.path.lexists(backup):
                raise ContractError('Native disabled-binding backup collision; preserve both paths')
            install.native_rename(target, backup)
            install.sync_dir(target.parent)
        if install.fingerprint(backup) != owned['identity'] or os.path.lexists(target):
            raise ContractError('Native executable disable readback failed')
        journal['status'] = 'disabled'; install.replace_record(journal_path, journal)
        for repo in repositories:
            results.append(drain_source_capabilities(repo, quiesced=True))
        if any(not result['drainable'] for result in results):
            raise ContractError('Native sources remain pending; owned extension stays disabled, evidence and transition journal retained')
        actions = []
        for action in plan['actions']:
            if action['path'] == str(target):
                if action['after'] is None:
                    continue
                action = {**action, 'before': None, 'changed': True}
            actions.append(action)
        plan['actions'] = actions
        plan['omp_disabled_backup'] = {'path': str(backup), 'original_path': str(target),
                                      'identity': owned['identity'], 'available': True,
                                      'kind': 'native-binding-disabled'}
    return {'binding_disabled': True, 'drains': results, 'canonical_writes_performed': False}


def finish_native_transition(plan):
    from . import installation as install
    path = Path(plan['state']) / 'omp-transition.json'
    if plan.get('omp_disabled_backup') and path.exists():
        value = install.load(path)
        if value.get('schema') != 'agent-memory/omp-native-transition-v1' or value.get('status') not in {'disabled', 'completed'}:
            raise ContractError('Native transition state changed before lifecycle completion')
        if value['status'] == 'completed':
            return
        value['status'] = 'completed'
        install.replace_record(path, value)


def recover_native_binding(state):
    """Restore a disabled extension only with its actual aware runtime selected."""
    from . import installation as install, private_state
    path = Path(state) / 'omp-transition.json'
    if not path.exists():
        return
    journal = install.load(path)
    fields = {'schema', 'installation_id', 'source', 'extension', 'identity', 'disabled_path', 'status'}
    if (not isinstance(journal, dict) or set(journal) != fields
            or journal['schema'] != 'agent-memory/omp-native-transition-v1'
            or journal['status'] not in {'prepared', 'disabled', 'completed'}):
        raise ContractError('Unverifiable native lifecycle recovery journal')
    if journal['status'] == 'completed':
        return
    old = install.read_receipt(state)
    if old is None:
        historical = install.load(Path(state) / 'receipt.json')
        binding = historical.get('omp_binding')
        if (historical.get('uninstalled') is not True or historical.get('verified') is not True
                or historical.get('installation_id') != journal['installation_id']
                or historical.get('source') != journal['source'] or not isinstance(binding, dict)):
            raise ContractError('Native uninstall recovery has no exact committed receipt')
        from .omp_installation import binding as expected_binding
        if binding != expected_binding(binding['agent_dir'], no_guidance=historical.get('no_guidance', False)):
            raise ContractError('Native uninstall binding no longer matches its saved root')
        target = Path(binding['extension_target'])
        backup = Path(journal['disabled_path'])
        if (str(target) != journal['extension'] or backup.parent != target.parent
                or not re.fullmatch(r'\.agent-memory-disabled-' + re.escape(journal['installation_id']) + r'-[0-9a-f-]{36}\.ts\.disabled', backup.name)
                or install.fingerprint(target) is not None or install.fingerprint(backup) != journal['identity']):
            raise ContractError('Committed native uninstall differs from its retained disabled executable')
        journal['status'] = 'completed'; install.replace_record(path, journal)
        return
    if not old or not old.get('omp_binding') or old['installation_id'] != journal['installation_id']:
        raise ContractError('Native recovery requires its exact saved installation')
    target = Path(old['omp_binding']['extension_target'])
    backup = Path(journal['disabled_path'])
    if (str(target) != journal['extension'] or backup.parent != target.parent
            or not re.fullmatch(r'\.agent-memory-disabled-' + re.escape(old['installation_id']) + r'-[0-9a-f-]{36}\.ts\.disabled', backup.name)):
        raise ContractError('Foreign native recovery target')
    if old['source'] != journal['source']:
        # A committed update owns the new executable; never overwrite it with old code.
        install.verify_targets([t for t in old['targets'] if t['path'] == str(target)], source=old['source'])
    else:
        if CAPABILITY not in old['source']['compatibility'].get('private_state_capabilities', []):
            raise ContractError('Native recovery selected an omp-unaware runtime')
        with private_state.activation_guard(old, old['source'], 'recover'):
            if install.fingerprint(target) is None:
                if install.fingerprint(backup) != journal['identity']:
                    raise ContractError('Disabled native executable no longer matches ownership')
                install.native_rename(backup, target)
                install.sync_dir(target.parent)
            if install.fingerprint(target) != journal['identity']:
                raise ContractError('Native recovery executable readback differs')
    journal['status'] = 'completed'; install.replace_record(path, journal)


def native_recovery_expected(state, target):
    """One exact restored owned wrapper may differ from a None staging before."""
    from . import installation as install
    journal_path = Path(state) / 'omp-transition.json'
    if not journal_path.exists():
        return None
    journal = install.load(journal_path)
    if (not isinstance(journal, dict) or journal.get('schema') != 'agent-memory/omp-native-transition-v1'
            or journal.get('status') != 'completed' or journal.get('extension') != str(target)):
        return None
    receipt = install.read_receipt(state)
    if (receipt is None or not receipt.get('omp_binding') or receipt['installation_id'] != journal.get('installation_id')
            or receipt['source'] != journal.get('source') or receipt['omp_binding']['extension_target'] != str(target)):
        return None
    owned = [item for item in receipt['targets'] if item['path'] == str(target)]
    if len(owned) != 1 or owned[0]['kind'] != 'wrapper' or owned[0]['identity'] != journal.get('identity'):
        raise ContractError('Native restored wrapper ownership differs from its recovery journal')
    install.verify_targets(owned, source=receipt['source'])
    return owned[0]['identity']


def main(argv=None):
    from . import omp_transport, turn_check
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True)
    parser.add_argument('--observer', action='store_true')
    parser.add_argument('--artifact')
    parser.add_argument('--omp-agent-dir')
    parser.add_argument('--stdin', action='store_true')
    parser.add_argument('--list-pending', action='store_true')
    parser.add_argument('--offset', type=int, default=0)
    parser.add_argument('--source')
    parser.add_argument('--session')
    parser.add_argument('--lifecycle')
    parser.add_argument('--quiet', action='store_true')
    parser.add_argument('--refresh', action='store_true')
    args = parser.parse_args(argv)
    try:
        if args.observer:
            from .identity_git import git
            cwd = Path(args.repo).resolve(strict=True)
            root = git(cwd, 'rev-parse', '--show-toplevel', optional=True)
            if root is None:
                if any(os.path.lexists(parent / '.git') for parent in (cwd, *cwd.parents)):
                    raise ContractError('Native repository is unavailable; preserve pending sources')
                return turn_check.emit({'verified': True, 'status': 'inactive-nonrepository',
                                        'canonical_writes_performed': False}, False)
            args.repo = str(Path(root).resolve(strict=True))
            from . import repository_config
            candidate = repository_config.bank_path(Path(args.repo).resolve())
            if not candidate.is_dir():
                return turn_check.emit({'verified': True, 'status': 'inactive-no-bank',
                                        'canonical_writes_performed': False}, False)
            from .omp_installation import observer_binding
            observer_binding(args.artifact, args.omp_agent_dir)
        modes = sum((args.observer, args.list_pending, bool(args.source), args.stdin and not args.observer))
        if modes != 1 or (args.observer and (args.quiet or args.refresh)):
            raise ContractError('Select exactly one native observer, source, pending or assessment operation')
        if args.list_pending:
            if args.refresh or args.stdin:
                raise ContractError('Pending inventory cannot acknowledge or refresh')
            result = omp_transport.list_pending(args.repo, args.offset)
        elif args.source:
            result = omp_transport.source(args.repo, args.session, args.lifecycle, args.source)
        else:
            maximum = omp_transport.MAX_PAYLOAD if args.observer else 65536
            raw = sys.stdin.buffer.read(maximum + 1)
            if len(raw) > maximum:
                raise ContractError('Native boundary input exceeds bound')
            value = load(raw)
            if args.observer:
                result = omp_transport.observe(args.repo, value)
            elif args.refresh:
                fields = {'session', 'lifecycle', 'observation_id', 'source_sha256', 'outcome', 'assessment'}
                if not isinstance(value, dict) or set(value) not in (fields, fields | {'publication_request'}):
                    raise ContractError('Native compact assessment accepts exactly one original observation, never an array')
                result = turn_check.check(args.repo, value, omp_transport.complete, 'omp')
            else:
                result = omp_transport.complete(args.repo, value)
        return turn_check.emit(result, args.quiet)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError):
        failure = {'verified': False, 'status': 'pending', 'canonical_writes_performed': False,
                   'reason': 'Native source work remains pending; inspect owned installation, binding and retained sources. No completion is inferred.'}
        if args.quiet:
            print('am: ' + failure['reason'], file=sys.stderr)
        else:
            import json
            print(json.dumps(failure))
        return 1
