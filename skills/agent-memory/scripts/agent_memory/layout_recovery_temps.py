"""Recognize only journal-derived temporary names and exact reviewed payloads."""
from pathlib import Path
import json
import os
import stat
import sys
import uuid
from . import layout_recovery_state as state
from . import layout_recovery_observation as raw, bank_observation, bank_writes as writes, sharing_limits
from .developer_identity import digest, encoded, uuid_valid
from .frontmatter import ContractError


def source_name(operation_id, path):
    return str(Path(path).parent / ('.agent-memory-layout-' + operation_id + '-' + digest(path.encode())[:16] + '.tmp'))


def entries(bank, operation_id, actions, base_view, final_facts, marker_payloads):
    base = {row['path']: row for row in base_view}
    candidates = {}
    bank_stat = bank.stat()
    inherited_gid = (bank_stat.st_gid
                     if sys.platform != 'linux' or bank_stat.st_mode & stat.S_ISGID else os.getegid())
    for action in actions:
        path = action['path']
        values = []
        for sha, facts in ((action.get('before'), base.get(path)), (action.get('after'), final_facts.get(path))):
            if sha is not None and facts is not None:
                values.append((sha, facts['uid'], facts['gid'], stat.S_IMODE(facts['mode']), facts['xattrs']))
        candidates[source_name(operation_id, path)] = values
    for payload in marker_payloads:
        identity = json.loads(payload)['operation_id']
        path = '.agent-memory-layout-marker-' + identity + '.tmp'
        candidates.setdefault(path, []).append((digest(payload), os.geteuid(), inherited_gid, 0o600, None))
    result = []
    for path, allowed in sorted(candidates.items()):
        target = bank / path
        if not target.exists() and not target.is_symlink():
            continue
        body, facts = raw.read_source(bank, path)
        xattrs = raw.metadata_supported(target)
        if not any((digest(body), facts['uid'], facts['gid'], stat.S_IMODE(facts['mode'])) == item[:4]
                   and (item[4] is None or xattrs == item[4]) for item in allowed):
            raise ContractError('Unknown, partial or changed transaction temporary; preserve it for explicit inspection')
        result.append({'path': path, 'sha256': digest(body), 'facts': facts, 'xattrs': xattrs})
    return result


def clean(bank, pins, owner, transaction):
    for pin in pins:
        bank_observation.markers(bank, owner, transaction)
        body, facts = raw.read_source(bank, pin['path'])
        if digest(body) != pin['sha256'] or facts != pin['facts'] or raw.metadata_supported(bank / pin['path']) != pin['xattrs']:
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


