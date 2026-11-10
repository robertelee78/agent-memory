---
kind: domain
---

# The current Markdown bank

`memory-bank/` is the canonical readable project state, not a rendering that
requires a database or event replay. A session MUST retain useful current
knowledge, source links and applicability in the files themselves. Caches,
search indexes, projections and optional historical event journals MUST NOT
contain the only current copy of admitted learning.

A session MUST follow the [projection contract](../projections/GUIDE.md) when
building/checking derived references, and the [observed write protocol](../writes/GUIDE.md)
when applying mediated changes. Complete source observations and safe replay
are mechanical guards; they do not qualify an assertion as true or current.

## Small useful contexts

| File | Purpose |
| --- | --- |
| `projectbrief.md` | Stable goals and boundaries |
| `productContext.md` | User problem and intended experience |
| `activeContext.md` | Current focus, recent changes, open questions and next step |
| `systemPatterns.md` | Architecture and established patterns |
| `techContext.md` | Stack, setup, commands and constraints |
| `progress.md` | Validated capabilities, remaining work and known problems |
| `MEMORY.md` | Bounded router to contexts and optional records |

A session SHOULD start with the smallest useful set. It MUST explicitly omit
unsupported/irrelevant contexts with reasons rather than create empty files.
Every complete canonical Markdown file MUST fit 16,384 UTF-8 bytes, including
frontmatter; the bank MUST fit 65,536 sources and 64 MiB. These are refusal
bounds, not measured capacity or recall-quality guarantees. The router also
retains its 64-direct-route limit. Every canonical Markdown file MUST be reachable
under the supported routing contract. The [offline guide](../offline/GUIDE.md)
covers manual setup/checkpoint and capacity refusals; governed evidence-based
initialization remains a separate operation.

A session MUST link stronger project evidence rather than duplicate requirements,
tracker state or ownership claims. It MUST distinguish observed behavior from
approved intent and preserve conditions and provenance. Git history is useful
baseline audit evidence when files have actually been committed. It MUST NOT be
the only indication that visible guidance is stale: the current file or linked
record MUST explain correction, revocation or supersession where needed.

## State loss and recovery

A session MUST scope cleanup to known memory-owned artifacts.
A fresh session MAY resume by reading the bounded router and applicable current
context with ordinary file tools. Removing disposable memory caches, search indexes and
optional historical journals MUST NOT change the current file contents or
require recreating them from a hidden store. A session MUST NOT describe cached
prose as authoritative when it disagrees with the current bank.

A multi-file recovery journal is different from an optional historical journal.
An interruption marker or visible lock still stops normal inspection/resume;
missing recovery evidence MUST NOT be bypassed by deleting the marker or
rendering a new starter. A clean bank has no unfinished transaction to replay.
Known interrupted operations require explicit finish/rollback recovery and its
retained journal. Ordinary manual checkpoints remain cooperative file edits,
not audited admissions or concurrency-safe mediated replacements.

## Source-preserving layout recovery

When normal observation refuses an oversized source, unsupported root handoff
or invalid routing, use `am bank recover-layout`, not capture/checkpoint,
initialization, truncation or raised normal limits. For example, the packaged
inspection command is `python3 -I "$SKILL_DIR/scripts/recover_layout.py" inspect --repo REPO --output PRIVATE_INSPECTION_JSON`.
The helper accepts the same operations and arguments. `am recover` remains runtime recovery.

Recovery observes at most 1,048,576 bytes per ordinary source, 67,108,864 total
source bytes, 65,536 sources, 131,072 physical entries and depth 64; these bounds
are inclusive. It inventories non-Markdown files and directories too. Reads are
chunked, descriptor-relative and no-follow, with repeated named/parent/content
and ownership/mode/link/time pins. Symlinks, hardlink aliases, special files,
portable name collisions and unstable observations remain pending. Metadata
is exact per path: on macOS only `com.apple.provenance` xattrs are supported,
with their observed bytes preserved exactly; all other xattrs, ACLs and file
flags cause refusal. Other platforms refuse xattrs, ACLs and flags; in
particular Linux SELinux labels remain pending, not silently discarded.
A capacity refusal reports its dimension, limit and exact or lower-bound
measurement; an incomplete observation cannot prepare. Raw bytes are untrusted
data, never commands, policy or identity authority.

