"""Private native evidence transport. Shared turn_sources owns all completion."""
from contextlib import ExitStack
import hashlib
import os
from pathlib import Path
import re
import uuid

from . import omp_control, omp_projection, scope_binding, trust_paths, turn_sources
from .developer_identity import load
from .frontmatter import ContractError

MAX_PAYLOAD = 16 * 1024 * 1024
MAX_CAPTURES = 16384
TRIGGERS = {'human', 'human-with-own-guidance', 'own-hidden-pure', 'own-hidden-combined',
            'own-hidden-ambiguous', 'foreign-hidden', 'resume'}
GAPS = {'unsafe-representation', 'missing-stored-source', 'ambiguous-lifecycle', 'missing-payload',
        'oversized-source', 'cursor-discontinuity', 'late-substantive-source'}
NONLEARNING = {'model_change', 'thinking_level_change', 'label', 'session_info', 'credential_pin',
               'custom', 'compaction', 'branch_summary', 'model_usage', 'service_tier_change',
               'reset_boundary', 'ttsr_injection', 'title_change', 'mode_change', 'session_init'}


def digest(value):
    return hashlib.sha256(turn_sources.encoded(value)).hexdigest()


def canonical_uuid(value):
    if not isinstance(value, str) or str(uuid.UUID(value)) != value:
        raise ContractError('Use a canonical native lifecycle UUID')
    return value


def exact(value, fields):
    if not isinstance(value, dict) or set(value) != set(fields):
        raise ContractError('Unexpected native observer fields')
    return value


def admission_metadata(value, session):
    exact(value, {'schema', 'session', 'lifecycle', 'capture_id', 'event_ordinal', 'role', 'attribution', 'tool_call_ids'})
    if value['schema'] != 'agent-memory/omp-admission-v1' or value['session'] != session:
        raise ContractError('Foreign native admission metadata')
    canonical_uuid(value['lifecycle']); canonical_uuid(value['capture_id'])
    if type(value['event_ordinal']) is not int or value['event_ordinal'] < 0:
        raise ContractError('Invalid retained native event ordinal')
    turn_sources.bounded(value['role']); turn_sources.bounded(value['attribution'])
    calls = value['tool_call_ids']
    if not isinstance(calls, list) or len(calls) > 1024:
        raise ContractError('Invalid native tool invocation metadata')
    for call in calls:
        turn_sources.bounded(call)
    if len(set(calls)) != len(calls):
        raise ContractError('Duplicate native tool invocation identity')
    return value


def read(path, maximum=MAX_PAYLOAD):
    return load(turn_sources.read_bytes(path, maximum))


def retain(path, value, maximum=MAX_PAYLOAD):
    turn_sources.private_mkdir(path.parent)
    try:
        turn_sources.exclusive(path, value, maximum)
    except FileExistsError:
        if read(path, maximum) != value:
            raise ContractError('Immutable native evidence changed; preserve both sources')
    return digest(value)


def replace_cursor(path, value):
    if len(turn_sources.encoded(value)) > 1024 * 1024:
        raise ContractError('Native cursor capacity reached')
    temporary = path.parent / ('.cursor-' + str(uuid.uuid4()))
    turn_sources.exclusive(temporary, value, 1024 * 1024)
    try:
        os.replace(temporary, path)
        turn_sources.sync(path.parent)
        if read(path, 1024 * 1024) != value:
            raise ContractError('Native cursor durable readback failed')
    finally:
        if temporary.exists():
            temporary.unlink()


def session_dir(repo, session, *, enrollment_context=None):
    from .omp_boundary import _base
    turn_sources.bounded(session)
    return _base(repo, enrollment_context=enrollment_context) / 'omp-source-v1' / hashlib.sha256(session.encode()).hexdigest()


def directories(path, pattern, maximum=65536):
    if not os.path.lexists(path):
        return []
    trust_paths.state(path)
    values = []
    with os.scandir(path) as scan:
        for i, entry in enumerate(scan):
            if i >= maximum or not re.fullmatch(pattern, entry.name):
                raise ContractError('Unknown or oversized native source inventory')
            child = Path(entry.path)
            trust_paths.state(child)
            values.append(child)
    return sorted(values)


def cursor_value(value, session, lifecycle):
    exact(value, {'schema', 'session', 'lifecycle', 'origin', 'last_native_entry', 'event_ordinal',
                  'control_interval', 'tool_lifecycles'})
    if value['schema'] != 'agent-memory/omp-cursor-v1' or value['session'] != session or value['lifecycle'] != lifecycle:
        raise ContractError('Foreign native cursor')
    canonical_uuid(lifecycle)
    origin = exact(value['origin'], {'kind', 'human_root', 'native_trigger'})
    canonical_uuid(origin['human_root'])
    if origin['native_trigger'] not in TRIGGERS or origin['kind'] not in {'genuine-user', 'foreign-continuation'}:
        raise ContractError('Unknown native origin')
    if (origin['kind'] == 'genuine-user') != (origin['native_trigger'] in {'human', 'human-with-own-guidance'}):
        raise ContractError('Native origin attribution differs')
    if type(value['event_ordinal']) is not int or value['event_ordinal'] < 0:
        raise ContractError('Invalid native ordinal')
    if not isinstance(value['tool_lifecycles'], dict) or len(value['tool_lifecycles']) > 1024:
        raise ContractError('Native active tool binding capacity reached')
    for key, owner in value['tool_lifecycles'].items():
        turn_sources.bounded(key); canonical_uuid(owner)
    if value['last_native_entry'] is not None:
        turn_sources.bounded(value['last_native_entry'])
    control = value['control_interval']
    fields = {'capture_order', 'metadata_matches', 'tool_metadata_matches', 'metadata_predecessors',
              'pure_entries', 'pure_mode', 'pure_tool_ids', 'native_trigger', 'generated_guidance',
              'human_entry', 'absent_native_entries'}
    if control is not None:
        if not isinstance(control, dict) or not set(control).issubset(fields):
            raise ContractError('Unknown native control interval fields')
        for name in ('capture_order', 'pure_tool_ids', 'absent_native_entries'):
            if name in control:
                values = control[name]
                maximum = 1024 if name == 'pure_tool_ids' else MAX_CAPTURES
                if not isinstance(values, list) or len(values) > maximum or len(set(values)) != len(values):
                    raise ContractError('Invalid native control identity inventory')
                for native in values:
                    turn_sources.bounded(native)
        for name in ('metadata_matches', 'tool_metadata_matches', 'metadata_predecessors'):
            if name in control:
                if not isinstance(control[name], dict) or len(control[name]) > MAX_CAPTURES:
                    raise ContractError('Invalid native metadata mapping inventory')
                for identifier, native in control[name].items():
                    canonical_uuid(identifier)
                    if native is not None:
                        turn_sources.bounded(native)
        if 'pure_entries' in control:
            if not isinstance(control['pure_entries'], dict) or len(control['pure_entries']) > MAX_CAPTURES:
                raise ContractError('Invalid native own-control source inventory')
            for native, item in control['pure_entries'].items():
                turn_sources.bounded(native)
                exact(item, {'snapshot_ref', 'snapshot_sha256'})
                _snapshot_path(Path('.'), item['snapshot_ref'])
                if not isinstance(item['snapshot_sha256'], str) or not re.fullmatch(r'[0-9a-f]{64}', item['snapshot_sha256']):
                    raise ContractError('Invalid native own-control snapshot digest')
        if 'pure_mode' in control and type(control['pure_mode']) is not bool:
            raise ContractError('Invalid native control classification')
        if 'native_trigger' in control and control['native_trigger'] not in TRIGGERS:
            raise ContractError('Unknown native control trigger')
        if 'human_entry' in control:
            turn_sources.bounded(control['human_entry'])
        if 'generated_guidance' in control:
            turn_sources.bounded(control['generated_guidance'], 8192)
    return value


def validate_root_wakes(path, session=None):
    trust_paths.state(path)
    entries = []; native_users = set()
    with os.scandir(path) as scan:
        for index, entry in enumerate(scan):
            if index >= 65536:
                raise ContractError('Native root wake inventory exceeds bound')
            entries.append(entry)
    entries.sort(key=lambda entry: entry.name)
    for entry in entries:
        if not entry.name.endswith('.json'):
            raise ContractError('Unknown native root wake record')
        root = canonical_uuid(entry.name[:-5])
        value = read(Path(entry.path), 4096)
        exact(value, {'schema', 'session', 'human_root', 'native_user_entry', 'request_id', 'status'})
        if (value['schema'] != 'agent-memory/omp-root-wake-v1' or value['human_root'] != root
                or value['status'] != 'requested' or (session is not None and value['session'] != session)
                or hashlib.sha256(value['session'].encode()).hexdigest() != path.parent.name):
            raise ContractError('Foreign native root wake')
        turn_sources.bounded(value['native_user_entry'])
        canonical_uuid(value['request_id'])
        cursor_value(read(path.parent / root / 'cursor.json'), value['session'], root)
        native_users.add(value['native_user_entry'])
    return native_users


def segments(repo, session, *, enrollment_context=None):
    directory = session_dir(repo, session, enrollment_context=enrollment_context)
    result = []
    if not directory.exists():
        return result
    trust_paths.state(directory)
    with os.scandir(directory) as scan:
        for i, entry in enumerate(scan):
            if i >= 65536:
                raise ContractError('Native session inventory exceeds bound')
            if entry.name == 'root-wakes':
                continue  # Producer source capture is independent of wake-state faults.
            canonical_uuid(entry.name)
            child = Path(entry.path); trust_paths.state(child)
            cursor = cursor_value(read(child / 'cursor.json', 1024 * 1024), session, entry.name)
            result.append((child, cursor))
    return sorted(result, key=lambda item: (item[1]['event_ordinal'], item[0].name))


def new_segment(repo, session, lifecycle, root, trigger, ordinal, *, enrollment_context=None):
    directory = session_dir(repo, session, enrollment_context=enrollment_context) / lifecycle
    if root != lifecycle:
        root_directory = session_dir(repo, session, enrollment_context=enrollment_context) / root
        root_cursor = cursor_value(read(root_directory / 'cursor.json'), session, root)
        if not source_bound_origin(root_directory, root_cursor):
            raise ContractError('Cannot extend an unbound native root family')
    turn_sources.private_mkdir(directory)
    cursor = {'schema': 'agent-memory/omp-cursor-v1', 'session': session, 'lifecycle': lifecycle,
              'origin': {'kind': 'genuine-user' if trigger in {'human', 'human-with-own-guidance'} else 'foreign-continuation',
                         'human_root': root, 'native_trigger': trigger},
              'last_native_entry': None, 'event_ordinal': ordinal, 'control_interval': None,
              'tool_lifecycles': {}}
    if not (directory / 'cursor.json').exists():
        retain(directory / 'cursor.json', cursor, 1024 * 1024)
    else:
        cursor = cursor_value(read(directory / 'cursor.json'), session, lifecycle)
        if not source_bound_origin(directory, cursor):
            raise ContractError('Cannot reuse an unbound native root family')
    return directory, cursor


