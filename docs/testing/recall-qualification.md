# Source-bound recall qualification (#206)

These controllers implement the approved B4/B5 protocol. Their presence is not a
performance, quality, memo-retention, Linux, Python-matrix or release acceptance
result. Run them only in the integration owner's exclusive host slot: timing,
model qualification and #209 stress must not overlap. The historical `33c95bb`
measurements are diagnostics, never the integrated comparison denominator.

## Environment and immutable builds

Use clean, ordinary Git clones, Git with local-file transport, and CPython 3.11
and 3.15 on the supported macOS and Linux host classes. The controllers and
public fixtures use the standard library; no graph, vector store, service or
model is needed for navigation, refresh or memo instrumentation. Semantic
quality alone needs the permitted real invoking model, its executable,
credentials/normal permissions and recorded provider usage/cost. A missing
provider or platform remains unqualified, not a simulated pass. Allow enough
private disk for retained full Worlds, source/proof artifacts, outputs and exact
memo inputs. Per-child refresh deadlines default to 7,200 seconds; whole matrix
children default to 172,800 seconds. These are supervision limits, not targets.

After freezing the final integrated source (including controller changes), run
from that clean checkout. Select **one** interpreter/environment per invocation;
use new output roots for every platform/Python cell. All paths below are actual
controller arguments; set `ROOT` to the final clean checkout and `EVIDENCE` to a
new external private directory, not a bank or source directory.

```sh
ROOT="$(pwd -P)"
PYTHON=python3.11
EVIDENCE=/tmp/am206-macos-py311
mkdir -m 700 "$EVIDENCE"
"$PYTHON" -B "$ROOT/scripts/evaluations/recall_comparison_builds.py" \
  --root "$ROOT" --out "$EVIDENCE/builds"
```

Repeat with `PYTHON=python3.15` and new macOS output; repeat both on Linux with
new Linux output roots. Do not infer any of those cells from another host or
interpreter. `builds/builds.json` retains full commit/tree and per-file source
inventories, actual public artifact inspection results/digests, dependency
versions and exact reverse patches. The builder creates independent clones:

* **A:** final integrated source minus only #206's `bank_capacity` closure repair
  and scoped catalog/memo suspension optimization.
* **B:** identical final integrated source with the repair only.
* **C:** full final integrated source.

The syntax-bound reverse operation refuses an ambiguous/moved seam. All files
outside the three reviewed #206 runtime paths must be byte/mode-identical. In
particular it never removes #244/#249 dependencies or copies the repair into A.
The controllers, frozen labels and every non-#206 feature remain identical. A
source/inventory change invalidates the evidence and requires a new frozen
comparison; do not carry forward old ratios. Before any local helper import,
controllers reject bytecode anywhere under `tests/`, `scripts/` and the portable
pack in the controller checkout; each target build receives the same check
before fixture imports or source binding. Linked `__pycache__` directories are
also rejected, including ignored, dangling or cyclic links, without traversing
them: a matching stale helper cache can otherwise be read before source binding.
`-B` alone only prevents writes and does not authenticate execution. Caches and
links are never deleted or repaired: use a new bytecode-free source-equivalent
clone instead.

Tracked symlinks are permitted only as archived data under
`docs/research/data/`, where negative fixtures intentionally include them.
The source inventory hashes their literal link-target bytes against the Git
blob and records their symlink type; it never opens the target. Dirty links
still refuse. Symlinks anywhere in executable `tests/`, `scripts/` or the
portable pack remain unsupported; no fixture exception relaxes those guards.
Active semantic-case and literal-label inputs still require frozen ordinary
files: the quality controller refuses link traversal, reads with a no-follow
nonblocking descriptor and verifies type, full mode and content against the
frozen source binding before any fixture/model execution. Parsing and retained
hashes use those same verified bytes, not a later reread.

## Complete serial commands

