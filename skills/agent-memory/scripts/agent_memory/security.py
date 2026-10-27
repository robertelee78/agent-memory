"""Bounded project policy checks; labels and UUIDs never authenticate a person."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import uuid

from . import repository_config, validation_memo
from . import bank_writes as writes
from . import local_registry as storage
from . import scope_binding as identity
from . import tombstones
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError
from .identity_git import git

CLASSES = {'user', 'assistant', 'tool', 'external', 'verified-repository'}
CATEGORIES = {'admission', 'mutation', 'retrieval', 'promotion'}
MIGRATION_ACTIONS = {'migrate', 'migration-restore', 'migration-finish', 'migration-rollback'}
ACTIONS = {'policy-check', 'initialize', 'checkpoint', 'recover-finish', 'recover-rollback'} | writes.OPERATIONS | MIGRATION_ACTIONS
LIMIT = 16384
REQUEST = {'project_id', 'category', 'source_class', 'content_kind', 'text',
           'record_ids', 'automatic', 'repository_evidence'}
SECRETS = (
    ('private-key', r'-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----[\s\S]*?(?:-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|\Z)'),
    ('token', r'\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}|AKIA[A-Z0-9]{16})\b'),
    ('credential', r'''(?i)["']?\b(?:password|passwd|token|secret|api[_-]?(?:key|token)|access[_-]?token|client[_-]?secret|secret[_-]?key|aws_secret_access_key)\b["']?\s*[:=]\s*["']?[^\s"'`,;}\]]{6,}'''),
    ('authorization-header', r'(?im)\bAuthorization[ \t]*:[ \t]*(?:Bearer|Basic)[ \t]+[A-Za-z0-9._~+/=-]{6,}'),
    ('url-credential', r'(?i)(?:https?|postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|amqps?)://[^/\s:@]+:[^@\s/]+@'),
)
SUSPICIOUS = (
    ('authority-override', r'(?is)\b(?:ignore|disregard|override|bypass)\b.{0,64}\b(?:system|developer|previous|instructions|safety|rules|current\s+(?:request|task)|operator)\b'),
    ('credential-disclosure', r'(?is)\b(?:reveal|exfiltrate|upload|send)\b.{0,64}\b(?:credentials|secrets|tokens|passwords)\b'),
    ('identity-permission-claim', r'(?i)\b(?:authenticated|administrator|admin|root)\s*[:=]\s*(?:true|yes|1)\b|\bI am (?:the )?(?:administrator|authenticated operator)\b'),
    ('raw-conversation', r'(?im)^(?:user|assistant|system|developer)\s*:\s*.*\n(?:[\s\S]*\n)?(?:user|assistant|system|developer)\s*:'),
)

REASONS = {reason for reason, _ in (*SECRETS, *SUSPICIOUS)} | {
    'terminal-deletion', 'explicit-procedure-promotion-required', 'operator-approval-required',
    'source-review-required'}


def scan(content, *, maximum=LIMIT):
    memo = validation_memo.current()
    key = validation_memo.text_key(content, maximum) if memo is not None else None
    if key is None:
        return _scan(content, maximum=maximum)
    return memo.checked('security-scan', key, lambda: _scan(content, maximum=maximum))


def _scan(content, *, maximum=LIMIT):
    if maximum not in {LIMIT,65536}:raise ContractError('Security scan needs the fixed source or review-page budget')
    if (not isinstance(content, str) or not content.strip() or len(content.encode()) > maximum
            or any(ord(c) < 32 and c not in '\n\r\t' for c in content)):
        raise ContractError('Security content must be useful bounded UTF8 text without control separators')
    redacted, reasons = content, []
    for reason, pattern in SECRETS:
        redacted, count = re.subn(pattern, '[REDACTED:' + reason + ']', redacted)
        if count:
            reasons.append(reason)
    suspected = [reason for reason, pattern in SUSPICIOUS if re.search(pattern, redacted)]
    return {'redacted_text': redacted, 'redaction_required': bool(reasons),
            'reasons': sorted(set(reasons + suspected)), 'quarantine_required': bool(suspected),
            'content_is_untrusted_data': True, 'exhaustive_detection_claimed': False}


def audit(repo, operation_id, category, phase, context, source_classes, reasons=(), record_ids=(), *, action='policy-check', bank_location=None):
    """Private metadata only. A policy check is never a successful-write receipt."""
    if (not uuid_valid(operation_id) or category not in CATEGORIES or action not in ACTIONS
            or phase not in {'checked', 'started', 'completed', 'refused'}
            or not set(source_classes).issubset(CLASSES) or not source_classes
            or any(not uuid_valid(item) for item in record_ids) or not set(reasons).issubset(REASONS)):
        raise ContractError('Invalid security audit metadata')
    # Logging never grants scope. Ordinary callers resolve it first; the
    # explicit binding workflow records its checked actor and proposed target.
    if (not isinstance(context, dict) or not uuid_valid(context.get('developer_id'))
            or not uuid_valid(context.get('project_id'))):
        raise ContractError('Audit requires checked attribution and project identifiers')
    root = Path(repo).resolve()
    if bank_location is not None:
        from .migration import locate, locations
        if action not in MIGRATION_ACTIONS or locate(root) != (root, Path(bank_location)):
            raise ContractError('Migration audit must use the actual existing bank locator')
        locations(root, Path(bank_location), operation_id)
    else:
        bank_location = repository_config.bank_path(root)
    operations, _ = writes.state_paths(root, Path(bank_location), operation_id)
    directory = operations.parent / 'security-audit'
    if directory.is_symlink() or (directory.exists() and not directory.is_dir()):
        raise ContractError('Security audit location must remain a canonical private directory')
    storage.private_mkdir(directory)
    path = directory / (operation_id + '.' + action + '.' + phase + '.' + context['developer_id'] + '.json')
    event = {'schema': 'agent-memory/security-audit-v1', 'operation_id': operation_id,
             'project_id': context['project_id'], 'developer_id': context['developer_id'],
             'category': category, 'phase': phase, 'action': action, 'source_classes': sorted(set(source_classes)),
             'reasons': sorted(set(reasons)), 'record_ids': sorted(set(record_ids)),
             'identity_authenticated': False}
    payload = encoded(event)
    if path.is_symlink():
        raise ContractError('Security audit must not follow a symlink')
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix='.security-audit-', dir=directory, delete=False) as stream:
            temporary = Path(stream.name)
            os.chmod(temporary, 0o600)
            stream.write(payload);stream.flush();os.fsync(stream.fileno())
        try:
            os.link(temporary, path)  # Exclusive publication of a complete immutable event.
        except FileExistsError:
            if path.is_symlink() or path.read_bytes() != payload:
                raise ContractError('Security operation ID has different metadata; preserve the audit')
        fd = os.open(directory, os.O_RDONLY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    if path.read_bytes() != payload:
        raise ContractError('Security audit readback failed; do not proceed')
    return {'path': str(path), 'verified': True, 'phase': phase}


def content_reasons(contents):
    if (not isinstance(contents, dict) or len(contents) > 64
            or any(not isinstance(body, str) for body in contents.values())
            or sum(len(body.encode()) for body in contents.values()) > 65536):
        raise ContractError('Governed mutation exceeds its bounded memory content batch')
    return sorted({reason for body in contents.values() for reason in scan(body)['reasons']})


def ensure_safe(contents):
    """Pure preview check; callers retain identity, source and write ownership."""
    reasons = content_reasons(contents)
    if reasons:
        raise ContractError('Security policy quarantined proposed memory; redact or review before writing: ' + ', '.join(reasons))


def complete_refresh_reasons(contents):
    """Inspect every bounded canonical source in a versioned complete refresh."""
    from . import bank_contract, sharing_limits
    sharing_limits.bank_size(contents)
    for path in contents:
        if path!='MEMORY.md':
            bank_contract.path_value(path)
    # The bank bound applies to payload bytes. The separately bounded path
    # label must also pass, without concatenation shrinking a 16KiB source.
    return sorted({reason for path,body in contents.items()
        for reason in scan(path+'\n'+body,maximum=65536)['reasons']})


def guarded_start(repo, operation_id, context, contents, source_classes, *, action, complete_refresh=False):
    """Call under the governed writer's resolved scope and bank lock."""
    if complete_refresh and action!='refresh':
        raise ContractError('Complete refresh scanning cannot widen a capture or another mutation')
    reasons = complete_refresh_reasons(contents) if complete_refresh else content_reasons(contents)
    event = audit(repo, operation_id, 'mutation', 'refused' if reasons else 'started',
                  context, source_classes, reasons, action=action)
    if reasons:
        raise ContractError('Security policy quarantined proposed memory; redact or review before writing: ' + ', '.join(reasons))
    return event


