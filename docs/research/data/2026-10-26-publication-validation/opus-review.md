# Static review: `memory_publication.land` timeout (runtime 1099ca6)

I only read the source. I ran nothing, so none of the numbers below are measured. The work counts come from reading the code, and the root cause stays unproven until profiling confirms it.

## 1. Most likely mechanism: `land` repeats a whole `prepare` pass more than twice under the same 300s deadline

`land` (`memory_publication.py:383-432`) makes these full-bank passes in order:

| Step | Call | Line |
|---|---|---|
| a | `request_value` → `recovery.tree(candidate)` + `bank.validate_files(after)` | 260-261 |
| b | `recovery.replay` (pre-retry readback) | 388 |
| c | `semantic.check` #1 | 395 |
| d | `semantic.check` #2, the deliberate repeat before push | 410 |
| e | `semantic.final_sources` | 417 |
| f | `recovery.replay` (post-push readback) | 422 |

`prepare` (226-245) is about one `check`, one `tree` and one `final_sources`. So `land` costs roughly 2·check + 2·replay + extras compared with prepare's 1·check + small. With prepare at about 134s, land would be at about 268s for the two checks alone, before either replay. Going past 300s is the expected result, not a surprise. If the branch is protected, `policy()` runs twice and `protected_candidate()` runs twice, adding more `gh` round trips (each up to 30s, `memory_publication.py:40-50`). Profiling is needed to tell whether the excess time is CPU or network.

**Ruled out statically:** waiting on locks. `local_registry.exclusive` uses `O_EXCL` and `LOCK_NB` and fails fast (`local_registry.py:42-50`), so a lock wait cannot be what is hanging.

## 2. Repeated full-bank work, counted from the code

Units used below:
- **V** = `bank.validate_files` on about 10k files (frontmatter parse, `records.validate` and `inherited`/`scopes.decide` per record; `bank_layout.py:82-167`).
- **S** = `security.scan` over every file.
- **D** = `writes.snapshot` = hashing the bank twice + `contract.validate` (rglob twice, a full disk read, `canonical_paths` twice, then V) (`bank_writes.py:48-59`, `bank_contract.py:340-381`).

A single V is linear in file count. The cost comes from how often it repeats.

- **`observe`** (`sharing_recovery.py:270-320`) ≈ 4D + 3V + 5S:
  - `sharing_boundary.inspect` runs twice (272/310), and each run takes 2 snapshots plus a full scan (`sharing_boundary.py:50,116-128`).
  - The quarantine `bounded(files_at)` adds a read + V + S (278).
  - `tree(head)` ends with `bounded` (186), and then `bounded(files)` runs again on the same bytes (306).
- **`replay`** (527-571) ≈ observe + 5V + 5S + k·(V+S):
  - `tree(base)` and `tree(candidate)` each validate (539), then `bounded(before/after)` re-validates the same maps (540).
  - `tree(head)` at 564 has the same bytes that `observe` already validated.
- **`frontier.build`** (`sharing_frontier.py:197-262`) runs `snapshots`, which is observe + `tree(base/head)` + `bounded` × 3 on maps already validated (`semantic_sharing.py:48-50`). It then runs `qualified_current` (2V, `sharing_hold_mapping.py:86`) and `final_sources` (D).
- **`review`** after build (`semantic_sharing.py:377-554`):
  - `qualified_current` runs again: 2V (404).
  - **Superlinear:** inside the `child_paths` fixpoint, every seed × 3 sides × every iteration calls `bank.validate_files` (474 → 210). That is O(H·V·iterations), where H is the number of held or seeded paths.
  - **Superlinear:** in `mask`, `container_paths(baseline)` sits inside the loop over held body paths (254 → 186), so each held path costs one V. On top of that, each `mask` call does about 5V (226, 236, 294), and it is called twice.
  - `merge_files` runs twice, each with 4× `bounded` (`sharing_recovery.py:382,481`): 8V + 8S.
  - `preserve_invalidation` runs twice at about 4V each (`semantic_sharing.py:298-315`).
  - `consolidate` → `qualified_sources` costs one V per target (363).
  - `final_sources` adds D, and `sharing_holds.record` calls `verify` twice, adding 2D (`sharing_holds.py:237,266`).
