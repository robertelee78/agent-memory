# v0.0.7 corrective qualification

Product implementation: 4cb7f02346471947a801cf0101aaafe3ab15889f.
Design/source: ADR-0019, fba0875fcdda2e56362c03fee2166a13344454d3.

## Focused checks

- Baseline identity/Codex contracts: 18 tests, 17.658 seconds, passed.
- Initial project/scope regression: 30 tests, 37.9 seconds, passed.
- Repair/hooks/doctor regression: 16 tests, 139.524 seconds, passed.
- Setup diagnostics after correction: six tests, 117.890 seconds, passed.
- Final explicit-link identity correction: 26 tests, 22.2 seconds, passed.
- Final adapter/doctor integration correction: 14 tests, 14.664 seconds, passed.
- Earlier common focused snapshot: 42 tests passed on macOS and Linux.
- Final Linux source snapshot: 49 tests, 21.648 seconds, passed.
- Final macOS source snapshot: 49 tests, 54.290 seconds, passed.
- Claude/OpenCode boundary regressions: 16 tests, 60.226 seconds, passed.
- Portable package validator: valid, version 0.0.7. No bank writes.

## Independent review

Opus reviewed the ADR and revisions before task extraction/execution. Its
implementation reviews are retained separately, including rejected findings
rather than an invented unconditional PASS. Those findings drove explicit-link
recovery, per-worktree/per-adapter failure isolation, owned malformed-metadata
healing, record-removal race handling and missing-ledger diagnostics.

Independent hook_research review of final T1/T2 code: PASS bound to product commit 4cb7f02346471947a801cf0101aaafe3ab15889f. Independent identity_research review of final T3/diagnostic
code: PASS. Five-file digest over path-NUL-bytes-NUL for diagnostics.py,
doctor_fix.py, hook_failure.py, codex_prompt.py, codex_stop.py:
0e0c5d0d8e54d086161ec8293ba3b98ac2c304d8f69651b007f6b8b9184f1672.
This is independent implementation review, not proof of every semantic outcome.

## Real Codex evidence

A disposable real Codex run using candidate adapters and simulated historical
device evidence completed refresh against a local bare authority with
remote_freshness_verified:true, Stop continuation with actual turn identity,
and verified no-learning acknowledgment. Elapsed 43.48 seconds, pending zero,
completed one. No publication was claimed. Explicit test hook trust is not a
claim about production hook trust configuration.

A separate real Codex durable-learning run completed governed admission and
semantic comparison. Publication correctly refused bare-remote governance,
retaining pending source; no completed publication was fabricated. A supported
policy/transport fixture run is separately qualified before final delivery.

## Operational limits

Tests use disposable homes and banks. No user runtime activation or project
bank repair was performed during source qualification. Read-only candidate
resolution recognizes the existing project IDs for agent-memory, ioa-brain,
cve-to-exploit and vox; it writes no registry or bank data. Reboot/remount is a
plausible origin of the device-number change, not a proven OS event. Unrelated
v0.0.7 roadmap tasks are not delivered by this corrective implementation.

## Supported-policy durable run

At product commit 4cb7f023, a real Codex run using the existing explicit synthetic
GitHub policy and real local Git transport completed governed capture, semantic
comparison and verified publication in 276.3 seconds. The witnessed fixture
commit was 2bea80b81c8468c97cd1fa24245255107e6afdaf. The source-bound acknowledgment
reported outcome published and publication_verified true, with zero pending and
one completed boundary. This verifies real model workflow against test policy;
it is not evidence of actual GitHub governance for a consumer repository.

A new Codex thread recalled the fixture's 17-second delay with its applicability
and operator evidence through governed recall. Final boundary counts were zero
pending and two completed. Its authority refresh remained pending because the
login-shell PATH lost the explicit fixture transport. The model diagnosed that
fixture environment without changing origin policy. Recall was from canonical
local memory; fresh imported/shared-authority recall is NOT claimed. No retry or
unbounded gate was used. Raw private proof and source hashes were retained at
am-v007-codex-sihhthvj; authentication material was removed by the controller.

All tested process exits, assessments, admissions, published outcomes and local
versus authority-fresh recall are reported separately. Public release signature,
installation/update and production activation are verified after publication.
