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
The stored-source-v2 cutover requires a renewed explicit attestation for the
new capability and artifact; an older saved rollout approval is insufficient.
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
The compiled native binary does not resolve the SDK as an external npm package:
the adapter uses erased type-only imports and the injected `pi.pi.VERSION`
runtime export. The fixed observer launcher also initializes its own local
source import path under isolated Python; public-console tests alone do not
exercise that callback route. The installed long-segment regression drives the
actual `/bin/sh .../python3.sh .../omp_boundary.py` route before inspecting and
assessing retained sources through the public command.
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

`message_end` and `tool_call` are cheap neutral metadata hints, never body,
role/attribution or reported tool-ID source ownership. Their closed identity is
`{kind:'hint',session,header_id,cwd,leaf_id}`; the worker records it without
calling source observe or acquiring the transport lock. Null leaf is neutral.
A nonnull leaf needs an exact native-ID disposition or a verified full
same-session source attempt; role/order/count cannot fill missing evidence.
One trailing macrotask coalesces FULL stored sweeps at minimum one-second
spacing. Every lifecycle/agent start/end/stop/shutdown and pre-compaction
boundary still captures a FULL checked stored view immediately. The last FULL
view, never a hint, anchors OLD transition coverage. Shared unsafe screening
remains unchanged, and unknown secrets or unavailable payloads remain limits.

For the supported omp 18.16 stored-source renderer, a fresh full `read` of
`skill://agent-memory` or `skill://agent-memory/<relative>/GUIDE.md` can be
recognized as installed operating context only when its unique invocation/result
pair and complete rendered content match the internally verified receipt-owned
instruction file. A guide MUST be a canonical artifact-declared domain, operation
or model instruction; filename shape alone is insufficient. The selected bytes,
mode, installation binding and whole target are checked again before publication.
The native result and model input are unchanged. The learning snapshot retains
the result identity and an empty content list with an `instruction_context`
annotation, not the manual body or duplicated display content. Partial reads,
aliases, traversal, foreign or modified instructions, unsupported wrappers and
other tool results follow ordinary screening and gap policy; this is not a
general tool-result exemption.

The annotation records the installation ID, observed receipt and artifact
digests, selected instruction-file digest and canonical relative path, fixed
renderer, and invocation and tool-call identities. It is an observer assertion of a comparison at
capture time, not publisher authentication, an instruction-authority grant,
or proof of current installation integrity. Historical `--source` readback
validates the immutable metadata-only snapshot, not the continued presence of
the old installed file. Existing snapshots, sealed manifests and gaps are not
rewritten or waived. An interrupted publication reuses its already-published
snapshot; a later fresh read must qualify against the then-current installation.

This classification adds no observation or wake by itself. It does not change
the empty own-maintenance companion allowlist or pure-batch rules: a manual read
after a wake can still leave an ambiguous interval requiring enrollment and
assessment. A manual classification cannot clear an older native/tool gap or
authorize historical role/position event-gap reassociation.

Fresh copy/link observer smokes reproduce the owned admission-guide coverage gap
before the GUIDE extension and retain body-free metadata without native/tool
gaps afterward. They use the public guide and exact measured renderer, not a
replay or repair of the original native World. These results do not attribute
the separate b85 `unknown-after-start` outcome or pass full native qualification.
The final stock-omp campaign MUST exercise an actual owned GUIDE read.

Private snapshots, manifests, unresolved gaps and enrollment records remain
under owner-only `omp-source-v1`. Neutral hints, counted attempts/outcomes,
finite-worker OPEN/CLOSED records and transition qualifications live in the
**sibling** `omp-stream-v1`, never new child kinds under the historical source
tree. Both remain on uninstall, without automatic purge/global retention quota.
Per-source/frame input is bounded at 16 MiB, per-segment captures at 16,384,
active tools at 1,024 and cursors at 1 MiB. Each side retains at most 64 MiB and
4,096 encoded source frames; that is not a decoded-tree/RSS or global storage
bound. One detached finite worker executes serially and drains on EOF; it is
not a daemon or source-completion ledger. Bounds/refusals/unknown outcomes stay
explicit pending, never truncation-as-assessment.

