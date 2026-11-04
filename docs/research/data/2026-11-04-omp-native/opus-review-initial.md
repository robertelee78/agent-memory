I'm requesting changes to the technical design. This is not operator approval, implementation acceptance, native support or a delivery claim.

## 1. Verdict

**REQUEST CHANGES.** The overall direction is sound: capture by value at `message_end`, store private snapshots, reuse the shared `turn_sources` ledger, use `session_stop` only as an advisory `continue:true`, and keep canonical mutation behind approved tools. Six contracts would still violate required invariants or leave the implementation ambiguous.

## 2. Blocking findings (highest priority first)

**B1. Registering every message as its own source can never finish for ordinary long tasks.**
- **What the design says.** "Source capture", "Exact action/schema invariants" (`capture`) and the acknowledgement section register each user, assistant and tool-result `message_end` as its own shared observation, and each one needs its own acknowledgement.
- **Source evidence (`turn_sources.py`).**
  - `MAX_SOURCES=128` per lifecycle and `MAX_TURNS=4096` shared pending slots across all harnesses.
  - Hitting capacity writes `fault.json` permanently.
  - `inventory()` reports complete only if there are no faults, so a lifecycle that once hit capacity can never complete, and `cleanup()` never frees its pending-index slot.
  - `boundary_assessment.validate` and `verify_assessment` work on one source at a time, so a published outcome runs `sharing_recovery.replay` once per source.
- **Consequences.**
  - A task with about 60 or more tool calls hits the 128 cap. It then stays pending permanently, even under explicit recovery.
  - The advisory continuation must ask for N approved acknowledgement calls. Under always-ask approval that is N prompts, which takes over the task.
  - Memory tool results produced during the turn are themselves new pending sources in the same lifecycle. That changes the fingerprint, uses up both `wake-0/1` slots, and the lifecycle never converges.
  - Lifecycles that never complete gradually use up the shared 4096 slots, which also blocks Codex and Claude.
- **Smallest fix.**
  - Keep per-message private snapshots, but register at most two ledger sources per lifecycle:
    - the user message, at capture time, so it survives cancellation;
    - one sealed "lifecycle segment" source whose snapshot is an ordered, hash-linked manifest of the captured message snapshots.
  - Seal at `session_stop` before requesting a continuation. If `session_stop` is skipped, seal at the next genuine user input, at resume, or through explicit recovery.
  - Messages after the seal and before the next genuine user input are unregistered control evidence.

**B2. Requiring a verified native mapping before acknowledgement makes legitimately retained sources unrecoverable.**
- **Native source evidence** for messages that are never persisted, or persisted in a different form:
  - Classifier refusals and empty error turns are not persisted (`agent-session.ts` ~4209–4211).
  - A prompt-generation change skips persistence (4292).
  - Duplicate persistence is suppressed (`#sessionMessageAlreadyPersisted`, 4150).
  - With obfuscation on, the extension sees deobfuscated assistant content while the session file stores `$$HASH$$` tokens (4621–4637).
  - Custom messages persist as `type:"custom_message"` entries with no `role` (steer entry `09d5ce9d`).
  - Session files are lazy: nothing is written before the first assistant message (`session-manager.ts` 3226).
  - Some message entries are appended without any `message_end`: interrupted tool results (12561), reparented history (12204–12208, 13759–13765) and `appendMessageToBranch` (14052).
- **Mapping strength.** The `reconcile` entries come from the caller, through a public `--observe` route the model can also run. So `native_mapping_verified` is an observer's assertion, not stronger evidence than the snapshot taken at `message_end`.
- **Policy violation.** The approved policy is to retain original pending learning and allow explicit recovery. Under this design those sources stay pending forever and nothing can acknowledge them.
- **Smallest fix.**
  - Make the native mapping a provenance qualification (`verified | unverified | absent`) stored in private metadata, not a precondition for acknowledgement.
  - Allow acknowledgement against the retained, digest-verified snapshot.
  - Define how `reconcile` handles native message entries that were never captured: record them as named coverage gaps that block segment sealing.
  - Define how it handles captured messages that were never persisted: they stay eligible from the snapshot.

**B3. With obfuscation on, the snapshot captures secrets the host deliberately keeps out of its own transcript.**
- **Evidence.** Extensions receive `displayEvent`, which is deobfuscated by design (4621–4637), and `secrets.enabled` installs the obfuscator (3289–3291). The design keeps snapshots without purge in v0.0.15.
- **Why it blocks.** This breaks the design's own rule to "exclude credentials", and every such assistant message also fails the reconcile projection.
- **Fix.** Specify the captured representation. With obfuscation active, either capture only the persisted (obfuscated) form at reconcile time, or record a coverage gap. Never store deobfuscated content. State what happens to thinking blocks and reasoning or text signatures (see the steer entries).

**B4. Lifecycle and attribution classification is not deterministic.**
- **Subagents.** Extension factories are rebound to every subagent session, including the task tool, eval `agent()` and `/tan` clones (`types.ts` 546–556). `session_stop` is skipped for subagents (`agent-session.ts` 5798). The design never mentions `ctx.agent.kind`, so subagent "user" prompts would open false human lifecycles that never get a continuation.
- **Unhandled roles.** The allowed role set (user/assistant/toolResult/custom) leaves out `developer`, `fileMention`, `bashExecution` and `pythonExecution`. `developer` and `fileMention` are persisted (4196–4203). A `fileMention` is part of the human's request, so refusing it creates a gap on ordinary file mentions.
- **Undefined classifications.**
  - "Exclusively memory-maintenance descendants" has no rule that avoids text heuristics.
  - Custom messages from other extensions and other attributions are not classified.
  - A single `session_stop` can face several pending lifecycles (original plus steering), but the `wake` payload covers only one lifecycle.
