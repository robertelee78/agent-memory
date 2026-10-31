---
id: ADR-0021
status: accepted
date: 2026-10-27
amended: 2026-10-30
amends: [ADR-0018, ADR-0020]
---

# ADR-0021: Unobtrusive boundaries and truthful operation health

## Context

The [operator-incident RCA](../research/2026-10-27-intrusive-boundary-rca.md)
identifies an actual Linux path-policy inconsistency, generic diagnostics,
unnecessary workspace calls and repeated task dialogue. v0.0.8's successful
source coverage did not establish acceptable operator-visible normal use.
This corrective source does not reopen or alter its shipped tasks.

The operator requested that the defects be fixed and a new release published.
The implementation uses one compact routine CLI/native assessment check, with
quiet stdout. This is a bounded improvement within the existing skill-plus-CLI
contract, not a claim that native tool cards can be hidden. No zero-visibility
capability is assumed. The operator requested fixes and release; the one-command route is a reversible
implementation default within the existing skill/CLI contract, not a claim of an
explicit preference vote. No new approval question blocks the clear defects.

## Decision

### T1: Consistent state trust and specific refresh diagnostics

Quick refresh MUST use the existing private-ancestor protection policy consistently
with the trusted path helpers through one shared policy evaluator, retaining
descriptor-based no-follow observation,
ownership, unsafe unprotected-path rejection and rebinding checks. It MUST NOT
chmod existing directories merely to pass. A protection barrier is a non-symlink,
current-user-owned directory without group/other access, observed root-to-leaf
through pinned directory descriptors. It protects descendants, not its own
replaceable entry. Preserve the shared Unix ownership/root-sticky/permission
policy exactly, including trusted ownership checks below a barrier; ACLs are not
authenticated. Group membership grants no Unix traversal through that barrier.
Parity tests MUST cover the common evaluator, and barrier chmod/rebinding during
observation MUST refuse stale proof. Direct diagnostic imports MUST use `-I -B`
(`-I` ignores PYTHONDONTWRITEBYTECODE); signed verification MUST NOT skip or bless
unexpected executable caches. Unknown changes remain preserved for inspection. Specific safe failure codes and next
steps MUST survive to the CLI; a setup-only doctor result MUST NOT claim that an
unexercised refresh operation succeeded. Safe diagnostics MUST identify the
failing phase and a fixed code (including transaction-path-untrusted,
transaction-recovery-pending, current-source-changed or current-review-required)
plus a concrete next action; they MUST NOT echo arbitrary secret content. Doctor
MUST exercise the relevant local prerequisite or label it not exercised, without
claiming a fresh remote observation from a local check.

Tests MUST reproduce protected 0775 intermediate directories below a trusted 0700
barrier, and MUST preserve refusal for unprotected writable paths, foreign owners,
symlinks, changed source/context and directory rebinding. No canonical bank,
feature code, index or branch changes are permitted from the quick check.

### T2: Quiet routine operation and distinct completion stages

Routine no-learning handling MUST avoid private workspace creation and use a
bounded direct assessment route. Codex and Claude perform routine checks before
emitting the task answer. OpenCode retains its existing native queued control
pass after the original answer: that pass MUST preserve the original native
answer record, suppress successful helper receipt bodies and MUST NOT repeat the
business answer. The v0.0.9 native compatibility exception below qualifies its
remaining model-generated control text; it is not a quiet-success claim. This timing exception avoids inventing a new pre-final source identity
contract. Native UI and SDK result attribution MUST be qualified separately, retaining
which original/control record the SDK exposes as the final result.
Codex and
Claude MAY combine the quick refresh observation and the explicit assessment
in that same CLI invocation; OpenCode MAY use one native call combining source acknowledgment and quick refresh through the
versioned combined-operation schema when the actual source is already in context. Source-window tools
remain required for unavailable, omitted or recovered original evidence. Supported quiet success MUST suppress result
payloads while retaining useful failure output. Instructions MUST put the routine
route first and avoid rereading unchanged loaded guides. First-use loading and
task-relevant recall remain visible and MUST be counted separately, never
reclassified to hide repeated routine work.

Refresh, assessment, admission and publication MUST retain distinct states.
A verified source assessment MUST NOT imply freshness, capture or publication.
Failed refresh MUST remain explicit and actionable without displacing the task
answer or inducing repeated repair attempts on every dialogue turn. Even on a
failed assessment, emit the task answer only once, with one brief failure notice.

