# Omp20 lane: retarget the native omp integration to omp 18.20.1

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-omp20`, detached at `a2d05a0f`. Nothing is committed: Main owns Git. Platform: macOS arm64 (darwin 27.3.0) and Lima `am-rust-v015-linux-01` (Ubuntu 24.04, 6.8.0-149-generic, aarch64). Evidence files are in `~/.local/state/agent-memory/research/rust-cutover/omp20-evidence/`. Throwaway probe tools are in `…/omp20-probe/` (not in the repository).

## 1. Binaries

Downloaded to `qualification/inputs/omp-18.20.1/`, together with the release `SHA256SUMS.txt`. Each digest equals the `SHA256SUMS.txt` line and the GitHub API asset `digest`. Tag `v18.20.1` is a lightweight tag pointing at `b7fe8c7f43c378faa0863e79f5509f9dde16ad9a`. The release is `immutable:false`.

| Asset | sha256 | Agrees with acceptance-plan §3a |
|---|---|---|
| omp-darwin-arm64 | `2a080bfd790309001ea001140382a5850be284e984c2f8678a81a41a37e77490` | yes |
| omp-linux-arm64 | `21305158dbdecfa771b72a62254831d60be6b96726245877c25324a4b9988f76` | yes |
| omp-linux-x64 | `24bef5a44d6db0d8f6e8f677404b9a7f1dde3c5e500c435290a3be71bb4dec2a` | not listed in §3a; matches SHA256SUMS and the API |

- The operator's installed `omp` is byte-identical to `omp-darwin-arm64`.
- I also downloaded negative-control images to `inputs/omp-18.19.0/`: darwin `d469628a…` and linux-arm64 `61826bf8…`. Both match §3a.

## 2. Upstream study (clone `omp-src-18.20.1/`, read-only)

Line numbers are at `b7fe8c7` unless marked. Files: AS = `packages/coding-agent/src/session/agent-session.ts`; RN = `…/extensibility/extensions/runner.ts`; SM = `…/session/session-manager.ts`. I compared against 18.19.0 (`04cd1a7`) and 18.16.0 (`650f3a1`); 1,163 commits separate 18.19.0 from 18.20.1.

### Extension API and version string
- `pi.pi` is `typeof index`, see `extensibility/extensions/types.ts:92,1876`. `VERSION` is re-exported from pi-utils (`index.ts:10`); `packages/coding-agent/package.json` has `"version": "18.20.1"`.
- The following are unchanged since 18.16 (the diff has 0 hits for the three helpers):
  - `CURRENT_SESSION_VERSION = 3` (`session-entries.ts:17`);
  - `sessionEntryLeafId`, `lastSelfSelectedLeafId`, `resolveSessionLeafId` (`session-entries.ts:374,379,391`);
  - `parseSessionContent` with result keys `entries, titleSlot, malformedRecords, sourceSize, invalidHeader` (`session-loader.ts:130-138,166-182`).
- The only new extension event is `session_clear` (`types.ts:1889`, `shared-events.ts:49-56`).
- Short skill descriptions (≤100 characters) are now used verbatim (`skill-descriptions.ts:424-431`). AM's description is longer, so it is still compressed by the smol model. **Consequence:** RPC startup alone sends one auxiliary model request. I observed it with the stub (roles `[user]`, no tools). On a real provider, "model-free startup" is therefore not free.

### Session file format and headers
- Entry types are identical in 18.16, 18.19 and 18.20.1. All non-message types are in `src/omp_transport.rs` `NONLEARNING`.
- The header keeps `parentSession` and `continuationOf` (`session-entries.ts:56-63`). Write-conflict continuation `#moveOffSessionFile` (`SM:1305-1352`) is unchanged since 18.16 (`SM:1272-1304` at 650f3a1). B2 is therefore unchanged, not new.
- New `isOwnedElsewhere()` (`SM:3296-3305`). `#recordSessionExit` now skips the exit `custom` record when another process owns the session (`AS:3981-3985`). This reduces exit-time sibling forks. AM needs no change.
- Relocation: 18.20.1 resumes sessions whose recorded directory is missing or moved (`SM:4495-4515` `isStrandedIn`, `openRelocated` at `SM:4522-4540` rewrites the header via `moveTo`). The `SESSION_CWD_CHANGE_REJECTED` branch in `switchSession` was removed (18.19 `AS:14178-14179`).
  - AM keys on runtime `getCwd()` and validates the Git root of `view.cwd`. The header cwd is only compared within one instance (`retiredView`).
  - A mid-process relocated switch appears as `session_switch`, giving a new instance and the transition path. No change was made.

