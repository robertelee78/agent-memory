# x86_64 FD-passing test portability

## Boundary

Base: `e47eae56d786dcd2add08490398a31415c5c3322`, branch `lane/x86-process-pass-fds`. Integration and tracker writer: v0.0.15 owner, Session `01a1a3dd-2f1f-7204-be24-f68278cb7aff`; parent work `gwa:robertelee78/agent-memory:ADR-0004:omp-first-class-integration` (#249). Lane resource: `lane:v0.0.15:x86-process-pass-fds`.

## Root cause and evidence

The test used `echo x >&"$1"` with an FD allocated by `fcntl(F_DUPFD, 3)`. The minimum is not the resulting descriptor: concurrent tests and inherited descriptors can put it above 9. On this host `/bin/sh` is `/usr/bin/dash`, whose numeric duplication redirection rejects a multi-digit operand. This is a test-shell assumption, not evidence of a `close_range` or pass-FD defect.

A standalone shell diagnostic explicitly opened FDs 8 and 10 to private fixture files. `/proc/<parent>/fd/{8,10}` showed both open. The same shell command with FD 8 exited 0; with FD 10 it printed `Syntax error: Bad fd number` and exited 2. `strace -f -e trace=execve,fcntl,write` recorded:

```text
execve("/bin/sh", ["/bin/sh", "-c", "printf \"x\\n\" >&\"$1\"", "fd-probe", "10"], ...) = 0
write(2, "fd-probe: 1: ", 13) = 13
write(2, "Syntax error: Bad fd number", 27) = 27
write(2, "\n", 1) = 1
+++ exited with 2 +++
```

Changing only the diagnostic to `printf "x\n" > /dev/fd/"$1"` with the same inherited FD 10 exited 0 and wrote `78 0a`. The original suite failure did not record its allocated FD number; that particular number is not claimed as observed. The deterministic high-FD regression below exercises the faulty assumption directly.

## Changes

- `tests/t0_process.rs`: allocate the passed FD with minimum 64, so the multi-digit case is always exercised; write via `/dev/fd/"$1"` instead of the shell's limited numeric-duplication syntax; explain the portability boundary.
- This report.

No production source, public API, registration trait, CLI route, obligation ledger, crate, or foundation primitive changed. The existing test still checks the exact inherited FD number, exact written bytes, absence of an unpassed FD in the child, and preservation of that unpassed FD in the parent. No declared runtime contract delta or intentionally deferred implementation.

## Verification

Host: Ubuntu 26.04 LTS; Linux `7.0.11-76070011-generic` x86_64; rustc `1.94.1 (e408947bf 2026-03-25)`; `/tmp` tmpfs. Each test command below ran once after the edit:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Exit 0 |
| `cargo test --locked --test t0_process` | 40 passed, 0 failed, 0 ignored; exit 0 |
| `CARGO_TARGET_DIR=/tmp/agent-memory-beast-evidence-5daf7d3/musl-target cargo test --locked --target x86_64-unknown-linux-musl --test t0_process` | 40 passed, 0 failed, 0 ignored; exit 0 |

Evidence: `/tmp/agent-memory-beast-pass-fds-e47eae5/{diagnosis.txt,dash-fd10.strace,fmt.log,gnu.log,musl.log,gnu-status.txt,musl-status.txt}` on node `omp-beast`, shared with the integration owner. These are diagnostic/proof records, not runtime source or scripts. No Python executed. macOS and Linux aarch64 execution remain integration-owner acceptance work; this lane does not claim them.

## Independent review

`ReviewFdTestFix` reviewed the named test, its helpers, descriptor validation/close-on-exec code, and the diagnostic evidence without running checks. Verdict: no findings; exact descriptor-number/file-identity checks and hidden-FD checks remain intact. macOS `/dev/fd` semantics were reviewed but not executed here. On Linux the path redirection reopens/truncates the file rather than sharing its offset; this test starts with an empty file and does not claim offset-sharing coverage.

The reviewer also identified the same shell syntax in the separate `close_range_falls_back_only_for_compatibility_errnos` test. It passed in both original runs, but remains susceptible if its isolated worker inherits enough descriptors to allocate a multi-digit FD. That test is outside this lane's assigned file-region ownership; the integration owner is notified rather than this lane silently widening scope.
