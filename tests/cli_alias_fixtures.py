"""Current and exact pre-am runtimes for public migration regressions."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[1]
PACK = ROOT/'skills/agent-memory'


def channel(world, *, legacy=False):
    root = world.repository('legacy channel' if legacy else 'current channel')
    pack = root/'skills/agent-memory'
    if legacy:
        fixtures = ROOT/'tests/fixtures/legacy-cli'
        provenance = json.loads((fixtures/'provenance.json').read_text())
        assert provenance['source_commit'] == '3af48ea0900fe7f597e3fdeb6d2b525acf984bf8'
        archive = fixtures/provenance['archive']
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == provenance['sha256']
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
                    assert member.isfile(), 'Legacy fixture contains a non-regular file'
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(stream.extractfile(member).read())
                    # Git archives add group-write permission via tar.umask;
                    # restore checkout modes so this is the shipped artifact.
                    target.chmod(0o755 if member.mode & 0o111 else 0o644)
    else:
        shutil.copytree(PACK, pack)
    world.git('add', '.', cwd=root)
    world.git('commit', '-m', 'Exact legacy runtime' if legacy else 'Current runtime', cwd=root)
    return root, pack


def install(world, root, *options, legacy=False, expected=0, env=None):
    helper = root/'skills/agent-memory/scripts/installer.py' if legacy else PACK/'scripts/installer.py'
    result = world.run([sys.executable, '-I', helper, 'install', '--remote', root.as_uri(),
                        '--ref', 'main', '--presets', 'none', *options],
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
