"""Explicit omp targets and receipt-v2 validation; never infer a native root."""
from contextlib import contextmanager
import json
import re
from pathlib import Path
from .frontmatter import ContractError

SCHEMAS = {'agent-memory/install-receipt-v1', 'agent-memory/install-receipt-v2'}
FIELDS = {'agent_dir', 'skill_target', 'extension_target', 'activation_target',
          'peer_reader_capability', 'rollout_attested'}
READER_CAPABILITY = 'omp-stored-source-v2'
HISTORICAL_READER_CAPABILITY = 'omp-harness-v1'


def binding(agent_dir, *, no_guidance=False, reader_capability=READER_CAPABILITY):
    if not isinstance(agent_dir, str) or not Path(agent_dir).is_absolute() or '..' in Path(agent_dir).parts:
        raise ContractError('Explicit --omp-agent-dir must be an absolute canonical root')
    root = Path(agent_dir)
    if root.resolve() != root or any(p.is_symlink() for p in (root, *root.parents)):
        raise ContractError('omp agent root must not contain symlinks')
    if reader_capability not in {HISTORICAL_READER_CAPABILITY, READER_CAPABILITY}:
        raise ContractError('Unknown native reader capability')
    return {'agent_dir': str(root), 'skill_target': str(root / 'skills' / 'agent-memory'),
            'extension_target': str(root / 'extensions' / 'agent-memory.ts'),
            'activation_target': None if no_guidance else str(root / 'AGENTS.md'),
            'peer_reader_capability': reader_capability, 'rollout_attested': True}


def configuration(args, old, selected, no_guidance):
    previous = old.get('omp_binding') if old else None
    explicit = getattr(args, 'omp_agent_dir', None)
    enabled = 'omp' in selected.split(',')
    if previous and not enabled:
        raise ContractError('Removing saved omp selection requires explicit owned uninstall/reinstall; all excludes omp')
    if not enabled:
        if explicit or getattr(args, 'omp_peers_upgraded', False) or getattr(args, 'omp_layout_quiescence', None):
            raise ContractError('omp root/rollout attestation requires an explicit omp preset')
        return None
    if previous:
        if explicit and explicit != previous['agent_dir']:
            raise ContractError('omp root differs from saved ownership; uninstall/reinstall explicitly')
        capability = previous.get('peer_reader_capability')
        if previous != binding(previous['agent_dir'], no_guidance=no_guidance, reader_capability=capability):
            raise ContractError('omp saved binding changed; preserve owned targets')
        if capability != READER_CAPABILITY and not getattr(args, 'omp_peers_upgraded', False):
            raise ContractError('Stored-source authority upgrade requires renewed --omp-peers-upgraded for omp-stored-source-v2')
        return binding(previous['agent_dir'], no_guidance=no_guidance)
    if not explicit or not getattr(args, 'omp_peers_upgraded', False):
        raise ContractError('Native omp requires --presets omp, --omp-agent-dir and --omp-peers-upgraded after quiescing/upgrading bank peers')
    return binding(explicit, no_guidance=no_guidance)