### `session_clear` (new in 18.20.1, commit `0e06223534`)
- `resetSessionContext()` at `AS:7079-7166` refuses while streaming. It then:
  - bumps `#promptGeneration` and cancels post-prompt tasks;
  - sets `#pendingNextTurnMessages = []` (`AS:7114`), which drops a queued `session-stop-continuation`;
  - resets the stop-continuation chain (`AS:7122`);
  - appends a durable `reset_boundary` entry (`AS:7159`);
  - emits `session_clear` (`AS:7164`).
- `/clear` existed with the same drop semantics in 18.16 (`AS:6691` at 650f3a1). Only the event is new.
- It is reachable only from the TUI (`slash-commands/builtin-lifecycle.ts:208-217`). There is no RPC command for it.

### `/fork` aborts an active response (commit `9e14fd419d`)
- `command-controller.ts:1448` and `selector-controller.ts:1540-1541` now call `await session.abort()` instead of warning.
- `abort()` (`AS:12189-12290`) waits for idle and agent_end maintenance. Then `#resetSessionStopContinuationState()` and `#clearPendingSessionStopContinuations()` run (`AS:12277-12278`).
- After that, `fork()` (`AS:12482…`) emits `session_before_switch` and `session_switch`.
- For AM this is the existing abort (skipped/lost) path followed by the existing transition path. No change was needed.

### Shutdown handler ordering
- This is unchanged from 18.19:
  - `#doDispose` writes the exit record (`AS:6848`), then `emitSessionShutdownEvent` (`AS:6854`).
  - All `session_shutdown` handlers start in load order and are awaited with `Promise.all`, each under the 2 s cap (`RN:165-184`, `RN:2042-2054`).
  - Agent abort and drains run afterwards (`AS:6860-6875`).
- `session_before_shutdown` (TUI cancellable gate, `shared-events.ts:109-113`, `AS:6648-6655`) exists since 18.19. AM does not use it.
- B3 is unchanged.

### `read` result used for manual-read recognition
- Path: `tools/read.ts:2203-2226`. A located `skill://` URL adds `resolvedPath`, `contentType: text/markdown`, `meta.source`, and a `[Skill file: <path>]` prefix.
- `mediaPath` is added only for Tern media extensions (`read.ts:2241-2244,2498-2503`); markdown is not one of them.
- **Measured** with a scripted local stub provider: a real 18.20.1 binary and a real read tool, but no model. I compared the stored entry, passed through a mirror of `omp.ts` `storedEntries`, with the recipe in `src/omp_projection.rs` `manual_result`, `manual_invocation` and `manual_details`. Content and details are byte-equal (sorted keys, as `same()` compares) for:
  - `skill://agent-memory` and `skill://agent-memory/capture/GUIDE.md`;
  - both copy and link installs.
- Header and `resolvedPath` are the logical agent-dir path, also for link installs.
- So the 18.16 renderer recipe holds on 18.20.1 with default read settings. The renderer ID was not changed (open question D9).

### RPC protocol
- Between 18.19 and 18.20.1, only the `select` UI request gained optional fields (`rpc-types.ts:800-807`, `wire/frames.ts:169-178`).
- The `ready`, `prompt`, `get_state` (`sessionId`, `sessionFile`, `isSettled`) and `session_settled` frames used by `am-dev omp-smoke` are unchanged (`rpc-mode.ts:1424,2272-2279`).
- Observed: the `ready` frame is protocolVersion 1 and supports protocol versions 1 and 2.

### Queued guidance in agent-session (18.16 binding line → 18.20.1 line)

