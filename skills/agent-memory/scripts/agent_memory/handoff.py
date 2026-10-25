"""Bounded checkpoint qualification; references never grant outside-file access."""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import stat
from urllib.parse import unquote, urlsplit

from . import scopes
from .developer_identity import digest, load
from .frontmatter import ContractError
from .identity_git import git

SECTION = 'Checkpoint evidence observations'
SCHEMA = 'agent-memory/checkpoint-evidence-v1'
MAX_SOURCE = 65536


def reference_path(ref):
    match = re.fullmatch(r'\[[^\]\n]*\]\((?:<([^<>]+)>|([^()\s]+))\)', ref)
    value = (match[1] or match[2]) if match else ref
    if any(ord(c) < 32 for c in value):
        return None, 'unavailable'
    parsed = urlsplit(value)
    if parsed.scheme in {'http', 'https'}:
        return None, 'external-unverified'
    if parsed.scheme or value.startswith('/'):
        return None, 'outside-uninspected'
    decoded = unquote(parsed.path)
    if Path(decoded).is_absolute():
        return None, 'outside-uninspected'
    return decoded, None


def read_inside(root, bank, relative):
    """Walk pinned directory descriptors; never traverse a symlink or leave root."""
    if not isinstance(relative, str) or Path(relative).is_absolute():
        return None, None, 'outside-uninspected'
    parts = list(bank.relative_to(root).parts) + list(Path(relative).parts)
    descriptors, names = [], []
    try:
        descriptors.append(os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW))
        for index, part in enumerate(parts):
            if part == '..':
                if len(descriptors) == 1:
                    return None, None, 'outside-uninspected'
                os.close(descriptors.pop()); names.pop()
                continue
            if part in {'', '.'}:
                continue
            if part.startswith('.') or any(ord(c) < 32 or ord(c) == 127 for c in part):
                return None, None, 'unavailable'
            final = index == len(parts) - 1
            path = '/'.join([*names, part])
            if final:
                if Path(path).is_relative_to(bank.relative_to(root)):
                    return None, None, 'canonical-memory-reference'
                allowed = git(root, 'ls-files', '--cached', '--others', '--exclude-standard',
                              '--error-unmatch', '-z', '--', path, optional=True)
                if allowed is None or path not in allowed.split('\0'):
                    return None, None, 'unavailable'
            flags = os.O_RDONLY | os.O_NOFOLLOW | (os.O_NONBLOCK if final else os.O_DIRECTORY)
            fd = os.open(part, flags, dir_fd=descriptors[-1])
            if not final:
                descriptors.append(fd); names.append(part)
                continue
            try:
                before = os.fstat(fd)
                if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_size > MAX_SOURCE:
                    return None, None, 'unavailable'
                chunks, size = [], 0
                while size <= MAX_SOURCE:
                    chunk = os.read(fd, min(8192, MAX_SOURCE + 1 - size))
                    if not chunk:
                        break
                    chunks.append(chunk); size += len(chunk)
                after = os.fstat(fd)
                if (size > MAX_SOURCE or (before.st_size, before.st_mtime_ns, before.st_ctime_ns) !=
                        (after.st_size, after.st_mtime_ns, after.st_ctime_ns)):
                    raise ContractError('Referenced evidence changed while being inspected')
                return b''.join(chunks), path, None
            finally:
                os.close(fd)
        return None, None, 'unavailable'
    except OSError:
        return None, None, 'unavailable'
    finally:
        for fd in reversed(descriptors):
            os.close(fd)


