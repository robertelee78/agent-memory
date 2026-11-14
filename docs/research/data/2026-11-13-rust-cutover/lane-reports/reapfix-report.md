# ReapFix lane report: bounded interrupt reap (src/process.rs)

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-reapfix` (detached at 5bd11582). I made no Git writes.

## Defect
`kill_and_reap_owned`, run from the SIGINT/SIGTERM handler, called a blocking `waitpid(pid, 0)` after each SIGKILL, with SIGINT and SIGTERM both masked. A child stuck in uninterruptible sleep therefore hung the process forever, and a second interrupt could not end it.

A second, latent defect showed up while testing. The std in this toolchain (rustc 1.99.1) keeps the forking thread's signal mask across fork and exec (`library/std/src/sys/process/unix/unix.rs`: "Inherit the signal mask from the parent rather than resetting it"). Because `spawn` blocks SIGINT/SIGTERM around the fork, every owned child (git, and so on) started with SIGINT/SIGTERM blocked. The baseline's Popen children start with the caller's mask. The earlier review's note that "std empties the child's mask" is wrong for this std.

## Baseline specification read
`identity_git.py` `_cleanup` (`_INTERRUPT_GRACE = .25`):
- The deadline is set once per `KeyboardInterrupt` (`time.monotonic() + grace`), before the kills.
- Every child is killed before any wait.
- Each wait is bounded by the time left of that one shared deadline.
- A second `KeyboardInterrupt` sets `_git_stop_reap_waits`: no further waits, but the remaining kills and closes still run.
- Unreaped children are marked `_git_cleanup_handoff` and are never signalled again.
- The count is surfaced as a note on the primary `KeyboardInterrupt`, which the uncaught traceback prints on stderr: `Git cleanup: {unreaped} killed-but-unreaped or cancellation-unproved children handed to standard CPython cleanup; no further signals`.

Contract tests read: `tests/contracts/test_identity_git_group.py` lines 479-503, 505-531, 533-548, 550-572, 574-598 and 695-730.

## Change (src/process.rs)
**Interrupt cleanup** (`cancel_and_reap_owned`):
- Runs once per process (`CLEANED`), in the first handler invocation.
- The budget starts before the kills: `INTERRUPT_GRACE_NS = 250_000_000`, read from `clock_gettime(CLOCK_MONOTONIC)`. The `clock` module is not used because its macOS clock is a `LazyLock` and can panic.
- Kill phase, with signals masked: each slot holding a PID is SIGKILLed after `waitpid(WNOHANG) == 0`, as before.
- `reap_within` then polls all pending PIDs with `waitpid(WNOHANG)`. Between polls it calls `nanosleep`, starting at 0.5 ms and doubling up to 50 ms (CPython `Popen._wait` delays, capped at `_EXIT_POLL_INTERVAL`), never past the deadline.
- When the deadline passes, the unreaped count goes into a lock-free `AtomicUsize`. Every processed slot is cleared, so a handed-off child is never signalled again by any later handler.
- Async-signal-safe throughout: no allocation, lock or formatting. Calls used: waitpid, kill, clock_gettime, nanosleep, pthread_sigmask and errno loads, plus SeqCst atomics.

**Second signal.** Decision: unmask during the bounded wait and use a re-entrant flag, rather than keeping the signals masked and polling `sigpending`. The reasons are documented in the module header comment:
- `reap_within` sets `REAPING` and then unblocks SIGINT/SIGTERM.
- A nested handler invocation only does an atomic store into `SECOND_SIGNAL`, which is safe at any re-entry point. Its EINTR cuts the current nanosleep short, and the loop stops before its next poll.
- A signal that became pending during the masked kill phase is delivered by the unblock itself, so the wait performs zero polls. All kills have still been attempted, as in the baseline's `finish(kill)`.
- Polling `sigpending` would add up to one poll interval of delay, and would leave the signal pending to be re-delivered as a fresh handler run. `sigtimedwait` does not exist on macOS.
- The caller's mask is restored before `REAPING` clears.
- SIGTERM outranks SIGINT. A SIGTERM that ends a SIGINT cleanup still makes the process die by SIGTERM at once.
- Later signals take the normal path: SIGINT only re-records and wakes; SIGTERM dies at once. The cleanup never reruns, so the grace never restarts.

**Supervisor-cancelled children.** `Owner::cancel` now uses `kill(pid, cancellation_signal())` on that positive PID and then marks its slot as `-pid` (`RESERVED` stays `-1`; PID 1 is never our child). The interrupt cleanup reaps such a child inside the budget but never signals it again. This matches the baseline's kill-once rule: "cleanup signalled a handed-off child again".

**After SIGINT the supervisor never waits or signals:**
- `Owner::cancel` marks a still-unreaped child `handed_off` instead of killing it.
- `Owner::reap` used to call the blocking `child.wait()`. It now polls `try_wait` with growing pauses on the SIGINT wake pipe (`wait_for_wake`). It stays unbounded, as in the baseline's ordinary `process.wait()`, until `interrupted()`; after that it makes one non-blocking look and then hands the child off.
- A blocking waitpid could not be ended by a SIGINT landing just before the call; the polling loop can.
- `Evidence.reaped` is false for a handed-off child.

**Surfacing the count.**
- `pub fn interrupt_cleanup_note() -> Option<String>` returns the baseline note naming this runtime: `Owned child cleanup: N killed-but-unreaped or cancellation-unproved children handed to process exit; no further signals`. It returns a note only after a SIGINT whose cleanup owned at least one child, mirroring the baseline note, which exists only when `_cleanup` ran under a `KeyboardInterrupt`.
- `exit_if_interrupted` writes that line to stderr before dying by SIGINT. `am` already calls it in `main.rs`, and `main.rs` is unchanged.

**Child mask fix.**
- `spawn` queries the caller's mask before the TerminationBlock.
- In `pre_exec`, the child first resets SIGINT/SIGTERM to SIG_DFL unless they are inherited as SIG_IGN; exec would reset them anyway, and this means no handler of ours ever runs in the forked child.
- Just before exec it restores the caller's mask.

**Kept guarantees.**
- A process group is never signalled; only positive PIDs.
- Free and RESERVED slots are never signalled.
- A recycled PID is never signalled (WNOHANG proof first).
- The process dies by the same signal afterwards.
- An inherited SIG_IGN stays ignored.
- Spawn still blocks signals around reserve, fork and record.

**Test seam (dev-tools builds only).** `process::set_cancellation_signal(SIGKILL | SIGTERM)` and `cancellation_signal()`. Release `am` builds without dev-tools use the constant SIGKILL.
- **Why it is needed:** on macOS (Darwin 27.3) I probed with C, never Python, for an unprivileged child that outlives SIGKILL by more than 250 ms. Every candidate was reaped within 2 ms of SIGKILL:
  - vfork: macOS vfork returns at once, like fork.
  - ptrace delegation: PT_ATTACH/PT_ATTACHEXC fail with EPERM.
  - SO_LINGER exit.
  - POSIX AIO on a pipe, FIFO or pty: ESPIPE, ESPIPE (processed at once), ENXIO.
  - Self `task_suspend`.
  - A hung Mach exception port.
  A SIGSTOPped child also dies on SIGKILL. With the seam set to SIGTERM, a child that traps or ignores SIGTERM, or is SIGSTOPped, outlives its cancellation exactly as an uninterruptible child outlives SIGKILL, and the real handler, budget, handoff and supervisor paths run unchanged.
- **Gating:** tests that use the seam are `#[cfg(feature = "dev-tools")]` and do not run in a plain `cargo test`.

