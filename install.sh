#!/usr/bin/env bash
# Generated from the same reviewed verifier shipped in the portable artifact.
# A truncated pipe cannot execute a complete prefix of this compound command.
if :; then
set -euo pipefail
for argument in "$@"; do
  case "$argument" in
    --help|-h)
      printf '%s\n' 'Install Agent Memory: install.sh [--check] [--json] [--mode link|copy] [--presets all|none|CLIENTS]' \
        'Custom discovery: --target-dir PATH, --bin-dir PATH, --no-guidance' \
        'Explicit sources: --development-source PATH or --remote URL --ref BRANCH' \
        'After installation: am --version; am update; am uninstall'
      exit 0 ;;
  esac
done
am_python=
old_ifs=$IFS
IFS=:
set -f
for directory in $PATH; do
  [ -n "$directory" ] || directory=.
  candidate=$directory/python3
  if [ -x "$candidate" ] && "$candidate" -I -c 'import sys; sys.exit(sys.version_info < (3, 11))' >/dev/null 2>&1; then
    am_python=$candidate
    break
  fi
done
IFS=$old_ifs
set +f
if [ -z "$am_python" ]; then
  echo 'am: Python 3.11+ must be available as python3 on PATH' >&2; exit 1
fi
"$am_python" -I - "$@" <<'AM_VERIFIED_INSTALLER'
"""C2E-style signed acquisition; standalone so the bootstrap embeds this verifier."""
from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import resource
import shutil
import stat
import subprocess
import tarfile
import tempfile

REPOSITORY = 'robertelee78/agent-memory'
API = 'https://api.github.com/repos/' + REPOSITORY + '/releases'
ASSETS = 'https://github.com/' + REPOSITORY + '/releases/download'
PUBLIC_KEY = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB+C//uSRKmScS5hTki796yvLR2oOSn2IKHLkr1RWE2y'
PRINCIPAL = 'release@agent-memory'
NAMESPACE = 'agent-memory.release'
ARCHIVE = 'agent-memory.tar.gz'
MANIFEST = 'release-manifest.json'
SIGNATURE = MANIFEST + '.sshsig'
PREFIX = 'skills/agent-memory/'
# Reuse the portable reader's complete inventory budgets.
MAX_BYTES = 16777216
MAX_FILE = 2097152
MAX_ENTRIES = 4096
MAX_ARCHIVE = MAX_BYTES + 1048576
MAX_TAR = MAX_BYTES + MAX_ENTRIES * 1024
MAX_METADATA = 65536
VERSION = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)')


class DistributionError(ValueError):
    """Acquisition/authentication failure, not an interrupted activation."""


def require(condition, message):
    if not condition:
        raise DistributionError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate release metadata field')
        result[key] = value
    return result


def parse(raw):
    require(len(raw) <= MAX_METADATA, 'Release metadata exceeds its bound')
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _: require(False, 'Nonfinite release metadata'))


def inventory_digest(files):
    return digest(json.dumps(files, sort_keys=True, separators=(',', ':')).encode())


def regular(path, maximum):
    mode = path.lstat().st_mode
    require(stat.S_ISREG(mode) and path.stat().st_nlink == 1,
            'Release input must be an ordinary unlinked file')
    with path.open('rb') as stream:
        raw = stream.read(maximum + 1)
    require(len(raw) <= maximum, 'Release input exceeds its bound')
    return raw


