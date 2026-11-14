# Bank trio review: bank_contract / bank_layout / bank_writes (+ tests/t1_bank.rs)

Reviewed tree: `/opt/agent-memory-worktrees/rust-v015-int`, **committed** content at HEAD (`c3e76a3`).
The four target files are byte-identical to their introducing commit `7622e91` (1037 / 510 / 1016 / 1093 lines);
all line numbers below refer to that committed content. Baseline: `skills/agent-memory/scripts/agent_memory/{bank_contract,bank_layout,bank_writes}.py`
in the same tree. Read-only review: no cargo, no Python, no Git writes.

Note for Main (not a finding against the commit): while I was reviewing, the *working copy* of this worktree changed underneath
(HEAD advanced and `src/bank_contract.rs`, `src/bank_writes.rs`, `src/fs/descriptor.rs`, … carry uncommitted edits). The working copy of
`src/bank_contract.rs` currently contains unresolved conflict markers (`<<<<<<< obs` / `=======` / `>>>>>>> txn` inside `trait BankObservation`)
and a duplicated `pub fn scan_sources`. It will not compile as-is; whoever owns that integration should resolve it before any build.

## Findings

### P2-1 — Fault rendezvous never pauses when `AGENT_MEMORY_FAULT_DIR` does not exist yet (gate-blocking)

- Rust: `src/bank_writes.rs:503` `let canonical = fs::canonical_path(&directory, true)?;` — `strict = true`
  (`src/fs/resolve.rs` returns `Err` as soon as a component is missing).
- Baseline: `bank_writes.py:141-143` checks `directory.resolve()` (non-strict) and then `directory.mkdir(parents=True, exist_ok=True)`.
- Failing input: every baseline pause-mode contract. `tests/contracts/test_bank_writes.py:60-70` (`paused`) passes
  `world.inside("rendezvous")`, and `tests/world.py:123-131` (`World.inside`) only resolves the path and never creates it; the same holds for
  `test_concurrent_bank_writers.py:33`, `test_bank_transactions.py:286/405/479`, `test_admission.py:300`, and others.
  Rust returns `No such file or directory (os error 2)` from `fault("locked-before-inventory")` and from the other stages. The child exits 1 without a `.ready` file,
  so `test_lock_precedes_inventory…`, `test_readback_and_receipt_creation…`, `test_external_target_edit…`, and every concurrent-writer
  scenario fail. `bank_writes.fault` is the shared failpoint for `runtime_faults`, `purging`, `installation`, `turn_sources` and `diagnostics`, so this breaks all of those pause-mode tests too.
- Fix: resolve non-strictly, as the baseline does: `fs::canonical_path(&directory, false)`. The later `validate_directories`
  and `DirectoryChain::create(&directory, false)` already refuse symlinked or occupied components and create the missing ones.
- Test gap: `t1_bank.rs` has no `fault()` test.

### P2-2 — `bank_root` refuses every bank whose parent directory has a non-UTF-8 entry name

- Rust: `src/bank_contract.rs:718` `let entry = crate::host::text(&entry, "bank root entry")?;` refuses with
  `Non-UTF-8 bank root entry cannot be recorded; …`.
- Baseline: `bank_contract.py:287-293` iterates `bank.parent.iterdir()` with surrogate-escaped names and only refuses a real
  portable alias. A surrogate-escaped name can never have the same `portable_key` as a UTF-8 bank name.
- Failing input: a default bank at `<repo>/memory-bank`. Put a file named `b"caf\xe9.txt"` (Latin-1) in the repository root.
  `bank_writes::bank_path` (`src/bank_writes.rs:186-191`) then fails, which takes `preview`, `apply`, `recover`, `checkpoint_sources` and every later caller
  of `bank_path` with it. Python works.
- This also contradicts the same patch series: `repository_config` (`src/repository_config.rs:764-767`) scans the same directory and skips
  non-UTF-8 names (`name.to_str().is_some_and(...)`). The declared delta covers non-UTF-8 *bank* paths, not siblings of the bank.
- Fix: skip entries that are not UTF-8 (`let Some(entry) = entry.to_str() else { continue };`). This is exact, because such a name cannot alias a UTF-8 name.

### P3-1 — Refusal-string parity in `parse_table` (duplicate check is inline; single-`|` row)

- Rust: `src/bank_contract.rs:341-343` checks `Duplicate route path` row by row. The baseline (`bank_contract.py:103-120`) parses and validates
  every row first and checks for duplicates only after the loop.
  - Failing input: a route table with rows `A`, `A` (a duplicate), and then a row such as `| [topics/x.md](<wrong.md>) | … |`. Python raises
    `Route label and relative target must agree`; Rust raises `Duplicate route path`.
  - Fix: collect `routes` first, then run the `seen` check after the loop.
- Rust: `src/bank_contract.rs:295-298`. A trimmed row that is exactly `|` fails `strip_prefix('|').and_then(strip_suffix('|'))`, which gives
  `Invalid route table row`. Python (`:104-108`) accepts the start/end test, splits `row[1:-1] == ""` into one cell, and raises
  `Route row must have five columns`.
  - Fix: if `row == "|"`, use an empty inner string.