def validate_receipt(receipt):
    if receipt.get('schema') not in SCHEMAS:
        raise ContractError('Unknown installation receipt schema')
    selected = receipt.get('presets', '').split(',')
    value = receipt.get('omp_binding')
    if ('omp' in selected) != (value is not None):
        raise ContractError('omp preset and receipt binding differ')
    if ('omp' in receipt.get('enabled_adapters', [])) != (value is not None):
        raise ContractError('Native enabled adapter differs from ownership binding')
    if value is None:
        if receipt['schema'] != 'agent-memory/install-receipt-v1':
            raise ContractError('Non-omp installs retain receipt-v1')
        return
    if receipt['schema'] != 'agent-memory/install-receipt-v2' or not isinstance(value, dict) or set(value) != FIELDS:
        raise ContractError('Malformed native ownership receipt')
    if (type(value['rollout_attested']) is not bool or value['rollout_attested'] is not True
            or not re.fullmatch(r'[0-9a-f]{32}', receipt['installation_id'])):
        raise ContractError('Invalid native rollout/installation identity')
    if value != binding(value['agent_dir'], no_guidance=receipt.get('no_guidance', False),
                        reader_capability=value['peer_reader_capability']):
        raise ContractError('Native ownership targets differ from canonical saved binding')
    targets = {target['path']: target for target in receipt['targets']}
    for field in ('skill_target', 'extension_target', 'activation_target'):
        path = value[field]
        if path is not None and path not in targets:
            raise ContractError('Native target is absent from ownership receipt')
    extension = targets[value['extension_target']]
    if extension['kind'] != 'wrapper' or extension.get('artifact_digest') != receipt['source']['artifact_digest']:
        raise ContractError('Native extension artifact identity differs from owned runtime')
    skill = targets[value['skill_target']]
    if skill['kind'] not in {'copy', 'link'} or skill.get('artifact_digest') != receipt['source']['artifact_digest']:
        raise ContractError('Native skill target differs from installed artifact')
    if value['activation_target'] is not None and targets[value['activation_target']]['kind'] != 'activation':
        raise ContractError('Native activation target differs from owned Markdown block')


def extension_wrapper(active_pack, artifact_digest, value):
    source = json.dumps(str(active_pack / 'hooks' / 'omp.ts'))
    options = json.dumps({'artifact': artifact_digest, 'agentDir': value['agent_dir'],
                          'readerCapability': value['peer_reader_capability'],
                          'rolloutAttested': True}, separators=(',', ':'))
    return ('// Receipt-owned native executable. Never edit provider or approval settings.\n'
            'import nativeMemory from ' + source + ';\n'
            'export default pi => nativeMemory(pi, ' + options + ');\n').encode()


def preview(value):
    return {'binding': value, 'executable_authority': 'stored native source observation and bounded advisory wakes only',
            'canonical_operations': 'ordinary approved native tools; no callback mutation or permission bypass',
            'private_retention': 'native stored user/tool plaintext after shared screening; immutable evidence retained on uninstall; no automatic purge or global quota',
            'compatibility_limit': 'unresolved source gaps/registration faults can require omp-aware readers indefinitely; old/manual/remote binaries outside local inventory are not fenced',
            'qualification': 'disk ownership is not native loading, source completion or platform/provider acceptance'}


def _answer_first_qualification(receipt):
    from . import installation
    selected = receipt['omp_binding']
    state = 'unverified'
    if receipt.get('no_guidance') is True or selected['activation_target'] is None:
        state = 'no_guidance'
    else:
        try:
            target = next(t for t in receipt['targets'] if t['path'] == selected['activation_target'])
            template = installation._activation_template(Path(receipt['source']['pack']), omp=True)
            expected = installation.sha(template.encode())
            raw = Path(target['path']).read_bytes()
            if (target['kind'] == 'activation' and target['block_hash'] == expected
                    and installation.activation(raw, expected, shim=template) == raw):
                state = 'receipt-owned'
        except (ContractError, OSError, ValueError, TypeError, KeyError, StopIteration):
            pass
    return {'activation': state, 'automation': 'unqualified', 'native_loading': 'not-observed'}


