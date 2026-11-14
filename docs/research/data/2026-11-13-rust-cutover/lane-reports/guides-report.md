# Guides lane report

## Result and baseline

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-guides`, detached baseline `1b878ab`. No Git writes, no Python execution, no sub-agents, no AWA/Vox/GitHub use. Runtime prerequisites were refreshed, at Main's request, from committed integration head `f739117`; owned changes were retained. `src/cli/table.rs` now includes its committed `identity` registration. No command registrations were invented by this lane.

Migrated every active packaged-helper invocation in the owned guidance to its public `am` command, preserving operands, flags, redirections, continuation lines and fence languages. Prefixed routes consume their already-supplied prefix (`bank_state.py inspect` → `am inspect`; `admit_learning.py capture` → `am capture`). Layout recovery uses the existing nested `am bank recover-layout` form, not a new root command. The command ledger below contains 168 mappings, including inline helper references and the CLI catalog.

The copied pack is explicitly guidance/data, not a self-executing runtime. It requires bootstrap-installed `am`; `am --version` checks availability. Removed interpreter prerequisites and bytecode/cache-only instructions, preserving ordinary Markdown/file-only fallback, verifier trust, drift preservation and recovery boundaries. All headings, anchors and section order remain. One obsolete executable-helper link in `turn/GUIDE.md` was migrated from `../scripts/admit_learning.py` to the admission operation guide `../admission/GUIDE.md`, so it no longer routes to removed executable source.

## Owned files modified

- `skills/agent-memory/SKILL.md`
- `skills/agent-memory/access/GUIDE.md`
- `skills/agent-memory/admission/GUIDE.md`
- `skills/agent-memory/assembly/GUIDE.md`
- `skills/agent-memory/authority/GUIDE.md`
- `skills/agent-memory/bank/GUIDE.md`
- `skills/agent-memory/bank/inspect/GUIDE.md`
- `skills/agent-memory/checkpoint/GUIDE.md`
- `skills/agent-memory/comparison/GUIDE.md`
- `skills/agent-memory/consolidation/GUIDE.md`
- `skills/agent-memory/discovery/GUIDE.md`
- `skills/agent-memory/governance/GUIDE.md`
- `skills/agent-memory/identity/GUIDE.md`
- `skills/agent-memory/inactive/GUIDE.md`
- `skills/agent-memory/initialize/GUIDE.md`
- `skills/agent-memory/layout/GUIDE.md`
- `skills/agent-memory/lifecycle/diagnose/GUIDE.md`
- `skills/agent-memory/lifecycle/install/GUIDE.md`
- `skills/agent-memory/lifecycle/migrate/GUIDE.md`
- `skills/agent-memory/lifecycle/purge/GUIDE.md`
- `skills/agent-memory/lifecycle/uninstall/GUIDE.md`
- `skills/agent-memory/lifecycle/update/GUIDE.md`
- `skills/agent-memory/maintain/artifact/GUIDE.md`
- `skills/agent-memory/maintain/cli/GUIDE.md`
- `skills/agent-memory/maintain/pack/GUIDE.md`
- `skills/agent-memory/offline/GUIDE.md`
- `skills/agent-memory/projections/GUIDE.md`
- `skills/agent-memory/records/GUIDE.md`
- `skills/agent-memory/resume/GUIDE.md`
- `skills/agent-memory/retrieval/GUIDE.md`
- `skills/agent-memory/scope/GUIDE.md`
- `skills/agent-memory/security/GUIDE.md`
- `skills/agent-memory/sharing/boundary/GUIDE.md`
- `skills/agent-memory/sharing/publication/GUIDE.md`
- `skills/agent-memory/sharing/recovery/GUIDE.md`
- `skills/agent-memory/sharing/refresh/GUIDE.md`
- `skills/agent-memory/sharing/semantic/GUIDE.md`
- `skills/agent-memory/turn/GUIDE.md`
- `skills/agent-memory/writes/GUIDE.md`
- `src/pack_validation.rs`
- `src/cli/surface.rs`
- `tests/t1_a2.rs`
- `tests/t2_cli.rs`
- `tests/t2_delivery.rs` (required trait-method addition only; committed upstream test additions also retained)

No template or boundary-prompt changes were needed: those owned text assets contained no legacy executable references. Report created at `/Users/robert.lee/.local/state/agent-memory/research/rust-cutover/guides-report.md`.

### Committed prerequisites refreshed (not lane implementation changes)

- `Cargo.lock`
- `Cargo.toml`
- `src/bank_writes.rs`
- `src/bin/am_dev/release.rs`
- `src/cli/helpers/mod.rs`
- `src/cli/helpers/scope_binding.rs`
- `src/cli/mod.rs`
- `src/cli/table.rs`
- `src/cli/wiring.rs`
- `src/client_hooks.rs`
- `src/error.rs`
- `src/fs/descriptor.rs`
- `src/hook_failure.rs`
- `src/lib.rs`
- `src/managed_source.rs`
- `src/package.rs`
- `src/private_state.rs`
- `src/project_identity.rs`
- `src/release_distribution.rs`
- `src/scope_binding.rs`
- `src/sharing_hold_mapping.rs`
- `src/turn_check.rs`
- `tests/obligations/python-baseline-v1.json`
- `tests/t0_fs.rs`
- `tests/t1_private_state.rs`
- `tests/t1_project_identity.rs`
- `tests/t1_scope_binding.rs`
- `tests/t2_leaves.rs`
- `tests/wiring-pending.txt`
- `tests/wiring.rs`

Also integrated the committed `f739117` additions in the owned shared test files: declaration-derived CLI root-name assertions in `tests/t2_cli.rs` and the two delivery regression tests appended to `tests/t2_delivery.rs`. Initial `pack_validation.rs` and `surface.rs` content was taken from committed integration content as Main requested before editing.

## Validator changes (diff summary)

- Appended the required method `CommandSurface::command(&self, name: &str) -> Result<Option<(Dispatch, Vec<String>)>>`. The tuple is the declared parser dispatch plus its fixed route prefix. The existing `Dispatch` and `CommandRoute` shapes and `dispatch` method remain unchanged.
- `Declared::from_value` builds its native-command index solely from the existing signed declaration generated by `src/cli/table.rs`. Route entries resolve to the helper declaration and prefix; native entries derive their operations/options from their own parser. Duplicate commands and malformed/missing declared helper routes refuse. No parallel command-name list or executable source discovery was added.
- `Declared::dispatch` now resolves authenticated declaration data without requiring a packaged helper file. The executable code is in the binary, not `scripts/*.py` assets. `Declared::command` returns the declaration lookup; an undeclared command returns `None` and is converted to the existing `dispatch` issue by validation.
- Replaced Python/launcher dispatch in `check_dispatch` with literal `am`/`agent-memory` command resolution. It rejects legacy invocations with `Python helper invocations are not supported; document the am command`. Root `--help`/`-h`, command help, the global `--json` selector, fixed prefixes, nested delegated verbs and child flags are handled. Nested delegation is bounded at eight levels. Unknown command/verb/option issues keep code `dispatch` and name the public `am` command. Missing provider keeps `command-surface-unregistered`.
- Native documented-command receipts now identify `command` instead of an executable `script`; their module and operation labels remain declaration-derived. Artifact inspection only forwards this metadata; there is no compiled consumer requiring the old script field.
- `git_operation`, `command_argv`, structural validation, the production fail-fast order and issue ordering are unchanged. The existing Python-inline detector still detects/refuses legacy inline interpreter invocations rather than executing them.
- `tests/t2_delivery.rs`'s intentionally non-executable `NoExecutableSurface` implements the new required method with `Ok(None)` (Main-approved).

### Tests and replaced baseline assertions

- Added CLI tests for real-table route/helper-only resolution, prefixed routes, nested delegated verb/flag checking, aliases, help and JSON selection, unknown command/verb/flag refusal, and both legacy interpreter/launcher forms refusing. Test-only amended declarations exercise nested routes/prefixes using the actual bank, authority and developer parser declarations; they do not register product commands.
- Replaced the Python-era `tests/t2_cli.rs` missing-helper-file assertion (baseline lines 666–676) with declaration lookup against an empty pack inventory and absence for an undeclared command. The same test's unknown helper identifier is now deliberately nonexistent rather than `retrieve.py`, so later retrieval registration does not invalidate it. The obligation annotation is retained.
- Updated `tests/t1_a2.rs`'s candidate-code/provider-unregistered fixture to use `am checkpoint inspect` instead of an interpreter invocation. Its candidate-code nonexecution assertion and obligation annotation are retained.
- Added `native_guide_structure::committed_native_pack_preserves_sections_links_loading_and_reachability`: inventories the actual pack, calls the same public frontmatter/Markdown/link/anchor/mandatory-target/literal-command/Git-operation scanners as the validator, and checks every operation's ordered nonempty sections, owner loading/navigation, root routes, reachability and mandatory-loading acyclicity. No permissive surface or modified pack is used. This independently proves the structure beyond the first unregistered command while preserving production's fail-fast order.

## Unregistered commands

At the assigned base `1b878ab`, the following documented groups/subcommands are absent from `src/cli/table.rs` (including aliases listed in the CLI catalog):

- `am admission`
- `am boundary`
- `am capture`
- `am checkpoint`
- `am claude-boundary`
- `am codex-boundary`
- `am compact`
- `am compare`
- `am context`
- `am correct`
- `am discover`
- `am doctor`
- `am exchange`
- `am explain`
- `am forget`
- `am get`
- `am govern`
- `am identity`
- `am inactive-records`
- `am init`
- `am inspect`
- `am install`
- `am migrate`
- `am omp-boundary`
- `am private`
- `am project`
- `am projection`
- `am publish`
- `am purge-data`
- `am recover`
- `am refresh`
- `am revoke`
- `am search`
- `am security-policy`
- `am semantic`
- `am sharing`
- `am status`
- `am supersede`
- `am tombstones`
- `am transaction`
- `am uninstall`
- `am update`
- `am version`
- `am bank recover-layout` (nested route missing from the registered `bank` helper)

After the approved `f739117` prerequisite refresh, `am identity` is registered; the remaining absent commands are:

- `am admission`
- `am boundary`
- `am capture`
- `am checkpoint`
- `am claude-boundary`
- `am codex-boundary`
- `am compact`
- `am compare`
- `am context`
- `am correct`
- `am discover`
- `am doctor`
- `am exchange`
- `am explain`
- `am forget`
- `am get`
- `am govern`
- `am inactive-records`
- `am init`
- `am inspect`
- `am install`
- `am migrate`
- `am omp-boundary`
- `am private`
- `am project`
- `am projection`
- `am publish`
- `am purge-data`
- `am recover`
- `am refresh`
- `am revoke`
- `am search`
- `am security-policy`
- `am semantic`
- `am sharing`
- `am status`
- `am supersede`
- `am tombstones`
- `am transaction`
- `am uninstall`
- `am update`
- `am version`
- `am bank recover-layout` (nested route still absent)

`am help`, `am --help` and `am -h` are intrinsic CLI help, not missing table registrations. Lifecycle version flags select the documented version operation. These missing declarations are expected integration dependencies being ported by other lanes. No Markdown/link/anchor/section issue was observed; the independent all-guide structure test passed. The production validator remains fail-fast and therefore prints only the first absent-command issue, not an invented complete error list.

## Verification

Platform: `aarch64-apple-darwin`, Darwin `27.3.0 arm64`; `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, LLVM 23.1.1. Other targets were not run. LSP reference lookup was attempted and reported no language server; bounded source reference search was used instead.

Commands and observed results:

1. `cargo build --offline --bin am` — exit 0; build finished successfully without warnings.
2. First `cargo test --offline --test t1_a2 --test t2_cli` — compilation refused two private imports in the newly added test fixture (`lifecycle::{object, string}`). Fixed the fixture to use the existing test JSON decoder, without changing production visibility.
3. Corrected `cargo test --offline --test t1_a2 --test t2_cli` — exit 0: **t1_a2: 80 passed; t2_cli: 18 passed**, no ignored or failed tests. Includes the all-guide structural test and all new native dispatch tests. Raw final test output: `artifact://11254`.
4. `./target/debug/am validate-pack --pack skills/agent-memory` — exit 1, exactly the expected unregistered-command issue below. It was run after the successful native build; subsequent edits only fixed test fixture construction, not the runtime or guidance.
5. `grep -rnE 'scripts/[a-z_]+\.py|python3|Python' skills/agent-memory --include='*.md' --include='*.txt'` — no matches, grep exit 1 (expected for empty results). **Retained historical matches: none.** The ordinary illustrative source paths `src/api/retry.py` and `src/app.py` remain unchanged; neither instructs executing a helper and neither matches the acceptance expression.

Exact real-validator output:

```json
{
  "schema": "agent-memory/pack-validation-v1",
  "valid": false,
  "writes_performed": false,
  "candidate_code_executed": false,
  "semantic_completeness_verified": false,
  "issues": [
    {
      "code": "dispatch",
      "path": "admission/GUIDE.md",
      "detail": "Command am capture is not declared by the command surface"
    }
  ]
}
```

No formatter, linter, broad suite, feature-enabled delivery suite or cross-platform run was performed. Main owns integration verification once the remaining command registrations land. No foundation changes or new runtime registration traits beyond the appended command-surface method; no obligation-ledger modifications by this lane.

## Complete command migration table

Locations are **final guidance file line numbers**. Continued command lines are joined for readability; their operands and flags remain unchanged in the files. Inline helper references and CLI catalog entries are included as well as every executable example.

file:line | old command | new command
--- | --- | ---
`skills/agent-memory/access/GUIDE.md:54` | <code>python3 -I "$SKILL_DIR/scripts/scope_binding.py" status --repo /path/to/repo</code> | <code>am identity status --repo /path/to/repo</code>
`skills/agent-memory/admission/GUIDE.md:29` | <code>admit_learning.py</code> | <code>am admission</code>
`skills/agent-memory/admission/GUIDE.md:89` | <code>python3 -I "$SKILL_DIR/scripts/admit_learning.py" capture --repo /path/to/repo --request /private/path/capture-request.json &gt; /private/path/captured.json</code> | <code>am capture --repo /path/to/repo --request /private/path/capture-request.json &gt; /private/path/captured.json</code>
`skills/agent-memory/admission/GUIDE.md:127` | <code>python3 -I "$SKILL_DIR/scripts/admit_learning.py" preview --repo /path/to/repo --capture /private/path/captured.json --request /private/path/comparison-request.json --proof /private/path/comparison-review.json &gt; /private/path/admission.json</code> | <code>am admission preview --repo /path/to/repo --capture /private/path/captured.json --request /private/path/comparison-request.json --proof /private/path/comparison-review.json &gt; /private/path/admission.json</code>
`skills/agent-memory/admission/GUIDE.md:130` | <code>python3 -I "$SKILL_DIR/scripts/admit_learning.py" apply --proposal /private/path/admission.json</code> | <code>am admission apply --proposal /private/path/admission.json</code>
`skills/agent-memory/assembly/GUIDE.md:69` | <code>python3 -I "$SKILL_DIR/scripts/assemble_context.py" preview --repo /path/to/repo --request /private/path/assembly.json</code> | <code>am context preview --repo /path/to/repo --request /private/path/assembly.json</code>
`skills/agent-memory/assembly/GUIDE.md:88` | <code>python3 -I "$SKILL_DIR/scripts/assemble_context.py" check --repo /path/to/repo --request /private/path/assembly.json --counter /private/path/counter.json</code> | <code>am context check --repo /path/to/repo --request /private/path/assembly.json --counter /private/path/counter.json</code>
`skills/agent-memory/authority/GUIDE.md:60` | <code>python3 -I "$SKILL_DIR/scripts/authority_policy.py" model</code> | <code>am authority-policy model</code>
`skills/agent-memory/authority/GUIDE.md:61` | <code>python3 -I "$SKILL_DIR/scripts/authority_policy.py" assess --input /tmp/reviewed-assertions.json</code> | <code>am authority-policy assess --input /tmp/reviewed-assertions.json</code>
`skills/agent-memory/bank/GUIDE.md:69` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" inspect --repo REPO --output PRIVATE_INSPECTION_JSON</code> | <code>am bank recover-layout inspect --repo REPO --output PRIVATE_INSPECTION_JSON</code>
`skills/agent-memory/bank/GUIDE.md:109` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" inspect --repo REPO --output PRIVATE_INSPECTION_JSON</code> | <code>am bank recover-layout inspect --repo REPO --output PRIVATE_INSPECTION_JSON</code>
`skills/agent-memory/bank/GUIDE.md:111` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" inspect --repo REPO --operation OPERATION_UUID --page 0 --output PRIVATE_PAGE_JSON</code> | <code>am bank recover-layout inspect --repo REPO --operation OPERATION_UUID --page 0 --output PRIVATE_PAGE_JSON</code>
`skills/agent-memory/bank/GUIDE.md:112` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" prepare --repo REPO --operation OPERATION_UUID --input PRIVATE_REVIEW_JSON --output PRIVATE_PROPOSAL_JSON</code> | <code>am bank recover-layout prepare --repo REPO --operation OPERATION_UUID --input PRIVATE_REVIEW_JSON --output PRIVATE_PROPOSAL_JSON</code>
`skills/agent-memory/bank/GUIDE.md:113` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" apply --repo REPO --operation OPERATION_UUID --proposal PRIVATE_PROPOSAL_JSON</code> | <code>am bank recover-layout apply --repo REPO --operation OPERATION_UUID --proposal PRIVATE_PROPOSAL_JSON</code>
`skills/agent-memory/bank/GUIDE.md:114` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" status --repo REPO --operation OPERATION_UUID</code> | <code>am bank recover-layout status --repo REPO --operation OPERATION_UUID</code>
`skills/agent-memory/bank/GUIDE.md:229` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" recover --repo REPO --operation OPERATION_UUID --mode finish --output PRIVATE_FINISH_JSON</code> | <code>am bank recover-layout recover --repo REPO --operation OPERATION_UUID --mode finish --output PRIVATE_FINISH_JSON</code>
`skills/agent-memory/bank/GUIDE.md:230` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" recover --repo REPO --proposal PRIVATE_FINISH_JSON</code> | <code>am bank recover-layout recover --repo REPO --proposal PRIVATE_FINISH_JSON</code>
`skills/agent-memory/bank/GUIDE.md:231` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" rollback --repo REPO --operation OPERATION_UUID --output PRIVATE_RESTORE_JSON</code> | <code>am bank recover-layout rollback --repo REPO --operation OPERATION_UUID --output PRIVATE_RESTORE_JSON</code>
`skills/agent-memory/bank/GUIDE.md:232` | <code>python3 -I "$SKILL_DIR/scripts/recover_layout.py" rollback --repo REPO --proposal PRIVATE_RESTORE_JSON</code> | <code>am bank recover-layout rollback --repo REPO --proposal PRIVATE_RESTORE_JSON</code>
`skills/agent-memory/bank/inspect/GUIDE.md:36` | <code>python3 -I "$SKILL_DIR/scripts/bank_state.py" inspect --repo /path/to/project</code> | <code>am inspect --repo /path/to/project</code>
`skills/agent-memory/bank/inspect/GUIDE.md:42` | <code>python3 -I "$SKILL_DIR/scripts/bank_state.py" inspect --repo /path/to/project --path activeContext.md</code> | <code>am inspect --repo /path/to/project --path activeContext.md</code>
`skills/agent-memory/checkpoint/GUIDE.md:93` | <code>python3 -I "$SKILL_DIR/scripts/checkpoint.py" inspect --repo /path/to/repo &gt; /private/path/current.json</code> | <code>am checkpoint inspect --repo /path/to/repo &gt; /private/path/current.json</code>
`skills/agent-memory/checkpoint/GUIDE.md:140` | <code>python3 -I "$SKILL_DIR/scripts/checkpoint.py" preview --repo /path/to/repo --input /private/path/fields.json &gt; /private/path/checkpoint.json</code> | <code>am checkpoint preview --repo /path/to/repo --input /private/path/fields.json &gt; /private/path/checkpoint.json</code>
`skills/agent-memory/checkpoint/GUIDE.md:142` | <code>python3 -I "$SKILL_DIR/scripts/checkpoint.py" apply --proposal /private/path/checkpoint.json</code> | <code>am checkpoint apply --proposal /private/path/checkpoint.json</code>
`skills/agent-memory/checkpoint/GUIDE.md:148` | <code>python3 -I "$SKILL_DIR/scripts/checkpoint.py" prepare --repo /path/to/repo --input /private/path/fields.json &gt; /private/path/prepare.json</code> | <code>am checkpoint prepare --repo /path/to/repo --input /private/path/fields.json &gt; /private/path/prepare.json</code>
`skills/agent-memory/checkpoint/GUIDE.md:155` | <code>bank_transactions.py apply</code> | <code>am transaction apply</code>
`skills/agent-memory/checkpoint/GUIDE.md:180` | <code>bank_writes.py recover</code> | <code>am writes recover</code>
`skills/agent-memory/comparison/GUIDE.md:68` | <code>python3 -I "$SKILL_DIR/scripts/compare_learning.py" inspect --repo /path/to/repo --request /private/path/comparison.json</code> | <code>am compare inspect --repo /path/to/repo --request /private/path/comparison.json</code>
`skills/agent-memory/comparison/GUIDE.md:161` | <code>python3 -I "$SKILL_DIR/scripts/compare_learning.py" review --repo /path/to/repo --request /private/path/comparison.json --judgment /private/path/judgment.json</code> | <code>am compare review --repo /path/to/repo --request /private/path/comparison.json --judgment /private/path/judgment.json</code>
`skills/agent-memory/comparison/GUIDE.md:194` | <code>python3 -I "$SKILL_DIR/scripts/compare_learning.py" check --repo /path/to/repo --request /private/path/comparison.json --proof /private/path/proof.json</code> | <code>am compare check --repo /path/to/repo --request /private/path/comparison.json --proof /private/path/proof.json</code>
`skills/agent-memory/consolidation/GUIDE.md:25` | <code>consolidate_learning.py</code> | <code>am compact</code>
`skills/agent-memory/consolidation/GUIDE.md:86` | <code>python3 -I "$SKILL_DIR/scripts/consolidate_learning.py" preview --repo /path/to/repo --input /private/path/maintenance-input.json &gt; /private/path/maintenance.json</code> | <code>am compact preview --repo /path/to/repo --input /private/path/maintenance-input.json &gt; /private/path/maintenance.json</code>
`skills/agent-memory/consolidation/GUIDE.md:86` | <code>python3 -I "$SKILL_DIR/scripts/consolidate_learning.py" preview --repo /path/to/repo --input /private/path/maintenance-input.json --plan /private/path/plan.json --private-output /private/path/maintenance.json</code> | <code>am compact preview --repo /path/to/repo --input /private/path/maintenance-input.json --plan /private/path/plan.json --private-output /private/path/maintenance.json</code>
`skills/agent-memory/consolidation/GUIDE.md:88` | <code>python3 -I "$SKILL_DIR/scripts/consolidate_learning.py" apply --proposal /private/path/maintenance.json</code> | <code>am compact apply --proposal /private/path/maintenance.json</code>
`skills/agent-memory/consolidation/GUIDE.md:88` | <code>python3 -I "$SKILL_DIR/scripts/consolidate_learning.py" apply --proposal /private/path/maintenance.json</code> | <code>am compact apply --proposal /private/path/maintenance.json</code>
`skills/agent-memory/consolidation/GUIDE.md:96` | <code>python3 -I "$SKILL_DIR/scripts/consolidate_learning.py" plan --repo /path/to/repo --input /private/path/planning-input.json --private-output /private/path/plan.json</code> | <code>am compact plan --repo /path/to/repo --input /private/path/planning-input.json --private-output /private/path/plan.json</code>
`skills/agent-memory/discovery/GUIDE.md:35` | <code>python3 -I "$SKILL_DIR/scripts/discover_memory.py" start --repo /path/to/repo --intent comparison --task 'Determine what this new observation changes'</code> | <code>am discover start --repo /path/to/repo --intent comparison --task 'Determine what this new observation changes'</code>
`skills/agent-memory/discovery/GUIDE.md:77` | <code>python3 -I "$SKILL_DIR/scripts/discover_memory.py" node --repo /path/to/repo --input /private/navigation.json --path topics/request-failures.md</code> | <code>am discover node --repo /path/to/repo --input /private/navigation.json --path topics/request-failures.md</code>
`skills/agent-memory/discovery/GUIDE.md:109` | <code>python3 -I "$SKILL_DIR/scripts/discover_memory.py" read --repo /path/to/repo --input /private/navigation.json --request /private/request.json --rationale 'These actual routes cover the request retry policy and its conditions.'</code> | <code>am discover read --repo /path/to/repo --input /private/navigation.json --request /private/request.json --rationale 'These actual routes cover the request retry policy and its conditions.'</code>
`skills/agent-memory/governance/GUIDE.md:29` | <code>govern_memory.py preview</code> | <code>am govern preview</code>
`skills/agent-memory/governance/GUIDE.md:191` | <code>python3 -I "$SKILL_DIR/scripts/govern_memory.py" preview --repo /path/to/repo --request /private/path/governance-request.json &gt; /private/path/governance.json</code> | <code>am govern preview --repo /path/to/repo --request /private/path/governance-request.json &gt; /private/path/governance.json</code>
`skills/agent-memory/governance/GUIDE.md:191` | <code>python3 -I "$SKILL_DIR/scripts/govern_memory.py" preview --repo /path/to/repo --request /private/path/governance-request.json --plan /private/path/plan.json --private-output /private/path/governance.json</code> | <code>am govern preview --repo /path/to/repo --request /private/path/governance-request.json --plan /private/path/plan.json --private-output /private/path/governance.json</code>
`skills/agent-memory/governance/GUIDE.md:193` | <code>python3 -I "$SKILL_DIR/scripts/govern_memory.py" apply --proposal /private/path/governance.json</code> | <code>am govern apply --proposal /private/path/governance.json</code>
`skills/agent-memory/governance/GUIDE.md:193` | <code>python3 -I "$SKILL_DIR/scripts/govern_memory.py" apply --proposal /private/path/governance.json</code> | <code>am govern apply --proposal /private/path/governance.json</code>
`skills/agent-memory/governance/GUIDE.md:200` | <code>python3 -I "$SKILL_DIR/scripts/govern_memory.py" plan --repo /path/to/repo --request /private/path/planning-request.json --private-output /private/path/plan.json</code> | <code>am govern plan --repo /path/to/repo --request /private/path/planning-request.json --private-output /private/path/plan.json</code>
`skills/agent-memory/identity/GUIDE.md:58` | <code>python3 -I "$SKILL_DIR/scripts/developer_identity.py" preview --create --label 'Developer label' &gt; /private/path/developer.json</code> | <code>am developer preview --create --label 'Developer label' &gt; /private/path/developer.json</code>
`skills/agent-memory/identity/GUIDE.md:60` | <code>python3 -I "$SKILL_DIR/scripts/developer_identity.py" apply --proposal /private/path/developer.json</code> | <code>am developer apply --proposal /private/path/developer.json</code>
`skills/agent-memory/identity/GUIDE.md:65` | <code>python3 -I "$SKILL_DIR/scripts/scope_binding.py" preview --repo /path/to/repo --action bind --reason 'Adopt this existing project bank' &gt; /private/path/bind.json</code> | <code>am identity preview --repo /path/to/repo --action bind --reason 'Adopt this existing project bank' &gt; /private/path/bind.json</code>
`skills/agent-memory/identity/GUIDE.md:67` | <code>python3 -I "$SKILL_DIR/scripts/scope_binding.py" apply --proposal /private/path/bind.json</code> | <code>am identity apply --proposal /private/path/bind.json</code>
`skills/agent-memory/identity/GUIDE.md:67` | <code>python3 -I "$SKILL_DIR/scripts/scope_binding.py" apply --proposal /private/path/bind.json</code> | <code>am identity apply --proposal /private/path/bind.json</code>
`skills/agent-memory/identity/GUIDE.md:109` | <code>python3 -I "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo &gt; /private/path/unlock.json</code> | <code>am writes recover --repo /path/to/repo &gt; /private/path/unlock.json</code>
`skills/agent-memory/identity/GUIDE.md:110` | <code>python3 -I "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo --proposal /private/path/unlock.json</code> | <code>am writes recover --repo /path/to/repo --proposal /private/path/unlock.json</code>
`skills/agent-memory/inactive/GUIDE.md:48` | <code>python3 -I "$SKILL_DIR/scripts/inactive_records.py" compact --file /private/path/inactive-record.md</code> | <code>am inactive-records compact --file /private/path/inactive-record.md</code>
`skills/agent-memory/inactive/GUIDE.md:53` | <code>python3 -I "$SKILL_DIR/scripts/inactive_records.py" filter --repo /path/to/repo --input /private/path/derived-records.json</code> | <code>am inactive-records filter --repo /path/to/repo --input /private/path/derived-records.json</code>
`skills/agent-memory/inactive/GUIDE.md:96` | <code>tombstones.py filter</code> | <code>am tombstones filter</code>
`skills/agent-memory/initialize/GUIDE.md:148` | <code>python3 -I "$SKILL_DIR/scripts/initialize.py" inspect --repo /path/to/repo &gt; /private/path/inventory.json</code> | <code>am init inspect --repo /path/to/repo &gt; /private/path/inventory.json</code>
`skills/agent-memory/initialize/GUIDE.md:149` | <code>python3 -I "$SKILL_DIR/scripts/initialize.py" read --repo /path/to/repo --path README.md</code> | <code>am init read --repo /path/to/repo --path README.md</code>
`skills/agent-memory/initialize/GUIDE.md:150` | <code>python3 -I "$SKILL_DIR/scripts/initialize.py" read --repo /path/to/repo --path docs/adr/001-example.md</code> | <code>am init read --repo /path/to/repo --path docs/adr/001-example.md</code>
`skills/agent-memory/initialize/GUIDE.md:173` | <code>python3 -I "$SKILL_DIR/scripts/initialize.py" inspect --repo /path/to/repo --awa-evidence /private/path/awa-evidence.json &gt; /private/path/inventory.json</code> | <code>am init inspect --repo /path/to/repo --awa-evidence /private/path/awa-evidence.json &gt; /private/path/inventory.json</code>
`skills/agent-memory/initialize/GUIDE.md:213` | <code>python3 -I "$SKILL_DIR/scripts/initialize.py" preview --repo /path/to/repo --review /private/path/review.json &gt; /private/path/initialize.json</code> | <code>am init preview --repo /path/to/repo --review /private/path/review.json &gt; /private/path/initialize.json</code>
`skills/agent-memory/initialize/GUIDE.md:215` | <code>python3 -I "$SKILL_DIR/scripts/initialize.py" apply --proposal /private/path/initialize.json</code> | <code>am init apply --proposal /private/path/initialize.json</code>
`skills/agent-memory/initialize/GUIDE.md:246` | <code>bank_transactions.py recover --repo PATH --mode finish</code> | <code>am transaction recover --repo PATH --mode finish</code>
`skills/agent-memory/layout/GUIDE.md:64` | <code>bank_contract.py validate</code> | <code>am bank validate</code>
`skills/agent-memory/layout/GUIDE.md:127` | <code>python3 -I "$SKILL_DIR/scripts/bank_layout.py" inspect --repo /path/to/repo</code> | <code>am bank-layout inspect --repo /path/to/repo</code>
`skills/agent-memory/layout/GUIDE.md:128` | <code>python3 -I "$SKILL_DIR/scripts/bank_layout.py" model</code> | <code>am bank-layout model</code>
`skills/agent-memory/layout/GUIDE.md:129` | <code>python3 -I "$SKILL_DIR/scripts/bank_layout.py" render-index --input /tmp/topic-index.json</code> | <code>am bank-layout render-index --input /tmp/topic-index.json</code>
`skills/agent-memory/layout/GUIDE.md:130` | <code>python3 -I "$SKILL_DIR/scripts/bank_layout.py" validate-index --file memory-bank/topics/index.md --path topics/index.md</code> | <code>am bank-layout validate-index --file memory-bank/topics/index.md --path topics/index.md</code>
`skills/agent-memory/lifecycle/diagnose/GUIDE.md:35` | <code>python3 -I "$SKILL_DIR/scripts/diagnose.py" version --json</code> | <code>am version --json</code>
`skills/agent-memory/lifecycle/diagnose/GUIDE.md:48` | <code>python3 -I "$SKILL_DIR/scripts/diagnose.py" status --repo /absolute/repository --json</code> | <code>am status --repo /absolute/repository --json</code>
`skills/agent-memory/lifecycle/diagnose/GUIDE.md:81` | <code>python3 -I "$SKILL_DIR/scripts/diagnose.py" doctor --repo /absolute/repository --json</code> | <code>am doctor --repo /absolute/repository --json</code>
`skills/agent-memory/lifecycle/install/GUIDE.md:108` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" install --check --development-source /path/to/checkout --mode copy --presets none --target-dir /path/to/custom/skills</code> | <code>am install --check --development-source /path/to/checkout --mode copy --presets none --target-dir /path/to/custom/skills</code>
`skills/agent-memory/lifecycle/install/GUIDE.md:126` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" install --development-source /path/to/checkout --mode copy --presets none --target-dir /path/to/custom/skills</code> | <code>am install --development-source /path/to/checkout --mode copy --presets none --target-dir /path/to/custom/skills</code>
`skills/agent-memory/lifecycle/install/GUIDE.md:178` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" recover</code> | <code>am recover</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:40` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" preview --repo REPO --to agent-memory/v1 &gt; /private/path/migration-preview.json</code> | <code>am migrate preview --repo REPO --to agent-memory/v1 &gt; /private/path/migration-preview.json</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:64` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" apply --proposal /private/path/migration-preview.json</code> | <code>am migrate apply --proposal /private/path/migration-preview.json</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:100` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" recover --repo REPO</code> | <code>am migrate recover --repo REPO</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:101` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" recover-lock --repo REPO</code> | <code>am migrate recover-lock --repo REPO</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:114` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" recover-lock --repo REPO --proposal /private/path/dead-lock.json</code> | <code>am migrate recover-lock --repo REPO --proposal /private/path/dead-lock.json</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:120` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" recover --repo REPO --operation OPERATION_UUID --mode finish &gt; /private/path/migration-recovery.json</code> | <code>am migrate recover --repo REPO --operation OPERATION_UUID --mode finish &gt; /private/path/migration-recovery.json</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:121` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" recover --repo REPO --mode finish --proposal /private/path/migration-recovery.json</code> | <code>am migrate recover --repo REPO --mode finish --proposal /private/path/migration-recovery.json</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:145` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" restore --repo REPO --operation ORIGINAL_UUID &gt; /private/path/restoration.json</code> | <code>am migrate restore --repo REPO --operation ORIGINAL_UUID &gt; /private/path/restoration.json</code>
`skills/agent-memory/lifecycle/migrate/GUIDE.md:146` | <code>python3 -I "$SKILL_DIR/scripts/migrate_bank.py" restore --repo REPO --proposal /private/path/restoration.json</code> | <code>am migrate restore --repo REPO --proposal /private/path/restoration.json</code>
`skills/agent-memory/lifecycle/purge/GUIDE.md:53` | <code>python3 -I "$SKILL_DIR/scripts/uninstall_runtime.py" purge-data --repo /absolute/repository --path /absolute/repository/memory-bank --check &gt; /private/path/purge-preview.json</code> | <code>am purge-data --repo /absolute/repository --path /absolute/repository/memory-bank --check &gt; /private/path/purge-preview.json</code>
`skills/agent-memory/lifecycle/purge/GUIDE.md:70` | <code>python3 -I "$SKILL_DIR/scripts/uninstall_runtime.py" purge-data --repo /absolute/repository --path /absolute/repository/memory-bank --confirm CONFIRMED_PREVIEW_SHA256 &gt; /private/path/purge-receipt.json</code> | <code>am purge-data --repo /absolute/repository --path /absolute/repository/memory-bank --confirm CONFIRMED_PREVIEW_SHA256 &gt; /private/path/purge-receipt.json</code>
`skills/agent-memory/lifecycle/purge/GUIDE.md:115` | <code>installer.py recover</code> | <code>am recover</code>
`skills/agent-memory/lifecycle/uninstall/GUIDE.md:75` | <code>python3 -I "$SKILL_DIR/scripts/uninstall_runtime.py" uninstall --check &gt; /private/path/uninstall-plan.json</code> | <code>am uninstall --check &gt; /private/path/uninstall-plan.json</code>
`skills/agent-memory/lifecycle/uninstall/GUIDE.md:89` | <code>python3 -I "$SKILL_DIR/scripts/uninstall_runtime.py" uninstall &gt; /private/path/uninstall-receipt.json</code> | <code>am uninstall &gt; /private/path/uninstall-receipt.json</code>
`skills/agent-memory/lifecycle/uninstall/GUIDE.md:137` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" recover</code> | <code>am recover</code>
`skills/agent-memory/lifecycle/update/GUIDE.md:49` | <code>python3 -I "$SKILL_DIR/scripts/update_runtime.py" update --check &gt; /private/path/update-check.json</code> | <code>am update --check &gt; /private/path/update-check.json</code>
`skills/agent-memory/lifecycle/update/GUIDE.md:97` | <code>python3 -I "$SKILL_DIR/scripts/update_runtime.py" update &gt; /private/path/update-receipt.json</code> | <code>am update &gt; /private/path/update-receipt.json</code>
`skills/agent-memory/maintain/artifact/GUIDE.md:50` | <code>python3 -I "$SKILL_DIR/scripts/artifact.py" inspect --pack /path/to/complete/package &gt; /private/path/artifact.json</code> | <code>am artifact inspect --pack /path/to/complete/package &gt; /private/path/artifact.json</code>
`skills/agent-memory/maintain/cli/GUIDE.md:45` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" --help</code> | <code>am --help</code>
`skills/agent-memory/maintain/cli/GUIDE.md:46` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" identity status --repo REPO --json</code> | <code>am identity status --repo REPO --json</code>
`skills/agent-memory/maintain/cli/GUIDE.md:47` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" init inspect --repo REPO</code> | <code>am init inspect --repo REPO</code>
`skills/agent-memory/maintain/cli/GUIDE.md:48` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" search --repo REPO --request RECALL_JSON</code> | <code>am search --repo REPO --request RECALL_JSON</code>
`skills/agent-memory/maintain/cli/GUIDE.md:49` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" checkpoint inspect --repo REPO</code> | <code>am checkpoint inspect --repo REPO</code>
`skills/agent-memory/maintain/cli/GUIDE.md:50` | <code>python3 -I "$SKILL_DIR/scripts/installer.py" boundary inspect --repo REPO</code> | <code>am boundary inspect --repo REPO</code>
`skills/agent-memory/maintain/cli/GUIDE.md:78` | <code>developer_identity.py</code> | <code>am developer</code>
`skills/agent-memory/maintain/cli/GUIDE.md:78` | <code>project_identity.py</code> | <code>am project</code>
`skills/agent-memory/maintain/cli/GUIDE.md:78` | <code>scope_binding.py</code> | <code>am identity</code>
`skills/agent-memory/maintain/cli/GUIDE.md:79` | <code>initialize.py</code> | <code>am init</code>
`skills/agent-memory/maintain/cli/GUIDE.md:79` | <code>bank_state.py inspect</code> | <code>am inspect</code>
`skills/agent-memory/maintain/cli/GUIDE.md:79` | <code>bank_contract.py</code> | <code>am bank</code>
`skills/agent-memory/maintain/cli/GUIDE.md:80` | <code>recover_layout.py</code> | <code>am bank recover-layout</code>
`skills/agent-memory/maintain/cli/GUIDE.md:81` | <code>retrieve.py</code> | <code>am search</code>
`skills/agent-memory/maintain/cli/GUIDE.md:82` | <code>discover_memory.py</code> | <code>am discover</code>
`skills/agent-memory/maintain/cli/GUIDE.md:83` | <code>assemble_context.py</code> | <code>am context</code>
`skills/agent-memory/maintain/cli/GUIDE.md:84` | <code>admit_learning.py capture</code> | <code>am capture</code>
`skills/agent-memory/maintain/cli/GUIDE.md:85` | <code>admit_learning.py</code> | <code>am admission</code>
`skills/agent-memory/maintain/cli/GUIDE.md:85` | <code>compare_learning.py</code> | <code>am compare</code>
`skills/agent-memory/maintain/cli/GUIDE.md:86` | <code>consolidate_learning.py</code> | <code>am compact</code>
`skills/agent-memory/maintain/cli/GUIDE.md:86` | <code>checkpoint.py</code> | <code>am checkpoint</code>
`skills/agent-memory/maintain/cli/GUIDE.md:87` | <code>govern_memory.py</code> | <code>am govern</code>
`skills/agent-memory/maintain/cli/GUIDE.md:88` | <code>reconcile_memory.py</code> | <code>am semantic</code>
`skills/agent-memory/maintain/cli/GUIDE.md:88` | <code>publish_memory.py</code> | <code>am publish</code>
`skills/agent-memory/maintain/cli/GUIDE.md:89` | <code>refresh_memory.py</code> | <code>am refresh</code>
`skills/agent-memory/maintain/cli/GUIDE.md:90` | <code>omp_boundary.py</code> | <code>am omp-boundary</code>
`skills/agent-memory/maintain/cli/GUIDE.md:91` | <code>sharing_boundary.py</code> | <code>am sharing</code>
`skills/agent-memory/maintain/cli/GUIDE.md:91` | <code>sharing_recovery.py</code> | <code>am exchange</code>
`skills/agent-memory/maintain/cli/GUIDE.md:92` | <code>migrate_bank.py</code> | <code>am migrate</code>
`skills/agent-memory/maintain/cli/GUIDE.md:93` | <code>projection.py</code> | <code>am projection</code>
`skills/agent-memory/maintain/cli/GUIDE.md:93` | <code>bank_writes.py</code> | <code>am writes</code>
`skills/agent-memory/maintain/cli/GUIDE.md:93` | <code>bank_transactions.py</code> | <code>am transaction</code>
`skills/agent-memory/maintain/pack/GUIDE.md:63` | <code>python3 -I "$SKILL_DIR/scripts/validate_pack.py" --pack /path/to/candidate &gt; /private/path/pack-check.json</code> | <code>am validate-pack --pack /path/to/candidate &gt; /private/path/pack-check.json</code>
`skills/agent-memory/offline/GUIDE.md:85` | <code>python3 -I "$SKILL_DIR/scripts/bank_contract.py" validate --repo /path/to/repository</code> | <code>am bank validate --repo /path/to/repository</code>
`skills/agent-memory/offline/GUIDE.md:86` | <code>python3 -I "$SKILL_DIR/scripts/bank_writes.py" inspect --repo /path/to/repository</code> | <code>am writes inspect --repo /path/to/repository</code>
`skills/agent-memory/offline/GUIDE.md:119` | <code>python3 -I "$SKILL_DIR/scripts/bank_contract.py" render --input /tmp/router-proposal.json</code> | <code>am bank render --input /tmp/router-proposal.json</code>
`skills/agent-memory/projections/GUIDE.md:44` | <code>python3 -I "$SKILL_DIR/scripts/projection.py" build --repo /path/to/repo --input /private/path/references.json</code> | <code>am projection build --repo /path/to/repo --input /private/path/references.json</code>
`skills/agent-memory/projections/GUIDE.md:49` | <code>python3 -I "$SKILL_DIR/scripts/projection.py" check --repo /path/to/repo --input /private/path/projection.json</code> | <code>am projection check --repo /path/to/repo --input /private/path/projection.json</code>
`skills/agent-memory/records/GUIDE.md:128` | <code>python3 -I "$SKILL_DIR/scripts/record_contract.py" model</code> | <code>am record-contract model</code>
`skills/agent-memory/records/GUIDE.md:129` | <code>python3 -I "$SKILL_DIR/scripts/record_contract.py" render --input /tmp/record-proposal.json</code> | <code>am record-contract render --input /tmp/record-proposal.json</code>
`skills/agent-memory/records/GUIDE.md:130` | <code>python3 -I "$SKILL_DIR/scripts/record_contract.py" validate --file memory-bank/topics/example.md --require redaction</code> | <code>am record-contract validate --file memory-bank/topics/example.md --require redaction</code>
`skills/agent-memory/records/GUIDE.md:131` | <code>python3 -I "$SKILL_DIR/scripts/record_contract.py" applicability --file memory-bank/topics/example.md --input /tmp/record-context.json</code> | <code>am record-contract applicability --file memory-bank/topics/example.md --input /tmp/record-context.json</code>
`skills/agent-memory/resume/GUIDE.md:58` | <code>python3 -I "$SKILL_DIR/scripts/retrieve.py" --repo /path/to/repo --request /private/path/task-recall.json</code> | <code>am search --repo /path/to/repo --request /private/path/task-recall.json</code>
`skills/agent-memory/retrieval/GUIDE.md:56` | <code>python3 -I "$SKILL_DIR/scripts/retrieve.py" --repo /path/to/repo --request /private/path/recall.json</code> | <code>am search --repo /path/to/repo --request /private/path/recall.json</code>
`skills/agent-memory/scope/GUIDE.md:50` | <code>python3 -I "$SKILL_DIR/scripts/scope_policy.py" model</code> | <code>am scope-policy model</code>
`skills/agent-memory/scope/GUIDE.md:51` | <code>python3 -I "$SKILL_DIR/scripts/scope_policy.py" decide --input /tmp/scope-request.json</code> | <code>am scope-policy decide --input /tmp/scope-request.json</code>
`skills/agent-memory/security/GUIDE.md:63` | <code>python3 -I "$SKILL_DIR/scripts/security_policy.py" --repo /path/to/repo --request /private/path/security-request.json</code> | <code>am security-policy --repo /path/to/repo --request /private/path/security-request.json</code>
`skills/agent-memory/security/GUIDE.md:63` | <code>python3 -I "$SKILL_DIR/scripts/security_policy.py" --repo /path/to/repo --request /private/path/procedure-request.json --operator-approval /private/path/operator-approval.json</code> | <code>am security-policy --repo /path/to/repo --request /private/path/procedure-request.json --operator-approval /private/path/operator-approval.json</code>
`skills/agent-memory/sharing/boundary/GUIDE.md:89` | <code>python3 -I "$SKILL_DIR/scripts/sharing_boundary.py" --repo /path/to/repo</code> | <code>am sharing --repo /path/to/repo</code>
`skills/agent-memory/sharing/publication/GUIDE.md:80` | <code>python3 -I "$SKILL_DIR/scripts/publish_memory.py" prepare --v2 --repo /path/to/repo --input /private/semantic-proof.json --output /private/prepared-publication.json</code> | <code>am publish prepare --v2 --repo /path/to/repo --input /private/semantic-proof.json --output /private/prepared-publication.json</code>
`skills/agent-memory/sharing/publication/GUIDE.md:106` | <code>python3 -I "$SKILL_DIR/scripts/publish_memory.py" land --v2 --repo /path/to/repo --input /private/publication-request.json --output /private/landed-publication.json</code> | <code>am publish land --v2 --repo /path/to/repo --input /private/publication-request.json --output /private/landed-publication.json</code>
`skills/agent-memory/sharing/recovery/GUIDE.md:65` | <code>python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" inspect --repo /path/to/repo</code> | <code>am exchange inspect --repo /path/to/repo</code>
`skills/agent-memory/sharing/recovery/GUIDE.md:66` | <code>python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" inspect --repo /path/to/repo --fetch</code> | <code>am exchange inspect --repo /path/to/repo --fetch</code>
`skills/agent-memory/sharing/recovery/GUIDE.md:102` | <code>python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" replay --repo /path/to/repo --request /private/replay.json</code> | <code>am exchange replay --repo /path/to/repo --request /private/replay.json</code>
`skills/agent-memory/sharing/recovery/GUIDE.md:108` | <code>python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" reconcile --repo /path/to/repo</code> | <code>am exchange reconcile --repo /path/to/repo</code>
`skills/agent-memory/sharing/recovery/GUIDE.md:109` | <code>python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" reconcile --repo /path/to/repo --base FULL_SHA</code> | <code>am exchange reconcile --repo /path/to/repo --base FULL_SHA</code>
`skills/agent-memory/sharing/refresh/GUIDE.md:93` | <code>python3 -I "$SKILL_DIR/scripts/refresh_memory.py" inspect --v2 --repo /path/to/repo --output /private/refresh-inspection.json</code> | <code>am refresh inspect --v2 --repo /path/to/repo --output /private/refresh-inspection.json</code>
`skills/agent-memory/sharing/refresh/GUIDE.md:94` | <code>python3 -I "$SKILL_DIR/scripts/refresh_memory.py" inspect --v2 --repo /path/to/repo --base FULL_SHARED_COMMIT --output /private/explicit-refresh-inspection.json</code> | <code>am refresh inspect --v2 --repo /path/to/repo --base FULL_SHARED_COMMIT --output /private/explicit-refresh-inspection.json</code>
`skills/agent-memory/sharing/refresh/GUIDE.md:123` | <code>python3 -I "$SKILL_DIR/scripts/refresh_memory.py" prepare --v2 --repo /path/to/repo --input /private/refresh-input.json --output /private/prepared-refresh.json</code> | <code>am refresh prepare --v2 --repo /path/to/repo --input /private/refresh-input.json --output /private/prepared-refresh.json</code>
`skills/agent-memory/sharing/refresh/GUIDE.md:150` | <code>python3 -I "$SKILL_DIR/scripts/refresh_memory.py" apply --v2 --input /private/prepared-refresh.json --output /private/applied-refresh.json</code> | <code>am refresh apply --v2 --input /private/prepared-refresh.json --output /private/applied-refresh.json</code>
`skills/agent-memory/sharing/semantic/GUIDE.md:82` | <code>python3 -I "$SKILL_DIR/scripts/reconcile_memory.py" inspect-v2 --repo /path/to/repo --output /private/initial-inspection.json</code> | <code>am semantic inspect-v2 --repo /path/to/repo --output /private/initial-inspection.json</code>
`skills/agent-memory/sharing/semantic/GUIDE.md:83` | <code>python3 -I "$SKILL_DIR/scripts/reconcile_memory.py" inspect-v2 --repo /path/to/repo --input /private/selection.json --output /private/selected-inspection.json --pages-dir /private/selected-pages</code> | <code>am semantic inspect-v2 --repo /path/to/repo --input /private/selection.json --output /private/selected-inspection.json --pages-dir /private/selected-pages</code>
`skills/agent-memory/sharing/semantic/GUIDE.md:107` | <code>python3 -I "$SKILL_DIR/scripts/reconcile_memory.py" page-v2 --repo /path/to/repo --input /private/selected-inspection.json --page 0</code> | <code>am semantic page-v2 --repo /path/to/repo --input /private/selected-inspection.json --page 0</code>
`skills/agent-memory/sharing/semantic/GUIDE.md:120` | <code>python3 -I "$SKILL_DIR/scripts/reconcile_memory.py" review-indexed-v2 --repo REPO --inspection SELECTED --input PRIVATE_DECISIONS --output PRIVATE_REVIEWED</code> | <code>am semantic review-indexed-v2 --repo REPO --inspection SELECTED --input PRIVATE_DECISIONS --output PRIVATE_REVIEWED</code>
`skills/agent-memory/sharing/semantic/GUIDE.md:250` | <code>govern_memory.py</code> | <code>am govern</code>
`skills/agent-memory/sharing/semantic/GUIDE.md:265` | <code>python3 -I "$SKILL_DIR/scripts/reconcile_memory.py" review-v2 --repo /path/to/repo --input /private/review.json --output /private/reviewed.json</code> | <code>am semantic review-v2 --repo /path/to/repo --input /private/review.json --output /private/reviewed.json</code>
`skills/agent-memory/sharing/semantic/GUIDE.md:266` | <code>python3 -I "$SKILL_DIR/scripts/reconcile_memory.py" check --v2 --repo /path/to/repo --input /private/proof.json --output /private/checked.json</code> | <code>am semantic check --v2 --repo /path/to/repo --input /private/proof.json --output /private/checked.json</code>
`skills/agent-memory/sharing/semantic/GUIDE.md:378` | <code>python3 -I "$SKILL_DIR/scripts/reconcile_memory.py" recover-holds --repo /path/to/repo --project-id VERIFIED_PROJECT_UUID</code> | <code>am semantic recover-holds --repo /path/to/repo --project-id VERIFIED_PROJECT_UUID</code>
`skills/agent-memory/turn/GUIDE.md:188` | <code>python3 -I "$SKILL_DIR/scripts/admit_learning.py" draft --repo REPO --input PRIVATE_JSON</code> | <code>am admission draft --repo REPO --input PRIVATE_JSON</code>
`skills/agent-memory/writes/GUIDE.md:61` | <code>python3 -I "$SKILL_DIR/scripts/bank_writes.py" inspect --repo /path/to/repo</code> | <code>am writes inspect --repo /path/to/repo</code>
`skills/agent-memory/writes/GUIDE.md:62` | <code>python3 -I "$SKILL_DIR/scripts/bank_writes.py" preview --repo /path/to/repo --path activeContext.md --content /private/path/reviewed.md &gt; /private/path/proposal.json</code> | <code>am writes preview --repo /path/to/repo --path activeContext.md --content /private/path/reviewed.md &gt; /private/path/proposal.json</code>
`skills/agent-memory/writes/GUIDE.md:64` | <code>python3 -I "$SKILL_DIR/scripts/bank_writes.py" apply --proposal /private/path/proposal.json</code> | <code>am writes apply --proposal /private/path/proposal.json</code>
`skills/agent-memory/writes/GUIDE.md:70` | <code>python3 -I "$SKILL_DIR/scripts/bank_transactions.py" preview --repo /path/to/repo --files /private/path/complete-reviewed-files.json --operation replace &gt; /private/path/transaction.json</code> | <code>am transaction preview --repo /path/to/repo --files /private/path/complete-reviewed-files.json --operation replace &gt; /private/path/transaction.json</code>
`skills/agent-memory/writes/GUIDE.md:72` | <code>python3 -I "$SKILL_DIR/scripts/bank_transactions.py" apply --proposal /private/path/transaction.json</code> | <code>am transaction apply --proposal /private/path/transaction.json</code>
`skills/agent-memory/writes/GUIDE.md:112` | <code>python3 -I "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo &gt; /private/path/unlock.json</code> | <code>am writes recover --repo /path/to/repo &gt; /private/path/unlock.json</code>
`skills/agent-memory/writes/GUIDE.md:113` | <code>python3 -I "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo --proposal /private/path/unlock.json</code> | <code>am writes recover --repo /path/to/repo --proposal /private/path/unlock.json</code>
`skills/agent-memory/writes/GUIDE.md:119` | <code>bank_transactions.py recover --repo ... --mode finish</code> | <code>am transaction recover --repo ... --mode finish</code>