def source_bound_origin(directory, cursor):
    """Historical notification-only cursors never acquire source authority."""
    root = cursor['origin']['human_root']
    if root != cursor['lifecycle']:
        root_directory = directory.parent / root
        try:
            root_cursor = cursor_value(read(root_directory / 'cursor.json'), cursor['session'], root)
        except FileNotFoundError:
            return False
        if root_cursor['origin']['human_root'] != root:
            return False
        return source_bound_origin(root_directory, root_cursor)
    if cursor['origin']['kind'] != 'genuine-user':
        return True
    native = (cursor['control_interval'] or {}).get('human_entry')
    if not native:
        return False
    binding = read(directory.parents[2] / 'omp-bank-binding-v1.json', 4096)
    repository = binding.get('repository')
    if not isinstance(repository, str) or not Path(repository).is_absolute():
        return False
    expected = str(uuid.uuid5(uuid.NAMESPACE_URL, str(Path(repository).resolve()) + ':omp:' + cursor['session'] + ':' + native))
    if cursor['lifecycle'] != expected:
        return False
    target = directory / 'captures' / (hashlib.sha256(native.encode()).hexdigest() + '.json')
    if os.path.lexists(target):
        captured = read(target, 4096)
        snapshot = read(_snapshot_path(directory, captured['snapshot_ref']))
        return (captured.get('schema') == 'agent-memory/omp-capture-v1'
                and captured.get('session') == cursor['session'] and captured.get('lifecycle') == cursor['lifecycle']
                and captured['native_entry_id'] == native and snapshot.get('native_entry_id') == native
                and digest(snapshot) == captured['snapshot_sha256']
                and snapshot.get('role') == 'user' and snapshot.get('attribution') == 'user')
    # A checked stored user can itself be unsafe/unavailable. Its exact-ID gap
    # preserves that real origin without admitting its payload or a metadata root.
    for path in _files(directory, 'gaps', r'[0-9a-f-]{36}\.json'):
        value = read(path, 4096)
        if (value.get('schema') == 'agent-memory/omp-gap-v1' and value.get('session') == cursor['session']
                and value.get('lifecycle') == cursor['lifecycle'] and value.get('native_entry_id') == native
                and value.get('reason') in {'unsafe-representation', 'oversized-source', 'missing-payload'}):
            return True
    return False


def active_segment(current, branch_ids=None):
    invalid = {cursor['origin']['human_root'] for directory, cursor in current
               if not source_bound_origin(directory, cursor)}
    current = [item for item in current if item[1]['origin']['human_root'] not in invalid]
    if not current:
        return None
    humans = [item for item in current if item[1]['origin']['kind'] == 'genuine-user']
    if branch_ids is not None:
        humans = [item for item in humans if (item[1]['control_interval'] or {}).get('human_entry') in branch_ids]
        if not humans:
            return next((item for item in reversed(current)
                         if item[1]['origin']['kind'] != 'genuine-user'), None)
    if not humans:
        return current[-1]
    root = max(humans, key=lambda item: item[1]['event_ordinal'])[1]['origin']['human_root']
    return max((item for item in current if item[1]['origin']['human_root'] == root),
               key=lambda item: item[1]['event_ordinal'])


def writable_successor(repo, session, owner, current, ordinal, *, enrollment_context=None):
    directory, cursor = owner
    if not (directory / 'segment.json').exists():
        return owner
    root = cursor['origin']['human_root']
    existing = [item for item in current if item[1]['origin']['human_root'] == root
                and source_bound_origin(*item) and not (item[0] / 'segment.json').exists()]
    if existing:
        return existing[-1]
    successor = new_segment(repo, session, str(uuid.uuid4()), root, 'foreign-hidden',
                            max([item[1]['event_ordinal'] for item in current] + [ordinal]) + 1,
                            enrollment_context=enrollment_context)
    current.append(successor)
    return successor


def _files(directory, child, pattern):
    path = directory / child
    if not path.exists():
        return []
    trust_paths.state(path)
    result = []
    with os.scandir(path) as scan:
        for i, entry in enumerate(scan):
            if i >= MAX_CAPTURES or not re.fullmatch(pattern, entry.name):
                raise ContractError('Native capture inventory exceeds bound or has unknown entries')
            result.append(Path(entry.path))
    return sorted(result)


def native_disposition_ids(repo, session):
    """Exact safely retained/qualified IDs; neutral hints are never source evidence."""
    result = set()
    for directory, cursor in segments(repo, session):
        if not source_bound_origin(directory, cursor):
            continue
        for path in _files(directory, 'captures', r'[0-9a-f]{64}\.json'):
            captured = read(path, 4096)
            snapshot = read(_snapshot_path(directory, captured['snapshot_ref']))
            native = captured['native_entry_id']
            if (snapshot.get('native_entry_id') != native or digest(snapshot) != captured['snapshot_sha256']):
                raise ContractError('Native disposition snapshot identity changed')
            result.add(native)
        for native, captured in (cursor['control_interval'] or {}).get('pure_entries', {}).items():
            if digest(read(_snapshot_path(directory, captured['snapshot_ref']))) != captured['snapshot_sha256']:
                raise ContractError('Native control disposition changed')
            result.add(native)
        for path in _files(directory, 'gaps', r'[0-9a-f-]{36}\.json'):
            value = read(path, 4096)
            native = value.get('native_entry_id')
            if (isinstance(native, str) and not native.startswith(('event:', 'tool:'))
                    and value.get('session') == session and value.get('lifecycle') == cursor['lifecycle']
                    and value.get('reason') in GAPS):
                result.add(native)
    return result


def _enroll(directory, cursor, reference, snapshot, kind, native):
    record = {'schema': 'agent-memory/omp-enrollment-v1', 'session': cursor['session'],
              'lifecycle': cursor['lifecycle'], 'source_reference': reference,
              'source_sha256': digest(snapshot),
              'snapshot_ref': 'segment.json' if kind == 'segment' else 'snapshots/' + hashlib.sha256(native.encode()).hexdigest() + '.json',
              'kind': kind, 'native_source_id': native}
    retain(directory / 'enrollments' / (digest(reference) + '.json'), record, 4096)
    if kind == 'admission':
        content = snapshot.get('content')
        calls = [block['id'] for block in content if isinstance(block, dict) and block.get('type') == 'toolCall'] if isinstance(content, list) else []
        retain(directory / 'admission.json',
               {'schema': 'agent-memory/omp-admission-v1', 'session': cursor['session'], 'lifecycle': cursor['lifecycle'],
                'capture_id': reference.rsplit(':', 1)[1], 'event_ordinal': cursor['event_ordinal'],
                'role': snapshot.get('role', 'custom'), 'attribution': snapshot.get('attribution', 'unknown'),
                'tool_call_ids': calls}, 4096)


def gap(directory, cursor, native_id, reason):
    if reason not in GAPS:
        raise ContractError('Unknown coverage gap')
    gap_id = str(uuid.uuid5(uuid.UUID(cursor['lifecycle']), str(native_id) + ':' + reason))
    value = {'schema': 'agent-memory/omp-gap-v1', 'session': cursor['session'],
             'lifecycle': cursor['lifecycle'], 'gap_id': gap_id, 'native_entry_id': native_id, 'reason': reason}
    retain(directory / 'gaps' / (gap_id + '.json'), value, 4096)
    return value


def _retained_gap(current, identities):
    """Check both live gaps and immutable manifests, including this sweep."""
    for directory, cursor in current:
        for path in _files(directory, 'gaps', r'[0-9a-f-]{36}\.json'):
            if read(path, 4096)['native_entry_id'] in identities:
                return True
        if (directory / 'segment.json').exists() and any(
                item['native_entry_id'] in identities for item in manifest_value(directory, cursor)['gaps']):
            return True
    return False


def capture(directory, cursor, entry, *, mapping='verified', invocation=None, witness=None, persisted=False):
    native = turn_sources.bounded(entry.get('id'))
    name = hashlib.sha256(native.encode()).hexdigest()
    snapshot_ref = 'snapshots/' + name + '.json'
    path = directory / snapshot_ref
    if os.path.lexists(path):
        snapshot = read(path)
        if not omp_projection._valid_instruction_context(snapshot):
            raise ContractError('Invalid retained native instruction context')
        if 'instruction_context' in snapshot:
            message = entry.get('message')
            if (entry.get('type') != 'message' or not isinstance(message, dict)
                    or message.get('role') != 'toolResult'
                    or snapshot.get('native_entry_id') != native
                    or snapshot.get('native_parent_id') != entry.get('parentId')
                    or snapshot.get('toolCallId') != message.get('toolCallId')
                    or snapshot.get('toolName') != message.get('toolName')
                    or snapshot.get('isError') is not message.get('isError')
                    or ('attribution' in snapshot) != ('attribution' in message)
                    or snapshot.get('attribution') != message.get('attribution')):
                raise ContractError('Retained instruction context differs from native identity')
        elif persisted:
            # Native persistence may truncate otherwise safe bodies. Reuse the
            # already screened immutable snapshot by exact structural identity,
            # never compare/replace it with a persisted rendering.
            message = entry.get('message', {}) if entry.get('type') == 'message' else entry
            if (snapshot.get('carrier') != entry.get('type') or snapshot.get('native_entry_id') != native
                    or snapshot.get('native_parent_id') != entry.get('parentId')
                    or (entry.get('type') == 'message' and snapshot.get('role') != message.get('role'))
                    or any(snapshot.get(key) != message.get(key) for key in
                           ('toolCallId', 'toolName', 'isError', 'attribution', 'customType')
                           if key in snapshot or key in message)):
                raise ContractError('Retained native snapshot identity differs')
        else:
            original = omp_projection.project(entry, persisted=persisted)
            if original.get('snapshot') != snapshot:
                raise ContractError('Retained native snapshot differs from its projection')
        projected = {'snapshot': snapshot, 'exclusions': list(omp_projection.EXCLUSIONS)}
    else:
        verified = witness._comparison if witness is not None else None
        projected = (omp_projection._project_owned_manual(entry, invocation, witness, verified)
                     if verified is not None and invocation is not None else None)
        if projected is not None and witness._verify() != verified:
            projected = None
        if projected is None:
            projected = omp_projection.project(entry, persisted=persisted)
        if 'gap' in projected:
            if not (directory / 'segment.json').exists():
                gap(directory, cursor, native, projected['gap'])
            return None
        snapshot = projected['snapshot']
    source_hash = retain(path, snapshot)
    record = {'schema': 'agent-memory/omp-capture-v1', 'session': cursor['session'],
              'lifecycle': cursor['lifecycle'], 'capture_id': str(uuid.uuid5(uuid.UUID(cursor['lifecycle']), native)),
              'native_entry_id': native, 'native_parent_id': entry.get('parentId'),
              'snapshot_ref': snapshot_ref, 'snapshot_sha256': source_hash,
              'representation': 'native-stored-learning-v1', 'mapping': mapping,
              'exclusions': projected['exclusions']}
    retain(directory / 'captures' / (name + '.json'), record, 4096)
    # Only the observer ingest path can resolve a previously sealed gap.
    if (directory / 'segment.json').exists():
        manifest_hash = digest(read(directory / 'segment.json'))
        for path in _files(directory, 'gaps', r'[0-9a-f-]{36}\.json'):
            old = read(path, 4096)
            if old['native_entry_id'] == native and old['reason'] in {'missing-stored-source', 'missing-payload', 'cursor-discontinuity'}:
                resolution = {'schema': 'agent-memory/omp-gap-resolution-v1', 'manifest_sha256': manifest_hash,
                              'gap_id': old['gap_id'], 'recovered_ref': snapshot_ref,
                              'recovered_sha256': source_hash, 'reason': 'recovered-safe-native-source'}
                retain(directory / 'resolutions' / (old['gap_id'] + '.json'), resolution, 4096)
    return record