def diagnostics(repo, receipt):
    from . import installation, omp_boundary, omp_transport, turn_sources
    selected = receipt.get('omp_binding') if receipt else None
    if selected is None:
        return {'binding': 'not-installed', 'native_loading': 'not-observed',
                'source_assessment': 'not-inferred', 'publication': 'not-inferred'}
    targets = [t for t in receipt['targets'] if t['path'] in
               {selected['skill_target'], selected['extension_target'], selected['activation_target']}]
    installation.verify_targets(targets, source=receipt['source'])
    result = {'binding': selected, 'owned_disk_integrity': 'verified',
              'native_loading': 'not-live-verified', 'supported_version_target': '18.16.0',
              'source_assessment': 'not-inferred-from-loading', 'refresh': 'not-inferred-from-assessment',
              'publication': 'requires-witnessed-shared-publication',
              'retention': 'private-native-snapshots-preserved-on-uninstall',
              'answer_first': _answer_first_qualification(receipt)}
    with turn_sources.read_only_locks():
        base = omp_boundary._base(repo)
        if (base / 'omp-source-v1').exists():
            pending = omp_transport.list_pending(repo)
            result['observer_evidence'] = 'retained'
            result['pending'] = pending
        else:
            result['observer_evidence'] = 'not-observed'
        if any((base / name).exists() for name in ('omp-stream-v1', 'omp-stream-session-locks')):
            from . import omp_stream
            streams = omp_stream.inventory(repo, base=base, read_only=True)
            result['transport_qualification'] = {
                'instances': len(streams['streams']), 'pending': streams['pending'],
                'assessment_authority': False, 'automatic_wake_support': 'unqualified'}
            try:
                result['wake_decisions'] = omp_stream.wake_decisions(repo, streams, base=base)
            except (MemoryError, RecursionError):
                raise
            except Exception:
                result['wake_decisions'] = omp_stream._wake_decision_summary(available=False)
        try:
            with turn_sources.source_barrier(repo, existing_only=True):
                omp_boundary.verify_marker(repo)
            result['stored_source_activation'] = 'verified'
        except (ContractError, OSError, ValueError, TypeError, KeyError):
            result['stored_source_activation'] = 'required-or-unverified'
    return result


def observer_binding(artifact_digest, agent_dir):
    """Read back the explicit installed authority before any observer write."""
    import hashlib
    from . import artifact, installation
    state, _ = installation.locations()
    receipt = installation.read_receipt(state)
    if (receipt is None or not receipt.get('omp_binding')
            or receipt['source']['artifact_digest'] != artifact_digest
            or receipt['omp_binding']['agent_dir'] != agent_dir
            or Path(receipt['source']['pack']).resolve() != Path(__file__).resolve().parents[2]):
        raise ContractError('Native observer is not the current explicit owned binding')
    target = receipt['omp_binding']['extension_target']
    owned = next(item for item in receipt['targets'] if item['path'] == target)
    installation.verify_targets([owned], source=receipt['source'])
    pack = Path(receipt['source']['pack'])
    if artifact.inspect(pack)['artifact_digest'] != artifact_digest:
        raise ContractError('Native observer executable closure differs from the installed artifact')
    for relative in ('hooks/omp.ts', 'hooks/python3.sh', 'scripts/omp_boundary.py',
                     'scripts/agent_memory/omp_boundary.py', 'scripts/agent_memory/omp_transport.py',
                     'scripts/agent_memory/omp_projection.py', 'scripts/agent_memory/omp_control.py',
                     'scripts/agent_memory/omp_installation.py', 'scripts/agent_memory/omp_stream.py',
                     'scripts/agent_memory/installation.py', 'scripts/agent_memory/artifact.py',
                     'scripts/agent_memory/trust_paths.py', 'scripts/agent_memory/frontmatter.py',
                     'scripts/agent_memory/pack_validation.py', 'scripts/agent_memory/migration_contract.py',
                     'scripts/agent_memory/turn_sources.py', 'scripts/agent_memory/security.py'):
        expected = receipt['artifact_files'].get(relative)
        if not expected or hashlib.sha256((pack / relative).read_bytes()).hexdigest() != expected['sha256']:
            raise ContractError('Native observer executable differs from installed artifact')
    return receipt


_OBSERVER_MINT = object()


