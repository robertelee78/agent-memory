# Signed release implementation qualification

Accepted candidate: `b068486990d1b05a9f731347d7db08281b13b484`.
Independent [acceptance verdict](https://github.com/robertelee78/agent-memory/issues/49#issuecomment-6400662844).
The [machine results](data/2026-10-25-signed-release/qualification.json) preserve
registered-method counts separately from the supplemental boundary spike.

| Target | Registered methods | Supplemental boundary cases | Source binding |
| --- | ---: | ---: | --- |
| macOS, Python 3.15.0 | 6 | 5 | All 450 tracked bytes/full Unix modes match before/after |
| Linux, Python 3.13.7 | 6 | 5 | All 450 tracked bytes/full Unix modes match before/after |

Both use the same canonical 150-file portable artifact:
`45f3bac2fc8a2ec7deeb3ab335d5e86a23806b788b2e85e18afea02e14f743dc`.
Both mandatory signing suites ran through the normal registry-selected worker,
with zero failures, errors, skips, expected failures or unexpected successes.
The source tree and portable bytes stayed unchanged through qualification.

The installed journeys cover one-line acquisition, quiet/unchanged update,
owned client copies, interrupted update/recovery, preserved project memory,
uninstall/reinstall and actual released v0.0.2-process adoption under022/002.
Negative cases include invalid signatures/namespace/key, unsafe signed archive
members, downgrade/version replacement and unavailable signing prerequisites.
The publisher control uses real commands, signing, TLS and local Git, plus a
bounded external service adapter; it covers first draft, partial-upload resume,
exact repeat, tag drift, current-default changes, push race and key custody.

The separately archived [transport spike](data/2026-10-25-signed-release/transport_boundary_spike.py)
executes the installed CLI for a genuine short HTTP body, oversized metadata,
archive size, chunked archive without Content-Length and signed gzip expansion.
Every case refused and preserved active targets, receipt, lifecycle state, valid
existing bank, unfinished feature, index and HEAD. Short HTTP bodies produced
curl18 on macOS and curl56 on Linux; both were refused, not normalized into one
reported transport code. These five cases are not additional registered methods.

Earlier 33-method runs belong to `2dcd8d1`, not the corrected candidate. Its earlier
Linux source staging inherited0664 files from Git archive's default umask and has
a different physical artifact identity. That cohort remains qualified separately;
it is not relabeled as the canonical artifact above. Final fresh Linux staging
used an explicit archive umask and verified full modes before execution.

Actual metadata remains0.0.2 until the complete milestone freezes. All signing
keys/versions/URLs in qualification are isolated fixtures. No production private
key signed qualification data, and no GitHub qualification release was created.
Actual signed public assets, tag and current default installation still require
readback during the real product release, before main activation and delivery.
The accepted implementation remains on the release integration branch until then.