The confirmed incident emitted a full PM answer in commentary and repeated its
question in the final answer. Routine guidance MUST keep the business response
out of commentary and emit it only once. For the pre-answer adapters, this follows
assessment; the OpenCode control pass MUST NOT emit another business response. PM
answers and questions MUST NOT be repeated solely as memory bookkeeping.
Codex MUST support quiet-success parity with the existing Claude adapter.
Direct no-learning stdin intake remains at most 64 KiB. A forgotten-operation
fallback retains the existing durable cap: at most one wake per source-set
fingerprint and two per native lifecycle; no unbounded repair retries. A known
unresolved failure MUST be reported once per session/state change rather than
replaying the same repair conversation every turn.
The implementation MUST document the actual native limits of post-answer
continuations and SDK final-result preservation. It MUST NOT silently remove
mandatory capture after forgotten invocation, invent no-learning receipts or
promise tool-card suppression through an unsupported host field.

Once the installed guidance is loaded, normal no-learning completion MUST use
at most one memory-maintenance command/native call total, create no private request workspace and emit no successful receipt body or
memory narration in the pre-answer adapters. OpenCode retains the explicitly
counted v0.0.9 compatibility limitation below. Required source recovery, genuine admission/publication and
consequential failures are distinct cases and MUST retain their actual work.
A failed refresh MAY coexist with a truthful no-learning assessment, but combined
health MUST remain pending/false and the process MUST report one actionable
failure rather than a green overall success. Existing assessment-only receipt
compatibility MUST NOT be silently redefined: existing source assessment schemas
keep their meaning, and a combined response MUST use an explicit versioned
combined-operation schema with separate stage results. CLI stdout silence is not evidence
of invisible tool cards. Stop fallback remains bounded; automatic capture MUST
NOT be removed to obtain a quiet transcript, and host-specific final-result
limitations MUST remain explicit.

### v0.0.9 OpenCode compatibility exception

The operator repeatedly requested a corrective release and reaffirmed the goal
of fully landing v0.0.9. The release implements the verified functional repairs
without making native silence a new release hold. This is a bounded delivery
decision under that existing mandate, not a claim that the operator cast an
explicit preference vote about control text. It does not waive data integrity,
permissions, exact source coverage or truthful stage reporting.

Actual OpenCode 2.1.3 with Baseten Kimi K3 completed the three-turn PM fixture,
preserved original answers and all bank/code/index/HEAD state, and used one
source-bound combined acknowledgment per turn with zero request workspaces.
Normal acknowledgments had no receipt body. Its model nevertheless emitted a
brief bookkeeping response and one gratuitous Code Mode null operation on
successful control passes. The v0.0.9 release MUST document and count that native
limitation and SDK final-record behavior; it MUST NOT call those trials quiet
passes or infer behavior for unqualified providers.

Stronger empty-output instructions and an exact-control stop-sequence experiment
failed actual native qualification. The complete experimental terminal code,
epoch caches, tool removal and sentinel handling were removed. No provider/model
qualified for that optimization. v0.0.9 MUST NOT enable it, cancel whole sessions,
filter substantive model answers or alter permissions to disguise the limitation.
Failed trials MUST remain retained as failures. The original functional baseline
is independently hash-matched to its completed native proof.

Fully silent native OpenCode completion is tracked by T4 for v0.0.12. This
exception permits only the documented control text/no-op; all v0.0.9 functional
acceptance criteria and safety boundaries remain required.

### T3: Operator-visible acceptance and corrective delivery

Acceptance MUST include an actual multi-turn PM conversation in Codex, Claude and
OpenCode, with a decision, a follow-up question, an unchanged bank and an injected
refresh failure. It MUST count routine commands/workspaces and repeated business
answers/questions, check truthful stage outcomes, and retain pending sources.
Native UI and machine-result behavior MUST be reported separately. Broad repeated remote CI is excluded.

Acceptance thresholds are routine assessment calls <=1, request workspaces=0,
repeated business answers/questions=0. OpenCode control text and its extra null
operation MUST be counted separately under the specific v0.0.9 exception; they
are not hidden as first-use loading or classified as quiet passes. An injected
refresh failure produces one
actionable notice and combined health false. Counts MUST be derived from actual
native event/transcript records with retained sanitized evidence, not merely a
passing narrative. Real Linux qualification MUST cover the observed 0700/0775
chain and a negative unprotected path. Each command has a hard 15-minute timeout;
timeouts fail that case and are not skipped.

