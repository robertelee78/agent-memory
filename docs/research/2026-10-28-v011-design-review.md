# v0.0.11 independent design review

Final design source: `ab0eed6a79b6c2a14f0bcf1b69070fbf587d57d2`.
Actual reviewer: `claude -p --model opus`, no tools, low effort.
T5 is RCA/guidance/proposal only; T6 includes the small new consistency guard.

## Initial BLOCK

# Verdict: BLOCK

The design is close. The attribution, strict-integrity posture, marshal warning and T6 stage separation are mostly sound. Five changes are needed before acceptance. Each is small and none widens scope.

## Necessary changes

### 1. ADR T5 implies a parser was selected

The recovery-proposal sentence says the proposal SHALL define "native Python cache format/version/header/code comparison, bounded non-executing parsing". That reads as a mandated parser strategy. It conflicts with the later text "approves no parser" and with RCA gate 3, which leaves bytewise comparison against regenerated caches as an alternative.

Change it to: "a selected verification strategy (conservative bytewise comparison against regenerated caches, or an independently reviewed isolated non-executing parser), with its bounds".

### 2. Closure of issue #238 is not precise

The ADR keeps the issue identity, but neither document says what closes #238. State one of these explicitly:

- #238 closes on accepted RCA, guidance and proposal, and implementation is a new Backlog item with its own approval gates.
- #238 stays open, with only the T5 deliverables marked complete.

Either way, say that closure does not imply shipped healing.

### 3. RCA evidence retention and provenance

- **Retention.** The private evidence is said to be "retained" under `/tmp/am-v011-cache-producer-*`, which can be cleared. Move it to a durable private location, or record a retention bound. The published hashes must stay verifiable against retained bytes.
- **Linux helper tree.** The "auxiliary fixture-helper tree from the earlier isolated test source" has no hash or revision binding. Record its source revision and tree hash, or state that it did not affect the three controls and why.
- **macOS interpreter.** macOS ran CPython 3.15.0, while the incident was 3.14. Say explicitly that it demonstrates the mechanism and cache-name closure, not the incident interpreter's cache tag or bytes.

### 4. T6 outer/inner envelope integrity

Passing the full prepare response means apply unwraps the inner proposal, while the session reads `changed_memory_paths` from the outer response. Require and test the following:

- Apply derives every pin solely from the inner proof, and outer fields are never trusted for authority.
- An envelope whose outer fields (pins, operation, `changed_memory_paths`) disagree with the inner proposal is refused rather than silently unwrapped.

Without this, a tampered or stale outer field could misreport the delta while apply proceeds.

### 5. Source-bind the T6 RCA claims

The statements that `bank_transactions.proposal_value` accepts the full prepared result, and that output preflight uses the inner pins, need source binding. Cite the revision and `file:line` for each, matching the source-bound standard T5 meets.

## Accepted as-is

- **Producer attribution.** It rests on the transcript invocation, the 44 ms precedence and the full 35-name replay. It explicitly avoids PID claims and AM-launcher blame.
- **Interpreter flag guidance.** It covers `-I` ignoring `PYTHONDONTWRITEBYTECODE`, and `-B` not suppressing reads or authenticating the installation. Importing an unverified package as its own verifier is forbidden.
- **Byte-equality caveat.** The disclosure that marshal reserialization was not byte-identical is correct and should remain.
- **Gated proposal.** It refuses exclusions, unsigned fallback, receipt blessing, chmod and deletion, and it requires quarantine plus normal reverification.
- **T6 stage distinctions.** `refresh_applied` is kept separate from `canonical_writes_performed`. An empty delta does not substitute for apply. Exact-inner compatibility is retained, and artifact-summary and unsafe-output refusals are tested.
- **Scope.** v0.0.12 deferral of T4 and the roadmap is preserved, and the v0.0.9 and v0.0.10 histories are untouched.

## Revised ACCEPT

**ACCEPT**

All five prior BLOCK items are resolved, and the T6 selection is coherent:

1. **T5 parser.** Bytewise-versus-parser is now explicitly an unselected alternative, behind an independent review gate. Parser-only requirements are scoped "only if the parser strategy is selected." In-process `marshal` on unproved bytes is barred, and the ADR states plainly that it approves no parser or repair.
2. **#238 closure.** It closes only on independent acceptance of the RCA, guidance and proposal, plus release delivery. Healing belongs to a separately gated new Backlog item, and the ADR prohibits presenting closure as shipped healing.
3. **Private evidence.** Durable owner-only copies exist on both hosts, outside `/tmp`, the repo and the payload. They are hash-bound, with `/tmp` explicitly not the sole retention.
4. **Auxiliary source.** 439 tracked hashes and modes are bound to a revision. All 501 AppleDouble extras are catalogued, shown as inert to the imports and entrypoints, not authenticated, and carry no clean-inventory claim. The earlier 241 figure is explained.
5. **Mac CPython 3.15.** It is scoped to mechanism and module closure only, with explicit non-transfer of byte, header or marshal identity.
6. **T6 guard.** The guard is:
   - pure metadata;
   - placed before preflight and apply;
   - sourced only from the inner proof, with an inner-derived delta;
   - backed by strict stage Booleans, refusal of unknown outer pins, and a typed failure with a fixed next step.

   It does no live reread, so historical retry is preserved. The baseline-then-refusal tests are sound, and the source binding to immutable `76a7886` is correct.

Non-blocking: the macOS archive revision appears only as the abbreviated `a67b8b8`. Consider using the full SHA for parity with the Linux binding.

## Independent code-aware approval

`/root/adr20_review` read the actual final amendment and source-bound evidence
and independently accepted the exact final design. Its earlier82d2169 approval
is superseded by this approval of the added consistency guard. No healing or
parser implementation was approved. AWA verified root execution attempts for
#238 and #239 before skill/runtime edits. Implementation and signed delivery
require their own independent acceptance.

## 2026-10-30 recovery amendment

The operator rejected the recurring Beast update refusal. T7/#240 therefore moved
into v0.0.11; T5/#238 remains the separate RCA/prevention outcome. The recurrence
observation found 25 cache extras, 189 expected files and no changed or missing
recorded payload. This read-only comparison did not freshly authenticate the
publisher or identify the writer.

Actual `claude -p --model opus` review of `bb4c2cdbca26e54dcecdcd13a82744fbc9da151e`
returned BLOCK on same-version reuse, qualification handoff, old-wrapper execution,
rollback semantics and launcher coverage. Revised source
`1f04d727708eb745f128d9986709217ec3d102e1` received Opus ACCEPT and separate
independent design ACCEPT. Actual review records remain private at
`/tmp/am-v011-t7-opus-review.json` and `/tmp/am-v011-t7-opus-rereview.json`.
Implementation acceptance and signed delivery remain separate gates.

The accepted strategy preserves opaque cache bytes without parsing, executing or
certifying them. It binds authenticated signed payload and source fingerprints,
forces fresh acquisition even for same-version repair, and uses ordinary
whole-runtime transaction backups. Restoring an old dirty tree after failure
remains unverified/pending and cannot invoke it as a verification step.
