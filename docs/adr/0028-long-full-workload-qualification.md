---
id: ADR-0028
status: accepted
approval-scope: v0.0.15 #209 qualification duration only
date: 2026-11-09
amends: [ADR-0009]
related: [ADR-0026]
---

# ADR-0028: Explicit long full-workload qualification exception

## Why

ADR-0009 normally requires individual tests to reasonably complete within
10–15 minutes and bounded, focused release qualification rather than a required
multi-hour end-to-end run. Existing #209 additionally requires the complete
24-session / 10,000-lesson, two-round public-operation workload on macOS and
Linux. Its retained corrected baseline took 76,620.196 seconds; the different,
smaller six-session pilot took 3,087.743 seconds. Neither is a matched final
baseline or proof that the full candidate meets a minutes-long budget.

The operator was offered bounded, state-verified stages or an explicit exception
retaining the full controller. On 2026-11-09 the operator selected **“Allow long
full-workload runs.”** The [durable decision](https://github.com/robertelee78/agent-memory/issues/209#issuecomment-6731154929)
records that new durations are unknown and matched runs may take many hours.
This record amends the duration requirement explicitly rather than silently
reinterpreting a smaller run or one command as full-workload qualification.

## Decision

For existing #209's v0.0.15 performance qualification only, the retained complete
controller and matched baseline/candidate runs MAY exceed the normal
10–15-minute test-duration budget. That exception also permits a long end-to-end
qualification run for this task. It does not change the default rule for other
stories, routine contract suites or later releases.

The operator authorizes the long-run route rather than requiring a new staged
controller merely to fit that budget. Existing source-freeze, comparison,
resource/access, technical-review and independent-acceptance requirements still
apply. This decision is not itself a frozen run manifest or a passing result.

## Requirements retained

- Both required macOS/Linux cells SHALL preserve all 24 sessions, 10,000 durable
  seed lessons, two rounds, four workers, three independent clones, distinct
  worktrees, two actual developer identities and every original semantic and
  recovery control.
- Exact learning, provenance, conditions, feature work, index and HEAD
  preservation SHALL remain required. A small, partial or interrupted run MUST
  NOT stand in for the full workload.
- Matched inputs, source/runtime bindings, actual checked-page consumption,
  declared acquisition-mode differences, monotonic command/setup/drain timings
  and normal busy-host observations SHALL follow ADR-0026. The old full baseline
  and the two small pilots remain qualified historical observations, not an
  automatically reusable comparison denominator.
- The existing 300-second public-command deadline SHALL remain unchanged unless
  separately and explicitly amended. This exception MUST NOT be applied to
  contract-worker deadlines, native callback deadlines or source/capacity bounds.
- Every failed, timed-out and interrupted attempt SHALL remain visible. No
  retry-until-green, skipped control, larger disclosure cap or empty-success
  fallback is authorized.
- No #209 percentage target, statistical-confidence claim or universal
  minutes-long completion claim is created. Actual workload timings and the
  already approved one-build/no-rebuild structural criterion SHALL be reported
  separately. Lack of improvement or a remaining dominant cost returns to review.

## Scope and dependencies

The work key remains
`robertelee78/agent-memory:ADR-0009:bounded-24-session-stress`; no replacement
story, Project or release is created. ADR-0026's bounded exporter architecture
and actual page-consumer safety rules are unchanged. This record does not alter
#206's separately approved numeric targets, authorize a graph/model study,
weaken #244 recovery, or claim stock-OMP qualification.

The default budget remains in ADR-0009 for all unaffected work. This scoped
exception MUST be carried into #209's requirement excerpt and source bindings
before claiming tracker reconciliation complete. Selecting this route is not
implementation acceptance or signed v0.0.15 delivery.

## Acceptance and review boundary

Freeze and technically review the exact complete-workload protocol before its
qualifying execution. Retain every observed result and independently accept
functional preservation and performance claims before delivery. Operator policy
acceptance above does not substitute for that technical review, execution or
independent verdict. Existing source and qualification evidence remains
historical; this record does not retroactively change failed outcomes.
