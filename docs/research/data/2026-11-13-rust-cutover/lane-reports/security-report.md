# Security lane report

Base: `f7391170a0774f1589218025450346ff7e356f23`; worktree `/opt/agent-memory-rust-security`, branch `lane/security`.

## Delivery / verification boundary

The complete `security.py` function set is implemented natively: bounded memoized scan, immutable metadata-only audit, mutation/refresh checks, actual repository evidence, exact operator approval, guarded evaluation and typed CLI output. The implementation worker executed no Python, build, test, formatter, linter or smoke command. Parent verification is recorded below; the owner independently captured the CPython baseline outside this session. No expected output was invented.

All baseline security functions are implemented within their owned module boundaries. Successful public evaluation additionally requires the genuine, unported `recovery_frontier` provider through canonical tombstone history; six public evaluator contract tests are dependency-gated rather than supplied a fake observer. Migration `bank_location` auditing is a mandatory typed pending interface because the migration locator/locations implementer is absent at the frozen base; ordinary audit paths are implemented, not pending.

## Exact source changes

- `Cargo.toml`: owner-approved `regex = { version = "=1.12.2", default-features = false, features = ["std", "perf"] }`.
- `Cargo.lock`: resolved the new pure-Rust regex dependency closure; existing package versions unchanged from the frozen lock.
- `src/security.rs` (new): complete domain, compiled-once bounded scanner, all three production trait implementations, pending migration interface, CLI envelope.
- `src/lib.rs`: one `pub mod security;` declaration.
- `src/model_reference.rs`: deleted temporary public `CLASSES` definition and migration comment; imports the sole `security::CLASSES` directly. No alias/re-export/shim.
- `src/cli/helpers/security.rs` (new): typed `--repo`, `--request`, `--operator-approval` parser/handler, bounded strict JSON input.
- `src/cli/helpers/mod.rs`: one security module declaration.
- `src/cli/table.rs`: one `security-policy` route at its baseline helper position.
- `src/cli/mod.rs`: registers real security for bank writes, repository config and sharing hold mapping. The existing sharing registry is single-assignment, so its composition registration uses a `LazyLock<Result<(), String>>`; repeated composition preserves success or the original error, never suppresses an error, replaces an existing provider, or panics. The two existing replaceable registries keep their existing semantics.
- `tests/wiring-pending.txt`: removed all three implemented security pending lines; added only `security::register_migration_audit waits for migration`.
- `tests/t1_security.rs` (new): filesystem/process-isolated production security and consumer-visible CLI regressions; additional workflow tests retain explicit genuine dependency reasons.
- `tests/t1_a1.rs`: moves the classification assertion to `security::CLASSES`.
- `tests/t1_project_identity.rs`: deletes the approximate `LabelScan`; existing fixture registration uses actual `security::Security`.
- `tests/t1_layout_recovery_state.rs`: deletes permissive `FixtureLabels`; existing fixture registration uses actual `security::Security`.
- `tests/t1_bank_transactions.rs`: enables complete-receipt regression and registers actual repository scanner; removes resolved security from the remaining recovery-frontier ignore reason. Parent additionally corrected the receipt assertion to compare `Option` values of canonical encoded bytes rather than insertion-order-sensitive `Value` objects, retaining the complete schema and every mutated-binding refusal.
- `tests/t1_scope_binding.rs`: enables the initialization-only create/replay test, whose remaining blocker was security; five existing-bank binding scenarios retain a precise real recovery-frontier dependency; other multi-module ignores name their actual remaining providers.
- `tests/t1_private_state.rs`: removes resolved security/scope entries and names actual remaining private-state/frontier/source-barrier providers.
- `tests/vectors/security-regex-v1.json` (new, parent): unchanged independently captured 31-case CPython result, including full sequential outputs and per-stage match spans.
- `tests/obligations/python-baseline-v1.json` (parent): two complete primitive obligations ported; eleven genuine dependency-gated obligations partial.
- This report.

### All three existing seam files (explicit zero-change accounting)