def verify_manifest(raw, signature, tag=None, installed_root=None):
    require(len(raw) <= MAX_METADATA and 0 < len(signature) <= 4096,
            'Manifest or signature exceeds its bound')
    require(shutil.which('ssh-keygen'), 'OpenSSH ssh-keygen with SSH signature support is required')
    signers = (PRINCIPAL + ' namespaces="' + NAMESPACE + '" ' + PUBLIC_KEY + '\n').encode()
    def verify(signers_path, signature_path):
        result = subprocess.run(['ssh-keygen', '-Y', 'verify', '-f', str(signers_path),
                                 '-I', PRINCIPAL, '-n', NAMESPACE, '-s', str(signature_path)],
                                input=raw, capture_output=True, timeout=20)
        require(result.returncode == 0, 'Release publisher signature is invalid')
    if installed_root is not None:
        signers_path = installed_root/'skills/agent-memory/keys/allowed-signers'
        signature_path = installed_root/SIGNATURE
        require(regular(signers_path, 4096) == signers and regular(signature_path, 4096) == signature,
                'Installed signature inputs differ from the pinned trust root')
        verify(signers_path, signature_path)
    else:
        with tempfile.TemporaryDirectory(prefix='am-signature-') as temporary:
            root = Path(temporary)
            (root/'allowed-signers').write_bytes(signers)
            (root/'signature').write_bytes(signature)
            verify(root/'allowed-signers', root/'signature')
    value = parse(raw)
    require(isinstance(value, dict) and set(value) == {
        'schema', 'repository', 'tag', 'version', 'source_commit', 'archive',
        'archive_size', 'archive_sha256', 'artifact_digest'}, 'Unknown release manifest fields')
    require(value['schema'] == 'agent-memory/release-v1' and value['repository'] == REPOSITORY,
            'Wrong release schema or repository')
    require(isinstance(value['version'], str) and len(value['version']) <= 64 and VERSION.fullmatch(value['version'])
            and value['tag'] == 'v' + value['version'] and (tag is None or tag == value['tag']),
            'Release tag/version differs from its selected stable release')
    require(isinstance(value['source_commit'], str)
            and re.fullmatch(r'[0-9a-f]{40}', value['source_commit']), 'Invalid release source commit')
    require(value['archive'] == ARCHIVE and type(value['archive_size']) is int
            and 0 < value['archive_size'] <= MAX_ARCHIVE, 'Invalid release archive identity')
    for field in ('archive_sha256', 'artifact_digest'):
        require(isinstance(value[field], str) and re.fullmatch(r'[0-9a-f]{64}', value[field]),
                'Invalid release digest')
    return value


def package_identity(root):
    """Inspect all bytes and modes without importing candidate Python."""
    require(stat.S_ISDIR(root.lstat().st_mode), 'Portable root must be an ordinary directory')
    files = {}; total = 0; count = 0
    for directory, dirs, names in os.walk(root, followlinks=False,
                                          onerror=lambda e: (_ for _ in ()).throw(e)):
        count += len(dirs) + len(names)
        require(count <= MAX_ENTRIES, 'Release inventory exceeds its entry bound')
        for name in dirs + names:
            path = Path(directory)/name
            mode = path.lstat().st_mode
            require(stat.S_ISREG(mode) or stat.S_ISDIR(mode), 'Unsafe release file type')
            if name in dirs:
                require(mode & 0o500 == 0o500, 'Unreadable release directory')
                continue
            raw = regular(path, MAX_FILE); total += len(raw)
            require(total <= MAX_BYTES and stat.S_IMODE(mode) in (0o644, 0o755),
                    'Release bytes or modes violate the portable bound')
            files[path.relative_to(root).as_posix()] = {'sha256': digest(raw), 'mode': oct(stat.S_IMODE(mode))}
    require('SKILL.md' in files, 'Release lacks its portable root')
    signers = (PRINCIPAL + ' namespaces="' + NAMESPACE + '" ' + PUBLIC_KEY + '\n').encode()
    require(regular(root/'keys/allowed-signers', 4096) == signers, 'Portable key differs from the pinned publisher')
    text = regular(root/'SKILL.md', MAX_FILE).decode('utf-8')
    declarations = re.findall(r'^metadata: (\{[^\n]+\})$', text, re.MULTILINE)
    require(len(declarations) == 1, 'Release needs one version declaration')
    version = parse(declarations[0].encode()).get('version')
    require(isinstance(version, str) and len(version) <= 64 and VERSION.fullmatch(version), 'Invalid portable release version')
    return version, inventory_digest(files), files


def extract(raw, destination, manifest):
    try:
        _extract(raw, destination, manifest)
    except (tarfile.TarError, EOFError) as exc:
        raise DistributionError('Invalid signed portable archive') from exc


