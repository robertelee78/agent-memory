# Review: CLI, console and signal handling (commit e47eae5)

Scope: `git diff 5daf7d3 e47eae5` for `src/process.rs` (owned-child registry and termination handler), `src/cli/**`, `src/console.rs`, `src/main.rs`, the `lifecycle::command` signature, `tests/t2_cli.rs` and `tests/vectors/cli-argparse-v1.json`. I compared it against the baseline `cli.py`, `console.py`, `installation.py` (`MEMORY_ROUTES` and `main`), the helper `main`s, and CPython 3.13 argparse. I read the 3.13.13 `argparse.py` source; the vectors were captured with 3.13.12, and the relevant code is identical in 3.12.13 and 3.13.13. Nothing was built or run.

## Findings

### F1 (P2): the handler replaces an inherited `SIG_IGN` for SIGINT and SIGTERM
- **Where:** `src/process.rs:136-154` (`install_termination_handler`). It installs `terminate` unconditionally.
- **Baseline:** CPython installs its SIGINT handler only if the inherited disposition is `SIG_DFL` (`signalmodule.c`: `if (sigint_func == DefaultHandler)`). It never touches SIGTERM. An ignored signal therefore stays ignored for the interpreter and for its children (exec keeps `SIG_IGN`).
- **Failing case:** a non-interactive script runs `am developer apply --proposal p.json & wait`. POSIX `sh` sets SIGINT to `SIG_IGN` for the background job. A terminal Ctrl-C reaches the same foreground process group.
  - Baseline: keeps running and completes.
  - Rust: dies mid-operation with status 130 and SIGKILLs its git children, which in the baseline would also have ignored the signal.
  - The same applies to supervisors that start `am` with SIGTERM ignored.
- **Fix:** for each signal, call `sigaction(signal, NULL, &old)` first. If `old.sa_sigaction == SIG_IGN`, skip installing the handler for that signal. The registry and spawn blocking are unaffected.

### F2 (P2): `_exit` in the handler skips the cleanup the baseline's SIGINT path runs
- **Where:** `src/process.rs:112-130` (`terminate`), which calls `_exit(128 + signal)` from wherever the main thread was interrupted.
- **Baseline:** SIGINT becomes `KeyboardInterrupt`, so every `finally` block runs. Example: `developer_identity.py:291-301` deletes the temp file and the owned `.developers.lock`. Only SIGTERM in the baseline dies without cleanup.
- **Rust side:** the equivalent cleanup is in `Drop` impls (`src/developer_identity.rs:625-654`, `OwnedLock` and `Temp`). `_exit` never runs them.
- **Failing case:** Ctrl-C during `am developer apply` after the lock file is created (`developer_identity.rs:749`).
  - Rust leaves `.developers.lock` and possibly `.developers-*` behind. The next `am developer preview --create` exits 1 with "Attribution registry is locked; inspect status/recover before retrying" (`developer_identity.rs:542-544`) until `developer recover` is run.
  - The baseline leaves no lock, and the next preview succeeds.
  - Every RAII guard in later-ported write paths (temps, staging, path locks) has the same exposure.
- **Exit status:** `_exit(128+sig)` also turns the baseline's death by signal (CPython re-kills itself with SIGINT after an uncaught `KeyboardInterrupt`) into a normal exit. Bash's wait-and-cooperative-exit then keeps a calling script's loop running after Ctrl-C. This is a declared delta, but that consequence is not mentioned in it.
- **Minimal fix (keeps async-signal safety):**
  - SIGTERM: keep the current immediate kill-reap-exit; it matches the baseline.
  - SIGINT, in the handler: SIGKILL and reap the owned children, store the signal in a static `AtomicI32`, and return. Note `sa_flags` is 0 (no `SA_RESTART`), so the main thread's blocked syscall returns `EINTR`.
  - `Active::advance`/`Group::poll` and `spawn`: refuse with a typed `Refusal::Interrupted` once the flag is set, so the domain call fails and guards run.
  - `main.rs`: once the result has unwound and before writing output, if the flag is set, call `signal(sig, SIG_DFL)`, unblock it, and `raise(sig)`, with a fallback of `_exit(128+sig)`.

### F3 (P2): a `--` before a subcommand is taken as the subcommand name
- **Where:** `src/cli/parser.rs:655-657`: `let selected = args.get(index)…` ("PARSER nargs keep a leading marker as the selected choice"). Also line 667, which passes `args[index+1..]` on to the subparser.
- **Baseline:** argparse 3.12+/3.13 `consume_positionals` does `if action.nargs == PARSER: if arg_strings_pattern[start_index] == '-': args.remove('--')`. The choice is therefore the argument after the marker. What the code comment describes is argparse ≤3.11 behaviour.
- **Failing cases:**
  - `am scope-policy -- model`: baseline exits 0 and prints the model JSON. Rust exits 2 with `am scope-policy: error: argument command: invalid choice: '--' (choose from model, decide)`.
  - The same happens for `am developer -- status`, `am writes -- inspect`, and others.
