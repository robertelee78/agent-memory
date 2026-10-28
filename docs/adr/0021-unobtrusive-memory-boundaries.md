---
id: ADR-0021
status: accepted
date: 2026-10-27
amended: 2026-10-28
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
answer record, remain silent on routine success and MUST NOT repeat the business
answer. This timing exception avoids inventing a new pre-final source identity
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
memory narration. Required source recovery, genuine admission/publication and
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

### Exact OpenCode control termination

Native PM qualification showed that asking a model to emit an empty final
response is insufficient: it can narrate completion or repeatedly call a null
operation. The adapter MUST NOT rely on that instruction as its terminal
mechanism and MUST NOT interrupt the whole session to stop one control pass.
The public interrupt API is session-scoped and could cancel newer operator work.

After an actual same-source combined check verifies both refresh and assessment,
the adapter MAY retain a replaceable in-process tuple of native session ID,
original source ID, retained synthetic control ID, acknowledging assistant ID
and operator-admission epoch. Only a subsequent model context for that exact
acknowledged control, with its native tail and epoch still matching and no later
operator/steering input, MAY remove tools from that outgoing request and request
a unique terminal sentinel through the documented request-local stop sequence.
When the qualified model emits the sentinel first and the provider honors the
stop setting, it omits that sentinel and completes the control without visible
text. This is conditional behavior, not a guarantee from instruction alone. This is a terminal
transport mechanism, never an authored assessment or capture receipt.

A failed/pending refresh, failed assessment, source recovery, cross-session
operation, missing native evidence or a changed tail/epoch MUST NOT arm this
transition. Persisted assessment-only completion MUST NOT arm it either.
Request shaping MUST apply only to the exact already-acknowledged control;
it MUST NOT interrupt an execution, remove queued input, alter genuine operator
turns or suppress a required failure notice. A newly admitted operator input
invalidates the cached tuple. If input arrives after a validated context snapshot,
the shaping affects only that old outgoing request, not the later input.
Restarting the plugin discards the tuple; it MUST NOT reconstruct terminal
success from an assessment receipt alone.

Tool removal MAY be enabled only for a provider/model whose actual native
qualification accepts that outgoing request with its existing tool history. A
documented request-local tool-choice-none setting SHOULD be preferred when the
host exposes one while retaining definitions. An undocumented setting MUST NOT
be invented. Unqualified providers retain the existing bounded control behavior
and an explicit compatibility limitation, rather than a false quiet-success claim.
A provider rejection or visible error is failed qualification.

The native model/provider must honor the documented stop setting and emit the
sentinel before any visible text. Nonempty pre-sentinel output or completion
without the sentinel is failed qualification, not quiet success. Qualification
MUST retain actual native completion, tool counts, original answer record and
SDK final-row attribution. An empty control row is not a replacement business
answer; host attribution limits MUST remain explicit. Repeated null calls or a
timeout remain failed qualification, never quiet success.

### T3: Operator-visible acceptance and corrective delivery

Acceptance MUST include an actual multi-turn PM conversation in Codex, Claude and
OpenCode, with a decision, a follow-up question, an unchanged bank and an injected
refresh failure. It MUST count routine commands/workspaces and repeated business
answers/questions, check truthful stage outcomes, and retain pending sources.
Native UI and machine-result behavior MUST be reported separately. Broad repeated remote CI is excluded.

Acceptance thresholds are routine assessment calls <=1, request workspaces=0,
repeated business answers/questions=0; an injected refresh failure produces one
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