The signed corrective release MUST retain actual public installation/update
checks and consumer-memory isolation. Verification MUST cover signatures, public artifact delivery, consumer-bank
isolation and absence of new executable-cache drift after verification. No
release is claimed by this design. Accepted for corrective v0.0.9 execution under the operator's fix-and-release
instruction and the retained Opus review. Source-bound AWA readiness and
independent implementation/delivery review remain separate requirements.


### T4: Fully silent native OpenCode completion (v0.0.12)

T4 is Backlog for v0.0.12 and MUST NOT delay v0.0.9's verified functional repairs.
It MUST establish a supported native mechanism for completing only the exact
acknowledged control without extra tools, bookkeeping prose or a repeated
business answer. It MUST preserve original task results and qualified SDK result
attribution, actual source assessment, automatic durable capture, permissions,
new operator/steering input and pending evidence. It MUST NOT substitute a
persisted assessment-only receipt for fresh combined success, invent source
coverage, cancel whole sessions, filter substantive answers or rely on unbounded
model retries. Alternative source timing or native API changes require their own
reviewed design before implementation. Actual native multi-turn qualification
MUST count routine calls, text, source coverage and failure behavior, retain
unsuccessful trials, and qualify provider/model compatibility explicitly.

### T5: Recurring bytecode writer RCA and recovery proposal (v0.0.11)

The existing `ADR-0021:recurring-bytecode-drift` follow-up SHALL retain its stable
issue identity. Its v0.0.11 outcome is a source-bound RCA, reviewed preventive
diagnostic guidance and independently reviewed recovery proposal. Issue #238
SHALL close only after those exact deliverables receive independent acceptance
and release delivery. The separate implementation item #240 SHALL own actual recovery with its own
design, attempt and acceptance gates under T7 below. Closure of #238 alone MUST
NOT be presented as shipped automatic recovery.

The [operator adoption incident](../research/2026-10-28-v010-operator-adoption.md)
proved signed payload bytes/modes were intact and 35 additional CPython3.14
caches caused an actual update refusal. The subsequent
[writer investigation](../research/2026-10-28-v011-bytecode-rca.md) SHALL bind the
actual process invocation and file evidence, distinguishing attribution from
mere timestamp correlation and actual incidents from disposable reproductions.
The investigation MUST NOT blame an AM-owned launcher without source evidence.

Preventive guidance SHALL prefer the installed CLI or documented interpreter
launcher. An independently necessary trusted diagnostic import MUST use `-I -B`
before package code runs; bare Python imports of a managed package MUST NOT be
used as a routine diagnostic shortcut. `-I` ignores the environment's no-write
flag, while `-B` prevents writes but does not authenticate or suppress reading
an already-present cache. An unverified installed package MUST NOT be imported
as its own integrity verifier.

The original recovery proposal explored bytewise regeneration and isolated parsing.
T7 supersedes that unselected strategy: unsigned cache contents are opaque evidence,
never an input to `marshal.load/loads`, proof of benignness or runtime execution.
The source-bound RCA and prevention work remains independently accepted as T5.

Bounded public-operation fixtures on macOS and Linux SHALL demonstrate the raw
import cache-creation/update-refusal mechanism and the ordinary guarded launcher's
contrasting behavior, retaining source/version, observations and unchanged
payload/receipt/bank/feature state. A smaller fixture MUST NOT be mislabeled as
reproducing every historical module or caller. Private original records SHALL
remain private; public evidence SHALL include only needed redacted invocations,
hashes, provenance and outcomes. Native model reruns are not required for this
CLI/source investigation.

### T6: Direct prepared-refresh handoff (v0.0.11)

The [refresh scripting RCA](../research/2026-10-28-v011-refresh-handoff-rca.md)
identifies unnecessary extraction code around an already supported CLI input.
Guidance SHALL retain the complete private prepare response and pass that same
file directly to refresh apply; it SHALL distinguish that stored response from
its concise stdout artifact summary. It MUST NOT require JavaScript extraction,
reserialization, a private copy of the nested proposal, or an ad hoc file
comparison solely to perform this handoff. Existing callers with the exact
inner proposal remain compatible.