With the host slot and a permitted exact model selected, the complete controller
runs all sizes, builds and stages serially:

```sh
"$PYTHON" -B "$ROOT/scripts/evaluations/recall_qualification_matrix.py" complete \
  --builds "$EVIDENCE/builds/builds.json" --out "$EVIDENCE/complete" \
  --model "$QUALIFICATION_MODEL" --timeout 7200 --controller-timeout 172800
```

`QUALIFICATION_MODEL` is the integration owner's explicitly selected real model
identifier/version, not a role or an inferred provider substitute. By default
semantic quality invokes the existing `claude -p` public JSON interface, with
no tools or persistent session, `--model` and the frozen action JSON schema.
If the selected invoking model uses another permitted interface, pass
`--model-command /absolute/private/model-argv.json`, a JSON array containing the
real adapter command and arguments. Its stdin is the full JSON model request;
stdout must contain `action` (a full action object, or top-level action plus its
fields), `model`, `usage`, and `reported_cost_usd`, or the actual Claude
`structured_output`/`modelUsage`/`total_cost_usd` fields. `provider_version` is
retained when reported. Missing identity/usage/cost prevents quality acceptance.
Every reported top-level model, model-usage key and canonical identity is
validated independently against the requested exact identity. The sole provider
spelling mapping is `anthropic/claude-opus-5-5` to `claude-opus-5-5`, validated
against the retained first-party Claude receipt. Thus a namespaced top-level
model with canonical native usage keys is valid; spelling differences do not
mean different executing models. Native `modelUsage` entries still require
`provider: firstParty` and the exact unprefixed `canonicalModel: claude-opus-5-5`,
even when their keys use the namespaced spelling. Wrong/mixed actual models,
unvalidated aliases and effort/route suffix removal cannot qualify. A canonical
top-level spelling alone still needs the native usage receipt. The controller
does not generate a synthetic model response, choose a fallback or hide a model
failure. The adapter must invoke the actual selected model, not use labels.

Quality observations retain every ordered native public response separately
from sorted relevance accounting. Literal A/B/C observations use the same frozen
request and compare their native order directly. Semantic models navigate
independently: reading the same pagination evidence in one batch or two is not
itself a native regression. After each semantic finish, the controller replays
**every actual source-read request on A, B and C in that request's original
World**, with the exact captured discovery trace, request (including path order)
and rationale. The input hashes and all replay outputs/failures are retained.
Each observed response must equal its own build's replay, and all three replays
of that exact request must agree. Missing replays or ordering, source-record,
coverage, recovery IDs/count/`recovery_required_actions`, or working-byte
authority differences fail parity; observations are never sorted or discarded
to manufacture equivalence. Selected source authority and semantic finish
outcomes are also compared across the independent model runs.

Replay commands are marked `native-replay`; their child seconds and whole replay
wall are separate from the observed workflow's native, model and end-to-end
clocks. They are post-finish correctness checks, not performance samples or model
input. Protected bank/Git/index/staged-byte checks still cover the whole World,
and every executed replay pack must match its frozen portable inventory.

The private semantic outcome oracle is frozen and hashed before any model call,
never included in adapter input. Resolved cases and true no-match require
complete coverage; wall disagreement also requires clarification. Unknown
prerequisite/session and hidden/unsafe guidance require incomplete coverage and
clarification, not false no-match or permission to reveal withheld data. HEAD
evidence never certifies staged/working bytes. Both absolute scoring and A/B/C
parity enforce these flags; old observations without exact-request replay
evidence are not retroactively passing qualification.

The same stages can be invoked individually, in serial host slots, without
repeating previously retained observations:

```sh
"$PYTHON" -B "$ROOT/scripts/evaluations/recall_qualification_matrix.py" performance \
  --builds "$EVIDENCE/builds/builds.json" --out "$EVIDENCE/performance"
"$PYTHON" -B "$ROOT/scripts/evaluations/recall_refresh_qualification.py" memo \
  --builds "$EVIDENCE/builds/builds.json" --out "$EVIDENCE/memo" --count 1000
"$PYTHON" -B "$ROOT/scripts/evaluations/recall_qualification_matrix.py" safety \
  --builds "$EVIDENCE/builds/builds.json" --out "$EVIDENCE/safety"
"$PYTHON" -B "$ROOT/scripts/evaluations/recall_quality_qualification.py" semantic \
  --builds "$EVIDENCE/builds/builds.json" --out "$EVIDENCE/semantic" \
  --model "$QUALIFICATION_MODEL"
```

For a standalone public refresh matrix:

```sh
"$PYTHON" -B "$ROOT/scripts/evaluations/recall_refresh_qualification.py" refresh \
  --builds "$EVIDENCE/builds/builds.json" --out "$EVIDENCE/refresh-50000" \
  --count 50000 --samples 5 --timeout 7200
```

A 1k or 10k refresh invocation is runnable diagnostic evidence, never the 50k
acceptance gate. B4's 1k corpus is not a reduced session cohort: it still executes
all **24 independent worlds, two real public prepare/apply operations each**,
paired under A and C, plus separate instrumentation-off controls for both builds.
Counts derive only from completed public operations, never planned iterations.

## What is measured and retained

Navigation reuses `recall_navigation_qualification.py` and the existing authored
`semantic_navigation_fixture.corpus`: actual public start/node/node/read,
complete workflow wall, each child wall, first empty-derived-state observation
and five sequential measured samples at 1k/10k/50k. There is no OS-cache flush or
filesystem-cold claim. Known-path timing is not semantic quality.

Refresh reuses `recall_refresh_smoke.completed_request`, the existing public
sharing/recovery fixtures, semantic conflict fixture and the full retained
`/tmp/agent-memory-v15-readiness/206-refresh-50000-v5.py` protocol (SHA-256
`39398f0dd1353ac5b0a02bef4549ae2a264bb577d126b6c864a0086d8586ccf0`). Its fixture
is the same authored semantic-navigation corpus, with the real disposable
canonical authority retained. This is not an invented small refresh facade.
Each operation performs public v2 inspect, complete changed-source selection,
actual private page reads, review-v2, prepare and whole apply. Review is a fixed
mechanical fixture judgment, never a semantic model quality claim. The
`semantic-sharing-judgment-v2` schema is unchanged; final #244 outer refresh and
comparison-v2 integration is consumed as present in the frozen source.

Four conditions run under A/B/C on the **same restored frozen World**:

1. Primed, no-import, inventory-unchanged discovery-qualified recall.
2. Unchanged inventory with catalog load and publication disabled by the existing
   public `AGENT_MEMORY_NO_RECALL_CATALOG=1` control.
3. Actual peer import invalidating the prior inventory. Recall uses the existing
   `daily_use.py` non-discovery exact-ID request shape; a new actual discovery
   operation runs after import. No pre-import trace is reused or invented.
4. Real contradictory local/landed source claims retained as held conflict, with
   independently qualified guidance returned and disputed paths withheld.

Each condition retains separate empty-derived-state/priming evidence and an
initial refresh observation outside the five sample medians. Full command argv,
input artifacts/hashes, raw stdout/stderr, UTC timestamps, native nested recall
metrics and whole public apply clocks remain distinct. Fixture creation,
inspection, review, prepare, initial observation, apply and model time are not
silently added together or substituted for one another. The native nested
metric comes from the actual returned `recall.metrics.elapsed_seconds`.
No-import C must hit its primed catalog; later ordinary recall must still hit
without republishing. All observed public guidance/provenance/safety projections
are paired across builds. Bank bytes/modes must equal the complete reviewed
result; source pack, feature bytes, original index, HEAD, branch and unrelated
work are preserved. Failed/timed-out/refused operations retain raw evidence and
failure state; none becomes successful empty recall or an omitted condition.