- **Fix:** use `let selected = value.as_str();` and `let rest = args.get(cursor + 1..).unwrap_or(&[]);`, correct the comment, and add a vector row.

### F4 (P3): ambiguous long-prefix errors are raised while classifying, not when the option is consumed
- **Where:** `src/cli/parser.rs:414-421` together with `484` (`classify(...).map_err(fail)?` inside the pattern pass).
- **Baseline:** 3.13 `_parse_optional` returns every candidate tuple. `consume_optional` raises `ambiguous option` only when it reaches that index.
- **Failing cases:**
  - `am developer preview -h --a`: baseline processes `-h` first, prints help and exits 0. Rust exits 2 with `ambiguous option: --a could match --adopt, --alias`.
  - `am developer preview --create --label --a`: both exit 2, but the baseline message is `argument --label: expected one argument`.
  - Latent: arguments owned by a subparser are also classified by the parent in Rust. A parent that declares two long flags sharing a prefix would refuse those arguments eagerly. Argparse never would, because PARSER consumes them.
- **Fix:** keep every candidate in `Pattern::Optional`, and raise the ambiguity in `consume_optional`.

### F5 (P2): `writes preview --content` reads the file without universal-newline translation
- **Where:** `src/cli/helpers/bank_writes.rs:93` uses `input::read_text` (`src/cli/input.rs:56-59`). Its doc comment claims it equals `Path(path).read_text(encoding='utf-8')`.
- **Baseline:** `bank_writes.py` `main` calls `Path(args.content).read_text(encoding="utf-8")`. Text mode with `newline=None` turns `\r\n` and lone `\r` into `\n`.
- **Failing cases:**
  - A CRLF Markdown record: baseline preview exits 0 with LF content. Rust exits 1, because `codec/frontmatter.rs:177` requires the text to start with `"---\n"`.
  - Body with a lone `\r` (LF elsewhere): the Rust proposal content keeps the `\r`, where the baseline has `\n`. `apply` then writes different bytes.
- **Fix:** add a universal-newline variant (`\r\n` and then lone `\r` → `\n`) and use it only for `bank_writes` `--content`. Narrow the `read_text` doc to `.read_bytes().decode('utf-8')`, which is what `record-contract --file` needs.

### F6 (P3): a failure to block signals is reported as `Refusal::Spawn`
- **Where:** `src/process.rs:872-874`.
- **Effect:** `identity_git::root_head` (`src/identity_git.rs:222-228`) treats `Spawn`/`Deadline` as "no repository", which is a successful absence. A registry or signal-mask failure would therefore become a fail-open absence instead of an infrastructure error. `pthread_sigmask` with these arguments cannot practically fail, hence P3.
- **Fix:** map it to `Refusal::Infrastructure`.

### F7 (P3): classification edge cases that differ from argparse
- **Negative numbers:** `src/cli/parser.rs:331-342` (`negative_number`) accepts ASCII digits only. Argparse uses `^-\d+$|^-\d*\.\d+$`, where `\d` matches Unicode digits and `$` also matches before a trailing `\n`. `am developer preview --create --label -٣` succeeds in the baseline (the label is `-٣`); Rust exits 2 with "expected one argument".
- **Single-dash prefixes:** `src/cli/parser.rs:389-412` compares single-dash prefixes against the whole argument (`flag.starts_with(arg)`). Argparse uses the part before `=`, so `-=x` matches every flag and is ambiguous. `am developer status -=x -h` exits 2 in the baseline and 0 in Rust.
- **Fix:** compare against the part before `=`, and accept Unicode decimal digits plus an optional final `\n`.

## Signal-handling checklist (verified, no defect)

- **Async-signal safety:** the handler only reads static atomics (lock-free), calls `waitpid` with an errno load through `__error`/`__errno_location`, calls `kill`, and calls `_exit`. It does no allocation, takes no lock, does no formatting and runs no `Drop`. `128 + signal` cannot overflow.
- **errno:** it is not saved or restored. That is correct, because the handler never returns. If F2's fix makes SIGINT return, errno must be saved and restored there.
- **`sa_mask`:** it contains both SIGINT and SIGTERM, so the handler is not reentered if both arrive together. `SA_RESTART` is not set, which only matters if the handler returns (F2).
- **Signal between fork and registration:** this cannot happen. SIGINT/SIGTERM are blocked in the spawning thread from before `reserve_owned` until the PID is stored. The `am` binary runs no other thread (no `thread::spawn` in `src`), so delivery cannot slip to another thread.
- **Mask restoration on error paths:**
  - `OwnedCapacity`, spawn failure (slot reset to 0 before the guard drops) and the post-spawn `refuse` paths all restore the mask through the RAII `TerminationBlock`.
  - The previous mask is restored, not cleared.
- **Child between fork and exec:**
  - std empties the child's mask before the `pre_exec` closures run, so `terminate` can run in the child.
  - There `waitpid` returns `ECHILD` for every copied sibling PID, and its own slot reads `RESERVED`. The child therefore only `_exit`s with 128+sig, which is benign.
  - Exec resets the handler to `SIG_DFL`.
