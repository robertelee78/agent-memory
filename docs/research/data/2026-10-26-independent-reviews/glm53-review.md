# Independent Static Review — Agent Memory v0.0.3 (candidate 90087179dd47ba5c32866c8e226afd7c919f1460)

**Reviewer identity:** GLM-5.3 (opencode provider), as reported by the harness. No invented reviewer identity.
**Disposition: APPROVED FOR STATIC SCOPE** — no concrete correctness/security/recovery release blocker found in the reviewed priority flows. This is explicitly **not** whole-release acceptance.

## Files actually reviewed (read, not executed)

- Docs: `README.md`, `docs/releases/v0.0.3.md`, `docs/contracts/lifecycle-security.md`, `docs/research/2026-10-26-renewed-dispute-resolution.md`
- Signed delivery: `install.sh`, `scripts/distribution/install.sh.in`, `scripts/distribution/release.py`, `scripts/distribution/README.md`, `agent_memory/release_distribution.py`
- Ownership/healing: `agent_memory/installation.py` (full), `updating.py`, `uninstallation.py`, `managed_source.py`, `diagnostics.py`
- Semantic recall: `agent_memory/discovery.py`, `recall_catalog.py`, `retrieval.py`, `bank_observation.py` (partial), `bank_writes.py`, `security.py`, `sharing_limits.py`, `pack_validation.py`, `artifact.py`, `__init__.py`, `schemas/runtime-compatibility.json`
- v2 sharing: `agent_memory/semantic_sharing.py`, `sharing_frontier.py`, `sharing_holds.py`, `sharing_hold_mapping.py`, `memory_publication.py`, `memory_refresh.py`, `sharing_recovery.py`, `local_registry.py`
- Regression: `tests/contracts/test_known_holds_v2.py`, registration in `tests/contracts.json`; layout checks of `memory-bank/`, `skills/agent-memory/`, `skills/agent-memory/keys/allowed-signers`

## Prioritized findings

**1. (Verified correct — the renewed-dispute fix)** `sharing_holds.py:171-183` (`filtered_records`) + `sharing_holds.py:251-262` (`record`) + `sharing_hold_mapping.py:42-93`. Proof validation reconstructs the registry as observed before the validating judgment by restoring the exact predecessor from `resolution_history` (previously `null`, which self-invalidated the proof); same-judgment re-check is idempotent; a stored resolution whose endpoint sources moved fails `validate_resolution(stored=True)` and forces renewal (stale-predecessor refusal). The registered regression (`test_known_holds_v2.py:45-92`) exercises three renewals, repeated proof checks, stale-proof refusal with registry byte-stability, registry loss/recovery, legacy import, wrong-project recovery. I traced the idempotency and renewal invariants through `frontier.build → prepare_review → review → record → check` and found them consistent. Not a blocker.

**2. (Non-blocking, diagnosability)** `memory_publication.py:394-398` — `land()` collapses every `ContractError` from `semantic.check()`, including `HoldFailure('known-holds-registry-missing'/'known-holds-interrupted')` with its specific recover-holds repair, into generic `pending-reconciliation` ("Sources or authority changed"). Trigger: holds registry lost/interrupted while a prepared publication is pending. Consequence: operator is steered to re-review instead of `am semantic recover-holds`, which `memory_refresh.py:249-258` correctly surfaces with `failure_code`/`failure_component`/`repair`. Fails closed, no data loss. **Not a release blocker.**

**3. (Non-blocking, hardening asymmetry)** `installation.py:334-343` — `git()` routes `ls-remote` through a subprocess with ambient environment (no `GIT_CONFIG_NOSYSTEM`/`GIT_CONFIG_GLOBAL` reset, unlike `managed_source.git`), so a user-level `url.*.insteadOf` rewrite can redirect channel resolution in `updating.py:189-192`. Consequence: confined to legacy managed non-default channels, which are already documented unsigned/operator-selected; hardened `remote get-url` equality, merge-base fast-forward and `verify_payload` anchoring to the recorded revision constrain identity, and the signed release channel never uses this path (`release.acquire` via curl). **Not a release blocker.**

**4. (Non-blocking, robustness)** `bank_writes.py:38-45` — `hashes()` fully reads every `.md` file with no per-file/total bound *before* `contract.validate` enforces the 16KiB/64MiB bank bounds; contrast the streaming 64MiB cap in `bank_observation.scan`. Trigger: a bank containing one very large `.md` (e.g., from a corrupted/hostile tracked commit) makes `am doctor` bank inspection and `semantic_sharing.final_sources` read it fully before rejecting. Same-UID hostile memory is documented out of the trusted boundary and retrieval/discovery stream with bounds. **Not a release blocker.**

**5. (Verified non-issues, noted for the record)** `recall_catalog.py:362-371` returns oversized reads unverified, but every consumer (`discovery.start`, `retrieval.evaluate_sources:111-119`) re-checks the >16KiB case and refuses/excludes before output. `installation.py:566-567` reuses the installed source for repeated `am install` on a release installation (latest acquisition happens in the bootstrap/`am update`), consistent with README guidance.

## Priority-flow verification summary

- **Pinned signature before candidate execution:** sound. `verify_manifest` (ssh-keygen `-Y verify` with pinned principal/namespace/key bytes), manifest field/tag/source-commit/digest pinning, gzip/tar expansion bound before parse, member type/mode/PAX/symlink/traversal rejection, exclusive-create extraction into a private 0o700 temp tree, full `package_identity` inventory digest equality, and only then `sys.path.insert` + import of candidate code. `installed()` re-verifies from disk bytes with the pinned in-tree allowed-signers equality check. `release.py build/publish` enforces clean-tree tracked-bytes archive, refuses version reuse/tag replacement, exercises the same public `acquire()` readback, and gates main promotion on bootstrap convergence.
- **Consumer exclusion of development memory:** sound. Archive is `git archive -- skills/agent-memory` only (development bank at repo-root `memory-bank/` is outside the path filter); `package_identity` inventories the full pack; `preflight_path` refuses targets overlapping any `memory-bank`/`MEMORY.md`.
- **Ownership/healing update/uninstall:** conservative and fail-closed — external receipt with per-target fingerprints, native no-replace/exchange renames, journal/backup/rollback with bank-readability gate for rollback, wrapper dispatch readback, downgrade/same-version identity refusal, managed payload anchoring to recorded revision, uninstall preserving modified/unrelated targets and restoring exact original bytes only when provable.

## Limitations

- Read-only static review: I ran no product code, tests, or installer, and claim no test results. The parent's focused 20-method/platform pass is unverified by me; the whole-reliability stress schedules (12-session/1,000-lesson; 24-session/10,000-lesson and independent variations) and release delivery remain PENDING and must not be approved from this review.
- Out of provable scope per the product's own contract: same-UID hostile memory/runtime, already-loaded verifier authentication, platform ACL auditing, the HTTPS-fetched bootstrap as initial trust anchor, and GitHub release immutability.
- Modules outside the stated priorities (e.g., `bank_transactions`, `consolidation`, `purging`, `checkpoint`, `projections`, `initialize`) were reviewed only at the interfaces used by priority flows.

Note for the operator (stated once, no action taken): this repository is not currently tied to a Vox room.