def matched_metadata(cursor):
    control = cursor['control_interval'] or {}
    return {**control.get('metadata_matches', {}), **control.get('tool_metadata_matches', {})}




def enroll_sealed(directory, cursor, manifest):
    if not source_bound_origin(directory, cursor):
        raise ContractError('Cannot enroll an unbound native root family')
    reference = manifest['admission_ref']
    if reference is not None:
        matching = [read(path, 4096) for path in _files(directory, 'captures', r'[0-9a-f]{64}\.json')]
        matching = [item for item in matching if item['snapshot_ref'] == reference]
        if len(matching) != 1:
            raise ContractError('Sealed admission no longer names one retained capture')
        admission = matching[0]
        _enroll(directory, cursor, 'omp:capture:' + admission['capture_id'],
                read(_snapshot_path(directory, reference)), 'admission', admission['native_entry_id'])
    _enroll(directory, cursor, 'omp:seal:' + manifest['seal_id'], manifest, 'segment', manifest['seal_id'])


def seal(directory, cursor):
    if not source_bound_origin(directory, cursor):
        raise ContractError('Legacy notification-only native origin cannot be sealed')
    if (directory / 'segment.json').exists():
        manifest = manifest_value(directory, cursor)
        enroll_sealed(directory, cursor, manifest)
        return manifest
    captures = [read(p, 4096) for p in _files(directory, 'captures', r'[0-9a-f]{64}\.json')]
    captures.sort(key=lambda value: (read(directory / value['snapshot_ref']).get('ordinal', 0), value['native_entry_id']))
    # Capture sequence is recorded at ingestion, not inferred from lexical native IDs.
    order = cursor['control_interval'] or {}
    sequence = order.get('capture_order', [])
    captures.sort(key=lambda value: sequence.index(value['native_entry_id']) if value['native_entry_id'] in sequence else len(sequence))
    for call_id in cursor['tool_lifecycles']:
        gap(directory, cursor, 'tool:' + call_id, 'missing-stored-source')
    gaps = [read(p, 4096) for p in _files(directory, 'gaps', r'[0-9a-f-]{36}\.json')]
    if not captures and not gaps:
        gap(directory, cursor, None, 'missing-stored-source')
        gaps = [read(p, 4096) for p in _files(directory, 'gaps', r'[0-9a-f-]{36}\.json')]
    admissions = [c for c in captures if read(directory / c['snapshot_ref']).get('role') == 'user'
                  and read(directory / c['snapshot_ref']).get('attribution') == 'user']
    admission = (admissions[0] if admissions else None) if cursor['origin']['kind'] == 'genuine-user' else (captures[0] if captures else None)
    previous = []
    with os.scandir(directory.parent) as siblings:
        for sibling in siblings:
            if sibling.name == 'root-wakes' or sibling.name == directory.name:
                continue
            canonical_uuid(sibling.name)
            other = Path(sibling.path)
            old_cursor = cursor_value(read(other / 'cursor.json'), cursor['session'], sibling.name)
            if old_cursor['event_ordinal'] < cursor['event_ordinal'] and (other / 'segment.json').exists():
                previous.append((old_cursor['event_ordinal'], digest(read(other / 'segment.json'))))
    manifest = {'schema': 'agent-memory/omp-segment-v1', 'session': cursor['session'],
                'lifecycle': cursor['lifecycle'], 'origin': cursor['origin'],
                'seal_id': str(uuid.uuid5(uuid.UUID(cursor['lifecycle']), 'seal')),
                'admission_ref': admission['snapshot_ref'] if admission else None,
                'captures': [{'ref': c['snapshot_ref'], 'sha256': c['snapshot_sha256']} for c in captures],
                'gaps': gaps, 'exclusions': list(omp_projection.EXCLUSIONS),
                'previous_digest': max(previous)[1] if previous else None}
    retain(directory / 'segment.json', manifest)
    enroll_sealed(directory, cursor, manifest)
    return manifest


def enroll_admission(directory, cursor, captured):
    if captured:
        _enroll(directory, cursor, 'omp:capture:' + captured['capture_id'], read(directory / captured['snapshot_ref']), 'admission', captured['native_entry_id'])


def _shared(repo, cursor, *, enrollment_context=None):
    if enrollment_context is not None:
        return turn_sources._enrollment_namespace(repo, 'omp', cursor['session'], cursor['lifecycle'], enrollment_context)
    return turn_sources.private_namespace(repo, 'omp', cursor['session'], cursor['lifecycle'])


def _shared_inventory(repo, cursor, enrollment_context=None):
    if enrollment_context is not None:
        return turn_sources._inspect_enrollment(repo, 'omp', cursor['session'], cursor['lifecycle'], enrollment_context)
    return turn_sources.inspect(repo, 'omp', cursor['session'], cursor['lifecycle'])


def all_segments(repo, errors=None, *, enrollment_context=None):
    from .omp_boundary import _base
    root = _base(repo, enrollment_context=enrollment_context) / 'omp-source-v1'
    result = []
    if not root.exists():
        return result
    trust_paths.state(root)
    with os.scandir(root) as scan:
        session_paths = sorted(list(scan), key=lambda e: e.name)
    if len(session_paths) > 65536:
        raise ContractError('Native session inventory exceeds bound')
    for session_entry in session_paths:
        try:
            if not re.fullmatch(r'[0-9a-f]{64}', session_entry.name):
                raise ContractError('Unknown native session namespace')
            session_path = Path(session_entry.path)
            trust_paths.state(session_path)
            with os.scandir(session_path) as scan:
                children = sorted(list(scan), key=lambda e: e.name)
            if len(children) > 65536:
                raise ContractError('Native session inventory exceeds bound')
            for entry in children:
                try:
                    if entry.name == 'root-wakes':
                        validate_root_wakes(Path(entry.path))
                        continue
                    canonical_uuid(entry.name)
                    directory = Path(entry.path); trust_paths.state(directory)
                    cursor = read(directory / 'cursor.json', 1024 * 1024)
                    cursor_value(cursor, cursor.get('session'), entry.name)
                    if session_dir(repo, cursor['session'], enrollment_context=enrollment_context) != session_path:
                        raise ContractError('Foreign native session namespace')
                    result.append((directory, cursor))
                except (OSError, ValueError, TypeError, KeyError):
                    if errors is None:
                        raise
                    errors.append('native-segment-unverifiable')
        except (OSError, ValueError, TypeError, KeyError):
            if errors is None:
                raise
            errors.append('native-session-unverifiable')
    return result


def registration(repo, directory, cursor, enrollment, *, perform=False, enrollment_context=None):
    if perform and not source_bound_origin(directory, cursor):
        raise ContractError('Cannot register an unbound native root family')
    state = _shared_inventory(repo, cursor, enrollment_context)
    matches = [s for s in state['sources'] if s['source_reference'] == enrollment['source_reference']]
    if matches:
        if len(matches) != 1 or matches[0]['source_sha256'] != enrollment['source_sha256']:
            raise ContractError('Native source reference changed digest or is duplicated')
        return matches[0], state['faults']
    if state['faults'] and state['faults'] != ['source-missing']:
        return None, state['faults']
    attempt_path = directory / 'registration-attempts' / (digest(enrollment['source_reference']) + '.json')
    attempt = {'schema': 'agent-memory/omp-registration-attempt-v1', 'session': cursor['session'],
               'lifecycle': cursor['lifecycle'], 'source_reference': enrollment['source_reference'],
               'source_sha256': enrollment['source_sha256']}
    if attempt_path.exists():
        if read(attempt_path, 4096) != attempt:
            raise ContractError('Native registration attempt identity changed')
        return None, ['registration-fault-retained']
    if not perform:
        return None, []
    _, _, base, shared, identity = _shared(repo, cursor, enrollment_context=enrollment_context)
    pending = base / 'pending'
    occupied = 0; exists = False
    if pending.exists():
        trust_paths.state(pending)
        with os.scandir(pending) as scan:
            for entry in scan:
                if not re.fullmatch(r'(?:0|[1-9][0-9]*)\.json', entry.name) or int(entry.name[:-5]) >= turn_sources.MAX_TURNS:
                    raise ContractError('Unverifiable pending index capacity')
                value = turn_sources.validate_reference(read(Path(entry.path), 4096), Path(repo).resolve(), identity['project_id'])
                occupied += 1
                exists |= value == turn_sources.ref(shared, identity)
    if not exists and occupied >= turn_sources.MAX_TURNS:
        return None, []
    from .omp_boundary import verify_marker
    verify_marker(repo)
    # Recover successful lost responses above; never repeat an unknown attempt
    # whose shared failure could not itself be durably written (e.g. disk full).
    retain(attempt_path, attempt, 4096)
    try:
        observation = turn_sources.register(repo, 'omp', cursor['session'], cursor['lifecycle'],
                                           enrollment['source_sha256'], native_source_id=enrollment['native_source_id'],
                                           source_reference=enrollment['source_reference'], _enrollment_context=enrollment_context)
    except (OSError, ValueError):
        # register's durable fault is intentionally never erased or retried.
        return None, ['registration-fault-retained']
    checked = _shared_inventory(repo, cursor, enrollment_context)
    found = [s for s in checked['sources'] if s['observation_id'] == observation['observation_id']]
    if len(found) != 1:
        raise ContractError('Native registration readback unavailable')
    return found[0], checked['faults']


def drain_enrollments(repo, *, enrollment_context=None):
    """Finish at most eight eligible enrollments, independently of capture time.

    Inventory and each selected registration must finish their durable work:
    elapsed-time cutoffs after scope/inspection can otherwise starve every
    callback. The native adapter bounds its await, not this helper's writes.
    """
    if enrollment_context is not None:
        scope_binding._enrollment_read_context(repo, enrollment_context)
    attempts = 0
    records = []
    for directory, cursor in all_segments(repo, [], enrollment_context=enrollment_context):
        if not source_bound_origin(directory, cursor):
            continue
        try:
            if (directory / 'segment.json').exists():
                enroll_sealed(directory, cursor, manifest_value(directory, cursor))
            for path in _files(directory, 'enrollments', r'[0-9a-f]{64}\.json'):
                enrollment = read(path, 4096)
                records.append((path.stat().st_mtime_ns, enrollment['source_reference'], directory, cursor, enrollment))
        except (OSError, ValueError, TypeError, KeyError):
            continue  # Joined inventory retains the exact fault; no retry of register.
    for _, _, directory, cursor, enrollment in sorted(records, key=lambda value: (value[0], value[1])):
        if attempts >= 8:
            return
        try:
            observation, faults = registration(repo, directory, cursor, enrollment, enrollment_context=enrollment_context)
            if observation is None and not faults:
                attempts += 1
                registration(repo, directory, cursor, enrollment, perform=True, enrollment_context=enrollment_context)
        except (OSError, ValueError, TypeError, KeyError):
            continue


