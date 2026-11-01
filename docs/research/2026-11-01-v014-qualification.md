# v0.0.14 prepublication qualification

Accepted and landed functional candidate:
`0440f4cc8031bacf59b0f54b7d440917b02d5d43`.
Independent reviewers accepted #245 and #243 separately after resolving their
actual initial BLOCK findings:
[#245 verdict](https://github.com/robertelee78/agent-memory/issues/245#issuecomment-6563093415),
[#243 verdict](https://github.com/robertelee78/agent-memory/issues/243#issuecomment-6563118387).

[Source-bound result evidence](2026-11-01-v014-qualification.json) records the
executed cases, source hashes, measurements, preservation and retained limits.

| Local qualification | Passing suites | Passing tests | Longest suite |
| --- | ---: | ---: | ---: |
| macOS, exact accepted candidate | 14 | 167 | 296.426 s |
| Linux, source-bound composed coverage | 14 | 167 | 187.841 s |

Every counted suite has zero failures, errors, skips or expected failures and
completed within its existing 300-second budget. Linux combines relevant passing
20a1ec6 suites, the corrected 55276fc update suite and final 0440f4c boundary
suites; independent review confirmed unchanged relevant source/test bytes. The
74 runtime Python source hashes also match across these receipts and the
prepared-release public scenarios. This is not a throughput or latency claim.

Both platforms exercised 23 public bank scenarios: exact/excess 16,384-byte
boundaries, synthetic 79,054-byte source, full/scoped inspection and recall,
unchanged small capture, doctor, unsupported layout and real aggregate/count
excess. Fetched-authority before/after scenarios retain the corrected typed
failure and unchanged local bank/feature/HEAD/index. Writer cases exercise
complete intended-map refusal, large owned transaction markers, retry and
interrupted finish/rollback.

Installed Codex and Claude fixtures cover unrelated-setting/group acceptance,
missing/duplicate owned groups, duplicate-key privacy, read-only preservation
and explicit uninstall retaining unrelated configuration. Six legacy receipt
edge cases per platform cover prompt-only evidence and invalid present Stop
fields. These proofs do not establish native client invocation, source coverage,
semantic truth, capture, memory publication or fresh recall.

Prepared source `7daf40220f7ef842b01f7ec8baef4648836c9a03` also passed the public
bank/hook scenarios on both platforms. Its locally built signed v0.0.14 portable
artifact digest is
`4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7`.
Publication and actual public fresh-install/v0.0.13-upgrade delivery remain
separate evidence; no prospective success is asserted here.

## Retained failures and scope

The optional historical exhaustive gate was stopped, not passed. Repository
README policy permits focused local qualification for bounded releases. Captured
historical logs contain elision, so no complete-matrix coverage is claimed.
A concurrent macOS update-suite attempt timed out at 300 seconds; the later
serial accepted-candidate suite passed. Initial Linux JavaScript skips were not
counted passing; an isolated, checksum-verified Node prerequisite enabled the
subsequent zero-skip boundary run without changing system/profile tools.

The historical installer suite still has two failures, reproduced with matching
assertions on the unchanged baseline: an old-receipt `changed` expectation and a
bootstrap fixture that expects the first failing Python to prevent selection of
a later supported interpreter. The released v0.0.2 updater also refuses modern
guide content identically on baseline and candidate. These are retained failures,
not claimed fixes or proof of v0.0.2-to-v0.0.14 compatibility. Actual delivery
qualification targets fresh installation and the current published v0.0.13.

The unavailable originally reported bank and unknown terminology were not
accessed, repaired or captured. Capacity recovery remains unavailable/pending;
no truncation, deletion, selector bypass, raised limit or reinitialization is
presented as repair.