| Element | 18.16 lines | 18.20.1 lines | Difference |
|---|---|---|---|
| async-wake stop hold | 5230-5237 | 5417-5424 | none |
| scheduler `aborted`/`stale-generation` skip | 5281-5292 | 5468-5479 | none |
| missing model / credential (Failed causes) | 8869-8886 | 9567-9584 | none |
| generation checks (return false) | 8927, 8969-8970, 9005-9006 | 9625, 9667-9668, 9705-9706 | see note 1 |
| nextTurn injection after the human entry | 8939-8943 | 9637-9641 | none |
| hidden-turn scheduler (re-queue on throw) | 9897-9942 | 10631-10687 | none |
| nextTurn `deliverAs` | 10200-10288 | 10990-11078 | see note 2 |
| `queuedMessageCount` | 10446-10457 | 11246-11257 | none |
| abort clears continuations | 11347-11348 | 12277-12278 | see note 3 |
| `#emitSessionStopEvent`, `#clearPendingSessionStopContinuations`, cap 8 | 5815… | 5983-6035, 5912-5919, 521 | byte-identical blocks |

Notes:
1. 18.20.1 has extra return-false points from autoParallel and autoPrewalk (`AS:9690-9710`). These are the same class of abort-during-setup exit. The prewalk plan nudge and auto-parallel hint add companions to the user batch, which the empty allowlist already classifies as combined.
2. 18.20.1 removed the ACP `#allowAcpAgentInitiatedTurns` exception. ACP is not a claimed mode.
3. Forced follow-up promotion was added just before `#resetInFlight`.

**Source inference only, not a review:** the failed, skipped/lost and deferred bindings carry over textually. B1 still needs the independent re-review.

## 3. Code changes (uncommitted in the worktree)
- `skills/agent-memory/hooks/omp.ts`:
  - The gate is now exactly `pi.pi.VERSION === '18.20.1'`.
  - New `session_clear` handler: it captures a full `ingest` (the stored `reset_boundary`), then calls `cancel(state)` and sets `state.request = undefined`. Rationale: stock has just dropped the queued own continuation (`AS:7114-7122`), so later successor work is `foreign-hidden`, not `own-hidden-ambiguous` (`src/omp_transport/observe.rs:1354-1358`). The pre-clear view is captured with the prior trigger.
- `src/omp_installation.rs`: new `pub const SUPPORTED_VERSION: &str = "18.20.1"`, the single Rust pin. Diagnostics `supported_version_targets` is now `["18.20.1"]`.
- `src/bin/am_dev/omp_smoke.rs`: the gate uses `SUPPORTED_VERSION`, with message "require exactly omp/18.20.1"; the report field `supported_versions` is now `["18.20.1"]`.
  - I also fixed a pre-existing defect in the same file. The "exact receipt-owned public command" check compared `canonical(--am)` with the literal `bin_dir/am`. Rust installs own `bin/am` as a symlink to the runtime, so the check refused every real Rust install. It also refused the controlled fixture, which is why `test_unsupported_binary_refuses_before_native_launch_or_human_turns` was ignored.
  - The check now requires the named command to be `am` or `agent-memory` inside the canonical receipt `bin_dir`, then runs its canonical target.
- `src/omp_stream.rs` `LINUX_MONOTONIC_IMAGES`: now exactly the 18.20.1 `omp-linux-arm64` and `omp-linux-x64` digests. The 18.16 and 18.19 images were removed.
- Docs:
  - `docs/testing/omp-native-qualification.md`: intro, blockers section rewritten for 18.20.1, renderer paragraph, smoke gate.
  - `docs/releases/v0.0.15.md`: compatibility section; the historical 18.19 clock paragraph is marked superseded.
  - `README.md`: omp section.
  - `skills/agent-memory/turn/GUIDE.md:62`.
- Not changed:
  - Python baseline files (`scripts/evaluations/omp_native_smoke.py`, `skills/…/omp_installation.py`, contract tests). Lane rule: never edit Python; Gate C deletes them.
  - The approved design `docs/design/v0.0.15-omp-integration.md` still names 18.16/18.19 and the `650f3a1` bindings. That needs an approved amendment (see open questions).
  - The renderer constant `omp-18.16-full-skill-v1`.

## 4. Tests
- New `tests/t2_omp_stream.rs::linux_clock_gate_admits_exactly_the_18_20_1_release_images`: exact allowlist; 18.16 and 18.19 arm64/x64 images and 18.20.1 musl images are refused.
- New `tests/t2_omp_install.rs::adapter_gates_on_the_single_supported_version_and_handles_session_clear`: `omp.ts` has exactly one `VERSION` comparison, equal to `SUPPORTED_VERSION`, and the exact `session_clear` handler body.
- `tests/c_test_omp_native_smoke.rs`:
  - The unsupported-version list is now `18.16.0, 18.19.0, 18.20.0, 18.20.2`.
  - Controlled binaries are 18.20.1.
  - `test_unsupported_binary_refuses_before_native_launch_or_human_turns` is **un-ignored** and passes after the smoke fix. This addresses B13.
