---
id: ADR-0017
status: proposed
date: 2026-10-25
depends-on: [ADR-0004, ADR-0005, ADR-0014]
---

# ADR-0017: Standalone semantic terminal search after v0.0.3

## Context and delivery boundary

The operator explicitly permits v0.0.3 to deliver semantic recall through the
portable skill and CLI inside Claude, Codex, OpenCode and custom harnesses.
Plain terminal semantic search remains requested and is deferred beyond v0.0.3.
This proposal records that follow-up rather than claiming the current literal
`am search` interprets meaning. It does not authorize an implementation design.
The specific later release remains to be selected after refinement.

## Proposed outcome

A later release SHALL provide useful semantic recall from a plain terminal query
without requiring an already active coding-agent session. The command SHALL
truthfully distinguish semantic interpretation from literal matching, retain
qualified source identities/conditions/evidence, and keep all current project,
trust, validity, visibility and applicability gates before returning guidance.

The implementation SHALL reuse the portable CLI and routed Markdown bank. It
MUST NOT introduce personal memory, cross-repository sharing, MCP or a required
vector database. An optional interpreter/provider and its configuration,
availability, privacy, cost and failure behavior SHALL be designed explicitly;
this proposal does not select a provider or silently send repository data to one.

## Acceptance and qualification

- Plain-language paraphrases with no required literal-word overlap SHALL locate
  the relevant qualified lessons in independently labeled examples.
- Missing conditions, stale/corrected/deleted guidance, unsafe sources and
  contradictory claims SHALL retain the existing selection and clarification
  boundaries; semantic interpretation MUST NOT bypass them.
- Unavailable interpretation SHALL give an actionable outcome and MUST NOT be
  presented as a successful semantic no-match. Any literal fallback SHALL be
  explicit.
- Evaluation SHALL retain query quality, native and interpreter latency, context
  disclosure and measured cost separately at agreed bank sizes. Numeric targets
  SHALL be agreed before a performance pass is claimed.
- The later release SHALL contain the independently accepted implementation and
  public command documentation before this follow-up is called delivered.

## Open design questions

Interpreter/provider choice, use of already installed harness CLIs, command
syntax/defaults, credentials/configuration, invocation/session reuse and exact
release assignment remain open. The current v0.0.3 harness workflow does not wait
for those decisions. A design review SHALL settle them before implementation.
