# Agent Memory v0.0.3: independent static review

**Reviewer:** Claude Opus 5.5 (`claude-opus-5-5`), as reported by the harness.
**Binding:** archive of candidate `90087179dd47ba5c32866c8e226afd7c919f1460`, as supplied by the parent. Nothing was executed, written, fetched or installed.

## Disposition: APPROVED FOR STATIC SCOPE

I found no release-blocking defect in the flows I read. Below are three low-severity findings and one note. This does not approve the whole release. The reliability stress schedules (12/1,000 and 24/10,000 plus variations), release delivery and all test results are still PENDING; I ran no tests.

## Files reviewed

- **Docs:** `README.md`, `docs/releases/v0.0.3.md`, `docs/contracts/lifecycle-security.md`, `docs/research/2026-10-26-renewed-dispute-resolution.md`
- **Distribution:** `scripts/distribution/release.py`, `scripts/distribution/install.sh.in`
- **Modules in `skills/agent-memory/scripts/agent_memory/`:** `release_distribution.py`, `updating.py`, `uninstallation.py`, `sharing_holds.py`, `sharing_hold_mapping.py`, `sharing_frontier.py`, `recall_catalog.py` (all in full)
- **Read in part:** `installation.py` (lines 96–215 and 330–1100), `semantic_sharing.py` (31–150 and 377–576), `memory_publication.py` (200–450), the proof-check call sites in `memory_refresh.py`, the flag handling in `console.py`, `developer_identity.digest`
- **Regression:** `tests/contracts/test_known_holds_v2.py`
- **Consumer exclusion:** the full file list under `skills/agent-memory/`

**Not reviewed:** `discovery.py`, `retrieval.py` (except where the catalog calls it), the body of `managed_source.py`, the body of `sharing_recovery.py`, `diagnostics.py`, `purging.py`, and the other test suites.

## What I verified

- **Signature before candidate code runs:** `install.sh.in` embeds the verifier and runs it under `python3 -I`. The default path checks the manifest signature with `ssh-keygen -Y verify` against the hard-coded key, principal and namespace *before* parsing the manifest (`release_distribution.py:79-101`).
  - Tag, version and repository must match the locator.
  - The archive's size and SHA-256 are checked before tar parsing, and gzip inflation is bounded.
  - Archive members may only be regular files or directories under `skills/agent-memory/`, with modes 0644/0755, no PAX headers and no `..`.
  - The full byte and mode inventory must match `artifact_digest`.
  - Only then is `agent_memory` imported, at `install.sh.in:69-74`.
  - The installer re-checks the staged layout through `source_info` → `release.installed` (`installation.py:351-360`).
- **Publishing:** `release.py` signs a `git archive` of `skills/agent-memory` only, reads back all four assets plus the public `acquire` path, and never force-pushes main.
- **Consumer exclusion:** the only content under `skills/agent-memory/` is the skill files, schemas, templates, keys and scripts. There is no bank. `preflight_path` refuses install targets that overlap a `memory-bank` or `MEMORY.md` (`installation.py:446-448`).
- **Update and ownership:**
  - Downgrades are refused, and a same-version signed release must have an identical identity (`updating.py:136-145`).
  - The legacy same-version transition compares bytes and executable intent against `git ls-tree`.
  - Activation uses staged, journaled native exchange with readback.
  - Uninstall removes only targets whose identity is unchanged, keeps the source whenever any client target is preserved, and never deletes backups.
- **Renewal fix:**
  - `record()` adds the replaced resolution to `resolution_history` (`sharing_holds.py:254-262`).
  - `filtered_records()` restores exactly the previous resolution and history when the current judgment is excluded (`:177-181`).
  - Re-recording the same judgment is idempotent. A stale earlier proof fails both the manifest and the `expected_inventory` check.
  - The first-commit crash case (journal written, no registry yet) recovers correctly, because `digest(None)` returns `None`, which matches the stored `observed_sha`.
  - The regression test exercises three renewals, a stale-proof refusal, unchanged registry bytes during checks, and registry loss followed by recovery.
- **Publication and refresh:** both call `semantic.check` again before acting. `land` checks once more right before pushing, compares hashes against `publish_hashes`, and refuses candidates that touch paths outside the bank or have unexpected parents.
- **Recall catalog:** the catalog is HMAC-authenticated with a private per-user key and stored outside both the checkout and the source tree. Before any qualified output, `recheck()` re-hashes all canonical bytes, so file stamps are only hints.

## Findings (all non-blocking)

1. **A fresh install has no protection against being served an older signed release** (`release_distribution.py:218-228`).
   - **Trigger:** whoever controls the HTTPS answer from `api.github.com/.../releases/latest` returns an older tag that has a valid signature.
   - **Consequence:** an outdated but genuinely signed runtime gets installed. The manifest has no expiry or freshness field. The updater's downgrade guard only helps once something is installed.
   - **Severity:** low. It requires compromising GitHub or TLS, and the docs already name HTTPS as the initial trust anchor. Worth stating explicitly as a limit in `lifecycle-security.md`.

2. **Resolution history has a hard cap with a misleading error** (`sharing_holds.py:66-71`, enforced at `:264`).
   - **Trigger:** a single dispute renewed more than 4,096 times, or the 16 MiB store limit reached through many renewals across many disputes.
   - **Consequence:** further renewals are refused for good with "Known resolution history is incomplete; preserve it for recovery". That message points the user to recovery, which won't help, instead of saying a capacity limit was hit.
   - **Severity:** low. This is practically unreachable in normal use.

3. **Uninstall doesn't apply the managed-source repair that update uses** (`uninstallation.py:77-78` compared with `installation.py:710-718`).
   - **Trigger:** a legacy managed-Git runtime checkout whose `.git` contents changed since install (for example, the user ran git inside it).
   - **Consequence:** update verifies the checkout via `managed_source.verify` and refreshes the receipt. Uninstall instead marks the source "identity differs" and leaves the runtime directory in place.
   - **Severity:** low. It fails safe and is reported under `preserved`. Signed-release installs aren't affected, because their source copy has no `.git`.

**Note (design, not a defect):** for an `operator-clarification` resolution, the evidence must include some current, qualified `user-feedback` record (`sharing_hold_mapping.py:76-78`). The code doesn't check that the feedback is about this particular dispute. That depends on the agent's judgment, which the code labels as a fallible assertion, plus the requirement that every involved pair be reviewed as non-contradictory. This matches the documented model.

## Limitations

- This was a static read only. No tests, product code or network calls were run.
- Several modules were read partially or not at all, as listed above.
- I did not assess recall quality, latency claims (4.836/4.869 s warm, 18.859 s cold), platform behaviour, concurrent stress, or GitHub-side release state.
- The parent's report that 20 focused methods passed is theirs; I did not verify it.