### P3-2 — `split_lines` is not `str.splitlines`: CRLF counts as two breaks (omissions), and `add_context_routes` uses LF only

- Rust: `src/bank_contract.rs:273-277` splits on `'\r'` and on `'\n'` separately. That is harmless in `parse_table`, because empty rows are filtered out, but in
  `parse_router` (`:429-437`) the empty piece between `\r` and `\n` fails `strip_prefix("- \`")`.
  - Baseline: `bank_contract.py:160-163` uses `splitlines()`, where `\r\n` is a single boundary.
  - Failing input: a router whose omission section reads `- \`a\`: x\r\n- \`b\`: y\n`, with LF headings. Python accepts it; Rust raises
    `Invalid or duplicated context omission`, so a Python-era bank in that state becomes unreadable.
- Rust: `src/bank_contract.rs:523` (`table.split_inclusive('\n')`) and the omission filter use LF only, while the baseline (`:208` and `:220`) uses
  `splitlines(keepends=True)`.
  - Failing input: a table whose last row ends with `\x0b\n`, which `parse_table` accepts. Python treats `"\n"` as a separate trailing blank line
    and emits `…\x0b\n<new rows>\n`; Rust emits `…\x0b\n<new rows>`, one byte shorter.
- Fix: implement one `splitlines(keepends)` helper with Python's boundary set, treating `\r\n` as one boundary, and use it in all three places.

### P3-3 — Temporary cleanup: the replacement error can be masked, and the temporary file leaks if `fchmod`/`fstat` fails

- Rust: `src/bank_writes.rs:872-873` runs `temporary.finish()?; replacement?;`, and `finish` (`:674`) maps `NotFound` to an error.
  - Baseline: `bank_writes.py:264-266` uses `unlink(missing_ok=True)` in `finally`, so the original error always wins.
  - Interleaving: pause at `before-recheck`. An external actor edits the target and deletes the `.agent-memory-write-*.tmp` file. After the release,
    Python reports `External edit observed immediately before replacement; preserve it`. Rust reports `No such file or directory (os error 2)`.
  - Fix: treat `NotFound` in `finish` as already gone, and order it as `let cleanup = temporary.finish(); replacement?; cleanup?;`.
- Rust: `:821` `create_exclusive` (create, then `fchmod`) and `:826` `file.stat()?` both run before the `Temporary` guard exists. If `fchmod` or `fstat` fails after
  the file was created, the file stays in the bank. In the baseline (`:230-233`), `temporary` is bound before `chmod`, so `finally` removes it.
  - Fix: construct the guard straight from the create result, or unlink at the `create_exclusive` call site on a post-create failure.

### P3-4 — `HOME=""` default state root diverges

- Rust: `src/bank_writes.rs:234` builds the default from `PathBuf::from(HOME).join(".local/state")`. When `HOME=""`, that gives the relative path
  `.local/state/agent-memory`, which is refused as `Write receipts require a canonical absolute state root outside the repository`.
- Baseline: `bank_writes.py:51` uses `Path.home()`, which is `expanduser("~")`. With `HOME=""` that is `/`, so the root is `/.local/state/agent-memory`, which is absolute and accepted.
- Fix: build the default as `expanduser_with_home("~/.local/state", home)`, then join `agent-memory`.

### P3-5 — Undeclared new refusals and masking

- `src/bank_writes.rs:741-750` reads the receipt with `fs::read_regular`, which forces `require_single_link: true`.
  A receipt that is hard-linked by a snapshot or backup tool (`cp -al`, rsnapshot) makes an idempotent replay fail with `Source must be single-link`.
  The baseline (`:214`) reads it anyway. Either declare this delta or use a reader without the single-link requirement for receipts.
- `src/bank_layout.rs:244` maps every `records::validate` error to `Invalid optional record, index or history; …`. That includes
  `Tombstone parser is not registered` (`src/records.rs:376`). A missing registration therefore looks like a corrupt user tombstone and invites manual
  "repair". The masking matches the baseline (`bank_layout.py:118-120`), but the baseline has no registration failure mode. Let the
  provider-absence refusal pass through, either with a typed `Kind` or by checking the registration before `inventory`.
- `src/bank_contract.rs:740-790`: `physical_paths` compares the full `Identity` (mtime, ctime, size, nlink) of every directory, *runtime entries included*,
  before and after the walk. If a writer creates or removes `.agent-memory.lock` during an unlocked reader's walk, the reader is refused with
  `Cannot completely inspect bank paths; preserve the existing layout`. The baseline (`:296-318`) has no such check. The refusal is fail-safe, but it is an
  undeclared delta. Declare it, or compare runtime entries with `same_binding` only.

## Step-by-step write protocol (Rust `apply_with_identity_and_environment` vs `_apply`, `bank_writes.py:194-266`)

Each step matches the baseline order except where noted:

1. `proposal_value`, `bank_path`, `state_paths`, `request_sha`, and `expected` (`observed` plus the target digest, replaced in place) are computed before any lock, as in the baseline.
2. The barrier is taken first, then the exclusive `flock` on `<bank>/.agent-memory.lock` with owner kind `bank-<operation>`. On error the barrier is released on Drop; on success `finish()` runs. This is equivalent to `ExitStack` / `__exit__`.
3. `fault("locked-before-inventory")`, `check_identity(owner)`, `observe(root, owner)`, then the project-ID check.
4. Receipt replay: the full field comparison, with `hashes_equal` matching Python dict equality (order-insensitive); `still_current` is computed; there are no writes. A symlinked receipt is refused.
5. `already_present` is computed; the stale-proposal refusal follows; `validate_replacement` runs under the lock.
6. The temporary is created in the target's parent with `O_EXCL|O_NOFOLLOW` and the exact mode `before.mode & 0o777` (`fchmod`, so umask has no effect), written, and fsynced. Then `fault("before-recheck")`, the checkpoint identity re-check, and the complete hash re-check, followed by the extra (declared) target-identity and temp-inode checks. Then `renameat`, directory fsync, `fault("after-replace")`.
7. Readback snapshot, `fault("before-receipt")`, a second snapshot, then the receipt in the baseline key order. `readback_at` uses `isoformat` semantics (microseconds dropped when zero). `record` creates the directory privately and refuses to overwrite an unexpected receipt (mode 0600 is tested). Then `fault("after-receipt")` and the result.
8. Cleanup only unlinks the inode this operation created. P3-3 covers the error-precedence gap.

## Registration traits

- Every provider lookup clones the `Arc` and drops the `RwLock` guard within the same statement before calling the provider. No registry lock is held across a provider call (`observe`, `scan_hashes`, `recover_layout`, `validate`, the six getters in `bank_writes`, and `records::validate`).
- No missing registration passes silently. Each absent provider produces a typed refusal before any write. The one exception is P3-5, where the tombstone-parser refusal is masked behind a content error.

## Bounds added by the port

None of these bounds refuses a legitimate bank:

- 1,000,000 physical entries.
- Depth 256.
- 1,000,000 parent entries in `bank_root`.
- 64 MiB receipts. Receipts hold one hash per file, so this allows about 600k files.
- 1 MiB fault-world JSON.

The bank's real ceiling remains `bank_capacity`. `validation_memo::map_input` limits match the baseline.

## Validation memo

- The key is complete: every name and body, sorted, length-prefixed, and SHA-256 hashed.
- Report and companion are stored together, and no other module uses the `bank-format` namespace.
- On a hit the guard re-parses the router, so `canonical-remote` normalization runs fresh.
- Failures are never stored.

No stale-hit path was found.

## Tests too weak to catch plausible regressions (`tests/t1_bank.rs`)

- **No test runs the write protocol at all.** Missing coverage:
  - `apply_with_identity_and_environment`
  - receipt replay
  - `already-present`
  - mode preservation
  - readback
  - cleanup

  Test-only providers for `BankObservation`, `TurnSources` and `ScopeBinding` are enough to drive it. Without them, a reordering of steps 3-7, a dropped fsync, or a wrong `outcome` would pass.
- **No `fault()` test**, which would have caught P2-1. A pause test with a not-yet-existing `AGENT_MEMORY_FAULT_DIR` is needed.
- **Most refusal tests only assert `is_err()`.** Affected tests:
  - `router_bounds_duplicate_routes_and_orphan_files`
  - `byte_zero_schema…`
  - `missing_empty_and_omitted…`
  - `write_proposals_refuse…`
  - `index_byte_row…`

  A wrong refusal string such as P3-1 passes all of them. Assert exact messages.
- **`expanded_bank_retains…/add_context_routes` checks only `contains` and the parsed result.** There is no byte-exact golden for the rewritten router, so prefix, blank-line or omission-filter byte drift would go unnoticed.
- **`memo_keeps_companion_records_and_rechecks_router` only mutates `MEMORY.md`.** Add a case that changes a non-router body under the same router and asserts a fresh (failing) result. That case guards key completeness.
- **Nothing covers `bank_root` with a non-UTF-8 sibling (P2-2), a CRLF omission list (P3-2), or `state_root_with` with `HOME` unset or empty (P3-4).**
- **`receipts_are_private…` uses a non-UUID receipt name through `record` only.** The real `state_paths_with` → `record` → `load_receipt` replay round-trip is untested.

## Verdict

**Changes requested (P2s).** Router render and parse byte parity, `validate_files`, inventory (reachability, cycles, depth, inherited
conditions, tombstone and resurrection), state-root and symlink refusals, and the observed-write ordering are faithful to the baseline.
Two P2s must be fixed before the cutover gate can trust it:

- **P2-1**: the fault rendezvous fails whenever the directory does not exist yet, which is how every baseline contract test uses it.
- **P2-2**: `bank_root` refuses repositories that have a non-UTF-8 name next to the bank.

The P3s are parity or cleanup refinements. The write protocol needs a real Rust test before the end-to-end obligations can be claimed.