def _coverage(directory, manifest, *, historical=False):
    for gap_value in manifest['gaps']:
        try:
            resolution = read(directory / 'resolutions' / (gap_value['gap_id'] + '.json'), 4096)
        except FileNotFoundError:
            return False
        exact(resolution, {'schema', 'manifest_sha256', 'gap_id', 'recovered_ref', 'recovered_sha256', 'reason'})
        if (resolution['schema'] != 'agent-memory/omp-gap-resolution-v1' or resolution['manifest_sha256'] != digest(manifest)
                or resolution['gap_id'] != gap_value['gap_id'] or resolution['reason'] not in
                {'recovered-safe-native-source', 'recovered-safe-payload', 'verified-native-reassociation'}):
            raise ContractError('Native gap resolution differs from sealed evidence')
        path = _snapshot_path(directory, resolution['recovered_ref'])
        recovered = read(path)
        if not omp_projection._valid_instruction_context(recovered):
            raise ContractError('Invalid recovered native instruction context')
        original = gap_value['native_entry_id']
        reassociated = False
        if isinstance(original, str) and original.startswith('event:') and resolution['reason'] == 'verified-native-reassociation':
            if not historical:
                return False  # Role/order metadata is not an exact native-ID witness.
            cursor = read(directory / 'cursor.json')
            reassociated = matched_metadata(cursor).get(original[6:]) == recovered.get('native_entry_id')
        if isinstance(original, str) and original.startswith('tool:') and resolution['reason'] == 'verified-native-reassociation':
            reassociated = recovered.get('role') == 'toolResult' and recovered.get('toolCallId') == original[5:]
        if digest(recovered) != resolution['recovered_sha256'] or (recovered.get('native_entry_id') != original and not reassociated):
            raise ContractError('Recovered source differs from original native identity')
    return True


def _snapshot_path(directory, reference):
    if not isinstance(reference, str) or not re.fullmatch(r'snapshots/[0-9a-f]{64}\.json', reference):
        raise ContractError('Invalid native snapshot reference')
    return directory / reference


def manifest_value(directory, cursor):
    manifest = read(directory / 'segment.json')
    exact(manifest, {'schema', 'session', 'lifecycle', 'origin', 'seal_id', 'admission_ref',
                     'captures', 'gaps', 'exclusions', 'previous_digest'})
    if (manifest['schema'] != 'agent-memory/omp-segment-v1' or manifest['session'] != cursor['session']
            or manifest['lifecycle'] != cursor['lifecycle'] or manifest['origin'] != cursor['origin']
            or manifest['exclusions'] != list(omp_projection.EXCLUSIONS)
            or not isinstance(manifest['captures'], list) or len(manifest['captures']) > MAX_CAPTURES
            or not isinstance(manifest['gaps'], list) or len(manifest['gaps']) > MAX_CAPTURES):
        raise ContractError('Foreign or oversized native sealed manifest')
    canonical_uuid(manifest['seal_id'])
    for item in manifest['captures']:
        exact(item, {'ref', 'sha256'})
        snapshot = read(_snapshot_path(directory, item['ref']))
        if digest(snapshot) != item['sha256']:
            raise ContractError('Native sealed capture digest changed')
        if not omp_projection._valid_instruction_context(snapshot):
            raise ContractError('Invalid sealed native instruction context')
    for item in manifest['gaps']:
        exact(item, {'schema', 'session', 'lifecycle', 'gap_id', 'native_entry_id', 'reason'})
        if (item['schema'] != 'agent-memory/omp-gap-v1' or item['session'] != cursor['session']
                or item['lifecycle'] != cursor['lifecycle'] or item['reason'] not in GAPS):
            raise ContractError('Foreign native coverage gap')
        canonical_uuid(item['gap_id'])
    return manifest

def historical_completed_family(repo, directory, cursor, *, enrollment_context=None):
    """Preserve completed old assertions without granting new root authority."""
    root = cursor['origin']['human_root']
    family = [item for item in segments(repo, cursor['session'], enrollment_context=enrollment_context)
              if item[1]['origin']['human_root'] == root]
    represented = False
    for old_directory, old_cursor in family:
        if not (old_directory / 'segment.json').exists():
            return False
        state = _shared_inventory(repo, old_cursor, enrollment_context)
        if state['faults'] or not state['sources']:
            return False
        if any(not source['completed'] for source in state['sources']):
            return False
        enrollments = [read(path, 4096) for path in _files(old_directory, 'enrollments', r'[0-9a-f]{64}\.json')]
        if not enrollments or not any(item['kind'] == 'segment' for item in enrollments):
            return False
        sources = {(item['source_reference'], item['source_sha256']) for item in state['sources']}
        if sources != {(item['source_reference'], item['source_sha256']) for item in enrollments}:
            return False
        represented = True
    return represented



def segment_inventory(repo, directory, cursor, *, enrollment_context=None):
    rows = []
    if not source_bound_origin(directory, cursor) and not historical_completed_family(repo, directory, cursor, enrollment_context=enrollment_context):
        return [{'session': cursor['session'], 'lifecycle': cursor['lifecycle'],
                 'human_root': cursor['origin']['human_root'], 'status': 'legacy-unverified-metadata',
                 'source_reference': 'omp:legacy-metadata:' + cursor['lifecycle']}]
    manifest = manifest_value(directory, cursor) if (directory / 'segment.json').exists() else None
    allowed = {'cursor.json', 'admission.json', 'segment.json', 'captures', 'gaps', 'snapshots',
               'enrollments', 'resolutions', 'admissions', 'registration-attempts'}
    with os.scandir(directory) as scan:
        if any(entry.name not in allowed for entry in scan):
            raise ContractError('Unknown or interrupted native source record')
    referenced = set()
    captures_by_id = {}
    for path in _files(directory, 'captures', r'[0-9a-f]{64}\.json'):
        captured = read(path, 4096)
        exact(captured, {'schema', 'session', 'lifecycle', 'capture_id', 'native_entry_id', 'native_parent_id',
                         'snapshot_ref', 'snapshot_sha256', 'representation', 'mapping', 'exclusions'})
        if (captured['schema'] != 'agent-memory/omp-capture-v1' or captured['session'] != cursor['session']
                or captured['lifecycle'] != cursor['lifecycle'] or captured['representation'] != 'native-stored-learning-v1'
                or captured['mapping'] not in {'verified', 'unverified', 'absent'}
                or captured['exclusions'] != list(omp_projection.EXCLUSIONS)):
            raise ContractError('Foreign native capture metadata')
        canonical_uuid(captured['capture_id'])
        native = turn_sources.bounded(captured['native_entry_id'])
        if path.name != hashlib.sha256(native.encode()).hexdigest() + '.json':
            raise ContractError('Native capture namespace differs')
        snapshot_path = _snapshot_path(directory, captured['snapshot_ref'])
        snapshot = read(snapshot_path)
        if digest(snapshot) != captured['snapshot_sha256']:
            raise ContractError('Native capture snapshot digest changed')
        if (snapshot.get('native_entry_id') != native
                or snapshot.get('native_parent_id') != captured['native_parent_id']
                or not omp_projection._valid_instruction_context(snapshot)):
            raise ContractError('Invalid native capture snapshot identity')
        referenced.add(captured['snapshot_ref'])
        captures_by_id[captured['capture_id']] = captured
    admission_path = directory / 'admission.json'
    if os.path.lexists(admission_path):
        admission = admission_metadata(read(admission_path, 4096), cursor['session'])
        captured = captures_by_id.get(admission['capture_id'])
        if captured is None or admission['lifecycle'] != cursor['lifecycle'] or admission['event_ordinal'] != cursor['event_ordinal']:
            raise ContractError('Native admission differs from its retained source capture')
        snapshot = read(_snapshot_path(directory, captured['snapshot_ref']))
        content = snapshot.get('content')
        calls = [block['id'] for block in content if isinstance(block, dict) and block.get('type') == 'toolCall'] if isinstance(content, list) else []
        if (admission['role'] != snapshot.get('role', 'custom') or admission['attribution'] != snapshot.get('attribution', 'unknown')
                or admission['tool_call_ids'] != calls):
            raise ContractError('Native admission metadata differs from its safe stored projection')
    for native, record in (cursor['control_interval'] or {}).get('pure_entries', {}).items():
        exact(record, {'snapshot_ref', 'snapshot_sha256'})
        if digest(read(_snapshot_path(directory, record['snapshot_ref']))) != record['snapshot_sha256']:
            raise ContractError('Own control snapshot digest changed')
        referenced.add(record['snapshot_ref'])
    for path in _files(directory, 'resolutions', r'[0-9a-f-]{36}\.json'):
        resolution = read(path, 4096)
        exact(resolution, {'schema', 'manifest_sha256', 'gap_id', 'recovered_ref', 'recovered_sha256', 'reason'})
        referenced.add(resolution['recovered_ref'])
    for path in _files(directory, 'gaps', r'[0-9a-f-]{36}\.json'):
        retained_gap = read(path, 4096)
        exact(retained_gap, {'schema', 'session', 'lifecycle', 'gap_id', 'native_entry_id', 'reason'})
        if manifest is not None and retained_gap not in manifest['gaps']:
            raise ContractError('Unenrolled late native coverage gap remains pending')
    for path in _files(directory, 'snapshots', r'[0-9a-f]{64}\.json'):
        if str(path.relative_to(directory)) not in referenced:
            raise ContractError('Unenrolled native snapshot remains pending')
    if manifest is None:
        rows.append({'session': cursor['session'], 'lifecycle': cursor['lifecycle'], 'human_root': cursor['origin']['human_root'],
                     'status': 'open-segment', 'source_reference': 'omp:open:' + cursor['lifecycle']})
    matched = matched_metadata(cursor)
    if manifest is None:
        for call_id in cursor['tool_lifecycles']:
            rows.append({'session': cursor['session'], 'lifecycle': cursor['lifecycle'], 'human_root': cursor['origin']['human_root'],
                         'status': 'pending-native-tool', 'source_reference': 'omp:tool:' + call_id})
    for path in _files(directory, 'admissions', r'[0-9a-f-]{36}\.json'):
        meta = admission_metadata(read(path, 4096), cursor['session'])
        if path.name != meta['capture_id'] + '.json':
            raise ContractError('Native admission metadata namespace differs')
        if meta['capture_id'] not in matched:
            rows.append({'session': cursor['session'], 'lifecycle': cursor['lifecycle'], 'human_root': cursor['origin']['human_root'],
                         'status': 'legacy-unverified-notification', 'source_reference': 'omp:event:' + meta['capture_id'],
                         'actionable': False})
    for path in _files(directory, 'registration-attempts', r'[0-9a-f]{64}\.json'):
        attempt = read(path, 4096)
        exact(attempt, {'schema', 'session', 'lifecycle', 'source_reference', 'source_sha256'})
        if (attempt['schema'] != 'agent-memory/omp-registration-attempt-v1' or attempt['session'] != cursor['session']
                or attempt['lifecycle'] != cursor['lifecycle']
                or path.name != digest(attempt['source_reference']) + '.json'):
            raise ContractError('Foreign native registration attempt')
        enrolled = read(directory / 'enrollments' / path.name, 4096)
        if any(attempt[key] != enrolled[key] for key in ('session', 'lifecycle', 'source_reference', 'source_sha256')):
            raise ContractError('Native registration attempt lost its enrollment identity')
    for path in _files(directory, 'enrollments', r'[0-9a-f]{64}\.json'):
        enrollment = read(path, 4096)
        exact(enrollment, {'schema', 'session', 'lifecycle', 'source_reference', 'source_sha256', 'snapshot_ref', 'kind', 'native_source_id'})
        if (enrollment['schema'] != 'agent-memory/omp-enrollment-v1' or enrollment['session'] != cursor['session']
                or enrollment['lifecycle'] != cursor['lifecycle'] or enrollment['kind'] not in {'admission', 'segment'}):
            raise ContractError('Foreign native enrollment')
        if enrollment['kind'] == 'admission' and not os.path.lexists(admission_path):
            raise ContractError('Native source admission metadata remains unenrolled')
        target = directory / 'segment.json' if enrollment['kind'] == 'segment' else _snapshot_path(directory, enrollment['snapshot_ref'])
        if digest(read(target)) != enrollment['source_sha256']:
            raise ContractError('Native enrollment source digest differs')
        try:
            observation, errors = registration(repo, directory, cursor, enrollment, enrollment_context=enrollment_context)
        except (OSError, ValueError):
            observation, errors = None, ['source-unavailable']
        status = 'registration-fault' if errors else ('completed' if observation and observation['completed'] else 'pending-assessment' if observation else 'enrollment-pending')
        if manifest and not _coverage(directory, manifest, historical=bool(observation and observation['completed'])):
            status = 'pending-coverage-gap'
        row = {'session': cursor['session'], 'lifecycle': cursor['lifecycle'], 'human_root': cursor['origin']['human_root'],
               'source_reference': enrollment['source_reference'], 'source_sha256': enrollment['source_sha256'], 'status': status}
        if observation:
            row['observation_id'] = observation['observation_id']
        if errors:
            row['faults'] = errors
        rows.append(row)
    if manifest and not any(row['source_reference'] == 'omp:seal:' + manifest['seal_id'] for row in rows):
        rows.append({'session': cursor['session'], 'lifecycle': cursor['lifecycle'], 'human_root': cursor['origin']['human_root'],
                     'source_reference': 'omp:seal:' + manifest['seal_id'], 'status': 'enrollment-pending'})
    if manifest and manifest['admission_ref'] is not None:
        captured = [read(path, 4096) for path in _files(directory, 'captures', r'[0-9a-f]{64}\.json')]
        captured = [item for item in captured if item['snapshot_ref'] == manifest['admission_ref']]
        if len(captured) != 1:
            raise ContractError('Native manifest admission capture is unavailable')
        reference = 'omp:capture:' + captured[0]['capture_id']
        if not any(row['source_reference'] == reference for row in rows):
            rows.append({'session': cursor['session'], 'lifecycle': cursor['lifecycle'], 'human_root': cursor['origin']['human_root'],
                         'source_reference': reference, 'status': 'enrollment-pending'})
    return rows


