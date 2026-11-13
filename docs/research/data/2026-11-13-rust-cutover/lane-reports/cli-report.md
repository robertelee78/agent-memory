# CLI lane report (v0.0.15 Rust cutover)

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-cli` (snapshot `5693675`, plus the committed files of `8758d6b`/`7622e91` that Main allowed me to copy). No Git writes.

## Files

Created (owned):
- `src/cli/mod.rs`: dispatch core. `Helper`, `Route`, `Native`, `Entry`, `Context`, `Handler`, `Dispatched`, `arguments`, `intent`, `dispatch`, `execute`, `refusal`, `PackRoot` + `register_pack_root`/`pack_root`, `register_providers`, `entry`.
- `src/cli/parser.rs`: the declarative argparse-compatible engine.
- `src/cli/table.rs`: `ENTRIES` (the command table) and `route_named`.
- `src/cli/surface.rs`: `declaration()`, `canonical_bytes()`, `Declared` (implements `pack_validation::CommandSurface`), `SCHEMA`, `ROOT_SCRIPT`, `ROOT_MODULE`.
- `src/cli/input.rs`: bounded caller-input reads (`INPUT_MAX_BYTES` = 64 MiB) with baseline `OSError` text.
- `src/cli/helpers/{mod,developer_identity,scopes,authority,records,lifecycle,artifact,pack_validation,bank_contract,bank_layout,bank_writes}.rs`: one parser declaration and typed handler per baseline helper.
- `src/console.rs`: port of `console.py`.
- `src/main.rs`: the `am` binary.
- `tests/t2_cli.rs` (14 tests, one of which is a re-exec worker) and `tests/vectors/cli-argparse-v1.json` (Main's baseline capture, copied byte-for-byte).

Modified:
- `Cargo.toml`: `[[bin]] name = "am", path = "src/main.rs"`.
- `src/lib.rs`: `pub mod cli;` and `pub mod console;`, each in its alphabetical position. The rest equals int at `dcf1367`; int's later `pub mod project_identity;` (from `b7d1518`+) is not in this worktree.
- `src/process.rs` (T0): rebased on int's file at `dcf1367`, so it includes `DESCRIPTOR_HEADROOM` from 4de6a76. Diff against int: about 200 added lines and none removed. The exception is rustfmt reflowing `Refusal::CaptureLimit { … }` onto several lines, triggered by the new variant's doc comment.
- `src/lifecycle.rs` (Main-approved): `command(operation, request: Result<Option<Value>>)`, so `--input` read/decode failures reach the lifecycle error envelope. `tests/t1_a1.rs`: its 5 call sites were adapted (`Ok(None)` / `Ok(Some(request))`).

Copied unchanged from `/opt/agent-memory-worktrees/rust-v015-int` as read-only dependencies, at `dcf1367`:
- `src/{artifact,bank_contract,bank_layout,bank_writes,developer_identity,handoff,host,local_registry,migration_codecs,migration_contract,omp_installation,pack_validation,records,scopes,sharing_limits}.rs` and `src/fs/{descriptor,lock,resolve}.rs`;
- `skills/agent-memory/schemas/migrations.json`;
- `tests/{t0_fs,t0_host,t0_unicode,t1_a2,t1_bank,t1_migration,t2_omp_install}.rs`.

## T0 change (process.rs)

- `pub fn install_termination_handler() -> io::Result<()>`: sets SIGINT/SIGTERM handlers. Each handler masks both signals while it runs.
- New `Refusal::OwnedCapacity`: every registry slot is in use. It is returned **before fork**, so no untracked child is ever launched.
- Private registry: `OWNED_SLOTS = MAX_CHILDREN = 3` `AtomicI32` slots. The source comment explains the size: the largest concurrent set of owned children at any baseline call site is one `Group` of 3 (repository_config's config/listed/ls-tree batch), and no call site spawns while a group is live. Slot states: 0 free, `RESERVED` (-1) fork pending, PID > 0.
- `spawn` (only once the handler is installed):
  1. Blocks SIGINT/SIGTERM in the spawning thread.
  2. Reserves a slot by CAS 0→RESERVED; if none is free, refuses `OwnedCapacity`.
  3. Forks/execs; std resets the child's signal mask before exec.
  4. Stores the PID, or 0 if spawn failed.
  5. Unblocks.
- A slot is cleared when `try_wait`/`wait` collects the child's status, or when waitpid reports ECHILD for it.
- Handler: every line group is commented as async-signal-safe. It uses static atomics, `waitpid` with errno read through `__error`/`__errno_location`, `kill` and `_exit`; no allocation, lock or formatting. For each slot with PID > 0:
  1. `waitpid(pid, WNOHANG)`.
  2. Only if that returns 0 (still our unreaped child): `kill(pid, SIGKILL)`, then a blocking `waitpid`.
  3. After all slots, `_exit(128 + signal)`, i.e. 130 for SIGINT and 143 for SIGTERM.
  
  A PID that was already reaped gives ECHILD and is skipped, so a recycled PID is never signalled. RESERVED and free slots are never signalled. The handler never uses `kill(0)` or negative PIDs, so no process group is signalled.
- Invariant (stated in the source): the binary supervises owned children on one thread, which the handler interrupts.

## Public API

- `cli::entry(argv0, args) -> (i32, Transcript)` picks the mode:
  - argv0 basename `am`/`agent-memory` → `console::main(prog = name)`;
  - argv `install --bootstrap-binary …` → console in bootstrap mode, prog `am`;
  - anything else → direct (machine) mode: raw stdout/stderr, like baseline `installer.py`.
- `cli::dispatch(args, prog) -> Dispatched { result: CommandResult, exited }`. `exited` marks argparse's help/usage SystemExit.
- `cli::execute(route, args, prog)` follows baseline `cli.execute`: an unprefixed route under `am`/`agent-memory` uses prog `am <route>`; otherwise the prog is unchanged. A helper's own nested `routes` are checked before its parser runs, keeping the prog, as baseline `bank_contract.main` did.
- `cli::register_providers()`: the composition root. It currently registers `pack_validation::register_command_surface(Declared::current())`.
- `console::{installed_name, printable, repr, text, safe, help_text, options, bootstrap_command, render, main, Transcript, Channel, NAMES, HUMAN, InstalledReceipt, register_installed_receipt}`.
- `cli::parser::{parse, usage, help, error, Parser, Subcommands, Subcommand, Arg, Opt, Positional, Exclusive, Action, Choices, Default, Matches, Value, Exit}`.

## Traits introduced and who implements them

| Trait | Registration | Implementer |
|---|---|---|
| `cli::PackRoot` (`pack_root() -> Result<PathBuf>`): runtime pack location, used as the default for `--pack` | `cli::register_pack_root` | installation (T2e). Unregistered → `<prog>: Pack location is not registered for this runtime; pass --pack PATH`, exit 1 |
| `console::InstalledReceipt` (`receipt() -> Result<Option<Value>>`): `locations()` + `read_receipt()` for the bootstrap rewrite; `Err` = the baseline's caught exceptions | `console::register_installed_receipt` | installation. Unregistered → `am: Installed receipt provider is not registered; bootstrap cannot select update`, exit 1 |
| `pack_validation::CommandSurface` | implemented by `cli::surface::Declared`; registered by `cli::register_providers` | done |
| `bank_contract::RouteTable` | not registered yet | CLI, once `layout_recovery` exists: implement it with `cli::execute(recover-layout route, args, "am bank")` and add the nested route (see recipe) |

## Old script → `am` command table (Main-approved naming)

Rule: a helper script that has a `MEMORY_ROUTES` name keeps only that route. A helper-only script becomes `am <stem with _ replaced by ->`.

| Old script | Module | `am` command | State |
|---|---|---|---|
| developer_identity.py | developer_identity | `am developer` | wired |
| project_identity.py | project_identity | `am project` | waits for module |
| scope_binding.py | scope_binding | `am identity` | waits |
| initialize.py | initialization | `am init` | waits |
| bank_state.py | bank_state | `am inspect` (prefix `inspect`) | waits |
| bank_contract.py | bank_contract | `am bank` | wired (`recover-layout` waits for layout_recovery) |
| retrieve.py | retrieval | `am search`, `am get`, `am explain` | waits |
| discover_memory.py | discovery | `am discover` | waits |
| assemble_context.py | assembly | `am context` | waits |
| admit_learning.py | admission | `am capture` (prefix `capture`), `am admission` | waits |
| claude_boundary.py / codex_boundary.py / omp_boundary.py | … | `am claude-boundary` / `am codex-boundary` / `am omp-boundary` | waits |
| private_workspace.py | private_workspace | `am private` | waits |
| compare_learning.py | comparison | `am compare` | waits |
| consolidate_learning.py | consolidation | `am compact` | waits |
| checkpoint.py | checkpoint | `am checkpoint` | waits |
| govern_memory.py | governance | `am govern`, `am correct`, `am supersede`, `am revoke`, `am forget` (intent mapping) | waits |
| publish_memory.py | memory_publication | `am publish` | waits |
| refresh_memory.py | memory_refresh | `am refresh`, `am boundary` | waits |
| reconcile_memory.py | semantic_sharing | `am semantic` | waits |
| sharing_boundary.py | sharing_boundary | `am sharing` | waits |
| sharing_recovery.py | sharing_recovery | `am exchange` | waits |
| projection.py | projections | `am projection` | waits |
| bank_writes.py | bank_writes | `am writes` | wired |
| bank_transactions.py | bank_transactions | `am transaction` | waits |
| migrate_bank.py | migration | `am migrate` | waits |
| recover_layout.py | layout_recovery | `am bank recover-layout` (nested route of bank) | waits |
| installer.py / am_console.py | installation / console | `am` root; `install`, `update`, `uninstall`, `purge-data`, `recover`, `version`, `status`, `doctor` are native entries | root wired; natives wait |
| update_runtime.py / uninstall_runtime.py / diagnose.py | updating / uninstallation / diagnostics | `am update` / `am uninstall` / `am version`, `am status`, `am doctor` | wait |
| artifact.py | artifact | `am artifact` | wired |
| authority_policy.py | authority | `am authority-policy` | wired |
| bank_layout.py | bank_layout | `am bank-layout` | wired |
| lifecycle_model.py | lifecycle | `am lifecycle-model` | wired |
| record_contract.py | records | `am record-contract` | wired |
| scope_policy.py | scopes | `am scope-policy` | wired |
| validate_pack.py | pack_validation | `am validate-pack` | wired |
| security_policy.py / inactive_records.py / tombstones.py | security / inactive / tombstones | `am security-policy` / `am inactive-records` / `am tombstones` | wait |

Because `.py` is 3 characters, `am <stem>` has exactly the length of `<stem>.py`. Usage wrapping of a renamed script is therefore byte-identical to the baseline; the vector test relies on this.

## Registration recipe for later lanes

1. Create `src/cli/helpers/<module>.rs`:
   - a `static PARSER: Parser` that transcribes the baseline argparse declaration exactly: `Opt::store/flag/append`, `.required()`, `.default("…")`, `.empty()` for `default=[]`, `.choices(&[…])` or `.computed(fn)`, `.help("…")`, `Positional`, `Exclusive`, `Subcommands { dest, required, commands }`; `description` = the module docstring;
   - `pub static HELPER: Helper { script, module, parser, routes: &[], run }`;
   - `fn run(&Matches, &Context) -> CommandResult`. It calls the typed domain function and reads inputs through `cli::input`: `load`, `read_bytes`, `read_text`, `load_text_or_stdin`. Read/decode errors are passed into the domain's `Result` input, so they reach the module's error envelope.
2. Add `pub mod <module>;` to `src/cli/helpers/mod.rs`.
3. Add one line per public command to `src/cli/table.rs` `ENTRIES`, at its baseline position:
   - delegated route: `route("search", &helpers::retrieval::HELPER)`;
   - prefixed route: `Entry::Route(Route { name: "capture", helper: …, prefix: &["capture"] })`;
   - installation's own commands: `Entry::Native(Native { name, help, parser, run })`.
4. Nested helper routes (bank → `recover-layout`) go into that helper's `routes`, plus a `Subcommand` entry with the baseline help text.
5. Providers a lane's module needs are registered in `cli::register_providers()`.
6. Nothing else changes: the surface, root usage and tests derive from the table. Streaming routes (`omp-boundary --stream`) are an exception: they need a handler that writes the framed stream directly. The console already has the baseline's uncaptured path for that command; the omp lane adds the writer.

## Argparse declarations wired (from the baseline mains)

- developer:
  - `status`: `--observe` (append, `[]`), `--harness`, `--session`, `--repo` (default `.`);
  - `preview`: required exclusive group (`--create` store_true | `--adopt` | `--select`), plus `--label` and `--alias` (append, `[]`);
  - `apply`: `--proposal` (required);
  - `recover`: `--proposal`.
- scope-policy: `model`; `decide --input` (required).
- authority-policy: `model`; `assess --input` (required).
- record-contract:
  - `model`;
  - `render --input`;
  - `validate --file`;
  - `applicability --file --input`;
  - render, validate and applicability also take `--require` (append, `[]`, choices = schema `x-policy-fields`).
- lifecycle-model: positional `operation` {definition, guide, current, transition}; `--input` (help "JSON request path, or - for stdin").
- artifact: positional `operation` {inspect}; `--pack`.
- validate-pack: `--pack`.
- bank:
  - `render --input` (required, `-` = stdin, help text kept);
  - `validate --repo .`;
  - both subcommands keep their help strings.
- bank-layout: `model`; `render-index --input`; `validate-index --file --path` (path help kept); `inspect --repo .`.
- writes: `inspect`, `preview` and `recover` take `--repo .`; `preview --path --content` (both required); `recover --proposal`; `apply --proposal` (required). The preview operation is `replace`.

## Obligations

Partially ported:
- `tests/contracts/test_public_cli.py::PublicCLIContract.test_default_help_version_flags_and_public_parser_names_are_read_only`
  - Ported: `am`/`help`/`--help`/`-h` print `help_text`; route `--help` and `help X` print `usage: am …` with no `am X X`; `--not-an-option` exits 2 with `am:`; no `.py` in output.
  - Missing: `version`/`--version`/`-V` output and `install|update|uninstall --help` wait for installation/diagnostics; the read-only snapshot needs an installed fixture.
- `tests/contracts/test_public_cli.py::PublicCLIContract.test_json_is_explicit_and_source_helpers_and_memory_results_remain_structured`
  - Ported: `--json` is accepted in any position under both names and only selects output; machine mode is ported.
  - Missing: `version --json` and the `identity status` route wait for diagnostics and scope_binding.
- `tests/contracts/test_public_cli.py::PublicCLIContract.test_refusals_and_preserved_uninstall_are_readable_and_keep_failure_codes`
  - Ported: render escaping (`\x1b`), the failure line and code, and the preserved-target and status lines, tested with result documents.
  - Missing: the real uninstall/status runs wait for uninstallation/diagnostics.
- `tests/contracts/test_skill_routes.py::SkillRouteContract.test_missing_delegated_helper_cannot_be_hidden_behind_the_installed_command`
  - Ported: the surface refuses `Delegated helper is missing: scripts/…` with code `dispatch`.
  - Missing: the baseline deletes `retrieve.py` from a complete copied pack, which needs retrieval and all routes registered.

Deferred:
- test_public_cli: bootstrap journey, legacy wrapper and recovery tests, which wait for installation/updating/uninstallation. `test_unsupported_runtime_python_refuses_before_loading_operations` is obsolete (no interpreter); Main decides on its ledger state.
- test_skill_routes: `test_every_documented_helper_dispatch_also_accepts_real_public_help` and `test_working_delegated_help_commands_are_valid_documented_commands` wait for every helper's registration and the guides' migration to `am` commands.
- test_unified_cli_operations: waits for admission, assembly, consolidation and governance.

Other tests in `t2_cli`, beyond the annotated ones:
- every Main-captured argparse vector (24 rows + 22a/22b/24a/24b): exit code, stderr bytes and stdout bytes (UUIDs normalised);
- input error envelopes, including `[Errno 2] No such file or directory: 'x'`;
- stdin input for `--input -`;
- the pack-root refusal;
- direct, bootstrap and non-UTF-8 modes;
- the selector, name and intent units;
- surface equals table;
- SIGTERM→143 and SIGINT→130 through the binary: a fake `git` execs `sleep 30`; the child is proven alive before the signal and `kill(pid,0)` gives ESRCH at the moment the binary's exit is observed, which proves it was killed and reaped by the binary; stdout is empty; a non-owned sibling in the same process group survives;
- normal owned-child completion with the handler installed, 3 runs;
- registry capacity: a re-exec'd single-test worker installs the handler and fills the registry with a 3-child `Group`. A 4th spawn returns `Refusal::OwnedCapacity` with no evidence, and its marker command never ran. After the group is dropped, every slot is free again.

## Declared deltas

- Help/usage layout: argparse 3.13 layout at a fixed 80-column width (usage wrapping, help position, sections). Not reproduced: COLUMNS/terminal width, textwrap hyphen breaking (whitespace wrap only), `%(…)s` expansion in help strings (no current help uses it). Error messages, exit codes, unique long-prefix abbreviation, ambiguity, `=` values, the `--` marker, value-with-space acceptance, negative-number positionals and help precedence follow the vectors. Choices are listed unquoted, as in 3.13.
- Combined short flags: only `-h` exists. `-hX` prints help and keeps `-X` as an extra; no other short options are declared. `--opt -- value` (consuming the marker) is not reproduced; it gives `expected one argument`.
- Root parser lists only registered commands until the other lanes land (vector rows 1/2 are asserted on the registered subset).
- Non-UTF-8 argv: refused with `<prog>: error: Non-UTF-8 argument cannot be recorded; …`, exit 2. Python carried surrogate escapes.
- `installed_name`: argv0 basename only; there are no interpreter flags to skip.
- Input files: bounded at 64 MiB with a new refusal text. They follow symlinks like `Path.read_bytes()`, since they are caller-supplied request data, not trust paths. The `OSError` text is reproduced as `[Errno N] strerror: repr(path)`; Python's `Path` normalisation of the displayed path is not. UTF-8 failures use the shared non-UTF-8/codec messages instead of `UnicodeDecodeError` text.
- `isprintable`/`repr` use the toolchain's Unicode tables (same category rule as CPython), not Unicode 15.1.
- `render` never crashes on malformed shapes. Where Python would raise (non-dict receipt, string items), missing keys render as `None` and non-dict items are shown via `str()`.
- `--pack` default: registered runtime location (`PackRoot`) or a refusal; there is no `__file__`.
- Bootstrap: `--bootstrap-binary PATH` is skipped by the guarded update rewrite; an unregistered receipt provider is a typed refusal.
- Closed stdout makes the exit status 1 (Python's BrokenPipe handling was interpreter-dependent).
- Termination: `_exit(128+signal)` after killing/reaping owned children. The baseline died by signal or KeyboardInterrupt.

## Not ported / gaps

- `console.main`'s `omp-boundary --stream` branch exists, but the output is captured. True framed streaming needs the omp lane's writer (see recipe).
- render branches for install/update/uninstall/status/doctor/migrate/recover are fully ported and unit-tested only with sample documents; their commands are not registered yet.

## Process notes

- I once ran `python3 -c ""` (an empty program, no behaviour inspected) by mistake inside a shell pipeline.
- I edited `tests/t1_a1.rs` call sites with a perl one-liner instead of the edit tool; the diff is the 5 lines listed above.

## Commands run (final results)

- `cargo build --offline --bin am`: ok.
- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets`: no warnings.
- `cargo test --offline --test t2_cli`, 3 times: 14 passed each time.
- `cargo test --offline` (all targets, against int `dcf1367` sources): everything passed, including t0_fs 40, t0_process 34, t1_a1 38, t1_a2 77, t1_bank 30, t1_migration 9, t2_omp_install 18, lib 4 and t2_cli 14.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/cli src/console.rs src/main.rs`: one hit, `console::printable`'s fixed 8-byte buffer (invariant commented), plus test code only.

Demonstration (binary linked as `am`):
```
$ am --help                      → help_text, exit 0
$ am scope-policy model          → {"model": {"schema": "agent-memory/scope-policy-v1", …}, …}, exit 0
$ am no-such-command             → usage: am [-h] {developer,…} ... / am: error: argument command: invalid choice: 'no-such-command' (choose from developer, bank, …), exit 2
$ am developer preview --create --adopt x
                                 → am developer preview: error: argument --adopt: not allowed with argument --create, exit 2
```
