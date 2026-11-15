# Independent review — ADR-0025 / RCA / checked-page export design (#209)

**Verdict: PASS** (no blockers). This is a technical review only; it is not operator approval of goal, protocol, or architecture, and it does not accept any implementation.

## Verification performed (evidence)

**Source-line claims vs pinned source** (`209-source-clean`, matching pin 33c95bb):
- `sharing_frontier.page:269–273` unconditionally calls `build` and returns one page after whole-manifest equality — verified exactly (page:269–272, `current=build(...)`, `current[2]!=manifest` check).
- `build:197–262` — verified: snapshots → components → holds → selection → tombstone/hidden withholding closure → `page_payloads` → manifest → `final_sources`, returning `(snapshot, chosen, manifest, pages)` with manifest page descriptors pinning exactly `index,sha256,bytes,source_keys,edges`, plus `known_inventory_sha256` — matching the design's binding claims.
- `selected:130–178`, `page_payloads:181–194` — verified, including per-item trial re-encoding (resets at page boundary; RCA correctly refuses the "globally quadratic" framing).
- `semantic_sharing.check:557` via `prepare_review:276` → `build` — verified.
- `memory_refresh.prepare` performs two checks (431 direct + 420 via `validate_transaction`); `memory_publication.prepare` one (285); `land` two (454, 472) with policy/index recheck before push — all verified.
- `bank_transactions:390–393, 437–440` — `validate_refresh` before and again under the lock, each skipped only on an existing receipt — verified at exactly those lines; the under-lock branch re-tests receipt existence, so no TOCTOU hole in the skip.
- `sharing_recovery.observe:282` — authority head checked before and after fetch, disposable ref with empty refmap, local boundary/HEAD recheck — matches RCA claim 2.
- `validation_memo` — operation-local, bounded (64 MiB / 65,544 entries / 8 maps), never stores failures, no authority/time/policy memoization — matches RCA claim 5 and the ADR's "bounded and intentionally conservative" characterization.
- `#244 recovery_frontier` API: **absent** in the analyzed tree (no match anywhere under `agent_memory/`), consistent with both docs treating it as a blocking integration dependency rather than inventing a substitute.
- Design-named primitives exist as described: `private_workspace.checked_directory/write_fd` (`O_EXCL|O_NOFOLLOW`, 0600/0700), non-exclusive `local_registry.private_mkdir` (correctly excluded), `sharing_limits.MAX_PAGE=65536`, `MAX_PRIVATE=512MiB`, `write_private` replace semantics, receipt `review_pages` (`sharing_limits:86`), public `reconcile_memory.py` entry point.