class ObserverBinding:
    """In-process executable attestation; fast checks never adopt a new receipt."""
    __slots__ = ('_seal', '_receipt_sha', '_receipt_identity', '_receipt_path',
                 '_agent_dir', '_artifact_digest', '_pack')

    def __init__(self, token, receipt, receipt_path, identity):
        if token is not _OBSERVER_MINT:
            raise TypeError('Observer binding is internally minted')
        from . import installation
        if identity is None or identity.get('kind') != 'file':
            raise ContractError('Observer ownership receipt is unavailable')
        values = {
            '_seal': token, '_receipt_sha': installation.sha(installation.encoded(receipt)),
            '_receipt_identity': (identity['kind'], identity['hash'], identity['mode']),
            '_receipt_path': receipt_path,
            '_agent_dir': receipt['omp_binding']['agent_dir'],
            '_artifact_digest': receipt['source']['artifact_digest'],
            '_pack': receipt['source']['pack'],
        }
        for name, value in values.items():
            object.__setattr__(self, name, value)

    def __setattr__(self, name, value):
        raise AttributeError('Observer binding is immutable')

    def __reduce__(self):
        raise TypeError('Observer binding cannot be serialized')

    def __reduce_ex__(self, protocol):
        raise TypeError('Observer binding cannot be serialized')


def verified_observer_binding(artifact_digest, agent_dir, *, receipt=None):
    """Full artifact/ownership verification, outside the transport lock."""
    from . import installation
    current = observer_binding(artifact_digest, agent_dir)
    if receipt is not None and installation.encoded(current) != installation.encoded(receipt):
        raise ContractError('Native observer installation changed; start a new instance')
    state, _ = installation.locations()
    path = state / 'receipt.json'
    identity = installation.fingerprint(path)
    if installation.read_receipt(state) != current or installation.fingerprint(path) != identity:
        raise ContractError('Native observer receipt changed during verification')
    return ObserverBinding(_OBSERVER_MINT, current, path, identity)


def verify_observer_binding(repo, receipt, proof):
    """Return an independently checked binding outside a live operation."""
    with _observe_observer_binding(repo, receipt, proof) as context:
        return context


@contextmanager
def _observe_observer_binding(repo, receipt, proof, *, held_source=None):
    """Check ownership and mint scope at entry, then freshly check at exit."""
    from . import installation, omp_boundary, trust_paths, turn_sources
    if type(proof) is not ObserverBinding or proof._seal is not _OBSERVER_MINT:
        raise ContractError('Native observer requires its exact internally verified binding')
    if (not isinstance(receipt, dict) or not isinstance(receipt.get('omp_binding'), dict)
            or installation.sha(installation.encoded(receipt)) != proof._receipt_sha
            or receipt['omp_binding']['agent_dir'] != proof._agent_dir
            or receipt['source']['artifact_digest'] != proof._artifact_digest
            or receipt['source']['pack'] != proof._pack):
        raise ContractError('Native observer proof differs from its original receipt')
    guard = (turn_sources._borrowed_source_barrier(held_source) if held_source is not None
             else turn_sources.source_barrier(repo))
    with guard:
        state, _ = installation.locations()
        if state / 'receipt.json' != proof._receipt_path:
            raise ContractError('Native observer ownership location changed')
        trust_paths.state(state)
        trust_paths.record(proof._receipt_path)
        identity = installation.fingerprint(proof._receipt_path)
        if (identity is None or identity.get('kind') != 'file'
                or (identity['kind'], identity['hash'], identity['mode']) != proof._receipt_identity):
            raise ContractError('Native observer installation changed; preserve pending work and start a new instance')
        with omp_boundary._observe_marker(repo, held_source=held_source) as context:
            yield context


_MANUAL_MINT = object()
# Renderer paths are enabled only for installation kinds actually measured.
_MEASURED_MANUAL_KINDS = frozenset({'copy', 'link'})


