# #255 G1 actual runtime controls

Research-only supplement to the parent's `qualify.py`, not an AM feature, public
CLI implementation, native eligibility adapter, legal approval, or performance
pass. This directory is the entire ownership boundary of this subdeliverable.
No production code, parent scale recipe, execution contract, or tracker was
changed. The parent owns integration, execution-slot assignment, independent
review, release evidence, and final contract freezing.

## Evidence status

**The new runtime recipes have not been executed.** They were authored while the
host was reserved for #209 and its subsequent correctness diagnostics. No model
service, graph service, model request, database request, or performance workload
was started by this subtask. A source/math-only smoke receipt, if present, is not
runtime qualification. `execution-manifest.json` freezes the concrete commands,
bounds, outcomes, evidence classes, and explicit platform prerequisites before
execution; it does not mark them passing in advance.

`prior-observations.json` retains exact SHA-256 bindings and compact extracts of
already-existing, read-only parent artifacts:

- SSG's real reference build/rebuild loaded 974 units, 11 queries and 62,251
  advisories, yielding 84,277 nodes and 103,339 edges. Embedding coverage was
  **0/63,225**, not complete.
- Real reference and alternate endpoint responses had dimensions 4096 and 1024.
- Actual 9341 accepted `not-a-real-model` and echoed that name. This is positive
  evidence that response `model` alone is not loaded-weight identity.
- Actual over-context reference/alternate requests returned HTTP 400, reporting
  9002/8192 and 4502/4096 prompt/context tokens respectively. These are configured
  serving limits, not the models' advertised training windows or a proof against
  every possible silent truncation.

Inherited files have no stronger provenance than the retained parent artifacts;
new graph/query/fault/API cells need their own actual run. Concurrent earlier
scale latencies do not become clean measurements through this supplement.

## Runnable entry points

Use the exact `macos_arm64_serial_command` argv/cwd in
`execution-manifest.json` **only after Main grants the exclusive host slot**.
The flag `--reserved-host-slot` is an explicit safety interlock, not a substitute
for the scheduler's approval. The supplied `host-config.json` names existing
artifacts on this research host. No build, dependency install, download, image
pull, global configuration edit, or real project export is performed.

`managed_endpoints.py` serially starts and reaps these actual servers:

1. Source-built llama.cpp **9341**, exact Qwen3-Embedding-8B Q8_0 weights, Metal
   on the supplied macOS profile, a single discovered model, configured context
   8192. The reference remains up only while the optional real graph controller
   executes.
2. Installed llama.cpp **9360**, exact Qwen3-Embedding-0.6B Q8_0 weights,
   dimension 1024, configured context 4096. This is another real endpoint,
   not an inherited 8B quality pass.
3. Actual llama.cpp router with both exact weight files catalogued and automatic
   loading disabled. Discovery must refuse its multiple real model IDs **before
   any embedding request**. No mock `/models` response supplies this proof.
4. Actual non-embedding, pooling-none server. The embedding route must refuse.
   This exercises a real capability mismatch, not a claim to have downloaded or
   evaluated a separate chat-weight model. `endpoint_controls.py --kind
   nonembedding` also accepts an operator-supplied real chat-only endpoint config
   for a separate actual cell; absent such an endpoint remains a prerequisite.
5. Actual alternate embedding server with a randomly generated private key file.
   Authorized embeddings must succeed and unauthorized embeddings must receive
   401/403. Keys never enter command argv as values or the returned evidence.

The launcher verifies exact weight sizes/digests, reference source revision,
expected server build numbers and records executable hashes/version output.
It does not conflate that owned local artifact evidence with BYO attestation.
All ports are loopback, ephemeral, and new. It never adopts a running service.
No compatible service is silently started by `endpoint_controls.py` itself:
that controller only sends synthetic requests to its supplied local API base.

The run root must be new. Repeating an invocation with the same root refuses
without modifying its historical evidence. Use a new root for an independent
repeat. Cleanup is idempotent and exact-resource-bound.

### Independent endpoint usage

`endpoint_controls.py --config FILE --output NEW_ROOT --ssg /opt/ssg` operates on
an existing real loopback endpoint without managing it. `FILE` contains `url`
and `dimension`, optional `model` and private `key_file`, and only for a known
llama profile explicit `context_tokens` and `tokenizer_url`. A root URL normalizes
to `/v1`; an explicit non-root API prefix is preserved. Redirects are refused.
It does not guess alternate vendor/tokenizer paths or forward keys elsewhere.
An explicit configured model is checked against independent available metadata;
multiple/no metadata IDs are not guessed. No metadata plus an explicit model
remains opaque, never verified identity. Raw response echoes are only consistency
checks after independent selection.

`--kind ambiguous` qualifies actual multiple-model metadata refusal; `--kind
nonembedding` qualifies actual embedding-capability refusal. These negative
cells report a semantic public outcome with exit 1 inside their receipt, while
the successful research controller exits 0 only when the expected refusal is
observed. They are not AM production commands or its exit-code implementation.