## Public API
New:
- `process::interrupt_cleanup_note() -> Option<String>`.
- `#[cfg(feature = "dev-tools")] process::set_cancellation_signal(libc::c_int) -> io::Result<()>`.

Changed behaviour:
- `exit_if_interrupted` prints the note.
- `Refusal::Interrupted` and `install_termination_handler`/`interrupted` documentation updated.
- No signature changes.

## Files modified
- `src/process.rs`
- `tests/t0_process.rs`: new `#[cfg(feature = "dev-tools")] mod interrupt_reap`, 8 tests plus one re-exec worker.
- `tests/t2_cli.rs`: `terminate_with` now asserts that stderr is exactly the note `... 0 killed-but-unreaped ...` after SIGINT and empty after SIGTERM.
- `tests/obligations/python-baseline-v1.json`: statuses via `am-dev obligations mark`; `rust_test`/`note` filled by hand.

## Tests (tests/t0_process.rs, re-exec workers, seam = SIGTERM)
**Ported:**
- `interrupt_reap_budget_is_shared_bounded_and_reports_unreaped_count` (obligation `test_b2_interrupt_reap_grace_is_shared_and_reports_unreaped_count`). Three owned children: one SIGSTOPped, two recording-and-ignoring SIGTERM. It asserts:
  - `raise(SIGINT)` takes at least 250 ms and under 700 ms: one budget shared by all three, where per-child budgets would take 750 ms.
  - The note reports 3.
  - Values refuse `Interrupted` with `reaped == false`, without waiting.
  - Each child received exactly one signal (the stopped one received none it could record) and all are still unreaped: no further signals and no reap.
  - `run` refuses after the interrupt.