def probe_plan(operation_id, requirements, directories, bank=None):
    """Derive every probe name before any bank mutation, including shadow parents."""
    if not uuid_valid(operation_id):
        raise ContractError('Invalid metadata probe operation identity')
    planned = dict(directories)

    def facts(value):
        if (not isinstance(value, dict) or set(value) != {'uid', 'gid', 'mode', 'xattrs'}
                or any(type(value[key]) is not int or value[key] < 0 for key in ('uid', 'gid', 'mode'))
                or value['mode'] > 0o7777 or not raw.valid_xattrs(value['xattrs'])):
            raise ContractError('Invalid metadata probe facts')
        return value

    for path, value in planned.items():
        raw.path_value(path)
        facts(value)
    tasks = []

    def route(target, value, kind):
        raw.path_value(target)
        facts(value)
        parent = Path(target).parent
        ancestors = [str(Path(*parent.parts[:i])) for i in range(1, len(parent.parts) + 1)]
        candidates = [path for path in ancestors if path in planned]
        if kind == 'directory':
            candidates.append(target)
        routes = []
        if kind == 'file':
            routes.append([])
        for first in candidates:
            first_parent = Path(first).parent
            shadow_parent = first_parent
            shadow = []
            chain = [path for path in candidates if path == first or Path(first) in Path(path).parents]
            for directory in chain:
                name = '.agent-memory-probe-' + operation_id + '-' + digest(encoded(
                    {'directory': directory, 'parent': str(shadow_parent)}))[:20]
                shadow_parent = shadow_parent / name
                shadow.append({'path': str(shadow_parent), 'type': 'directory',
                               'target': directory, 'facts': planned[directory]})
            routes.append(shadow)
        result = []
        for shadow in routes:
            nodes = list(shadow)
            if kind == 'file':
                destination = Path(shadow[-1]['path']) if shadow else parent
                name = '.agent-memory-probe-' + operation_id + '-' + digest(encoded(
                    {'target': target, 'facts': value, 'parent': str(destination)}))[:20]
                nodes.append({'path': str(destination / name), 'type': 'file',
                              'target': target, 'facts': value})
            result.append(nodes)
        return {'type': kind, 'target': target, 'facts': value, 'routes': result}

    for path, value in sorted(planned.items()):
        tasks.append(route(path, value, 'directory'))
    for requirement in sorted(requirements, key=encoded):
        if not isinstance(requirement, dict) or set(requirement) != {'path', 'facts'}:
            raise ContractError('Invalid metadata probe requirement')
        tasks.append(route(requirement['path'], requirement['facts'], 'file'))
    plan = {'operation_id': operation_id, 'directories': planned, 'tasks': tasks}
    sharing_limits.private_bytes(plan)
    return plan


def _probe_validate(plan):
    try:
        requirements = [{'path': task['target'], 'facts': task['facts']}
                        for task in plan['tasks'] if task['type'] == 'file']
        expected = probe_plan(plan['operation_id'], requirements, plan['directories'])
    except (KeyError, TypeError, ValueError) as exc:
        raise ContractError('Invalid metadata probe journal') from exc
    if plan != expected:
        raise ContractError('Metadata probe journal differs from its deterministic plan')


def validate_probe_plan(plan, requirements, directories):
    if plan != probe_plan(plan['operation_id'], requirements, directories):
        raise ContractError('Metadata probe plan differs from authorized requirements')


def _probe_nodes(plan, requirements=None, directories=None):
    _probe_validate(plan)
    selected = None if requirements is None else {encoded(item) for item in requirements}
    selected_dirs = None if directories is None else set(directories)
    available = {encoded({'path': task['target'], 'facts': task['facts']})
                 for task in plan['tasks'] if task['type'] == 'file'}
    if selected is not None and not selected <= available:
        raise ContractError('Metadata probe selection is outside the journal')
    if selected_dirs is not None and not selected_dirs <= set(plan['directories']):
        raise ContractError('Metadata directory probe selection is outside the journal')
    nodes = {}
    for task in plan['tasks']:
        if task['type'] == 'file':
            if selected is not None and encoded({'path': task['target'], 'facts': task['facts']}) not in selected:
                continue
        elif selected_dirs is not None and task['target'] not in selected_dirs:
            continue
        for route in task['routes']:
            if selected_dirs is not None and any(node['type'] == 'directory' and
                                                 node['target'] not in selected_dirs for node in route):
                continue
            for node in route:
                previous = nodes.setdefault(node['path'], node)
                if previous != node:
                    raise ContractError('Conflicting metadata probe names')
    return sorted(nodes.values(), key=lambda node: (len(Path(node['path']).parts), node['path']))


def _probe_parent(bank, path):
    descriptors = [os.open(bank, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)]
    try:
        for part in Path(path).parts[:-1]:
            descriptors.append(os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                       dir_fd=descriptors[-1]))
        _probe_bound(bank, path, descriptors)
        return descriptors
    except BaseException:
        for descriptor in reversed(descriptors):
            os.close(descriptor)
        raise


