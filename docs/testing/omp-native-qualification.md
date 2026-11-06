# Native omp integration: setup, limits and qualification

The target is **omp 18.16.0**, not an inferred version range. The
[source-bound interface study](../research/2026-11-04-omp-native-interfaces.md)
pins upstream `650f3a108229ee11bf68ba3377f4e8c3391cd424` and the originally
inspected macOS binary. Source/binary equivalence is unproven. This document is
an executable qualification recipe and support contract, **not a claim that
its matrix has passed**. Signed release evidence must name the actual candidate,
installed artifact, binary, provider, platform, client mode and independent reviewer.

## Explicit installed binding

`all` remains `agents,codex,claude,opencode`. It never includes executable omp
code, including an update of a previously saved `all` installation. A person
must choose their actual canonical absolute omp agent directory; the installer
never derives its owned target from HOME, XDG or the active profile.

First quiesce all old/aware agent-memory writers touching the same bank/private
state and upgrade peer readers before confirming `--omp-peers-upgraded`. This
is an operator rollout attestation, not an inferred process-list guarantee.
Different clones with different private state do not share this source index.
Preview a trusted complete candidate, then apply the same arguments without
`--check`:

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/installer.py" install --check --development-source /absolute/candidate --presets omp --omp-agent-dir /absolute/omp-agent --omp-peers-upgraded
```

The preview names three independently owned destinations: the skill directory,
`extensions/agent-memory.ts`, and the optional AGENTS.md activation block.
`--no-guidance` omits only that Markdown block; explicit omp still selects its
executable extension. No provider, authentication, approval, tools or global
extension configuration is changed. `PI_CODING_AGENT_DIR` may select that
**already chosen** native root when launching omp; it never redirects saved
update/uninstall ownership. Restart native clients after an installed change.

The explicit native installation emits `install-receipt-v2` with its exact
`omp_binding`. Non-omp installations retain v1. Omitted update configuration
preserves the saved root. Reinstalling into a different root, or selecting
`all` to remove an existing native binding implicitly, refuses; use explicit
owned uninstall/reinstall. Modified/unowned files are preserved. An older
runtime cannot adopt v2 ownership as if it were v1, even after sources drain.

The apply path uses that same explicit/saved root when validating lifecycle
write boundaries, including the extension destination. Qualification MUST
exercise actual application in both copy and link modes; a successful
`--check` does not prove installation or native extension loading.
The signed one-line installer accepts these same explicit native selection and
quiescence options; its generated loader and checked-in loader MUST agree.

Enabling a new native route during a layout recovery hold additionally needs
`--omp-layout-quiescence /absolute/private-envelope.json`. This is the existing
independently reviewed layout-quiescence attestation/review envelope, bound to
the candidate artifact and including the exact owned native extension path in
`invocation_routes`; a second boolean is not a replacement for that review.
Preview forwards that validated native envelope into the common private-state
compatibility check without publishing it. The common check still requires the
exact bank/artifact, independent review and **all** newly enabled invocation
routes, not just the native extension. Apply retains the existing guarded
publication ordering; preview never renews the bank's saved attestation.


## Authority, privacy and coverage

The extension can retain **private source evidence and bounded advisory
requests only**. It cannot admit/correct/consolidate memory, refresh/exchange
Git, acknowledge assessments, change permissions, force a model, execute
model-selected shell commands or rewrite the substantive answer. It launches
only its fixed installed observer helper through the receipt-owned
`/bin/sh .../python3.sh` supported-interpreter selector. This is not an
interpreter-pinned security claim. Canonical operations use ordinary model
tools with the person's unchanged native permissions.

`message_end` retains metadata only. Display content, arguments/results,
thinking, signatures and image bytes are never copied or hashed by that
callback. Subsequent lifecycle sweeps project detached **stored** native
entries, preserve native placeholders and attribution/tool identities, and
exclude credential/config/provider/reasoning payloads. Shared unsafe-source
screening withholds a flagged entry as a whole. It does not detect every
unknown secret: native stored user/tool plaintext can remain plaintext in
private snapshots. An unavailable image/file/artifact payload stays a coverage
gap; no arbitrary path or URL is dereferenced to fill it.

Private snapshots, manifests, unresolved gaps and enrollment records live
beside the shared turn-source state under owner-only `omp-source-v1`. They
remain on uninstall. There is **no automatic purge or global retention quota**
in this milestone. Per-source/manifest input is bounded at 16 MiB, per-segment
captures at 16,384, active tool bindings at 1,024, cursor at 1 MiB and in-memory
metadata queue at 4,096. Bounds/disk-full stay explicit pending failures, never
truncation-as-assessment. The existing shared source/index bounds are unchanged.

Each substantive segment has at most one admission source and one sealed
manifest in the shared ledger, not one per assistant/tool message. Late
substantive/foreign work opens a linked successor with the original human root;
completed sources are never reopened. Genuine new human steering owns new
assistant work, while a late tool result retains its invoking lifecycle/root.
A fork has its own namespace; inherited context is not a fresh human admission.
Resume keeps existing source identities and snapshots. Native mapping is a
qualification, not a requirement that the host transcript survive forever.

An exact reserved own hidden message has **no companion allowlist**. Human
input, joined foreign hook text, goal/plan/vibe/todo/advisor messages or unknown
companions force substantive capture. Pure control classification never grants
a tool permission; unexpected assistant text or unrecognized invocations are
retained in an ambiguous successor rather than silently discarded. Root-keyed
exclusive wake records permit at most **one automatic request per genuine human
root**, including after foreign continuations or changed pending fingerprints.
A later human wake carries both suppressed and unsuppressed earlier work from
all sessions of this bound project. Guidance is at most 8,192 UTF-8 bytes;
explicit pending cursors/faults account for omitted references. There is no
shared-core wake reservation that can suppress the healthy new root.

The adapter stops awaiting after at most 950 ms (earlier on native cancellation)
but never kills a helper during shared writes. This is a callback-await bound,
not a whole-observation or registration wall-clock deadline: capture, scope
checks, inventory and durable enrollment can finish in the background after it.
A timeout, native continuation cap, queued/deferred scheduling, permission
refusal, cancellation, provider/tool failure or model omission means **pending**,
not completion. It returns advisory `continue:true`, never a blocking decision
or synthetic human instruction.
Ephemeral `--no-session` may retain safe private snapshots, but cannot recover
content the native process never exposed safely. Missing/unsafe payloads remain
explicit gaps, potentially indefinitely. Combined modes can leave a final
pending successor when the person stops; later human wake or explicit recovery
is required. Source acknowledgement is not an exactly-once semantic publication
lock; independent agents can still propose overlapping meanings and must
reobserve/search before ordinary admission/publication.

## Inspect, assess and troubleshoot

```sh
am status --repo /absolute/project
am doctor --repo /absolute/project
am omp-boundary --repo /absolute/project --list-pending --offset 0
am omp-boundary --repo /absolute/project --source ACTUAL_ID_OR_ENROLLMENT_REFERENCE --session ACTUAL_SESSION --lifecycle ACTUAL_LIFECYCLE
```

Follow `next_offset` until exhausted. A successful page never claims global
source completeness. Registered rows have an actual `observation_id`;
`enrollment-pending`/`registration-fault` rows preserve the original source
reference and do not invent one. Later observer admit/ingest/seal calls first
persist every arriving source/metadata record and recheck scope, then run a
separate registration-progress phase. Each phase selects oldest retained
fault-free enrollments first and attempts at most eight, using their original
references and digests. The bound is a count, not an elapsed-time check after
expensive capture, inventory, scope or shared-state inspection: those costs
cannot consume the next phase's entire opportunity to register. Already
registered and faulted records do not consume attempt slots. Capacity can still
leave enrollment pending; a later phase resumes without discarding new arrivals.
A successful registration whose response was lost is recovered from the exact
shared reference/digest. A shared fault or an uncertain attempted write is
retained and never blindly registered again or erased by transport retry.

The unchanged `NativeSources` contracts in `tests/contracts/test_omp_boundary.py`
and dedicated `tests/contracts/test_omp_enrollment_progress.py` regressions cover
large retained segments, bounded oldest-first progress, new arrivals, lost
success, uncertain attempts and retained shared faults. The latter also drives
an explicitly installed public observer/source/no-learning journey with 151
stored entries. These are real transport/CLI checks, not native callback,
extension-loading or model/provider qualification.

Read every source before assessment. Use exactly one original observation per
`am omp-boundary --repo ... --stdin --quiet --refresh` invocation, with exact
fields `session,lifecycle,observation_id,source_sha256,outcome,assessment` and
`publication_request` only for `published`. No assessment array or synthetic
merged receipt exists. An ordinary approved tool invocation may chain two
explicit commands, each returning its own receipt. Admission acknowledgement
requires the sealed segment already registered. Every unresolved substantive
gap refuses both no-learning and publication completion. Only observer ingest
can retain an evidence-checked gap resolution; model input cannot waive a gap.
A no-learning assessment is the agent's bounded authored review and candidate
dispositions, never deterministic semantic truth. Publication requires the
existing intended-delta authority witness. Receipts retain
`assessment_is_agent_assertion:true` and
`source_publication_link_verified:false`.

Disk ownership, current native discovery/loading, source enrollment/coverage,
source assessment, refresh and witnessed publication are distinct stages.
`am status`/doctor explicitly say when native loading is not live verified.
Disabled skills/extensions, unsupported versions, missing bank/binding and
duplicate module paths do not imply support or completion. Check the selected
native root, version and extension filters; do not enable a denied tool or
change provider/approval mode merely to make memory pass. Inactive no-bank
contexts never initialize a replacement bank automatically.

## Lifecycle drain and rollback

Before the first native shared reference, the observer durably writes
`turn-sources-compat-v1.json` declaring `omp-harness-v1`, an immutable
`omp-bank-binding-v1.json` binding the canonical repository, bank, project and
private namespace, and a local registry pin to that protected state location.
Common lifecycle preflight unions that requirement with `layout-frontier-v1`,
saved bindings, actual known private-state namespaces, source indexes and
recovery state. Changing the configured state root cannot hide the saved
binding. Missing/offline/malformed inventory is unknown, not empty.
Clearing one feature cannot clear the other. The total order is runtime
lifecycle → bank source barrier → transport registration → sorted completion
locks → bank write (optional catalog invalidation last).

An aware explicit lifecycle request with `--omp-writers-quiesced` can disable
only the unchanged receipt-owned native executable, retain a disable journal,
and attempt the bound drain proof. `--check` previews that phase without
removal. Incomplete/unregistered/orphan sources, unresolved gaps, faults,
in-flight writers or an incomplete inventory retain the marker and refuse
incompatible activation. A failed drain keeps its owned binding disabled and
its recovery journal/evidence; use a capable trusted runtime to inspect and
complete real work, then retry. Recovery never restores an incapable reader.

The quiescence selection is the same exact writer assertion in preview and
apply; preview/apply is a separate controller mode. A preview does not publish a
drain proof or clear the actual capability marker, so common compatibility can
still refuse removal of the last reader even when the drain phase is planned.
An unchanged or changed managed/signed update verifies the native extension's
receipt-owned bytes and mode on disk. Public dispatch readback executes only
the saved bin directory's `am` and `agent-memory`, never the non-executable
TypeScript extension.


A successful drain leaves immutable completed snapshots intact, writes a
private proof bound to the exact repository/private-state inventory and
verified completion state, durably removes the marker and reads it back.
Markerless retained snapshots are accepted **only with that bound proof**,
not by pretending the inventory is empty. Manually deleting the marker, an
index entry, a gap or a proof is not recovery. New source evidence recreates
the marker before shared visibility. Compatible readers remain required for
unresolved permanent gaps/faults. Local discovery cannot fence manually invoked
old binaries, undiscovered banks, remote private state or already-running
omp-unaware peers; mixed-reader use on one protected index is unsupported.

## Runnable real-installed minimal smoke

Prepare a disposable existing bank/clone through the normal identity/init
skill. Do not point a smoke at production project memory. Install this exact
candidate into an explicit isolated native root, preserving real provider access
and normal native permissions. The following driver verifies that the exact
selected binary reports exactly **`omp/18.16.0` before launching RPC or using a provider/model**.
It discovers the actual installed binding, uses two real RPC human turns and
the real model/tools, checks original answers once, and requires both exact
submitted texts and their distinct stored native human IDs/roots in the
current run's persistent session and source-bound observer evidence. Completed
observations count only after the installed public source verifier checks their
enrollment and stored digests. A historical `omp-source-v1` directory, a
`status` label or a manual shared registration cannot qualify a new session.
The report retains source/binary/receipt identity and current human observation
references without copying arbitrary display payloads:

```sh
python3 scripts/evaluations/omp_native_smoke.py --binary /absolute/omp --am /absolute/owned/bin/am --receipt /absolute/install/receipt.json --repo /absolute/disposable/project --output /absolute/new/private-smoke-output --model actual-provider/actual-model
```

The driver never auto-approves. Headless UI requests are refused and preserved
as such. A smoke pass is not a durable-capture/Git-exchange, full-mode,
platform-wide, peer-client or release acceptance pass. Its report explicitly
keeps `full_native_qualification:false`.

## Full independent native release matrix

Retain immutable source/artifact/binary/provider/platform pins and actual
readback for **each** cell below. Synthetic transport tests exercise regressions
only; CLI-only or mocked harness runs cannot fill native rows.

| Group | Required real-native journeys and negative controls |
| --- | --- |
| Platforms/modes | Supported macOS and Linux; persistent TUI, print and RPC individually; disabled discovery and ephemeral recovery limits explicitly separated |
| Learning | Initial qualified recall; model-forgets automatic capture; search/read-before-write duplicate/distinct candidates; governed durable admission; no-learning plus separate refresh; real memory-only Git exchange and applicability-qualified peer/clone readback |
| Sources/privacy | Stored assistant placeholders versus deobfuscated display; native plaintext user/tool screening; file mentions/images/artifact gaps; developer/custom/autolearn/reminders; subagents/advisors; >128-message segment; actual native IDs and absent/unverified mapping; fork/resume/navigation/rewind/compaction |
| Batch/wake | Joined own+foreign stop-hook text; same-batch new human+queued guidance after skipped/failed/deferred hidden turns; own+foreign hidden nextTurn; unknown preparation companions; goal/plan/vibe/todo/advisor/before-agent injections; N human roots ≤N requests; one open successor per root; original unsuppressed P carried at H2 and H3; >8,192-byte backlog with explicit cursor; faulted foreign namespace and completion contention |
| Failure/recovery | Native permission refusal and rejected tool; executing-tool cancel; steering during tool with invocation-root ownership; crash/lost helper response; real unavailable/offline Git authority; safe snapshot/gap recovery; successful retry with original observations; concurrent same-bank and independent clone sessions without duplicate publication claims |
| Capacity/idempotence | Full shared pending capacity retains arriving evidence before bounded drain; capacity frees registers original reference once; crash after shared registration reuses actual observation; registration-fault never re-enters register; no source-per-message exhaustion |
| Lifecycle | Fresh/update `all` never enables omp; explicit missing root/attestation refusal; v1/v2 ownership; same-root repeat and changed-environment preservation; modified/colliding targets; duplicate module copies and disposal; actual install/update/status/doctor/uninstall/exact rollback/recovery; markerless completed snapshots with bound drain proof; pending/unknown refuses incapable restore; preserve unrelated files/Git/index/worktrees |
| Mixed features/peers | Both source/layout capabilities active; either resolves first; old runtime refuses until both drain; native activation under layout hold has refreshed independent route attestation; live frontier-only changes affect next observation; retained Codex/Claude/OpenCode boundary regressions |

Inspect source/assessment/refresh/publication and actual native answer/input
preservation independently, retain failures/unsupported cases, and obtain
independent source-bound implementation and qualification acceptance before
signed v0.0.15 delivery. Provider access, a disposable Linux environment,
interactive TUI support and qualified peer runtimes are real external proof
prerequisites, never simulated passes.