Each substantive segment has at most one admission source and one sealed
manifest in the shared ledger, not one per assistant/tool message. Late
substantive/foreign work opens a linked successor with the original human root;
completed sources are never reopened. Genuine new human steering owns new
assistant work, while a late tool result retains its invoking lifecycle/root.
A fork has its own namespace; inherited context is not a fresh human admission.
Resume keeps existing source identities and snapshots. Native mapping is a
qualification, not a requirement that the host transcript survive forever.
Fork inheritance uses normalized timestamps from the actual stored header and
entries, never notification time. On cold resume, post-fork genuine work is
not blanket-inherited. Missing fork-point evidence retains available content
as nonhuman context with an explicit ambiguity gap instead of dropping it.

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

The total stop-callback bound remains **950 ms**, earlier on cancellation;
helpers are never killed during shared writes. Queue ONE source-first armed
seal (`request_wake: true`, fresh nonce, `terminal: false`) before CLOCK and
CLOCK-ACK. No prewarm or second seal is allowed. The single source outcome
commits before wake preparation; missing ACK cannot prevent source retention.
The checkpoint freshly rechecks scope/compatibility and the actual held
source/transport descriptors. The late window must match this attempt's
current nonce, with its own successful reservation transition.
Bun hrtime and Python monotonic have different epochs. Four timestamps bound
their offset; use the conservative lower bound and subtract platform rate
uncertainty before the pre-reservation check. The provisional 100 ms margin
must cover measured post-reservation p99. Expired/cancelled/EOF/broken-output
windows reserve nothing new. A timely reservation returned late remains one
unknown attempt; late results cannot change the active request/lifecycle or
deliver guidance. Rate, host-loop and warm-latency support remain unqualified
until the required platform measurements. Source capture may finish after
the callback; that is not delivered-wake proof.

Finite nested guards may reuse only the executor's exact live source descriptor
as a locator, never a cached enrollment or canonical authority. All fresh
scope entry/exit checks remain. Qualify released/foreign-thread/transport/stream/
live observations, borrowed entry and exit inode swaps, same-root bank changes
with real fresh-mint preconditions, canonical-writer busy-first behavior, and
activated second-root retargets with an unreadable native-binding canary and a
real exclusive source-lock holder. Wrong-bank refusal MUST precede native
binding reads, STARTED, retirement and private-state creation. Its read-only
scope mint may inspect that bank's compatibility and writer/hold state first.
Final close outside the executor and post-response registration retain their
own fresh guards. Local installed-pipe success is not actual stock-native,
clock/rate, platform-matrix or uncensored p99 qualification.

On shutdown, worker creation is disabled, the trailing hint timer is cancelled,
and the adapter awaits actual stdin finish for at most 1.8 seconds within the
native two-second cap. It does not wait for observer transactions or kill them.
A timed-out or partial transfer remains OPEN; normal print needs actual
greater-than-pipe-buffer flush/CLOSE qualification, not a synchronous end() call.
Refusal, unavailable authority, continuation caps, cancellation and model
omission remain pending, never a synthetic human or blocking decision.
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
reference and do not invent one. Ordinary ingest/seal registration retains its
existing guarded path. An armed finite-worker source frame instead commits its
source outcome and attempts its response before registration progress. After
the observer and transport guards unwind, a separate fresh-guard phase selects
the oldest retained fault-free enrollments and attempts at most eight, using
their original references and digests. It also runs after a committed-source
busy retry or ordinary output loss, without replaying source or wake, before
the next FIFO request or FINAL. The bound is a count, not an elapsed-time check.
Already registered and faulted records do not consume attempt slots. Capacity
can leave enrollment pending; a later phase resumes without dropping arrivals.
A successful registration whose response was lost is recovered from its exact
shared reference/digest. A shared fault or uncertain attempted write is retained
and never blindly registered again or erased by transport retry.

The migrated `NativeSources` contracts in `tests/contracts/test_omp_boundary.py`
and dedicated `tests/contracts/test_omp_enrollment_progress.py` regressions cover
large retained segments, bounded oldest-first progress, new arrivals, lost
success, uncertain attempts and retained shared faults. The latter also drives
an explicitly installed finite-stream/source/no-learning journey with 151
stored entries. The removed public `--observer --stdin` write route must refuse
before source writes. These are real transport/CLI checks, not native callback,
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