def _probe_bound(bank, path, descriptors):
    root = Path(bank).lstat()
    opened = os.fstat(descriptors[0])
    if (root.st_dev, root.st_ino) != (opened.st_dev, opened.st_ino):
        raise ContractError('Metadata probe root binding changed')
    for index, part in enumerate(Path(path).parts[:-1], 1):
        named = os.stat(part, dir_fd=descriptors[index - 1], follow_symlinks=False)
        opened = os.fstat(descriptors[index])
        if not stat.S_ISDIR(named.st_mode) or (named.st_dev, named.st_ino) != (opened.st_dev, opened.st_ino):
            raise ContractError('Metadata probe parent binding changed')


class ProbeOwnershipError(ContractError):
    def __init__(self, path):
        super().__init__('pending-ambiguous-probe-ownership')
        self.probe_path = path


def _probe_xattrs(fd, path):
    if raw.sys.platform != 'darwin':
        if not hasattr(os, 'listxattr'):
            raise ProbeOwnershipError(path)
        values = {name: os.getxattr(fd, name) for name in os.listxattr(fd)}
    else:
        ctypes = raw.ctypes
        libc = ctypes.CDLL(None, use_errno=True)
        libc.flistxattr.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int]
        libc.flistxattr.restype = ctypes.c_ssize_t
        libc.fgetxattr.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_size_t,
                                  ctypes.c_uint32, ctypes.c_int]
        libc.fgetxattr.restype = ctypes.c_ssize_t
        size = libc.flistxattr(fd, None, 0, 0)
        if size < 0 or size > 16384:
            raise ProbeOwnershipError(path)
        names = ctypes.create_string_buffer(size)
        if size and libc.flistxattr(fd, names, size, 0) != size:
            raise ProbeOwnershipError(path)
        values = {}
        for name in names.raw.split(b'\0')[:-1] if size else []:
            count = libc.fgetxattr(fd, name, None, 0, 0, 0)
            if count < 0 or count > 16384:
                raise ProbeOwnershipError(path)
            body = ctypes.create_string_buffer(count)
            if libc.fgetxattr(fd, name, body, count, 0, 0) != count:
                raise ProbeOwnershipError(path)
            try:
                decoded = name.decode('utf-8')
            except UnicodeDecodeError as exc:
                raise ProbeOwnershipError(path) from exc
            values[decoded] = body.raw
    if any(len(value) > 16384 for value in values.values()):
        raise ProbeOwnershipError(path)
    return {name: value.hex() for name, value in sorted(values.items())}


def _probe_pin(bank, node):
    descriptors = _probe_parent(bank, node['path'])
    fd = None
    try:
        fd = os.open(Path(node['path']).name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                     dir_fd=descriptors[-1])
        info = os.fstat(fd)
        if ((node['type'] == 'file' and (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size != 32))
                or (node['type'] == 'directory' and not stat.S_ISDIR(info.st_mode))):
            raise ProbeOwnershipError(node['path'])
        nonce = os.read(fd, 33) if node['type'] == 'file' else None
        if nonce is not None and len(nonce) != 32:
            raise ProbeOwnershipError(node['path'])
        attrs = _probe_xattrs(fd, node['path'])
        named = os.stat(Path(node['path']).name, dir_fd=descriptors[-1], follow_symlinks=False)
        _probe_bound(bank, node['path'], descriptors)
        if raw.signature(named) != raw.signature(info) or raw.signature(os.fstat(fd)) != raw.signature(info):
            raise ProbeOwnershipError(node['path'])
        parent = os.fstat(descriptors[-1])
        return {'path': node['path'], 'type': node['type'], 'facts': raw.signature(info),
                'flags': getattr(info, 'st_flags', 0), 'xattrs': attrs,
                'parent': {'dev': parent.st_dev, 'ino': parent.st_ino},
                'nonce_sha256': digest(nonce) if nonce is not None else None}
    finally:
        if fd is not None:
            os.close(fd)
        for descriptor in reversed(descriptors):
            os.close(descriptor)


