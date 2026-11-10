---
id: ADR-0004
status: accepted
approval-scope: v0.0.1; v0.0.4 turn-boundary hooks; v0.0.15 receipt-owned instruction-context and stock-harness source-authority clarifications for task 249
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002]
---

# ADR-0004: Harness-independent skill and CLI

The v0.0.1 scope was accepted in the [operator approval](../design/v0.0.1-approval.md).
The operator subsequently approved v0.0.4 execution of turn-boundary hooks for
Claude Code, Codex, and OpenCode. On 2026-11-07 the operator approved treating
AM's exactly verified installed manual as operating instructions rather than
learning material for task #249; independent Opus review approved the bounded
engineering design. This clarifies the existing hook/skill authority separation,
not a new memory authority or a change to ordinary source screening. Other
later-milestone proposals remain under review. Implementation, native acceptance
and delivery require separate evidence.

On 2026-11-08 the operator clarified that omp is an independently maintained,
frequently updated client: changing it is not an AM integration strategy. The
stock-harness and source-authority constraints below clarify the existing
public-surface and truthful-provenance decision. They do not approve an unfinished
transport design or establish implementation, native qualification or delivery.

## Context

Claude Code, Codex, OpenCode, and custom harnesses need the same memory behavior.
The operator selected the AWA-style portable skill plus CLI interface and later
approved turn-boundary hooks; MCP integration remains excluded.

## Decision

The portable core SHALL be the Memory Bank file contract plus the skill that
operates it. A deterministic local CLI SHALL validate and maintain that
contract. All supported harnesses SHALL use the same files, semantic model,
commands, and verified mutation protocol.

The first release SHALL provide the shared skill and deterministic helpers for
automatic durable capture, search-before-write, consolidation, pruning, and the
memory-only Git exchange in ADR-0012. Turn boundaries SHALL invoke the common
skill/CLI operation. No hook, client plugin, or daemon SHALL be required for
the common workflow; MCP is excluded below.

MCP integration SHALL NOT be implemented in the planned releases. Its previous
proposal is withdrawn by the operator, who selected skill plus CLI as sufficient.

In `v0.0.4`, harness hooks SHALL invoke the common capture/refresh operation
automatically at turn boundaries, including when an agent forgets to invoke the
skill. Hooks SHALL supply invocation only: they MUST NOT own memory semantics,
bypass admission or Git rules, or interrupt unrelated ongoing work. The operator
approved a Codex same-turn continuation that MAY delay that turn's final response
while the memory operation runs; it MUST preserve normal permissions and MUST
leave unsuccessful or unverified work pending. Supported harness
events and integration details SHALL be established and tested through each
selected harness's public invocation surface before claiming support.
Explicit skill/CLI invocation MUST remain available to custom harnesses and
clients without hooks. Skill discovery/activation bindings are distinct from
these optional runtime callbacks.

Failure or absence of the CLI beyond installation MUST NOT prevent a client
from reading the current bank or making the constrained single-file manual
checkpoint described by ADR-0010. That fallback does not provide mediated
admission, authorization, concurrency, audit, or verified-receipt guarantees.
Full governed mutations require the CLI or the same packaged transaction helper.

The CLI MAY grow to cover these capability groups:

```text
agent-memory status|doctor
agent-memory context|search|get|explain
agent-memory capture|remember|checkpoint
agent-memory correct|supersede|revoke|forget
agent-memory project|compact|rebuild|migrate
agent-memory eval
```

Public operations SHALL resolve project identity and developer attribution,
refuse ambiguous project selection, and expose memory IDs, applicability,
validity, provenance, and current/historical status where relevant. Resolution
SHALL follow [ADR-0011](0011-principal-and-project-identity.md).

Installation bindings SHALL preserve one core policy. Client instruction files
SHALL be short activation shims, not independent implementations. Installation
or update MUST preserve unrelated user configuration. Provenance SHALL identify
the actual harness, session, repository, developer, and operation.

The implementation SHOULD support Codex `AGENTS.md`, OpenCode `AGENTS.md` and
`instructions`, and Claude Code `CLAUDE.md`/`AGENTS.md` without requiring those
files to share identical syntax. Generated, sentinel-delimited activation
blocks are preferred to whole-file symlinks. No binding MAY substitute a
client-local repository identifier or credential for the common identity model.

