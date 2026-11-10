# Sharing performance RCA and alternatives — 2026-11-06

Research for existing [#209](https://github.com/robertelee78/agent-memory/issues/209), not implementation approval. Analyzed product: `33c95bbebef61e3ac6e58c5c59e0d411b6dc63e8`. Research cutoff: 2026-11-06, not the remainder of November. “State of the art” here means a primary-source comparison of applicable methods, not a claim to have surveyed every publication. Current vendor documentation is undated unless otherwise stated; access at the cutoff does not establish its release date.

## Completed workload, not full qualification

[Verified measurement](https://github.com/robertelee78/agent-memory/issues/209#issuecomment-6694386037): 76,620.196 seconds wall (21h17m); 24 sessions, 10,000 seeds, two rounds, four workers, two developer identities, three clones, 2,191 public command receipts, zero timeouts. Every controller-declared scenario completed, including post-round recovery/semantic controls. Exact source bytes/modes matched before and after. `complete_compatible_rounds=true`; `full_daily_use_v1_acceptance=false`; `required_fault_matrix_complete=false`. No Linux qualification follows. Fixture harness labels are not native-harness evidence.

This is the normally busy target macOS host, explicitly selected by the operator. Python actually used 3.15.0, unlike the earlier failed 3.13.12 observation. Earlier failed/confounded runs are not paired speedup denominators. The corrected third-capture queries and observation-only 3600-second stuck guard are declared controller differences; this is not original 300-second compliance.

| Command | Calls | Cumulative seconds | Share of summed command time |
| --- | ---: | ---: | ---: |
| semantic page-v2 | 964 | 56,461.760 | 40.0% |
| refresh apply | 56 | 20,664.969 | 14.6% |
| publication land | 100 | 12,355.382 | 8.7% |
| semantic review-v2 | 157 | 11,381.395 | 8.1% |
| refresh prepare | 54 | 9,293.278 | 6.6% |
| semantic inspect-v2 | 157 | 8,055.571 | 5.7% |
| publication prepare | 99 | 6,888.543 | 4.9% |

Summed command duration is 141,308.115 seconds: commands overlap, so this is NOT a wall-time decomposition. Their duration sum divided by enclosing wall is 1.84 average in-flight command intervals, not CPU utilization. The controller lacks command start/end timestamps; exact critical-path attribution is unavailable. First/second all-round preservation boundaries were about 8.13/16.13 hours; remaining controls continued through 21.28 hours. An optimization must retain that later work.

Supervisor collected 2,550 resource samples: one-minute host-load median4.34, p95 approximately8.01, range2.00–16.23 on18logicalCPUs. Child CPU totals were96,363.640user and43,003.568system seconds. Load is not latency attributable to other work. Samples contain owned-process lifetime-average CPU and RSS, not instantaneous CPU, disk wait, foreign-job attribution, thermal throttling or a causal quiet/busy comparison. No claim that contention explains the runtime is supported.

## Root causes and safety boundaries

Module paths below are relative to `skills/agent-memory/scripts/agent_memory/` at the analyzed commit.

1. **Repeated complete observation per page — source-proven.** `sharing_frontier.page:269–273` unconditionally calls `build`, compares the whole manifest, then returns one page. `build:197–262` obtains base/local/landed snapshots; creates effective-framing components; qualifies holds, selections and lifecycle closure; constructs every disclosure page; and finally rechecks sources. Fetching P pages repeats this entire operation P times. The freshness check is intentional; rebuilding all derived pages is not the only way to preserve the later semantic/currentness boundary.
2. **Expensive observation is broader than hashing.** `semantic_sharing.snapshots` calls `sharing_recovery.observe`, bounded Git-tree reads and complete bank qualification. `observe` checks authority before/after fetch, scans local sources, validates identities/policy, and reads authority objects. These counts are source-derived, not measured network latency. The baseline's GitHub transport is a local fixture shim, not an Internet throughput benchmark.
3. **Refresh/publication repeat independently required checks.** `semantic.check:557–566` re-reviews its v2 judgment through `prepare_review` and `build`. `memory_publication.prepare` performs one such check; ordinary `land` performs two, with replay/source/index/policy/push/readback boundaries. `memory_refresh.prepare` performs two checks. Fresh `refresh apply` calls `bank_transactions.apply`, which invokes `validate_refresh` before and under the bank lock (`bank_transactions:390–393,437–440`); each calls `semantic.check`. A grep limited to `memory_refresh.apply` misses these indirect builds.
4. **Apply has additional complete-bank work.** Transaction snapshot/preflight, three outcome validations around durable receipt fault boundaries, post-apply authority observation, useful recall, and final snapshot reread the bank. Existing-receipt replay skips the two semantic checks but still performs observation/recall. These boundaries have distinct state-loss/TOCTOU purposes; deleting them as “duplicates” would be an unsupported fix.
5. **Parsing, filesystem traversal and derived framing recur.** Existing `validation_memo` is finite, pure-result and operation-local; it does not survive separate page processes. It deliberately does not memoize authority, time, policy, applicability, holds or semantic judgments. Existing format-map reuse also has oversized-input fallback. Raw scan-call counts include memo hits and are NOT scanner misses or causal percentages.
6. **Selection/pagination algorithms have additional costs, not yet the primary explanation.** `selected:130–178` matches changed meanings against all components and repeatedly stringifies effective framing. `page_payloads:181–194` encodes a trial growing page and then the accepted page per item. Work grows within each page but resets at64KiB boundaries; calling this globally quadratic in total item count is incorrect. Across P separate page requests, whole-frontier work does multiply by P. Neither algorithm's share may be inferred from source alone.

## Focused runtime attribution

Disposable public-CLI cProfile diagnostic, same pinned Python3.15/runtime, fresh project/identity and copied equivalent48authored+10,000seed bank; retained original bank never mutated. One authored record gets a fixture-only evidence append. This is an actual public `inspect-v2` and `page-v2`, not mocked timings. cProfile overhead is present; the request has2changed components, pending selectors and1page, so it is NOT an equivalent full-workload timing or a dense-selection benchmark.

| Profile observation | inspect | page |
| --- | ---: | ---: |
| Public wall seconds, including profiling | 74.313 | 74.034 |
| Full build calls | 1 | 1 |
| snapshots inclusive seconds | 50.419 | 49.713 |
| components inclusive seconds | 10.431 | 10.238 |
| final_sources inclusive seconds | 6.822 | 7.194 |
| validate_files calls | 15 | 15 |
| validate_files inclusive seconds | 33.122 | 33.223 |
| subprocess.run calls | 353 | 353 |
| subprocess.run inclusive seconds | 4.346 | 4.213 |
| page_payloads inclusive seconds | 0.013 | 0.013 |

Nested inclusive times overlap and MUST NOT be summed. Exact returned page digest matched its manifest; fixture and original retained bank hashes were unchanged. The page's profile includes721,824`posix.open`calls,123,595frontmatter.read calls and2,665,103json.loads calls. These are profiler call counts, not numbers of distinct source files. That confirms repeated parsing/path work, not a network-only or serialization-only diagnosis. For this sparse frontier, materializing the page itself is tiny compared with repeating source observation. The public page process repeats essentially the complete inspection workload.

### Completed publication/refresh profiles

A second fresh equivalent10kfixture completed14publicCLIcommands, including real reviewed publication and peer refresh; every command exited0. Copied/retained bank hashes were preserved and the peer's final complete bank matched the publisher. This fixture uses one authored evidence append and a fresh peer worktree; it does not duplicate the full stress topology or dense interaction frontier.

| Profiled operation | Public wall seconds | build calls | bank observe calls | validate_files calls | validate_files inclusive seconds |
| --- | ---: | ---: | ---: | ---: | ---: |
| publication review | 136.153 | 1 | 8 | 39 | 51.461 |
| publication prepare | 136.140 | 1 | 9 | 41 | 54.108 |
| publication land | 320.052 | 2 | 25 | 106 | 130.711 |
| refresh prepare | 256.179 | 2 | 17 | 81 | 96.679 |
| refresh apply | 344.249 | 2 | 26 | 100 | 139.396 |

The profile confirms the indirect two-build fresh-apply path. Apply's two `validate_transaction` calls total244.491inclusive seconds; its three `validate_outcome` calls total20.519inclusive seconds; useful recall totals9.789inclusive seconds. These are nested scopes, not disjoint removable costs. The full land profile has549,396frontmatter.read calls; apply has559,727. Existing operation-local memoization does not eliminate filesystem-dependent normalization and all other traversals.

Thus the complete dominant-path RCA is **(a)** repeated whole-frontier work for each page and **(b)** repeated full observation/format/parsing work across independent correctness boundaries in review/prepare/land/apply. A page bundle targets(a), not all of(b). Subprocess waiting, file traversal and Python parsing all occur; these profiles do not support blaming network alone, more workers alone, or page encoding alone. No exclusive wall-time percentage for(b), host-contention percentage, dense-frontier bound or measured candidate speedup is claimed. Those are explicit residual measurement limits, not hidden “complete RCA” claims.

Private complete pipeline profiles and receipts: `/tmp/agent-memory-v15-readiness/209-rca-pipeline-profile-run-01`. cProfile overhead and single-fixture differences mean its timings cannot be subtracted from the unprofiled21h17mbaseline.

## Primary-source alternatives study

| Method / primary source | Verified principle | Fit and correctness cost |
| --- | --- | --- |
| [Bazel remote caching](https://bazel.build/remote/caching), current docs, undated | Separate action-result lookup from content-addressed blob storage | Exact result bytes can be reused by input/validator identity. Does not establish current authority or that all dependencies were observed. A shared remote cache is unnecessary for the initial local fix. |
| [Remote Execution API GetTree](https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto), current schema, undated | CAS root digest plus continuation token; traversal order is not generally stable across independent invocations | Bind all pages/order to one immutable manifest. Do not import GetTree's missing-subtree omission into a no-truncation semantic protocol. Existing complete-page hashes already suffice; no new Merkle protocol is required. |
| [Elasticsearch search_after + PIT](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/paginate-search-results), current docs, undated | Continuation needs stable sort/tiebreaker; refresh can change results; PIT preserves index state. Prefer PIT/search_after over scroll for deep pagination | Retain a snapshot identity rather than recompute a moving query. This is a design analogy, not a proposal to install Elasticsearch. Snapshot readability is separate from permission to publish against current authority. |
| [SQLite isolation](https://sqlite.org/isolation.html) and [WAL](https://sqlite.org/wal.html), current docs | WAL readers retain a transaction snapshot; long readers affect checkpointing; WAL requires shared-memory coordination on the same host | Could persist derived inventory, but introduces migrations, cache invalidation, crash/connection lifetime and portability burdens. No database-backed authoritative memory. Defer unless simpler bounded reuse is insufficient. |
| [PostgreSQL SET TRANSACTION](https://www.postgresql.org/docs/current/sql-set-transaction.html), current docs | Imported snapshot must precede the first query and use suitable isolation; the snapshot comes from an exporting transaction | Server/transaction lifetime does not fit independent portable CLI invocations. Materialized local artifacts provide the relevant property without a database server. |
| [Nix content-addressed outputs](https://nix.dev/manual/nix/2.33/store/derivation/outputs/content-address), versioned manual, publication date unverified | Fixed-output content hash is checked against the produced output; content identity differs from a derivation/action identity | Check exact page bytes, not just a filename/cache key. No need to introduce experimental floating CA or Nix itself. |
| [Mokhov, Mitchell, Peyton Jones: Build Systems à la Carte](https://www.microsoft.com/en-us/research/wp-content/uploads/2018/03/build-systems-5ab0f42d0f937.pdf),2018preprint, §§2–4 | Separate scheduling from deciding what to rebuild; verifying traces check dependency hashes; constructive traces retain results; dynamic dependencies must be tracked | An incremental source-view cache must include framing/router, lifecycle, holds, executing validator/scanner, policy and authority dependencies as appropriate. Content hashes alone are not complete applicability/currentness dependencies. A dirty bit/mtime-only shortcut is not enough. |
| [RFC9162](https://www.rfc-editor.org/rfc/rfc9162.html), December2021,Experimental | Domain-separated hash trees and inclusion/append-only consistency proofs | Useful model for immutable identity. Memory edits/deletions are not an append-only transparency log, so consistency proofs do not prove absence/currentness or nonresurrection. Existing manifest digests are simpler for this workload. |
| [RFC9111](https://www.rfc-editor.org/rfc/rfc9111.html), June2022,StandardsTrack | Cache identity/freshness and revalidation are separate protocol concepts | Do not turn a durable content hash or artifact-complete marker into a currentness lease. This is an analogy, not HTTP TTL semantics for memory. |
| [Georges et al., Statistically Rigorous Java Performance Evaluation](https://users.elis.ugent.be/~leeckhou/papers/oopsla07-stat.pdf),OOPSLA2007 | Scheduling/system effects vary runtime; choosing best runs can mislead; experimental context and uncertainty matter | Preserve every attempt; one successful busy-host run is a measured operating point, not a distribution or significance result. Java/JIT details are not automatically applicable to this Python CLI. |
| [pyperf system guidance](https://pyperf.readthedocs.io/en/latest/system.html),currentdocs,undated | CPU affinity/isolation and environment metadata affect benchmark stability | Operator explicitly wants normal busy-host capability, so do NOT tune or stop their workload. Keep shared-load operational measurements separate from focused causal profiles and document variability. |

A search covering2025/2026pagination surfaced production guidance and secondary summaries, but no verified new November2026method justifies replacing established snapshot/content-addressing principles. This is not evidence that no newer method exists. Floating documentation versions are not asserted to have been released before the cutoff merely because their current pages can be read.

## Comparative decision criteria

- **Optional checked-page export:** removes P whole rebuilds while keeping independent fresh review/prepare/apply/land. Lowest new authority surface; private bounded artifacts/crash markers and stale-source refusals are mandatory. Strongest first candidate, not demonstrated speedup.
- **Single-process streamed/batched page RPC:** similar compute reuse but changes process/session lifetime, cancellation, restart and consumer transport. Feasible alternative if artifact persistence is unacceptable; no reason yet to add a daemon.
- **Operation-local immutable parsed source view:** addresses repeated parsing across independently required rechecks while actual source/index/policy/hold reads still run fresh. Must distinguish pure format results from filesystem-dependent authority normalization. Existing memo demonstrates both the pattern and its limits. Needs precise contracts before implementation.
- **Persistent Merkle/CAS source index:** potentially avoids more work, but non-Git working files, staged/index state, router/framing closure, hold store, validator version and changing authority remain dependencies. More invalidation/fault cases than bounded export. Consider only if residual profiles warrant it.
- **Higher worker count/parallel landings:** can mask per-operation cost or increase contention and stale retries. Changes the required schedule; not an accepted substitute for eliminating redundant work.
- **Larger limits, shorter workload or padded success:** do not fix repeated work and do not meet #209. Reject.

## Evidence retention

Raw private source-bound profiles/fixture streams remain under `/tmp/agent-memory-v15-readiness/209-rca-profile-run-01`; baseline receipts are durably indexed under local state `research/209-baseline-handoff/corrected-shared-20261106/`. Raw artifacts can contain fixture credentials and withheld bodies: publish sanitized statistics and digests only. Scripts run with isolated Python and bytecode writes disabled. No product optimization or canonical-checkout modification occurred.

## 2026-11-10 residual refresh failure and component attribution

This addendum has its own November 10 observation date; it does not extend the
November 6 literature cutoff retroactively. The second frozen S0 run
(`b16e1ff3918afc3c8885e87b729ce26dfd0ea6b1`) terminated unsuccessfully after
26,990.36 seconds: 625 commands, six 300-second `refresh.apply` timeouts, 618
successful exits and one expected missing-page refusal. Cleanup is unknown,
failed Worlds remain retained, and candidate/Linux stress arms were not started.
The prior independent 641-command failure remains a separate failed attempt.

Read-only, standard-library forensics authenticated all six prepared inputs,
journal proposals and original hash maps. Each operation had 10,220 base files,
10,289 intended files and 70 changed paths. The journals and actual bytes show
legal ordered applied prefixes of 24, 24, 23, 27, 43 and 44 changes, with every
router still at its original state and no canonical transaction receipt.
Therefore all six entered the mutation loop; they did not spend the entire
deadline waiting for its ordinary nonblocking lock. Exact historical guard
durations and time spent before mutation remain unavailable.

A new owner-private diagnostic reconstructed the authenticated 10,220-file,
10,584,148-byte base separately from every failed World. Current runtime
`d96020632201d3f6723d5441f03e8f267eca3627` was independently authenticated from
Git before importing a cache-free source export with official macOS Python
3.15.0, `-I -B`. These are component measurements, not a replay or acceptance
of either the old or current complete workload.

| Component | Profiled wall seconds | Measured detail |
| --- | ---: | --- |
| Complete source scan | 0.817763 | 10,220 reads; 51,087 Python `posix.open` calls, 0.388430 self seconds |
| Complete layout check | 0.804451 | 82,034 Python `posix.stat` calls plus path construction/processing |
| Cold format validation | 4.652352 | One ordinary operation-local memo entry |
| Warm format validation | 0.334001 | `deepcopy` 0.302086 inclusive seconds; no observation/currentness reuse |

The Python call counts are not independently traced kernel syscalls. Profiling
overhead is present. The transaction performs two source scans and two layout
checks per changed path, in addition to initial/final semantic and safety checks.
Multiplying these two profiled samples by 140 gives approximately 227.1 seconds;
that is an inference, not a runtime prediction or proof of historical timeout
attribution.

A separate unprofiled, ordered one-worker then four-worker diagnostic ran three
read-only scan/layout pairs per worker on the same authenticated base, with
source/input verification before and after. Mean pair wall time was 0.916390
seconds for one worker and 1.563620 seconds for four concurrent workers. This
short sample demonstrates cost amplification in that arrangement, not a
distribution, isolated contention effect or complete apply duration. No host
quieting, workload change or deadline extension occurred.

The first diagnostic inadvertently read preceding token-bearing command-receipt
lines while locating an input hash. It did not parse, retain, emit or use token
values. This protocol deviation is retained; the extension used only the known
journal and already authenticated diagnostic inputs and did not repeat it.
Private reports and receipts remain owner-only.

### Applicable methods checked for this residual

- [PEP 471](https://peps.python.org/pep-0471/) explains reducing directory/path
  overhead and explicitly warns that `DirEntry` metadata is cached. Reusing a
  directory-entry result cannot replace a fresh authorization check.
- [Python 3.15 descriptor-relative filesystem APIs](https://docs.python.org/3.15/library/os.html)
  and [open/openat](https://man7.org/linux/man-pages/man2/open.2.html) permit
  reducing repeated path resolution. An open descriptor remains valid after
  rename, so fresh canonical ancestry-to-descriptor binding remains essential.
  Linux-only `openat2` is not a portable Mac/Linux solution.
- [Git's racy-stat analysis](https://git-scm.com/docs/racy-git) rejects treating
  unchanged timestamps/size as general proof of unchanged bytes. No stat-only
  source cache or watcher-as-authority is selected.
- [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html) separates
  journaling, cooperative locks and durability assumptions. It does not justify
  treating AM's lock as excluding editors or replacing canonical Markdown with
  a database merely to avoid repeated checks.

The proposed narrow direction is cheaper mechanics *inside* each existing check:
one internal no-follow parent descriptor chain per scanned directory, freshly
rebound before reused sibling reads and after every read. Standalone reads and
the first leaf retain their existing sequence. Direct fresh no-follow layout
stat/path processing does not cache observations across checks. No safety
boundary, per-path full read set, fault, cap, authority check or 300-second limit
is removed. Fable passed the first design; Opus required precise error mapping,
sequence, post-read race coverage and incomplete-traversal handling. The revised
design and ADR clarification require actual closure review before integration.

A separate direct internal `replace_file` component completed all 70 original
ordered changes in five new private copies: 136.380 seconds for one worker and
208.555–211.401 seconds per worker with four concurrent workers. Each worker made
140 observed-bank and 140 layout checks. Those guard totals dominated the
component; staging, marker checks, file/directory flushes and wrapper overhead
were a small residual in this sample. All copies matched the complete intended
hash map, with source export and marker unchanged. Public apply's other
validations, locks, journal, receipt and readback were not run, so these results
do not prove complete-operation compliance. The live integration HEAD/index
changed through the parent's concurrent research commit; both observed states
were clean, and the independently archived runtime imported by every worker
remained unchanged. Failed Worlds were never mutated.
