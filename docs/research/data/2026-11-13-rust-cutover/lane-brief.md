# Rust cutover: lane brief

Working rules for anyone porting or reviewing a slice of the v0.0.15 Rust cutover. Decision: `docs/adr/0029-rust-only-runtime.md`. Design (read it fully first): `docs/design/v0.0.15-rust-cutover.md`. Import graph and tiers: `import-graph.json`, `tier-assignment.json`, `json-codec-call-sites.json` in this directory. Obligation ledger: `tests/obligations/python-baseline-v1.json`. Earlier lanes' reports (public API, registration points, declared deltas, deferred tests): `lane-reports/` in this directory.

## Ownership and integration

- The integration branch is `migrate/rust-only-v0.0.15`. One coordinator (the v0.0.15 owner session) is its only writer: it merges lanes, runs macOS arm64 and Linux aarch64 musl, and commits.
- A lane owns the files named in its assignment and nothing else. Work from the current head of the integration branch on a branch `lane/<name>`; push that branch; never push to the integration branch. Put your report on the lane branch at `docs/research/data/2026-11-13-rust-cutover/lane-reports/<name>-report.md`.
- If a file you do not own blocks you, tell the coordinator with file:line and evidence. Shared files that several lanes extend (`src/lib.rs`, `src/error.rs` `Kind`, registration traits in `src/bank_contract.rs` / `src/bank_writes.rs`, `src/fs/descriptor.rs`) are edited append-only so they merge three-way: add at the end of the enum/trait/impl, never reorder or reformat existing items.
- Never execute Python, not even to inspect behaviour: read it as a specification. No new crates without the coordinator's approval. No `build.rs`. No Python parser, ever.

## What to run

- `cargo fmt --check`; `cargo check --offline --all-targets --features dev-tools` with no warnings in your files; your own test target green three times in a row; then the whole suite with `cargo test --offline --no-fail-fast`.
- Say exactly which platform(s) you ran on (triple, kernel, rustc). Code must build for `aarch64-apple-darwin`, `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`: use only libc symbols present on both macOS and linux-musl, or cfg-gate them.

## Porting rules

- Baseline Python is the specification: `skills/agent-memory/scripts/agent_memory/<module>.py`, schemas under `skills/agent-memory/schemas/`, templates, and the contract tests in `tests/contracts/`. Read each module completely before porting. Do not edit or delete Python.
- One Rust module per baseline module, same name, `src/<module>.rs`, registered with one `pub mod` line in alphabetical position in `src/lib.rs`.
- Intended behaviour is reproduced exactly: accepted inputs, refusals and refusal strings, JSON field names/shapes/order, on-disk bytes of durable records (use the `codec::json` style matching the baseline `json.dumps` keywords at that call site), file modes, lock paths and lock class (`flock`), bounds, fsync and rename order.
- Accidental Python behaviour is not reproduced: non-UTF-8 text, UTF-16/32 or BOM JSON, unbounded reads, interpreter-version-dependent output (argparse help layout, traceback text, set ordering, float repr outside the codec). Refuse such input with a typed refusal or pick the deterministic behaviour, and list it under "declared deltas" in your report. Decide yourself.
- No panic on input. `unwrap()`, `expect()`, indexing and slicing are forbidden on anything derived from files, proposals, arguments, environment, child output or registry state; where the baseline raised, return the same refusal. They are allowed only for an invariant established in the same function (one-line comment naming it) and for lock poisoning. Report the result of `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/<your modules>`.
- Every read is bounded by an explicit named limit. Every child process has the finite deadline of the baseline `timeout=` at that call site. Children are owned direct children: at the deadline kill and reap that child; never signal process groups.
- System access only through the foundation: `agent_memory::fs` (pinned descriptors, trust paths, locks, native rename, bounded reads, inventory, metadata, `canonical_path`), `codec::{json,frontmatter}`, `process`, `clock`, `host` (uuid4, UTC, hostname, pid state, expanduser, `Sources` for injectable id/clock), `unicode`, `error`. No weaker path-based `std::fs` call on trust-relevant paths. `unsafe` only in `src/fs`, `src/process.rs`, `src/clock.rs`, `src/host.rs`. If the foundation lacks a primitive, add the minimal one to the proper foundation file with tests and list it under "foundation changes".
- A baseline import of a module that is not yet ported (see `src/lib.rs`) becomes a small registration trait in your module; a missing registration is a typed refusal, never a silent pass. Reuse a trait that already exists for the same callee (see `lane-reports/`): add required methods at its end, no default bodies; provider accessors are `pub(crate) fn <trait_snake_case>_provider() -> Result<Arc<dyn Trait>>` placed after the `register_*` function. No stubs, `todo!()`, placeholders or fake fallbacks.
- CLI: do not port argparse. Port each subcommand as a typed public function returning the result value, exit code and stdout/stderr text, and register it in `src/cli/table.rs` following the recipe in `lane-reports/cli-report.md`.
- Pack schemas and templates: `include_str!` from their existing paths, decoded strictly; never copied into Rust literals. Single definitions; reuse existing helpers.
- Readable code: one statement per line, a named helper per baseline function.

## Tests

- `tests/<lane>.rs`. Real filesystem and process behaviour in private 0700 directories. On macOS `/tmp` is a symlink to `/private/tmp`: canonicalise temporary roots. Tests never depend on the user's HOME, Git configuration, network or credentials, and are safe under parallel `cargo test`.
- Port the baseline contract tests that exercise your modules. Put `// obligation: tests/contracts/<file>.py::<Class>.<method>` (exact ledger id) above each ported test. Expected bytes and digests come from the baseline and committed files, never from your implementation.
- A test that needs a module not yet ported stays in the file with `#[ignore = "needs <modules> registered"]` and its obligation line; never fake a provider.
- `tests/obligations_ledger.rs` fails when an annotated obligation is still `unported` in the ledger. After adding tests run `cargo run --offline --features dev-tools --bin am-dev -- obligations mark --status <ported|partial> --ids-file <file>` and commit the ledger change on your lane branch.

## Report

Files created or modified (exact list, including shared files); public API; registration traits introduced or extended and their future implementer; CLI declarations; obligations fully ported, partially ported (what is missing), deferred (waiting for which module); declared deltas; foundation changes; the exact commands you ran, on which platform, with final results. A hand-over that says "partial" lists exactly which baseline functions or branches are not ported: an unlisted gap is a defect.