- `src/repository_config.rs`: **no source edits**; existing `SecurityScan::{has_reasons(&str), scan(&str)}` is implemented by `security::Security` and registered at composition.
- `src/bank_writes.rs`: **no source edits**; existing `Security::{guarded_start, audit, guarded_start_transaction}` is implemented by the same production provider; the transaction adapter preserves `complete_refresh`.
- `src/sharing_hold_mapping.rs`: **no source edits**; existing `SecurityScan::has_reasons(&Value)` is implemented by the same production provider. No scanner substitute or truthy/non-string normalizer.

Keeping the three existing signatures avoids a broad registry/caller cutover: the two scan traits intentionally accept different types and the writer trait returns a different guarded-start shape. One stateless real provider is the explicitly approved alternative to a new common trait.

## Public API and callers

`security::{CLASSES, CATEGORIES, MIGRATION_ACTIONS, ACTIONS, LIMIT, REVIEW_LIMIT, AUDIT_LIMIT, REQUEST_LIMIT, REQUEST, REASONS}`; `scan(&Value, maximum) -> Result<Value>` accepts only 16384 or 65536; `audit(repo, operation_id, category, phase, context, source_classes, reasons, record_ids, action, bank_location) -> Result<Value>`; `content_reasons`, `ensure_safe`, `complete_refresh_reasons`, `guarded_start(..., complete_refresh) -> Result<Value>`; `repository_source`, `approval_value`, `evaluate(..., approval: Option<&Value>, record_audit: bool)`, `command_result`, and stateless `Security`.

`ACTIONS` derives its set from the existing writer operations and migration/fixed actions rather than duplicating the writer's definition. The only Rust consumers of the former exported `model_reference::CLASSES` were its own renderer and the `t1_a1` assertion; both now use `security::CLASSES`. Historical Python and unrelated comparison/score classifications are untouched.

New pending interface:

```rust
pub trait MigrationAudit: Send + Sync {
    fn validate_location(&self, root: &Path, bank: &Path, operation_id: &str) -> Result<()>;
}
pub fn register_migration_audit(provider: Option<Arc<dyn MigrationAudit>>);
```

The migration implementer MUST establish `locate(root) == (root, bank)` and run its actual `locations(root, bank, operation_id)` checks. A migration-only bank-location call with no registration refuses `Migration audit provider is not registered`; non-migration action plus explicit bank refuses the baseline migration-locator message. No guessed locator was added.

## Safety, bounds and durable behavior

- Source and review scan budgets remain 16/64 KiB of UTF-8 bytes, nonempty/useful text and baseline control-separator refusal. The scanner initializes only after the bounded input check.
- All nine detection patterns and three fixed-length leading-prefix searches compile once in `LazyLock`; both compiled regex and lazy DFA budgets are 4 MiB. Credential, token and authorization leading boundaries are rejected using their bounded prefixes before any unbounded greedy value/body search. Unchanged intermediate redaction stages borrow their input via `Cow`; actual source text is never normalized or casefolded.
- Small mutation batches remain at most 64 string bodies / 64 KiB aggregate, each scanned at 16 KiB. Complete refresh uses the real complete-bank count/per-file/aggregate checks and real route validation, then scans separately bounded `path + '\n' + body` under the 64 KiB review budget. Only action `refresh` may select that path.
- Repository evidence has the exact two-field schema, lowercase SHA256, canonical local relative path, no hidden components, no memory-bank subtree, no source symlink/ancestor symlink, actual explicit Git inventory membership and actual bounded ordinary-file digest. The original inventory/byte observation is repeated before successful evaluation output. Git uses existing foundation ownership, finite 10-second deadline and bounded output.
- Scope is independently required, tombstone history and complete source hashes are freshly read, then scope and deletion/source hashes are rechecked. A classification/UUID/approval label never authenticates anyone. Conversation/suspicious/deleted text is not echoed. Exact separate approval uses project, content digest, integer source version >=1, explicit approved decision, useful single-line ref; JSON null approval has the baseline absent-approval meaning.
- Audit accepts only content-free fixed metadata and canonical UUIDs. `developer_identity::encoded` preserves sorted indent-2 UTF-8 durable bytes/trailing newline. Event names preserve operation/action/phase/developer identity. No source text, redacted body or secret hash enters audit records.
- Existing private directory creation, pinned no-follow parents/files, exact mode 0600 for new events, full file fsync, exclusive complete-event publication, parent fsync, collision comparison and final bounded readback are mandatory. Same-byte retries succeed; any differing prior destination is preserved and refused. Cleanup on a refusal cannot replace the original refusal.