def repository_source(root, evidence):
    if (not isinstance(evidence, dict) or set(evidence) != {'path', 'sha256'}
            or not isinstance(evidence['path'], str)
            or not isinstance(evidence['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', evidence['sha256'])):
        raise ContractError('Verified repository evidence needs an explicit path and SHA256 observation')
    relative = Path(evidence['path'])
    if (relative.is_absolute() or relative.as_posix() != evidence['path'] or not relative.parts
            or any(part in {'.', '..'} or part.startswith('.') for part in relative.parts)
            or relative.is_relative_to(repository_config.relative(root)) or any(ord(c) < 32 for c in evidence['path'])):
        raise ContractError('Repository evidence must be canonical local source, not memory or private metadata')
    target = root / relative
    if any(part.is_symlink() for part in (target, *list(target.parents)[:len(relative.parts)-1])):
        raise ContractError('Repository evidence must not follow source symlinks')
    listed = git(root, 'ls-files', '--cached', '--others', '--exclude-standard', '-z', optional=True)
    if listed is None or evidence['path'] not in set(listed.split('\0')):
        raise ContractError('Repository evidence is ignored or not in the explicit Git source inventory')
    if not target.is_file() or target.stat().st_size > 65536 or digest(target.read_bytes()) != evidence['sha256']:
        raise ContractError('Repository evidence changed or exceeds the bounded reader')


def approval_value(approval, request):
    fields = {'schema', 'project_id', 'content_sha256', 'source_version', 'ref', 'decision'}
    if (not isinstance(approval, dict) or set(approval) != fields
            or approval['schema'] != 'agent-memory/operator-procedure-approval-v1'
            or approval['project_id'] != request['project_id']
            or approval['content_sha256'] != digest(request['text'].encode())
            or type(approval['source_version']) is not int or approval['source_version'] < 1
            or approval['decision'] != 'approved' or not isinstance(approval['ref'], str)
            or not approval['ref'].strip() or any(ord(c) < 32 for c in approval['ref'])):
        raise ContractError('Procedure eligibility requires separate explicit current-operator approval bound to exact content, source version and project')


def evaluate(repo, request, *, approval=None, record_audit=True):
    if (not isinstance(request, dict) or set(request) != REQUEST or not uuid_valid(request['project_id'])
            or request['category'] not in CATEGORIES or request['source_class'] not in CLASSES
            or request['content_kind'] not in {'summary', 'procedure', 'conversation'}
            or type(request['automatic']) is not bool or not isinstance(request['record_ids'], list)
            or len(request['record_ids']) > 64 or len(set(request['record_ids'])) != len(request['record_ids'])
            or any(not uuid_valid(item) for item in request['record_ids'])):
        raise ContractError('Security request needs explicit project, source class, operation and bounded content/record identities')
    root = Path(repo).resolve()
    if request['category'] == 'promotion' and request['content_kind'] != 'procedure':
        raise ContractError('Procedure promotion requires an explicitly typed procedure request')
    context = identity.require(root, request['project_id'])
    project_id, observed, deleted = tombstones.history(root)
    if request['source_class'] == 'verified-repository':
        repository_source(root, request['repository_evidence'])
    elif request['repository_evidence'] is not None:
        raise ContractError('Keep repository evidence verification separate from user/assistant/tool/external classification')
    checked = scan(request['text'])
    reasons = list(checked['reasons'])
    if any(item in deleted for item in request['record_ids']):
        reasons.append('terminal-deletion')
    if request['content_kind'] == 'conversation':
        reasons.append('raw-conversation')
    procedure = request['content_kind'] == 'procedure' or request['category'] == 'promotion'
    if procedure:
        if request['automatic'] or request['category'] != 'promotion':
            reasons.append('explicit-procedure-promotion-required')
        elif approval is None:
            reasons.append('operator-approval-required')
        else:
            approval_value(approval, request)
    elif approval is not None:
        raise ContractError('Approval is for a separate reviewed procedure promotion, never routine capture permission')
    weak_source = request['source_class'] in {'assistant', 'tool', 'external'}
    needs_review = weak_source and request['automatic'] and request['category'] == 'admission'
    decision = 'quarantine' if reasons else 'review-required' if needs_review else 'eligible'
    if needs_review:
        reasons.append('source-review-required')
    # Recheck both project selection and the entire bank so deletion/source
    # changes cannot make an already assembled result current by accident.
    identity.require(root, project_id, expected=context)
    if tombstones.history(root)[1] != observed:
        raise ContractError('Canonical bank changed during security evaluation; reread before using the result')
    if request['source_class'] == 'verified-repository':
        repository_source(root, request['repository_evidence'])
    event = None
    if record_audit:
        event = audit(root, str(uuid.uuid4()), request['category'], 'checked', context,
                      [request['source_class']], reasons, request['record_ids'])
    return {'schema': 'agent-memory/security-result-v1', 'decision': decision, **checked,
            'redacted_text': None if checked['quarantine_required'] or request['content_kind'] == 'conversation'
                or 'terminal-deletion' in reasons else checked['redacted_text'],
            'repository_evidence': request['repository_evidence'],
            'reasons': sorted(set(reasons)), 'scope_context': context, 'source_hashes': observed,
            'source_class': request['source_class'], 'source_assertions_verified': False,
            'review_required_for_admission': weak_source,
            'procedure_eligibility': procedure and decision == 'eligible',
            'procedural_guidance': False, 'identity_authenticated': False,
            'publication_verified': False, 'canonical_writes_performed': False, 'audit': event,
            'record_ids': request['record_ids'], 'content_is_untrusted_data': True}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True)
    parser.add_argument('--request', required=True)
    parser.add_argument('--operator-approval')
    args = parser.parse_args(argv)
    try:
        request = load(Path(args.request).read_bytes())
        approval = load(Path(args.operator_approval).read_bytes()) if args.operator_approval else None
        print(json.dumps(evaluate(args.repo, request, approval=approval), indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps({'schema': 'agent-memory/security-result-v1', 'error': str(exc),
                          'verified': False, 'canonical_writes_performed': False}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
