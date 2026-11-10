# Bank availability and owned-hook health: secondary RCA

Status: research and proposed design only. No approval, implementation, release
assignment or actual bank repair follows from this document. The three outcomes
below are independent of the bounded v0.0.13 installer work.

## Evidence and limits

The operator reported `activeContext.md` at 79,054 bytes and `handoff-prompt.md`
at 28,478 bytes in `/opt/c2e-worktrees/shared-auth-pm-decisions`. This investigation
could not locate that checkout on the Mac or the available Linux host (Beast), in their
matching registered worktrees, or in their local project-registry metadata. This
means the **affected checkout was unavailable for inspection**, not that the
repository never existed or its memory was absent. Its actual router, relative
handoff path, original invoking runtime and source producer remain unverified.

Both available hosts reported signed v0.0.12 source
`38f3e424471449fdd8a7caed16e81606e17beda2`. The controlled reproduction uses a clean
Git archive of that published source. Its five relevant bank modules match the
available installed source. Those observations do not identify the runtime that
produced the original report.

The original terminology, definition, source observation and attempted admission
were not available. No actual terminology was captured, inferred, corrected or
reported missing. A mechanical availability refusal cannot establish whether a
real agent attempted admission, whether the rule was durable, whether an equivalent
already existed, or whether earlier memory was lost.

## Controlled public reproduction

The [result extract](data/2026-10-31-secondary-memory-rca/controlled-results.json)
retains all 22 operation outcomes and source hashes. The
[static reproducer](data/2026-10-31-secondary-memory-rca/reproduce.py.txt) creates
only disposable homes, registries, Git repositories and synthetic Markdown. It
never loads the C2E pipeline or copies actual memory. The approximate 77,200 and
27,800-byte examples in that original experiment are **synthetic**, distinct from
the subsequently supplied exact operator sizes. The 16,384/16,385 controls prove
the threshold independently.

| Controlled source | Full/scoped inspect | Scoped search | Admission start | `bank validate` |
| --- | --- | --- | --- | --- |
| Native `activeContext.md`, 16,384 bytes | succeeds | succeeds | eligible capture | `valid: true` |
| Same native source, 16,385 bytes | refuses 16 KiB guard | refuses | refuses before admission | **`valid: true`** |
| Native context, 77,200 bytes | refuses 16 KiB guard | not repeated | refuses | **`valid: true`** |
| Literal root `handoff-prompt.md`, 27,800 bytes | refuses 16 KiB guard | not repeated | refuses | rejects native path grammar |
| Literal root handoff file, 100 bytes | rejects native path grammar | not repeated | refuses | rejects native path grammar |

Each operation preserved its fixture's bank bytes, HEAD and original index. The
experiment deliberately changed fixture sizes *between* cases; it did not admit
or publish any candidate. `eligible` is a private capture result, not admitted
canonical learning. No actual bank or harness profile was changed.

To repeat independently, create a new private case directory, retain
`git archive 38f3e424471449fdd8a7caed16e81606e17beda2` there as `source.tar`,
extract it into that case’s `source/`, copy the static reproducer beside
`source/` as `reproduce.py`, and invoke it with Python 3.11+ using `-I -B`. It uses the repository's isolated `World` fixtures and
bounds each public command to 60 seconds. Do not point it at a live checkout.

## Cause 1: format validation and operational eligibility differ

In the frozen published source, `bank_state.inspect` calls
`bank_writes.snapshot` before applying `--path`. The snapshot delegates to
`bank_observation.observe`; its `scan` inventories every non-runtime physical
Markdown source and refuses a source beyond 16,384 bytes before ordinary layout
validation. Thus selecting the small `projectbrief.md` does not skip a large
unrelated context. Recall and admission use the same complete-bank prerequisite.
`admission.start` cannot reach its eligible decision while that observation is
unavailable. These refusals preserve data and must not become successful empty
recall or invented no-learning judgments.

The public `bank_contract.validate` takes a different route: it reads complete
Markdown files and calls the pure format/layout validator without the shared
per-source observation bound. The controlled native oversized context therefore
receives `valid: true` from that command while normal inspection and capture
reject it. This is a demonstrated API/contract inconsistency, independent of
whether the original large files were created by a manual editor or another
writer. That original producer is unknown.

Doctor's **bank** check calls `bank_writes.snapshot`; it is not this legacy
format-validator exception. No actual affected-checkout doctor result was
obtained. A literal unsupported root handoff filename has another possible
layout defect, but the real relative path/router are unknown, so that conditional
finding must not be attributed to the reported bank.