The embedding cell retains actual small 1/4/8-input batch responses, index/count/
dimension/finite/nonzero checks, unknown-alias behavior, explicit over-context
request/refusal, tokenizer counts, token-usage difference, repeated probes and
own-text/neighbor controls. Unsupported/opaque tokenizer evidence stays
**unverified**. It never clips a source to force a request to succeed. It does
not identify a provider's maximum batch/token budget from three small batches.

Malformed batches are deliberately **protocol simulation**: mutate copies of a
real response to duplicate/remove/add indices, use a boolean index, change the
model/dimension or inject NaN. Those receipts cannot be cited as actual provider
misbehavior or compatibility proof.

Four fixed probes at three immediate repeats measure extra tokens, elapsed time,
maximum normalized coordinate deviation and cosine distance. Both diagnostic
limits are `1e-5`, fixed before this run. The report compares the probe cost with
a six-document synthetic fill, not a representative maintenance rate or clean
latency pass. Immediate equality cannot attest weights, invisible alias changes,
cross-restart/back-end stability or universal vector compatibility. Cross-time
reuse of an opaque alias still relies on the provider keeping it stable.

Own-text versus neighbor-text uses the same six authored synthetic documents and
four authored labels for both renderers, preserving exact rendered hashes and
real embeddings/rankings. It is an isolated mechanism control derived from
SSG's `unit_text`/`unit_text_related` design, not a byte-equivalent SSG renderer,
independent full AM quality suite, or coding-benefit study.

## Actual graph, process faults, rebuild and erasure

`graph_controls.py run` requires the selected `podman` or `docker` engine,
existing pinned MAGE image, actual SSG binary/source, an already-running actual
reference embedding endpoint config, and a new output root. The serial launcher
supplies these with `--with-graph`. The graph controller does not manage the
embedding server and never connects to the parent's `17690` graph.

Every generation gets its **own container plus own data and log volumes**.
There is no Enterprise database assumption, global cross-project ANN index, or
label-only isolation. At most two graph containers are live. Each is bounded to
2 CPUs/2 GiB, with loopback Bolt and unique receipt-bound identity. Candidate
index searches validate their entire returned project/generation domain; a
separate foreign physical instance contains an explicit foreign canary.
Wrong-project reads refuse; mismatched checkout bank-state reads are stale.
This is scratch routing/binding, not the native reader's production authority.

The actual SSG binary builds a tiny authored synthetic class with **all eleven
real packaged query templates**, runs its named `class` query and checks source
provenance. Changed source must exit 11; changed template must exit 3; replacing
a known template with an actual write plan must be refused at `EXPLAIN` with
exit 3. A forced rebuild restores successful named query. A guarded actual
`workspace up` invocation with equal addresses must refuse before invoking
Docker; its deliberately empty executable PATH prevents accidental startup if
that guard regresses. No SSG/user container or volume is adopted or removed.
Template checks are not database ACLs: this receipt does **not** prove reader
credentials lack writes. Transient workspace loading/parser quality and the
advisory evidence critic are source-assessed, not falsely runtime-passed.

A real builder child process performs actual graph transaction fill, actual API
embedding fill and persisted vector batches, actual vector-index DDL, an actual
inferred-similarity write, and immutable manifest/pointer publication. Durable
file handshakes stop it at each named boundary. The parent sends real SIGKILL:

| Boundary | Required observable result |
| --- | --- |
| `graph-fill` | Kill with a graph transaction open; rollback leaves candidate empty, active unchanged |
| `vector-fill` | Kill after first committed vector batch; partial candidate never routes active |
| `index-creation` | Kill immediately after real candidate-local DDL, before ready/publication |
| `similarity` | Kill after a real inferred edge, before ready/publication |
| `pointer-before` | Complete candidate file is durable, active pointer still old |
| `pointer-after` | Atomic replacement and directory fsync have happened; newly routed candidate is complete |

These are actual builder-process crashes at explicit operation boundaries,
**not a claim to interrupt inside Memgraph's DDL engine or between arbitrary
filesystem instructions**. A separate cell SIGKILLs the real inactive partially
filled database process, restarts that exact owned container, observes recovery,
and proves its crash did not change the qualified active graph. This is not
power-loss/hardware durability or active-database recovery certification.

Publication uses a real OS file lock, monotonic authority/epoch receipt, expected
active-pointer compare, fsynced immutable manifest, `os.replace` and parent
filesystem fsync. A reader pins one manifest/instance and rechecks its authority.
Interrupted candidates cannot publish merely by age. An actual successful rebuild
uses another physical candidate and performs the same publication path.