### v0.0.15: stock harnesses and stored-source authority

Task #249 MUST work through unmodified, released omp public interfaces. It MUST
NOT require an omp fork, custom build, private native patch or upstream change.
AM MUST NOT reorder or disable unrelated operator extensions, weaken permissions
or silently change the operator's client installation to qualify its integration.
Supported capabilities and version limits MUST be stated and exercised against
stock clients; compatibility MUST NOT be inferred from a version string alone.

Native notifications SHALL be scheduling hints, not source-ownership authority.
Genuine human origin MUST come from a checked native stored-user entry and its
actual native identity. Notification role, attribution, current callback context,
position or timing MUST NOT invent a human root, assign a foreign source to the
current session or resolve a real source gap.

Missing notification-to-entry mapping MAY remain absent or unverified while an
independently checked, digest-verified safe stored source remains assessable.
Actual unavailable, unsafe, oversized or incompletely transferred source evidence
MUST remain explicitly pending. This distinction MUST NOT waive historical gaps,
rewrite immutable evidence, fabricate completion or relax #249's existing native,
peer-harness, lifecycle, permission and macOS/Linux acceptance. Detailed capture,
transport and recovery changes still require independent engineering review and
actual stock-client qualification.

The task-specific [native delivery-window contract](../design/v0.0.15-omp-integration.md#helper-actions-acknowledgement-and-bounded-wakes)
defines the reviewed Linux same-monotonic profile and atomic reservation-start
transition. Source review does not substitute for the original native matrix.

### v0.0.15: verified installed instructions are operating context

Task #249 SHALL distinguish an exactly verified full native read of AM's own
receipt-owned installed manual from substantive task evidence. The supported
observer SHALL retain a content-free provenance annotation, not the manual body
as learning material. This MUST NOT change native tool results, model input,
permissions, provider/model selection, canonical admission or publication.
Ordinary and unverified tool results MUST retain the existing source-screening
and coverage rules; a caller-supplied flag MUST NOT confer instruction authority.

The observer MUST verify the current installation receipt, exact owned target,
artifact/manual bytes and unique invocation/result linkage against the measured
supported carrier immediately before fresh capture. Copy and link renderers MUST
be measured independently. Extra/mixed, partial, ambiguous, changed, foreign or
unsupported representations MUST NOT receive this classification.

Classification MUST NOT close any pre-existing native-keyed gap or retained
tool-call gap. Immutable snapshots and sealed evidence MUST remain unchanged.
Crash retry MUST complete an already-published representation without selecting a
different representation after installation update or uninstall. Historical
readback SHALL explain the observed provenance and intentionally nonlearning
body without claiming current installation authentication or complete coverage.

The shared source envelopes, exclusions, own-maintenance allowlist, wake budgets,
human/foreign attribution and pending-source gates MUST remain unchanged.
Classification itself SHALL create no new shared observation or wake; existing
control policy MAY still enroll substantive or ambiguous intervals containing a
manual read. The exact carrier, closed annotation, retry and qualification design
is specified in the
[task-specific execution addendum](../design/v0.0.15-omp-instruction-context.md).
Actual copied and linked native reads, adversarial nonqualification, old-gap
preservation, crash retry and historical readback MUST be qualified before
claiming this behavior supported. This clarification MUST NOT narrow #249's
previously approved native, lifecycle, peer-harness or platform acceptance.


## Consequences

- One skill and CLI serve current and future harnesses, including scripts.
- Installation bindings are tested through real client-visible files and public
  commands; memory semantics remain shared.
- Boundary semantics and recovery belong to the skill/CLI; optional hooks may
  supply invocation without creating a second memory implementation.

## Alternatives considered

- **MCP:** excluded by the operator; no separate protocol surface is needed.
- **Hook-owned memory logic:** rejected because semantics would diverge.
  v0.0.3 callbacks invoke the common skill/CLI rather than duplicate it.
- **One implementation per client:** rejected because behavior and state drift.
- **Instruction files only:** rejected because free-form direct edits cannot
  enforce schemas, locking, provenance, or deletion.