def observe(repo, bank, references):
    root, bank = Path(repo), Path(bank)
    head = git(root, 'rev-parse', '--verify', 'HEAD', optional=True)
    branch = git(root, 'symbolic-ref', '--quiet', '--short', 'HEAD', optional=True)
    rows = []
    for ref in references:
        try:
            relative, kind = reference_path(ref)
        except ValueError:
            relative, kind = None, 'unavailable'
        raw, path = None, None
        if kind is None:
            raw, path, kind = read_inside(root, bank, relative)
        row = {'ref': ref, 'kind': kind or 'local-unpublished', 'sha256': None,
               'bytes': None, 'commit': None, 'blob': None}
        if raw is not None:
            row.update(sha256=digest(raw), bytes=len(raw))
            blob = git(root, 'rev-parse', '--verify', head + ':' + path, optional=True) if head else None
            if blob and re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}', blob):
                size = git(root, 'cat-file', '-s', blob)
                if size.isdigit() and int(size) <= MAX_SOURCE:
                    text = git(root, 'cat-file', 'blob', blob)
                    committed = os.fsencode(text)
                    if len(committed) + 1 == int(size):
                        committed += b'\n'
                    if len(committed) == int(size) and committed == raw:
                        row.update(kind='repository-committed', commit=head, blob=blob)
        rows.append(row)
    if (git(root, 'rev-parse', '--verify', 'HEAD', optional=True) != head or
            git(root, 'symbolic-ref', '--quiet', '--short', 'HEAD', optional=True) != branch):
        raise ContractError('Checkout changed during evidence observation')
    return {'schema': SCHEMA, 'references': rows, 'observed_checkout': {'head': head, 'branch': branch},
            'publication_verified': False, 'identity_authenticated': False}