def pending_inventory(repo, *, enrollment_context=None):
    if enrollment_context is not None:
        scope_binding._enrollment_read_context(repo, enrollment_context)
    rows = []; faults = []; represented = set()
    for directory, cursor in all_segments(repo, faults, enrollment_context=enrollment_context):
        try:
            segment_rows = segment_inventory(repo, directory, cursor, enrollment_context=enrollment_context)
            rows.extend(segment_rows)
            represented.update(row['observation_id'] for row in segment_rows if 'observation_id' in row)
        except (OSError, ValueError, TypeError, KeyError):
            faults.append('native-enrollment-unverifiable')
            rows.append({'session': cursor['session'], 'lifecycle': cursor['lifecycle'], 'human_root': cursor['origin']['human_root'],
                         'source_reference': 'omp:fault:' + cursor['lifecycle'], 'status': 'registration-fault'})
    # Include orphan shared registrations, even if they never reached the transport receipt.
    from .omp_boundary import _base
    turns = _base(repo, enrollment_context=enrollment_context) / 'turn-sources-v2' / 'turns'
    try:
        shared_directories = directories(turns, r'[0-9a-f]{64}')
    except (OSError, ValueError, TypeError, KeyError):
        faults.append('shared-source-inventory-unverifiable')
        shared_directories = []
    for directory in shared_directories:
        sources = []
        for i in range(turn_sources.MAX_SOURCES):
            try:
                value = read(directory / ('source-' + str(i) + '.json'), 4096)
            except FileNotFoundError:
                continue
            except (OSError, ValueError, TypeError, KeyError):
                faults.append('shared-source-record-unverifiable')
                continue
            if not isinstance(value, dict):
                faults.append('shared-source-record-unverifiable')
                continue
            if value.get('harness') == 'omp':
                sources.append(value)
        if not sources:
            continue
        first = sources[0]
        try:
            root, _, _, expected, identity = _shared(repo, first, enrollment_context=enrollment_context)
            if expected != directory:
                raise ContractError('Foreign shared source namespace')
            with turn_sources.locked(directory):
                inventory = turn_sources.inventory(directory, identity)
            for value in inventory['sources']:
                if value['observation_id'] not in represented:
                    rows.append({'session': value['session'], 'lifecycle': value['lifecycle'],
                                 'source_reference': value['source_reference'], 'observation_id': value['observation_id'],
                                 'source_sha256': value['source_sha256'], 'status': 'orphan-registration'})
            faults.extend(inventory['faults'])
        except (OSError, ValueError, TypeError, KeyError):
            faults.append('orphan-registration-unverifiable')
    rows.sort(key=lambda r: (r['session'], r['lifecycle'], r['source_reference']))
    return rows, sorted(set(faults))


def list_pending(repo, offset=0):
    from .omp_boundary import source_barrier, private_lock, _base
    if type(offset) is not int or not 0 <= offset <= 262144:
        raise ContractError('Invalid native pending cursor')
    with scope_binding._fresh_enrollment_locations(), source_barrier(repo), private_lock(_base(repo) / 'omp-transport.lock'):
        rows, faults = pending_inventory(repo)
        rows = [r for r in rows if r['status'] != 'completed']
        from . import omp_stream
        for index, item in enumerate(omp_stream.inventory(repo)['pending']):
            rows.append({'session': item['session'], 'lifecycle': item['instance_id'],
                         'source_reference': 'omp:stream:' + item['instance_id'] + ':' + str(index),
                         'status': 'pending-native-coverage', 'qualification': item,
                         'actionable': False})
        page = []; position = offset
        for row in rows[offset:]:
            if len(turn_sources.encoded(page + [row])) > 48000:
                break
            page.append(row); position += 1
        return {'operation': 'pending', 'pending': page, 'faults': faults,
                'next_offset': position if position < len(rows) else None,
                'inventory_complete': not faults and position == len(rows) and offset == 0,
                'source_completeness_verified': False, 'canonical_writes_performed': False, 'verified': True}


def source(repo, session, lifecycle, identifier):
    from .omp_boundary import source_barrier, private_lock, _base
    with scope_binding._fresh_enrollment_locations(), source_barrier(repo), private_lock(_base(repo) / 'omp-transport.lock'):
        return _source(repo, session, lifecycle, identifier)


def _source(repo, session, lifecycle, identifier):
    canonical_uuid(lifecycle)
    directory = session_dir(repo, session) / lifecycle
    cursor = cursor_value(read(directory / 'cursor.json'), session, lifecycle)
    if not source_bound_origin(directory, cursor) and not historical_completed_family(repo, directory, cursor):
        raise ContractError('Native source has no independently stored root origin')
    segment_inventory(repo, directory, cursor)
    observation = None
    for path in _files(directory, 'enrollments', r'[0-9a-f]{64}\.json'):
        enrollment = read(path, 4096)
        match, errors = registration(repo, directory, cursor, enrollment)
        if enrollment['source_reference'] == identifier or (match and match['observation_id'] == identifier):
            if observation is not None:
                raise ContractError('Ambiguous native source')
            observation = (enrollment, match, errors)
    if observation is None:
        raise ContractError('Original native source unavailable')
    enrollment, shared, faults = observation
    manifest = manifest_value(directory, cursor) if (directory / 'segment.json').exists() else None
    if enrollment['kind'] == 'segment':
        payload = manifest
    else:
        candidates = [read(p, 4096) for p in _files(directory, 'captures', r'[0-9a-f]{64}\.json')]
        candidates = [c for c in candidates if 'omp:capture:' + c['capture_id'] == enrollment['source_reference']]
        if len(candidates) != 1:
            raise ContractError('Original native capture unavailable')
        payload = read(_snapshot_path(directory, candidates[0]['snapshot_ref']))
    if payload is None or digest(payload) != enrollment['source_sha256']:
        raise ContractError('Retained native source digest changed')
    capture_metadata = [read(path, 4096) for path in _files(directory, 'captures', r'[0-9a-f]{64}\.json')]
    absent = (cursor['control_interval'] or {}).get('absent_native_entries', [])
    qualifications = []
    for item in capture_metadata:
        snapshot = read(_snapshot_path(directory, item['snapshot_ref']))
        qualification = {'native_entry_id': item['native_entry_id'],
                         'mapping': 'absent' if item['native_entry_id'] in absent else item['mapping'],
                         'snapshot_reference': str(_snapshot_path(directory, item['snapshot_ref'])),
                         'snapshot_sha256': item['snapshot_sha256'], 'exclusions': item['exclusions']}
        if 'instruction_context' in snapshot:
            qualification['instruction_context'] = snapshot['instruction_context']
            qualification['instruction_context_qualification'] = 'Historical observer assertion; body intentionally nonlearning, not current installation authentication or complete source coverage.'
        qualifications.append(qualification)
    return {'verified': True, 'source': payload, 'enrollment': enrollment, 'observation': shared,
            'origin': cursor['origin'], 'coverage_complete': manifest is not None and _coverage(directory, manifest, historical=bool(shared and shared['completed'])),
            'origin_qualification': 'stored-native-id-bound' if source_bound_origin(directory, cursor) else 'historical-unverified-native-origin',
            'exclusions': list(omp_projection.EXCLUSIONS), 'faults': faults,
            'retained_directory': str(directory), 'captures': qualifications,
            'gaps': manifest['gaps'] if manifest else [],
            'gap_resolutions': [read(path, 4096) for path in _files(directory, 'resolutions', r'[0-9a-f-]{36}\.json')],
            'canonical_writes_performed': False}


def complete(repo, value):
    from .omp_boundary import source_barrier, private_lock, _base
    fields = {'session', 'lifecycle', 'observation_id', 'source_sha256', 'outcome', 'assessment'}
    if not isinstance(value, dict) or set(value) not in (fields, fields | {'publication_request'}):
        raise ContractError('Use exact native observation-bound assessment fields')
    with scope_binding._fresh_enrollment_locations(), source_barrier(repo), private_lock(_base(repo) / 'omp-transport.lock'):
        # No held-enrollment authority reaches acknowledgement or refresh.
        scope_binding.require(repo)
        checked = _source(repo, value['session'], value['lifecycle'], value['observation_id'])
        if not checked['coverage_complete'] or checked['faults'] or checked['observation'] is None:
            raise ContractError('Native segment not sealed/enrolled or has unresolved coverage gaps')
        directory = session_dir(repo, value['session']) / value['lifecycle']
        cursor = read(directory / 'cursor.json')
        if not source_bound_origin(directory, cursor):
            observation = checked['observation']
            receipt = observation.get('completion') if observation.get('completed') else None
            if (not isinstance(receipt, dict) or receipt.get('input_sha256') != digest(value)
                    or receipt.get('source_sha256') != value['source_sha256']):
                raise ContractError('Native assessment requires an independently stored root origin')
            return {'verified': True, 'already_completed': True, **receipt,
                    'origin_qualification': 'historical-unverified-native-origin'}
        manifest = read(directory / 'segment.json')
        segment_refs = [read(p, 4096) for p in _files(directory, 'enrollments', r'[0-9a-f]{64}\.json')]
        sealed = [e for e in segment_refs if e['kind'] == 'segment']
        if len(sealed) != 1 or registration(repo, directory, cursor, sealed[0])[0] is None:
            raise ContractError('Admission acknowledgement requires registered sealed segment')
        if checked['enrollment']['source_sha256'] != value['source_sha256']:
            raise ContractError('Native assessment source digest differs')
        receipt = turn_sources.verify_assessment(repo, value, value['observation_id'])
        return turn_sources.complete(repo, 'omp', value['session'], value['lifecycle'], value['observation_id'], value['source_sha256'], receipt)