- **`frontier.selected`** (`sharing_frontier.py:161-162`) costs changed keys × all rows × searches (2–8). It calls `retrieval.match` and rebuilds `str(value['effective_framing'])` for every row on every search. This is O(changed·N·S), and it runs again in each `check`.

Per `check`, that comes to roughly 8D + 40V + 18S, plus O(H·V), plus the `selected` cost. Most of those V and S runs parse **byte-identical maps**: the same `base`, `landed` and `local` maps over and over.

**What profiling must still establish:** how the time splits between V, S, D (disk), `selected`, and `git`/`gh` subprocess wall time, and what H and k were for session0.

## 3. Smallest safe optimization: reuse pure validation only for identical content, within one operation

**What to add:** a per-process memo, active only inside `land` and `prepare`, that is created on entry and dropped on exit. It is never persisted and never shared across sessions. It covers three pure functions:

1. **`bank.validate_files(files)`**
   - Key: `digest(encoded(tx.file_hashes(files)))`. That is SHA-256 over every path's exact bytes.
   - Skip the memo whenever `parsed_records is not None`.
   - Cache successes only.
   - Return a deep copy (or make sure no caller mutates the result).
2. **`recovery.bounded(files, quarantine_records=…)`**
   - Key: the same content digest plus `quarantine_records` and `allow_absent`.
3. **`security.scan(content, maximum=…)`**
   - Key: `(sha256(content), maximum)`. It is pure regex (`security.py:48-61`).

**Unchanged, every time:**
- Every disk read and both before/after hash passes in `snapshot` and `files_at`.
- `canonical_paths` before/after.
- Every `ls-tree`/`cat-file` call with per-blob hash verification.
- Every `ls-remote`/`server()` call, fetch, `final_sources`, `index_bytes` check, both `semantic.check` calls, both `policy()` calls, the post-push `replay`, history and tombstone inventories, holds `record`/`verify`, and all bounds.

Any change to the source produces a new digest, so it is re-validated in full. Nothing is ever inferred as absent, and no observation is skipped.

**Hazards to guard against:**
- **Keys:** never key by path, mtime, commit OID alone, or Python object identity.
- **Key cost:** the key must hash the full map every time, which costs about 10k SHA-256 calls. That is cheap compared with V, but should be measured.
- **Returned results:** callers must not mutate them. Today `checked['entries']` is read-only in the call sites I traced, but a deep copy is the safer choice.
- **Exceptions:** do not cache them.
- **Lifetime:** the memo must not leak into other CLI entry points or long-lived importers. Use a context variable that is reset in a `finally` block.

**Optional follow-ups (same principle, do only if the profile points there):**
- Hoist the per-row framing string out of the per-search loop in `selected` (`sharing_frontier.py:162`) by computing it once per row. This is pure and has no semantic change.
- The `child_paths`/`mask` repeats in §2 are already removed by the memo, without changing their logic.

**Do not:** raise the deadline, drop the second `check`, reuse an `observe` result across steps, skip the post-push `replay`, or narrow which sides or history get validated.

## 4. Evidence to collect next

1. **Profile one land.** Run `cProfile` on a disposable reproduction of the session0 configuration with the public helper. Wrap `raw_command`/`command`/`api` to log per-subprocess wall time and argv[0..2], so `git` and `gh` time is separated from Python CPU time.
2. **Count calls.** Count calls to `validate_files`, `bounded`, `scan`, and `snapshot`, and count distinct content digests for each. If calls far outnumber distinct digests, the redundancy is confirmed. Also record H, the fixpoint iteration count, k, and the changed count in `selected`.
3. **Regression tests.**
   - On a synthetic bank, land at N=1k and N=4k files. Assert that the number of `validate_files` parses per distinct digest is 1, and that wall time grows about linearly.
   - Negative tests with the memo enabled. Each must still refuse or report pending:
     - Mutate one local record between check #1 and check #2 (a `writes.fault` hook at `publication-before-final-review`).
     - Advance authority before the push.
     - Change the original index.
     - Add an unsafe changed payload under v2.
   - Assert the memo is empty after `land` returns or raises.
4. **Session0 before any retry.** The timeout may have killed the process after the push at line 419. The first `replay` in the next `land` (388-393) is the correct inventory for that, so keep it, and do not hand-push.