def _extract(raw, destination, manifest):
    require(len(raw) == manifest['archive_size'] and digest(raw) == manifest['archive_sha256'],
            'Archive differs from the signed release identity')
    # Bound inflation before tarfile parses even a signed malformed PAX header.
    with gzip.GzipFile(fileobj=io.BytesIO(raw)) as stream:
        expanded = stream.read(MAX_TAR + 1)
    require(len(expanded) <= MAX_TAR, 'Release expansion exceeds its bound')
    destination.mkdir(mode=0o700)
    seen = set(); total = 0
    with tarfile.open(fileobj=io.BytesIO(expanded), mode='r:') as archive:
        for member in archive:
            name = member.name
            path = PurePosixPath(name)
            require(len(seen) < MAX_ENTRIES and name not in seen and path.as_posix() == name
                    and not path.is_absolute() and '..' not in path.parts
                    and (name.startswith(PREFIX) or (member.isdir() and name in ('skills', 'skills/agent-memory'))),
                    'Unsafe, duplicate or excessive release member')
            seen.add(name)
            require(not member.pax_headers and member.mode in (0o644, 0o755)
                    and member.type in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE),
                    'Unsafe release member type or mode')
            target = destination.joinpath(*path.parts)
            if member.isdir():
                require(member.mode == 0o755 and member.size == 0, 'Invalid release directory')
                target.mkdir(mode=0o755, parents=True, exist_ok=True)
                continue
            total += member.size
            require(0 <= member.size <= MAX_FILE and total <= MAX_BYTES, 'Release member exceeds its byte bound')
            target.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
            with archive.extractfile(member) as stream:
                data = stream.read(MAX_FILE + 1)
            require(len(data) == member.size, 'Truncated release member')
            with target.open('xb') as stream:
                stream.write(data)
            target.chmod(member.mode)
    version, identity, _ = package_identity(destination/'skills/agent-memory')
    require(version == manifest['version'] and identity == manifest['artifact_digest'],
            'Portable identity differs from its signed declaration')


def download(url, maximum, *, json_response=False):
    require(url.startswith('https://'), 'Release acquisition requires HTTPS')
    require(shutil.which('curl'), 'curl is required for bounded HTTPS release acquisition')
    def bound_output():
        _, hard = resource.getrlimit(resource.RLIMIT_FSIZE)
        limit = maximum + 1 if hard == resource.RLIM_INFINITY else min(maximum + 1, hard)
        resource.setrlimit(resource.RLIMIT_FSIZE, (limit, hard))
    with tempfile.TemporaryFile() as output:
        result = subprocess.run(['curl', '--disable', '--fail', '--silent', '--show-error', '--location',
                                 '--proto', '=https', '--proto-redir', '=https', '--max-redirs', '5',
                                 '--max-time', '30', '--max-filesize', str(maximum),
                                 '--header', 'Accept: ' + ('application/vnd.github+json' if json_response else 'application/octet-stream'), '--', url],
                                stdout=output, stderr=subprocess.PIPE, timeout=35,
                                preexec_fn=bound_output)
        require(result.returncode == 0, 'Signed release download failed (curl exit '
                + str(result.returncode) + '); check release availability and network access')
        output.seek(0); raw = output.read(maximum + 1)
    require(len(raw) <= maximum, 'Release download exceeds its bound')
    return raw


def probe():
    """Authenticate the latest release identity before fetching its payload."""
    locator = parse(download(API + '/latest', MAX_METADATA, json_response=True))
    require(isinstance(locator, dict) and locator.get('draft') is False and locator.get('prerelease') is False
            and isinstance(locator.get('tag_name'), str), 'Stable release locator is unavailable')
    tag = locator['tag_name']
    require(len(tag) <= 65 and tag.startswith('v') and VERSION.fullmatch(tag[1:]), 'Invalid stable release tag')
    base = ASSETS + '/' + tag + '/'
    raw = download(base + MANIFEST, MAX_METADATA)
    signature = download(base + SIGNATURE, 4096)
    manifest = verify_manifest(raw, signature, tag)
    return manifest, raw, signature, base