Sessions SHALL use prepare's `changed_memory_paths` and the complete applied
transaction's `receipt.changed_memory_paths` with their actual stage meanings.
An empty delta MUST NOT substitute for apply, source revalidation, authority
freshness, relevant recall or cursor completion. `refresh_applied` can be true
while `canonical_writes_performed` is false; publication and feature-code
freshness remain separate. No unchanged-text comparison proves shared truth.

The existing full-envelope route SHALL gain an explicit metadata-consistency
guard before output preflight or apply side effects. All authority and source
pins MUST derive solely from the validated inner proposal/proof. For a full
prepare response, require its known schema, prepare operation, prepared stage,
strict Boolean stage facts and no additional outer authority/pin fields.
Its declared `changed_memory_paths` MUST equal the pure delta reconstructed from
the inner observed/planned hash maps; blocked paths MUST match the inner proof.
Conflicts remain informational data, not permission or authoritative source pins.
An inconsistent wrapper SHALL fail as `refresh-prepared-envelope-invalid` in
`apply.prepared-envelope`, with a fixed safe next step to preserve and use the
original retained preparation or obtain a newly reviewed one. It MUST NOT edit
fields automatically or bless the wrapper as a receipt.

This guard MUST NOT reread live sources or run fresh semantic checks merely to
validate report metadata: an already-applied import must retain its supported
full-file historical retry. Existing transaction/semantic/under-lock checks
remain responsible for actual applicability, source validity and fresh outcomes.
Exact-inner callers retain their existing contract. Public tests SHALL first
retain the old acceptance of contradictory outer operation/delta/repository
fields as a baseline (no invalid inner authority was accepted), then prove all
three are refused before receipt/cursor/Git metadata mutation. Malformed Boolean
stage facts and other contradictory stage metadata SHALL also be refused.

The full-envelope route MUST retain existing v2 private-output requirements,
exact project/repository/operation/checkout/source pins, semantic proof,
under-lock rechecks, idempotency and failure preservation. Public-command proof
SHALL cover unchanged and actual imported content, exact inner-proposal
compatibility, full-file retry, changed local/authority source or inner pins,
artifact-summary input refusal and unsafe output refusal. Canonical memory,
feature work and the original index/HEAD SHALL change only as the existing
reviewed refresh operation permits. This outcome requires no new CLI schema,
semantic engine, harness integration, MCP or cross-repository behavior.

T5, T6 and T7 are selected for bounded v0.0.11 work under the operator's current
release instruction. Independent design approval, AWA attempt readiness,
source-bound acceptance and actual signed release delivery remain separate.
T4 and other deferred roadmap work remain Backlog for v0.0.12 with their original
approval gates. The published v0.0.9 and v0.0.10 delivery histories remain intact.


### T7: Signed update recovery from opaque cache drift (v0.0.11)

