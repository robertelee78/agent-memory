# Corebank lane report

## Files

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-corebank`.

Created:
- `src/bank_contract.rs`
- `src/bank_layout.rs`
- `src/bank_writes.rs`
- `tests/t1_bank.rs`

Modified:
- `src/lib.rs`: three alphabetically placed module declarations only.
- `src/fs/descriptor.rs`: the coordinator-announced `entries_bounded` primitive (below); no other T0/API/crate change.

This report is the requested external documentation artifact. No Python execution, network, Git writes, AWA/Vox operations, or subagent spawning occurred. The two required API handover messages were delivered to Main immediately after the corresponding code compiled and `t1_bank` passed.

## Public API

All three modules use the existing order-preserving `codec::json::Value`, `error::Result`, and `lifecycle::CommandResult`. Schema data is `include_str!` from the existing schema files; record fixture data is included from the existing template. The domain layer never parses argparse or launches a Python script.

### bank_contract

- Schema-backed `SPEC`, `CONTEXTS`, `FIELDS`; named router, route-count, physical-entry and traversal-depth bounds.
- `timestamp`, `header`, `path_value`, `text_cell`, `route_value`.
- `route_table(&[Value], source)` and `parse_table(text, source)` preserve relative-link framing and bank-relative labels.
- `router_text(metadata, routes, omitted)` and `parse_router(text) -> Result<(Value, Vec<Value>, Value)>`.
- `context_route(path)`, `add_context_routes(text, &[String], updated)`, `render(proposal)`.
- `portable_key`, `portable_aliases`, `runtime_path`, `bank_root`, `physical_paths`, `file_topology`.
- `ParsedRecords = BTreeMap<String, (String, Value, String)>`: kind, metadata, body.
- `validate_files(&Value, Option<&mut ParsedRecords>)`, `validate_files_uncached`, `validate(repo, owned_lock, owned_transaction)`.
- The validation memo retains the report and parsed-record companion together and reparses the router on cache hits, retaining fresh remote normalization.
- `Observation { checked: Value, hashes: Value, sources: BTreeMap<String, Vec<u8>>, recovery_frontier: Value }`.
- Shared observer accessors `observe` and `scan_hashes` (also used by bank_writes).
- `command_render`, `command_validate`, `recover_layout`.

### bank_layout

- Schema-backed `SPEC`, `MAX_INDEX_BYTES`, `MAX_NESTED_INDEXES`.
- `index_header`, `index_text`, `parse_index`, `constraints`, `inherited`.
- `inventory(files, root_metadata, root_routes, Option<&mut ParsedRecords>) -> Result<Vec<Value>>`.
- Inventory checks complete reachability, route uniqueness/cycles/depth, inherited component/branch conditions, project and lifecycle agreement, admitted record kinds, procedure nonbindingness, active/version uniqueness, safe history and tombstone routing, and deleted-ID resurrection.
- `model`, `render_index`, `validate_index(path, raw)`, `inspect(repo)`, `command_result`.
- `inspect` directly calls bank_writes::snapshot now that both modules exist.

### bank_writes

- `OPERATIONS`, `PROPOSAL_FIELDS`, named receipt/fault-manifest/pause bounds.
- `bank_path`, `runtime`, `hashes`, `snapshot(repo, owned_lock)`.
- `StateEnvironment { state_home, xdg_state_home, home }`, `StateEnvironment::current`, `state_root`, `state_root_with`, `state_paths`, `state_paths_with`. Explicit injection avoids mutating the process environment in tests.
- `result(operation, fields)`, `validate_replacement`, `preview`, `preview_with_sources`, `proposal_value`, `fault`, `record`.
- `apply`, `apply_with_identity`, `apply_with_identity_and_environment`.
- `recover`, `inspect`, `command_result`.
- The replacement implementation contains the full barrier/exclusive-flock/identity/inventory/receipt-replay sequence, same-parent ordinary-file temporary with mode preservation, file fsync, complete read-set recheck, native replacement, directory fsync, readback and final pre-receipt recheck. Unexpected receipts are never overwritten. Already-present and already-receipted retries remain distinct, with historical receipts preserved if the bank later changes.
- Hash-map equality intentionally ignores JSON object insertion order, as Python dict equality does; durable request and receipt encodings use the shared developer_identity::encoded codec.

### Shared T0 primitive

`Descriptor::entries_bounded(&self, limit: usize) -> io::Result<Option<Vec<OsString>>>`: returns None upon observing an entry beyond the bound, without returning a partial inventory or allocating the extra entry. Legacy entries delegates through a private common iterator. All corebank directory enumerations use the bounded method. The test target covers exact, over-limit and zero limits. Main was notified immediately. The leaves lane owns the canonical shared implementation; keep its implementation when integrating if it differs internally.

## Traits requiring production wiring

All registrations take `Option<Arc<dyn Trait>>`; absent providers return a typed `ContractError`, never a successful fallback. Registry locks are released before invoking provider code.

| Owning module / trait | Methods | Future implementer |
| --- | --- | --- |
| bank_contract::BankObservation | observe(repo, owner, transaction) -> Observation; scan_hashes(bank) -> Value | bank_observation |
| bank_contract::RecoveryCoverage | public_coverage(frontier) -> Value | recovery_frontier |
| bank_contract::RouteTable | execute(route, arguments) -> CommandResult | CLI lane; route is recover-layout |
| bank_writes::ScopeBinding | context_value(context); require(repo, project_id, expected, owned_lock) -> Value | scope_binding |
| bank_writes::Checkpoint | validate_write(proposal, sources) | checkpoint |
| bank_writes::RecoveryFrontier | capture_scope(repo) -> Value; observe(repo) -> Value | recovery_frontier |
| bank_writes::RecoveryTransactions | apply_checkpoint(proposal) -> Value | recovery_transactions |
| bank_writes::Security | guarded_start(...); audit(...) -> Value | security |
| bank_writes::TurnSources | canonical_writer_barrier(repo) -> Box<dyn CanonicalWriterBarrier> | turn_sources |
| bank_writes::CanonicalWriterBarrier | finish(); implementation releases resources on Drop, including refusals | turn_sources guard |

Registration functions are `register_bank_observation`, `register_recovery_coverage`, `register_route_table` in bank_contract, and `register_scope_binding`, `register_checkpoint`, `register_recovery_frontier`, `register_recovery_transactions`, `register_security`, `register_turn_sources` in bank_writes.

The existing records::TombstoneParser registration must also be supplied by the tombstones module before tombstone inventory succeeds; it was not replaced or faked here.

## CLI declarations and output contract

Common: subcommand required; `main(argv)` argparse handling is not ported. Domain command-result adapters carry the JSON value, exit status, exact encoded stdout including trailing LF, and empty stderr. Success JSON is indent=2, ensure_ascii=False. Failure JSON is default compact-layout-with-spaces/ASCII encoding. Capacity details use bank_capacity::details.

bank_contract:
- recover-layout: forwarded route with all following arguments; no script execution.
- render: `--input` REQUIRED, JSON file or `-` for stdin; duplicate keys rejected by CLI input decoding.
- validate: `--repo`, default `.`.
- Success exit 0; refusal exit 1 with schema bank-contract-result-v1, operation, valid=false, error, writes_performed=false, optional capacity details.

bank_layout:
- model: no flags.
- render-index: `--input` REQUIRED.
- validate-index: `--file` REQUIRED; `--path` REQUIRED (canonical bank-relative location).
- inspect: `--repo`, default `.`.
- Success exit 0; refusal exit 1. Failure text is the baseline generic layout refusal except capacity errors, which retain their structured capacity message/details. No secret parser metadata is disclosed.

bank_writes:
- inspect: `--repo`, default `.`.
- preview: `--repo`, default `.`; `--path` REQUIRED; `--content` REQUIRED (file containing replacement Markdown). Typed preview's operation argument should be `replace` for this CLI route.
- recover: `--repo`, default `.`; `--proposal` OPTIONAL.
- apply: `--proposal` REQUIRED.
- Success exit 0; refusal exit 1 with schema write-result-v1, operation, error, verified=false, inspection_required=(operation==apply), optional capacity details.
- No mutually exclusive argparse groups or choices occur in these three baseline parsers.

## Obligations and tests

`tests/t1_bank.rs`: 30 passing tests. Every baseline-derived test has an exact `// obligation: tests/contracts/...::Class.method` annotation. No later-module provider was mocked or implemented as a test-only replacement.

