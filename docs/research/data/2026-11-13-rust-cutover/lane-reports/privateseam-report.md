# Lane PrivateStateSeam — report

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-privateseam`, detached at integration commit `46a0100`. No Git writes.

## Outcome

`installation::seams::PrivateState` now has a production provider, and it is registered in `cli::register_providers`. `am install`, `am update` and `am uninstall` no longer refuse with `Private-state lifecycle guard is not registered for this runtime; the lifecycle operation refuses` (`Error::Unregistered`). The next refusal on a fresh `am install --development-source` is **`Private-state hooks provider is not registered`**. That is `private_state::register_private_state_hooks`, which still waits for installation, omp_boundary and admission (listed in `tests/wiring-pending.txt`). The same unregistered hooks already make the registered `updating::Runtime::banks` refuse in `install --check`.

## Files modified (exact list)

- `src/private_state.rs` (owned): `activation_guard` rebinding, plus the `#[cfg(test)] mod activation_rebinding_tests` regression.
- `src/cli/wiring.rs`: new adapter `LifecyclePrivateState` with the helpers `lifecycle_roots` and `present`.
- `src/cli/mod.rs` (shared, append-only): one `register_private_state` call appended at the end of `register_providers`.
- `tests/wiring-pending.txt` (shared): deleted the `installation::seams::register_private_state blocked by …` line.
- `tests/t1_private_state.rs`: updated the existing `activation_guard` callers to the new signature: a turbofish `::<_, ContractError>`, and the transition returns `Ok(None)`. Behaviour is unchanged. All of these tests are still `#[ignore]` for the same reasons as before.

## Public API change

```rust
pub fn activation_guard<T, E: From<ContractError>>(
    installation: Option<&Value>, target_artifact: Option<&Value>, operation: &str,
    current_repo: &Path, runtime_root: &Path,
    transition: Option<&mut dyn FnMut() -> std::result::Result<Option<Value>, E>>,
    body: impl FnOnce(&Value) -> std::result::Result<T, E>,
) -> std::result::Result<T, E>
```

- The transition may now return `Some(target)`. That replacement becomes the target artifact for **both** `require_compatible` proofs: the one before the body and the exact readback after it. This matches the baseline closure `target.clear(); target.update(prepare_lifecycle(plan))`. `None` keeps the caller's target, so callers that don't rebind behave as before.
- The function is generic over the caller's error type, so installation's `Error::Unregistered`, `Hook` and `Release` pass through the transition and body without being flattened into a `ContractError`. Internal refusals convert with `From<ContractError>`.
- Nested-barrier semantics are unchanged. A nested call reuses the outer context and still runs its transition, and the rebinding applies there too, as in the baseline. Drop still restores the context and unlocks on refusal or unwind.

Callers checked: there are no other `src` callers. Installation reaches the guard only through the seam. The test callers are in `tests/t1_private_state.rs`, updated above.

## Adapter (`src/cli/wiring.rs::LifecyclePrivateState`)

It follows the wiring.rs convention: forward to the plain `private_state` functions and add no behaviour.

- `current_repo` is `std::env::current_dir()`, matching baseline `Path.cwd()`. `repository_inventory` canonicalises it itself.
- `runtime_root` is `installation::locations(&Environment::process())?.runtime_pack()`, which is the owned runtime pack that the lifecycle activates. `updating::Runtime::banks` (the registered `Updating` adapter) and `uninstallation` use the same root.
  - `cli::pack_root()` was **not** used. It requires an existing Rust-era receipt and the running executable to be the receipt's binary, so it would refuse on a first install or a development-source install.
- JSON `null` maps to the baseline `None` for `installation` and `target`, the same convention as `uninstallation.rs`. `prepare_lifecycle`'s `None` maps to `Value::Null`; installation's transition only rebinds when a target was selected, as in the baseline `if target is not None`.
- A `Some(Value::Null)` rebinding cannot happen with the current callers. If it did, it refuses through `compatibility` with "Target runtime has an unknown private-state compatibility declaration"; the baseline would have raised `TypeError`.

## Tests