After the ownership receipt commits, lifecycle activation durably writes
`turn-sources-compat-v1.json` declaring `omp-stored-source-v2`, the protected
`omp-bank-binding-v1.json` and registry pin. Renewed explicit peer rollout is
required for this exact artifact; observers only verify those existing records.
If postcommit activation cannot finish, installation returns verified:false
with committed receipt/pending activation retained. Use ordinary `am recover`;
do not roll back or delete that committed receipt to hide the interlock.
An old marker is REFUSED until explicit lifecycle recovery upgrades it.
After both source and sibling coverage inventories drain, exact rollback
remains possible with bound proofs; the upgrade is not permanently one-way.
Only stock omp is supported: no native patch, private API or changed approval
policy is an accepted substitute for these checks.
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
index entry, a gap or a proof is not recovery. New stored-source activation
must reestablish the marker through the committed lifecycle route before writes.
Historical and sibling stream drain proofs remain separately bound.
Compatible readers remain required for
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
The native session may begin with title metadata; qualification requires one
matching session header after that prologue, not a header at physical line one.
Only fixed AM diagnostic codes are retained from native UI events; arbitrary
notification/display text remains excluded.
Readback waits for background observer publication within one shared absolute
deadline (default 240 seconds), distinct from the 950 ms callback-await bound.
Known verified pending/null-observation states remain pending; malformed or
misbound source evidence still refuses. A detached finite worker may remain a
live child throughout the session: child-process quiescence is not required.
Instead the driver enumerates the complete joined public pending inventory and
checks actual per-instance contiguous received attempts and terminal outcomes.
A verified full seal must cover the current stored native leaf. The pinned 18.16
public selector semantics honor explicit `activeLeafId` (including null), otherwise
an entry selects itself; an unavailable selected ID falls back to the newest
self-selecting entry. Physically later usage/background entries are not assumed
to be the active leaf. OPEN repository identity compares canonical paths, including
the macOS `/tmp` alias. Unresolved execution/coverage qualifiers remain pending
(an otherwise healthy open stream is expected). These receipts prove received
work, not an unseen producer queue.
It rechecks the latest native settled state and both answer counts after late
frames; an earlier settled response cannot qualify a later duplicate or unsettled session.
The report retains source/binary/receipt identity and current human observation
references without copying arbitrary display payloads.

The actual native run must exercise the loaded receipt-owned hook's `/bin/sh hooks/python3.sh scripts/omp_boundary.py --stream` route. The report records that receipt-owned route and actual stream receipts, without claiming a separate process-argv trace. A public `am omp-boundary --stream` fixture is an internal-transport check only; raw framed source input grants no assessment/publication authority and is not native-loading proof.

```sh
python3 scripts/evaluations/omp_native_smoke.py --binary /absolute/omp --am /absolute/owned/bin/am --receipt /absolute/install/receipt.json --repo /absolute/disposable/project --output /absolute/new/private-smoke-output --model actual-provider/actual-model
```

The driver never auto-approves. Headless UI requests are refused and preserved
as such. A smoke pass is not a durable-capture/Git-exchange, full-mode,
platform-wide, peer-client or release acceptance pass. Its report explicitly
keeps `full_native_qualification:false`.

The pre-cutover macOS source `90d822b…`, Linux RPC runs and `bf7722a` worker receipts below are retained historical evidence, explicitly **superseded for the current stored-source-v2/finite-stream candidate**. They are not current-source passes; fresh frozen-source qualification is required.

The [2026-11-06 minimal macOS RPC evidence](../research/data/2026-11-06-omp-native/minimal-smoke.json)
records an actual pass on source `90d822b42e41b96b27682ebca2bcf405c6a26bfc`,
installed artifact `c8c4a03280d7c34e2eb975dfc4157a8a5999409790a0973a5d020ea3166a55b5`,
the fixed 18.16.0 binary and `openai-codex/gpt-6.1-sol`. Both current human
entries mapped to distinct source-bound observations, each answer occurred
once, and the latest native state settled. Observer wait was 116.08 seconds;
the callback's pending warning and unresolved coverage/tool rows remain
preserved. Source completeness, assessment, refresh and publication were **not**
qualified. The synthetic late-frame regression separately passed its normal,
late-duplicate and late-unsettled subcases.

The separate [Linux offline startup receipts](../research/data/2026-11-06-omp-native/linux-startup.json)
bind the actual upstream arm64 binary and pinned Python 3.11.18/3.15.2
bookworm carriers. Python, Git, shell, `omp --version` and `--help` each exited 0
with networking disabled, read-only roots, nonroot identity and dropped
capabilities; all ten owned containers were absent afterward. No AM
installation/extension, model, provider or session ran. These checks establish
available executable prerequisites, **not Linux native integration support**.