def _pin_matches(actual, expected, *, directory_children=False):
    if any(actual[key] != expected[key] for key in ('path', 'type', 'flags', 'xattrs', 'parent', 'nonce_sha256')):
        return False
    keys = ('dev', 'ino', 'mode', 'uid', 'gid') if directory_children and actual['type'] == 'directory' else (
        ('dev', 'ino', 'mode', 'uid', 'gid', 'nlink') if actual['type'] == 'directory' else tuple(actual['facts']))
    return all(actual['facts'][key] == expected['facts'][key] for key in keys)


def _attempt_budget(base):
    """Bound and pin all attempt evidence before loading any record."""
    if not os.path.lexists(base):
        return {}
    found, total = {}, 0

    def visit(path, level):
        nonlocal total
        info = path.lstat()
        directory = level < 2 or (level == 2 and path.name == 'pins')
        if (directory and not stat.S_ISDIR(info.st_mode)) or (not directory and not stat.S_ISREG(info.st_mode)):
            raise ContractError('Unknown metadata probe attempt evidence')
        found[str(path)] = raw.signature(info)
        if len(found) > raw.MAX_ENTRIES:
            raise ContractError('Metadata probe attempt capacity exceeded')
        if not directory:
            total += info.st_size
            if total > sharing_limits.MAX_PRIVATE:
                raise ContractError('Metadata probe attempt capacity exceeded')
            return
        with os.scandir(path) as entries:
            for entry in entries:
                name = entry.name
                if level == 0:
                    valid = uuid_valid(name)
                elif level == 1:
                    valid = name in {'plan.json', 'completion.json', 'pins'}
                else:
                    prefix = '.agent-memory-probe-'
                    valid = name.startswith(prefix) and name.endswith('.json') and uuid_valid(name[len(prefix):-5])
                if not valid:
                    raise ContractError('Unknown metadata probe attempt evidence')
                visit(path / name, level + 1)

    visit(Path(base), 0)
    return found


def _attempt_store(directory, path, value):
    base = Path(directory) / 'metadata-probe-attempts'
    snapshot = _attempt_budget(base)
    body = sharing_limits.private_bytes(value)
    if not os.path.lexists(path):
        missing, parent = 1, path.parent
        while str(parent) not in snapshot and parent != base.parent:
            missing += 1
            parent = parent.parent
        total = sum(info['size'] for name, info in snapshot.items() if stat.S_ISREG(info['mode']))
        if len(snapshot) + missing > raw.MAX_ENTRIES or total + len(body) > sharing_limits.MAX_PRIVATE:
            raise ContractError('Metadata probe attempt capacity exceeded')
    state.write_once(path, value)


def _attempt(bank, plan, directory, requirements, directories):
    templates = _probe_nodes(plan, requirements, directories)
    indexes = {node['path']: index for index, node in enumerate(_probe_nodes(plan))}
    attempt_id = str(uuid.uuid4())
    mapped, nodes = {}, []
    for template in templates:
        old_parent = str(Path(template['path']).parent)
        parent_ref = mapped.get(old_parent)
        parent = parent_ref['path'] if parent_ref else old_parent
        path = str(Path(parent) / ('.agent-memory-probe-' + str(uuid.uuid4())))
        parent_pin = None
        if parent_ref is None:
            try:
                descriptors = _probe_parent(bank, path)
            except FileNotFoundError:
                pass
            else:
                try:
                    info = os.fstat(descriptors[-1])
                    parent_pin = {'dev': info.st_dev, 'ino': info.st_ino}
                finally:
                    for descriptor in reversed(descriptors):
                        os.close(descriptor)
        node = {**template, 'template': template['path'], 'path': path, 'index': indexes[template['path']],
                'parent_ref': parent_ref['path'] if parent_ref else None, 'parent_pin': parent_pin,
                'nonce': os.urandom(32).hex() if template['type'] == 'file' else None}
        mapped[template['path']] = node
        nodes.append(node)
    value = {'schema': 'agent-memory/layout-metadata-probe-attempt-v1', 'operation_id': plan['operation_id'],
             'plan_sha256': digest(encoded(plan)), 'attempt_id': attempt_id,
             'requirements': requirements, 'directories': directories, 'nodes': nodes}
    attempt_dir = Path(directory) / 'metadata-probe-attempts' / attempt_id
    _attempt_store(directory, attempt_dir / 'plan.json', value)
    writes.fault('layout-recovery-probe-after-attempt-journal')
    return attempt_dir, value


