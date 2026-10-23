"""Inspect the configured memory-only Git boundary without publishing anything."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from . import bank_writes as writes, repository_config, scope_binding, security
from .developer_identity import digest, encoded
from .frontmatter import ContractError
from .identity_git import git


def prior_deletions(root, prefix, indexed, current):
    """Prove missing indexed paths belonged to the last complete canonical bank.

    The original index is evidence only. Never stage a deletion in the caller's
    index merely to make sharing possible, and never bless an arbitrary tracked
    cache/backup as memory.
    """
    missing = {p for p in indexed if p not in current}
    if not missing:
        return set()
    # Local import avoids the observation -> boundary -> observation cycle.
    from . import sharing_recovery as recovery
    head = git(root, 'rev-parse', '--verify', 'HEAD')
    try:
        files = recovery.tree(root, head, prefix)
    except ContractError:
        return set()
    proven = set()
    for path in missing:
        relative = path[len(prefix) + 1:]
        mode, blob, stage = indexed[path]
        if (relative not in files or mode != '100644' or stage != '0'
                or (root / path).exists() or (root / path).is_symlink()):
            continue
        expected = recovery.command(root, 'rev-parse', head + ':' + path).strip()
        if blob == expected:
            proven.add(path)
    return proven


def inspect(repo, *, owned_lock=None):
    root, bank = writes.bank_path(repo)
    context = scope_binding.require(root, owned_lock=owned_lock)
    configuration = repository_config.settings(root)
    checked, observed = writes.snapshot(root, owned_lock=owned_lock)
    prefix = configuration['bank_root'] + '/'
    canonical = [prefix + path for path in sorted(observed)]
    if len(encoded(canonical)) > 65536:
        raise ContractError('Sharing path inventory exceeds its bounded command manifest')
    project = scope_binding.project.status(root, owned_lock=owned_lock)
    remote = project['evidence']['authority_name']
    if remote:
        repository_config.safe_label(remote)
    configured = configuration['authority_branch']
    remote_head = git(root, 'symbolic-ref', '--quiet', 'refs/remotes/' + remote + '/HEAD', optional=True) if remote else None
    branch_prefix = 'refs/remotes/' + remote + '/' if remote else None
    if configured:
        branch, source = configured, 'explicit-local-configuration'
    elif remote_head and remote_head.startswith(branch_prefix):
        branch, source = remote_head[len(branch_prefix):], 'local-remote-default-branch-observation'
    else:
        branch, source = 'main', 'initial-main-default-unverified'
    repository_config.validate_branch(branch)
    # Read actual Git evidence; canonical remote identifiers are never fetch URLs.
    indexed = git(root, 'ls-files', '--stage', '-z', '--', configuration['bank_root'])
    entries = {}
    for item in indexed.split('\0'):
        if not item:
            continue
        header, path = item.split('\t', 1)
        mode, blob, stage = header.split()
        # Several stages for one path must never masquerade as ordinary.
        entries[path] = (mode, blob, stage if path not in entries else 'unmerged')
    deleted = prior_deletions(root, configuration['bank_root'], entries, canonical)
    tracked, unmerged, excluded_tracked = set(), 0, 0
    for path, (mode, _, stage) in entries.items():
        if path not in canonical and path not in deleted:
            excluded_tracked += 1
        elif stage != '0' or mode != '100644':
            unmerged += 1
        else:
            tracked.add(path)
    ignored = set()
    for selection in ('--cached', '--others'):
        output = git(root, 'ls-files', selection, '--ignored', '--exclude-standard', '-z', '--', *canonical)
        ignored.update(path for path in output.split('\0') if path)
    blockers = []
    if ignored:
        blockers.append('canonical-memory-is-ignored')
    if excluded_tracked:
        blockers.append('excluded-bank-artifacts-are-tracked')
    if unmerged:
        blockers.append('unmerged-or-nonordinary-memory-index-entry')
    pending = sorted(set(canonical) - tracked)
    if pending:
        blockers.append('canonical-memory-is-not-tracked')
    if not remote:
        blockers.append('no-resolved-sharing-remote')
    if source == 'initial-main-default-unverified':
        blockers.append('default-branch-needs-explicit-configuration-or-fresh-remote-observation')
    # Validate all canonical content before describing it as an eligible delta.
    # This is still not a semantic admission or publication receipt.
    safety = []
    for entry in checked['entries']:
        if entry['lifecycle'] in {'candidate', 'quarantined'}:
            safety.append('unadmitted-content')
    for path in observed:
        target = bank / path
        with target.open('rb') as stream:
            raw = stream.read(16385)
        if len(raw) > 16384:
            safety.append('bounded-content-review-required')
            continue
        scan = security.scan(raw.decode('utf-8') + '\n' + prefix + path)
        safety.extend(scan['reasons'])
    if safety:
        blockers.append('unsafe-or-unreviewable-memory-content')
    scope_binding.require(root, checked['project_id'], expected=context, owned_lock=owned_lock)
    if (writes.snapshot(root, owned_lock=owned_lock)[1] != observed or repository_config.settings(root) != configuration
            or git(root, 'ls-files', '--stage', '-z', '--', configuration['bank_root']) != indexed):
        raise ContractError('Canonical bank or boundary configuration changed; inspect again')
    return {'schema': 'agent-memory/sharing-boundary-v1', 'project_id': checked['project_id'],
        'bank_root': configuration['bank_root'], 'authority_remote': remote,
        'authority_branch': branch, 'authority_branch_source': source,
        'remote_freshness_verified': False, 'publication_verified': False, 'admission_verified': False,
        'memory_path_count': len(canonical),
        # Unsafe names and bodies must not be disclosed by a failed boundary check.
        'eligible_memory_paths': canonical if not safety else [],
        'untracked_memory_count': len(pending), 'ignored_memory_count': len(ignored),
        'proven_deleted_memory_paths': sorted(deleted) if not safety else [],
        'excluded_tracked_artifact_count': excluded_tracked, 'unmerged_or_nonordinary_count': unmerged,
        'safety_reasons': sorted(set(safety)), 'blockers': blockers,
        'local_boundary_ready': not blockers,
        'bank_observation_sha256': digest(encoded({'project_id': checked['project_id'], 'sources': observed})),
        'roles': {'memory': 'durable-project-learning', 'awa': 'work-state-and-delivery', 'vox': 'live-coordination'},
        'requires_awa': False, 'requires_vox': False, 'personal_memory': False,
        'writes_performed': False, 'validation_scope': 'local-memory-git-boundary-only'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True)
    args = parser.parse_args(argv)
    try:
        print(json.dumps(inspect(args.repo), indent=2))
        return 0
    except (ContractError, OSError, ValueError, UnicodeError, TypeError, KeyError, AttributeError, RecursionError):
        print(json.dumps({'schema': 'agent-memory/sharing-boundary-v1', 'verified': False,
            'writes_performed': False, 'error': 'Unresolved, unsafe, interrupted or changed repository boundary; inspect and preserve local work'}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
