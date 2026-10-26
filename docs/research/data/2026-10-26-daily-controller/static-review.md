# Effective-target offline adapter: bounded static approval

Reviewer: **codex-review-59**, independent of **codex-root**. Candidate **d5b4f59621cd90cc0552e44734f4e018d5fbf0f3**, tree **473f3be89cd9816c55d748f252da75bf3ad439d5**, compared with **5669e4fb94bcbeafdd0c1834d66ebff1ed5ced1f**. Existing work: `robertelee78/agent-memory:ADR-0009:daily-use-reliability`.

**APPROVED as a bounded static apparatus repair. All five previously established controller coverage blockers are now addressed in the reviewed source; no further concrete blocker found in this delta.** This permits proceeding to the fresh pilot and required qualification. It does not establish that they pass, accept the product or whole #99 story, or erase the aborted/continued earlier diagnostics.

## The actual Git target is now selected

The generated adapter starts from actual cwd, consumes leading `-C <path>` arguments, and resolves repeated relative paths against the effective target accumulated so far. An absolute path replaces that prefix; an empty `-C` leaves the existing target. The supported concatenated `-Cpath` spelling is also recognized. Leading `-c`/`--config-env` consume their value rather than treating it as a subcommand; fixed one-token global switches are skipped. The first actual command then determines whether the selected checkout's fetch/push/ls-remote is denied.

This covers the actual product `sharing_recovery.raw_command` shape: no subprocess cwd change, fixed global switches and config arguments, `-C <actual session root>`, then `ls-remote` or `fetch`. It also preserves the no-`-C` cwd case. Other worktree targets, local Git inspection and the private bare authority itself delegate to the original adapter. A word such as `fetch` inside a config value or later ordinary argument no longer chooses the network operation. This is a finite fixture parser for the supported invocations, not a production-complete parser for every Git global option.

I independently reconstructed the factory's literal source with inert path/interpreter stand-ins and parsed its AST without evaluating controller/product code. The generated string/newline syntax is valid. Paths are inserted using Python string literals; original arguments are passed unchanged to the retained POSIX shell wrapper through `/bin/sh`, with no shell interpolation or new transport authentication. The old wrapper still performs the fixture HTTPS-to-file mapping and invokes its retained real Git.

## Actual denials, interval and restoration

Only a matched effective session-0 target and network command appends a private denial record, with actual target, operation, original arguments and monotonic time. The log is opened append-only with an owner-only creation mode. The generated newline is an actual JSONL delimiter. `offline_start` now requires both pending/unfresh product output and a nonempty matching-target denial log; a different pending reason cannot alone impersonate a successful injected outage.

The adapter remains installed through the verified reverse-order peer landings. Rejoin first requires precisely all peers 1..N-1, performs another actual unavailable observation, and requires the denial count to increase before recording its event. Only then does it remove the flag and restore exact original wrapper bytes/mode before session 0's landing/reconciliation. The later event retains the denial log by content hash. The cwd/`-C` mismatch identified at 5669 is therefore closed in the actual dataflow rather than patched by changing product commands.

## Other four repairs remain intact

The `public`, `review`, `require_scenario_events` and `capture` function source segments are byte-identical to the reviewed 5669 versions. They retain:

- Positive wall/withheld body and dependent-route holds, developer attribution, the strict allowed-component upper bound, and mandatory scenario/phase/session event completion.
- Exact ordinary admitted metadata/body/mode checks, seed hashes/IDs and initial per-session modes, with the narrow exact-tombstone final deletion exception. This delta strengthens seed enumeration to recursive `rglob`, catching unexpected nested Markdown records instead of checking only the top directory.
- Both active negative-fixture markers checked across actual public stdout/stderr, while raw private evidence remains distinct.
- Attempted argv/environment/input receipts, exit null, exception/timeout chain and available streams on raised execution failure; re-raise without a diagnostic product retry. The ordinary 300-second worker deadline and owned cleanup are unchanged.

There are no portable/runtime changes and no scenario/deadline/size reduction in this delta. Native product qualification remains fresh work; the parser and extra assertions may still expose an apparatus expectation or real product failure during the pilot, and those outcomes must remain recorded distinctly.

## Probe evidence is qualified

The six-control adapter summary binds the exact current controller SHA **f48fe78daf796a8cfcc1527280b62d82d6c2988a7a995cbf0d4c83730cf73a76** and explicitly reports zero memory-product commands/no whole-workload qualification. The supplied adapter directory contains that summary, not a complete per-control command transcript; I do not adopt its flags as independently executed product proof. My approval rests on the source/dataflow review.

I also inspected the two retained simulated receipt rows from the 5669 retention probe and authenticated their compressed stream hashes. They show an exit-null chained timeout/partial-stream receipt and a normal return flagged for forbidden public output. Their wrapper function is unchanged here. These are apparatus simulations, not native 300-second timeout tests or memory-product commands. I executed neither probe.

## Release

`static-review.source.json` binds the exact commit/tree, four inspected source files, preserved function segments, inert adapter parse/hash and retained probe-file hashes. No product imports, execution/tests/metaprobes/models, live runtime/bank/key reads, source edits or external writes occurred. All own read subprocesses are terminal and collected; no Worlds or background jobs exist. **All reviewer holds are RELEASED. External writer grant: NONE / RELEASED.**