Fully ported domain behavior (filesystem/CLI integration portions, where applicable, are called out below):
- RouterContract.test_render_refuses_empty_files_and_missing_omission_reasons
- RouterContract.test_byte_zero_schema_and_restricted_metadata_errors
- RouterContract.test_timezone_offsets_are_validated_without_normalizing_invalid_components
- RouterContract.test_missing_empty_and_omitted_existing_targets_fail
- RouterContract.test_router_bounds_duplicate_routes_and_orphan_files
- RouterContract.test_optional_authority_anchor_uses_the_common_reader_and_canonical_identifiers
- LayoutContract.test_index_byte_row_and_unicode_framing_limits_are_enforced_before_a_valid_result
- LayoutContract.test_source_index_locations_reject_controls_and_preserve_ordinary_unicode_and_spaces

Partially ported obligations:
- RouterContract.test_expanded_bank_has_six_useful_contexts_and_independent_router_time: complete in-memory format behavior plus prose-preserving route addition; public repo observation waits for bank_observation.
- LayoutContract.test_all_optional_directories_preserve_record_identity_provenance_conditions_and_nonbinding_procedures: all four record directories, metadata/provenance/conditions, route constraints and parsed companions tested; actual inspect/Git-index preservation waits for bank_observation and CLI.
- LayoutContract.test_two_nested_indices_resolve_correct_relative_links_and_retain_inherited_conditions_after_clone: exact relative target, two levels, inherited scope chain and identical copied-map results tested; actual Git clone and public inspect wait for bank_observation/CLI.
- LayoutContract.test_missing_orphan_cyclic_duplicate_and_overdeep_routes_refuse_without_repair: all five complete-map refusal cases and input preservation tested; public filesystem observer wrapper waits for bank_observation.
- LayoutContract.test_metadata_project_status_unadmitted_unknown_schema_and_inherited_scope_mismatches_refuse: all seven cases and body-free error adapter tested; public inspect wrapper waits for bank_observation.
- LayoutContract.test_optional_history_event_is_not_current_guidance_and_duplicate_or_deletion_labels_refuse: all five cases, noncurrent output, optional event removal, canonical current-byte preservation tested; public inspect wrapper waits for bank_observation.
- LayoutContract.test_tombstones_and_duplicate_record_versions_cannot_hide_or_recreate_current_content: duplicate exact record versions and multiple active versions tested. Safe tombstones, resurrection and non-Markdown physical detection wait for tombstones::TombstoneParser and bank_observation.

