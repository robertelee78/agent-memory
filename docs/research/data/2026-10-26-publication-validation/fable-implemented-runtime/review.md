I found no concrete blockers. This was a static read only: I didn't run any code or tests, so none of this says anything about timing, the 300-second 10k landing, or release readiness.

## What holds up

- **Keys match the inputs.** The map key is a length-prefixed SHA-256 over sorted path/body pairs (`validation_memo.py:70-91`), so different maps can't collide. The scan key is the content bytes plus the budget, and only 16384 and 65536 are accepted (`:94-104`); any other budget skips the memo and fails as before (`security.py:57`). Non-dict input, non-string paths or bodies, oversize input and bad UTF-8 all skip the memo and get full validation.
- **The cached map result depends only on the bytes, apart from the one ambient check, which is redone.** `_validate_files` is identical to `before/…/bank_contract.py:312-337`. Its other dependencies are pure functions of the bytes: `inventory`, `records.validate`, `scopes.decide` and `time_value` read no clock, environment or filesystem. The only outside dependency is `normalize(canonical-remote)`, which can resolve `file://` paths and the working directory. On a cache hit, `fresh_guard` reruns `parse_router` on the current text (`bank_contract.py:320-321`), so that check stays fresh. Results don't depend on dict order, because entries follow the route traversal and topology uses sets, so replaying a result to a caller whose dict is ordered differently is safe.
- **Callers can't corrupt the cache.** A miss stores a deep copy and returns the original; a hit returns a deep copy (`validation_memo.py:32,46`). Failures are never stored, and oversized or unserializable results are simply not cached, never truncated.
- **Parsed-record output skips the memo entirely** (`bank_contract.py:314`). So `bank_observation.observe` (`bank_observation.py:107`) and catalog publication always parse fresh.
- **Lifetime and nesting are sound.** Each call of a decorated `main` gets a fresh memo, nested calls share the outer one, and only the owner resets it in `finally`, including on `SystemExit` from argparse. A `ContextVar` keeps it per thread, and the only thread in the pack (`managed_source.py:49`) doesn't validate anything.
- **The fresh checks are unchanged.** The decorated `main`s moved by exactly four lines (import plus decorator) in all four modules, and the call sites match `before/`. In `land`, `policy()`, `index_bytes()`, `final_sources`, `replay` and the second `semantic.check` all still run. Only the pure validate and scan steps underneath them can be served from the cache, and only for identical bytes.
- **Recall catalog.** Adding `validation_memo` to `ENGINE_ROOTS` changes the engine fingerprint, so existing catalogs are invalidated once. `operation` is still fingerprinted through the `@wraps` closure. `generation_details` refuses to run while a memo is active (`recall_catalog.py:107-108`), and every caller catches that and falls back to the full read: `retrieval.py:225,231`, `discovery.py:159`. `invalidate_after_change` doesn't call `generation()`, so invalidation after writes still happens inside these operations.

## Limitations and future hardening (not blockers)

1. **The byte budget counts JSON size, not memory.** Weight is compact JSON size + 256 (`validation_memo.py:35`). Python dicts for up to 8 maps of about 10k entries take several times that on the heap, plus a temporary deep copy on each store and hit. Actual memory can be well above 64 MiB, so the bound is finite but loose.
2. **Map eviction ignores namespace** (`:41-45`). Storing a 9th map pops entries oldest-first, which also flushes every scan entry older than the oldest map. This only costs performance.
3. **Any single file over 16 KiB disables map reuse for that whole map** (`:77-85`). If large records exist, the speedup for the 10k landing may not happen. Nothing in the static code shows the target workload stays under this cap.
4. **The catalog is now off for `am refresh apply`'s recall.** `memory_refresh.apply` calls `retrieval.retrieve` (before-version line 197) while the memo is active, so it can neither load nor publish a catalog (`catalog_updated: false`). Correctness is fine because it falls back; it's a behaviour change worth stating.
5. **Small accuracy issues.** The docstring's "never stores … scope … decisions" is a bit loose: cached results include the pass of `inventory`'s inheritance `scopes.decide` check. That is still purely a function of the bytes. Also, setting `MAX_MAPS` or `MAX_ENTRIES` to 0 would still store one entry; the shipped values are positive, so this doesn't matter in practice.

## What the tests cover

`tests/contracts/test_validation_memo.py` covers:
- mutation isolation, and changed or renamed maps;
- the `file://` symlink recheck on a hit;
- scan budget separation, and that callers can't change cached scan results;
- unwinding of nested operations;
- small-budget eviction and oversize fallback;
- the catalog refusing to run under an active memo;
- `land` with source, authority, index and policy changes after the first check.

The parsed-records test (`:62-73`) only proves the memo is skipped. Nothing tests results across different dict orders, maps with a file over 16 KiB, or the catalog behaviour under refresh `apply`.