Proposed outcome: [ADR-0018 bank-validation amendment](../adr/0018-repair-diagnostics-and-observation-bounds.md#proposed-amendment-consistent-bank-source-eligibility).

## Cause 2: the cooperative fallback lacks an oversized-bank recovery contract

The installed `offline/GUIDE.md` emphasizes a 16 KiB/64-route **router** and
permits a manual single-file `activeContext.md` checkpoint with a reread and diff.
It also describes renderer output plus `bank_contract.py validate` as a manual
file-contract workflow. It does not prominently state the ordinary reader's
16 KiB limit for every canonical context at that entry point. `layout/GUIDE.md`
requires concise contexts and native optional-record formats; the sharing guide
states the complete canonical source limit. These are not permission to import
an arbitrary root handoff file or keep unbounded transcripts in current memory.

The existing failure routes preserve unknown originals and recover known
transactions using their original journals; ordinary Git inspection may support
comparison of one's own file edit. They do not specify a governed layout-repair
entry point when the ordinary complete snapshot itself is invalid. Calling the
same admission, checkpoint, consolidation or sharing operation again does not
remove its prerequisite. Neither a documented manual fallback nor a green format
result establishes semantic admission or safe truncation authority.

The proposed correction is an explicit recovery-specific observation/review path,
not a larger normal cap, a skipped source, an initialization over existing data,
or a direct edit of the reported bank. It must preserve the exact originals and
resolve meaning before producing bounded native sources.

Proposed outcome: [ADR-0010 recovery amendment](../adr/0010-memory-bank-file-contract.md#amendment-source-preserving-layout-recovery).

## Cause 3: runtime doctor and lifecycle verification use different ownership units

This is a separate static code finding in published v0.0.12, not an observed
C2E bank-size effect. `diagnostics.runtime_doctor` verifies an activation target's
managed block, but a `hook-config` target falls through to equality with the
receipt's **whole-file** fingerprint (`diagnostics.py:436–443`). In contrast,
`installation.verify_targets` parses that shared JSON and uses
`client_hooks.locate` to require exactly one receipt-owned prompt entry and any
recorded Stop entry, retaining a final source-stability check. Uninstall also
removes only those owned entries.

The doctor code can consequently reject unrelated shared configuration changes
that the lifecycle ownership verifier accepts. Correcting bank-size validation
does not correct this ownership granularity, and accepting an unrelated hook
entry says nothing about actual hook execution, source assessment, capture or
publication. No real harness configuration was modified for this investigation.
An independent public fixture reproduction is required before implementation
acceptance; the present finding is source-confirmed, not a live-host verdict.

Proposed outcome: [ADR-0007 owned-hook amendment](../adr/0007-portable-pack-and-lifecycle.md#proposed-amendment-entry-scoped-owned-hook-health).

## Reported execution failure: substituting AGENTS.md for capture

Separately, the operator supplied another agent’s retrospective describing an
orchestration decision: its assignment directed a subagent to persist a rule in
`AGENTS.md` if memory tooling failed, it approved that substitution, and it later
acknowledged conflating durable repository instructions with the requested memory
capture. The report says the parent accepted the alternative before completing
diagnosis and initially failed to distinguish observed commands from inferred
capture effects. The subagent reportedly followed the parent’s fallback direction.

This is **operator-supplied reported execution evidence**, not an independently
inspected tool trace. The original rule, command outputs and actual `AGENTS.md`
bytes were not available. This investigation therefore cannot confirm what was
written, which admission/publication stages actually ran, or whether any later
cleanup happened. No actual file was edited or terminology inferred here.

The report identifies a workflow violation separate from the three technical
outcomes. An instruction-file edit is not governed project-memory admission or
publication. Existing completion rules already require a specific pending state
when prerequisites fail; they do not authorize relabelling alternative persistence
as successful capture. No fourth technical feature or task is established by
this report alone. Once the actual source is available, remediation must use the
intended capture/publication workflow and independently review any proposed
removal of substitute `AGENTS.md` content. A retrospective promise to remove it
is not permission to delete unknown current instructions.

## Source binding and follow-up

The frozen source references are:

- `bank_state.py`: `inspect` line 64; selector-specific inspection line 84.
- `bank_writes.py`: `snapshot` line 43.
- `bank_observation.py`: `scan` line 62.
- `admission.py`: `classify` line 65; `start` line 161.
- `bank_contract.py`: `render` line 229; pure format validation line 324;
  filesystem validation line 352.
- `diagnostics.py`: runtime doctor line 356, exact whole-file branch lines
  436–443; bank doctor line 496.
- `installation.py`: `verify_targets` line 819.
- `client_hooks.py`: exact owned-entry `locate` line 45.

Each proposed ADR outcome needs its own immutable design review, scoped task,
public acceptance and delivery evidence. They must not inherit approval or a
release assignment from the v0.0.13 installer work. Retained fixture hashes are
design evidence, not acceptance of a future implementation; its frozen-source
public cases must run again. Actual terminology admission
requires the original rule and source evidence after the actual checkout is
located; this RCA supplies neither.
