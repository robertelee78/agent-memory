"""Explicit omp targets and receipt-v2 validation; never infer a native root."""
import json
import re
from pathlib import Path
from .frontmatter import ContractError

SCHEMAS = {'agent-memory/install-receipt-v1', 'agent-memory/install-receipt-v2'}
FIELDS = {'agent_dir', 'skill_target', 'extension_target', 'activation_target',
          'peer_reader_capability', 'rollout_attested'}


def binding(agent_dir, *, no_guidance=False):
    if not isinstance(agent_dir, str) or not Path(agent_dir).is_absolute() or '..' in Path(agent_dir).parts:
        raise ContractError('Explicit --omp-agent-dir must be an absolute canonical root')
    root = Path(agent_dir)
    if root.resolve() != root or any(p.is_symlink() for p in (root, *root.parents)):
        raise ContractError('omp agent root must not contain symlinks')
    return {'agent_dir': str(root), 'skill_target': str(root / 'skills' / 'agent-memory'),
            'extension_target': str(root / 'extensions' / 'agent-memory.ts'),
            'activation_target': None if no_guidance else str(root / 'AGENTS.md'),
            'peer_reader_capability': 'omp-harness-v1', 'rollout_attested': True}


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
        if previous != binding(previous['agent_dir'], no_guidance=no_guidance):
            raise ContractError('omp saved binding changed; preserve owned targets')
        return previous
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
    if value != binding(value['agent_dir'], no_guidance=receipt.get('no_guidance', False)):
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
    options = json.dumps({'artifact': artifact_digest, 'agentDir': value['agent_dir'], 'rolloutAttested': True}, separators=(',', ':'))
    return ('// Receipt-owned native executable. Never edit provider or approval settings.\n'
            'import nativeMemory from ' + source + ';\n'
            'export default pi => nativeMemory(pi, ' + options + ');\n').encode()


def preview(value):
    return {'binding': value, 'executable_authority': 'stored native source observation and bounded advisory wakes only',
            'canonical_operations': 'ordinary approved native tools; no callback mutation or permission bypass',
            'private_retention': 'native stored user/tool plaintext after shared screening; immutable evidence retained on uninstall; no automatic purge or global quota',
            'compatibility_limit': 'unresolved source gaps/registration faults can require omp-aware readers indefinitely; old/manual/remote binaries outside local inventory are not fenced',
            'qualification': 'disk ownership is not native loading, source completion or platform/provider acceptance'}


def diagnostics(repo, receipt):
    from . import installation, omp_boundary, omp_transport
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
              'retention': 'private-native-snapshots-preserved-on-uninstall'}
    base = omp_boundary._base(repo)
    if (base / 'omp-source-v1').exists():
        pending = omp_transport.list_pending(repo)
        result['observer_evidence'] = 'retained'
        result['pending'] = pending
    else:
        result['observer_evidence'] = 'not-observed'
    return result


def observer_binding(artifact_digest, agent_dir):
    """Read back the explicit installed authority before any observer write."""
    import hashlib
    from . import installation
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
    for relative in ('hooks/omp.ts', 'hooks/python3.sh', 'scripts/omp_boundary.py',
                     'scripts/agent_memory/omp_boundary.py', 'scripts/agent_memory/omp_transport.py',
                     'scripts/agent_memory/omp_projection.py', 'scripts/agent_memory/omp_control.py',
                     'scripts/agent_memory/omp_installation.py'):
        expected = receipt['artifact_files'].get(relative)
        if not expected or hashlib.sha256((pack / relative).read_bytes()).hexdigest() != expected['sha256']:
            raise ContractError('Native observer executable differs from installed artifact')
    return receipt
