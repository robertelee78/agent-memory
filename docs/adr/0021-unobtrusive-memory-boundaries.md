---
id: ADR-0021
status: proposed
date: 2026-10-27
amends: [ADR-0018, ADR-0020]
---

# ADR-0021: Unobtrusive boundaries and truthful operation health

## Context

The [operator-incident RCA](../research/2026-10-27-intrusive-boundary-rca.md)
identifies an actual Linux path-policy inconsistency, generic diagnostics,
unnecessary workspace calls and repeated task dialogue. v0.0.8's successful
source coverage did not establish acceptable operator-visible normal use.
This proposed corrective source does not reopen or alter its shipped tasks.

The product choice between one compact routine command and zero visible memory
tool calls is pending. This document is not approved for execution or release.

## Proposed requirements

### T1: Consistent state trust and specific refresh diagnostics

Quick refresh MUST use the existing private-ancestor protection policy consistently
with the trusted path helpers, retaining descriptor-based no-follow observation,
ownership, unsafe unprotected-path rejection and rebinding checks. It MUST NOT
chmod existing directories merely to pass. Specific safe failure codes and next
steps MUST survive to the CLI; a setup-only doctor result MUST NOT claim that an
unexercised refresh operation succeeded.

Tests MUST reproduce protected 0775 intermediate directories below a trusted 0700
barrier, and MUST preserve refusal for unprotected writable paths, foreign owners,
symlinks, changed source/context and directory rebinding. No canonical bank,
feature code, index or branch changes are permitted from the quick check.

### T2: Quiet routine operation and distinct completion stages

Routine no-learning handling MUST avoid private workspace creation and use a
bounded direct assessment route. Supported quiet success MUST suppress result
payloads while retaining useful failure output. Instructions MUST put the routine
route first and avoid unnecessary rereading of unchanged loaded guides.

Refresh, assessment, admission and publication MUST retain distinct states.
A verified source assessment MUST NOT imply freshness, capture or publication.
Failed refresh MUST remain explicit and actionable without displacing the task
answer or inducing repeated repair attempts on every dialogue turn.

PM answers and questions MUST NOT be repeated solely as memory bookkeeping.
The implementation MUST document the actual native limits of post-answer
continuations and SDK final-result preservation. It MUST NOT silently remove
mandatory capture after forgotten invocation, invent no-learning receipts or
promise tool-card suppression through an unsupported host field.

The routine command budget and fallback policy remain pending the operator's
visibility choice. CLI stdout silence alone is not evidence of invisible UI.

### T3: Operator-visible acceptance and corrective delivery

Acceptance MUST include an actual multi-turn PM conversation in Codex, Claude and
OpenCode, with a decision, a follow-up question, an unchanged bank and an injected
refresh failure. It MUST count routine commands/workspaces and repeated business
answers/questions, check truthful stage outcomes, and retain pending sources.
Native UI and machine-result behavior MUST be reported separately. Each command
MUST fit the operator's 10–15 minute maximum; broad repeated remote CI is excluded.

The signed corrective release MUST retain actual public installation/update
checks and consumer-memory isolation. No milestone or release is claimed by this
proposal. Design review, AWA decomposition and product alignment precede readiness.