Deletion changes both synthetic bank-state and privacy epoch while an actual
builder is paused before publication. It must resume with stale/refused status,
not publish; the former reader is stale. A permitted rebuild excludes the
synthetic deleted canary and must query zero copies of its body. A separate
**consent-only** epoch change with unchanged canonical bank-state repeats the
stale-builder/reader fence, disposes its stores, removes active routing and ends
unavailable without further embedding disclosure. These controls do not claim
production canonical delete/consent APIs exist.

Before disposing affected stores, the recipe creates an actual snapshot,
stops the instance and inventories actual snapshot/WAL/log file paths, sizes and
hashes through a read-only, network-disabled helper using the same pinned image.
Snapshot and WAL existence are required for the deletion erasure cell. Whole
owned data/log volumes are removed and their absence checked against both engine
inspection and listing. Old local vector batch files and builder logs are also
hashed then unlinked; the controlled model server is stopped and its owned logs
hashed/unlinked by the launcher. No `DETACH DELETE` is represented as snapshot/
WAL compaction. This is logical disposal of enumerated owned resources, **not
forensic secure media erasure, external-backup deletion or provider retention
control**. Any failed/unknown identity remains erasure-pending.

### Cleanup and recovery

The normal run's `finally` reaps owned builders, removes only receipt-bound
containers/volumes and inventories outcomes. Names alone never grant authority:
full run label, exact container ID and volume creation identity are checked.
Unknown/recycled resources are left untouched with actionable resource IDs.
The controller snapshots unrelated container IDs before/after; it never invokes
prune, broad `rm`, unqualified `pkill`, user-directory cleanup or SSG down/up.

For an interrupted graph controller, run the same Python interpreter and exact
integrated `graph_controls.py` path with:

```text
cleanup --output /tmp/am-v15-delivery/runtime-controls-actual-20261105/graph-controls --engine podman
```

For managed-process recovery, run the integrated `managed_endpoints.py` with:

```text
--cleanup --output /tmp/am-v15-delivery/runtime-controls-actual-20261105 --reserved-host-slot
```

Managed cleanup compares run ID, PID birth/command snapshot and executable hash
before signaling; PID reuse or unknown identity refuses rather than killing a
user process. It removes only inventoried owned private credentials/logs. Run
graph cleanup separately with the original engine when needed. An uncatchable
controller SIGKILL between OS process creation and its durable process receipt
requires operator inspection; no fabricated complete-cleanup claim is made.
Private config names credential **paths**, never values. Research evidence and
synthetic responses remain in the private output root, not a real bank.

## Required parent interface/contract changes

No parent file was edited. Integrate these explicit deltas rather than interpreting
its old dictionary-only check as crash evidence:

1. `qualify.py`'s `interrupted_candidate_did_not_publish` means only its synthetic
   file/dictionary control. Actual process/graph crash evidence must cite this
   controller's per-boundary artifacts, exact ownership IDs and observed exits.
2. Parent scale vectors must not trust `body.model == requested_model` as loaded
   weight identity. Select from independent metadata first; owned launch/artifact
   pins can strengthen local identity, while opaque providers retain the literal
   status **provider model identity not verifiable**. Do not silently inherit
   managed artifact evidence for an arbitrary URL.
3. Scale `--graph` currently names one fixed whole-replaced scratch graph. Keep
   that measurement explicitly distinct from physically isolated complete
   generation publication. Do not reuse the parent's idle graph or delete user
   data to run these controls.
4. Bind final contract tolerances to actual per-profile repeat/probe and token
   observations. `1e-5` is this predeclared diagnostic experiment limit, not an
   automatically frozen production/backend tolerance; probe overhead on six
   texts is not 1k/10k/50k maintenance acceptance.
5. Preserve independent status axes: metadata/weight assurance, token-processing
   assurance, source/epoch currency, graph/vector completeness, erasure authority,
   and semantic result. Missing platform/legal/endpoint evidence is a prerequisite,
   not success, empty guidance, or permission to start an alternative service.
6. The production generation publisher still needs canonical/private frontier,
   policy/profile and native final-source qualification integrations. These
   synthetic epoch files and reader outcomes demonstrate an OS/database protocol,
   not those production API guarantees.

## Platform and remaining execution prerequisites

The runnable supplied managed profile is macOS arm64 native Metal plus dedicated
Linux-arm64 MAGE under the existing engine. Record only cells actually exercised.
Native Linux arm64 requires the same pinned source/build versions, CPU-native
executables, exact weights, existing local image and a private path-adjusted
`backend: cpu` configuration on that host. macOS evidence cannot qualify native
Linux or Linux amd64. No containerized source build is substituted for absent
native-platform execution.

The parent must execute the retained recipe in its reserved slot, retain final
source/artifact hashes and receipts, inspect expected outcomes, and separately
finish its scale, independent-suite, operator/resource/legal and technical-review
gates. No 1k/10k/50k measurement, native Linux result, standalone chat-weight
endpoint, full transient workspace/parser/critic execution, or production .16
API guarantee is claimed by this subdeliverable.
