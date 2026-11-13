# Review: T0 additions for the second domain batch

Snapshot: `/opt/agent-memory-worktrees/rust-v015-lane-corebank` (HEAD `5693675` plus the working tree), compared with `c62ccd5`.
Scope: `src/host.rs`, `src/unicode.rs`, `src/fs/resolve.rs`, the diffs of `src/fs/{descriptor,lock,rename,mod}.rs`, `src/error.rs` and `src/remote_identity.rs`, and the tests `tests/t0_host.rs`, `tests/t0_unicode.rs` and the new cases in `tests/t0_fs.rs`.
How it was reviewed: by reading code and running `git diff`/`jq` only. Nothing was edited, and cargo and Python were not run.

## Findings

### P2-1 `canonical_path`: a symlink in the repository can make one name lookup use hundreds of MB and thousands of syscalls
- **Where:** `src/fs/resolve.rs:37` (`links > 4096`) and `:47-57`, where each target's components are pushed to the front of `pending` with no limit.
- **How it is triggered:** `repository_config` discovery calls `canonical_path(&directory.join(&name), true)` for every symlink it finds in the repository tree (`src/repository_config.rs:629`). That tree is repository content and may be a clone.
- **Example input:** a committed symlink `a -> a/a/a/…/a` with a 4095-byte target on Linux, which is 2048 components.
  - Each of the 4096 allowed expansions resolves the leading `a` again and leaves 2047 more components queued.
  - Before the refusal, `pending` holds about 4096 × 2047 ≈ 8.4 million `OsString`s. That is about 400 MB for the `VecDeque` slots plus 8.4 million small heap allocations, roughly 0.5–0.7 GB per lookup.
  - On macOS the target limit is 1023 bytes, giving about 2.1 million entries and about 150 MB.
- **Cost of a plain loop:** even `loop -> loop` costs 4096 `lstat` + `readlink` pairs, about 8192 syscalls, for each symlink entry. `MAX_DISCOVERY_ENTRIES` allows 1,000,000 entries.
- **What the baseline does:** CPython's `realpath` finds a loop the second time it meets the same link, using its `seen` map. `os.walk`'s `is_dir()` hits the kernel's `ELOOP` after 40 links (Linux MAXSYMLINKS=40, macOS 32).
- **Minimal fix:** `const MAX_SYMLINKS: usize = 40;` and refuse when `links > MAX_SYMLINKS`.
  - This keeps `pending` at or below 40 × 2047 entries.
  - Declare the delta: a non-looping chain of more than 40 links resolves in CPython but is refused here.
  - If exact parity is preferred instead, port CPython's `seen: HashMap<PathBuf, Option<PathBuf>>` and also cap `pending.len()`.
- **Test to add:** the self-amplifying link case in `canonical_names_resolve_links_before_parent_components`.

### P3-1 `expanduser` follows `os.path.expanduser`, but every baseline caller uses `pathlib.Path.expanduser`/`Path.home()`
- **Where:** `src/host.rs:207-240` (the unchanged-path returns at `:214`, `:222`, `:228`). The test `tests/t0_host.rs:70-71` asserts this behaviour.
- **Baseline call sites:** `bank_writes.py:51`, `developer_identity.py:88`, `local_registry.py:187`, `sharing_limits.py:22,65`.
- **Baseline behaviour (Python 3.11+):**
  - `Path.expanduser` raises `RuntimeError("Could not determine home directory.")` if the expanded first part still starts with `~`. That happens for an unknown `~user`, for HOME unset with no passwd entry, and for HOME beginning with `~`.
  - pathlib drops `.` parts first, so `./~/x` becomes `~/x` and is then expanded.
  - CPython's `pwd` turns any `getpw*_r` error into KeyError. Rust's `passwd()` turns non-zero codes into a hard error instead.
- **Actual impact:** every Rust caller then requires an absolute path, so the result is still a refusal. Only the message differs. For example, `~nosuchuser/x` gives "…must be absolute" or "Select an absolute owner-only output…" instead of RuntimeError. `AGENT_MEMORY_CONFIG_HOME=./~/cfg` is refused here but accepted by Python.
- **Fix:** either
  - make `expanduser` refuse with "Could not determine home directory." when the result still starts with `~`, drop leading `.` components, and update the test; or
  - record this as a declared delta of T0.