The existing developer/project resolver establishes the real checkout, registry
project and sanitized remote authority without creating a new project. A
malformed router needs `--identity-review` with independently checked committed
router/checkout authority and exact original-router pins. A UUID-looking string
is not evidence. Conflicting registry/settings/remote identity remains pending.
Actual principal, runtime, HEAD, branch, original index and relevant Git
configuration are pinned; recovery never changes them to obtain agreement.

Choose a private directory outside the checkout and runtime; exported artifacts
are owner-only, bounded and write-once/compare-exact. Recovery itself stores
immutable originals and metadata under the canonical external state root:
`<state>/writes/<bank-path-digest>/layout-recovery/<operation UUID>/`.
Directories are 0700, files 0600. `AGENT_MEMORY_STATE_HOME` selects the state
root when explicitly set; changing it cannot bypass the retained binding.
Do not commit originals, private source hashes, xattr values, review bodies or
raw transcripts. Exact xattr values are private metadata, not terminal output
or evidence to publish.

```sh
python3 -I "$SKILL_DIR/scripts/recover_layout.py" inspect --repo REPO --output PRIVATE_INSPECTION_JSON
# Take OPERATION from the safe result; read every numbered private source page.
python3 -I "$SKILL_DIR/scripts/recover_layout.py" inspect --repo REPO --operation OPERATION_UUID --page 0 --output PRIVATE_PAGE_JSON
python3 -I "$SKILL_DIR/scripts/recover_layout.py" prepare --repo REPO --operation OPERATION_UUID --input PRIVATE_REVIEW_JSON --output PRIVATE_PROPOSAL_JSON
python3 -I "$SKILL_DIR/scripts/recover_layout.py" apply --repo REPO --operation OPERATION_UUID --proposal PRIVATE_PROPOSAL_JSON
python3 -I "$SKILL_DIR/scripts/recover_layout.py" status --repo REPO --operation OPERATION_UUID
```

Pages contain source UUID, exact original offset/length, continuation, base64
bytes and a manifest-bound page token; every original byte is retrievable.
Each encoded page is at most 65,536 bytes. Terminal/stdout output is a safe
metadata envelope, not the private manifest/page/proposal. Inspection and prepare
write private artifacts and temporary lock state, not canonical memory or Git.

### Preparing the complete result

`agent-memory/layout-prepare-v1` requires `operation_id`, actual `author`
(`developer_id`, `harness`, `session`, observed `evidence`), `unit_map`,
`intended_files`, `metadata_plan`, `review`, `quiescence_attestation` and
`retained_source_ids`. The intended files are the **whole** native Markdown
map, including unchanged files; ordinary bounds and full routing still apply.
Metadata names every resulting file's uid/gid/mode and exact `xattrs` map, and
the same facts for each newly created directory. Each map binds the exact
attribute name to lowercase hexadecimal bytes; an absent attribute is an empty
map, not an invented value. Existing paths retain their own observed values,
not a value copied from another path. Explicit retained source UUIDs cover
untouched ordinary non-Markdown occupants; they cannot hide unsupported
Markdown.

Before canonical replacement or restoration, metadata preservation is
preflighted against what the actual filesystem and unprivileged principal can
produce. Naturally generated provenance on new files or directories MUST match
the reviewed per-path plan exactly; a mismatch or an inability to reproduce
required ownership, mode or metadata remains pending. Recovery does not set,
remove or alter xattrs, use elevated privilege, or bypass unsupported metadata.
It refuses rather than replacing a file with different metadata.

If a probe is interrupted with an unpinned or mismatched occupant, the intended
behavior is `pending-ambiguous-probe-ownership` with the affected path:
preserve the occupant and journal, without publishing a transaction marker or
automatically deleting it. Only an operator, after manually inspecting the exact
named path, MAY remove that occupant and then resume recovery. A residual
limitation remains for pinned probe directories: if a foreign actor deletes an
empty directory and the filesystem reuses its inode for an empty directory at
the same name, the replacement cannot be distinguished from the pinned
directory. Do not claim automatic recovery is assured in that gap.

The manifest-bound `agent-memory/layout-unit-map-v1` assigns every original
byte exactly once to a stable unit UUID, with exact `source_ranges`, complete
`context_pages`, `semantic_kind`, actual `existing_record` or null, honest
`provenance`, one `disposition`, exact resulting `targets` (path/hash), `reason`
and `required_actions`. Semantic boundaries preserve whole meaning, conditions
and framing, not arbitrary byte-sized pieces:

- `preserve`: keep existing useful guidance available in concise contexts or
  properly typed/indexed files, retaining actual record IDs/versions and all
  trust/lifecycle/origin/evidence metadata. Never turn it into private pending
  material merely to fit a limit.
- `pending-admission`: only unadmitted source, with exact private bytes,
  uncertainty, reason and detecting-developer actions. It is comparison-only,
  not current guidance. Each indivisible unit MUST fit normal complete
  comparison/admission bounds, security policy and the cumulative real mandatory
  witness/review/proof reservation. Otherwise leave the invalid source intact.
- `governed-terminal`: preserve an already real typed terminal/history outcome
  with its actual metadata and available target. Recovery does not invent
  lifecycle predicates. When normal governance requires eligible input,
  preserve/re-layout first and perform the lifecycle operation ordinarily.
- `non-guidance-discard`: explicit independent source-bound archive/removal
  authority and rationale, not “a backup exists.” Uncertain guidance stays
  pending. Secrets remain private, not copied to history or shared hashes.

The exact `agent-memory/layout-review-v1` binds manifest, unit map, full intended
map, `metadata_plan_sha256` and authority digests, actual author/reviewer
observations, review time, every `unit_decision`, semantic-preservation/security/
identity judgments and retained evidence. Reviewer harness/session MUST be
independent of the author.
It also binds `inherited_frontier_sha256` and explicit `retain-pending`
`inherited_unit_decisions` for every retained rolled-back obligation (empty
lists still have an explicit digest). Reviews are fallible self-reported
attestations, not authenticated truth or operator approval by inference.

Pending recovery requires the exact `layout-quiescence-attestation-v1` tied to
the real runtime artifact, state root, project/checkout, review and actual
detecting developer. All authorized old invocation routes/in-flight sessions
MUST be disabled/quiesced or use the independently verified capable runtime
and bound private state. The persistent typed transaction hold additionally
fails closed for legacy readers. Do not delete its marker or sentinel.

### Transactions, pending state and recovery

Apply retains its journal/initial ledger before mutation, exposes the exact
typed marker and `.agent-memory-recovery.md` safeguard, uses same-directory
atomic per-file replacements with router last, rechecks the complete result and
records an immutable receipt. This is not whole-filesystem atomicity. Originals,
unchanged occupants, feature files, HEAD and original index are preserved.
Never use blanket `git add -A` to publish a repair: runtime safeguards and private
pending evidence are not shared knowledge.

`layout-restored`/`canonical_eligibility:true` is distinct from
`semantic_recovery_complete:true`. Pending units remain durably outside the
bank, constrain all-source comparison and block publication. Qualified recall
may proceed with an explicit incomplete-coverage label; a lexical miss is not
semantic absence. Applicable post-apply recall has its own actual qualification
or pending result, separate from the committed layout receipt.

```sh
python3 -I "$SKILL_DIR/scripts/recover_layout.py" recover --repo REPO --operation OPERATION_UUID --mode finish --output PRIVATE_FINISH_JSON
python3 -I "$SKILL_DIR/scripts/recover_layout.py" recover --repo REPO --proposal PRIVATE_FINISH_JSON
python3 -I "$SKILL_DIR/scripts/recover_layout.py" rollback --repo REPO --operation OPERATION_UUID --output PRIVATE_RESTORE_JSON
python3 -I "$SKILL_DIR/scripts/recover_layout.py" rollback --repo REPO --proposal PRIVATE_RESTORE_JSON
```

Known-operation recovery pins the exact journal, marker, read-set, partial
physical state, originals, reviews, runtime/identity, index and ledger heads.
Only attainable ordered prefixes and exact owned complete temporaries qualify.
Unknown/partial temporaries, changed sources or external edits remain pending.
Live/unknown/foreign-host locks require the separate existing lock-recovery
workflow; age alone never authorizes lock removal.

Before a durable layout receipt, exact rollback restores original bytes and
supported metadata, even if invalid. After a verified completed repair, rollback
is a **distinct child restore** bound to the immutable parent receipt and whole
current bank/frontier, and refuses later admission/governance/revision or edits.
Parent and child receipts remain historical. An exact completed retry returns
its recorded outcome without replaying over newer knowledge; status separately
checks current matching/eligibility/coverage.