def _attempt_value(path, plan):
    value = state.read(path / 'plan.json')
    if (set(value) != {'schema', 'operation_id', 'plan_sha256', 'attempt_id', 'requirements', 'directories', 'nodes'}
            or value['schema'] != 'agent-memory/layout-metadata-probe-attempt-v1'
            or value['operation_id'] != plan['operation_id'] or value['plan_sha256'] != digest(encoded(plan))
            or value['attempt_id'] != path.name or not uuid_valid(path.name)):
        raise ContractError('Invalid metadata probe attempt')
    templates = _probe_nodes(plan, value['requirements'], value['directories'])
    indexes = {node['path']: index for index, node in enumerate(_probe_nodes(plan))}
    if len(templates) != len(value['nodes']):
        raise ContractError('Metadata probe attempt is outside its authorized plan')
    mapped, seen = {}, set()
    for template, node in zip(templates, value['nodes']):
        if set(node) != set(template) | {'template', 'index', 'parent_ref', 'parent_pin', 'nonce'}:
            raise ContractError('Invalid metadata probe attempt node')
        old_parent = str(Path(template['path']).parent)
        reference = mapped.get(old_parent)
        parent = reference if reference else old_parent
        name = Path(node['path']).name
        if (node['template'] != template['path'] or node['type'] != template['type']
                or node['target'] != template['target'] or node['facts'] != template['facts']
                or node['index'] != indexes[template['path']] or str(Path(node['path']).parent) != parent
                or not name.startswith('.agent-memory-probe-') or not uuid_valid(name[len('.agent-memory-probe-'):])
                or node['path'] in seen or node['parent_ref'] != reference):
            raise ContractError('Metadata probe attempt paths changed')
        if node['type'] == 'file':
            try:
                nonce = bytes.fromhex(node['nonce'])
            except (TypeError, ValueError) as exc:
                raise ContractError('Invalid metadata probe nonce') from exc
            if len(nonce) != 32 or nonce.hex() != node['nonce']:
                raise ContractError('Invalid metadata probe nonce')
        elif node['nonce'] is not None:
            raise ContractError('Invalid metadata directory probe nonce')
        if reference and node['parent_pin'] is not None:
            raise ContractError('Invalid metadata probe parent reference')
        if node['parent_pin'] is not None and (not isinstance(node['parent_pin'], dict)
                or set(node['parent_pin']) != {'dev', 'ino'} or
                any(type(item) is not int for item in node['parent_pin'].values())):
            raise ContractError('Invalid metadata probe parent pin')
        seen.add(node['path'])
        mapped[template['path']] = node['path']
    names = {entry.name for entry in path.iterdir()}
    if not names <= {'plan.json', 'pins', 'completion.json'}:
        raise ContractError('Unknown metadata probe attempt evidence')
    completion = state.read(path / 'completion.json', missing=True)
    if completion is not None and completion != {
            'schema': 'agent-memory/layout-metadata-probe-completion-v1',
            'operation_id': plan['operation_id'], 'attempt_id': value['attempt_id'],
            'plan_sha256': digest(encoded(value))}:
        raise ContractError('Invalid metadata probe completion')
    pins_dir = path / 'pins'
    if pins_dir.exists():
        if pins_dir.is_symlink() or not pins_dir.is_dir():
            raise ContractError('Invalid metadata probe pin directory')
        entries = list(pins_dir.iterdir())
        expected_names = {Path(node['path']).name + '.json': node for node in value['nodes']}
        if len(entries) > raw.MAX_ENTRIES or any(entry.name not in expected_names for entry in entries):
            raise ContractError('Unknown metadata probe pin evidence')
        for entry in entries:
            node = expected_names[entry.name]
            try:
                pin = state.read(entry)
            except (ContractError, OSError) as exc:
                raise ProbeOwnershipError(node['path']) from exc
            if (not isinstance(pin, dict) or set(pin) != {'path', 'type', 'facts', 'flags', 'xattrs', 'parent', 'nonce_sha256'}
                    or pin['path'] != node['path'] or pin['type'] != node['type']
                    or not isinstance(pin['facts'], dict) or not isinstance(pin['parent'], dict)
                    or set(pin['parent']) != {'dev', 'ino'} or not isinstance(pin['xattrs'], dict)
                    or pin['nonce_sha256'] != (digest(bytes.fromhex(node['nonce'])) if node['nonce'] else None)):
                raise ProbeOwnershipError(node['path'])
    return value