- `tests/fanout_b_omp.rs`: status/doctor `supported_version_targets == ["18.20.1"]`, added in an already-ignored (recovery_frontier) test.
- Behavioural harness, out of repo (`omp20-probe/fake-helper.mjs`, Bun): it loads the real `omp.ts` with a fake `pi` and a frame-speaking helper.
  - Control run: after a granted wake, later work is ingested as `own-hidden-ambiguous`.
  - With `session_clear`: the clear capture carries `own-hidden-ambiguous` and the `reset_boundary`; later work is `resume`.
  - With VERSION 18.19.0: "inactive: unsupported public API or activation binding" and no helper spawn.
- Gates, all on macOS arm64:
  - `cargo fmt --check`: OK.
  - `cargo check --offline --all-targets --features dev-tools`: 0 warnings.
  - First run, before the smoke fix: t2_omp_boundary 17/0, t2_omp_install 21/0, t2_omp_stream 94/0, t2_omp_transport 19/0, fanout_b_omp 49 passed / 207 ignored, c_test_omp_native_smoke 7/11 ignored.
  - After the fix: c_test_omp_native_smoke 8 passed / 10 ignored. The final full rerun result is reported in the final message.
  - The remaining ignored c_test and fanout tests fail only on `recovery_frontier provider is not registered` (the bank fixture), which pre-exists in this tree.
- Not run: the three-times repetition and the whole suite (the lane rules ask for them; Main integrates). Linux test binaries were not run via `linux-run.sh`, because it wipes the shared VM's `/private/tmp/am-rust-v015-linux-*`.

## 5. Probes per platform

### macOS arm64
World: `/private/tmp/omp20-world-mac`, with a throwaway HOME, XDG directories and `--omp-agent-dir`.
- **Install and activation.** `am install --development-source <copy with debug am at target/release/am> --presets omp --omp-agent-dir … --omp-peers-upgraded`, copy mode, exit 0 (artifact `7a39ace3…`).
  - Link mode, in a separate world, exit 0; the skill target is a symlink.
  - `am --json status`: `owned_disk_integrity: verified`, `supported_version_targets: ["18.20.1"]`, `answer_first.activation: receipt-owned`.
  - `am doctor` fails on the pre-existing `recovery_frontier provider is not registered` and `omp.native_boundary` issues of this tree. That is not caused by this change.
  - Bank scope binding is impossible in this tree for the same reason, so the helper reports pending.
- **RPC handshake without a prompt** (real 18.20.1 with a stub `models.yml` provider):
  - `ready` arrives at about 260 ms (protocol 1, supports 1 and 2); `get_state` succeeds and reports settled.
  - The installed AM extension passed the version gate, spawned `am omp-boundary --stream`, and then reported `am: observer pending` (unbound bank).
  - Negative, same world with real **18.19.0**: `am: inactive: unsupported public API or activation binding`.
  - Evidence: `macos-rpc-startup-18.20.1.json`, `…-18.19.0-negative.json`.
- **`am-dev omp-smoke` gate:**
  - 18.19.0 refuses before launch ("require exactly omp/18.20.1, observed omp/18.19.0", `native_requests: []`).
  - 18.20.1 passes the gate and runs two RPC turns against the stub. It then fails as expected: the stub answer lacks the sentinels, and the observer is pending.
  - Evidence: `macos-omp-smoke-*.json`.
- **Manual-read carrier:** see §2. Session files are under `/private/tmp/omp20-world-mac{,-link}/sessions`.

### Linux arm64 (Lima, `/tmp/omp20-probe-lane`)
- Images were re-verified in the VM: 18.20.1 `21305158…`, 18.19.0 `61826bf8…`.
- Probe: a throwaway crate (`omp20-probe/src/main.rs`, aarch64 musl, built with rust-lld as in `linux-run.sh`) drives the product `agent_memory::omp_stream::ClockWindows::system`. A `-e` extension spawns it as a direct child of the real omp (`ppid == omp pid`) and sends CLOCK and CLOCK-ACK rounds every 250 ms, using omp's own `process.hrtime.bigint()`.
- Results:
  - **18.20.1**: the first three rounds were refused while the image hash was computed (about 0.8 s, debug build); the next three were admitted (verified window).
  - **18.19.0**: 0 of 79 rounds admitted in 20 s.
  - Evidence: `linux-arm64-clock-18.20.1.json`, `…-18.19.0.json`.
