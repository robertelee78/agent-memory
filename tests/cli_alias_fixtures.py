"""Current and exact historic runtimes for public migration regressions."""
from contextlib import contextmanager
import hashlib
import importlib
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[1]
PACK = ROOT/'skills/agent-memory'


def _archived_pack(world, root, fixture, source_commit, archive_sha256):
    fixtures = ROOT/'tests/fixtures'/fixture
    provenance = json.loads((fixtures/'provenance.json').read_text())
    assert provenance['source_commit'] == source_commit
    assert provenance['sha256'] == archive_sha256
    archive = fixtures/provenance['archive']
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == archive_sha256
    with tarfile.open(archive) as stream:
        for member in stream:
            relative = Path(member.name)
            assert not relative.is_absolute() and '..' not in relative.parts
            if relative == Path('skills'):
                assert member.isdir()
                continue
            assert relative.parts[:2] == ('skills', 'agent-memory')
            target = world.inside(root/relative)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                assert member.isfile(), 'Historic fixture contains a non-regular file'
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(stream.extractfile(member).read())
                # Git archives add group-write permission via tar.umask;
                # restore checkout modes so this is the shipped artifact.
                target.chmod(0o755 if member.mode & 0o111 else 0o644)
    return root/'skills/agent-memory'


def channel(world, *, legacy=False):
    root = world.repository('legacy channel' if legacy else 'current channel')
    pack = root/'skills/agent-memory'
    if legacy:
        _archived_pack(world, root, 'legacy-cli',
            '3af48ea0900fe7f597e3fdeb6d2b525acf984bf8',
            'd687778a8e393ffbd1a95c79cbd41c401d7f030f91923f61a0dc73f757a6becf')
    else:
        shutil.copytree(PACK, pack)
    world.git('add', '.', cwd=root)
    world.git('commit', '-m', 'Exact legacy runtime' if legacy else 'Current runtime', cwd=root)
    return root, pack


@contextmanager
def pre_source_fence_writers(world):
    """Execute unchanged pre-fence writers without replacing current observers."""
    pack = _archived_pack(world, world.inside('pre-source-fence-writer'),
        'pre-source-fence', '95b17dfe2173a7befcd363ca91c57ff710e6a654',
        'b1a9760fbb90fe1adc6192745db11e0df6c739840cd73d4769286302ca8baff7')
    package = pack/'scripts/agent_memory'
    name = '_pre_source_fence_' + world.token
    spec = importlib.util.spec_from_file_location(name, package/'__init__.py',
        submodule_search_locations=[str(package)])
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    try:
        spec.loader.exec_module(module)
        yield tuple(importlib.import_module(name + '.' + child)
                    for child in ('bank_writes', 'bank_transactions', 'local_registry'))
    finally:
        for child in tuple(sys.modules):
            if child == name or child.startswith(name + '.'):
                del sys.modules[child]


def install(world, root, *options, legacy=False, expected=0, env=None):
    helper = root/'skills/agent-memory/scripts/installer.py' if legacy else PACK/'scripts/installer.py'
    command = [sys.executable, '-I', helper, 'install', '--remote', root.as_uri(),
               '--ref', 'main', '--presets', 'none', *options]
    if legacy:
        # This fixture identifies an exact historical checkout digest. Fix only
        # its initial checkout modes; upgrade processes retain the caller umask.
        command = [sys.executable, '-I', '-c',
                   'import os,subprocess,sys;os.umask(0o022);sys.exit(subprocess.call(sys.argv[1:]))',
                   *command]
    result = world.run(command,
                       check=False, env=env, timeout=120)
    assert result.returncode == expected, (result.returncode, result.stdout, result.stderr)
    return json.loads(result.stdout)


def alias_tmpdir(world):
    alias = world.root/'macOS-style-temp-alias'
    alias.symlink_to(world.paths['tmp'], target_is_directory=True)
    world.env['TMPDIR'] = str(alias)
    return alias


def advance_to_current(world, root):
    pack = root/'skills/agent-memory'
    shutil.rmtree(pack)
    shutil.copytree(PACK, pack)
    world.git('add', '.', cwd=root)
    world.git('commit', '-m', 'Upgrade to am runtime', cwd=root)
    return world.git('rev-parse', 'HEAD', cwd=root).stdout.strip()