### P3-2 The Unicode tests check each table against its vector but never check the lookups that depend on it
- **Where:** `tests/t0_unicode.rs:18-46`. The lookups are binary searches at `src/unicode.rs:1540`, `:2308` and `:4669`.
- **The gap:** binary search is only correct if the table is sorted, and no test asserts that.
  - `casefold` is checked with one sample string.
  - `is_word` is checked only at range endpoints and at U+0301. It is never checked at a gap code point (`end+1`).
- **What I verified with jq:** the committed vectors are sorted today.
  - casefold: 1530 rows, strictly ascending.
  - alnum: 748 ranges, non-decreasing.
  - html5: 2231 names in C order, all ASCII, shortest name 2 characters.
- **Fix:**
  - Assert `windows(2)` ordering for all three tables.
  - Assert `casefold(&char::from_u32(src).to_string()) == target` for every row.
  - Assert `!is_word(char(end+1))` wherever the next range starts above `end+1`.

### P3-3 Two new tests cannot fail on the defects they are named after
- **`replace_rename_never_exposes_missing_target_to_a_reader`** (`tests/t0_fs.rs:1131`): it does one rename. That rename can finish before the reader thread starts, so an unlink+rename emulation would also pass. Fix: wait on a `Barrier` until the reader has started, then alternate many renames while it reads.
- **`fixed_utc_instants_match_datetime_isoformat`** (`tests/t0_host.rs`): it has no year 1, year 9999 or 1900/2100 (non-leap century) cases. I checked the algorithm by hand (Hinnant `civil_from_days`) and it is correct, so this is a test gap only. Add `-62135596800 → 0001-01-01T00:00:00+00:00`, `253402300799 → 9999-12-31T23:59:59+00:00`, `4107542400 → 2100-03-01…`, and `-62135596801` and `253402300800` as errors.

### P3-4 Two public T0 primitives have no production caller
- `Descriptor::write_once` (`src/fs/descriptor.rs:366`) and `host::current_username` (`src/host.rs:200`) are called only from the t0 tests.
- Delete them, or name the lane that will use them.

### P3-5 `FlockGuard::descriptor_mut` lets the locked descriptor be moved out of the guard
- **Where:** `src/fs/lock.rs:86`.
- **The problem:** safe code can call `std::mem::replace(guard.descriptor_mut(), other)`. `Drop` then sends `LOCK_UN` to `other`, and the flock stays held by the descriptor that was moved out, beyond the guard's lifetime.
- **Callers:** `developer_identity.rs:854` and `local_registry.rs:494,717` only call `read_bounded`.
- **Fix:** expose `FlockGuard::read_bounded(&mut self, limit)`, or a borrowed I/O view, instead of `&mut Descriptor`.

### P3-6 `uuid4` reads entropy by opening `/dev/urandom` by path
- **Where:** `src/host.rs:21`.
- **The problem:** this fails in a chroot or sandbox without `/dev`. CPython's `os.urandom` uses getrandom(2)/getentropy(2).
- **Fix:** use `libc::getentropy(bytes.as_mut_ptr().cast(), 16)`, which is available on macOS ≥10.12, musl ≥1.1.20 and glibc ≥2.25.

