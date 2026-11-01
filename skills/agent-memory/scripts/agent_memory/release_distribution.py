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
import signal
import time
from urllib.parse import urlsplit
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


DOWNLOAD_ATTEMPTS = 3
DOWNLOAD_BUDGET = 30.0
STDERR_LIMIT = 8192
RETRY_CURL = {5, 6, 7, 16, 18, 28, 52, 55, 56, 92}
RETRY_HTTP = {408, 500, 502, 503, 504}


def download_location(url):
    parsed = urlsplit(url)
    host = parsed.hostname or 'configured HTTPS host'
    if not re.fullmatch(r'[A-Za-z0-9.-]{1,253}', host):
        host = 'configured HTTPS host'
    name = parsed.path.rsplit('/', 1)[-1]
    stage = {MANIFEST: 'release manifest', SIGNATURE: 'publisher signature',
             ARCHIVE: 'release archive', 'install.sh': 'bootstrap readback'}.get(name, 'release download')
    if parsed.path.endswith('/latest'):
        stage = 'release discovery'
    return {'stage': stage, 'host': host}


def safe_download_detail(raw):
    text = ''.join(c for c in raw.decode('utf-8', 'replace')
                   if 32 <= ord(c) < 127 or c in '\n\r\t')
    text = re.sub(r'(?im)\b(?:authorization|proxy-authorization|cookie|set-cookie)\s*:.*$',
                  '[redacted authentication header]', text)
    text = re.sub(r'(?i)https?://\S+', '[redacted URL]', text)
    text = re.sub(r'\S+@\S+', '[redacted userinfo]', text)
    assignment = '(?:"[^"\\n]*"|\'[^\'\\n]*\'|\\S+)'
    text = re.sub(r'(?i)\b(?:token|access_token|password|passwd|secret|api[_-]?key)\s*[:=]\s*' + assignment,
                  '[redacted secret]', text)
    text = re.sub(r'\b[A-Z][A-Z0-9_]*\s*=\s*' + assignment,
                  '[redacted environment value]', text)
    return ' '.join(text.split())[:512]