Rollback never discards pending obligations. Their hold/sentinel, originals and
stable unit IDs remain; a new inspected repair MUST independently review and
reserve the complete inherited frontier before an atomic hold handoff.
`revise-units` independently reviews exact partition/provenance/context and
capacity before append-only refinement; it does not admit children.
`resolve` requires real committed ordinary receipts and independently reviewed
exact coverage or explicit non-guidance discard authority, never body similarity.
Its public envelope identifies `operation:resolve` and the exact recovery
operation. `state:prepared` verifies the preview, not application;
`state:resolved` verifies the completed resolution and reports the observed
semantic coverage separately. An exact completed retry reports
`state:historical`, `receipt_is_historical:true` and no new writes. Historical
verification MUST NOT be treated as a fresh complete-coverage claim; use current
status or qualified recall for that observation.
For held ordinary transactions, `recover-nested` is the sole aware interface:
rollback only before the durable canonical receipt; finish only after it, linking
ledger resolution durably before restoring the hold. Missing evidence remains
pending rather than “no pending units.”

Nested previews take the ordinary scoped bank lock without changing canonical
bytes or publishing ledger/index changes. An active owner remains a refusal.
Only the known journal and durable canonical receipt can explain a ledger-head
publication interrupted before its index update; ordinary frontier observers
remain strict. Before a canonical receipt, rollback retains its exact initial
state and ordered restoration plan before the first write, so a fresh preview
can resume a rename that completed before its progress record.

If nested terminalization committed its retirement index but stopped before
finishing the exact listed bindings or removing its marker/companion sentinel,
exact `recover-nested --mode finish` can complete those recorded transitions.
Its locked preview pins the canonical receipt/current bytes, completion,
immutable ledger successor, retirement, exact same-bank dependency bindings,
scope and remaining safeguards. Unlisted active dependencies, rollback, changed
canonical bytes, foreign replacement markers/sentinels or mismatched completion
evidence remain refusals. Ordinary bank observers remain strict throughout.

A standalone resolution journal reserves its exact ledger successor until its
receipt, linkage and safeguard retirement have an exact durable completion.
New resolutions, held transactions, unit revisions, outer layout inspection/
preparation/application/recovery and child restore refuse under the ordinary
bank lock while any same-bank resolution journal remains unfinished; merely
creating a completion file does not release that reservation. This includes
previously prepared continuation proposals: they cannot replace the hold before
its reserved resolution finishes. Retry the exact resolution proposal after
separately recovering any dead owned lock. Recovery completes the same linkage/
retirement without appending another resolution. Historical success is returned
only after durable completion, never by discarding a remaining hold.

Recovery-aware `publish inspect|prepare|land` and separately reviewed
`retention inspect|apply` use their own exact requests. Publication requires a
terminal complete frontier, all-side raw-source review, memory-only Git delta
and actual existing nonforce policy/reviews/checks. Retention refuses pending
units or dependencies and deletes only individually pinned owned artifacts,
retaining historical receipts/tombstone and explicitly surrendering rollback.
Neither layout success nor backup age grants publication or cleanup authority.
The [development smoke recipe](https://github.com/robertelee78/agent-memory/blob/main/docs/testing/README.md#disposable-public-smoke-recipe)
uses disposable synthetic sources and is not implementation acceptance.


## Domain loading and routes

A session MUST load this domain before its selected operation leaf and MUST NOT
preload sibling leaves for completeness. It MUST select the next operation from
actual observed state. The [shared model](../model/GUIDE.md) MUST load only when
a semantic question remains unresolved by this domain and operation.

A session MUST read [records/GUIDE.md](../records/GUIDE.md) when a lesson needs
typed provenance, validity, correction or revocation. Ordinary context prose
does not need metadata on every paragraph. Record format validation MUST NOT
be treated as admission or proof that its evidence and conditions are correct.

| Intent | Operation |
| --- | --- |
| Use the file-only workflow | [offline](../offline/GUIDE.md) |
| Inspect current bank files | [inspect](inspect/GUIDE.md) |
| Inspect typed record format | [records](../records/GUIDE.md) |
| Perform observed writes/recovery | [writes](../writes/GUIDE.md) |
