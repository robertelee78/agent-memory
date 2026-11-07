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