Additional focused tests cover a literal golden router byte string derived from the baseline renderer; route insertion with compact section separation; report/companion memo retention; Unicode/case component aliases and file-vs-directory topology; real physical inventory and symlinks even inside excluded runtime subtrees; bank-root spelling aliases without repair; bounded T0 listing; canonical/private external state paths and symlink/nondirectory refusals; exact private receipt bytes/mode and unexpected-receipt preservation; missing-provider refusals; and full-result UTF-8 replacement capacity at 16,384/16,385 bytes.

Deferred integration obligations (not claimed migrated):
- RouterContract.test_minimal_bank_resumes_from_only_canonical_files, portable-helper copy behavior, and filesystem portions of the annotated router tests: bank_observation + recovery_frontier + CLI wiring.
- Both BankUsabilityContract scenarios: bank_observation + recovery_frontier and CLI; clone/generated-state-loss setup also requires the integration fixture owner.
- LayoutContract.test_copied_artifact_preserves_index_and_code_and_refuses_busy_or_interrupted_banks: bank_observation + CLI/runtime distribution.
- All end-to-end BankWritesContract methods, concurrent-writer scenarios, bank-eligibility integration, and bounded-bank-observation integration: real bank_observation, scope_binding/project_identity, recovery_frontier, security, turn_sources, checkpoint/recovery_transactions where relevant, and CLI. The full observed-write implementation is present, but these tests cannot truthfully run until those production providers land.
- Checkpoint missing-context preparation: bank_transactions + checkpoint + scope_binding; add_context_routes itself is tested here.