- **Fix.** Add a decision table:
  - A genuine human lifecycle starts on `role:user` + `attribution:user` + `agent.kind==="main"`.
  - Subagent sessions are not registered; the parent's tool result covers them.
  - Every role gets an explicit disposition.
  - Control interval = from a `session-stop-continuation` entry to the next genuine user entry.
  - One continuation lists every reserved lifecycle, with defined semantics when only some reservations succeed.

**B5. Adding `omp` to the shared harness list breaks older runtimes, and the rollback gate does not catch it.**
- **Evidence (`turn_sources.py`).**
  - `index()` calls `validate_reference(old, …)` on every slot without catching errors.
  - An older runtime whose harness list lacks `omp` therefore raises on any retained omp reference.
  - That blocks all Codex and Claude registration in that runtime.
- **Evidence (`installation.py`).**
  - `rollback_compatibility` (937–962) checks only `readable_bank_schemas`.
  - The design keeps omp pending state on uninstall, so rollback or downgrade would hit this failure.
- **Fix.** Declare a private turn-source compatibility marker and refuse rollback or downgrade below the omp-aware version while omp references exist, or give a different tested contract. Add this to the acceptance criteria.

**B6. Installer activation and receipt contract are underspecified.**
- **Silent install on update.** `presets('all')` expands every key in `values`, and update defaults to `old['presets']`, which is usually `'all'` (`installation.py` 415–431, 495). Adding `omp` would install executable extension authority on update without the explicit preview and authorization the design itself requires.
- **No receipt field for the omp root.** Existing uninstall rebuilds preset and hook paths from the current environment (547–550, 433–444). The design says to "persist chosen root in receipt" but defines no receipt field, schema version or compatibility rule.
- **Fix.**
  - `omp` must be opt-in only: excluded from `all`, and requiring an explicit root.
  - Specify the receipt field, the validator change, and that uninstall uses the receipt targets only.

## 3. Nonblocking concerns (pin these during implementation)

- **Capture idempotency.** `register()` generates a random `observation_id` and has no idempotency key. If the 5-second kill lands between `register` and the capture record, a retry duplicates the source. Pin a content-addressed `source_reference` (including `capture_id`) and look it up in the inventory before registering. Also pin the schemas, locations and bounds for the cursor registry, capture record, mapping record and snapshot envelope; they are currently prose only.
- **"No shell" vs the existing launcher.** Peers run through a `/bin/sh python3.sh` launcher (`installation.py` `client_hook_entries`), and the wrapper searches `PATH` for an interpreter. Specify the exact exec target and how the interpreter is pinned.
- **Duplicate-load guard.** Two module instances of the same file only share state through a process-global registry (for example `globalThis[Symbol.for(...)]`). Say so, because it conflicts with the "no module-global state" wording.
- **`session_stop` latency.** It is awaited and subject to the runner's event-handler timeout, whose value I couldn't verify. Several serialized 5-second helper calls can exceed it. Bound total time and treat a timeout as no continuation, with pending state kept.
- **Per-message cost.** Each message spawns a Python process, and `register` can scan up to 2×4096 index slots.
- **`control_origin_hint`.** It is never set for omp, because the continuation digest never equals a snapshot digest. Define `generated_sha256` and say the hint is unused.
- **Smaller gaps.**
  - Define behaviour for an unbound repo or no memory bank (stay inactive rather than log a gap per message).
  - Cover `--no-session` mode.
  - A cancelled first request may leave no session file at all.
  - Images may differ between their in-memory and on-disk forms (the session file externalizes them to a blob store).
- **Evidence limit.** In the cancel fixture only one `session_stop` was observed. That is consistent with "cancellation can skip session_stop" but the cause wasn't isolated.

## 4. Artifact, inspected material and unverified limits

- **Reviewed artifact.** `/tmp/agent-memory-omp-native/execution-design.md`, stated SHA-256 `7801f967bad62da2101f033355a84596c1cc84b8bf60eca10c81d3684d573fa7`. I could not compute the digest with read-only tools, so it is unverified. A manual text comparison found the local file identical to the raw `a0b92ec3…/docs/design/v0.0.15-omp-integration.md`.
- **Inspected.**
  - `feasibility-report.md`, `verified-invariants.json`, `message-persistence-comparison.json`.
  - `steer/` and `capture-order/` summaries, `cancel/` summary and observations.
  - Identity `report.md`.
  - `turn_sources.py` (full), `boundary_assessment.py`, `codex_boundary.py`, and `installation.py` (presets, receipt, rollback).
  - `agent-session.ts` (`message_end` persistence, obfuscation, `session_stop`, subagent kind, direct appends), `session-manager.ts` (lazy persistence, blobs), `types.ts` (agent identity, event API).
- **Not verified.**
  - The extension runner and wrapper source, including the handler timeout value.
  - Where Codex and Claude register sources, and at what granularity (their hooks weren't provided).
  - `exported-evidence.json` and the remaining scenario raw files in detail.
  - Whether the installed binary matches the pinned source.
  - Linux, TUI and print modes.