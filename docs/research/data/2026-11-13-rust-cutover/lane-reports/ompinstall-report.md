# OMP installation Rust port

## Files

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-ompinstall`.

- Created `src/omp_installation.rs`: complete baseline function coverage, including configuration, receipt validation, extension generation, preview/diagnostics, observer attestation and scoped verification, and copied/linked installed-manual comparisons.
- Created `tests/t2_omp_install.rs`: 18 filesystem-isolated tests.
- Modified `src/lib.rs`: one alphabetical `pub mod omp_installation;` registration.
- Modified `src/artifact.rs`: **superseded by artifact owner at integration**. Main explicitly authorized a lane-local correct implementation of the frozen `BINARY_ENTRY`, `BINARY_MAX_BYTES`, `runtime_digest`, and `inspect_runtime` APIs for compilation/testing. Take the artifact owner's implementation, not this file, when integrating. No other T0/error/Cargo changes.
- Created this requested report outside the worktree.
- Did not edit Python, TypeScript, templates, repository documentation, or other lanes' files; did not write Git, execute Python, access network/awa/Vox, or start subagents.

Read the complete shared brief, cutover design, baseline module, ADR-0004 and ADR-0029, including harness integration, Python-era adoption, and artifact identity requirements.

## Public API

- Constants `READER_CAPABILITY`, `HISTORICAL_READER_CAPABILITY`, `MAX_RECEIPT` (16 MiB), `MAX_MANUAL` (16 MiB), `MAX_ACTIVATION` (2 MiB).
- `Configuration { omp_agent_dir: Option<&str>, omp_peers_upgraded: bool, omp_layout_quiescence: &[PathBuf] }`; repeated layout attestations are representable.
- `binding(agent_dir, no_guidance, reader_capability) -> Result<Value>`.
- `configuration(args, old, selected, no_guidance) -> Result<Option<Value>>`.
- `validate_receipt(&Value) -> Result<()>`.
- **Frozen extension API:** `extension_wrapper(active_pack: &Path, binary: &Path, artifact_digest: &str, binding: &Value) -> Result<Vec<u8>>`.
- `preview(&Value) -> Value`, `diagnostics(repo, receipt: Option<&Value>) -> Result<Value>`.
- `observer_binding(artifact_digest, agent_dir) -> Result<Value>`.
- `verified_observer_binding(artifact_digest, agent_dir, receipt: Option<&Value>) -> Result<ObserverBinding>`.
- `verify_observer_binding(repo, receipt, proof) -> Result<Value>`.
- `observe_observer_binding(repo, receipt, proof, held_source, callback) -> Result<T>`: keeps verification and caller operation within the marker/barrier scope and propagates the exit recheck.
- `owned_manual(receipt, relative) -> Result<Option<OwnedManual>>`; callers use `"SKILL.md"` for the baseline default. `OwnedManual` exposes `verify`, `revalidate`, and immutable borrowed metadata/text getters. `ManualComparison` contains the exact six baseline comparison fields.
- `ObserverBinding` and `OwnedManual` have private fields, no public mint, no `Clone`, and no serialization implementation. Rust therefore refuses forged/subclassed/serialized authority structurally rather than implementing Python runtime guards.
- Module-local `Error::{MissingActivation, Contract(ContractError)}` and default-parameter `Result<T, E = Error>` preserve a specifically matchable missing-registration refusal without modifying shared errors.

## Frozen T2e -> T2f activation interface

There is exactly one registration point: `register_activation(Option<Arc<dyn OmpActivation>>)`. No per-import registries and no fallback implementation. Native operations needing this interface return `Error::MissingActivation` when it is absent; not-installed diagnostics remain available without registration.

`OmpActivation: Send + Sync` has REQUIRED methods:

1. `locations() -> Result<RuntimeLocations>` (`state`, this executing runtime's `pack`, and owned Rust `binary`). Installation/CLI wiring supplies these trusted locations, not request payloads.
2. `verify_targets(targets: &[&Value], source: &Value) -> Result<()>`: installation's complete ownership checks, borrowing rather than cloning target records.
3. `activation_template(pack) -> Result<String>`: installation's selected OMP activation template renderer.
4. `activation(raw, expected, template) -> Result<Vec<u8>>`: installation's exact activation block comparison/rendering.
5. `diagnostics(repo) -> Result<NativeDiagnostics>`: T2f composite operation acquires read-only turn-source locks, checks source evidence and pending transport, inventories streams, produces wake decisions (unavailable summary on diagnostic-only wake failure), then checks the marker under an existing-only source barrier. It never enrolls/promotes a marker. Typed fields are `pending: Option<Value>`, optional `StreamDiagnostics { instances, pending, wake_decisions }`, and `stored_source_verified`.
6. `observe_marker(repo, held: Option<&dyn HeldSource>, operation: &mut dyn FnMut(&Value) -> Result<()>) -> Result<()>`: acquire/borrow the source barrier, observe the marker, run the operation exactly once, freshly recheck marker/bank binding on exit, including an operation refusal. No source enrollment or writes. T2f owns this scope and its lock ordering; the installation module independently checks the receipt identity inside it.
7. `instruction_uri(relative) -> Option<String>`: canonical OMP projection instruction URI policy, no alias normalization.

Implementer: one trusted CLI composition over T2e `installation` and T2f `omp_boundary`, `omp_projection`, `omp_stream`, `omp_transport`, plus T1 `turn_sources`. `HeldSource::recheck` is implemented by turn_sources' real held-barrier observation. This interface replaces every unported lazy import as one activation capability, not six ad-hoc hooks. Artifact inspection and filesystem trust are called directly, not delegated through this interface.

Runtime attestation calls **`artifact::inspect_runtime(pack, binary)` directly** and compares both F and the complete `artifact_files` inventory. It does not reproduce a second digest definition. The Rust installation receipt must record F in `source.artifact_digest`, skill and extension target `artifact_digest`, and include `bin/am` in `artifact_files` using the owner's shared artifact API.

## Python assumptions / harness-lane retargeting contract

The six saved `omp_binding` fields retain their names and paths: `agent_dir`, `skill_target`, `extension_target`, `activation_target`, `peer_reader_capability`, `rollout_attested`. The `*.ts` extension target remains TypeScript intentionally, and `hooks/omp.ts` remains the portable adapter import; neither is a Python compatibility alias.

1. `hooks/omp.ts:9` (`Binding`) currently has only `artifact`, `agentDir`, `rolloutAttested`, `readerCapability`. Add REQUIRED `executable: string`. The generated Rust wrapper now emits this absolute owned Rust binary path alongside the existing options. `artifact` must be F (pack inventory plus binary), never the old portable-only digest. The capability stays `omp-stored-source-v2`.
2. `hooks/omp.ts:37` derives `pack` from the adapter module path. This can still locate portable adapter assets, but MUST NOT imply an executable/module-installation identity; the owned executable comes from the receipt-generated `binding.executable`.
3. `hooks/omp.ts:38` sets helper to `pack/scripts/omp_boundary.py`. Remove it; the Rust route is `binding.executable` with the first argument `omp-boundary`.
4. `hooks/omp.ts:39` sets launcher to `pack/hooks/python3.sh`. Remove it; no launcher/interpreter probe is needed.
5. `hooks/omp.ts:269-271` spawns `/bin/sh` with `[launcher, helper, '--stream', ...]`. Replace with `spawn(binding.executable, ['omp-boundary', '--stream', '--repo', repo, '--session', session, '--instance', instance, '--lifecycle', lifecycle, '--artifact', binding.artifact, '--omp-agent-dir', binding.agentDir], ...)`, preserving the finite stream's framing, queue/drain/lifecycle behavior. Do not turn this into a shell command string. The existing warning at line 232 already names `am omp-boundary --list-pending` and remains valid.
6. `hooks/omp.ts:30,303-304,491-505` calls clock fields `p1`/`p2` and calibrates them against JS hrtime. These wire field names need not change, but old Python process/clock qualification is NOT Rust qualification. Establish the supported exact Rust clock relation or explicitly refuse wake support; ordinary source processing must not weaken.
7. Baseline `omp_installation.observer_binding` compares the source pack with Python `__file__.resolve().parents[2]`. The Rust module instead compares the receipt pack to the trusted executing `RuntimeLocations.pack` and verifies `RuntimeLocations.binary` in F. No script location or interpreter identity is authority.
8. Baseline observer executable closure individually names `hooks/python3.sh`, `scripts/omp_boundary.py`, and `scripts/agent_memory/{omp_boundary,omp_transport,omp_projection,omp_control,omp_installation,omp_stream,installation,artifact,trust_paths,frontmatter,pack_validation,migration_contract,turn_sources,security}.py`, plus `hooks/omp.ts`. All Python entries disappear. The shared runtime inventory verifies the entire remaining portable pack, including `hooks/omp.ts`, PLUS the installed binary entry `bin/am`; it is not a renamed script allowlist.
9. Python-era receipt `source.artifact_digest`, target digests, and `artifact_files` are portable-only identities. Reissue them using the shared runtime inspection; do not reinterpret an old digest as F. Generated wrapper `artifact` is F, and its new `executable` is the owned runtime binary, not `source/scripts/installer.py`, `bin_dir/am` if that is merely a launcher, or a Python source-tree path.
10. The adoption design's existing Claude/Codex/OpenCode hook commands `/bin/sh …/python3.sh …/*.py` are owned legacy command strings, not Rust routes. The installation/client-hooks owner must recognize exact old entries and replace only owned entries with direct Rust executable/subcommand invocations. This lane does not edit those configurations.
11. The binding's `rollout_attested: true` / adapter's `rolloutAttested: true` is emitted ONLY after explicit renewed `--omp-peers-upgraded`; it is never copied as authorization from an old Python or Rust receipt. A running old TypeScript module keeps its old target until harness restart; installation/adoption must retain the design's quiescence gates and restart notice. No native loading or provider/platform acceptance is inferred from disk ownership.

## Argparse / presentation

Baseline module has no `main`, argparse parser, stdout/stderr, or exit-code wrapper; it is a library. Its caller's flags affecting these functions are:

- install `--presets`: optional comma-separated string; help lists `all, none, agents,codex,claude,opencode,omp`, with OMP excluded from `all`;
- install `--omp-agent-dir`: optional string, default `None`;
- install/update `--omp-peers-upgraded`: `store_true`, default `False`;
- install/update `--omp-layout-quiescence`: `append`, default `None`, repeatable;
- install `--no-guidance`: `store_true`, default `False`;
- install `--mode`: choices `link`/`copy` (ownership validation accepts both).

These declarations belong to the CLI/installation lane. No argparse interpreter/help-layout behavior was introduced here.

## Tests / obligation accounting

Fully ported (exact obligation comment above the Rust test):

- `tests/contracts/test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_legacy_rollout_boolean_cannot_authorize_new_capability` -> `legacy_rollout_boolean_cannot_authorize_new_capability`.

Additional direct module tests cover canonical exact binding shape/order, existing/dangling/ancestor symlinks, no path creation, no-guidance binding, unknown capability, all saved/explicit selection refusals, root and renewed attestation requirements, repeatable quiescence input, all receipt schemas/fields/target roles/artifact matches, immutable old configuration, exact generated TypeScript bytes (including escaping and Rust executable), typed missing registration, honest preview/not-installed diagnostics, and an independently hashed F inventory vector plus swapped-binary/mode controls. The independent golden was calculated from the baseline sorted compact ASCII inventory string using `printf ... | shasum -a 256`, not from the Rust implementation.

Partially ported: `test_installer.py::InstallerContract.test_omp_binding_installs_candidate_activation_in_copy_and_link_modes` has binding/receipt kind coverage in `valid_receipts_keep_native_and_nonnative_schema_distinction` and wrapper bytes in `wrapper_bytes_bind_artifact_root_and_current_reader_without_python`; actual installation, candidate activation, public command invocation, and copy/link mutation await `installation`.

Deferred, not mocked/faked:

- `test_installer.py::InstallerContract.{test_omp_missing_or_malformed_candidate_activation_refuses_without_fallback,test_omp_historical_owned_shared_block_updates_and_rolls_back_by_recorded_hash,test_omp_historical_shared_activation_uninstall_preserves_unrelated_text,test_omp_modified_or_foreign_activation_is_preserved}`: `installation`/uninstallation and real activation ownership.
- `test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_real_binding_proof_rejects_forgery_and_mid_instance_drift_before_source`: real `installation`, `omp_boundary`, `omp_transport`, `omp_stream`, `turn_sources` implementation/CLI wiring. The Rust token types already structurally prevent mint/clone/serialization forgery; runtime receipt/marker/bank-binding drift qualification still needs those real modules.
- `test_omp_wake_decisions.py::AnswerFirstQualification.test_no_guidance_is_explicitly_unqualified_without_unhealthy_disk_status`: installation and T2f diagnostics/stream integration. Direct no-guidance binding and honest absent diagnostics are tested here, not represented as a complete obligation port.
- `test_omp_boundary.py::OwnedInstructionBoundary` methods `test_exact_carrier_and_unforgeable_witness`, `test_malformed_carriers_and_invocations_decline`, `test_witness_declines_changed_receipt_file_and_permissions`, `test_link_witness_declines_retargeted_owned_link`, `test_historical_annotation_is_closed_and_installation_independent`, `test_fresh_capture_preserves_surrounding_source_and_retries`, `test_fresh_owned_guides_capture_exact_metadata_in_copy_and_link_modes`, `test_owned_guide_alias_partial_tamper_and_foreign_reads_keep_real_gaps`, `test_noninstruction_artifact_read_retains_ordinary_body_not_owned_metadata`, `test_historical_owned_guide_never_authorizes_read_after_installation_changes`, `test_owned_guide_rechecks_receipt_selection_and_nested_symlinks`, `test_unrelated_ancestor_churn_preserves_witness_and_public_annotation`, `test_verification_races_refuse_selected_binding_payload_and_policy_changes`, `test_nonread_result_never_mints_manual_witness`, `test_screened_invocation_does_not_hide_following_answer`, `test_snapshot_publication_recovery_survives_uninstall_and_retry`, `test_mismatched_orphan_snapshot_is_integrity_failure`, `test_preexisting_native_gap_is_not_reclassified`, `test_sealed_tool_gap_does_not_authorize_late_classification`: real installation plus projection/transport/boundary/source modules. The complete comparison and pinned-descriptor code is ported, but these end-to-end authority tests cannot honestly run before those implementations exist.

This lane does not claim native stock-harness acceptance, provider acceptance, transactional source completion, wake qualification, or completion of #249.

## Declared deltas

- Required design delta: runtime artifact digest F replaces portable-only digest and script closure identity; `bin/am` is mode `0o755`, capped by the artifact owner's 64 MiB limit. The generated wrapper adds the required explicit `executable` option.
- Required adoption delta: every configuration reissue requires explicit `--omp-peers-upgraded`, even an already-current reader capability; no old `rollout_attested` inheritance.
- Rust native values require UTF-8 strings. Malformed JSON shapes/types receive deterministic contract errors or decline manual classification rather than interpreter exceptions. Strict receipt decoding rejects duplicate keys and BOM/UTF-16/32 text using T0; the codec's already-declared surrogate/depth policies apply.
- Former unbounded receipt/activation/manual reads have explicit named 16 MiB / 2 MiB / 16 MiB limits respectively. Manual's limit matches baseline OMP `MAX_SOURCE`; receipt/activation limits prevent unbounded ownership reads.
- Observer/manual immutability, internal minting, and nonserialization are enforced by Rust visibility/ownership, not Python object token checks or pickle errors.
- Binding path spelling is deterministically normalized using native path components before canonical comparison; no interpreter-dependent path representation is retained.
- Missing OMP registration is a typed refusal even for optional manual evidence; it is not silently represented as an authenticated or unavailable-but-successful native runtime.

## Verification performed

All commands ran in the named lane worktree, offline where Cargo exposes that option:

1. `cargo check --offline --all-targets` — passed; no warnings in lane-owned code, seven pre-existing unused-import warnings in `tests/t1_a2.rs`.
2. `cargo fmt --check` — initially reported formatting differences only in the new module/test and the authorized artifact additions.
3. `cargo fmt && cargo test --offline --test t2_omp_install` — passed, 18/18 (before final interface refinements).
4. Final exact finish-line command: `cargo fmt && cargo fmt --check && cargo check --offline --all-targets && cargo test --offline --test t2_omp_install && cargo test --offline --test t2_omp_install && cargo test --offline --test t2_omp_install` — passed: formatting clean, all targets checked, no warnings in owned files, 18/18 three consecutive times. The same seven pre-existing `tests/t1_a2.rs` warnings remain.

No `t1_a2` tests or broad unrelated suites were run. Main must integrate the artifact owner's implementation, implement/register the real activation composite, rerun this target, and perform Linux/native end-to-end qualification. Tracker/Vox reconciliation intentionally not run, as prohibited by the shared lane brief.

## Added-rule handover check

Re-read the shared brief's Added rules after Main's message. Removed the remaining argument `unwrap()` and directory-name indexing; the module has no `unwrap()`, `expect()`, indexing/slicing, panic macro, or child-process launch. No child deadline is needed in this module because it starts no children.

Required exact command `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/omp_installation.rs` produced **no matches, exit 1**. The broader source search also found no process-launch or panic macros; its two original hits were removed rather than justified. The lane-local authorized artifact additions likewise use fallible field/object access instead of `unwrap`.

After those changes, reran `cargo fmt && cargo fmt --check && cargo check --offline --all-targets && cargo test --offline --test t2_omp_install && cargo test --offline --test t2_omp_install && cargo test --offline --test t2_omp_install`: all passed again, **18/18 three consecutive times**, no warnings in lane-owned code; the seven pre-existing `t1_a2` unused-import warnings remain.

Baseline route name assumed for direct Rust invocation: **`omp-boundary`**, matching the existing public guidance `am omp-boundary --list-pending` and the old `scripts/omp_boundary.py` helper route. Main accepted the new `executable` option; the CLI lane is authoritative for the final dispatch spelling.

Implementation completeness versus deferred verification: no baseline `omp_installation` function body/branch is omitted. Python token constructors/setattr/serialization guards become Rust-private, non-Clone, nonserialized token types. The six unavailable-module imports are represented by the REQUIRED activation trait methods enumerated above, with typed refusal until real registration; their implementations belong to the named installation/harness/source owners. The deferred/partially ported entries above refer to end-to-end **tests**, not an undisclosed unported branch in this module.