## Declared deltas

- Physical/root directory inspection is deterministic (sorted through T0) and explicitly bounded to 1,000,000 entries; physical recursion is bounded to depth 256. The baseline os.walk/read-directory behavior was unbounded and host-order-dependent. Crossing a bound refuses without partial adoption or repair.
- Receipt reads have a named 64 MiB limit and fault-world JSON a named 1 MiB limit; JSON nesting and UTF-8 restrictions inherit the shared strict codec. The baseline used unbounded receipt/world-file reads.
- Non-UTF-8 bank paths/source text cannot become Rust text/JSON: a typed refusal preserves original files. No interpreter replacement-decoding or surrogate behavior is reproduced.
- State roots containing parent traversal are refused rather than relying on Python Path spelling/resolve accidents. state_paths also explicitly requires a canonical UUIDv4 operation ID before constructing a receipt name; all baseline runtime callers pass UUIDv4 IDs (including the private-state sentinel).
- Fault rendezvous uses held no-follow descriptors and refuses symlinked ancestors/special rendezvous nodes, rather than accepting ambiguous path-based checks. Test roots here are canonicalized before use, including macOS /tmp resolution.
- Replacement temporaries use T0 UUID names with the same .agent-memory-write- prefix and .tmp suffix instead of tempfile's interpreter-dependent random spelling. Durable content, mode, lock and receipt paths are unchanged.
- The immediate pre-rename target signature and temporary inode are rechecked in addition to the baseline complete hash-map check. Same-byte inode/metadata replacement races refuse instead of being silently adopted. Temporary cleanup unlinks only the inode this operation created.
- An ill-shaped registered recovery provider lacking hold_sha256 gives an explicit typed refusal rather than accidental Python KeyError behavior.
- Missing lazy-module registrations are explicit typed refusals, as required by the cutover design. No disabled safety guard or fake fallback is supplied.

## Verification actually run

In the assigned worktree only:

1. `cargo test --offline --test t1_bank` — 11 passed (initial bank_contract handover).
2. `cargo test --offline --test t1_bank` — 11 passed (bank_writes handover).
3. `cargo test --offline --test t1_bank` — 25 passed (expanded domain/filesystem tests).
4. `cargo fmt --all` — success.
5. `cargo fmt --check` — success.
6. `cargo check --offline --all-targets` — success; no warnings in owned files. Seven pre-existing unused-import warnings were emitted only from the separately owned `tests/t1_a2.rs`.
7. `cargo test --offline --test t1_bank` — 26 passed.
8. `cargo test --offline --test t1_bank` — 26 passed.
9. `cargo test --offline --test t1_bank` — 26 passed.

Following Main's malformed-input review, the three modules were hardened without changing existing public signatures, four malformed-input regressions were added, and the final three runs passed 30/30. `t1_a2` was not executed. Linux verification and production-provider integration remain Main's responsibility.

## Added-rule review and final verification