def wake(repo, session, lifecycle, payload, *, delivery_window=None, enrollment_context=None):
    exact(payload, {'observation_ids', 'enrollment_refs'})
    for key in payload:
        if not isinstance(payload[key], list) or payload[key] != sorted(set(payload[key])):
            raise ContractError('Use sorted unique native wake references')
    directory = session_dir(repo, session, enrollment_context=enrollment_context) / canonical_uuid(lifecycle)
    cursor = cursor_value(read(directory / 'cursor.json'), session, lifecycle)
    rows, faults = pending_inventory(repo, enrollment_context=enrollment_context)
    current = [r for r in rows if r['session'] == session and r.get('human_root') == cursor['origin']['human_root'] and r['status'] != 'completed']
    expected_ids = sorted(r['observation_id'] for r in current if 'observation_id' in r)
    expected_refs = sorted(r['source_reference'] for r in current if 'observation_id' not in r and r['source_reference'].startswith(('omp:capture:', 'omp:seal:')))
    if payload['observation_ids'] != expected_ids or payload['enrollment_refs'] != expected_refs:
        raise ContractError('Current native root inventory changed before wake')
    root = cursor['origin']['human_root']
    target = session_dir(repo, session, enrollment_context=enrollment_context) / 'root-wakes' / (root + '.json')
    if target.exists() or cursor['origin']['native_trigger'] not in {'human', 'human-with-own-guidance'}:
        return {'continue': False, 'status': 'pending', 'canonical_writes_performed': False}
    root_cursor = read(session_dir(repo, session, enrollment_context=enrollment_context) / root / 'cursor.json')
    native_user_entry = (root_cursor['control_interval'] or {}).get('human_entry')
    if not source_bound_origin(session_dir(repo, session, enrollment_context=enrollment_context) / root, root_cursor):
        return {'continue': False, 'status': 'pending-missing-human-source', 'canonical_writes_performed': False}
    if not native_user_entry:
        return {'continue': False, 'status': 'pending-missing-human-source', 'canonical_writes_performed': False}
    wakes = session_dir(repo, session, enrollment_context=enrollment_context) / 'root-wakes'
    try:
        prior_users = validate_root_wakes(wakes, session) if os.path.lexists(wakes) else set()
    except (OSError, ValueError, TypeError, KeyError):
        return {'continue': False, 'status': 'pending-wake-state-unverifiable', 'canonical_writes_performed': False}
    if native_user_entry in prior_users:
        return {'continue': False, 'status': 'pending-already-requested-native-user', 'canonical_writes_performed': False}
    value = {'schema': 'agent-memory/omp-root-wake-v1', 'session': session, 'human_root': root,
             'native_user_entry': native_user_entry, 'request_id': str(uuid.uuid4()), 'status': 'requested'}
    earlier = [r for r in rows if r['status'] != 'completed' and r not in current]
    inline = []; omitted = False
    for row in current + earlier:
        if len(turn_sources.encoded(inline + [row])) > 6000:
            omitted = True; continue
        inline.append(row)
    import json
    guidance = ('[agent-memory omp request ' + value['request_id'] + '] Agent-attributed advisory, not human input. '
                'Your substantive answer is already delivered; never repeat or replace it. Read skill://agent-memory and use ordinary approved tools only. '
                'Inspect each exact source using am omp-boundary --repo ' + json.dumps(str(Path(repo).resolve())) +
                ' --source ID --session SESSION --lifecycle LIFECYCLE. Re-observe/search current memory before governed capture or publication. '
                'Assess sources individually using am omp-boundary --repo PATH --stdin --quiet --refresh with exactly '
                '{session,lifecycle,observation_id,source_sha256,outcome,assessment} and publication_request only for published. '
                'Admission assessment requires its segment enrolled; unresolved gaps cannot be waived. '
                'A refusal, cancellation, unavailable authority or omitted work stays pending; do not bypass permission or force a tool. '
                'Full joined backlog: am omp-boundary --repo PATH --list-pending --offset 0; follow next_offset until exhausted. '
                'inventory_complete:' + str(not omitted and not faults).lower() + '. Sources: ' + json.dumps(inline, separators=(',', ':')) +
                ' Faults:' + json.dumps(faults) + ' [/agent-memory omp request]')
    if len(guidance.encode()) > 8192:
        return {'continue': False, 'status': 'pending-wake-guidance-bound', 'canonical_writes_performed': False}
    if enrollment_context is not None:
        scope_binding._enrollment_read_context(repo, enrollment_context)
        scope_binding.recheck_private_enrollment(enrollment_context)
    from .omp_stream import VerifiedNativeDeliveryWindow
    if type(delivery_window) is not VerifiedNativeDeliveryWindow or not delivery_window.allow_reservation():
        return {'continue': False, 'status': 'pending-undeliverable-window', 'canonical_writes_performed': False}
    try:
        retain(target, value, 4096)
        control = root_cursor['control_interval'] or {}
        control['generated_guidance'] = guidance
        root_cursor['control_interval'] = control
        replace_cursor(session_dir(repo, session, enrollment_context=enrollment_context) / root / 'cursor.json', root_cursor)
    except Exception:
        # Reservation may have reached durable storage. Never retry this attempt.
        return {'continue': False, 'status': 'pending-wake-unknown-after-reservation',
                'canonical_writes_performed': False}
    return {'continue': True, 'additionalContext': guidance, 'request_id': value['request_id'],
            'inventory_complete': not omitted and not faults, 'canonical_writes_performed': False}


def resolve_tool_result(current, owner, captured, invocations):
    """Recover only an unambiguous native invocation, including crash readback."""
    directory, cursor = owner
    current = [item for item in current if source_bound_origin(*item)]
    if not source_bound_origin(directory, cursor):
        return False
    recovered = read(_snapshot_path(directory, captured['snapshot_ref']))
    call_id = recovered.get('toolCallId')
    if recovered.get('role') != 'toolResult' or not call_id:
        return False
    if 'instruction_context' in recovered:
        for old_dir, old_cursor in current:
            if any(read(path, 4096)['native_entry_id'] == 'tool:' + call_id
                   for path in _files(old_dir, 'gaps', r'[0-9a-f-]{36}\.json')):
                return False
            if (old_dir / 'segment.json').exists() and any(
                    old['native_entry_id'] == 'tool:' + call_id
                    for old in manifest_value(old_dir, old_cursor)['gaps']):
                return False
    roots = {c['origin']['human_root'] for _, c in current if call_id in c['tool_lifecycles']}
    root = cursor['origin']['human_root']
    if len(invocations.get(call_id, set())) != 1 or not roots or roots != {root}:
        return False
    for old_dir, old_cursor in current:
        if call_id not in old_cursor['tool_lifecycles']:
            continue
        if (old_dir / 'segment.json').exists():
            manifest = manifest_value(old_dir, old_cursor)
            for old_gap in manifest['gaps']:
                if old_gap['native_entry_id'] == 'tool:' + call_id:
                    reference = 'snapshots/' + hashlib.sha256(recovered['native_entry_id'].encode()).hexdigest() + '.json'
                    retain(old_dir / reference, recovered)
                    retain(old_dir / 'resolutions' / (old_gap['gap_id'] + '.json'),
                           {'schema': 'agent-memory/omp-gap-resolution-v1', 'manifest_sha256': digest(manifest),
                            'gap_id': old_gap['gap_id'], 'recovered_ref': reference,
                            'recovered_sha256': digest(recovered), 'reason': 'verified-native-reassociation'}, 4096)
        if not (old_dir / 'segment.json').exists():
            old_cursor['tool_lifecycles'].pop(call_id, None)
            replace_cursor(old_dir / 'cursor.json', old_cursor)
    return True


def _checked_payload_coverage(current, payload, rows, faults):
    """Retained safe native IDs or explicit payload-not-required metadata only."""
    native_ids = {item['id'] for item in payload['view']['native_parents']}
    qualified = {entry['id'] for entry in payload['entries']
                 if entry.get('type') in NONLEARNING or entry.get('agent', {}).get('kind') == 'sub'}
    for directory, cursor in current:
        if not source_bound_origin(directory, cursor):
            continue
        for path in _files(directory, 'captures', r'[0-9a-f]{64}\.json'):
            captured = read(path, 4096)
            snapshot = read(_snapshot_path(directory, captured['snapshot_ref']))
            if digest(snapshot) != captured['snapshot_sha256']:
                raise ContractError('Checked native payload digest changed')
            qualified.add(captured['native_entry_id'])
        for native, captured in (cursor['control_interval'] or {}).get('pure_entries', {}).items():
            if digest(read(_snapshot_path(directory, captured['snapshot_ref']))) != captured['snapshot_sha256']:
                raise ContractError('Checked native control payload changed')
            qualified.add(native)
    relevant = [row for row in rows if row['session'] == payload['view']['session']]
    references = [{'session': row['session'], 'lifecycle': row['lifecycle'],
                   'observation_id': row['observation_id'], 'source_sha256': row['source_sha256']}
                  for row in relevant if 'observation_id' in row and 'source_sha256' in row
                  and row['status'] not in {'orphan-registration', 'legacy-unverified-metadata', 'open-segment'}
                  and any(cursor['lifecycle'] == row['lifecycle'] and source_bound_origin(directory, cursor)
                          for directory, cursor in current)]
    references_complete = len(references) <= 64
    references = references[:64]
    incomplete = {'pending-coverage-gap', 'registration-fault', 'pending-native-tool',
                  'legacy-unverified-metadata', 'enrollment-pending', 'orphan-registration', 'open-segment'}
    complete = (bool(native_ids) and native_ids.issubset(qualified) and not faults
                and references_complete and not any(row['status'] in incomplete for row in relevant)
                and payload['view']['tail_status'] == 'available')
    return sorted(qualified & native_ids), references, complete


def _verify_observer_receipt(receipt, *, repo=None, verified_binding=None):
    from . import omp_installation
    if not isinstance(receipt, dict):
        raise ContractError('Native observation requires a current installed binding')
    if repo is None:
        return omp_installation.verified_observer_binding(receipt['source']['artifact_digest'],
            receipt['omp_binding']['agent_dir'], receipt=receipt)
    return omp_installation._observe_observer_binding(repo, receipt, verified_binding)


