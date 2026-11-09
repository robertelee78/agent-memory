# #255 reviewed storage-access contrast — actual mechanism evidence

## Import provenance and current authority

Imported from exact peer source `f148da42e0944adf323556685981b1545e0c8a9e`.
The observations, controller identity and receipt bindings below belong to that
peer diagnostic, not an execution against Main's final source. This import did
not run the controller or any study. The retained controller and result JSON are
historical evidence; their hashes are not newly computed Main verification.

The [current graph/embedding execution contract](../design/v0.0.16-graph-embedding-execution.md)
is a stronger proposed merged amendment pending independent review. Its §§8.1/13
require exact-only v1 scoring outside the graph engine while retaining typed graph
projection/querying and the historical exact-versus-ANN evidence. Graph DB and
embeddings remain useful selected core capabilities; this diagnostic neither
removes them nor selects a replacement. The accepted original G1 research boundary
at peer `362ab0aac21399381f3379550726ca750c614b1d` and its
[historical freeze](data/g1-contract-freeze-20261109.json) remain accepted. The
`G1_acceptance=false` below describes this diagnostic's scope, not a reversal of
that later research acceptance. No full runtime pass or signed delivery follows.
Current private-build licensing and resource dispositions in contract §§2/13.2
remain controlling; historical legal/resource limitations below create no new
approval, license, launch or resource gate.

The independently reviewed minimal controller (SHA256 `a32469f7f992dc51290f764cf8a16dd571c408382398ca4c9fcef32b35e2ddbb`) was executed unchanged after provisioning its exact pinned image. [Retained source](data/2026-11-05-ssg-runtime/independent-execution/storage-access-contrast-controller-20261108.py) and [sanitized results/hash bindings](data/2026-11-05-ssg-runtime/independent-execution/storage-access-contrast-20261108.json) make the diagnostic reproducible.

## Observation

Two separate owned graph instances received the same eleven synthetic4096-dimensional vectors and unchanged Memgraph flags. The idle CREATE VECTOR INDEX succeeded. The second CREATE, issued while an explicit read transaction remained open, returned the reviewed storage-access TransientError. Each CREATE was attempted once. Both arms settled and disposed their exact-owned container and two volumes; all six resources were read back absent. The three pre-existing unrelated containers and external volume inventory were unchanged.

The outcome is `deliberate-transaction-contention-mechanism-observed-not-historical-cause`. No model or embedding endpoint was called, no historical experiment changed, and no WAL/snapshot/access-timeout/image setting was tuned. Cleanup is ordinary verified ownership disposal, not a forensic erasure claim.

## Source and runtime

The exact Docker index is `sha256:e8e03640071c57de6a13f2f4c5f9ed1789797dc5e8e771bbe62203b20e1b8c27`, arm64 child `sha256:fe2f37e45e263225129bcb6955612ac9956dea0e8db614f9c39eb74b105c6c39`, configuration `44b5244a3622425584cae52b0cdf1ea1690ebb98e27110b3adfd1af7ab8227a9`. All three were read back after pull. The existing running VM reports32GiB/12CPUs; it was not restarted or reconfigured. The frozen controller additionally checked available memory and each instance's actual cgroups/user/flags.

The historical qualification venv was absent: the first launch exited127 before the controller started. A separate private Python3.15 environment with neo4j5.28.2 and pytz2026.6 was provisioned. Both actual new arms use that same recorded runtime. This is not the old Python3.13.13 environment, a matched historical replay or a whole-workload performance comparison. Full interpreter/driver-source identities and before/after ownership receipts stay private under `255-storage-contrast-release-20261108-02`.

## Reproduction boundary

Use a supported macOSarm64 host, the exact Linuxarm64 image already provisioned in the selected Podman connection, neo4j5.28.2, a new private output, and an explicitly reserved short diagnostic window. The controller never pulls or changes machine settings. Invoke the retained source with `-I -B`, `--source-sha256 a32469f7f992dc51290f764cf8a16dd571c408382398ca4c9fcef32b35e2ddbb`, `--output ABSOLUTE_NEW_PRIVATE_DIRECTORY`, `--engine ABSOLUTE_PODMAN_EXECUTABLE`, and `--reserved-host-slot`. Preserve failures; do not retry CREATE/DROP or change resource/profile settings to obtain a desired outcome. These are historical reproduction requirements, not authorization to launch a new diagnostic.

## What remains unproved

This establishes a possible read-transaction/unique-storage-access conflict in the pinned binary. It does not identify the historical50k lock holder, prove a snapshot caused that failure, qualify ANN recall, repair semantic-quality failures, establish legal deployment authority, freeze the G1 contract, or satisfy full #255/release acceptance. `G1_acceptance` remains false for this diagnostic. Before any separately authorized new50k experiment, retain the separately reviewed stop-on-unknown CREATE/DROP policy, outside-Bolt snapshot telemetry, exact source/input/profile freeze and independent quality design. The original failed study and its opened holdout remain unchanged.