- New regression, `src/private_state.rs` `activation_rebinding_tests` (lib unit tests, because they need the module-private `BARRIERS` context):
  - **`transition_replacement_is_the_target_of_the_compatibility_proof`.** It runs the guard nested under an outer context. The repository is a symlink loop, so as soon as the target's compatibility declaration passes, the proof refuses deterministically with "Symlink loop or excessive name resolution". It needs no HOME, Git or registered hooks. It shows:
    - valid target → loop refusal;
    - unknown-token target → "unknown private-state compatibility declaration";
    - transition returning `None` keeps the unknown target;
    - unknown target rebound to a valid one → loop refusal, so the replacement was used;
    - valid target rebound to an unknown one → declaration refusal.
    - In every case the body is not entered.
  - **`transition_refusal_keeps_the_caller_error_and_skips_the_body`.** A custom caller error type passes through unchanged and the context is restored.
  - Limit: the post-body readback with a rebound target is not observable until `PrivateStateHooks` is registered, because the pre-body proof must pass first. It is the same variable as the pre-body check in the code.
- No baseline test exists for rebinding. The only baseline use of `activation_guard` in `tests/` is `test_update_discovery_gaps.py::test_saved_bank_binding_keeps_missing_repository_and_bank_required`, already annotated, partial and ignored in `t1_private_state.rs`.
- **Step 4, fixture conversions: none possible.** `tests/t2_installation.rs` and `tests/t2_lifecycle.rs` keep `FixturePrivateState`, because every real `activation_guard`, `require_compatible` and `prepare_lifecycle` call on a lifecycle with a target reaches `repository_inventory` → `private_state_hooks_provider()`. That provider is unregistered and waits for installation, omp_boundary and admission. No t2 test was ignored because of this seam.
- Obligations: no baseline tests were ported, so the ledger was not marked.

## End-to-end proof (macOS)

- I built `am` with `cargo build --offline --bin am` for this lane, and for the base commit `46a0100` from a `git archive` export in a separate `/tmp` directory, with no Git writes.
- Each run used a disposable `/private/tmp` world following the `tests/support/world.rs` env pattern: `env -i`, with `HOME`, `XDG_*`, `TMPDIR`, `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`, `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL` (an empty file), `LC_ALL=C`, `TZ=UTC`, `PATH=/usr/bin:/bin`, and cwd `<world>/repo`.
- The development source was the minimal valid pack from the `t2_installation` `development_source` fixture, plus the built `am` copied to `target/release/am`.
- Command: `am install --development-source <world>/dev --presets none`

| build | install | uninstall | update |
|---|---|---|---|
| base 46a0100 | `Private-state lifecycle guard is not registered for this runtime; the lifecycle operation refuses` | `Missing, linked or oversized installation record: …/receipt.json` | `No installation receipt; install the managed channel first` |
| lane | **`Private-state hooks provider is not registered`** | same as base (no receipt) | same as base (no receipt) |

`install --check` with the lane build also refuses with `Private-state hooks provider is not registered`. That comes from the registered `updating` banks, which run before the private-state seam in check mode.

The next missing provider is `private_state::register_private_state_hooks`. Its refusal is a `ContractError` raised in `private_state.rs` (`private_state_hooks_provider`), not an `installation::Error::Unregistered`.

Additional finding, outside this lane: with the real repository pack (`skills/` from this worktree) as the development source, `am install` refuses earlier, during artifact inspection, with `Command am capture is not declared by the command surface`. The real SKILL.md references `am capture`, which the Rust command surface does not declare yet.

## Declared deltas

- A `Some(null)` rebinding refuses with the target compatibility-declaration refusal. In the baseline this was an accidental `TypeError`, and no current caller produces it.

## Foundation changes

None.

## Hygiene

`grep -n 'unwrap()\|expect(\|\[.*\.\.' src/private_state.rs src/cli/wiring.rs`:
- line 1004 `&[][..]` is pre-existing; it is an empty slice and is not derived from input;
- lines 1154–1161 are all inside `#[cfg(test)] mod activation_rebinding_tests`;
- there are no hits in `wiring.rs`.

## Commands run

Platform: Darwin 27.3.0 arm64 (`aarch64-apple-darwin`), rustc 1.99.1.

- `cargo fmt` (fmt changed only lane files), then `cargo fmt --check` → clean.
- `cargo check --offline --all-targets --features dev-tools` → 0 warnings, 0 errors.
- `cargo test --offline --features dev-tools --lib activation_rebinding` → 2 passed.
- `cargo test --offline --features dev-tools --test wiring --test t1_private_state` → wiring 2 passed (1 ignored); t1_private_state 10 passed (7 ignored).
- `cargo test --offline --no-fail-fast --features dev-tools` → exit 0; 56 test binaries, 778 passed, 0 failed, 38 ignored. No `FAILED` or `panicked` lines (log: `/tmp/privateseam-test.log`).