### Declared deltas

1. The approved immutable publication mechanism is `fs::rename_at(..., NoReplace)` instead of Python `link(temp, destination)` plus unlink. Source inspection confirms native `RENAME_EXCL` on macOS and `renameat2(RENAME_NOREPLACE)` on Linux, with no emulation/overwrite fallback. Complete bytes, destination exclusivity, file/parent synchronization, identical retry and retained collision evidence are preserved. A successful publish removes the temporary name atomically rather than in a second namespace operation. No new unsafe/foundation code.
2. Previously unbounded CLI request/approval reads now use explicit `REQUEST_LIMIT = 1 MiB`; audit existing/readback bytes and encoded metadata use `AUDIT_LIMIT = 64 KiB`. Oversize data is refused without truncation, repair or canonical writes.
3. Foundation-pinned reads/rechecks may refuse concurrent namespace/source changes the baseline path-based reads could race through. No stale bytes are silently accepted. Foundation UTF-8-only JSON, duplicate-key policy, surrogate/depth refusals remain the shared documented codec deltas.
4. Argparse formatting/interpreter launch aliases are not carried over; the declared native command and typed JSON outcomes are.

No intentional stricter detector/redactor policy is introduced; any observed regex mismatch is a defect to resolve against captured baseline vectors, not a silent policy strengthening.

## Exact nine-pattern comparison

Baseline source `skills/agent-memory/scripts/agent_memory/security.py`, SHA256 `768a91fad491ed3996defae434a41b983c2aa42dacc4aede5ab3009226a5c5d8`.

Rust templates below are exact symbolic expressions constructed in `security::patterns`; they are not a second runtime definition. Substitutions are defined once here:

- `I(literal)` expands each ASCII letter to `[lowerUPPER]`, adding `İı` to I/i, `ſ` to S/s, `K` to K/k. Other literal characters are retained. There is no full fold (`ß` does not become `ss`).
- `S = [\x09-\x0d\x1c-\x20\x{85}\x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}]`; `s` means the same class contents without brackets.
- `W` is underscore plus exactly the source-defined baseline Unicode 15.1 `unicode::ALNUM` ranges. `N = [^W]`. This excludes combining marks, non-underscore connector punctuation and joiners from Python word membership.
- `G = (?:N|N(?s:.{0,62})N)` is exactly a 1..64-scalar dotall separator whose first/last scalars satisfy the two inner word boundaries; the 1-scalar alternative does not charge twice.
- `E = (?:\z|N)` consumes the final nonword only for suspicious detection; these matches are never used as replacement spans.
- `B(start)` / `B(end)` are source-offset word-boundary checks using the baseline table. Credential/token/authorization use bounded leading-prefix searches to check `B(start)` before their anchored greedy full pattern. Credential's prefix uses the captured label start after any optional quote, and the full key/separator syntax implies `B(end)`. Every rejection restarts after its starting scalar, not after the prefix or entire rejected span, preserving inner/overlapping matches. `sk-` alone has a hyphen-containing repeated body: its greedy end is backtracked one ASCII scalar at a time to the latest valid terminal Python boundary with >=20 body scalars, just as the original terminal `\b` demands.

### 1. private-key

Python:

```text
-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----[\s\S]*?(?:-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|\Z)
```

Rust:

```text
-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----(?s:.*?)(?:-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|\z)
```

Difference: dotall equivalent replaces `[\s\S]`; Python strict end `\Z` is Rust strict end `\z`. No case-insensitive flag, no Unicode-range expansion.

### 2. token

Python:

```text
\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}|AKIA[A-Z0-9]{16})\b
```

Rust:

```text
\A(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}|AKIA[A-Z0-9]{16})
```