On 2026-10-30 Beast again refused `am update`: 25 additional CPython3.14 cache
files had appeared while all 189 recorded payload files still matched their
receipt. This read-only comparison is not fresh signature verification or caller
attribution. The operator explicitly rejected treating this as a normal failed
update. `ADR-0021:verified-cache-healing` (#240) therefore moves from v0.0.12 to
v0.0.11. T5's research-only outcome remains distinct.

The updater SHALL recover automatically when the existing signed runtime differs
only by a tightly bounded set of opaque cache extras. It SHALL authenticate the
old signed manifest and signed payload inventory and the selected new signed
release before activation. Receipt fields alone MUST NOT establish authenticity.
Every signed path, byte and mode MUST match the authenticated old payload. Extra
paths SHALL be restricted to ordinary current-owner directories named
`__pycache__` beside signed Python modules and current-owner regular single-link
recognized CPython cache files for those exact signed modules. Names, counts,
per-file bytes, aggregate bytes and full-tree observation SHALL remain bounded.
Foreign owners, links, special files, unrelated paths, changed/missing signed
payload and changed signed metadata MUST refuse with specific next steps. Cache
bytes SHALL remain opaque and untrusted: no execution, marshal parsing, cache
validity claim, source-file repair or new receipt that blesses extra files.

Before any `agent_memory` import, updated public launchers SHALL disable bytecode
writes and route bytecode reads to a fresh private empty cache prefix, retaining
that prefix through process exit. Startup MUST NOT import suspect installed
package caches as its own verifier. Source entrypoints and the operator's already
trusted interpreter/stdlib remain the installation trust base; this is not a
sandbox or protection from arbitrary modification of the executing launcher.
The old v0.0.10 launcher cannot retroactively acquire this protection. The public
one-line signed installer SHALL provide the supported first transition using its
independently acquired verified candidate; this limitation MUST be documented
rather than claiming the old installed command already runs the new updater.

Under the existing lifecycle lock the updater SHALL rebind the complete observed
old tree and preserve it, including all opaque cache bytes, in the normal
journaled transaction backup. It SHALL activate a clean authenticated candidate
using ordinary installation planning, compatibility checks, target ownership,
exchange, readback and receipt commit. Same-version signed reinstall is eligible
when cache drift needs recovery; a clean already-current install remains a no-op.
`--check` SHALL report planned recovery and SHALL NOT mutate active source,
receipts, targets, bank or repository state. The installed source and client-visible
packages MUST pass the ordinary exact inventory after activation. Rollback MAY restore the exact pre-existing active tree through the existing
transaction; this restores the previous state and does not qualify its caches as
safe. Recovery MUST NOT invoke that old runtime, claim a healthy rollback or
mark the dirty tree verified. Failed/interrupted activation SHALL preserve the
original tree and journal and report pending recovery. A trusted source-only
retry SHALL requalify the state rather than execute the restored cache files. The backup and
its source-binding evidence SHALL be retained and identified in the result.

Independent public-command acceptance SHALL cover clean no-op, older-version
upgrade, same-version cache recovery, read-only preview, unknown opaque cache
contents, changed/missing signed payload, wrong path/name, foreign ownership,
symlink/hardlink and concurrent-tree mutation, plus failed/interrupted activation
and exact retry. macOS and Linux fixtures SHALL retain original bytes, receipt,
index, HEAD and bank evidence. No test SHALL execute an untrusted cache. Actual
operator repair follows the same trusted public workflow; no manual deletion,
receipt editing, unsigned fallback or permission weakening substitutes for it.


#### T7 implementation boundaries

One explicit qualification result SHALL bind the authenticated old manifest
digest, authenticated non-cache inventory digest, receipt/source identity and
complete observed dirty-source fingerprint. `current`, `prepare` and the source
`add` operation SHALL consume that same result, rechecking the bound fingerprint;
no global integrity exclusion or general `verify_targets` relaxation is allowed.
Only the managed signed source target is eligible. Other targets, including copy
mode client packages, remain strict. Drift there MUST name the affected target
and recommend trusted installation diagnosis; it MUST NOT silently bless copies.

Qualified drift SHALL disable the same-manifest reuse shortcut, force fresh
private authenticated archive acquisition even for the current version, and
force the source replacement action. `--check` SHALL separately report planned
recovery when `update_available` is false. No pre-activation subprocess SHALL
invoke the old runtime (`wrapper_readback` included). Only the freshly installed
new wrapper may be exercised during post-activation readback.

The allowance SHALL be at most 256 cache files, 2 MiB per file and 16 MiB total,
within a bounded complete tree. Accepted cache modes SHALL be 0600 or 0644, with
current-user ownership and link count one. Directory modes SHALL exclude group
or world write permission. Cache names SHALL be
`<signed-module-stem>.cpython-3NN[.opt-N].pyc`, with the matching signed `.py`
module in the parent of `__pycache__`; all other names remain refused.

Cache-read isolation SHALL cover the public `wrapper()` bootstrap and legacy
raw runpy wrapper, `am_console.py`, `installer.py`, `update_runtime.py`,
`hooks/python3.sh` (Codex/Claude prompt and Stop) and every OpenCode plugin Python
invocation. Each process SHALL create a fresh owner-only empty temporary cache
prefix and set `sys.pycache_prefix` with bytecode writes disabled before runpy
or any `agent_memory` import. The prefix SHALL live for the child/process lifetime;
cleanup SHALL remove only that process's owned temporary directory on normal
exit, while termination may leave an empty temporary directory. This is cache
read isolation, not code authentication or a sandbox.

Acceptance SHALL prove the acquired candidate contains no caches, no old runtime
subprocess runs before activation, updated CLI and hook launchers never read a
poisoned cache or write caches in the installed tree, and a source-only retry
converges after a simulated concurrent writer ceases. Failed activation that
restores a dirty source MUST remain unverified/pending with all old bytes intact.