def retain_download_failure(value):
    """Retain sanitized evidence only; never write inside runtime or repository."""
    raw = (json.dumps(value, sort_keys=True, indent=2) + '\n').encode()
    require(len(raw) <= 8192, 'Download diagnostic exceeds its private report bound')
    parent = Path('/tmp').resolve(strict=True)
    node = Path(parent.anchor)
    for part in (None, *parent.parts[1:]):
        if part is not None:
            node = node / part
        info = node.lstat()
        require(stat.S_ISDIR(info.st_mode) and info.st_uid == 0
                and (not info.st_mode & 0o022 or info.st_mode & stat.S_ISVTX),
                'Unsafe download diagnostic temporary root')
    root = Path(tempfile.mkdtemp(prefix='am-download-failure-', dir=parent))
    fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        require(info.st_uid == os.geteuid() and stat.S_IMODE(info.st_mode) == 0o700,
                'Download diagnostic directory must be private and current-owner')
        record = os.open('failure.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                         0o600, dir_fd=fd)
        with os.fdopen(record, 'wb') as stream:
            stream.write(raw); stream.flush(); os.fsync(stream.fileno())
        named = root.lstat()
        require((named.st_dev, named.st_ino) == (info.st_dev, info.st_ino),
                'Download diagnostic directory changed before readback')
    finally:
        os.close(fd)
    return str(root/'failure.json')


class DownloadError(DistributionError):
    """Terminal transport evidence and actionable guidance, never payload bytes."""
    def __init__(self, location, attempts, elapsed):
        last = attempts[-1] if attempts else {'reason': 'deadline', 'curl_exit': None,
                                              'http_status': None, 'detail': ''}
        category = last['reason']
        if category == 'transport':
            category = {5:'proxy DNS failure',6:'DNS failure',7:'connection failure',16:'HTTP/2 failure',
                        18:'incomplete response',28:'transfer timeout',52:'empty response',55:'send failure',
                        56:'receive failure',92:'HTTP/2 stream failure',22:'HTTP rejection',
                        35:'TLS handshake failure',60:'certificate validation failure',
                        77:'certificate configuration failure',23:'local staging write failure'}.get(last['curl_exit'], 'transport failure')
        self.details = {'schema': 'agent-memory/download-failure-v1', **location,
                        'attempts': attempts, 'attempt_count': len(attempts),
                        'elapsed_seconds': round(elapsed, 3), 'budget_seconds': DOWNLOAD_BUDGET,
                        'reason': category, 'curl_exit': last['curl_exit'],
                        'http_status': last['http_status'], 'activation_accepted': False}
        try:
            self.details['diagnostic_path'] = retain_download_failure(self.details)
        except (OSError, ValueError, TypeError, DistributionError):
            self.details['diagnostic_path'] = None
            self.details['diagnostic_error'] = 'Could not retain the private diagnostic; save this error text.'
        reason = last['reason']; status = last['http_status']; code = last['curl_exit']
        if reason == 'size-limit':
            guidance = 'Check expected release size and local temporary-storage/file-size limits; report a persistent mismatch.'
        elif code in (35, 51, 58, 59, 60, 64, 66, 77, 82, 83, 90, 91):
            guidance = 'Check the system clock, trusted certificates and any TLS-inspecting proxy configuration, then retry.'
        elif status == 429:
            guidance = 'The server rate-limited this request. Wait for its limit to reset, then retry the same command.'
        elif status in (401, 403):
            guidance = 'Check server access restrictions, proxy policy and GitHub API rate limits, then retry.'
        elif status == 404:
            guidance = 'The release endpoint or asset is unavailable. Check the selected release or report the missing asset.'
        elif code == 23 or reason == 'process-start':
            guidance = 'Check that curl is executable and private temporary storage has space and writable permissions.'
        else:
            guidance = 'Retry the same command after checking network, DNS, VPN/proxy access and connection speed to the reported host.'
        self.repair = 'Failed downloaded bytes were not accepted for activation. ' + guidance
        if self.details['diagnostic_path']:
            self.repair += ' Private diagnostic: ' + self.details['diagnostic_path']
        else:
            self.repair += ' ' + self.details['diagnostic_error']
        status_text = str(status) if status is not None else 'unknown'
        exit_text = str(code) if code is not None else 'not completed'
        message = ('Signed release download failed: ' + location['stage'] + ' from ' + location['host']
                   + ' after ' + str(len(attempts)) + ' attempt(s) (curl ' + exit_text
                   + '; HTTP ' + status_text + '; ' + category + ').')
        if last['detail']:
            message += ' ' + last['detail']
        super().__init__(message)


def download(url, maximum, *, json_response=False):
    require(url.startswith('https://'), 'Release acquisition requires HTTPS')
    require(shutil.which('curl'), 'curl is required for bounded HTTPS release acquisition')
    location = download_location(url)
    start = time.monotonic(); deadline = start + DOWNLOAD_BUDGET; attempts = []
    for number in range(1, DOWNLOAD_ATTEMPTS + 1):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        def bound_output():
            _, hard = resource.getrlimit(resource.RLIMIT_FSIZE)
            bound = max(maximum + 1, STDERR_LIMIT + 1)
            resource.setrlimit(resource.RLIMIT_FSIZE, (bound if hard == resource.RLIM_INFINITY else min(bound, hard), hard))
        attempt_start = time.monotonic(); code = None; status = None; detail = ''; reason = 'transport'
        with tempfile.TemporaryFile() as body, tempfile.TemporaryFile() as metadata, tempfile.TemporaryFile() as errors:
            try:
                result = subprocess.run(['curl', '--disable', '--fail', '--silent', '--show-error', '--location',
                    '--proto', '=https', '--proto-redir', '=https', '--max-redirs', '5',
                    '--max-time', str(remaining), '--connect-timeout', str(min(5.0, remaining)),
                    '--max-filesize', str(maximum), '--header', 'Accept: ' +
                    ('application/vnd.github+json' if json_response else 'application/octet-stream'),
                    '--output', '/dev/fd/' + str(body.fileno()), '--write-out', '%{http_code}', '--', url],
                    stdout=metadata, stderr=errors, pass_fds=(body.fileno(),), close_fds=True,
                    timeout=remaining, preexec_fn=bound_output)
                code = result.returncode
            except subprocess.TimeoutExpired:
                reason = 'deadline'
            except OSError as exc:
                reason = 'process-start'; detail = safe_download_detail(str(exc).encode())
            metadata.seek(0); raw_status = metadata.read(17)
            if re.fullmatch(rb'[0-9]{3}', raw_status) and raw_status != b'000':
                status = int(raw_status)
            errors.seek(0); detail = detail or safe_download_detail(errors.read(STDERR_LIMIT))
            size = os.fstat(body.fileno()).st_size
            if size > maximum or code in (63, -signal.SIGXFSZ):
                reason = 'size-limit'
            elif code is not None and code < 0:
                reason = 'process-terminated'
            if code == 0 and reason == 'transport':
                body.seek(0); raw = body.read(maximum + 1)
                require(len(raw) <= maximum, 'Release download exceeds its bound')
                return raw
        attempts.append({'attempt': number, 'curl_exit': code, 'http_status': status,
                         'reason': reason, 'detail': detail, 'received_bytes': size,
                         'seconds': round(time.monotonic() - attempt_start, 3)})
        retry = reason == 'transport' and (((status is None or status < 400) and code in RETRY_CURL)
                                           or (code == 22 and status in RETRY_HTTP))
        if not retry or number == DOWNLOAD_ATTEMPTS:
            break
        delay = 0.25 * number
        if deadline - time.monotonic() <= delay:
            break
        time.sleep(delay)
    raise DownloadError(location, attempts, time.monotonic() - start)


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
