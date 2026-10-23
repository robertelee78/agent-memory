"""Checkpoint active context through one identity-bound observed replacement."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

from . import bank_contract as bank
from . import bank_transactions as transactions
from . import bank_writes as writes
from . import scope_binding as identity
from . import scopes
from . import security
from . import local_registry as storage
from .developer_identity import digest, load
from .frontmatter import ContractError

FIELDS = {'focus': 'Current focus', 'recent_changes': 'Recent changes', 'open_questions': 'Open questions', 'next_step': 'Next step'}
INPUT = {*FIELDS, 'conditions', 'evidence', 'harness', 'session', 'observed_active_sha256'}
EXTRA = ('Checkpoint conditions', 'Checkpoint evidence', 'Checkpoint provenance')
LIMIT = 16384


def result(operation, **values):
    return {'schema': 'agent-memory/checkpoint-result-v1', 'operation': operation,
            'publication_verified': False, 'identity_authenticated': False, **values}


def headings(body):
    """Conservative ATX/fenced profile; refuse unsupported ambiguous boundaries."""
    if any((ord(c) < 32 and c not in '\n\r\t') or c in '\x7f\x85\u2028\u2029' for c in body):
        raise ContractError('Unsupported control or Unicode line separator; inspect context before checkpointing')
    output, offset, fence, list_seen = [], 0, None, False
    reserved = {*FIELDS.values(), *EXTRA}
    for line in body.splitlines(keepends=True):
        raw = line.rstrip('\r\n')
        token = re.match(r' {0,3}(`{3,}|~{3,})(.*)', raw)
        if token:
            marker, rest = token.groups()
            if fence is None:
                if marker[0] != '`' or '`' not in rest:
                    fence = marker
            elif marker[0] == fence[0] and len(marker) >= len(fence) and not rest.strip():
                fence = None
        elif fence is None and not raw.expandtabs(4).startswith('    '):
            # A setext underline can refer to several preceding paragraph lines,
            # including indented continuation lines. Never guess its ownership.
            if re.fullmatch(r' {0,3}(?:-+|=+)[ \t]*', raw):
                raise ContractError('Setext/underline boundary requires explicit inspection before checkpointing; preserve the original context')
            if re.search(r'<(?:!--|/?[A-Za-z]|[?!])', raw):
                raise ContractError('Raw HTML requires explicit inspection or a complete fenced example before checkpointing')
            match = re.fullmatch(r'( {0,3})(#{1,6})(?:[ \t]+(.*))?[ \t]*', raw)
            if match:
                indent, hashes, content = match.groups()
                if indent and list_seen:
                    raise ContractError('Ambiguous nested-list heading requires explicit inspection before checkpointing')
                title = re.sub(r'(?:^|[ \t]+)#+[ \t]*$', '', content or '').strip(' \t')
                if len(hashes) != 2 and title in reserved:
                    raise ContractError('Reserved checkpoint section names require level-two headings; clarify the existing structure')
                # All ATX levels delimit preserved sections. Only the stable H2
                # names are managed fields; other levels never become fields.
                output.append((offset, title if len(hashes) == 2 else None))
                list_seen = False
            elif re.match(r' {0,3}(?:[-+*]|[0-9]{1,9}[.)])[ \t]+', raw):
                list_seen = True
        offset += len(line)
    if fence is not None:
        raise ContractError('Active context/input has an unterminated Markdown fence; inspect it before checkpointing')
    return [(start, output[index + 1][0] if index + 1 < len(output) else len(body), title)
            for index, (start, title) in enumerate(output)]


def text(value, limit=2048):
    if (not isinstance(value, str) or not value.strip() or len(value.encode()) > limit
            or any(ord(c) < 32 and c not in '\n\t' for c in value)):
        raise ContractError('Checkpoint fields require bounded useful Markdown without new sections')
    normalized = value.strip()
    if headings(normalized):
        raise ContractError('Checkpoint fields must not create unfenced headings after normalization')
    return normalized


def list_text(items):
    # Indent continuation lines so a fenced example remains inside its item and
    # cannot manufacture apparent root sections on a later checkpoint.
    return '\n'.join('- ' + text(item, 1024).replace('\n', '\n    ') for item in items)


def input_value(value):
    if not isinstance(value, dict) or set(value) != INPUT:
        raise ContractError('Checkpoint input requires four reviewed fields, conditions, evidence, harness/session and observed active hash')
    for field in ('focus', 'next_step', 'harness', 'session'):
        text(value[field], 2048 if field == 'focus' else 512)
    for field in ('recent_changes', 'open_questions', 'evidence'):
        if not isinstance(value[field], list) or len(value[field]) > 16:
            raise ContractError('Checkpoint lists are bounded to sixteen concise items')
        for item in value[field]:
            text(item, 1024)
    if not value['evidence'] or len(set(value['evidence'])) != len(value['evidence']):
        raise ContractError('Checkpoint evidence must contain unique source references, not a raw transcript')
    scopes.conditions(value['conditions'], allow_unknown=True)
    sha = value['observed_active_sha256']
    if sha is not None and (not isinstance(sha, str) or not re.fullmatch('[0-9a-f]{64}', sha)):
        raise ContractError('Observed active context must be SHA256, or null only for separate missing-context preparation')
    return value


def render(base, value, developer_id):
    value = input_value(value)
    desired = {FIELDS[field]: text(value[field]) if field in {'focus', 'next_step'} else
               list_text(value[field]) or 'None supplied.' for field in FIELDS}
    desired['Checkpoint conditions'] = '```json\n' + json.dumps(value['conditions'], ensure_ascii=True, sort_keys=True, indent=2) + '\n```'
    desired['Checkpoint evidence'] = list_text(value['evidence'])
    desired['Checkpoint provenance'] = '```json\n' + json.dumps({'developer_id': developer_id, 'harness': value['harness'],
        'session': value['session'], 'identity_authenticated': False}, ensure_ascii=True, sort_keys=True, indent=2) + '\n```'
    seen, edits = set(), []
    for start, end, title in headings(base):
        if title in desired:
            if title in seen:
                raise ContractError('Duplicate checkpoint section requires clarification; do not choose one silently')
            seen.add(title)
            edits.append((start, end, '## ' + title + '\n\n' + desired[title] + '\n\n'))
    body = base
    for start, end, content in reversed(edits):
        body = body[:start] + content + body[end:]
    for title, content in desired.items():
        if title not in seen:
            separator = '' if body.endswith('\n\n') else '\n' if body.endswith('\n') else '\n\n'
            body += separator + '## ' + title + '\n\n' + content + '\n\n'
    if len(body.encode()) > LIMIT:
        raise ContractError('Checkpoint exceeds 16 KiB; consolidate safely without dropping distinct evidence or conditions')
    final = [title for _, _, title in headings(body)]
    if any(final.count(title) != 1 for title in desired):
        raise ContractError('Rendered checkpoint has ambiguous section boundaries; preserve the original context')
    return body


def inspect(repo):
    root, bank_path = writes.bank_path(repo)
    checked, observed = writes.snapshot(root)
    body = (bank_path / 'activeContext.md').read_bytes().decode() if 'activeContext.md' in observed else None
    if body is not None and len(body.encode()) > 65536:
        raise ContractError('Active context exceeds the bounded reader; inspect and consolidate explicitly before checkpointing')
    if writes.snapshot(root)[1] != observed:
        raise ContractError('Bank changed during checkpoint inspection; reread')
    route = next((row for row in checked['routes'] if row['path'] == 'activeContext.md'), None)
    return result('inspect', repo_root=str(root), project_id=checked['project_id'], active_context=body, route=route,
                  observed_active_sha256=observed.get('activeContext.md'), source_hashes=observed,
                  preparation_required=body is None, content_is_untrusted_data=True, writes_performed=False)


def preview(repo, value, *, prepare=False):
    value = input_value(value)
    context = identity.require(repo)
    current = inspect(repo)
    if current['observed_active_sha256'] != value['observed_active_sha256']:
        raise ContractError('Active context changed or was not read; inspect it and preserve compatible evidence before a fresh proposal')
    if prepare:
        if not current['preparation_required']:
            raise ContractError('Active context already exists; use the single-file checkpoint')
        root, bank_path = writes.bank_path(repo)
        files = transactions.files_at(bank_path)
        files['activeContext.md'] = render('# Active context\n', value, context['developer_id'])
        security.ensure_safe({'activeContext.md': files['activeContext.md']})
        metadata, routes, omitted = bank.parse_router(files['MEMORY.md'])
        routes.append({'path': 'activeContext.md', 'purpose': 'Current focus, changes, questions and next step',
                       'scope': 'repository', 'lifecycle': 'active', 'read-when': 'When starting or resuming work'})
        omitted.pop('activeContext.md', None)
        metadata['router-updated'] = storage.now()
        files['MEMORY.md'] = bank.router_text(metadata, routes, omitted)
        proposal = transactions.preview(root, files, 'replace')
        if (proposal['proposal']['observed_hashes'] != current['source_hashes']
                or proposal['proposal']['scope_context'] != context):
            raise ContractError('Bank or identity changed during context preparation; reread')
        return result('prepare', separate_router_operation=True, transaction=proposal['proposal'], writes_performed=False, verified=False)
    if current['preparation_required']:
        raise ContractError('Active context is omitted; prepare its useful content and route as a separate observed transaction first')
    body = render(current['active_context'], value, context['developer_id'])
    security.ensure_safe({'activeContext.md': body})
    prepared = writes.preview(repo, 'activeContext.md', body, 'checkpoint')['proposal']
    if prepared['observed_hashes'] != current['source_hashes'] or prepared['scope_context'] != context:
        raise ContractError('Bank or identity changed during checkpoint preparation; inspect again')
    return result('preview', proposal={'schema': 'agent-memory/checkpoint-proposal-v1', 'input': value,
                                      'base_text': current['active_context'], 'write': prepared},
                  changed_memory_paths=['memory-bank/activeContext.md'] if body != current['active_context'] else [],
                  writes_performed=False, verified=False)


def apply(prepared):
    p = prepared.get('proposal', prepared) if isinstance(prepared, dict) else prepared
    if not isinstance(p, dict) or set(p) != {'schema', 'input', 'base_text', 'write'} or p['schema'] != 'agent-memory/checkpoint-proposal-v1':
        raise ContractError('Invalid checkpoint proposal; router preparation is a separate transaction')
    value = input_value(p['input'])
    write = writes.proposal_value(p['write'])
    if (write['path'] != 'activeContext.md' or write['operation'] != 'checkpoint'
            or write['observed_hashes'].get('activeContext.md') != value['observed_active_sha256']):
        raise ContractError('Checkpoint may replace only the observed activeContext.md')
    if (not isinstance(p['base_text'], str) or digest(p['base_text'].encode()) != value['observed_active_sha256']
            or not isinstance(write['scope_context'], dict)
            or render(p['base_text'], value, write['scope_context']['developer_id']) != write['content']):
        raise ContractError('Checkpoint content must match the reviewed fields and preserve unrelated existing sections')
    receipt = writes.apply(write)
    return result('apply', write=receipt, verified=receipt['verified'] and receipt['still_current'],
                  writes_performed=receipt['writes_performed'], changed_memory_paths=receipt['changed_memory_paths'],
                  sharing='local-or-pending-sharing')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for name in ('inspect', 'preview', 'prepare'):
        sub = commands.add_parser(name);sub.add_argument('--repo', default='.')
        if name != 'inspect':sub.add_argument('--input', required=True)
    writer = commands.add_parser('apply');writer.add_argument('--proposal', required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == 'inspect':value = inspect(args.repo)
        elif args.command == 'apply':value = apply(load(Path(args.proposal).read_bytes()))
        else:value = preview(args.repo, load(Path(args.input).read_bytes()), prepare=args.command == 'prepare')
        print(json.dumps(value, indent=2, ensure_ascii=False));return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps(result(args.command, error=str(exc), verified=False, inspection_required=True)));return 1


if __name__ == '__main__':sys.exit(main())