class _OwnedManual:
    """In-process evidence, valid only while the same installation re-verifies."""
    __slots__ = ('_receipt_sha', '_artifact', '_agent_dir', '_relative_path', '_comparison')

    def __init__(self, token, receipt_sha, artifact, agent_dir, relative, comparison):
        if token is not _MANUAL_MINT:
            raise TypeError('Owned manual evidence is internally minted')
        object.__setattr__(self, '_receipt_sha', receipt_sha)
        object.__setattr__(self, '_artifact', artifact)
        object.__setattr__(self, '_agent_dir', agent_dir)
        object.__setattr__(self, '_relative_path', relative)
        object.__setattr__(self, '_comparison', comparison)

    def __setattr__(self, name, value):
        raise AttributeError('Owned manual evidence is immutable')

    def __reduce__(self):
        raise TypeError('Owned manual evidence cannot be serialized')

    def _verify(self):
        current = _manual_comparison(self._receipt_sha, self._artifact, self._agent_dir, self._relative_path)
        return current if current == self._comparison else None

    def revalidate(self):
        return self._verify() is not None

    @property
    def relative_path(self):
        return self._relative_path

    @property
    def text(self):
        return self._comparison[0]

    @property
    def header_path(self):
        return self._comparison[1]

    @property
    def installation_id(self):
        return self._comparison[2]

    @property
    def observed_receipt_sha256(self):
        return self._comparison[3]

    @property
    def artifact_digest(self):
        return self._comparison[4]

    @property
    def manual_file_sha256(self):
        return self._comparison[5]