## Notes (not defects)
- **`entries()` uses `.expect("unbounded directory listing")`** (`src/fs/descriptor.rs`, working tree only; this is the bank lane's uncommitted `entries_bounded`). The invariant it relies on is set in `entries_with_limit`, not in the same function, which is what the lane rule requires. Either justify it in a comment or have `entries_with_limit(None)` return `Vec` directly.
- **Very long numeric character references:** `html_unescape("&#" + "1"*5000)` gives U+FFFD. CPython 3.11+ raises ValueError because of the int max-str-digits limit. This is an undeclared but harmless delta; list it.
- **`nfc_casefold` uses the crate's newer Unicode tables.** These differ only for scalars that were unassigned in 15.1, for example the new Kirat Rai compositions in 16.0. This is already documented in the comment; list it as a delta.

## Checked and correct
- **`uuid4`:** the RFC 4122 bits are right (`b6 = b6&0x0f|0x40`, `b8 = b8&0x3f|0x80`) and the 8-4-4-4-12 layout is right.
- **`hostname`:** the buffer is zeroed and 1024 bytes, larger than HOST_NAME_MAX on both platforms. A missing NUL is refused. musl truncates and NUL-terminates. Non-UTF-8 names get a typed refusal.
- **`pid_state`:** pid ≤ 0 is refused before `kill`, so process groups are never signalled. `ESRCH` maps to Gone, `EPERM` to Denied, any other error to an error. Callers take an `i32`.
- **`getpw*_r`:** the buffer outlives the `CStr` copies. `assume_init` happens only after `code == 0` and `result != null`. The `ERANGE` retry doubles the buffer up to 1 MiB. `getuid` matches CPython.
- **`isoformat_utc`:** the Hinnant algorithm is correct for every year from 1 to 9999. The range is enforced, and microseconds are omitted when zero, as `datetime.isoformat` does. In `utc_now_isoformat`, pre-epoch values are floored.
- **`canonical_path` behaviour against CPython `realpath`:**
  - A symlink is resolved before a following `..`.
  - In non-strict mode a missing component is kept and later `..` still pops it.
  - Absolute targets reset to `/`, and `..` at `/` stays at `/`.
  - Non-UTF-8 names are kept as `OsString`.
  - Loops are refused in both modes. Python 3.11/3.12 raise RuntimeError, 3.13+ do not; choosing refusal is fine, but declare it.
- **`remote_identity` diff:** it is formatting only, apart from moving `resolve` into `fs::canonical_path` with the same semantics: `lstat` errors keep the component, `readlink` errors become `ambiguous()`, and the budget is the same.
- **`RenameMode::Replace`:** it uses plain `renameat`, which is atomic on both platforms. The `unreachable!` arms follow from the early return in the same function. `native_rename` fsyncs the target directory, and the source directory too when it differs. Callers using `rename_at` directly fsync themselves (`local_registry.rs`, `developer_identity.rs`).
- **`try_acquire_owned`:** it returns the live descriptor on `Busy`, an I/O error or a search-only descriptor. `try_acquire` keeps its earlier behaviour. `Drop` unlocks and then closes.
- **`read_prefix`:** `fstat` refuses directories and FIFOs before any read. `pread` starts at offset 0, short reads loop, and `EINTR` is retried. Growth or any change to the inode during the read gives `SourceChanged`. The size is bounded by `maximum`.
- **`write_once`:** only `EINTR` is retried, partial counts are returned, and a zero-length write returns `Ok(0)`.
- **`html_unescape`:** it matches CPython's `_charref` regex and `_replace_charref`.
  - The numeric, hex, invalid-charref, surrogate/>0x10FFFF and invalid-codepoint branches all match.
  - Named lookup takes up to 32 characters, adds an optional `;`, then tries the longest prefix. Single-character prefixes can never match because the shortest name is 2 characters.
  - An unmatched name is emitted as text.
  - All slicing happens at char boundaries.
- **`casefold`/`is_word`:** both tables match the vectors exactly. The `\w` rule is isalnum or `_`. I spot-checked the vectors: ß→ss, İ→i̇, ŉ→ʼn, U+0345→ι, Cherokee U+AB70→U+13A0, U+13F8→U+13F0; and in `\w`, U+0345 and U+0301 are out while U+0660, U+00B2, U+2160 and U+1F100 are in.
- **`error.rs`:** the new `Kind` variants and the `pub(crate)` constructors are fine, and `Kind` is `#[non_exhaustive]`.

## Verdict
There is one P2 defect: unbounded memory and syscall amplification in `fs::canonical_path` (P2-1). Fix it before merge by lowering the symlink budget to 40, or by porting CPython's `seen` map with a `pending` cap. There are no P1 findings. Every `unsafe` block holds its invariants. The rest of the new T0 surface is correct; P3-1 to P3-6 are parity, test-strength and maintainability items.