def _progress_path(attempt_dir, node):
    return attempt_dir / 'pins' / (Path(node['path']).name + '.json')


def probe_entries(bank, plan, directory):
    base = Path(directory) / 'metadata-probe-attempts'
    snapshot = _attempt_budget(base)
    if not snapshot:
        return []
    attempts = sorted(path for path in base.iterdir())
    result = []
    for attempt_dir in attempts:
        attempt = _attempt_value(attempt_dir, plan)
        nodes = attempt['nodes']
        known = {node['path'] for node in nodes}
        by_path = {node['path']: node for node in nodes}
        for node in nodes:
            record = _progress_path(attempt_dir, node)
            try:
                actual = _probe_pin(bank, node)
            except FileNotFoundError:
                continue
            except OSError as exc:
                raise ProbeOwnershipError(node['path']) from exc
            try:
                pin = state.read(record, missing=True)
                if pin is None or not _pin_matches(actual, pin, directory_children=True):
                    raise ProbeOwnershipError(node['path'])
                parent_pin = node['parent_pin']
                if node['parent_ref']:
                    parent = state.read(_progress_path(attempt_dir, by_path[node['parent_ref']]), missing=True)
                    parent_pin = ({key: parent['facts'][key] for key in ('dev', 'ino')} if parent else None)
            except (ContractError, KeyError, TypeError, ValueError) as exc:
                raise ProbeOwnershipError(node['path']) from exc
            if pin['parent'] != parent_pin:
                raise ProbeOwnershipError(node['path'])
            if node['type'] == 'file' and pin['nonce_sha256'] != digest(bytes.fromhex(node['nonce'])):
                raise ProbeOwnershipError(node['path'])
            if node['type'] == 'directory':
                descriptors = _probe_parent(bank, node['path'])
                fd = None
                try:
                    fd = os.open(Path(node['path']).name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                 dir_fd=descriptors[-1])
                    for name in os.listdir(fd):
                        if str(Path(node['path']) / name) not in known:
                            raise ProbeOwnershipError(str(Path(node['path']) / name))
                    _probe_bound(bank, node['path'], descriptors)
                    info = os.fstat(fd)
                    if (info.st_dev, info.st_ino) != (pin['facts']['dev'], pin['facts']['ino']):
                        raise ProbeOwnershipError(node['path'])
                finally:
                    if fd is not None:
                        os.close(fd)
                    for descriptor in reversed(descriptors):
                        os.close(descriptor)
            result.append({**pin, 'record': str(record), 'index': node['index']})
    if _attempt_budget(base) != snapshot:
        raise ContractError('Metadata probe attempt evidence changed during observation')
    return result