def acquire(destination, *, observation=None):
    manifest, raw, signature, base = observation if observation is not None else probe()
    extract(download(base + ARCHIVE, manifest['archive_size']), destination, manifest)
    (destination/MANIFEST).write_bytes(raw)
    (destination/SIGNATURE).write_bytes(signature)
    return manifest


def installed(root):
    require(stat.S_ISDIR((root/'skills').lstat().st_mode), 'Release skills directory is redirected')
    manifest = verify_manifest(regular(root/MANIFEST, MAX_METADATA), regular(root/SIGNATURE, 4096),
                               installed_root=root)
    version, identity, _ = package_identity(root/'skills/agent-memory')
    require(version == manifest['version'] and identity == manifest['artifact_digest'],
            'Installed portable identity differs from its signed release')
    require(set(p.name for p in root.iterdir()) == {'skills', MANIFEST, SIGNATURE}
            and set(p.name for p in (root/'skills').iterdir()) == {'agent-memory'},
            'Unexpected content in the signed source layout')
    return manifest


import sys
sys.dont_write_bytecode = True

class UsageError(ValueError):
    pass

def usage(condition, message):
    if not condition: raise UsageError(message)

def bootstrap():
    require(sys.version_info >= (3, 11), 'Python 3.11+ is required')
    args = sys.argv[1:]
    choices = {}; index = 0
    while index < len(args):
        word = args[index]
        option, separator, value = word.partition('=')
        if option in ('--remote', '--ref', '--development-source', '--mode', '--presets', '--target-dir', '--bin-dir'):
            if not separator:
                index += 1
                usage(index < len(args), option + ' requires a value')
                value = args[index]
            usage(value and not value.startswith('--'), option + ' requires a value')
            if option in ('--remote', '--ref', '--development-source'): choices[option] = value
        else:
            usage(word in ('--check','--dry-run','--no-guidance','--json','--verbose','-v'),
                  'Unknown install option: '+word+' (use --help)')
        index += 1
    usage(not ('--development-source' in choices and ('--remote' in choices or '--ref' in choices)),
          'Choose development source or managed remote/ref, not both')
    with tempfile.TemporaryDirectory(prefix='am-bootstrap-') as temporary:
        root = Path(temporary)/'source'
        explicit = bool(choices)
        if '--development-source' in choices:
            root = Path(choices['--development-source']).expanduser().resolve(strict=True)
        elif explicit:
            remote = choices.get('--remote', 'https://github.com/robertelee78/agent-memory.git')
            ref = choices.get('--ref', 'main')
            usage(ref and not ref.startswith('-'), 'Invalid explicit source branch')
            result = subprocess.run(['git', 'clone', '--quiet', '--depth', '1', '--single-branch',
                                     '--branch', ref, '--', remote, str(root)], capture_output=True,
                                    timeout=120, env={**os.environ, 'GIT_TERMINAL_PROMPT': '0'})
            if result.returncode:
                print('am: Could not stage the operator-selected source',file=sys.stderr)
                raise SystemExit(result.returncode)
        else:
            acquire(root)
        pack = root/'skills/agent-memory' if (root/'skills/agent-memory').is_dir() else root
        sys.path.insert(0, str(pack/'scripts'))
        if not explicit:
            from agent_memory import installation, console
            installation.BOOTSTRAP_SOURCE = root
            return console.main(['install', *args])
        # Explicit custom/development selection is its own trust decision, as in
        # the existing installer; it is never labeled publisher-authenticated.
        import runpy
        entry = pack/'scripts/am_console.py'
        if not entry.is_file(): entry = pack/'scripts/installer.py'
        sys.argv = [str(entry), 'install', *args]
        runpy.run_path(str(entry), run_name='__main__')
        return 0

try:
    raise SystemExit(bootstrap())
except (OSError, ValueError, KeyError, TypeError, RecursionError, subprocess.SubprocessError) as error:
    if '--json' in sys.argv:
        print(json.dumps({'operation': 'install', 'verified': False, 'error': str(error)}))
    else:
        print('am: ' + str(error), file=sys.stderr)
    raise SystemExit(2 if isinstance(error, UsageError) else 1)
AM_VERIFIED_INSTALLER
fi