def observe(repo, value, *, receipt=None, on_started=None, delivery_window=None, verified_binding=None):
    from .omp_boundary import source_barrier, private_lock, _base
    exact(value, {'schema', 'action', 'session', 'lifecycle', 'payload'})
    if value['schema'] != 'agent-memory/omp-observer-request-v2' or value['action'] not in {'ingest', 'seal', 'wake'}:
        raise ContractError('Unsupported native observer operation')
    session = turn_sources.bounded(value['session']); canonical_uuid(value['lifecycle'])
    payload = value['payload']
    if value['action'] != 'wake':
        fields = {'entries', 'branch_ids', 'event_ordinal', 'terminal', 'trigger', 'inherited_ids', 'view'}
        exact(payload, fields | {'request_wake'} if value['action'] == 'seal' else fields)
        if value['action'] == 'seal' and type(payload['request_wake']) is not bool:
            raise ContractError('Invalid native wake selection')
        if (type(payload['event_ordinal']) is not int or payload['event_ordinal'] < 0
                or type(payload['terminal']) is not bool or payload['trigger'] not in TRIGGERS):
            raise ContractError('Native stored sweep exceeds bound')
        view = omp_projection.checked_view(repo, session, payload)
        for key in ('branch_ids', 'inherited_ids'):
            if not isinstance(payload[key], list) or len(payload[key]) > MAX_CAPTURES or len(set(payload[key])) != len(payload[key]):
                raise ContractError('Invalid native branch inventory')
            for native in payload[key]:
                turn_sources.bounded(native)
        if not set(payload['inherited_ids']).issubset(item['id'] for item in view['native_parents']):
            raise ContractError('Inherited native source identity unavailable')
    if verified_binding is None:
        verified_binding = _verify_observer_receipt(receipt)
    with source_barrier(repo), private_lock(_base(repo) / 'omp-transport.lock'):
        with _verify_observer_receipt(receipt, repo=repo, verified_binding=verified_binding) as context, ExitStack() as locations:
            fences = []
            if value['action'] == 'ingest':
                from . import omp_stream
                fences = omp_stream.retire_for_recovery(repo, session, exclude_instance=getattr(on_started, 'instance_id', None))
            if on_started is not None:
                on_started()
            locations.enter_context(scope_binding._observe_enrollment_locations(context))
            if value['action'] == 'wake':
                return {'verified': True, **wake(repo, session, value['lifecycle'], payload, delivery_window=delivery_window,
                                               enrollment_context=context)}
            if value['action'] in {'ingest', 'seal'}:
                current = [item for item in segments(repo, session, enrollment_context=context) if source_bound_origin(*item)]
                known = {}; tools = {}; invocations = {}; retained_results = []
                snapshots = {}; retained_tool_gaps = set()
                raw_calls = {}; raw_results = {}
                for candidate in payload['entries']:
                    if not isinstance(candidate, dict) or not isinstance(candidate.get('message'), dict):
                        continue
                    msg = candidate['message']
                    if msg.get('role') == 'assistant' and isinstance(msg.get('content'), list):
                        for block in msg['content']:
                            if isinstance(block, dict) and block.get('type') == 'toolCall' and isinstance(block.get('id'), str):
                                raw_calls.setdefault(block['id'], []).append(candidate)
                    elif msg.get('role') == 'toolResult' and isinstance(msg.get('toolCallId'), str):
                        raw_results.setdefault(msg['toolCallId'], []).append(candidate)
                for d, c in current:
                    if not source_bound_origin(d, c):
                        continue
                    referenced = {record['snapshot_ref'] for record in
                                  (c['control_interval'] or {}).get('pure_entries', {}).values()}
                    for path in _files(d, 'resolutions', r'[0-9a-f-]{36}\.json'):
                        referenced.add(read(path, 4096)['recovered_ref'])
                    for path in _files(d, 'enrollments', r'[0-9a-f]{64}\.json'):
                        referenced.add(read(path, 4096)['snapshot_ref'])
                    if (d / 'segment.json').exists():
                        manifest = manifest_value(d, c)
                        referenced.update(item['ref'] for item in manifest['captures'])
                        if manifest['admission_ref'] is not None:
                            referenced.add(manifest['admission_ref'])
                        for old in manifest['gaps']:
                            if isinstance(old['native_entry_id'], str) and old['native_entry_id'].startswith('tool:'):
                                retained_tool_gaps.add(old['native_entry_id'][5:])
                    tools.update(c['tool_lifecycles'])
                    for path in _files(d, 'captures', r'[0-9a-f]{64}\.json'):
                        cap = read(path, 4096); known[cap['native_entry_id']] = (d, c)
                        referenced.add(cap['snapshot_ref'])
                        snapshot = read(_snapshot_path(d, cap['snapshot_ref']))
                        if snapshot.get('role') == 'assistant' and isinstance(snapshot.get('content'), list):
                            for block in snapshot['content']:
                                if block.get('type') == 'toolCall':
                                    tools[block['id']] = c['lifecycle']
                                    invocations.setdefault(block['id'], set()).add(cap['native_entry_id'])
                        if snapshot.get('role') == 'toolResult' and snapshot.get('toolCallId'):
                            retained_results.append(((d, c), cap))
                    for path in _files(d, 'snapshots', r'[0-9a-f]{64}\.json'):
                        if str(path.relative_to(d)) not in referenced:
                            # A sealed ordinary orphan may be unrecorded pure
                            # control; never reopen its parent for later work.
                            if (d / 'segment.json').exists() and 'instruction_context' not in read(path):
                                continue
                            snapshots.setdefault(path.name, []).append((d, c))
                    for path in _files(d, 'gaps', r'[0-9a-f-]{36}\.json'):
                        old = read(path, 4096)
                        if isinstance(old['native_entry_id'], str) and old['native_entry_id'].startswith('tool:'):
                            retained_tool_gaps.add(old['native_entry_id'][5:])
                        if old['native_entry_id']:
                            known.setdefault(old['native_entry_id'], (d, c))
                    if not (d / 'segment.json').exists():
                        control = c['control_interval'] or {}
                        control['absent_native_entries'] = [cap['native_entry_id'] for cap in
                                                           (read(p, 4096) for p in _files(d, 'captures', r'[0-9a-f]{64}\.json'))
                                                           if cap['native_entry_id'] not in payload['branch_ids']]
                        c['control_interval'] = control
                        replace_cursor(d / 'cursor.json', c)
                for result_owner, result_capture in retained_results:
                    if resolve_tool_result(current, result_owner, result_capture, invocations):
                        result = read(_snapshot_path(result_owner[0], result_capture['snapshot_ref']))
                        tools.pop(result['toolCallId'], None)
                active = active_segment(current, payload['branch_ids'])
                pure_known = {}
                for old_d, old_c in current:
                    for native, record in (old_c['control_interval'] or {}).get('pure_entries', {}).items():
                        if native not in known:
                            pure_known[native] = (old_d, record)
                fresh = [entry for entry in payload['entries'] if isinstance(entry, dict)
                         and entry.get('id') not in known and entry.get('id') not in pure_known
                         and entry.get('type') not in NONLEARNING]
                if active and fresh:
                    d, c = active
                    control = c['control_interval'] or {}
                    if any(entry.get('type') == 'message' and entry.get('message', {}).get('role') == 'user'
                           and entry.get('message', {}).get('attribution') == 'user' for entry in fresh):
                        if any(omp_projection.project(entry).get('snapshot', {}).get('content') == control.get('generated_guidance')
                               for entry in fresh) and control.get('generated_guidance'):
                            payload['trigger'] = 'human-with-own-guidance'
                    classified = omp_control.pure_batch(fresh, control.get('generated_guidance'),
                                                       continuing=control.get('pure_mode', False),
                                                       tool_ids=control.get('pure_tool_ids', []))
                    if classified is not None:
                        projected, pure_tools = classified
                        pure_records = control.setdefault('pure_entries', {})
                        for entry, snapshot in projected:
                            native = entry['id']
                            reference = 'snapshots/' + hashlib.sha256(native.encode()).hexdigest() + '.json'
                            snapshot_hash = retain(d / reference, snapshot)
                            pure_records[native] = {'snapshot_ref': reference, 'snapshot_sha256': snapshot_hash}
                            pure_known[native] = (d, pure_records[native])
                            if snapshot.get('role') == 'toolResult':
                                c['tool_lifecycles'].pop(snapshot.get('toolCallId'), None)
                        control.update(pure_mode=True, pure_tool_ids=pure_tools, native_trigger='own-hidden-pure')
                        c['control_interval'] = control
                        replace_cursor(d / 'cursor.json', c)
                    elif control.get('pure_mode'):
                        control['pure_mode'] = False
                        control['native_trigger'] = 'own-hidden-ambiguous'
                        c['control_interval'] = control
                        replace_cursor(d / 'cursor.json', c)
                        # Reclassification retains the reserved control context in
                        # the linked substantive successor, never reopens its parent.
                        inherited_control = []
                        for native, (old_d, record) in pure_known.items():
                            snapshot = read(_snapshot_path(old_d, record['snapshot_ref']))
                            if snapshot['carrier'] == 'custom_message':
                                inherited_control.append({'type': 'custom_message', 'id': native,
                                                          'parentId': snapshot['native_parent_id'],
                                                          'customType': snapshot['customType'], 'content': snapshot['content'],
                                                          'attribution': snapshot.get('attribution')})
                        payload['entries'] = inherited_control + payload['entries']
                        for item in inherited_control:
                            pure_known.pop(item['id'], None)
                deferred_mentions = {}
                for position, entry in enumerate(payload['entries']):
                    if not isinstance(entry, dict):
                        raise ContractError('Malformed native stored carrier')
                    native = turn_sources.bounded(entry.get('id'))
                    if native in payload['inherited_ids'] and native not in known:
                        continue
                    if native in pure_known:
                        continue
                    if isinstance(entry.get('agent'), dict) and entry['agent'].get('kind') == 'sub':
                        continue
                    if entry.get('type') in NONLEARNING:
                        continue
                    if entry.get('type') not in {'message', 'custom_message'}:
                        if active is None:
                            active = new_segment(repo, session, value['lifecycle'], value['lifecycle'], 'resume', payload['event_ordinal'],
                                                 enrollment_context=context)
                            current.append(active)
                        active = writable_successor(repo, session, active, current, payload['event_ordinal'], enrollment_context=context)
                        gap(*active, native, 'missing-stored-source')
                        continue
                    message = entry.get('message') if isinstance(entry.get('message'), dict) else {}
                    stored_user = entry.get('type') == 'message' and message.get('role') == 'user' and message.get('attribution') == 'user'
                    human = stored_user and view['fork_status'] != 'unavailable'
                    inherited = native in payload['inherited_ids']
                    if native in known:
                        d, c = known[native]
                        # Recover original missing evidence, never mutate an existing safe snapshot.
                        name = hashlib.sha256(native.encode()).hexdigest()
                        if not (d / 'captures' / (name + '.json')).exists():
                            if (d / 'segment.json').exists() and not any(
                                    read(path, 4096)['native_entry_id'] == native
                                    and read(path, 4096)['reason'] in {'missing-stored-source', 'missing-payload', 'cursor-discontinuity'}
                                    for path in _files(d, 'gaps', r'[0-9a-f-]{36}\.json')):
                                continue
                            prior = d / 'snapshots' / (name + '.json')
                            if os.path.lexists(prior) and 'instruction_context' in read(prior):
                                raise ContractError('Instruction context conflicts with retained native gap')
                            capture(d, c, entry, mapping='verified' if native in payload['branch_ids'] else 'absent')
                        continue
                    mention_owner = None
                    ambiguous_mention = False
                    if message.get('role') == 'fileMention':
                        following = None
                        for later in payload['entries'][position + 1:]:
                            later_message = later.get('message', {}) if isinstance(later, dict) else {}
                            if later_message.get('role') in {'assistant', 'toolResult'}:
                                break
                            if later_message.get('role') == 'user' and later_message.get('attribution') == 'user':
                                following = later['id']
                                break
                        if following is not None and following not in known:
                            deferred_mentions.setdefault(following, []).append(entry)
                            continue
                        if following in known:
                            mention_owner = known[following]
                        else:
                            ambiguous_mention = active is None or (active[0] / 'segment.json').exists()
                    call_id = message.get('toolCallId')
                    tool_owner = mention_owner or next((item for item in current if call_id and item[1]['lifecycle'] == tools.get(call_id)), None)
                    ambiguous_call = bool(call_id and (tool_owner is None or len(invocations.get(call_id, set())) > 1
                                                      or len({item[1]['origin']['human_root'] for item in current
                                                              if call_id in item[1]['tool_lifecycles']}) > 1))
                    if ambiguous_call:
                        tool_owner = None
                    content_blocks = message.get('content')
                    invocation_owners = {tools.get(block.get('id')) for block in content_blocks
                                         if isinstance(block, dict) and block.get('type') == 'toolCall' and tools.get(block.get('id'))} if isinstance(content_blocks, list) else set()
                    if tool_owner is None and len(invocation_owners) == 1:
                        tool_owner = next((item for item in current if item[1]['lifecycle'] in invocation_owners), None)
                    if human and not inherited:
                        if active and not (active[0] / 'segment.json').exists():
                            seal(*active)
                        lifecycle = str(uuid.uuid5(uuid.NAMESPACE_URL, str(context.root) + ':omp:' + session + ':' + native))
                        active = new_segment(repo, session, lifecycle, lifecycle,
                                             'human-with-own-guidance' if payload['trigger'] in {'own-hidden-combined', 'human-with-own-guidance'} else 'human',
                                             max([c['event_ordinal'] for _, c in current] + [payload['event_ordinal']]) + 1,
                                             enrollment_context=context)
                    elif active and not tool_owner and (active[0] / 'segment.json').exists():
                        root = active[1]['origin']['human_root']
                        active = new_segment(repo, session, str(uuid.uuid4()), root,
                                             'own-hidden-ambiguous' if payload['trigger'].startswith('own-') else 'foreign-hidden',
                                             max([c['event_ordinal'] for _, c in current] + [payload['event_ordinal']]) + 1,
                                             enrollment_context=context)
                    elif active is None and not tool_owner:
                        # Context inherited on a fork is not a new human admission.
                        if inherited:
                            continue
                        active = new_segment(repo, session, value['lifecycle'], value['lifecycle'], 'resume', payload['event_ordinal'],
                                             enrollment_context=context)
                        active = writable_successor(repo, session, active, current, payload['event_ordinal'], enrollment_context=context)
                        gap(*active, native, 'ambiguous-lifecycle')
                    owner = tool_owner or active
                    if tool_owner and (tool_owner[0] / 'segment.json').exists():
                        root = tool_owner[1]['origin']['human_root']
                        owner = next((item for item in current if item[1]['origin']['human_root'] == root
                                      and not (item[0] / 'segment.json').exists()), None)
                        if owner is None:
                            owner = new_segment(repo, session, str(uuid.uuid4()), root, 'foreign-hidden',
                                                max([c['event_ordinal'] for _, c in current] + [payload['event_ordinal']]) + 1,
                                                enrollment_context=context)
                    snapshot_name = hashlib.sha256(native.encode()).hexdigest() + '.json'
                    existing_owners = snapshots.get(snapshot_name, [])
                    if len(existing_owners) > 1:
                        raise ContractError('Ambiguous retained native snapshot')
                    if existing_owners:
                        owner = existing_owners[0]
                    d, c = owner
                    if stored_user and view['fork_status'] == 'unavailable':
                        if (d / 'segment.json').exists():
                            owner = writable_successor(repo, session, owner, current, payload['event_ordinal'], enrollment_context=context)
                            d, c = owner
                        gap(d, c, native, 'ambiguous-lifecycle')
                    if (ambiguous_mention or ambiguous_call) and not existing_owners:
                        gap(d, c, native, 'ambiguous-lifecycle')
                    if stored_user and native in deferred_mentions:
                        order = (c['control_interval'] or {}).get('capture_order', [])
                        for mention in deferred_mentions.pop(native):
                            capture(d, c, mention)
                            mention_id = mention['id']
                            if mention_id not in order:
                                order.append(mention_id)
                            known[mention_id] = (d, c)
                        c['control_interval'] = {**(c['control_interval'] or {}), 'capture_order': order}
                    invocation = None
                    witness = None
                    candidates = raw_calls.get(call_id, [])
                    if (receipt is not None and not existing_owners and not ambiguous_call
                            and message.get('role') == 'toolResult' and message.get('toolName') == 'read'
                            and message.get('isError') is False and len(candidates) == 1
                            and len(raw_results.get(call_id, [])) == 1
                            and any(block.get('type') == 'toolCall' and block.get('id') == call_id
                                    and block.get('name') == 'read'
                                    and isinstance(block.get('arguments'), dict)
                                    and block['arguments'].get('path') == 'skill://agent-memory'
                                    for block in candidates[0]['message']['content'] if isinstance(block, dict))
                            and call_id not in retained_tool_gaps
                            and len(invocations.get(call_id, set())) == 1
                            and candidates[0].get('id') in invocations[call_id]
                            and candidates[0].get('id') in known
                            and not _retained_gap(current, {native, 'tool:' + call_id})):
                        invocation_owner = known[candidates[0]['id']][0]
                        invocation_path = invocation_owner / 'snapshots' / (hashlib.sha256(candidates[0]['id'].encode()).hexdigest() + '.json')
                        invocation_record = invocation_owner / 'captures' / (hashlib.sha256(candidates[0]['id'].encode()).hexdigest() + '.json')
                        projected_invocation = omp_projection.project(candidates[0]).get('snapshot')
                        if (projected_invocation is not None and os.path.lexists(invocation_record)
                                and os.path.lexists(invocation_path)):
                            retained_invocation = read(invocation_record, 4096)
                            if (retained_invocation.get('native_entry_id') == candidates[0]['id']
                                    and retained_invocation.get('snapshot_ref') == str(invocation_path.relative_to(invocation_owner))
                                    and retained_invocation.get('snapshot_sha256') == digest(projected_invocation)
                                    and projected_invocation == read(invocation_path)):
                                from . import omp_installation
                                witness = omp_installation._owned_manual(receipt)
                                invocation = candidates[0]
                    cap = capture(d, c, entry, mapping='verified' if native in payload['branch_ids'] else 'absent',
                                  invocation=invocation, witness=witness, persisted=view['source_kind'] == 'retired-file')
                    order = (c['control_interval'] or {}).get('capture_order', [])
                    if native not in order:
                        order.append(native)
                    if len(order) > MAX_CAPTURES:
                        gap(d, c, native, 'oversized-source')
                        raise ContractError('Native capture capacity reached')
                    c['control_interval'] = {**(c['control_interval'] or {}), 'capture_order': order}
                    if human and not inherited:
                        c['control_interval']['human_entry'] = native
                    c['last_native_entry'] = native
                    content = message.get('content')
                    if message.get('role') == 'assistant' and isinstance(content, list):
                        for block in content:
                            if isinstance(block, dict) and block.get('type') == 'toolCall':
                                call = turn_sources.bounded(block.get('id'))
                                c['tool_lifecycles'][call] = c['lifecycle']; tools[call] = c['lifecycle']
                                invocations.setdefault(call, set()).add(native)
                    if call_id and cap is not None and not ambiguous_call:
                        resolution_current = current if owner in current else [*current, owner]
                        if resolve_tool_result(resolution_current, owner, cap, invocations):
                            tools.pop(call_id, None)
                    cursor_value(c, session, c['lifecycle'])
                    replace_cursor(d / 'cursor.json', c)
                    if human and not inherited:
                        enroll_admission(d, c, cap)
                    known[native] = owner
                    if owner not in current:
                        current.append(owner)
                if view['tail_status'] == 'active-unavailable':
                    if active is None:
                        active = new_segment(repo, session, value['lifecycle'], value['lifecycle'],
                                             'resume', payload['event_ordinal'], enrollment_context=context)
                    active = writable_successor(repo, session, active, current, payload['event_ordinal'], enrollment_context=context)
                    gap(*active, None, 'ambiguous-lifecycle')
                if active:
                    d, c = active
                    if (value['action'] == 'seal' and c['origin']['kind'] == 'genuine-user') or payload['terminal']:
                        seal(d, c)
                if payload['terminal']:
                    for old_d, old_c in segments(repo, session, enrollment_context=context):
                        if source_bound_origin(old_d, old_c) and not (old_d / 'segment.json').exists():
                            seal(old_d, old_c)
            scope_binding.recheck_private_enrollment(context)
            drain_enrollments(repo, enrollment_context=context)
            current = segments(repo, session, enrollment_context=context)
            rows, faults = pending_inventory(repo, enrollment_context=context)
            chosen = active_segment(current, payload.get('branch_ids'))
            latest = chosen[1] if chosen else None
            root_rows = [r for r in rows if latest and r['session'] == session and r.get('human_root') == latest['origin']['human_root'] and r['status'] != 'completed']
            payload_ids, source_refs, source_complete = _checked_payload_coverage(current, payload, rows, faults)
            envelope_identity = getattr(on_started, 'identity', None)
            if fences and isinstance(envelope_identity, dict):
                from . import omp_stream
                native_ids = sorted(item['id'] for item in view['native_parents'])
                token = None
                if (view['source_kind'] == 'retired-file' and source_complete
                        and all(entry.get('type') in NONLEARNING for entry in payload['entries'])):
                    token = omp_stream._nonlearning_tail_evidence(session, envelope_identity['view_id'], native_ids)
                with scope_binding._fresh_enrollment_locations():
                    omp_stream.resolve_recovery(repo, fences, view_id=envelope_identity['view_id'],
                        native_ids=native_ids, tool_ids=envelope_identity['tool_ids'],
                        payload_native_ids=payload_ids, source_references=source_refs,
                        complete=view['source_kind'] == 'retired-file' and source_complete,
                        leaf_id=view['leaf_id'], nonlearning_evidence=token)
            result = {'verified': True, 'status': 'pending' if root_rows or faults else 'observed',
                      'session': session, 'lifecycle': latest['lifecycle'] if latest else value['lifecycle'],
                      'wake_payload': {'observation_ids': sorted(r['observation_id'] for r in root_rows if 'observation_id' in r),
                                       'enrollment_refs': sorted(r['source_reference'] for r in root_rows if 'observation_id' not in r and r['source_reference'].startswith(('omp:capture:', 'omp:seal:')))},
                      'faults': faults, 'held': context.held, 'canonical_writes_performed': False}
            result['source_references'] = source_refs
            result['source_coverage_complete'] = view['source_kind'] == 'retired-file' and source_complete
            if value['action'] == 'seal' and payload['request_wake'] and latest:
                try:
                    result.update(wake(repo, session, latest['lifecycle'], result['wake_payload'], delivery_window=delivery_window,
                                       enrollment_context=context))
                except Exception:
                    # The source transaction is verified independently of wake preparation.
                    result.update({'continue': False, 'status': 'pending-wake-unavailable'})
            return result