The performance receipt separately reports B/A restoration and C/B incremental
optimization. At 50k it gates C/A median complete navigation and primed nested
recall at **at most 0.80**. For conditions 2–4 it gates C/A median nested recall
and whole apply at **at most 1.10**. Other sizes are disclosed without inventing
extra thresholds. A failure/refusal of integrated A is retained and prevents a
pass; the integration owner obtains the approved disposition rather than
substituting a historical workload.

## Memo exact-input keep/revert rule

B4 freezes each independently created World once (seed, actual authority/root,
canonical files, Git/index and private identity state), then restores that same
snapshot for A/C and on/off controls. Frozen private roots/UUIDs are retained
rather than replaced by production impersonation or a fake identity. Corpus,
Git timestamps, round order, imports and conflict labels are deterministic;
paired operations use the exact same original bytes and operation order.

The disposable `sys.setprofile` observer sees the **original executing**
`validation_memo.Memo.checked` frames for `bank-format` and `security-scan`. It
never patches the runtime, makes synthetic validation calls, changes catalog
engine function fingerprints or invents hits. Each call/return retains the
namespace, complete typed key (including digest bytes and maximum/type keying),
actual pre-existing membership/hit or miss, return completion, memo instance,
and exact captured immutable input blobs addressed by SHA-256. Raw events
remain available even when a check fails or is not stored. Instrumentation-off
runs must return identical public outcomes and preserve the same state.
The input blobs capture declared pure data only: `stable` for `bank-format`,
and `content`/`maximum` for `security-scan`. Producer closure cells can also hold
implementation functions (including parsed-companion producers); serializing
those is not input accounting and can raise inside the profile callback.
This selection leaves the complete key and actual membership/completion proof
unchanged; a compatibility smoke is not the full 24-world keep/revert result.

The rule reports `keep` only when all 48 paired applies per build actually
complete, both namespace hit controls are observed, no baseline successful hit
count is lost for any matching operation/namespace/exact key, and all on/off and
A/C public outcomes agree. Missing work, zero observed baseline namespace hits,
failures or lost hits produce `revert-or-pending`, with exact affected keys;
there is no fabricated hit count. This is not #209's full 24-session/10k/two-round
stress acceptance and does not alter or rerun that workload.

## Frozen quality and full safety controls

The literal runner uses the retained literal-label files' actual metadata
prototype, verifies the complete canonical fixture digest and all 24 literal
requests/expected labels, and refuses any drift. It retains the known
`no-shared-terms-paraphrase` miss instead of relabeling it semantic success.
The separate 16 semantic questions, contexts and expected labels are extracted
unchanged from `semantic-controller-v5.py:19–36`; no answer/path hint enters a
model request. Each model sees only the question/context, real public
observations and operation guides. Labels are consulted after finish. Every
case retains selected sources, exact body/metadata/version/provenance, source
hashes, found/missed/extras, precision/recall, uncertainty and safety checks.
Model wall and actual usage/cost are separate from public native and complete
end-to-end wall. The head-versus-staged fixture does not certify working bytes.

The safety stage executes the full existing recall/memo/discovery,
refresh/invalidation/conflict/recovery/security, multi-machine, and final
#244 layout/frontier/publication/retention modules in disposable Worlds, not a
narrowed test selection. It consumes actual verified-hold fixture interfaces
from `tests/layout_recovery_fixtures.py` and
`test_layout_recovery_frontier.py`; missing final integrated modules fail the
stage rather than creating an empty frontier or v1 comparison fallback. Final
integration's catalog-enabled/disabled hold/frontier and forced invalidation
cells remain obligations of those public modules, not inferred from timing.
The matrix receipt is explicitly one platform/interpreter only. Independent
review, the combined native macOS/Linux 3.11/3.15 matrix and release gates are
still required before acceptance; no cross-platform result is fabricated by
these controllers.
