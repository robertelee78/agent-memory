"""Offline, byte-verified published v0.0.14 portable runtime fixture."""
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'tests/fixtures/v0.0.14-runtime'
ARCHIVE_SHA256 = 'c40f73e7c48d17056a48d9f99f3b799c5bcf3bad042ef555c7faa45fdbfeb4bb'


def extract(destination):
    proof = json.loads((FIXTURE / 'provenance.json').read_text())
    assert proof['source_commit'] == '342448aa0d045c554c6094ca3b36d997f47f6d79'
    raw = (FIXTURE / 'agent-memory.tar.gz').read_bytes()
    assert hashlib.sha256(raw).hexdigest() == proof['archive_sha256'] == ARCHIVE_SHA256
    files = proof['files']
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:gz') as archive:
        members = archive.getmembers()
        assert len(members) == len(files)
        checked = []
        seen = set()
        for member in members:
            name = member.name
            path = PurePosixPath(name)
            assert member.isfile() and not member.issym() and not member.islnk()
            assert not path.is_absolute() and path.parts[:2] == ('skills', 'agent-memory')
            assert all(part not in ('', '.', '..') for part in path.parts)
            assert name not in seen and name in files
            seen.add(name)
            entry = files[name]
            body = archive.extractfile(member).read()
            assert len(body) == member.size == entry['size']
            assert format(member.mode, '04o') == entry['mode'] and member.mode in (0o644, 0o755)
            assert hashlib.sha256(body).hexdigest() == entry['sha256']
            checked.append((path, body, member.mode))
        assert seen == set(files)
    # Validate the complete archive before creating or executing any member.
    for path, body, mode in checked:
        target = destination.joinpath(*path.parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open('xb') as stream:
            stream.write(body)
        target.chmod(mode)
    return destination / 'skills/agent-memory'


def install_with_pending_source(world):
    """Run the released installer, identity helpers and hook without new coordinators."""
    import subprocess
    import sys
    import uuid

    channel = world.repository('released v014 channel')
    pack = extract(channel)
    world.git('add', '.', cwd=channel)
    world.git('commit', '-m', 'Byte-verified released v0.0.14 runtime', cwd=channel)
    repo = world.repository()

    def command(script, *args):
        result = world.run([sys.executable, '-I', '-B', pack/'scripts'/script, *args], timeout=120)
        return json.loads(result.stdout)

    def proposal(value):
        path = world.root/('v014-proposal-'+uuid.uuid4().hex+'.json')
        path.write_text(json.dumps(value))
        path.chmod(0o600)
        return path

    installed = command('installer.py', 'install', '--remote', channel, '--ref', 'main',
                        '--mode', 'copy', '--presets', 'codex')
    assert installed['verified'] and installed['receipt']['source']['version'] == '0.0.14'
    request = json.loads((pack/'templates/router-proposal.example.json').read_bytes())
    request['project-id'] = str(uuid.uuid4())
    rendered = command('bank_contract.py', 'render', '--input', proposal(request))
    bank = repo/'memory-bank'
    bank.mkdir()
    for name, body in rendered['files'].items():
        target = bank/name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(body)
    command('developer_identity.py', 'apply', '--proposal',
            proposal(command('developer_identity.py', 'preview', '--create', '--label',
                             'Disposable released-hook source fixture')))
    command('scope_binding.py', 'apply', '--proposal',
            proposal(command('scope_binding.py', 'preview', '--repo', repo, '--action', 'bind',
                             '--reason', 'Explicit released-hook fixture bank adoption')))
    prompt = 'Public synthetic pre-barrier source.'
    event = {'hook_event_name': 'UserPromptSubmit', 'session_id': str(uuid.uuid4()),
             'turn_id': str(uuid.uuid4()), 'cwd': str(repo), 'prompt': prompt}
    hook = subprocess.run([sys.executable, '-I', '-B', str(pack/'hooks/codex_prompt.py')],
                          input=json.dumps(event), text=True, capture_output=True,
                          cwd=repo, env=world.env, timeout=15)
    assert hook.returncode == 0 and 'hookSpecificOutput' in json.loads(hook.stdout), hook.stderr
    sources = list((world.paths['state']/'agent-memory/writes').glob('*/turn-sources-v2'))
    assert len(sources) == 1
    records = list(sources[0].glob('turns/*/source-0.json'))
    assert len(records) == 1
    observed = json.loads(records[0].read_bytes())
    assert observed['schema'] == 'agent-memory/turn-source-v2'
    assert observed['harness'] == 'codex' and observed['source_sha256'] == hashlib.sha256(prompt.encode()).hexdigest()
    barrier = sources[0].parent/'source-barrier.lock'
    assert not barrier.exists()
    return {'repo': repo, 'bank': bank, 'channel': channel, 'pack': pack,
            'receipt': installed['receipt'], 'sources': sources[0], 'barrier': barrier}