def _manual_comparison(receipt_sha, artifact_digest, agent_dir, relative='SKILL.md'):
    """Reobserve the binding and read through pinned no-follow descriptors."""
    import hashlib
    import os
    import stat
    from . import installation, omp_projection, omp_transport, trust_paths

    descriptors, link_descriptors = [], []
    try:
        if omp_projection._instruction_uri(relative) is None:
            return None
        receipt = observer_binding(artifact_digest, agent_dir)
        if omp_transport.digest(receipt) != receipt_sha:
            return None
        skill_path = receipt['omp_binding']['skill_target']
        targets = [item for item in receipt['targets'] if item['path'] == skill_path]
        if len(targets) != 1 or targets[0]['kind'] not in _MEASURED_MANUAL_KINDS:
            return None
        target = targets[0]
        installation.verify_targets([target], source=receipt['source'])
        selected = receipt['artifact_files'][relative]
        if not isinstance(selected, dict) or set(selected) != {'sha256', 'mode'}:
            return None
        expected = selected['sha256']
        if (not isinstance(expected, str) or not re.fullmatch(r'[0-9a-f]{64}', expected)
                or not isinstance(selected['mode'], str)):
            return None
        logical_path = Path(skill_path) / relative
        link_pin = None
        if target['kind'] == 'link':
            link = Path(skill_path)
            destination = Path(receipt['source']['pack'])
            if (not link.is_absolute() or '..' in link.parts
                    or not destination.is_absolute() or '..' in destination.parts
                    or target['identity'] != {'kind': 'link', 'target': str(destination)}):
                return None
            link_parts = link.parts[1:]
            node = Path(link.anchor)
            fd = os.open(node, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            link_descriptors.append(fd)
            link_dirs = [(fd, None, None, os.fstat(fd))]
            protected, _ = trust_paths.inspect_entry(node, link_dirs[0][3], False)
            for part in link_parts[:-1]:
                parent = fd
                fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
                link_descriptors.append(fd)
                node = node / part
                value = os.fstat(fd)
                protected, _ = trust_paths.inspect_entry(node, value, protected)
                link_dirs.append((fd, parent, part, value))
            link_value = os.stat(link_parts[-1], dir_fd=fd, follow_symlinks=False)
            trust_paths.inspect_entry(link, link_value, protected, final=True, allow_final_link=True)
            if not stat.S_ISLNK(link_value.st_mode) or os.readlink(link_parts[-1], dir_fd=fd) != str(destination):
                return None
            link_pin = (link, link_parts[-1], fd, link_value, link_dirs, str(destination))
            path = destination / relative
        else:
            path = logical_path
        if not path.is_absolute() or '..' in path.parts:
            return None

        def directory_pin(value):
            # Sibling entries may change directory size, links and timestamps.
            # Authority is the selected named inode and its unchanged trust policy.
            return (value.st_dev, value.st_ino, value.st_mode, value.st_uid, value.st_gid)

        def pin(value):
            return (value.st_dev, value.st_ino, value.st_mode, value.st_uid,
                    value.st_gid, value.st_nlink, value.st_size,
                    value.st_mtime_ns, value.st_ctime_ns)

        root = Path(path.anchor)
        descriptors.append(os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW))
        protected, _ = trust_paths.inspect_entry(root, os.fstat(descriptors[0]), False)
        pins = [directory_pin(os.fstat(descriptors[0]))]
        parts = path.parts[1:]
        node = root
        for index, part in enumerate(parts):
            final = index == len(parts) - 1
            flags = os.O_RDONLY | os.O_NOFOLLOW | (os.O_NONBLOCK if final else os.O_DIRECTORY)
            fd = os.open(part, flags, dir_fd=descriptors[-1])
            descriptors.append(fd)
            node = node / part
            value = os.fstat(fd)
            protected, _ = trust_paths.inspect_entry(node, value, protected, final=final)
            if final and (not stat.S_ISREG(value.st_mode) or value.st_nlink != 1
                          or oct(stat.S_IMODE(value.st_mode)) != selected['mode']
                          or value.st_size > omp_projection.MAX_SOURCE):
                return None
            pins.append(pin(value) if final else directory_pin(value))

        chunks, size = [], 0
        while size <= omp_projection.MAX_SOURCE:
            chunk = os.read(descriptors[-1], min(8192, omp_projection.MAX_SOURCE + 1 - size))
            if not chunk:
                break
            chunks.append(chunk)
            size += len(chunk)
        if size > omp_projection.MAX_SOURCE:
            return None
        raw = b''.join(chunks)
        if hashlib.sha256(raw).hexdigest() != expected:
            return None
        text = raw.decode('utf-8')
        installation.verify_targets([target], source=receipt['source'])
        if omp_transport.digest(observer_binding(artifact_digest, agent_dir)) != receipt_sha:
            return None
        if (directory_pin(os.fstat(descriptors[0])) != pins[0]
                or directory_pin(os.stat(root, follow_symlinks=False)) != pins[0]):
            return None
        for index, part in enumerate(parts):
            selected_pin = pin if index == len(parts) - 1 else directory_pin
            if (selected_pin(os.fstat(descriptors[index + 1])) != pins[index + 1]
                    or selected_pin(os.stat(part, dir_fd=descriptors[index], follow_symlinks=False)) != pins[index + 1]):
                return None
        if link_pin is not None:
            link, name, parent, value, link_dirs, destination = link_pin
            if (directory_pin(os.stat(link.anchor, follow_symlinks=False)) != directory_pin(link_dirs[0][3])
                    or pin(os.stat(name, dir_fd=parent, follow_symlinks=False)) != pin(value)
                    or os.readlink(name, dir_fd=parent) != destination):
                return None
            for fd, parent_fd, part, before in link_dirs:
                if (directory_pin(os.fstat(fd)) != directory_pin(before)
                        or (parent_fd is not None
                            and directory_pin(os.stat(part, dir_fd=parent_fd, follow_symlinks=False)) != directory_pin(before))):
                    return None
        return (text, str(logical_path), receipt['installation_id'], receipt_sha, artifact_digest, expected)
    except (OSError, ValueError, TypeError, KeyError, IndexError, ContractError):
        return None
    finally:
        for fd in reversed(descriptors + link_descriptors):
            os.close(fd)


def _owned_manual(receipt, relative='SKILL.md'):
    """Mint comparison evidence only for the current explicitly owned binding."""
    from . import omp_transport
    try:
        receipt_sha = omp_transport.digest(receipt)
        artifact_digest = receipt['source']['artifact_digest']
        agent_dir = receipt['omp_binding']['agent_dir']
        comparison = _manual_comparison(receipt_sha, artifact_digest, agent_dir, relative)
        if comparison is None:
            return None
        return _OwnedManual(_MANUAL_MINT, receipt_sha, artifact_digest, agent_dir, relative, comparison)
    except (ValueError, TypeError, KeyError, ContractError):
        return None
