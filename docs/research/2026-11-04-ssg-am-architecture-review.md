# Independent SSG-to-AM architecture review

Date: 2026-11-04. Reviewer: `anthropic/claude-opus-5-5:high` (Claude Opus 5.5), independent of the drafting agent.

The following is the reviewer's final report. Architecture acceptance, task design readiness, implementation acceptance, and delivery remain separate. No runtime tests were performed by the reviewer. References are to the reviewed draft line positions; later metadata/index edits do not change its substantive decision.

# Final verdict: ADR-0017, ADR-0023 and ADR-0024

**Architecture: PASS.** ADR-0017, ADR-0023 and ADR-0024 may be marked accepted as architecture. I read the current drafts and found no remaining blocking findings. Nothing new is requested.

## Fixes confirmed in the text
- **Reference table** (0024:40-41): Qwen is now the G1 reference / candidate managed profile, and production uses the configured compatible profile. llama.cpp is a "reference local serving option, not a mandatory dependency".
- **Provider identity and drift** (0024:126-145):
  - all available provider revision and config evidence is bound to the vectors;
  - opaque aliases show `provider model identity not verifiable` in both machine-readable and rendered status;
  - reused vectors warn that they rely on the alias staying stable, and a full re-embed stays available;
  - any observable change triggers invalidation and a rebuild;
  - G1 studies probe-at-fill, and probes are never presented as proof of identical weights.
- **Prefixes** (0024:147-153): document/query prefixes are versioned configuration and may be empty; the reference profile uses an AM retrieval instruction.
- **Checkout-to-build binding** (0024:209-212): one branch can't serve another branch's build as complete.
- **Bring-your-own graph** (0024:225-228): it must provide a dedicated AM-owned store meeting the isolation, purge and ownership contract, or that profile is refused.
- **Provider retention** (0024:229-231): retention terms are disclosed, and AM never claims provider-side erasure.
- **Automatic maintenance** (0024:239-245):
  - content passes security, quarantine, redaction, visibility, lifecycle and disclosure eligibility before it is projected or sent;
  - it runs within G1-defined rate, batch and cost limits;
  - it never starts services, and stopped services leave updates pending.
- **G1 smoke tests and contract** (0024:325-338): they cover the reference server plus another real compatible endpoint, the API cases, the binding and bring-your-own items, and identity/drift limits.
- **G2 acceptance** (0024:363-379) uses the configured profile and vector store and covers the bring-your-own journeys.
- **Pass scoping** (0024:387-388 and 0017:86): quality and resource passes apply only to measured profiles.
- **Approval channel moved to P2** (0023:109-120): it is a reviewed P2 design gate, and neither G1 nor local recall waits for it.
- **ADR-0017 grant wording** (0017:80): "expansion without a verified operator grant".

## Readiness
- **G1 (v0.0.15 research): Ready to execute.** It is actionable and non-circular. Every input is obtainable without product implementation or federation:
  - the pinned SSG source and reference artifacts;
  - scratch macOS and Linux hosts;
  - the reference model download and a second compatible endpoint;
  - existing labelled suites plus independently authored new ones.

  Operator/counsel licence disposition, frozen targets, and independent review are G1 deliverables, not preconditions. Platforms, endpoints or licences it can't obtain stay explicit prerequisites, never passes.
- **v0.0.16 G3, G2, P1 and #200: design-blocked until G1's reviewed contract lands.** That contract covers:
  - the profile and licence disposition, and the isolation unit and query domain;
  - checkout-to-build binding, the bring-your-own graph contract, and the identity/drift contract;
  - rate, batch and cost limits;
  - frozen schemas, exit codes, suites and targets.

  After that, each task still needs its own independent design approval.
- **P2 and #56:** additionally blocked until the operator schedules and accepts #55/#57. P2 also needs its own reviewed approval-channel design. #62 stays federation-scoped and doesn't gate local delivery.

## When you set the status
The accepted status must cover architecture only. Two suggestions:
- **Frontmatter and index:** mirror the existing `approval-scope:` frontmatter convention (e.g. architecture-only, G1 Ready, .16 tasks gated on G1), and make the ADR index rows say the same.
- **Tracker reconciliation:** the ADR README requires reconciling the managed issues through `awa` after the commit.

I made no edits and no tracker changes, and ran no tests.
