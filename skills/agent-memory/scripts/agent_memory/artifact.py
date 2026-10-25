"""Describe a complete portable artifact without executing candidate code."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re

from .frontmatter import ContractError, unique_object
from . import pack_validation

PACK = Path(__file__).resolve().parents[2]
COMPATIBILITY = 'schemas/runtime-compatibility.json'


def require(condition, message):
    if not condition:
        raise ContractError(message)


def declaration(root, relative, files):
    require(isinstance(relative, str) and relative in files
            and not PurePosixPath(relative).is_absolute()
            and '..' not in PurePosixPath(relative).parts,
            'Declared artifact asset is missing or escapes the inventory')
    path = root / relative
    with path.open('rb') as stream:
        raw = stream.read(65537)
    require(len(raw) <= 65536 and hashlib.sha256(raw).hexdigest() == files[relative]['sha256'],
            'Declaration is oversized or changed after package validation')
    return json.loads(raw, object_pairs_hook=unique_object,
                      parse_constant=lambda _: (_ for _ in ()).throw(ContractError('Nonfinite declaration')))


def strings(value, *, allow_empty=False):
    return (isinstance(value, list) and (allow_empty or bool(value))
            and all(isinstance(v, str) and 0 < len(v) <= 256 for v in value)
            and len(set(value)) == len(value))


def command_names(compatibility):
    # Older runtimes know only the original name. Never add ownership that an
    # old source's planner would strand on its next install/update.
    names = compatibility.get('installed_commands', ['agent-memory'])
    require(strings(names) and set(names) <= {'am', 'agent-memory'}
            and 'agent-memory' in names, 'Unsupported installed command declaration')
    return tuple(names)


def inspect(pack):
    root = Path(pack).resolve(strict=True)
    validation = pack_validation.validate(root)
    files = validation['files']
    spec = declaration(root, COMPATIBILITY, files)
    require(isinstance(spec, dict) and spec.get('schema') == 'agent-memory/runtime-compatibility-v1',
            'Unsupported runtime compatibility declaration')
    require(strings(spec.get('readable_bank_schemas'))
            and strings(spec.get('migrator_source_schemas'), allow_empty=True)
            and strings(spec.get('git_required_for'))
            and strings(spec.get('optional_adapters'), allow_empty=True)
            and isinstance(spec.get('python_minimum'), str)
            and re.fullmatch(r'[0-9]+\.[0-9]+', spec['python_minimum'])
            and spec.get('required_python_packages') == []
            and spec.get('required_agent_clients') == [],
            'Compatibility must declare bounded schema/tool lists and a client-neutral standard-library core')
    command_names(spec)
    from . import migration_contract
    migrations = migration_contract.inspect(root, spec, files)
    router = declaration(root, 'schemas/bank-router.json', files)
    require(router['properties']['schema']['const'] in spec['readable_bank_schemas'],
            'Native bank schema is absent from its readability declaration')
    adapters = []
    for relative in spec['optional_adapters']:
        require(relative.startswith('adapters/'), 'Optional adapters must be explicitly separated')
        adapter = declaration(root, relative, files)
        require(isinstance(adapter, dict) and adapter.get('schema') == 'agent-memory/optional-adapter-v1'
                and isinstance(adapter.get('name'), str) and bool(adapter['name'])
                and adapter.get('kind') in {'service', 'client-discovery'}
                and strings(adapter.get('commands'), allow_empty=True)
                and type(adapter.get('requires_authentication')) is bool
                and isinstance(adapter.get('unavailable_behavior'), str)
                and bool(adapter['unavailable_behavior']), 'Invalid optional adapter declaration')
        entry = adapter.get('entrypoint')
        require(isinstance(entry, str) and entry.startswith('scripts/') and entry in files,
                'Optional adapter entrypoint is missing from the portable artifact')
        adapters.append({'path': relative, **adapter})
    current = pack_validation.inventory(root)
    readback = {p: {'sha256': hashlib.sha256(raw).hexdigest(), 'mode': oct(mode)}
                for p, (raw, mode) in sorted(current.items())}
    require(files == readback, 'Artifact changed while its declaration was being inspected')
    digest = hashlib.sha256(json.dumps(files, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    return {'schema': 'agent-memory/artifact-inspection-v1', 'operation': 'inspect', 'valid': True,
            'version': validation['version'], 'artifact_digest': digest, 'files': files,
            'file_count': len(files), 'compatibility': spec, 'optional_adapters': adapters, 'migration_catalog': migrations,
            'guides': validation['guides'], 'documented_commands': validation['documented_commands'],
            'file_operations': validation['file_operations'], 'writes_performed': False,
            'candidate_code_executed': False, 'source_origin_authenticated': False,
            'installation_state_checked': False, 'runtime_behavior_certified': False,
            'limits': ['Digest binds relative file names, bytes and file modes; it is not a signature.',
                       'Schema/tool capabilities are explicit declarations, separately exercised by public tests.',
                       'Installation ownership, channel and client activation require lifecycle readback.']}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('inspect',))
    parser.add_argument('--pack', default=str(PACK))
    args = parser.parse_args(argv)
    try:
        value = inspect(args.pack)
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, SyntaxError, RecursionError) as exc:
        value = {'schema': 'agent-memory/artifact-inspection-v1', 'operation': 'inspect', 'valid': False,
                 'writes_performed': False, 'candidate_code_executed': False,
                 'error': str(exc), 'installation_state_checked': False}
    print(json.dumps(value, indent=2))
    return 0 if value['valid'] else 1
