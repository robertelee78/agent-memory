#!/usr/bin/env python3
"""Build or publish the C2E-style signed portable release in one transaction."""
from pathlib import Path
import argparse
import gzip
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'skills/agent-memory/scripts'))
from agent_memory import artifact, release_distribution as release


def run(argv, *, data=None):
    result = subprocess.run(argv, cwd=ROOT, input=b'' if data is None else data,
                            capture_output=True, timeout=180,
                            env={**os.environ, 'GH_PROMPT_DISABLED':'1', 'GH_PAGER':'cat', 'GIT_TERMINAL_PROMPT':'0'})
    release.require(result.returncode == 0, 'Release command failed: ' + argv[0])
    return result.stdout


def git(*args):
    return run(['git', *args]).decode().strip()


def render(verifier):
    template = (ROOT/'scripts/distribution/install.sh.in').read_text()
    release.require(template.count('__VERIFIER__') == 1, 'Invalid bootstrap template')
    return template.replace('__VERIFIER__', verifier)


def build(output, key):
    release.require(not git('status', '--porcelain', '--untracked-files=all'), 'Release source must be clean')
    source = git('rev-parse', 'HEAD')
    artifact.inspect(ROOT/'skills/agent-memory')
    public = run(['ssh-keygen', '-y', '-f', str(key)]).decode().split()
    release.require(' '.join(public[:2]) == release.PUBLIC_KEY, 'Private key does not match the pinned AM key')
    output.mkdir(mode=0o700)
    # Git archive supplies tracked bytes, not ignored bytecode or development memory.
    tracked = run(['git', 'archive', '--format=tar', source, '--', 'skills/agent-memory'])
    with tempfile.TemporaryDirectory(prefix='am-release-pack-') as temporary:
        root = Path(temporary)/'source'; root.mkdir()
        entries = []
        with tarfile.open(fileobj=io.BytesIO(tracked), mode='r:') as archive:
            for member in archive:
                if member.isdir(): continue
                release.require(member.isfile() and member.name.startswith(release.PREFIX), 'Unsafe tracked release entry')
                raw = archive.extractfile(member).read(release.MAX_FILE + 1)
                release.require(len(raw) <= release.MAX_FILE, 'Tracked asset exceeds portable bound')
                mode = 0o755 if member.mode & 0o111 else 0o644
                path = root/member.name; path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(raw); path.chmod(mode)
                entries.append((member.name, raw, mode))
        version, identity, _ = release.package_identity(root/'skills/agent-memory')
        tag = 'v' + version
        with (output/release.ARCHIVE).open('wb') as stream, gzip.GzipFile(filename='', mode='wb', fileobj=stream, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w:', format=tarfile.USTAR_FORMAT) as archive:
                for name, raw, mode in sorted(entries):
                    member = tarfile.TarInfo(name); member.size = len(raw); member.mode = mode
                    archive.addfile(member, io.BytesIO(raw))
        raw = (output/release.ARCHIVE).read_bytes()
        release.require(len(raw) <= release.MAX_ARCHIVE, 'Release archive exceeds bound')
        manifest = {'schema': 'agent-memory/release-v1', 'repository': release.REPOSITORY,
                    'tag': tag, 'version': version, 'source_commit': source,
                    'archive': release.ARCHIVE, 'archive_size': len(raw),
                    'archive_sha256': release.digest(raw), 'artifact_digest': identity}
        manifest_raw = (json.dumps(manifest, sort_keys=True, separators=(',', ':'))+'\n').encode()
        (output/release.MANIFEST).write_bytes(manifest_raw)
        run(['ssh-keygen', '-Y', 'sign', '-n', release.NAMESPACE, '-f', str(key), str(output/release.MANIFEST)])
        (output/(release.MANIFEST+'.sig')).rename(output/release.SIGNATURE)
        verified = release.verify_manifest(manifest_raw, (output/release.SIGNATURE).read_bytes(), tag)
        release.extract(raw, Path(temporary)/'verified', verified)
        verifier = (root/'skills/agent-memory/scripts/agent_memory/release_distribution.py').read_text()
        bootstrap = render(verifier)
        release.require((ROOT/'install.sh').read_text() == bootstrap,
                        'Tracked bootstrap differs from the shared verifier; regenerate it before source review')
        (output/'install.sh').write_text(bootstrap); (output/'install.sh').chmod(0o755)
    release.require(git('rev-parse', 'HEAD') == source and not git('status', '--porcelain', '--untracked-files=all'),
                    'Source changed during release packaging')
    return manifest


def publish(key, notes, integration_branch):
    release.require(integration_branch and integration_branch.startswith('integrate/')
                    and git('symbolic-ref','--short','HEAD') == integration_branch,
                    'Publish the reviewed release integration branch, not active main')
    release.require(git('remote','get-url','origin') == 'https://github.com/'+release.REPOSITORY+'.git',
                    'Publisher origin differs from the product repository')
    branch = json.loads(run(['gh','api','repos/'+release.REPOSITORY+'/branches/main']))
    rules = json.loads(run(['gh','api','repos/'+release.REPOSITORY+'/rules/branches/main']))
    release.require(branch['protected'] is False and not rules,
                    'Main promotion policy changed; review it before publishing this release')
    with tempfile.TemporaryDirectory(prefix='am-release-') as temporary:
        output = Path(temporary)/'assets'
        manifest = build(output, key); tag = manifest['tag']; source = manifest['source_commit']
        found = subprocess.run(['gh', 'api', 'repos/'+release.REPOSITORY+'/releases/tags/'+tag],
                               cwd=ROOT, input=b'', capture_output=True, timeout=60,
                               env={**os.environ, 'GH_PROMPT_DISABLED':'1', 'GH_PAGER':'cat'})
        existing = json.loads(found.stdout) if found.returncode == 0 else None
        release.require(existing is not None or b'404' in found.stderr, 'Release absence cannot be established')
        remote_source = run(['gh', 'api', 'repos/'+release.REPOSITORY+'/commits/'+source, '--jq', '.sha']).decode().strip()
        release.require(remote_source == source, 'Exact source is not published')
        if existing is None:
            tags = run(['git', 'ls-remote', '--tags', 'origin', 'refs/tags/'+tag])
            release.require(not tags.strip(), 'Existing tag has no matching release; refusing version reuse')
            status = json.loads(run(['gh','api','repos/'+release.REPOSITORY+'/compare/main...'+source]))['status']
            release.require(status == 'ahead', 'Release source must include current main before publication')
            # Create the draft first; the same command resumes missing uploads.
            run(['gh', 'release', 'create', tag, '--repo', release.REPOSITORY, '--target', source,
                 '--draft', '--title', tag, '--notes-file', str(notes)])
            existing = json.loads(run(['gh','api','repos/'+release.REPOSITORY+'/releases/tags/'+tag]))
        release.require(existing['tag_name'] == tag and not existing['prerelease'], 'Existing release selection differs')
        if existing['draft']:
            release.require(existing['target_commitish'] == source, 'Draft source differs; refusing replacement')
        else:
            actual_source = run(['gh','api','repos/'+release.REPOSITORY+'/commits/'+tag,'--jq','.sha']).decode().strip()
            release.require(actual_source == source, 'Published source differs; refusing version reuse')
        expected = {p.name:p.read_bytes() for p in output.iterdir()}
        names = [a['name'] for a in existing['assets']]
        release.require(len(names) == len(set(names)) and set(names) <= set(expected), 'Unexpected existing release assets')
        release.require(all(a['size'] == len(expected[a['name']]) for a in existing['assets']), 'Existing asset size differs')
        with tempfile.TemporaryDirectory(prefix='am-release-readback-') as downloaded:
            for name in names:
                run(['gh','release','download',tag,'--repo',release.REPOSITORY,'--pattern',name,'--dir',downloaded])
            files = {p.name:p.read_bytes() for p in Path(downloaded).iterdir()}
            release.require(all(files[n] == expected[n] for n in names), 'Existing release bytes differ; refusing replacement')
        missing = set(expected)-set(names)
        release.require(not missing or existing['draft'], 'Published release is incomplete; refusing alteration')
        for name in sorted(missing):
            run(['gh','release','upload',tag,str(output/name),'--repo',release.REPOSITORY])
        with tempfile.TemporaryDirectory(prefix='am-complete-readback-') as downloaded:
            run(['gh','release','download',tag,'--repo',release.REPOSITORY,'--dir',downloaded])
            release.require({p.name:p.read_bytes() for p in Path(downloaded).iterdir()} == expected,
                            'Complete release assets differ after upload')
        if existing['draft']:
            run(['gh', 'release', 'edit', tag, '--repo', release.REPOSITORY, '--draft=false', '--latest'])
        actual = json.loads(run(['gh', 'api', 'repos/'+release.REPOSITORY+'/releases/latest']))
        release.require(actual['tag_name'] == tag and not actual['draft'] and not actual['prerelease'],
                        'Published stable release readback differs')
        # Exercise the same public locator/download/signature/extraction boundary.
        with tempfile.TemporaryDirectory(prefix='am-public-readback-') as temporary:
            observed = release.acquire(Path(temporary)/'source')
            release.require(observed == manifest, 'Public release differs from signed publication')
        # Only now may main expose the signed-only installer/updater. Ordinary
        # push refuses concurrent divergent changes; it never forces a ref.
        status = json.loads(run(['gh','api','repos/'+release.REPOSITORY+'/compare/main...'+source]))['status']
        if status not in ('identical','behind'):
            result = subprocess.run(['git','-c','credential.helper=',
                                     '-c','credential.helper=!gh auth git-credential',
                                     'push','origin',source+':refs/heads/main'], cwd=ROOT,
                                    input=b'', capture_output=True, timeout=180,
                                    env={**os.environ,'GIT_TERMINAL_PROMPT':'0'})
            release.require(result.returncode == 0,
                            'Signed '+tag+' is published and verified, but main advanced; no ref was forced. '
                            'Review integration of '+source+' before claiming default activation')
        status = json.loads(run(['gh','api','repos/'+release.REPOSITORY+'/compare/main...'+source]))['status']
        release.require(status in ('identical','behind'), 'Main activation could not be verified')
        return {'published':True, 'tag':tag, 'source_commit':source, 'artifact_digest':manifest['artifact_digest'],
                'url':actual['html_url'], 'default_activation_performed':True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('build','publish','render-bootstrap'))
    parser.add_argument('--private-key', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--notes-file', type=Path)
    parser.add_argument('--integration-branch')
    args = parser.parse_args()
    if args.operation == 'render-bootstrap':
        print(render((ROOT/'skills/agent-memory/scripts/agent_memory/release_distribution.py').read_text()), end='')
        return
    release.require(args.private_key is not None, 'A private-key path is required')
    key = args.private_key.expanduser().resolve(strict=True)
    release.require(key.is_file() and not key.is_symlink() and key.stat().st_mode & 0o077 == 0,
                    'Signing key must be an owner-only file')
    if args.operation == 'build':
        release.require(args.output is not None, 'Build output is required')
        result = build(args.output.resolve(), key)
    else:
        release.require(args.notes_file is not None and args.notes_file.is_file(), 'Release notes are required')
        result = publish(key, args.notes_file.resolve(), args.integration_branch)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print('am release: '+str(error), file=sys.stderr); sys.exit(1)