- `second_interrupt_during_the_bounded_reap_ends_every_remaining_wait` (`test_b2_second_interrupt_during_reap_ends_all_remaining_waits`). A real SIGINT from another thread 60 ms into the wait ends the cleanup in under 250 ms. The note reports 3, each child got exactly one cancellation, and none is reaped.
- `interrupt_in_timeout_cleanup_never_restarts_the_budget_or_signals_a_handoff` (`test_b2_interrupt_in_timeout_cleanup_never_restarts_grace_or_signals_handoff`):
  - A 200 ms group deadline cancels child 0, whose reap waits unboundedly.
  - SIGINT at about 600 ms ends it 250 ms or more and under 700 ms after the signal.
  - The handler does not re-signal the supervisor-cancelled child.
  - Values 1 and 2 refuse `Interrupted`.
  - A later SIGINT returns in under 100 ms.
  - Every child received exactly one SIGTERM.

**Partial:**
- `signal_pending_at_the_bounded_wait_stops_it_before_any_reap_after_every_kill` (`test_b2_second_interrupt_stops_waits_but_finishes_kill_and_fd_cleanup`). SIGINT and SIGTERM are both made pending on the worker thread and then unblocked. The worker dies by SIGTERM less than 250 ms after the unblock (zero reap polls), and all three children record their cancellation (every kill attempted).
  - **Missing:** the second signal is SIGTERM, because two SIGINTs cannot both be pending on one thread, and pipe closing after the stop is not observed because the worker dies.
- `interrupt_during_an_ordinary_reap_switches_to_the_shared_budget` (`test_interrupt_during_ordinary_or_system_exit_reap_switches_to_shared_grace`). Dropping a group cancels and then waits unboundedly; SIGINT 300 ms later ends it within 250 to 700 ms, the note reports 3, and nothing is re-signalled.
  - **Missing:** SystemExit has no Rust counterpart.
- `ordinary_cleanup_without_an_interrupt_keeps_waiting_beyond_the_budget` (`test_ordinary_and_system_exit_keep_unbounded_os_wait_parity`). Ordinary group drop waits longer than 250 ms until children that ignore SIGTERM exit on their own, then reaps them. There is no interrupt and no note.
  - **Missing:** SystemExit has no Rust counterpart.

**Regression test, no obligation:**
- `owned_children_start_with_the_callers_signal_mask`. With the handler installed, `sh -c 'kill -INT $$'` and `kill -TERM $$` die by their signal, and the parent's mask is unchanged after spawn. Before the mask fix, the TERM-recording children never ran their trap.

**Not ported here (outside the referenced lines and not about the reap bound):**
- `test_interrupts_during_launch_select_and_parsing_preserve_primary_and_cleanup`
- `test_cleanup_faults_attempt_remaining_real_kills_closes_and_reaps`

## Declared deltas
- **Note wording:** `Owned child cleanup: N killed-but-unreaped or cancellation-unproved children handed to process exit; no further signals`, in place of `Git cleanup: N ... handed to standard CPython cleanup ...`.
  - The handler covers every owned child, not only Git.
  - It is printed as a bare stderr line by `exit_if_interrupted` instead of inside a Python traceback.
  - It is printed only after SIGINT and only when the cleanup owned at least one child.
- **SIGTERM:** runs the same bounded cleanup and dies. In the baseline SIGTERM's default action kills at once, with no cleanup and no note.
- **Second signal:** a second SIGINT/SIGTERM during the wait is handled by a nested handler invocation (stop only). A SIGTERM among them still kills by SIGTERM right after the wait stops.
- **Ordinary reap:** with no interrupt, the reap polls (0.5 ms doubling to 50 ms, woken by the SIGINT pipe) instead of a blocking waitpid. It is still unbounded, as in the baseline.
- **Child mask:** owned children now start with the caller's mask. Previously SIGINT/SIGTERM were blocked in them, unlike the baseline.

## grep
`grep -n 'unwrap()\|expect(' src/process.rs` shows only pre-existing lines: 599, 1617, 1837, 1858 and 2208 (invariants), plus 2085-2158 in the existing `#[cfg(test)]` module. None was added by this change.

## Commands run (aarch64-apple-darwin, Darwin 27.3.0 xnu-13432.60.57 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13))
- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: 0 warnings, 0 errors.
- `cargo check --offline --all-targets --features dev-tools --target aarch64-unknown-linux-musl` and the same for `x86_64-unknown-linux-musl` (`CARGO_TARGET_DIR=/tmp/reapfix-xcheck`): 0 warnings. This was after replacing the deprecated musl `libc::time_t` alias with `as _`.
- `cargo test --offline --features dev-tools --test t0_process --test t2_cli --test obligations_ledger`, three times in a row: all green each time (47 + 24 + 5).
- `cargo test --offline --no-fail-fast --features dev-tools` (whole suite): exit 0, 78 test binaries, 1190 passed, 0 failed, 237 ignored.
  - This run compiled before the final type-only edit (`as libc::time_t` → `as _` in `pause_ns`).
  - After that edit I re-ran fmt, check, the three t0/t2/ledger rounds and `--lib process::` (2 passed).
- Linux runtime was not run: no Linux host was available in this lane.