Difference: leading/trailing `\b` are explicit `B(start)` / `B(end)` checks, including greedy `sk-` backtracking described above. The bounded prefix is `(?:gh[pousr]_|github_pat_|sk-|AKIA)`; leading-boundary rejection happens before the anchored full pattern runs on a borrowed suffix at that source position. `\A` prevents any subsequent forward search by that full match. Token alphabet is deliberately ASCII and case-sensitive, exactly as the original.

### 3. credential

Python:

```text
(?i)["']?\b(?:password|passwd|token|secret|api[_-]?(?:key|token)|access[_-]?token|client[_-]?secret|secret[_-]?key|aws_secret_access_key)\b["']?\s*[:=]\s*["']?[^\s"'`,;}\]]{6,}
```

Rust:

```text
\A["']?(?P<label>I(password)|I(passwd)|I(token)|I(secret)|I(api)[_-]?(?:I(key)|I(token))|I(access)[_-]?I(token)|I(client)[_-]?I(secret)|I(secret)[_-]?I(key)|I(aws_secret_access_key))["']?S*[:=]S*["']?[^s"'`,;}\]]{6,}
```

Difference: exact `I` and `S` replace implicit engine case/whitespace definitions. A fixed-length `["']?(?P<label>keys)` prefix (the same exact key alternatives above) checks `B(label.start)` before the anchored full match scans any value; the full key/separator syntax implies the label's end boundary. Optional opening quotes remain in the actual redaction span, including when preceded by a word. Shorter prefix alternatives do not exclude longer valid full-key alternatives. Rejected prefixes restart after their starting scalar, so `xapi-token=abcdef` still discovers the inner valid `token` after `-`. Greedy value matching and closing-quote/bracket behavior remain unchanged. Sequential token replacement precedes this pattern, including baseline redaction-marker rescanning.

### 4. authorization-header

Python:

```text
(?im)\bAuthorization[ \t]*:[ \t]*(?:Bearer|Basic)[ \t]+[A-Za-z0-9._~+/=-]{6,}
```

Rust:

```text
\AI(Authorization)[ \t]*:[ \t]*(?:I(Bearer)|I(Basic))[ \t]+[A-Za-zİıſK0-9._~+/=-]{6,}
```

Difference: bounded prefix `I(Authorization)` checks leading `B(start)` before the anchored full body search; explicit Python simple-case expansion applies both to literals and the otherwise-ASCII letter class. Multiline flag has no effect without anchors and is omitted. Horizontal spacing is still ASCII space/tab, not generic Unicode whitespace.

### 5. url-credential

Python:

```text
(?i)(?:https?|postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|amqps?)://[^/\s:@]+:[^@\s/]+@
```

Rust:

```text
(?:I(http)|I(https)|I(postgres)(?:I(ql))?|I(mysql)|I(mongodb)(?:\+I(srv))?|I(redis)|I(amqp)|I(amqps))://[^/s:@]+:[^@s/]+@
```

Difference: equivalent scheme alternations, explicit simple case and whitespace expansion, no authority/URL normalization and no newly parsed URL semantics. The greedy user/password classes and terminating `@` remain the baseline regex behavior.

### 6. authority-override

Python:

```text
(?is)\b(?:ignore|disregard|override|bypass)\b.{0,64}\b(?:system|developer|previous|instructions|safety|rules|current\s+(?:request|task)|operator)\b
```

Rust:

```text
(?:I(ignore)|I(disregard)|I(override)|I(bypass))G(?:I(system)|I(developer)|I(previous)|I(instructions)|I(safety)|I(rules)|I(operator)|I(current)S+(?:I(request)|I(task)))E
```

Difference: leading `B(start)`, inner boundary-exact dotall/scalar-budget `G`, explicit simple-case/whitespace and terminal `E`. Alternative placement of `operator` is immaterial to boolean detection. The consumed final nonword is not echoed/redacted and does not alter suspicion classification.

### 7. credential-disclosure

Python:

```text
(?is)\b(?:reveal|exfiltrate|upload|send)\b.{0,64}\b(?:credentials|secrets|tokens|passwords)\b
```

Rust:

```text
(?:I(reveal)|I(exfiltrate)|I(upload)|I(send))G(?:I(credentials)|I(secrets)|I(tokens)|I(passwords))E
```

Difference: same exact boundary/scalar/case handling as authority override. No normalization or broadened verb vocabulary.

### 8. identity-permission-claim

Python:

```text
(?i)\b(?:authenticated|administrator|admin|root)\s*[:=]\s*(?:true|yes|1)\b|\bI am (?:the )?(?:administrator|authenticated operator)\b
```

Rust:

```text
(?:(?:I(authenticated)|I(administrator)|I(admin)|I(root))S*[:=]S*(?:I(true)|I(yes)|1)|I(I am )(?:I(the ))?(?:I(administrator)|I(authenticated operator)))E
```

Difference: common leading `B(start)` and terminal `E` for either original branch; explicit simple case and whitespace. Spaces inside the second branch remain exact literal spaces, not generic whitespace.

### 9. raw-conversation

Python:

```text
(?im)^(?:user|assistant|system|developer)\s*:\s*.*\n(?:[\s\S]*\n)?(?:user|assistant|system|developer)\s*:
```

Rust:

```text
(?m)^(?:I(user)|I(assistant)|I(system)|I(developer))S*:S*.*\n(?:(?s:.*)\n)?(?:I(user)|I(assistant)|I(system)|I(developer))S*:
```

Difference: explicit simple case and whitespace; local dotall for the optional arbitrary intermediate lines, preserving the first line's non-dotall `.*`, multiline line starts and required LF separators. U+0085/U+2028 are whitespace where `\s` occurs but do not become LF anchors.

## Golden baseline capture

Requests prepared before detector completion:

- `/tmp/security-regex-vector-request-v1.json`: original input and exact source commit/path/digest, 24 Unicode/ASCII/boundary/ordered-redaction cases, requests all nine original per-pattern and sequential-stage scalar/UTF-8 spans plus exact full scan/errors.
- `/tmp/security-regex-vector-supplement-v1.json`: seven additional `sk-` greedy-boundary, ASCII-ignorecase character-class and overlap cases. The original request was not rewritten.

The owner supplied `tests/vectors/security-regex-v1.json`: 95,608 unchanged bytes, SHA256 `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`, captured with CPython 3.13.12 / Unicode 15.1.0 from frozen baseline `78eaf70a3cfc7c7e3c9444435715d7d437c242dd`. Its security source SHA256 matches this lane's base exactly. The consumer uses the actual `case.scan.result` / `case.scan.error` envelope and binds the actual captured source commit and hash; metadata and captured outputs were not rewritten. All 31 original/supplemental cases pass on GNU and musl. Original per-pattern and sequential-stage scalar/UTF-8 spans remain committed review evidence. No placeholder, optional skip or self-generated baseline proof is supplied. Runtime public evaluation still needs the genuine recovery-frontier implementer.

## Tests and obligation statuses

`/tmp/security-obligation-ids.txt` contains all 13 annotated security-related baseline IDs. Parent applied these final statuses through `am-dev`:

- `/tmp/security-ported-obligation-ids.txt`: **two ported** primitive obligations, complete-refresh scanning and security memo budget/isolation. Both run successfully on GNU and musl.
- `/tmp/security-partial-obligation-ids.txt`: **eleven partial** contracts: six public evaluator scenarios requiring canonical recovery-frontier observation and five other genuinely dependent workflows. Complete executable bodies remain present; no ignored test is claimed as full proof.

New active test coverage additionally includes exact immutable audit bytes/modes/idempotence/conflict preservation/symlink-directory refusal/repair, bad metadata refusal before I/O, pending migration refusal, both real scanning traits, complete-batch/aggregate/refreshed-path bounds, fixed scan budgets/control separators and sequential redaction safety.

New ignored tests (no provider mocks):

| Test | Exact remaining dependency |
|---|---|
| `source_classes_and_real_repository_inventory` | recovery_frontier for canonical tombstone history |
| `secrets_redact_without_private_audit_payload` | recovery_frontier for canonical tombstone history |
| `hostile_sources_never_echo_or_execute` | recovery_frontier for canonical tombstone history |
| `wrong_project_private_evidence_and_claims_refuse` | recovery_frontier for canonical tombstone history |
| `procedure_approval_is_separate_exact_and_typed` | recovery_frontier for canonical tombstone history |
| `terminal_deletion_suppresses_cached_text` | recovery_frontier for canonical tombstone history |
| `checkpoint_refuses_secrets_before_writes` | checkpoint, recovery_frontier, turn_sources |
| `initialization_refuses_secret_reviewed_summary` | initialization |
| `audit_failure_stops_real_write_until_repair` | recovery_frontier, turn_sources |
| `interrupted_checkpoint_replay_retains_metadata_only` | checkpoint, recovery_frontier, turn_sources |
| `complete_v2_refresh_preserves_feature_state_and_qualifies_last_source` | semantic_sharing, memory_refresh, sharing_recovery, recovery_frontier, turn_sources |

Previously ignored tests enabled with real production registrations:

- `tests/t1_bank_transactions.rs::complete_receipt_schema_and_every_binding_are_required`.
- `tests/t1_scope_binding.rs::create_has_initialization_only_handoff_without_chicken_and_egg`.

The parent-applied assertion correction in `complete_receipt_schema_and_every_binding_are_required` is test-only: `developer_identity::encoded` writes sorted object keys, so a decoded receipt has sorted key order while the pre-encoding fixture retains its construction order. Direct `Value::Object` equality therefore compared representation order, not the receipt contract. Comparing `Option<canonical encoded bytes>` instead preserves absence/presence, all fields and all values under the actual durable codec; it neither drops fields nor weakens any missing/extra/mutated binding refusal. No production receipt behavior changed. Targeted GNU/musl proof and the final proof section remain parent-owned.

Existing genuinely deferred tests retain only remaining providers:

- `t1_bank_transactions::real_partial_rename_converges_finish_and_rollback_at_every_file_boundary`: recovery_frontier.
- Five existing-bank `t1_scope_binding` scenarios (`combined_status_readonly_missing_conflicting_and_known_attribution`, `stale_developer_registry_bank_and_remote_never_overwrite`, `public_status_preserves_whitespace_paths_and_ignores_ambient_git`, `historical_device_scope_link_verifies_current_alias_without_bank_changes`, `conflicting_historical_aliases_allow_explicit_scope_link`): recovery_frontier for the required existing-bank binding snapshot.
- Seven remaining `t1_scope_binding` workflow tests: recovery_frontier and turn_sources (namespace-changing transactions, interruption/recovery, identity-under-lock, mediated writers and private enrollment).
- Six existing `t1_private_state` lifecycle/native absence scenarios: private_state_hooks, recovery_frontier and turn_sources.

No security-only ignore remains. Existing fake security scanners in project/layout tests were removed rather than retained as passing approximations. Other unrelated fixture providers are not changed.

### Canonical observer dependency evidence and observed initial failure

The parent-provided `/tmp/agent-memory-security-proof/initial-security-contracts.log` reports six passing active contracts, six public evaluator failures at the real CLI with `recovery_frontier provider is not registered`, one missing-golden-file failure and five existing ignored workflows. The worker did not rerun that check. The six evaluator scenarios now retain precise genuine dependency annotations; the mandatory golden consumer is unchanged and active.

This is not an accidental write/default-scope choice in the security implementation:

- Rust `security.rs:888-893` independently resolves the ordinary scope, then calls `tombstones::history`; the second complete history check is at `security.rs:964`.
- `tombstones.rs:128-131` calls `bank_writes::snapshot`; `bank_writes.rs:347-349` calls `bank_contract::observe`; production `bank_observation.rs:424-431` unconditionally resolves the real frontier and invokes `observe_owned`. Its source-stability recheck also requires the frontier at `bank_observation.rs:398-403`.
- Exact baseline chain: `security.py:213-214,242-243` calls ordinary identity require and both tombstone-history observations; `tombstones.py:68-70` calls `writes.snapshot`; `bank_writes.py:43-46` calls `bank_observation.observe`; `bank_observation.py:200-205` unconditionally obtains `recovery_frontier.observe`, and `:188-190` checks it again. Omitting these calls, using raw file scan instead, or substituting an empty frontier would remove required held/interrupted recovery coverage.
- The five re-gated scope tests prepare a bind/link over an existing bank. `scope_binding.rs:882-884` requires its canonical snapshot; the apply freshness check at `:1055-1059` also observes it when applicable.
- The create/replay test remains active: initial preview has no bank (`:882` branch false); its first apply has null base files, and replay has an already recorded operation, so `:1055-1059` does not take the snapshot path. `binding_guard :976-1038` uses real project evidence rather than a canonical payload snapshot.
- The complete-receipt test remains active: `bank_transactions.rs:804-836,838-845` validates bounded receipt bytes, proposal/file hashes, settings and expected paths; it does not call a canonical bank observer.

Required implementer for the public evaluator dependency is the already declared `bank_writes::RecoveryFrontier` registration, specifically real `observe_owned` and `check` (plus `verified_hold` when the observed frontier contains a hold). No provider trait, production safety branch or valid baseline outcome was altered to make tests appear green.

### Source-review corrections (unexecuted by this worker)

- P2: credential, token and authorization patterns now perform a compiled bounded-prefix search and original-source leading-boundary check before their `\A`-anchored full pattern runs against a borrowed suffix. This removes repeated greedy-to-end work on rejected `xpassword=...`, `xsk-...` and header-prefix candidates. Prefix rejection advances only one source scalar from its own start; there is no non-overlapping enumeration, skipped rejected span, text copy or normalization. The existing ordered redaction loop and terminal `sk-` greedy boundary backtracking remain intact.
- Active deterministic regressions `dense_rejected_credential_prefixes_keep_later_and_overlapping_matches` and `dense_invalid_token_and_header_prefixes_keep_later_positive_matches` exercise dense rejected candidates at both fixed budgets, later genuine positives, optional opening quotes, longer key alternatives and the overlapping `xapi-token=abcdef` case. They assert real scan/provider outputs, not elapsed time.
- P3: CLI `--operator-approval ""` is filtered to absent before opening an approval file, matching Python's conditional. Other option values, including whitespace-only paths, retain their existing behavior. An active real-CLI regression compares absent versus empty approval before request validation; the dependency-gated public source-class contract additionally exercises empty approval with a valid request.
- The reviewer independently agreed that public-status fixture setup is the fifth genuine scope frontier gate: `tests/t1_scope_binding.rs:1062` creates the bank, `:1064` invokes `w.bind`, and helper `:472-473` invokes `scope::apply(self.preview(..., "bind"))`; preview helper `:462-470` reaches `scope_binding.rs:882-884`. The status operation alone is read-only, but its unchanged binding setup is not frontier-free.
- Obligation subsets remain exactly two intended ported and eleven partial; the parent-owned ledger remains conservatively partial until qualification. No production frontier checks/refusals, test bodies, golden requirements or dependency gates were weakened.



## Dependency and platform account

New locked packages: regex 1.12.2; regex-automata 0.4.18; regex-syntax 0.8.11; aho-corasick 1.1.5; memchr 2.8.3. Enabled top-level features are only std/perf: all character classification/case semantics are explicit baseline tables/literals, not crate Unicode tables. These crates are Rust regex/search components, with no C library/backend or interpreter dependency. Actual GNU/musl target verification is parent-owned and unrun here; do not label it verified from dependency inspection.

Commands actually executed by this worker:

```text
sha256sum skills/agent-memory/scripts/agent_memory/security.py
# 768a91fad491ed3996defae434a41b983c2aa42dacc4aede5ab3009226a5c5d8
cargo generate-lockfile --offline
# Locking 23 packages to latest Rust 1.85 compatible versions; Adding regex v1.12.2
# Metadata resolution only: no build/check/test execution.
git show f7391170a0774f1589218025450346ff7e356f23:Cargo.lock
# Read-only frozen lock input; confirms no unrelated version changes.
```

Linux x86_64 workstation; no target, rustc/kernel verification or compilation was run. Required source-policy inspection (not a build/lint/test): `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/security.rs` returned exactly `474:            bank_contract::merge(&mut value, &crate::bank_capacity::details(&error)).expect("capacity fields");`. This sole expect has a preceding invariant comment: both merge inputs are objects constructed by the error-envelope helpers. No input-derived indexing/slicing/unwrap matches. Captured-vector proof and executable verification remain parent-owned; no passing results invented.

## Observed parent verification

Host: Linux x86_64 kernel `7.0.11-76070011-generic`, `/tmp` tmpfs; rustc `1.94.1 (e408947bf 2026-03-25)`, cargo `1.94.1`.

- `cargo fetch --locked`: pass for the approved dependency closure.
- `cargo fmt --check`: final pass.
- `cargo check --offline --locked --all-targets --features dev-tools`: pass, no warnings.
- `cargo test --offline --locked --features dev-tools --target <triple> --test t1_security`: **10 passed, 0 failed, 11 genuine dependency ignored**, three consecutive runs on each of `x86_64-unknown-linux-gnu` and `x86_64-unknown-linux-musl`. Every run includes all 31 independently captured CPython cases, the dense rejected-prefix regressions at both scan budgets, and actual binary empty-approval behavior.
- `cargo test --offline --locked --features dev-tools --target <triple> --test t1_bank_transactions`: after the assertion correction, **26 passed, 0 failed, 1 frontier-dependency ignored** on each target.
- `cargo test --offline --locked --features dev-tools --target <triple> --test obligations_ledger`: **5 passed, 0 failed** on each target after the final two-ported/eleven-partial marking.
- Actual GNU binary `am security-policy --help`: exit 0, real request/repository/operator-approval options displayed.
- Final required panic scan has one `expect("capacity fields")` in `command_result`, immediately preceded by the invariant that both merge operands are helper-constructed objects. No other production unwrap/expect/slice-range match.

Full-suite and correction boundary is explicit: parent ran `cargo test --offline --locked --features dev-tools --target <triple> --no-fail-fast` **once on each target**, recording **548 passed, 2 failed, 33 ignored** across 27 top-level harnesses. One failure was the not-yet-delivered external capture; the other was the newly enabled receipt test comparing insertion-ordered values across a sorted encoding. The real capture and canonical-byte receipt assertion corrected those two paths, which are green in the target-qualified results above. No production code changed after those full runs; parent reran only affected targets and ledger checks, not a second broad suite. This is composite proof, not a claim that a fresh full-suite invocation exited zero.

Earlier qualification exposed six evaluator scenarios and five scope fixture setups requiring a real missing frontier; the baseline follows the same unconditional observer chain. Precise dependency gates were restored without fake providers, altered production refusals or deleted test bodies. Independent review also found greedy-tail retries on rejected credential candidates; prefix/boundary checks now precede anchored greedy credential/token/header matching. Dense negative/later-positive regressions prove preserved accepted outputs without timing assertions. Empty operator-approval arguments retain baseline absent-approval meaning.

Initial failed logs, the original external-capture request, the captured-envelope adapter failure and both full-suite logs are retained in `/tmp/agent-memory-security-proof/`, together with all six final focused runs, corrected receipt/ledger proofs, platform, format/check, panic scan and actual help output.

Additional release qualification is the integration owner's: macOS arm64 and Linux aarch64 musl. GNU is a development proof target, not an additional release artifact. Native harness and installed signed-release acceptance are not claimed. The frozen base lacks the subsequently merged test-world files, so the owner was asked to remove its security-only fixture ignore when merging this lane; no route-presence branching is added here. Whole #249 acceptance and tracker reconciliation remain with the sole integration/tracker owner.

## Independent source review

`ReviewSecurityWave` returned final source verdict **correct**, confidence 0.8, with no new findings after the prefix-ordering, empty-approval and dependency-accounting corrections. The reviewer performed source review only and explicitly did not run checks. This is not CPython parity or platform qualification evidence: actual captured vectors, regex limits on each target, exclusive-rename filesystem support, GNU/musl/macOS/aarch64 execution and final ledger qualification remain parent-owned.

Malformed dynamically typed request fields (such as list/dict values where a source/category string is required) produce the explicit typed security-contract refusal rather than CPython's incidental `unhashable type` exception wording. Malformed JSON uses the shared Rust codec's diagnostic wording rather than CPython decoder wording. These refusal-message differences do not authorize invalid input, echo its payload or convert it into success.
