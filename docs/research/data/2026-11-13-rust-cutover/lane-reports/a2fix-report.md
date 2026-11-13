# A2Fix report — second domain batch repair

Worktree: `/opt/agent-memory-worktrees/rust-only-v015` (sole editor). Checkpoints were committed by Main: 8758d6b (failures, `inspect_runtime`), then 33b5deb + dcf1367 (T0 review, developer/local-registry gaps). This report covers all three checkpoints plus the final diff against dcf1367.

## 1. The original 24 `tests/t1_a2.rs` failures

| Failure group | Root cause | Wrong side | Fix |
|---|---|---|---|
| 17 × `developer_contract` (child `status.success()` false; "Identity config roots must not use symlinks") | On macOS, `/tmp` is a symlink to `/private/tmp`. Baseline `developer_identity.paths()` (py:84-99) refuses symlinked ancestors. Baseline tests run from `Path(tempfile).resolve()` (`tests/world.py:60`). | Test | `developer_private_root` canonicalises the created root |
| `pack_internal_symlink_never_reads_target` (got `malformed-pack`, expected `file-type`) | The pack root sat under `/tmp`. `inventory` opens the chain without following symlinks. Every baseline caller passes `Path(pack).resolve(strict=True)` (pack_validation.py:364, artifact.py:52), and the Rust `with_validated` already canonicalises. | Test | `PackDir::new` canonicalises |
| `inventory_has_exact_known_digest_and_reader_limits` (NotADirectory) | Same | Test | Same |
| `artifact_declarations_bind_actual_bytes…` | Same: the declaration read went through the `/tmp` symlink | Test | Same |
| `canonical_writer_empty_slot_is_not_a_known_owner` (NotADirectory in the test's own `DirectoryChain::open`) | `/tmp` root | Test | Registry roots canonicalised in all 6 `registry_contract` tests |
| `repository_physical_contexts_refuse_replacement`, `…walk_prunes…`, `repository_roots_retain_committed_deleted_locator` | `/tmp` root. Baseline `settings()` resolves the repo (py:`Path(repo).resolve()`), and the Rust `settings_with` / `bank_path` / `observe_settings` already canonicalise. | Test | `RepositoryDir::new` canonicalises; `PrivateRoot` also canonicalises |

No production symlink refusal was loosened. Main confirmed that the unmodified tests pass 68/68 on Linux, where `/tmp` is a real directory. The unused imports were removed (`Path`, and 6 × `PermissionsExt`).

## 2. Behaviour gaps found and fixed

### artifact / pack_validation
- `router['properties']['schema']['const']` with a missing key: the baseline raises KeyError, and this is now refused before the membership test ("Bank router declaration lacks its schema constant"). Previously the Rust compared `Null` and returned the wrong message.
- `inspect_validation` was rewritten into named helpers with no panic sites (`check_compatibility`, `adapter_entry`, `files_digest`, `file_count`, `members`).
- The pack inventory used an unbounded `entries()`. It now uses `entries_bounded(4096 - seen)`; overflow gives the same `inventory-bound` refusal.
- `check_dispatch`: argv indexing and the dispatch-map index were replaced by `get` and slice patterns.
- `validate_inventory`, required-section span: Python slicing returns an empty list when the span is out of range or reversed. The Rust used `original[a..b]`, which could panic; it now uses `get(..)`, so such a section is reported as empty.
- `validate_inventory`: the `domain.unwrap()`, `split_once(..).unwrap()`, `graph[..]` and `mandatory.get_mut().unwrap()` sites became refusals or conditional access.
- `splitlines`: the CRLF handling no longer uses `unwrap`.
- `git_operation` no longer indexes argv.
- The redirect check uses `rev().nth(1)`.
- `command` no longer panics while encoding its output.

### developer_identity
- `aliases` / `labels`: Python validates every item before comparing for duplicates (py:62-80). The Rust returned "Duplicate" at the first repeat even when a later item was invalid. Fixed.
- `preview` select check: Python uses truthiness of `new_labels or new_aliases`. This now uses the shared `sharing_limits::truthy` (made `pub(crate)`; `-0` is also falsy).
- All unwrap/unreachable sites were removed:
  - typed `ProposalFields`;
  - `Result`-returning `insert`;
  - `merged_entry` (the `setdefault` + ordered extend);
  - `leaf()` / `spec_text()` instead of `file_name().unwrap()`;
  - `sha256_hex` instead of `digest(..).unwrap()`;
  - a let-else for the pid.
- `recover` reads through the new `FlockGuard::read_bounded`.

### local_registry
- `paths`: Python refuses only a non-directory root (py:189). A non-directory ancestor is now treated as "absent" instead of being refused.
- `check_canonical_writer`: any descriptor OS error now maps to the baseline outer refusal "Canonical writer observation became unsafe; preserve for inspection". Previously it surfaced the raw OS text.
- `check_canonical_writer`: the owner pid accepts any positive JSON integer, as Python does (no `os.kill` is made on this path). Previously values ≥ 2^31 were refused.
- `exclusive`: the empty-slot cleanup lets "Created metadata slot parent changed; preserve the marker" through. Other errors map to "…empty-slot cleanup became unsafe", matching py:240-247.
- All unwrap sites were removed; locked I/O goes through `FlockGuard::{read_bounded, write_once}`.

### repository_config
- Physical discovery listed directories without a bound. It now uses `entries_bounded(MAX_DISCOVERY_ENTRIES - examined)`.
- The bank-alias directory listing is now bounded by `MAX_DISCOVERY_ENTRIES`.
- `router_kind`, `local_value`, `parse_local` and `listed_paths` now use slice patterns, `strip_suffix` and `split_first` instead of indexing (no behaviour change).

### scopes / authority / records / handoff / sharing_limits
- scopes (from the PolicyFix sub-agent): the scope is parsed before conditions are validated (py:54-56). The comparison loop was rewritten without unwraps.
- authority: unwraps replaced (`domains()`, `source_kinds()`, `split_first`, `replace`). No behaviour gaps were found.
- records (sub-agent): restructured to one helper per baseline function.
  - Source-capture lookup uses Python equality (`True`, `1.0`).
  - Evidence strings are validated before duplicates are checked.
  - Version comparison happens only for a link to the same record.
  - `render` ignores an unused `--file` error.
- records (mine):
  - confidence accepts `-0`;
  - unwraps removed;
  - if the success document cannot be encoded, it now takes the fixed redacted refusal, as Python does because it encodes inside its `try` (py:236-244). Previously the Rust panicked.
- handoff (sub-agent): an identity key missing from the current observation means "not reusable" (py:225-235); previously it compared as null.
- handoff (mine): lock poisoning is tolerated; `get` / `strip_suffix` / `split_once` replace slicing; `set_pair` replaces `pairs[5]`; invariant comments on the cue scanner.
- sharing_limits (sub-agent):
  - publication goes through a pinned chain and uses no-replace with readback;
  - inspection-receipt `len` and truthiness match py:86-87;
  - a missing conflict field is refused;
  - the `expect` was removed.

### Shared addition outside my file list
- `src/lifecycle.rs`: added `CommandResult::json(value, exit_code, style)`. It encodes without panicking: a depth or non-finite failure gives exit 1, empty stdout and the refusal on stderr. artifact, developer_identity, scopes, authority, records and pack_validation use it.
- `lifecycle::command`'s own `expect` (first batch) was left untouched.

## 3. `inspect_runtime` (design "Artifact digest") — done
- `artifact::BINARY_ENTRY = "bin/am"` and `BINARY_MAX_BYTES = 64 MiB`.
- `runtime_digest(files, sha, mode) -> pack_validation::Result<(Value, String)>`:
  - pure;
  - inserts `{"sha256","mode":"0o755"}` at the sorted position;
  - refuses `bin/am` or anything under `bin/`, a mode other than 0o755, and a digest that is not 64 lowercase hex characters (code `runtime-binary`).
- `inspect_runtime(pack, binary)` and `inspect_runtime_with(pack, binary, surface, migration)`.
- `pack_validation::file_entry` is the single definition of the entry shape.
- `pack_validation::binary_asset` does the read: canonical parent, no-follow pinned descriptor, regular files only, size checked before the read and bounded during it, identity rechecked.
- Tests:
  - the hand-computed digest `022501c3…2bd5` from `shasum` over the documented canonical JSON;
  - the digest changes when the bytes change;
  - the pack-only digest is unchanged;
  - a mode change is refused (only 0o755 is legal);
  - a packed `bin/am` is refused;
  - a symlinked binary is refused;
  - oversize is refused at the bound, and exactly the bound is accepted.

## 4. T0 review (`t0-additions-review.md`) — all items fixed
- **P2-1:** `MAX_SYMLINKS = 40` and `MAX_PENDING_COMPONENTS = 131072`. Tests cover the self-amplifying link, a plain loop, and a 40-link chain that resolves while 41 is refused.
- **P3-1:** `expanduser` now follows pathlib, the form every baseline caller uses:
  - `.` parts are dropped, so `./~/x` expands;
  - an unknown `~user`, no passwd entry, or a HOME starting with `~` gives "Could not determine home directory.";
  - passwd errors are treated as KeyError;
  - the result is in pathlib normal form, so `registry_path` text matches `str(Path)`.
- **P3-2:** the casefold, alnum and html tables get ordering checks; casefold is checked per row; alnum gaps are checked.
- **P3-3:** the rename test uses a barrier and 2000 renames under a running reader. UTC tests added for year 1, year 9999, 1900-02-28/03-01 and 2100-02-28/03-01, plus the out-of-range errors.
- **P3-4:** `host::current_username` deleted. `Descriptor::write_once` is kept: `local_registry::exclusive` calls it through `FlockGuard::write_once` to reproduce the baseline single `os.write`.
- **P3-5:** `FlockGuard::descriptor_mut` removed and replaced by `read_bounded` / `write_once`.
- **P3-6:** uuid4 uses kernel entropy. Main replaced my `getentropy` call with a cfg-gated `entropy()`, because `libc::getentropy` is missing on linux-musl. I copied Main's `host.rs` back into this worktree.
- **Note:** `Descriptor::entries` no longer contains an `expect`.

## 5. No-panic pass (lane "Added rules")
`grep -n 'unwrap()\|expect(\|\[.*\.\.' src/<11 modules>` — what remains and why:
- `expect` on the four `LazyLock` schema decodes (developers, scope-policy, authority-policy, record). They are compiled in from the pack, decoded by tests, and each carries an invariant comment.
- `v[..i]` inside `enumerate` (artifact `strings`, pack file-operations). Commented.
- handoff `managed_cues` slicing: ASCII `find` / `char_indices` bounds. Commented.
- pack_validation Markdown/shell scanners (about 80 slicing sites): the invariant is documented once over the scanner group (ASCII-delimiter / `char_indices` bounds in the same string or in its equal-length mask). It is backed by a new `malformed_input_contract` test that feeds 100,000 generated strings to every parser: delimiters, multibyte scalars, U+2028, U+0085, CR.
- **Not done:** I did not add a separate per-function comment for each scanner function.

Child processes: every git child in my modules goes through `identity_git` with the baseline 10 s `_QUERY_TIMEOUT`; no other children are spawned.

## 6. Tests added
- `malformed_input_contract` ×2.
- Runtime digest tests ×3.
- The 6 tests proposed by sub-agents. Each was checked against baseline line numbers before adding: 3 records, scope order, handoff missing identity, sharing inspection counts.
- t0: resolve bounds, rename barrier, UTC limits, pathlib expanduser, unicode ordering and gaps.

## 7. Declared deltas (new in this lane)
- `canonical_path` refuses a non-looping symlink chain longer than 40, which CPython `realpath` would resolve.
- A selected bank whose `MEMORY.md` is a symlink to a file is inspected and refused. Python's `is_file()` follows it and skips physical discovery.
- A dangling `.git` symlink in the repository root, or in a nested directory, counts as present (stat without following). Python's `exists()` is False there.
- The bank-alias directory listing and the developer-config path checks are bounded or descriptor-based. A non-directory ancestor of the developer config root is refused with the OS error, where Python reported absence.
- Developer and local lock pid ≥ 2^31: refused as malformed. Python crashes with OverflowError in `os.kill`.
- `expanduser` gives a typed refusal where Python raised an uncaught RuntimeError. pathlib's special `//` root is not preserved.
- `html_unescape` of numeric references longer than 4300 digits gives U+FFFD; CPython ≥ 3.11 raises ValueError.
- `nfc_casefold` uses the crate's newer Unicode NFC tables; this differs only for scalars unassigned in 15.1.
- The `sharing_limits` no-replace publication refuses namespace races (sub-agent).
- Encoding failures in CLI results give exit 1 with an empty stdout. Python raised RecursionError; records was already the documented exception.

## 8. Cargo commands (final state)
- `cargo fmt` then `cargo fmt --check` → clean.
- `cargo check --offline --all-targets` → 0 warnings, 0 errors.
- `cargo test --offline` × 3 → all green each time. Totals: t1_a2 79, t0_fs 40, t0_host 3, t0_unicode 3, plus the other targets, 0 failed.

## 9. Files changed since dcf1367
- `src/{artifact,authority,developer_identity,handoff,local_registry,pack_validation,records,repository_config,scopes,sharing_limits}.rs`
- `src/fs/{descriptor,lock,resolve}.rs`
- `src/host.rs` — identical to integration dcf1367
- `src/lifecycle.rs` — not owned: `CommandResult::json` added
- `tests/{t1_a2,t0_fs,t0_host,t0_unicode}.rs`

Some of these were already part of the earlier checkpoints; diff against dcf1367 to see what is new.