The separate [Linux minimal RPC evidence](../research/data/2026-11-06-omp-native/linux-minimal-rpc.json)
records actual copy-installed, receipt-bound two-human RPC smokes on those two
Linux arm64 VM-container carriers, using the same selected model and normal
native approval mode. Both drivers reported distinct verified human/source
mappings, one answer per submitted turn and settled native state; retained
pending warnings and coverage/tool rows are not completion. Public source
verifier responses and private World file inventories agreed before and after
each scoped evidence copy. Transient unknown-role processes exited before two
empty samples and the copy; those samples are not a global writer fence or
drain proof. The original conservative Python 3.11 override and the later
parent scope clarification remain separate, hash-bound records.

The source was a Git-tree-verified, mode-normalized archive export, not a Git
checkout: the install receipt correctly reports an unversioned development
source. The evidence binds that export and the separately inspected artifact
to the named commit without claiming reproducible source/binary equivalence.
These minimal RPC observations do not establish source completeness,
assessment, refresh, publication, peer exchange, TUI/print support, full
platform support or release acceptance.

The [registered-worker receipts](../research/data/2026-11-06-omp-native/registered-worker-batches.json)
retain all 26 unchanged native transport/lifecycle/smoke-driver-control methods
in seven disjoint, exhaustive batches at `bf7722a`. Every actual worker passed
within the unchanged 300-second deadline, with no skips or removed subcases.
The original unsplit timeout remains a failed attempt. This qualifies the
worker grouping, not the real provider/platform/peer matrix below.

A crash after an ordinary pure-control snapshot but before its cursor record
does not establish the old batch's nonlearning classification. Later substantive
work MUST NOT add a capture to that sealed parent or erase the historical orphan.
The successor retains the available control/work evidence; the historical
unqualified snapshot remains an explicit inventory fault, not silently complete.

## Full independent native release matrix

Retain immutable source/artifact/binary/provider/platform pins and actual
readback for **each** cell below. Synthetic transport tests exercise regressions
only; CLI-only or mocked harness runs cannot fill native rows.

| Group | Required real-native journeys and negative controls |
| --- | --- |
| Platforms/modes | Supported macOS and Linux; persistent TUI, print and RPC individually; disabled discovery and ephemeral recovery limits explicitly separated |
| Foreground/control timing | Actual first-turn receipt-owned omp activation even when the runtime advisory is absent; task-relevant recall/refresh before answering; substantive answer once before automatic maintenance; confirming a decision is not an explicit memory request; the governed continuation performs and assesses its work without deferring again; absent/refused control stays pending rather than moving maintenance to the next foreground turn; explicit memory requests and existing Codex/Claude/OpenCode timing preserved |
| Learning | Initial qualified recall; model-forgets automatic capture; search/read-before-write duplicate/distinct candidates; governed durable admission; no-learning plus separate refresh; real memory-only Git exchange and applicability-qualified peer/clone readback |
| Sources/privacy | Stored assistant placeholders versus deobfuscated display; native plaintext user/tool screening; file mentions/images/artifact gaps; developer/custom/autolearn/reminders; subagents/advisors; >128-message segment; actual native IDs and absent/unverified mapping; fork/resume/navigation/rewind/compaction |
| Batch/wake | Joined own+foreign stop-hook text; source-bound failed-path SAME-batch real human+own queued guidance positive, skipped/lost guidance pending-and-recovery negative, and deferred/unsupported source inference plus real pending-and-recovery and separate foreign nextTurn control, as detailed immediately below; own+foreign hidden nextTurn; unknown preparation companions; goal/plan/vibe/todo/advisor/before-agent injections; N human roots ≤N requests; one open successor per root; original unsuppressed P carried at H2 and H3; >8,192-byte backlog with explicit cursor; faulted foreign namespace and completion contention |
| Failure/recovery | Native permission refusal and rejected tool; executing-tool cancel; steering during tool with invocation-root ownership; crash/lost helper response; real unavailable/offline Git authority; safe snapshot/gap recovery; successful retry with original observations; concurrent same-bank and independent clone sessions without duplicate publication claims |
| Capacity/idempotence | Full shared pending capacity retains arriving evidence before bounded drain; capacity frees registers original reference once; crash after shared registration reuses actual observation; registration-fault never re-enters register; no source-per-message exhaustion |
| Lifecycle | Fresh/update `all` never enables omp; explicit missing root/attestation refusal; v1/v2 ownership; same-root repeat and changed-environment preservation; modified/colliding targets; duplicate module copies and disposal; actual install/update/status/doctor/uninstall/exact rollback/recovery; markerless completed snapshots with bound drain proof; pending/unknown refuses incapable restore; preserve unrelated files/Git/index/worktrees |
| Mixed features/peers | Both source/layout capabilities active; either resolves first; old runtime refuses until both drain; native activation under layout hold has refreshed independent route attestation; live frontier-only changes affect next observation; retained Codex/Claude/OpenCode boundary regressions |
| Stored-source API/transport | Actual public parser/leaf-selector exports and closed result shapes; strict malformed/truncated/multiheader/session/cwd/version controls; stable exact old-file read after writer drain; fork/activeLeafId/null and invalid graph controls; delayed old hint cannot create a new-session human; no positional gap waiver; source-first short print drains; normal cloned hints remain assessable; T/unsafe active-tail negatives; response pressure and expired/cancelled/lost-window behavior; warm and post-reservation p99 on both platforms |
| Wake-decision diagnostics | Real offered/refused/unknown/not-armed and armed-without-nonce seals; fixed codes bound to actual immutable OPEN/attempt/outcome and matching root-wake; retain only after the writer queues the original unsubstituted OBSERVED frame, not proof of write/receipt; refused/substituted frames remain unrecorded; later output loss is not delivery; spoofed/conflicting/unreadable diagnostics never rewrite evidence, change health or authenticate an unretained seal-to-root cause; at most64 summary rows; source/stream/close/drain/legacy-reader state unchanged |