No runtime domain function or branch in the three assigned baseline modules is left unported. The word “partial” above refers only to test-obligation coverage. The deliberately unported Python entry-point pieces are exactly: argparse/parser-error handling in the three `main` functions; `bank_contract.main`'s argv normalization and input-file/stdin JSON loading; `bank_layout.main`'s input/index-file reads; and `bank_writes.main`'s content/proposal-file reads. Their four/two/four semantic command branches are provided as typed functions and command-result adapters, with recover-layout forwarded through RouteTable. Production implementations of the separately owned lazy-import modules remain required registrations, not omitted safety checks or stub branches in these modules.

Removed every unwrap on router/proposal/receipt text, decoded runtime values, state paths and memo values. Section extraction uses fallible lookups. Memo tuple/companion decoding uses checked slice patterns, not indices. Command-output encoder failures now return each module's ordinary error result instead of panicking. Added public `bank_writes::load_receipt(&[u8]) -> Result<Value>` as the shared bounded strict receipt decoder actually used by apply; all pre-existing signatures remain stable.

Four added tests assert exact refusals for malformed/reversed/truncated router text (including every UTF-8-boundary prefix of the golden router), missing/mistyped proposal fields, a truncated on-disk receipt preserved byte-for-byte, and overdeep result encoding. The truncated receipt deterministically refuses with `Expecting property name enclosed in double quotes at byte 1`; Python interpreter-version-specific JSONDecodeError formatting is not reproduced.

Additional commands actually run:
10. `cargo fmt --all` — success.
11. `cargo test --offline --test t1_bank` — 30 passed.
12. `cargo fmt --check` — success.
13. `cargo check --offline --all-targets` — success; the same seven unrelated t1_a2 unused-import warnings only.
14. `cargo test --offline --test t1_bank` — 30 passed.
15. `cargo test --offline --test t1_bank` — 30 passed.
16. `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/bank_contract.rs src/bank_layout.rs src/bank_writes.rs` — 11 hits, reproduced and justified below.
17. `grep -n 'Command\|process::\|Deadline\|timeout\|FAULT_PAUSE_SECONDS' src/bank_contract.rs src/bank_layout.rs src/bank_writes.rs` — no child-launch call sites; only CommandResult type names, the intentional test-fault process exit, and the fault rendezvous's finite baseline 10-second deadline.

Final required panic-site grep:
```text
src/bank_contract.rs:24:    .expect("bundled bank router schema")
src/bank_contract.rs:30:        .unwrap()
src/bank_contract.rs:39:        .unwrap()
src/bank_contract.rs:41:        .map(|v| v.as_str().unwrap())
src/bank_contract.rs:238:    let mut parts = vec![".."; source.len() - common];
src/bank_contract.rs:1016:            merge(&mut v, &bank_capacity::details(&e)).expect("capacity result fields");
src/bank_layout.rs:24:    .expect("bundled topic index schema")
src/bank_layout.rs:496:            bank::merge(&mut value, &bank_capacity::details(&e)).expect("capacity result fields");
src/bank_writes.rs:170:    digest(Some(bytes)).expect("present digest input")
src/bank_writes.rs:988:                .expect("capacity result fields");
src/bank_writes.rs:991:                result(operation, &fields).expect("write error result"),
```

Justification of every hit: bank_contract 24/30/39/41 and bank_layout 24 refer exclusively to reviewed compile-time include_str schemas, never files read at runtime, proposals, registry values or child output; each enclosing initializer has an explicit invariant comment. Contract 238 is a grep false positive: a vec repetition of the literal `..`, not indexing/slicing; common is counted from a zip with source and cannot exceed source.len(). Contract 1016, layout 496 and writes 988/991 operate only on objects constructed within the same error branch; one-line invariant comments precede them. Writes 170 passes Some(bytes) in that function, so digest necessarily returns Some; the comment states that invariant. The only remaining ordinary indexing in the domain modules is the five-column route-row cell access, immediately preceded by the local cardinality check and its invariant comment. There are no input-derived unwraps, unchecked input slices, unreachable branches, or child launches in these three modules. No direct child process requires a deadline here; repository discovery calls the already-ported repository_config API rather than spawning independently.
