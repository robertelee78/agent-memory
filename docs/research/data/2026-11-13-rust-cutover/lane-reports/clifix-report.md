# CliFix lane report: CLI/signal review findings F1–F7

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-clifix`, detached at `1b878ab`. I made no Git writes. The review is `cli-signal-review.md` (commit e47eae5).

## Files modified

All seven files are owned by this lane:

- `src/process.rs`. Only the owned-child registry, the handlers and spawn signal masking changed, plus the code they need in `Owner`, `Active::advance`, `Group::poll` and `Evidence.reaped`.
- `src/main.rs`
- `src/cli/parser.rs`
- `src/cli/input.rs`
- `src/cli/helpers/bank_writes.rs`
- `tests/t2_cli.rs`
- `tests/vectors/cli-argparse-v1.json`. I appended rows 25–36 only; the one existing line changed is the final `}` of row 24b, which became `},`. The appended rows are byte-identical to Main's capture `cli-vectors-v2.json` (checked with jq).

Untouched: `src/cli/mod.rs`, `src/cli/table.rs` and every shared file. I also made no change to `src/error.rs`, because the new refusal lives on `process::Refusal`.

Also written:

- `cli-vector-requests-2.md`: the argv requests for rows 25–36.
- This report.

## Public API changes (`agent_memory::process`)

- **`Refusal::Interrupted`**: a new variant, appended last. Spawn and supervision return it once SIGINT has been recorded.
- **`install_termination_handler()`**: semantics changed (described under F1/F2).
- **`interrupted() -> bool`**: new. True once the SIGINT handler has run.
- **`exit_if_interrupted()`**: new. It does nothing when SIGINT has not been recorded. Otherwise it restores `SIG_DFL`, unblocks the signal, `raise`s it, and falls back to `_exit(128+sig)`.

`cli::input` changes:

- New `read_text_universal(path)`.
- The `read_text` doc is narrowed to `read_bytes().decode('utf-8')`.

## Findings

### F1: an inherited `SIG_IGN` stays ignored

**Change.** `install_termination_handler` first reads each signal's current disposition with `sigaction(sig, NULL, &old)`. If it is `SIG_IGN`, no handler is installed for that signal.

**SIGTERM decision.** CPython never touches SIGTERM.

- **Default on entry:** I install the handler. It kills and reaps the owned children, then dies by SIGTERM: `SIG_DFL`, unblock, `raise`, with `_exit(143)` only as an unreachable fallback.
  - The process still dies without running any cleanup, like the baseline.
  - The difference is that owned children never outlive `am`; this is the operator requirement.
  - The exit status is death by signal 15, which is the baseline's status. The old code gave `exit(143)`.
- **Ignored on entry:** it stays ignored, exactly as in the baseline. `am` keeps running, so its children cannot outlive it.
- Exec resets a caught handler to default, so default and ignored are the only dispositions `am` can inherit.

**Test.** `a_termination_signal_inherited_as_ignored_stays_ignored` runs through the binary. A `pre_exec` sets `SIG_IGN` before exec. The test covers both pairs (SIGINT ignored then SIGTERM sent, and the reverse). It checks:

- `am` and its owned git child are still running 300 ms after the ignored signal.
- The other signal then kills `am` by that signal number, and the owned child is gone.

### F2: SIGINT no longer skips cleanup

**Change.**

- **`interrupt` (SIGINT):**
  - Kills and reaps the owned children.
  - Stores the signal in `static INTERRUPTED: AtomicI32`.
  - Writes one byte to a self-pipe.
  - Restores the saved errno and **returns**.
- **`terminate` (SIGTERM):** keeps the immediate kill → reap → die behaviour.
- **Supervision:**
  - `spawn` checks `interrupted()` while SIGINT/SIGTERM are blocked, before reserving a slot or forking, and refuses with `Refusal::Interrupted` (no child is created).
  - `Active::advance` checks the flag before and after each step; once it is set the step returns `Refusal::Interrupted`.
  - `Group` reaches every query through `advance`, so `run`, `run_for` and `Group::value` all return `Interrupted`. Siblings are refused the same way.
  - Every guard on the stack then drops normally.
- **`main`:** calls `exit_if_interrupted()` after `cli::entry` returns and **before** writing any output. A Ctrl-C therefore prints no result and the process dies by SIGINT, the same as CPython re-killing itself after an uncaught `KeyboardInterrupt`. A shell loop calling `am` stops. `main` calls it a second time after writing, for a SIGINT that lands while printing.

**Audit of the blocking paths.** Handlers are installed with `sa_flags = 0`, i.e. without `SA_RESTART`, so a blocking call returns EINTR promptly. This is needed so that `poll` and a terminal `read` wake up. Path by path:

- **`Group::poll`:**
  - EINTR was already ignored: the next `sweep` → `advance` sees the flag.
  - A SIGINT that lands between the last flag check and the `poll` call is covered by the self-pipe's read end, which is now in the pollfd set (`3*MAX_CHILDREN+1` slots). The handler's write makes it readable, so `poll` returns at once. Without the pipe it could block until a deadline or until the pipes closed.
- **`Owner::observe` (`try_wait`) and `Owner::reap` (`wait`):**
  - EINTR is retried.
  - The handler may already have reaped the child, so a later `waitpid` returns ECHILD. With the flag set, ECHILD is now read as "collected by the handler": the new `Owner.collected` flag, and `done()` means status known or collected. It is not counted as a supervision fault, so `cleanup_error` stays `None`.
  - `Evidence.reaped` reports `done()`. `Output` is never built for a collected child, because `complete()` still requires a real status.
- **`Owner::cancel`:**
  - Blocks SIGINT/SIGTERM around observe and `kill`.
  - Otherwise the handler could reap the child between the supervisor's "still running" observation and `Child::kill`, and the kill could reach a recycled PID.
  - With the signals blocked, an unreaped PID cannot be reused.
- **Child pipes and stdin writes:** `drain` and the stdin write are non-blocking. Their `Interrupted` errors were already treated like `WouldBlock`; the step then returns `Interrupted` through the post-check.
- **`cli::input::bounded`:**
  - `read_to_end` retries every EINTR, so `am … --input -` at a terminal would have ignored Ctrl-C until EOF.
  - It is now a bounded manual `read` loop: EINTR is retried only while not interrupted. Once interrupted, it returns `[Errno 4] Interrupted system call`, and `main` then dies by SIGINT without printing.
- **Elsewhere:** flock is `LOCK_NB`, and std's `write_all` in `main` retries EINTR. The `fs::descriptor` EINTR retries are not my files; they read regular files and do not block on a terminal.

**Covering tests.**

- `sigint_during_developer_apply_releases_lock_and_temp_then_dies_by_sigint`, through the binary:
  1. The test busy-polls for `.developers.lock`.
  2. When it appears, the test `SIGSTOP`s `am`, waits with `WUNTRACED` until it is stopped, and confirms the lock **still exists while it is stopped**, which proves `am` is inside the lock window.
  3. It then sends SIGINT followed by SIGCONT.
  4. It asserts:
     - wait status `WIFSIGNALED`/`WTERMSIG == SIGINT`, not exit 130;
     - empty stdout;
     - no `.developers.lock` and no `.developers-*` temp file;
     - the next `am developer preview --create` exits 0.

  The test needs 3 in-window hits within 400 attempts and fails if it gets none.
- `sigint_kills_and_reaps_owned_children_then_dies_by_sigint` and `sigterm_…_dies_by_sigterm`, through the binary: `ExitStatus::signal() == Some(sig)` and `code() == None`; the owned child is gone; stdout is empty; a non-owned sibling survives.
- `sigint_handler_reaps_every_owned_child_itself_and_restores_errno`, a re-exec worker:
  1. The worker installs the handler, launches a 3-child `Group`, sets `errno = EDOM` and calls `raise(SIGINT)`.
  2. Right after `raise` returns (nothing but the handler has run), for each child PID: `waitpid(pid, WNOHANG) == -1/ECHILD` and `kill(pid,0) == -1/ESRCH`. The worker is the children's parent, so ECHILD proves that this process reaped them, i.e. the handler did. Init did not.
  3. errno is still `EDOM`.
  4. `value(0..3)` all return `Interrupted` with `cleanup_error == None`, `finish()` is `Ok`, and a later `run` returns `Interrupted` with no evidence and no child launched (a marker file is never created).
- `sigint_while_a_three_child_group_is_supervised_refuses_as_interrupted`:
  - The same worker, but SIGINT is sent with `pthread_kill` to the supervising thread while it is blocked in `group.value(0)`, which exercises the poll/EINTR path.
  - `value(0)` returns `Interrupted` within 10 s; the children run `sleep 30`.
  - The evidence says `reaped`, all three PIDs return ECHILD/ESRCH, and the remaining values are `Interrupted`.

**Mutation check.** I ran it once and restored the code afterwards. I made the handler's reap non-blocking (`WNOHANG`), removed the errno restore, and disabled the `SIG_IGN` skip. Three tests then failed: the reap worker, the SIGTERM test and the inherited-ignore test.

**Limit of the errno test.** In the normal flow the handler's own syscalls succeed, so the errno assertion does not drive a path where the handler would have clobbered errno. The save/restore is verified by reading the code.

### F3: `--` before a subcommand

**Change.** In the PARSER branch of `consume_positionals`, the choice is the argument after the leading marker, and the subparser receives `args[cursor+1..]`. Only the first marker is removed, as in argparse 3.12+.

**Covered by vector rows:**

- 25: `scope-policy -- model` exits 0 with the model JSON.
- 26: `developer -- bogus` gives `invalid choice: 'bogus'`.
- 27: `developer -- -- bogus` gives `invalid choice: '--'`.

### F4: ambiguity is raised when the option is consumed

**Change.** `Pattern::Optional` now carries every candidate. `classify` no longer fails. `consume_optional` raises `ambiguous option: <arg> could match …` only when it reaches that index.

Because a parent never consumes arguments its subparser owns, the latent parent-level refusal is gone too.

**Covered by vector rows:**

- 28: `-h --a` gives help, exit 0.
- 29: `--label --a` gives `expected one argument`.
- 30: `--a x -h` gives ambiguous, exit 2.

### F5: `writes preview --content` uses universal newlines

**Change.** New `input::read_text_universal`: strict UTF-8, then `\r\n`→`\n`, then lone `\r`→`\n`. It is used only at the `bank_writes` `--content` call site. `read_text` (record-contract `--file`) keeps the bytes unchanged, and its doc is narrowed.

**Test.** `content_files_are_read_with_universal_newlines_for_writes_preview_only` covers a CRLF file and a file with lone CRs (including `\r\r\n`), plus `read_text` byte preservation.

**Gap: no binary-level test.** Through the binary, `am writes preview` exits 1 with "Repository boundary security scan provider is not registered" before it uses the content. `repository_config::register_security_scan` waits for `security` per `tests/wiring-pending.txt`. Any bank also needs `init`, which is not ported. So a CRLF file and an LF file produce identical results today, and a binary test cannot distinguish them. Once `security` is registered and a bank fixture exists, a binary test should assert that the proposal `content` from a CRLF file and a lone-CR file equals the LF text.

### F6: a signal-mask failure is infrastructure

**Change.** A failing `TerminationBlock::new()` in `spawn` now maps to `Refusal::Infrastructure`, not `Spawn`. The block is acquired through `TerminationBlock::when_installed()`.

`identity_git::root_head` and `_git_output` treat it as `Error::Infrastructure`, which fails closed. `Refusal::Interrupted` is likewise neither `Spawn` nor `Deadline`, so neither function reads it as an ordinary absence.

**Test.** None: `pthread_sigmask` with valid arguments cannot be made to fail.

### F7: negative numbers and single-dash prefixes follow argparse

**Change.**

- `negative_number` implements `^-\d+$|^-\d*\.\d+$`:
  - `\d` is Unicode general category Nd. The table `DECIMAL_DIGITS` in parser.rs holds 64 ranges: Unicode 15.1, which is CPython 3.13 `unicodedata`. I derived it from `DerivedGeneralCategory-16.0.0.txt` minus the seven ranges Unicode 16.0 added (10D40, 116D0–116E3, 11BF0, 16130, 16D70, 1CCF0, 1E5F1).
  - `$` accepts one final `\n`.
- Single-dash prefix matching compares against the part before `=` and carries the `=` separator and explicit value, as `_get_option_tuples` does.

**Covered by vector rows:**

- 31–33 are accepted as negative numbers: `-٣`, `-.٥`, `-1\n`.
- 34–35 are refused: `-1\n\n`, `-¹` (category No).
- 36: `developer status -=x -h` gives `ambiguous option: -=x could match -h, --help, --observe, --harness, --session, --repo`, exit 2.

## Handler invariants (exact)

**What the handlers touch:**

- `OWNED` (3 × `AtomicI32`): load, then `compare_exchange(pid, 0)` after the child is collected.
- `INTERRUPTED` (`AtomicI32`): store; SIGINT handler only.
- `WAKE_WRITE` (`AtomicI32`): load, then `write(fd, 1 byte)`; SIGINT only.
- The thread's errno: saved and restored by the SIGINT handler.

Nothing else. In particular: no allocation, no lock, no formatting, no `Drop`, no std I/O.

**Why each call is async-signal-safe.** Each item below is on the POSIX async-signal-safe list or is a lock-free atomic on a static:

- Lock-free SeqCst atomics on statics, i.e. plain loads, stores and CAS.
- `waitpid`, `kill` (positive PIDs only, never a group), `write`.
- `sigaction`, `sigemptyset`/`sigaddset`, `pthread_sigmask`, `raise`, `_exit`.
- errno access through `__error()`/`__errno_location()`: returns a thread-local pointer, nothing else.

**Kill and reap per slot:**

- A slot holding 0 or `RESERVED` (-1) is skipped.
- `waitpid(pid, WNOHANG) == 0` proves the PID is this process's unreaped child. Only then does the handler call `kill(pid, SIGKILL)` followed by a blocking `waitpid(pid, 0)`, retried on EINTR.
- The slot is then cleared.

The supervisor never kills while the handler could interleave, because `Owner::cancel` blocks both signals. Spawn reserves a slot and records the PID under the same block. So a handler never sees a forked-but-unrecorded child, and never signals a recycled PID.

**Masking during a handler.** `sa_mask` = {SIGINT, SIGTERM}, so the handlers never nest. A SIGTERM pending behind SIGINT runs afterwards and dies.

**How the flag is observed** (single-threaded binary; the handler runs on the supervising thread):

1. `spawn` checks the flag while signals are blocked, before reserving a slot. A SIGINT arriving after that check stays pending until the PID is recorded, and then kills that child.
2. `Active::advance` checks it before and after each step.
3. `Group::poll` is woken by the self-pipe.
4. `Owner` reads ECHILD-after-interrupt as "collected".
5. `input::bounded` stops on EINTR once interrupted.
6. `main` checks it after `cli::entry` returns (before output) and again after output.

**Wake pipe.**

- Created once by `install_termination_handler` before any handler exists: `pipe` + `FD_CLOEXEC` + `O_NONBLOCK` on both ends.
- The write end is non-blocking, so the handler never blocks; a full pipe is already readable.
- The pipe is never drained, because every later step refuses anyway.
- `spawn` already marks every non-passed descriptor close-on-exec in the child.

**In a forked child before exec.**

- The handlers can run there: std unblocks the mask before `pre_exec`. `waitpid` returns ECHILD for the copied PIDs, and the child's own slot reads `RESERVED`.
- SIGINT returns, and the child then execs. The parent kills it once the PID is recorded.
- SIGTERM dies by SIGTERM; the parent then observes an exited child.

## Declared deltas (updated)

- **Termination status.**
  - Was: `exit(128+sig)`.
  - Now: death by the signal itself, for SIGINT after unwinding and for SIGTERM at once. This matches the baseline.
  - This supersedes the "Termination: `_exit(128+signal)`" delta in `lane-reports/cli-report.md` lines 44, 184 and 201. That file is not mine; the coordinator should update it.
- **SIGINT during a step that launches no child.** The baseline aborts at the next bytecode and runs its `finally` blocks. Rust finishes that step's atomic write, runs every guard, prints nothing and dies by SIGINT. For example, a SIGINT during `developer apply` after the lock is created may still publish the registry; the next `preview` sees the applied state. Both outcomes leave no lock, temp file or partial state behind.
- **Callers that convert a process refusal into an absence and continue.** Two exist:
  - `bank_state::git_context_with_environment` (`Err(_) => context_unavailable()`, src/bank_state.rs:106).
  - `release_distribution` (maps everything else to `"process-terminated"`, src/release_distribution.rs:1286).

  They are not my files. Each keeps going after a SIGINT until its next spawn or the end of the command. Every later spawn is refused, and `main` prints nothing and dies by SIGINT. If either should end immediately, its owner would check `process::interrupted()` / `Refusal::Interrupted`.
- **Coverage gap.** No registered `am` command launches a 3-child `Group` yet: `repository_config::settings` is reached only through providers that are still pending. The 3-child-group signal tests therefore use the in-process worker with the same library handler, not the binary.

## Commands run

Platform: `aarch64-apple-darwin`, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13).

Results:

| Command | Result |
|---|---|
| `cargo fmt --check` | OK |
| `cargo check --offline --all-targets --features dev-tools` | 0 warnings, 0 errors |
| `cargo test --offline --test t2_cli` ×3 | 21 passed each time |
| `cargo test --offline --test t0_process` ×3 | 34 passed each time |
| `cargo test --offline --no-fail-fast` | every target green; no failures (t1_project_identity 42 passed in 161 s; t2_cli 21; t0_process 34; t1_bank_transactions 25 passed, 2 ignored as before) |
| `cargo check --offline --all-targets --features dev-tools --target aarch64-unknown-linux-musl` and `--target x86_64-unknown-linux-musl` | finished, 0 warnings (compile only; not run on Linux) |
| `grep -n 'unwrap()\|expect(\|\[.*\.\.' <my modules>` | see below |

Grep result:

- `src/process.rs` keeps four pre-existing invariant `expect`s (lines 357, 1305, 1525, 1546) and test-module unwraps.
- `src/cli/input.rs:41` slices `bytes[filled..]` right after `resize`; a one-line comment names the invariant.
- Nothing else in `src/main.rs`, `src/cli/parser.rs` or `src/cli/helpers/bank_writes.rs`.

**Incident.** An exploratory shell command of mine exported `HOME` to a temporary directory, and the variable persisted in the session shell. Later, `cargo fmt` under that HOME made rustup install stable 1.100.0 into the temp directory, and one `cargo test` failed offline resolution. I restored `HOME`, deleted the temp directories, and re-ran `cargo fmt` and everything after it on rustc 1.99.1. A probe pipeline also contained a stray no-op `python3 -c 1`; it read no code and produced nothing used here.

## Checks for the coordinator

- `cargo test --offline --test t2_cli` (three times, for the signal tests) and `--test t0_process`.
- The whole suite on Linux aarch64 musl.
- `tests/t2_cli.rs` adds no obligation annotations, so the ledger is unchanged.