The queued-guidance cells use the exact definitions, pinned stock-source branches,
positive and negative evidence, and pending rules in the
[design](../design/v0.0.15-omp-integration.md#diagnostics-dependencies-and-acceptance).
Record per platform and exercised native mode: observed stimulus, actual event
and stored-source/parent-chain readback, independent evidence of a shared batch
where required, source/root/wake identities and status, later original-identity
recovery, and source-inferred or unknown limitations separately. A positive
failed-path cell requires both the pre-dispatch failure prerequisite and an
independently witnessed same batch; neither adjacency, event timing nor a
`requested` wake alone proves either. An unapproved or unavailable safe
credential/model stimulus, missing executable/artifact pin, missing required batch witness,
or unexercised cell remains pending. A skipped/lost result must not be called
guidance delivery, and deferred own guidance must not be fabricated using the
foreign control. Preserve actual failed attempts. Source review or supported-case
inference does not replace independently reviewed real native qualification.
The entire remaining table and original platform/mode and peer requirements are
unchanged; the callback's 950 ms deadline, 30 s contention limit and 300 s
suite deadline remain unchanged. No increase, permission bypass, hidden retries,
modified stock harness or reduction of the existing workloads is authorized.


The retired-file reader uses only the exact file from a prior checked native
view, public pure `parseSessionContent`/`resolveSessionLeafId`, strict UTF-8,
native header/version/anchors and owned no-follow descriptor checks. NEW capture
starts independently; OLD recovery precedes its FINAL. It never materializes a
lazy file, constructs SessionManager/BlobStore, discovers transcript paths or
spools/hashes raw bodies. Its provisional 16 MiB raw cap needs measured host-loop
qualification. Known snapshots are reused by native ID, not persisted-payload
digest. The version-qualified persistence truncation suffix becomes an
oversized-source gap in **all** views, including a resumed SessionManager;
blob/image references remain unavailable. User-owned ancestor symlinks outside
the qualified root-owned system aliases retain T; no path-policy relaxation or
native installation change is inferred.

Unavailable final OLD views retain non-actionable T without invalidating safe
prefix assessment. Empty/unknown/pre-ID tails cannot be cleared by absence.
All-nonlearning T recovery needs a nonempty anchored, complete committed view,
not invented observations. Normal notification cloning alone never creates T.


Inspect source/assessment/refresh/publication and actual native answer/input
preservation independently, retain failures/unsupported cases, and obtain
independent source-bound implementation and qualification acceptance before
signed v0.0.15 delivery. Provider access, a disposable Linux environment,
interactive TUI support and qualified peer runtimes are real external proof
prerequisites, never simulated passes.