- **Reap versus slot release, PID reuse and double reap:**
  - The handler signals only after `waitpid(pid, WNOHANG) == 0`. An already-reaped PID gives `ECHILD`, and a recycled PID can only belong to a new child of ours, which is owned anyway.
  - The main thread never resumes after the handler, so there is no double reap.
  - `release_owned` clears every matching slot.
- **No group signals:** only positive PIDs are passed to `kill`.
- **Full registry:** refused before fork. `OwnedCapacity` is not `Infrastructure`, `Spawn` or `Deadline`, so `_git_output` and `root_head` treat it as `Error::Infrastructure`, which fails closed.
- **3-slot sizing:** true today.
  - `repository_config.rs:362` launches the largest group (3); `project_identity.rs:406` launches 2.
  - Neither group body spawns.
  - `run`/`run_for` are single-child.
  - It is enforced only at runtime, by the fail-closed refusal. There is no static check that a group body never spawns.
- **macOS vs musl:** every libc item used exists on both (`sa_sigaction`, `__error`/`__errno_location`, `pthread_sigmask`). I found no platform difference.

## Parser, dispatch, console and surface (verified)

- **Matches the vectors and 3.13 argparse:**
  - `--opt=value`.
  - Unique long prefixes.
  - `--` handling for ordinary positionals and options, apart from F3.
  - `-h`/`-hX`/`-h=x`.
  - `store_true=` ("ignored explicit argument").
  - Required mutually exclusive groups and conflict order.
  - `append` with choices.
  - Option-looking values and values containing spaces (rows 14/15).
  - Repeated options.
  - How unrecognized arguments propagate between parser levels (row 9/16 order).
- **`cli::arguments` and `console::options`:** equal to the baseline, including keeping `--json` after `--` and keeping `--label=--json` intact.
- **Routes:** a route only ever reaches a static `fn` handler. Input paths are only read (`--input`/`--file`/`--pack`), never executed.
- **Intent mapping and prog:** `intent` for correct/supersede/revoke/forget matches `cli.execute`, and nested routes get none. The prog rewrite (`am <route>`, unchanged when a prefix is set) matches.
- **Modes:** an installed basename gives console mode; otherwise `install --bootstrap-binary` at argv[1..2] gives bootstrap mode; anything else gives direct mode.
  - Spoofing bootstrap mode only enables the guarded install→update rewrite, which a user can request directly anyway. It is not harmful.
  - install.sh runs the binary as `am-<triple>`, so it reaches bootstrap mode as intended.
- **Console output:** console stderr and argparse exits are passed through `safe(lines=True)`. Direct mode is raw, as in the baseline.
- **`printable`/`repr`:** match CPython for the tested categories; the grapheme-extend escape is avoided by the leading `a`.
- **EPIPE:** `main.rs` writes with `write_all`/`flush`, and std ignores SIGPIPE. There is no print macro in `src`. A closed stdout gives exit 1 and never a panic.
- **Surface:** `Declared::current()` is built only from `declaration()` over `table::ENTRIES`, and `register_providers` registers only that. The surface-equality test covers commands, flags, routes and prefixes.
- **`lifecycle::command`:** the new signature routes read/decode errors into the lifecycle envelope. `guide` and `definition` never read `--input`, as in the baseline.

## Tests too weak to catch plausible regressions

- **`tests/t2_cli.rs:814-820`:** "not alive after the binary exits" cannot tell "killed and reaped by `am`" apart from "killed, then the zombie was reparented and reaped by init". A handler that drops the blocking `waitpid` usually still passes, depending on timing. Assert the reap from inside the binary instead, for example with a test-only subreaper (`PR_SET_CHILD_SUBREAPER` on Linux) or by making the test process the child's parent.
- **Signal coverage:**
  - The signal tests cover one `run_for` child. Nothing signals while a 3-child `Group` is live. The capacity worker checks registration and release, not that group children are killed.
  - There is no test with SIGINT inherited as ignored (F1).
  - There is no test that Ctrl-C during a write leaves no lock or temp file (F2).
- **Vectors:**
  - There is no row for a leading `--` before a subcommand (F3), for `-h` followed by an ambiguous prefix (F4), or for `-=x` / Unicode negative numbers (F7).
  - Rows 1/2 are checked only for usage prefix and message shape.
- **Input:** no test feeds a CRLF `--content` file to `writes preview` (F5).

## Verdict

The termination handler is async-signal-safe, and the owned-child registry is race-free for the single-threaded `am` binary. The 3-slot bound holds for every current call site and fails closed. Dispatch can never execute a path from input, and the surface is canonical.

It is not ready to merge as is. Fix F1 (inherited `SIG_IGN` overridden), F2 (SIGINT `_exit` skips the lock and temp cleanup the baseline performs), F3 (`-- <subcommand>` refused where 3.13 argparse accepts it) and F5 (CRLF `--content` refused or changed). Then add the tests listed above. F4, F6 and F7 are small P3 follow-ups.
