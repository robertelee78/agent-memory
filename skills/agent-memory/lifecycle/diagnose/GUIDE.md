---
kind: operation
domain: maintain
---

# Diagnose runtime and bank health

## Use when

A session MUST use this operation to inspect installed/runtime identity, report
current setup, or diagnose a failed lifecycle or memory operation. Diagnostics
work before developer selection, installation or bank initialization.

## Required context

The session MUST load the [maintain domain](../../maintain/GUIDE.md) first.
It SHOULD use a trusted complete package outside affected installation targets
when the installed wrapper or source cannot run. It MUST resolve `SKILL_DIR`
to that package rather than a deleted or incomplete managed checkout.

## Inputs and preconditions

Python 3.11 or newer is required. Git enables local revision and repository
evidence; no provider credentials, agent-client executable, network connection
or identity bootstrap is needed. `--repo` selects the repository/worktree whose
one bank is inspected. A session MUST retain original journals and user edits.

## Observe

The session SHOULD start with the installed `am version --json` or
the trusted source helper:

```sh
python3 -I "$SKILL_DIR/scripts/diagnose.py" version --json
```

The session MUST distinguish `executing_source` from the installation's
`recorded_source`. The former measures the package actually running; the latter
is receipt history until doctor verifies ownership. An inactive receipt is
`uninstalled`, even when changed targets were preserved during removal.

## Plan

For a quick current view, the session SHOULD run:

```sh
python3 -I "$SKILL_DIR/scripts/diagnose.py" status --repo /absolute/repository --json
```

Status reads selected/default and indexed router candidates, local identity
selection, enabled discovery adapters and runtime/bank transaction markers.
It does not scan routed content or the entire filesystem. The session MUST NOT
treat `not_located` as proven bank absence, `root_routes_by_lifecycle` as the
complete record inventory, or a cached receipt as verified installation health.
Counts and freshness that were not inspected remain explicitly unassessed.

The additive `setup` assessment supplies an actionable `next_step`. `setup_needed`
is a shallow first-use candidate, not proof of exhaustive absence; use doctor
before initialization. `binding_required` and `binding_mismatch` distinguish an
unadopted bank from conflicting identities. A recognized router in the Git index
or HEAD that is absent on disk yields `missing_known_bank`: a session MUST inspect
Git state and preserve remaining files instead of initializing a replacement.
A configured directory or reserved project UUID alone is not evidence that a
bank previously existed. Staged deletion does not establish an accidental loss
or permission to restore/delete data.

Doctor alone may establish `not_initialized` after complete discovery, or `ready`
after bank validation and current project-authority checks. These assessments
describe bank setup, not semantic truth or overall runtime health. A healthy
bank can coexist with failed runtime checks, which remain failures. Busy writes,
pending separately authorized purges and unreadable/malformed evidence take
precedence over generic initialization guidance. Diagnostics MUST NOT bind,
initialize, restore a bank, clean markers or resume a purge automatically.

## Apply

The session SHOULD run the deeper read-only audit when setup or health is unclear:

```sh
python3 -I "$SKILL_DIR/scripts/diagnose.py" doctor --repo /absolute/repository --json
```

The installed machine equivalents are `am status --json` and `am doctor --json`.
Without `--json`, these commands print a human-readable view of the same result.
A session MAY add `--projection /absolute/reference-projection.json` to doctor
when an optional derived reference file is actually in use. Doctor MUST NOT
initialize missing state, rebuild caches, acquire a new writer lock, migrate,
recover, uninstall or purge data on the operator's behalf.

## Read back

The session MUST read `failures` and `warnings`, not just the process exit code.
Exit 1 means a component failed verification; exit 0 permits qualifications such
as missing setup, an active writer, pending recovery or the initial unsigned
distribution channel. Each failure names its component/path and a bounded repair.
The initial channel's digest establishes artifact identity, not publisher trust.

A session MUST distinguish `runtime.path_trust` from ordinary access/readability.
The Unix ownership/mode checks neither authenticate already-loaded verifier code
nor audit every platform ACL. An installation that may have been attacker-writable
MUST be inspected or recovered using a trusted complete outside helper; its own
reported checks MUST NOT be treated as self-authentication.

## Verify

Doctor checks actual source revision/channel/digest, the portable route graph,
receipt/journal integrity, existing lifecycle lock state, owned wrappers and
client links/copies, activation block hashes, directory/backup permissions and
the established one-bank locator. Outside activation text may differ safely.
It validates canonical Markdown, topic indices, records, history and tombstones,
and checks a selected projection against current hashes and active references.
The session MUST retain applicability checks in ordinary recall; a structurally
current reference is not proof that its lesson applies to this task.

## Outcomes

The session MUST report unresolved identity separately from readable local files,
and a busy bank separately from a validated bank. Native filesystem API detection
and permission inspection perform no write probe; actual per-filesystem atomic
switch support is established during lifecycle activation. There is no global
projection registry, vector service, authoritative event-log store, cross-repo
federation or bundled migrator in the native v1 baseline; the session MUST NOT
describe `not_enabled` checks as tested integrations or demand their setup.

## Failure handling

The session MUST follow the repair for the failing component and preserve
unrelated data, ownership evidence and backups. It MUST NOT force replacement,
reset a registry, steal a live lock, delete a marker or initialize a replacement
bank to obtain a successful diagnostic. A stale derived projection can be
discarded/rebuilt through the [projections operation](../../projections/GUIDE.md);
canonical Markdown remains the recovery source.

## Diagnostic fallback

For a pending runtime transaction, the session MUST follow the
[installation recovery operation](../install/GUIDE.md) using its trusted outside
helper, then rerun doctor and the original lifecycle command. For a pending
bank write it MUST use the [writes operation](../../writes/GUIDE.md) and the
original operation/receipt. An unfinished data purge requires its own
[purge operation](../purge/GUIDE.md) and the same explicitly authorized exact
confirmation; a doctor warning MUST NOT grant new deletion consent.
