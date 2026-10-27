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
or identity bootstrap is needed. Signed-source authentication requires OpenSSH
SSH-signature support; its absence remains a runtime failure, not bank corruption.
`--repo` selects the repository/worktree whose
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
For an explicitly requested safe repair, use `am doctor --fix --repo /absolute/repository`.
The repair rechecks the exact recorded source, recovers an owned interrupted
runtime transaction when its journal can be verified, and reconciles missing
or outdated receipt-owned client integration with the transactional installer.
It may also complete cleanup of an unlocked bank transaction whose verified
receipt uniquely establishes its outcome, using the existing recovery protocol.
It then runs doctor again and reports repairs and remaining refusals. It does not
infer identity, overwrite modified targets, resume purges,
or rebuild an arbitrary selected projection. Active locks, untrusted sources,
changed hooks and ambiguous transactions remain preserved for inspection.
Packaged public entrypoints disable Python bytecode before importing product
modules. A session MUST use these entrypoints for supported audits. For custom
Python inspection, it MUST pass `-B` or set `sys.dont_write_bytecode = True`
before the first product import; setting it after imports is too late. `-I`
alone does not disable bytecode. A session MUST NOT call a bare product import
read-only unless bytecode suppression is already established.

Supported runtime audits preserve the complete artifact file bytes and modes;
they MUST NOT filter executable caches out of an integrity comparison. Unexpected
`__pycache__` or `.pyc` files remain drift, even if all receipt-owned source bytes
match. A session MUST preserve them and use a trusted outside helper to inspect
the receipt and changed targets. Read-only diagnostics grant no permission to clean caches,
rewrite receipts or force an update. A source hash identifies bytes, not the
process that wrote an unexpected file.

Runtime-artifact immutability is distinct from a declared private audit outside
the package: governed recall may write such an audit while preserving canonical
memory and runtime. Version, status, doctor, artifact inspection and package
validation perform no setup or private audit writes unless `doctor --fix` was requested.
Without `--json`, these commands print a human-readable view of the same result.
A session MAY add `--projection /absolute/reference-projection.json` to doctor
when an optional derived reference file is actually in use. Without `--fix`,
doctor MUST NOT initialize missing state, rebuild caches, acquire a new writer
lock, migrate, recover, uninstall or purge data. `--fix` MUST use the verified receipt-owned runtime and bank recovery operations
described above, or the shared identity preview/apply protocol for a uniquely
recognized historical checkout after a filesystem device-number change. It MUST
preserve the existing project UUID, bank and historical binding. Read-only
continuity already permits memory operations; persistence is optional and changes
pinned registry evidence, so previously prepared proposals require fresh preparation.
Prompts that failed before source recording require a new real prompt.

Doctor also reports private Codex prompt/stop failure records independently.
Their fixed stage/reason codes describe the last observed adapter failure;
absence of a failure record does not establish assessment, capture or publication.

## Read back

The session MUST read `failures` and `warnings`, not just the process exit code.
Exit 1 means a component failed verification; exit 0 permits qualifications such
as missing setup, an active writer, pending recovery or the initial unsigned
distribution channel. Each failure names its component/path and a bounded repair.
The initial channel's digest establishes artifact identity, not publisher trust.

The machine `security` summary reports `publisher_authentication`,
`signature_verification_performed` and the `recorded_source_selection` separately.
A signed `release` source requires fresh pinned-key verification of its manifest
and actual portable bytes, agreeing with the recorded activation. Diagnostics
use existing signature/key files without network or verification scratch writes.
A recorded managed Git source remains unsigned; explicit development/custom
source is operator-selected trust. Missing,
malformed or inapplicable active receipt evidence leaves selection unavailable.
A session MUST preserve the negative already-loaded-verifier and platform-ACL
qualifications, including on failure results, and MUST NOT interpret these fields
as a full ownership/compatibility/recovery pass. Signature verification MUST
supply actual activated-source evidence; a digest or extra receipt claim cannot
stand in for it. Old-process unsigned transitions remain unsigned observations.

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
federation or production format conversion chain in the native v1 baseline;
the session MUST NOT
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