def probe_clean(bank, pins, owner, transaction):
    for pin in sorted(pins, key=lambda row: (-len(Path(row['path']).parts), row['path'])):
        bank_observation.markers(bank, owner, transaction)
        record = state.read(Path(pin['record']))
        expected = {key: value for key, value in pin.items() if key not in {'record', 'index'}}
        if record != expected:
            raise ProbeOwnershipError(pin['path'])
        try:
            actual = _probe_pin(bank, pin)
        except OSError as exc:
            raise ProbeOwnershipError(pin['path']) from exc
        if not _pin_matches(actual, expected):
            raise ProbeOwnershipError(pin['path'])
        descriptors = _probe_parent(bank, pin['path'])
        try:
            _probe_bound(bank, pin['path'], descriptors)
            info = os.stat(Path(pin['path']).name, dir_fd=descriptors[-1], follow_symlinks=False)
            if (info.st_dev, info.st_ino) != (pin['facts']['dev'], pin['facts']['ino']):
                raise ProbeOwnershipError(pin['path'])
            if pin['type'] == 'directory':
                os.rmdir(Path(pin['path']).name, dir_fd=descriptors[-1])
            else:
                os.unlink(Path(pin['path']).name, dir_fd=descriptors[-1])
            os.fsync(descriptors[-1])
            writes.fault('layout-recovery-probe-removed:' + str(pin['index']))
        finally:
            for descriptor in reversed(descriptors):
                os.close(descriptor)


def probe_complete(bank, plan, directory):
    if probe_entries(bank, plan, directory):
        raise ContractError('Metadata probes remain pending')
    base = Path(directory) / 'metadata-probe-attempts'
    if not base.exists():
        return
    for attempt_dir in sorted(base.iterdir()):
        attempt = _attempt_value(attempt_dir, plan)
        completion = {'schema': 'agent-memory/layout-metadata-probe-completion-v1',
                      'operation_id': plan['operation_id'], 'attempt_id': attempt['attempt_id'],
                      'plan_sha256': digest(encoded(attempt))}
        _attempt_store(directory, attempt_dir / 'completion.json', completion)


def _probe_clean_live(bank, node, descriptors, fd, original, owner, transaction):
    try:
        bank_observation.markers(bank, owner, transaction)
        _probe_bound(bank, node['path'], descriptors)
        info = os.fstat(fd)
        named = os.stat(Path(node['path']).name, dir_fd=descriptors[-1], follow_symlinks=False)
        if (info.st_dev, info.st_ino) != (original.st_dev, original.st_ino) or raw.signature(info) != raw.signature(named):
            raise ProbeOwnershipError(node['path'])
        if node['type'] == 'file':
            body = bytes.fromhex(node['nonce'])
            observed = os.pread(fd, 33, 0)
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or observed != body[:info.st_size] or info.st_size > len(body):
                raise ProbeOwnershipError(node['path'])
        elif not stat.S_ISDIR(info.st_mode) or os.listdir(fd):
            raise ProbeOwnershipError(node['path'])
        if raw.signature(os.fstat(fd)) != raw.signature(info):
            raise ProbeOwnershipError(node['path'])
        if node['type'] == 'directory':
            os.rmdir(Path(node['path']).name, dir_fd=descriptors[-1])
        else:
            os.unlink(Path(node['path']).name, dir_fd=descriptors[-1])
        os.fsync(descriptors[-1])
    except OSError as exc:
        raise ProbeOwnershipError(node['path']) from exc