**Profile-number claims vs data/raw runs:**
- Sanitized JSON matches the RCA tables exactly: inspect/page walls 74.313/74.034 s; snapshots 50.419/49.713; components 10.431/10.238; final_sources 6.822/7.194; validate_files 15 calls, 33.122/33.223 s; page_payloads 0.013 s; pipeline walls 136.153/136.140/320.052/256.179/344.249 with build counts 1/1/2/2/2, observe 8/9/25/17/26, validate_files 39/41/106/81/100 and quoted inclusive seconds; apply `validate_transaction` 2×=244.491, `validate_outcome` 3×=20.519.
- Raw runs confirm the counts quoted only in prose: page posix.open 721,824 (inspect's 721,826 correctly not quoted), frontmatter.read 123,595, json.loads 2,665,103, land/apply frontmatter.read 549,396/559,727. Pipeline `outcome.json`: complete=true, banks preserved, `full_workload_or_speedup_claim=false`.
- Arithmetic: 56,461.760/141,308.115 = 40.0%; 141,308.115/76,620.196 = 1.84 in-flight — both as stated, with the overlap caveat correctly preventing wall-time decomposition.

## Assessment against the review dimensions

1. **RCA completeness, dominant paths:** The seven listed commands cover ~88.5% of summed command time, and focused profiles now cover every one of them (inspect/page/review/prepare/land/apply). The two-part diagnosis — (a) per-page whole-frontier rebuild, (b) independently required re-observation across review/prepare/land/apply — is source-proven and profile-confirmed (page build ≈ 73.7 s vs page_payloads ≈ 0.013 s; land/apply each 2 builds + 25–26 observes). Complete.
2. **Causal/profile limitations:** Explicit and correct — no per-command timestamps in baseline, nested inclusive times not summable, cProfile overhead, sparse 1-page fixture ≠ dense frontier, load samples non-causal, memo hits inflating scan counts. No overclaim found.
3. **SOTA comparison:** Primary sources; every dated citation checks out (RFC 9162 Dec 2021 Experimental; RFC 9111 June 2022 Standards Track; Build Systems à la Carte 2018; Georges et al. OOPSLA 2007); undated vendor docs are consistently marked undated with the explicit non-assertion of release dates. Non-exhaustiveness is stated. Compliant with the cutoff rule.
4. **First candidate follows evidence:** Yes. Page-v2 is the single largest bucket (40% of summed time, 964 calls), each repeating a ~74 s build to emit a ~0.013 s page; exporting the already-built checked pages from one build is the minimal intervention. Alternatives table is honest about trade-offs; no speedup is claimed.
5. **Residual parsing cost:** Correctly fenced as a separately identified opportunity (b) requiring its own design, with the memo's actual limits cited accurately — not smuggled into the first candidate.
6. **Invariants:** Design preserves every boundary: hashes = identity not currentness/permission; marker = disk completeness only; fresh final_sources + hold-inventory digest recheck + frontier token before/after; exclusive no-follow owner-only artifacts outside canonical roots; 64 KiB/512 MiB caps, refusal over truncation; both comparison arms share one real page consumer; negative controls retained; no lease/atomicity claim. No weakening found.
7. **Qualification not weakened:** `full_daily_use_v1_acceptance=false` and `required_fault_matrix_complete=false` are stated in both docs; 24/10k/2-round/4-worker cells retained on both platforms; 300 s timeout failures retained; 3600 s guard and 20%/15-min targets explicitly unapproved; busy host remains eligible without tuning.
8. **Dependency/order accuracy:** #244 genuinely absent and correctly blocking; order chain (review → operator approval → freeze/baseline → implementation → qualification → acceptance) is coherent; #206/#249 correctly not invented as hard dependencies.
9. **Proposed vs accepted:** Front matter `status: proposed`; the ADR repeatedly and correctly denies implementation/target/protocol approval, scopes the prior Opus review as supporting evidence only, and defers ADR-0016 amendment to an accepted version.

## Blockers

None. Every checkable factual claim I tested matched source, data file, or raw runs.

## Nonblocking suggestions

1. **Durability of quoted counts:** The posix.open/frontmatter.read/json.loads and land/apply frontmatter counts quoted in the RCA exist only in the raw `/tmp/...run-01` profiles, not in the sanitized `docs/research/data` JSON (its function lists are filtered). Since `/tmp` is volatile, add these sanitized counts (numbers only) to the durable data file, or note that the durable sanitized copy intentionally omits them.
2. **RCA pipeline table completeness:** `refresh-review` (127.715 s, 1 build, data present in both JSON and raw) is profiled but not listed in the completed-pipeline table; adding it would make the table cover the data file one-for-one.
3. **complete.json integrity chain:** Page digests are reachable only via `manifest_artifact_sha256` → manifest descriptors. That chain is closed, but a one-sentence statement in the design that per-page digests are intentionally pinned transitively (not duplicated in the marker) would preempt a reviewer misreading the marker as self-sufficient.
4. **#244 contract drift:** Step 6 pins "recovery_frontier.observe/check" by name against a not-yet-landed API; the "exact contract equivalence" clause covers semantics, but consider stating explicitly that naming/shape drift in the landed #244 API triggers design re-review rather than ad-hoc adaptation.

## Separate flag: underlying pre-existing product observation (diagnosis only, no scope expansion)

No correctness bug was demonstrated by this diagnosis. One pre-existing performance anomaly deserves its own separate diagnosis ticket rather than silent inclusion in #209: within a *single* operation on a ~10k-record bank, profiles show 123,595 `frontmatter.read` and 2,665,103 `json.loads` calls plus 15 `validate_files` invocations despite the operation-local `validation_memo` — its bounds (`MAX_MAPS = 8` for `bank-format`, LRU eviction) may be thrashing within one operation, i.e., the existing memo may be underperforming its own intent independent of any #209 optimization. This is an observation for separate investigation, not a request to widen the proposed implementation.

**Status note:** Operator approval of the numeric goal, measurement/qualification protocol, and final architecture remains outstanding and is not granted by this review.
