"""Clearly test-only format codecs in isolated trusted fixture releases.

No fixture schema, converter or compatibility claim enters the production pack.
The fixtures transform one router and one context while preserving the complete
normalized native representation, including every other source byte.
"""
import json
from pathlib import Path
import shutil
import sys

from identity_fixtures import bind
from runtime_update_fixtures import ROOT, bank, snapshot

LEGACY = 'agent-memory/test-only-legacy'
MIDDLE = 'agent-memory/test-only-intermediate'
NATIVE = 'agent-memory/v1'
CODE = r'''"""TEST ONLY: not a historical or proposed production bank format."""
from . import bank_contract
from .developer_identity import digest, encoded

LEGACY = 'agent-memory/test-only-legacy'
MIDDLE = 'agent-memory/test-only-intermediate'


def normalized(files, schema):
    normalized = dict(files)
    normalized['MEMORY.md'] = normalized['MEMORY.md'].replace(schema, 'agent-memory/v1', 1)
    if schema == LEGACY:
        normalized['projectbrief.md'] = normalized['projectbrief.md'].replace('# Fixture encoded project\n', '# Project\n', 1)
    elif schema == MIDDLE:
        normalized['projectbrief.md'] = normalized['projectbrief.md'].replace('# Fixture intermediate project\n', '# Project\n', 1)
    checked = bank_contract.validate_files(normalized)
    return {'project_id': checked['project_id'], 'preservation_sha256': digest(encoded(normalized))}


def legacy(files):
    return normalized(files, LEGACY)


def middle(files):
    return normalized(files, MIDDLE)


def first(files):
    result = dict(files)
    result['MEMORY.md'] = result['MEMORY.md'].replace(LEGACY, MIDDLE, 1)
    result['projectbrief.md'] = result['projectbrief.md'].replace('# Fixture encoded project\n', '# Fixture intermediate project\n', 1)
    return result


def second(files):
    result = dict(files)
    result['MEMORY.md'] = result['MEMORY.md'].replace(MIDDLE, 'agent-memory/v1', 1)
    result['projectbrief.md'] = result['projectbrief.md'].replace('# Fixture intermediate project\n', '# Project\n', 1)
    return result
'''


def entry(function):
    return {'module': 'scripts/agent_memory/migration_fixture.py', 'function': function}


def catalog(pack):
    path = pack/'schemas/migrations.json'
    value = json.loads(path.read_text())
    value['codecs'] += [{'schema': LEGACY, 'validator': entry('legacy')},
                        {'schema': MIDDLE, 'validator': entry('middle')}]
    value['steps'] = [{'id': 'test-only-first', 'from': LEGACY, 'to': MIDDLE, 'converter': entry('first')},
                      {'id': 'test-only-second', 'from': MIDDLE, 'to': NATIVE, 'converter': entry('second')}]
    path.write_text(json.dumps(value))
    (pack/'scripts/agent_memory/migration_fixture.py').write_text(CODE)
    path = pack/'schemas/runtime-compatibility.json'
    value = json.loads(path.read_text()); value['migrator_source_schemas'] = [LEGACY, MIDDLE]
    path.write_text(json.dumps(value))


def installed(world, *, codecs=True):
    source = world.inside('trusted-fixture-source')
    shutil.copytree(ROOT/'skills/agent-memory', source)
    if codecs:
        catalog(source)
    result = world.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'install',
                        '--development-source', source, '--presets', 'none'], check=False)
    value = json.loads(result.stdout)
    if not value['verified']:
        raise AssertionError(value)
    return Path(value['receipt']['bin_dir'])/'am', source


def memory(world, *, legacy=True):
    repo = world.repository()
    directory = bank(world, repo)
    bind(world, repo)
    original = snapshot(directory)
    if legacy:
        path = directory/'MEMORY.md'
        path.write_text(path.read_text().replace(NATIVE, LEGACY, 1))
        path = directory/'projectbrief.md'
        path.write_text(path.read_text().replace('# Project\n', '# Fixture encoded project\n', 1))
    (directory/'projectbrief.md').chmod(0o640)
    return repo, directory, original


def save(world, value, name='migration-proposal.json'):
    path = world.inside(name); path.write_text(json.dumps(value)); path.chmod(0o600)
    return path


def call(world, command, *args, expected=0, env=None):
    result = world.run([command, '--json', 'migrate', *args], check=False, env=env, timeout=120)
    value = json.loads(result.stdout) if result.stdout else None
    if result.returncode != expected:
        raise AssertionError((result.returncode, value, result.stderr))
    return value


def preview(world, command, repo):
    return call(world, command, 'preview', '--repo', repo, '--to', NATIVE)


def unlock(world, command, repo):
    value = call(world, command, 'recover-lock', '--repo', repo)
    if value.get('proposal'):
        return call(world, command, 'recover-lock', '--repo', repo,
                    '--proposal', save(world, value, 'unlock.json'))
    return value


def recover(world, command, repo, operation, mode):
    value = call(world, command, 'recover', '--repo', repo, '--operation', operation, '--mode', mode)
    return call(world, command, 'recover', '--repo', repo, '--mode', mode,
                '--proposal', save(world, value, 'recovery.json'))
