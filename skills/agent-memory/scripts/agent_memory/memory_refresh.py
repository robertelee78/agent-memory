"""Reconcile landed repository memory without importing feature code or changing its index."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import stat
import sys
import uuid

from . import bank_transactions as tx, bank_writes as writes, local_registry as storage
from . import retrieval, scope_binding, semantic_sharing as semantic, sharing_recovery as recovery
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError
from .identity_git import git
from . import sharing_limits as limits

LIMIT = 4 * 1048576
CURSOR = 'agent-memory/refresh-cursor-v1'
CURSOR_FIELDS = {'schema', 'repo_root', 'project_id', 'authority_head', 'operation_id',
                 'receipt_sha256', 'resulting_hashes', 'git_context', 'checksum'}


class UntrackedCanonicalMemory(ContractError):
    """The quick equality check cannot cover locally untracked memory."""


def result(operation, state, **values):
    return {'schema': 'agent-memory/refresh-result-v1', 'operation': operation, 'sharing_state': state,
            'refresh_applied': False, 'canonical_writes_performed': False, 'original_index_writes_performed': False,
            'publication_verified': False, 'feature_code_freshness_verified': False,
            'remote_freshness_verified': False, 'relevant_imports_read': False,
            'local_memory_available': True, **values}


def index_hash(root):
    path = Path(git(root, 'rev-parse', '--path-format=absolute', '--git-path', 'index'))
    if path.is_symlink():
        raise ContractError('Refresh refuses a redirected original index')
    return digest(path.read_bytes() if path.exists() else None)


def cursor_path(root):
    root, bank = writes.bank_path(root)
    directory, _ = writes.state_paths(root, bank, str(uuid.uuid4()))
    target = directory.parent/'refresh-cursor.json'
    if target.is_symlink():
        raise ContractError('Disposable refresh cursor is a symlink; preserve it for inspection')
    return target


def cursor(root):
    target = cursor_path(root)
    if not target.exists():
        return None, None, 'absent'
    with target.open('rb') as stream:
        raw = stream.read(limits.MAX_PRIVATE+1)
    sha = digest(raw)
    try:
        if len(raw) > limits.MAX_PRIVATE:
            raise ContractError('Oversized cursor')
        value = load(raw)
        if (not isinstance(value, dict) or set(value) != CURSOR_FIELDS or value['schema'] != CURSOR
                or value['repo_root'] != str(root) or not uuid_valid(value['operation_id'])
                or value['project_id'] != scope_binding.require(root)['project_id']
                or value['checksum'] != digest(encoded({k: v for k, v in value.items() if k != 'checksum'}))):
            raise ContractError('Unverified cursor')
        recovery.oid(value['authority_head'])
        _, receipt = tx.locations(*writes.bank_path(root), value['operation_id'])
        if receipt.is_symlink() or not receipt.is_file():
            raise ContractError('Cursor does not bind its actual completed receipt')
        with receipt.open('rb') as stream:
            receipt_raw = stream.read(limits.MAX_PRIVATE+1)
        if len(receipt_raw) > limits.MAX_PRIVATE or digest(receipt_raw) != value['receipt_sha256']:
            raise ContractError('Cursor receipt is changed or exceeds the bounded reader')
        r = load(receipt_raw)
        if (set(r) != tx.RECEIPT_FIELDS or r['schema'] != 'agent-memory/transaction-receipt-v1'
                or r['verified'] is not True or r['outcome'] != 'finished'
                or r['operation_id'] != value['operation_id'] or r['project_id'] != value['project_id']
                or r['bank_path'] != str(writes.bank_path(root)[1]) or r['resulting_hashes'] != value['resulting_hashes']):
            raise ContractError('Cursor receipt does not prove this bank operation')
        recorded = value['git_context']
        if (not isinstance(recorded, dict) or set(recorded) not in ({'revision'}, {'branch', 'revision'})
                or not all(isinstance(v, str) and v for v in recorded.values())):
            raise ContractError('Cursor lacks its original branch/revision context')
        recovery.oid(recorded['revision'])
        if recorded != retrieval.git_context(root):
            # A caller may have stashed imported memory before switching
            # branches or committing code. Its absence in the new checkout
            # must not be interpreted using another checkout's import base.
            return None, sha, 'checkout-context-changed'
        return value, sha, 'verified-local-hint'
    except (ContractError, ValueError, TypeError, KeyError, UnicodeError):
        # Disposable state is not authority. Preserve damaged bytes and recover
        # a baseline from actual Git history, with new semantic review.
        return None, sha, 'unverified-preserved'


def inspect(repo, base=None, *, version2=False):
    root, _ = writes.bank_path(repo)
    observed_context = retrieval.git_context(root)
    previous, _, state = cursor(root)
    chosen = base if base is not None else previous['authority_head'] if previous else None
    if version2:
        from . import sharing_frontier
        request=sharing_frontier.request_value(None)
        request['base']=chosen
        value=sharing_frontier.inspect(root,request)
    else:
        value = semantic.inspect(root, chosen)
    if (retrieval.git_context(root) != observed_context
            or value['binding']['local_head'] != observed_context.get('revision')):
        raise ContractError('Checkout changed during cursor/base inspection; inspect again')
    return result('inspect', 'review-required', inspection=value, cursor_state=state,
                  baseline_source='explicit' if base is not None else 'cursor' if previous else 'git-common-base',
                  git_metadata_writes_performed=True, remote_freshness_verified=True)


def current(repo):
    """Prove only a point-in-time equal bank, never semantic truth or no pending work."""
    from . import records, sharing_boundary, sharing_holds
    root, bank = writes.bank_path(repo)
    before_index = index_hash(root)
    before_git = retrieval.git_context(root)
    prior, cursor_sha, cursor_state = cursor(root)
    if cursor_state not in {'absent', 'verified-local-hint'}:
        raise ContractError('Preserve the unverified refresh cursor and use full review')
    boundary = sharing_boundary.inspect(root)
    if not boundary['local_boundary_ready']:
        if set(boundary['blockers']) == {'canonical-memory-is-not-tracked'}:
            raise UntrackedCanonicalMemory('Locally untracked canonical memory requires full review.')
        raise ContractError('Local sharing boundary is not ready; use full review')
    context = scope_binding.require(root)

    def transaction_inventory(operations):
        # Resolve from / using directory descriptors. Neither intermediate nor
        # final components may be followed if replaced by a symlink.
        state_root = operations.parents[2]
        pieces = [*state_root.parts[1:], 'writes', operations.parent.name, 'transactions']
        descriptors = []
        identities = []
        flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
        try:
            descriptors.append(os.open('/', flags))
            for index, piece in enumerate(pieces):
                try:
                    fd = os.open(piece, flags, dir_fd=descriptors[-1])
                except FileNotFoundError:
                    return identities, ('absent', index)
                descriptors.append(fd)
                metadata = os.fstat(fd)
                if index >= len(state_root.parts) - 2 and (
                        metadata.st_uid != os.getuid() or stat.S_IMODE(metadata.st_mode) & 0o022):
                    raise ContractError('Untrusted transaction recovery location')
                if index >= len(state_root.parts) - 2:
                    identities.append((metadata.st_dev, metadata.st_ino, metadata.st_mode,
                                       metadata.st_uid, metadata.st_mtime_ns, metadata.st_ctime_ns))
            with os.scandir(descriptors[-1]) as entries:
                if next(entries, None) is not None:
                    raise ContractError('Transaction recovery evidence requires review')
            return identities, ('empty',)
        finally:
            for fd in reversed(descriptors):
                os.close(fd)

    def runtime_state():
        holds, hold_sha = sharing_holds.read_store(root, context['project_id'])
        if holds['records']:
            raise ContractError('Known holds require explicit review')
        operations, _ = writes.state_paths(root, bank, str(uuid.uuid4()))
        return holds, hold_sha, cursor(root), transaction_inventory(operations)

    before_state = runtime_state()
    root, observation, binding, values, checked = semantic.snapshots(
        root, prior['authority_head'] if prior else None, quarantine_records=False)
    if not observation['shared_bank_present'] or not values['base'] or not (
            values['base'] == values['local'] == values['landed']):
        return result('current', 'review-required', pending_capture='unknown',
                      pending_publication='unknown', semantic_truth_verified=False,
                      git_metadata_writes_performed=True)
    head = recovery.tree(root, observation['local_head'], observation['bank_root'])
    if head != values['local']:
        return result('current', 'review-required', pending_capture='unknown',
                      pending_publication='unknown', semantic_truth_verified=False,
                      git_metadata_writes_performed=True)
    for entry in checked['local']['entries']:
        if entry['format'] == 'standalone-record':
            _, meta, _ = records.parse(values['local'][entry['path']])
            if meta.get('visibility') != 'visible' or meta.get('status') in {'candidate', 'quarantined'}:
                raise ContractError('Hidden or unadmitted memory requires review')

    def mode_and_index():
        for path in values['local']:
            target = bank / path
            mode = target.lstat().st_mode
            if not stat.S_ISREG(mode) or stat.S_IMODE(mode) != 0o644:
                raise ContractError('Nonordinary local bank file mode requires review')
        prefix = observation['bank_root'] + '/'
        tree_raw = recovery.command(root, 'ls-tree', '-rz', '--full-tree', observation['local_head'],
                                    '--', observation['bank_root'], limit=recovery.MAX_LOCATOR_TREE_BYTES)
        index_raw = recovery.command(root, 'ls-files', '--stage', '-z', '--', observation['bank_root'],
                                     limit=recovery.MAX_LOCATOR_TREE_BYTES)
        tree_entries, index_entries = {}, {}
        for item in filter(None, tree_raw.split('\0')):
            header, path = item.split('\t', 1); mode, kind, oid = header.split()
            if not path.startswith(prefix) or mode != '100644' or kind != 'blob' or path in tree_entries:
                raise ContractError('Nonordinary HEAD bank entry requires review')
            tree_entries[path] = oid
        for item in filter(None, index_raw.split('\0')):
            header, path = item.split('\t', 1); mode, oid, stage = header.split()
            if not path.startswith(prefix) or mode != '100644' or stage != '0' or path in index_entries:
                raise ContractError('Nonordinary index bank entry requires review')
            index_entries[path] = oid
        if tree_entries != index_entries or set(tree_entries) != {prefix + path for path in values['local']}:
            raise ContractError('HEAD and index bank inventories differ')
        return tree_raw, index_raw

    before_modes = mode_and_index()
    writes.fault('refresh-current-before-final-recheck')
    semantic.final_sources(root, observation, binding)
    if (scope_binding.require(root, context['project_id'], expected=context) != context
            or retrieval.git_context(root) != before_git or index_hash(root) != before_index
            or sharing_boundary.inspect(root) != boundary or runtime_state() != before_state
            or mode_and_index() != before_modes or cursor(root)[1:] != (cursor_sha, cursor_state)):
        raise ContractError('Memory, index, identity or recovery state changed during observation')
    if recovery.server(root, observation['authority_remote'], observation['authority_branch']) != observation['authority_head']:
        raise ContractError('Authority advanced during observation')
    return result('current', 'bank-synchronized', pending_capture='unknown', pending_publication='unknown',
                  semantic_truth_verified=False, source_assertions_verified=False,
                  remote_freshness_verified=True, observed_authority_head=observation['authority_head'],
                  git_metadata_writes_performed=True)


def validate_transaction(p, *, owned_lock=None):
    evidence = p.get('refresh_evidence')
    if (p.get('operation') != 'refresh' or not isinstance(evidence, dict)
            or set(evidence) != {'proof', 'recall', 'git_context'}):
        raise ContractError('Source-bound refresh requires the exact semantic proof, recall and checkout context')
    retrieval.request_value(evidence['recall'])
    retrieval.observed_context(p['repo_root'], evidence['recall']['context'])
    binding = evidence['proof']['judgment']['binding']
    if (evidence['recall']['project_id'] != p['project_id'] or binding['project_id'] != p['project_id']
            or binding['repo_root'] != p['repo_root'] or binding['scope_context'] != p['scope_context']
            or binding['source_hashes']['local'] != p['observed_hashes']
            or retrieval.git_context(p['repo_root']) != evidence['git_context']):
        raise ContractError('Refresh cannot substitute a project, branch, source set or original observed bank')
    reviewed = semantic.check(p['repo_root'], evidence['proof'], owned_lock=owned_lock)
    if reviewed['refresh_files'] != p['files'] or tx.file_hashes(p['files']) != evidence['proof']['refresh_hashes']:
        raise ContractError('Refresh transaction is not the complete exact reviewed three-way result')
    return reviewed


def prepare(repo, request):
    if not isinstance(request, dict) or set(request) != {'proof', 'recall'}:
        raise ContractError('Prepare refresh with complete proof and an explicit relevant recall request')
    retrieval.request_value(request['recall'])
    root, _ = writes.bank_path(repo); before = index_hash(root)
    reviewed = semantic.check(root, request['proof'])
    p = tx.preview(root, reviewed['refresh_files'], operation='refresh')['proposal']
    p['refresh_evidence'] = {**request, 'git_context': retrieval.git_context(root)}
    validate_transaction(p)
    if index_hash(root) != before:
        raise ContractError('Original index changed during preparation; preserve it and inspect')
    return result('prepare', 'prepared', proposal=p, conflicts=reviewed['conflicts'],
                  blocked_paths=reviewed['blocked_paths'], changed_memory_paths=tx.changed(p),
                  git_metadata_writes_performed=True, remote_freshness_verified=True)


def save_cursor(root, p, observation, receipt, expected):
    value = {'schema': CURSOR, 'repo_root': str(root), 'project_id': p['project_id'],
             'authority_head': (p['refresh_evidence']['proof']['judgment']['binding']['base']
                 if p['refresh_evidence']['proof']['blocked_paths'] or p['refresh_evidence']['proof']['blocked_components']
                 else observation['authority_head']), 'operation_id': p['operation_id'],
             'receipt_sha256': digest(Path(receipt['receipt_path']).read_bytes()),
             'resulting_hashes': receipt['resulting_hashes'],
             'git_context': p['refresh_evidence']['git_context']}
    value['checksum'] = digest(encoded(value))
    path = cursor_path(root); path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    storage.replace(path, limits.private_bytes(value), expected)
    return {'stored': True, 'authority_head': observation['authority_head'], 'content_free': True}


def apply(prepared):
    p = tx.proposal_value(prepared)
    if 'refresh_evidence' not in p:
        raise ContractError('The refresh operation requires its original source-bound proposal')
    root, _ = writes.bank_path(p['repo_root']); original_index = index_hash(root)
    old_cursor, cursor_sha, cursor_state = cursor(root)
    had_receipt = tx.locations(*writes.bank_path(root), p['operation_id'])[1].exists()
    receipt = tx.apply(p)
    if index_hash(root) != original_index:
        raise ContractError('Original feature index changed externally; preserve and inspect it')
    proof = p['refresh_evidence']['proof']
    held = set(proof['blocked_paths'])
    conflicts_pending = bool(held or proof['blocked_components'])
    common = {'receipt': receipt, 'canonical_writes_performed': receipt['writes_performed'],
              'refresh_applied': receipt['outcome'] == 'finished', 'cursor_state': cursor_state,
              'conflicts_pending': conflicts_pending, 'blocked_paths': sorted(held),
              'blocked_components': proof['blocked_components'],
              'git_metadata_writes_performed': not had_receipt, 'authority_snapshot': p['refresh_evidence']['proof']['judgment']['binding']['authority_head']}
    if (not receipt['still_current'] or receipt['outcome'] != 'finished'
            or retrieval.git_context(root) != p['refresh_evidence']['git_context']):
        return result('apply', 'historical-or-changed-local', **common,
                      reason='Recorded import remains historical; current local bank/branch changed and is preserved')
    # Observe again after the local transaction. A moving/offline remote cannot
    # turn a mechanically completed import into a claim of latest shared truth.
    try:
        observation = recovery.observe(root,quarantine_records=proof['schema']=='agent-memory/semantic-sharing-proof-v2')
    except ContractError:
        return result('apply', 'pending-stale', **{**common, 'git_metadata_writes_performed': None},
                      reason='Import is consistent but latest authority could not be verified; inspect again before shared recall')
    common['git_metadata_writes_performed'] = True
    expected = p['refresh_evidence']['proof']['judgment']['binding']['source_hashes']['landed']
    if observation['landed_hashes'] != expected:
        return result('apply', 'pending-new-authority', **common,
                      reason='New shared memory landed during import; inspect and review it before current imported recall')
    recalled = retrieval.retrieve(root, p['refresh_evidence']['recall'], deferred_paths=held)
    if writes.snapshot(root)[1] != receipt['resulting_hashes'] or retrieval.git_context(root) != p['refresh_evidence']['git_context']:
        raise ContractError('Bank/branch changed during imported recall; preserve and inspect it')
    stored = {'stored': False, 'reason': 'Prior cursor cannot safely advance and is preserved; Git can reconstruct a baseline'}
    if cursor_state != 'unverified-preserved':
        if old_cursor is None or recovery.ancestor(root, old_cursor['authority_head'], observation['authority_head']):
            stored = save_cursor(root, p, observation, receipt, cursor_sha)
    imported = set(tx.changed(p))
    return result('apply', 'refreshed-with-conflicts' if conflicts_pending else 'refreshed', **common, remote_freshness_verified=True,
                  relevant_imports_read=True, recall=recalled,
                  recalled_import_paths=[row['path'] for row in recalled['results'] if row['path'] in imported],
                  observed_authority_head=observation['authority_head'], cursor=stored,
                  source_assertions_verified=False, semantic_truth_verified=False,
                  feature_history_changed=False, unrelated_working_files_changed=False)


def read_input(path, *, version2=False):
    raw=limits.read_bytes(path,limits.MAX_PRIVATE if version2 else LIMIT)
    value=load(raw)
    body=value.get('proposal',value) if isinstance(value,dict) else value
    proof=(body.get('proof') or body.get('refresh_evidence',{}).get('proof')) if isinstance(body,dict) else None
    if isinstance(proof,dict) and proof.get('schema')=='agent-memory/semantic-sharing-proof-v2':return value
    if version2:raise ContractError('Explicit v2 refresh reader requires the actual v2 source-bound proof')
    return value


from . import validation_memo


@validation_memo.operation()
def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='operation', required=True)
    check = sub.add_parser('inspect'); check.add_argument('--repo', required=True); check.add_argument('--base')
    current_parser = sub.add_parser('current'); current_parser.add_argument('--repo', required=True)
    preview = sub.add_parser('prepare'); preview.add_argument('--repo', required=True); preview.add_argument('--input', required=True)
    writer = sub.add_parser('apply'); writer.add_argument('--input', required=True)
    for command in (check,preview,writer):
        command.add_argument('--output');command.add_argument('--v2',action='store_true')
    args = parser.parse_args(argv)
    try:
        input_value = None
        if getattr(args, 'output', None):
            supplied = Path(args.output).expanduser()
            if not supplied.is_absolute() or '..' in supplied.parts:
                print(json.dumps(result(args.operation, 'pending-private-output',
                    canonical_writes_performed=False, git_metadata_writes_performed=False,
                    failure_component='output', failure_code='private-output-path-invalid',
                    reason='The private output path must be absolute and must not contain parent-directory components.',
                    repair='Choose a new absolute path in an owner-only directory outside the repository and installed runtime, then retry with --output. Preserve the bank and pending evidence.')))
                return 1
            if hasattr(args, 'repo'):
                output_repo = args.repo
            else:
                input_value = read_input(args.input,version2=args.v2)
                output_repo = tx.proposal_value(input_value)['repo_root']
            limits.preflight_private_output(args.output, output_repo)
        if args.operation == 'current': value = current(args.repo)
        elif args.operation == 'inspect': value = inspect(args.repo, args.base,version2=args.v2)
        elif args.operation == 'prepare':
            request=read_input(args.input,version2=args.v2)
            proof=request.get('proof') if isinstance(request,dict) else None
            if isinstance(proof,dict) and proof.get('schema')=='agent-memory/semantic-sharing-proof-v2' and not args.output:
                print(json.dumps(result('prepare','pending-private-output',failure_component='prepare',
                    failure_code='private-output-required',
                    reason='A v2 refresh proposal may retain quarantined local originals and requires private output.',
                    repair='Rerun am refresh prepare with the original --repo and --input plus --v2 --output /private/path/refresh.json; select an owner-only location outside the repository.'),indent=2))
                return 1
            value = prepare(args.repo, request)
        else: value = apply(input_value if input_value is not None else read_input(args.input,version2=args.v2))
        if getattr(args, 'output', None):
            value=limits.write_private(args.output,value,output_repo)
        print(json.dumps(value, indent=2)); return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError, RecursionError) as exc:
        from .sharing_holds import HoldFailure
        authority_failed=isinstance(exc,recovery.AuthorityFailure)
        output_failed=isinstance(exc,limits.PrivateOutputError)
        untracked_memory=isinstance(exc,UntrackedCanonicalMemory)
        known=isinstance(exc,HoldFailure) or authority_failed
        print(json.dumps(result(args.operation, 'pending-private-output' if output_failed else 'pending-inspection', canonical_writes_performed=False if output_failed or args.operation == 'current' else None,
                                git_metadata_writes_performed=False if output_failed or untracked_memory else None,
                                **({'pending_capture': 'unknown', 'pending_publication': 'unknown', 'semantic_truth_verified': False}
                                   if args.operation == 'current' else {}),
                                failure_component='output' if output_failed else 'local-boundary' if untracked_memory else exc.component if authority_failed else 'known-dispute-state' if known else args.operation,
                                failure_code='private-output-invalid' if output_failed else 'canonical-memory-untracked' if untracked_memory else exc.code if known else 'refresh-unverified',
                                reason=str(exc) if output_failed or untracked_memory or known else 'Unresolved, offline, unsafe, interrupted or changed refresh; retain the original proposal and local memory, inspect/recover before retry',
                                repair=str(exc) if output_failed else 'Preserve local memory and feature work. Run am refresh inspect --v2 --repo REPO --output /private/path/inspection.json using an owner-only directory outside the repository, then complete the required review. Do not stage or reset memory just to pass this quick check.' if untracked_memory else exc.repair if known else 'Preserve the proposal, bank and pending evidence. Run am doctor for this repository and inspect the failed component before repeating refresh inspection; do not reset or replace the bank.')))
        return 1


if __name__ == '__main__':
    sys.exit(main())