def probe(bank, plan, directory, requirements=None, directories=None, *, owner, transaction):
    if probe_entries(bank, plan, directory):
        raise ContractError('Interrupted metadata probes require checked cleanup')
    attempt_dir, attempt = _attempt(bank, plan, directory, requirements, directories)
    covered_files, covered_directories = set(), set()
    required_files = ({encoded(item) for item in requirements} if requirements is not None else
                      {encoded({'path': task['target'], 'facts': task['facts']})
                       for task in plan['tasks'] if task['type'] == 'file'})
    required_directories = set(plan['directories']) if directories is None else set(directories)
    try:
        by_path = {node['path']: node for node in attempt['nodes']}
        for node in attempt['nodes']:
            parent_pin = node['parent_pin']
            if node['parent_ref']:
                parent_record = state.read(_progress_path(attempt_dir, by_path[node['parent_ref']]), missing=True)
                if parent_record is None:
                    continue
                parent_pin = {key: parent_record['facts'][key] for key in ('dev', 'ino')}
            if parent_pin is None:
                continue
            descriptors = _probe_parent(bank, node['path'])
            try:
                bank_observation.markers(bank, owner, transaction)
                _probe_bound(bank, node['path'], descriptors)
                parent = os.fstat(descriptors[-1])
                if {'dev': parent.st_dev, 'ino': parent.st_ino} != parent_pin:
                    raise ProbeOwnershipError(node['path'])
                name = Path(node['path']).name
                try:
                    if node['type'] == 'directory':
                        os.mkdir(name, node['facts']['mode'], dir_fd=descriptors[-1])
                        fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptors[-1])
                    else:
                        fd = os.open(name, os.O_CREAT | os.O_EXCL | os.O_RDWR | os.O_NOFOLLOW,
                                     node['facts']['mode'], dir_fd=descriptors[-1])
                except FileExistsError as exc:
                    raise ProbeOwnershipError(node['path']) from exc
                pinned = False
                original = None
                try:
                    original = os.fstat(fd)
                    if node['type'] == 'file':
                        body = bytes.fromhex(node['nonce'])
                        written = 0
                        while written < len(body):
                            count = os.write(fd, body[written:])
                            if count <= 0:
                                raise OSError('Unable to write metadata probe nonce')
                            written += count
                        os.fsync(fd)
                    created = os.fstat(fd)
                    named = os.stat(name, dir_fd=descriptors[-1], follow_symlinks=False)
                    if raw.signature(created) != raw.signature(named):
                        raise ProbeOwnershipError(node['path'])
                    os.fsync(descriptors[-1])
                    writes.fault('layout-recovery-probe-before-pin:' + str(node['index']))
                    pin = _probe_pin(bank, node)
                    if (pin['parent'] != parent_pin or
                            (pin['facts']['dev'], pin['facts']['ino']) != (created.st_dev, created.st_ino) or
                            (node['type'] == 'file' and pin['nonce_sha256'] != digest(bytes.fromhex(node['nonce'])))):
                        raise ProbeOwnershipError(node['path'])
                    _attempt_store(directory, _progress_path(attempt_dir, node), pin)
                    pinned = True
                    writes.fault('layout-recovery-probe-after-pin:' + str(node['index']))
                except BaseException:
                    if not pinned:
                        if original is None:
                            raise ProbeOwnershipError(node['path'])
                        _probe_clean_live(bank, node, descriptors, fd, original, owner, transaction)
                    raise
                finally:
                    os.close(fd)
            finally:
                for descriptor in reversed(descriptors):
                    os.close(descriptor)
            actual = {'uid': pin['facts']['uid'], 'gid': pin['facts']['gid'],
                      'mode': stat.S_IMODE(pin['facts']['mode']),
                      'xattrs': raw.metadata_supported(bank / node['path'])}
            if actual != node['facts']:
                raise ContractError('pending-metadata-preservation')
            if node['type'] == 'directory':
                covered_directories.add(node['target'])
            else:
                covered_files.add(encoded({'path': node['target'], 'facts': node['facts']}))
        if not required_files <= covered_files or not required_directories <= covered_directories:
            raise ContractError('Metadata probe parent is unavailable')
    finally:
        original_error = raw.sys.exc_info()[1]
        try:
            pins = probe_entries(bank, plan, directory)
            if pins:
                probe_clean(bank, pins, owner, transaction)
            probe_complete(bank, plan, directory)
            writes.fault('layout-recovery-probe-directories-removed')
        except ProbeOwnershipError:
            raise
        except BaseException:
            if original_error is None:
                raise