def validate_observation(value):
    if (not isinstance(value, dict) or set(value) != {'schema', 'references', 'observed_checkout',
            'publication_verified', 'identity_authenticated'} or value['schema'] != SCHEMA
            or value['publication_verified'] is not False or value['identity_authenticated'] is not False
            or not isinstance(value['references'], list) or not 1 <= len(value['references']) <= 16
            or not isinstance(value['observed_checkout'], dict)
            or set(value['observed_checkout']) != {'head', 'branch'}):
        raise ContractError('Unknown checkpoint evidence observation')
    head = value['observed_checkout']['head']
    branch = value['observed_checkout']['branch']
    if ((head is not None and (not isinstance(head, str) or not re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}', head)))
            or (branch is not None and (not isinstance(branch, str) or len(branch) > 4096
                                      or any(ord(c) < 32 for c in branch)))):
        raise ContractError('Invalid observed checkpoint checkout')
    for row in value['references']:
        if (not isinstance(row, dict) or set(row) != {'ref', 'kind', 'sha256', 'bytes', 'commit', 'blob'}
                or not isinstance(row['ref'], str) or len(row['ref'].encode()) > 1024
                or row['kind'] not in {'repository-committed', 'local-unpublished', 'external-unverified',
                    'outside-uninspected', 'unavailable', 'canonical-memory-reference'}):
            raise ContractError('Invalid checkpoint evidence reference')
        if row['sha256'] is not None and (not isinstance(row['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', row['sha256'])):
            raise ContractError('Invalid checkpoint evidence hash')
        if row['sha256'] is None:
            if any(row[key] is not None for key in ('bytes', 'commit', 'blob')):
                raise ContractError('Uninspected references cannot contain invented byte evidence')
        elif type(row['bytes']) is not int or not 0 <= row['bytes'] <= MAX_SOURCE:
            raise ContractError('Invalid checkpoint evidence byte count')
        for key in ('commit', 'blob'):
            if row[key] is not None and (not isinstance(row[key], str) or not re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}', row[key])):
                raise ContractError('Invalid checkpoint committed evidence')
        if row['kind'] == 'repository-committed' and any(row[k] is None for k in ('sha256', 'commit', 'blob')):
            raise ContractError('Committed evidence requires its actual byte/blob observation')
    return value


def check_pins(repo, bank, references, expected):
    validate_observation(expected)
    if observe(repo, bank, references) != expected:
        raise ContractError('Checkpoint source evidence changed; preserve the proposal and inspect/review again')


def managed_cues(body, names):
    """Recognize reserved labels, including malformed headings, outside examples."""
    names = {name.casefold() for name in names}
    fence, offset, cues = None, 0, set()
    for line in body.splitlines(keepends=True):
        start = offset
        offset += len(line)
        raw = line.rstrip('\r\n')
        token = re.match(r' {0,3}(`{3,}|~{3,})(.*)', raw)
        if token:
            marker, rest = token.groups()
            if fence is None:
                if marker[0] != '`' or '`' not in rest:
                    fence = marker
            elif marker[0] == fence[0] and len(marker) >= len(fence) and not rest.strip():
                fence = None
            continue
        if fence is not None or raw.expandtabs(4).startswith('    '):
            continue
        # These adornments detect a cue only; they never validate a section.
        # Bare reserved labels include setext titles. The strict shared parser
        # below refuses unsupported/ambiguous managed formatting.
        title = re.sub(r'<[^<>]*>', '', raw).strip(' \t')
        title = re.sub(r'^#+[ \t]*', '', title)
        title = re.sub(r'[ \t]+#+[ \t]*$', '', title).strip(' \t*_~')
        if title.casefold() in names:
            cues.add(start)
    return cues


def sections(body):
    from .checkpoint import headings, FIELDS, EXTRA
    expected = {*FIELDS.values(), *EXTRA, SECTION}
    cues = managed_cues(body, expected)
    if not cues:
        return {}
    parsed = headings(body)
    if cues != {start for start, _, title in parsed if title in expected}:
        raise ContractError('Every reserved checkpoint label requires exact managed heading framing')
    found = {}
    for start, end, title in parsed:
        if title in expected:
            if title in found:
                raise ContractError('Duplicate checkpoint section')
            found[title] = body[start:end].split('\n', 1)[1].strip()
    return found


def json_section(value):
    match = re.fullmatch(r'```json\n(.*)\n```', value, re.S)
    if not match:
        raise ContractError('Checkpoint conditions/evidence require unambiguous JSON framing')
    return load(match[1].encode())


def reusable(base, current):
    """Retain original provenance for the same actual evidence, avoiding HEAD churn."""
    try:
        prior = validate_observation(json_section(sections(base).get(SECTION, '')))
        def identities(value):
            return [{key: row[key] for key in ('ref', 'kind', 'sha256', 'bytes', 'blob')} for row in value['references']]
        if identities(prior) == identities(current):
            return prior
    except (ContractError, ValueError, TypeError, KeyError):
        pass
    return current


def assess(body, project, context, repo, bank, *, inspect_only=False):
    from .checkpoint import list_text
    try:
        found = sections(body)
        if not found:
            return {'managed_checkpoint': False, 'applicable': None, 'assignment': False,
                    'statement_applicability': 'unassessed orientation'}
        conditions = scopes.conditions(json_section(found.get('Checkpoint conditions', '')), allow_unknown=True)
        decision = scopes.decide({'bank_project_id': project, 'project_id': project,
                                  'conditions': conditions, 'context': context})
    except (ContractError, ValueError, TypeError, KeyError):
        return {'managed_checkpoint': True, 'applicable': False, 'assignment': False,
                'conditions': None, 'reasons': ['unqualified-checkpoint-framing']}
    value = {'managed_checkpoint': True, 'applicable': decision['applicable'], 'assignment': False,
             'conditions': conditions, 'reasons': decision['reasons'], 'evidence_status': 'not-inspected-inapplicable'}
    if not decision['applicable'] and not inspect_only:
        return value
    try:
        stored = validate_observation(json_section(found.get(SECTION, '')))
        refs = [row['ref'] for row in stored['references']]
        if len(set(refs)) != len(refs) or found.get('Checkpoint evidence') != list_text(refs):
            raise ContractError('Evidence list differs from its historical observation')
        current = observe(repo, bank, refs)
        value.update(evidence_status='qualified-observations', evidence=[])
        for old, new in zip(stored['references'], current['references']):
            status = ('matching-local-bytes' if old['sha256'] is not None and old['sha256'] == new['sha256'] else
                      'changed-local-bytes' if old['sha256'] is not None and new['sha256'] is not None else
                      'unavailable-here' if old['sha256'] is not None else 'original-byte-binding-unverified')
            value['evidence'].append({'ref': old['ref'], 'recorded': old, 'current': new, 'qualification': status})
        value['evidence_bytes_match'] = all(row['qualification'] == 'matching-local-bytes' for row in value['evidence'])
    except (ContractError, ValueError, TypeError, KeyError):
        value['evidence_status'] = 'historical-or-unqualified-evidence'
    return value