- Observation: Bun's hrtime origin differs from CLOCK_MONOTONIC (j0 about 0.97 s, p1 about 29429 s). The guard's offset arithmetic tolerates this; only the rate relation matters. Independent clock-provenance review is still required (B4).
- Linux `am` debug build: aarch64-musl `81941e9c…`. I did not install `am` on Linux because no Linux install probe was required.

### Linux x86_64 (not run; no x86_64 host here)
Build, already done here, tested only for the build:
```
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld cargo build --offline --target x86_64-unknown-linux-musl --bin am
# am sha256 ca282a64e98522a6c6c45a85b5e148b3ddca31140ef7bd55785d2101ec1ac454
cd ~/.local/state/agent-memory/research/rust-cutover/omp20-probe && CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld cargo build --offline --target x86_64-unknown-linux-musl --bin omp20-clock-probe
```
On omp-beast, copy `target/x86_64-unknown-linux-musl/debug/omp20-clock-probe`, `clock-probe-ext.ts`, `omp-18.20.1/omp-linux-x64`, a negative image (`omp-18.19.0/omp-linux-x64`, `b44fda2a…`) and a stub `models.yml` (provider `stub`, baseUrl `http://127.0.0.1:9/v1`, `apiKey: OMP20_STUB_KEY`) to `$D`. Then run:
```
cd $D && sha256sum omp-*      # expect 24bef5a4… for 18.20.1
for v in 18.20.1 18.19.0; do
  mkdir -p agent-$v home-$v && cp models.yml agent-$v/
  extra=; [ $v = 18.20.1 ] && extra=--offline
  ( sleep 35 ) | env HOME=$D/home-$v PI_CODING_AGENT_DIR=$D/agent-$v PROBE_BIN=$D/omp20-clock-probe \
    PROBE_OUT=$D/out-$v.json PROBE_SECONDS=20 OMP20_STUB_KEY=stub \
    timeout 90 ./omp-$v --mode rpc -e $D/clock-probe-ext.ts --model stub/stub-model --no-session --no-extensions --no-title $extra
  jq '{omp_version, arch, start, total_rounds, admitted_rounds}' out-$v.json
done
```
- Pass: 18.20.1 has `admitted_rounds ≥ 1` and `start.ppid == omp_pid`.
- Refuse: 18.19.0 has `admitted_rounds == 0`.

## 6. Open questions and blockers
1. **D9 renderer ID.** The 18.20.1 carrier equals the recipe (stub-measured, default read settings). I kept `omp-18.16-full-skill-v1`. Keeping it or introducing a new closed value is an operator and reviewer decision. A real native read (copy and link) is still required. Non-default read settings (line numbers or hashlines, which set `rowPrefixes`) would fall back to screening, as in 18.16.
2. **B1.** The binding text is source-identical (table above). Independent review and approval of an 18.20.1 binding addendum are needed, plus a design amendment: design rev 8 still names 18.16 and 18.19 and the `650f3a1` line numbers.
3. **`session_clear` adaptation.** Clearing `state.request` changes successor labels from own-hidden-ambiguous to foreign-hidden after `/clear`. This is my reading of the design's intent; please confirm. A native TUI `/clear` control is needed (RPC cannot trigger it).
4. **B2 and B3** are unchanged. 18.20.1 reduces exit-record sibling forks (`isOwnedElsewhere`).
5. **x86_64.** The image was admitted by directive without a probe. Run §5 on omp-beast, then do the clock-provenance review (B4 and D2).
6. **Startup auxiliary model call.** AM's long SKILL description triggers a smol-model compression request at startup. With `--offline` that goes to the active provider. Qualification spend and privacy notes should mention it.
7. **Pre-existing, not in scope.**
   - `recovery_frontier` is unregistered in this tree, which blocks bank binding, doctor and most fanout and c_test journeys.
   - `am install` run from a cwd that contains an AM source tree refuses ("Possible existing context bank"); installing from HOME works.
