# v0.0.10 operator adoption and recurring cache drift

The release passed separate public disposable-profile installation/upgrade
checks before either actual operator installation was updated. Signed v0.0.10
source is `76a7886408e8a295adca7f3df2536efdb1a107b7`.

## macOS

The actual signed v0.0.9 to v0.0.10 update succeeded in 149.643 seconds.
Version readback reported the exact verified release. Doctor for the project
returned `ok: true`, ready setup and no warnings in 3.598 seconds. This elapsed
update time is an actual-profile observation, not the 17.757-second disposable
upgrade result; its latency cause was not profiled and is not attributed to a
specific operation. No source/receipt bypass was needed.

## Linux refusal and bounded repair

The first actual operator update safely refused after 1.588 seconds:
`Installed portable identity differs from its signed release`.
Read-only forensics established all 189 signed v0.0.9 payload hashes and modes,
the receipt manifest identity and pinned SSH signature were exact. There were
35 additional owned regular CPython3.14 bytecode files and no unknown payload
modifications or links. The installed signed payload was intact; its complete
installed inventory no longer matched.

The caches had matching headers/source bindings, and their fully consumed,
parsed code attributes recursively matched trusted recompilation of the signed
source under the same Python version. Cached code was not executed. This does
**not** claim raw marshal serialization identity; interning/reference encoding
can differ. The original cache bytes and proof were retained.

A forensic-qualified repair preserved the exact cache directory in private
quarantine using the existing lifecycle lock, retained directory descriptors and
Linux no-replace rename. It changed no payload, receipt, journal, owner record,
permission or integrity exclusion. The source signature was not rewritten or
replaced with a newly blessed local inventory. All 35 quarantined file hashes
remained identical.

One subsequent normal update succeeded in 28.470 seconds. It reported verified
v0.0.10 source, and doctor passed without warnings in 3.690 seconds. The consumer
repository's HEAD, index, bank and Git status were exactly preserved. The new
installed source had no bytecode extras. This was a verified repair of one owned
profile, not a shipped automatic `doctor --fix` cache-healing feature.

## Established cause and unresolved producer

The immediate refusal cause was extra executable-cache inventory. The process
that created those files is **unidentified**. Signed v0.0.9 was installed at
2026-10-28T15:12:50.995271UTC; cache modification times range from
18:09:44.654728 to 18:09:56.856725UTC. Timestamps establish ordering, not caller
identity. The current AM launcher uses `-I -B`; these observations do not prove it
created the caches or establish an end-to-end writer RCA.

Recurring cache creation and evidence-preserving automatic recovery design need
a focused v0.0.11 research follow-up. It must distinguish AM-owned entry points
from external imports, reproduce the actual writer before attributing cause,
and independently review any proposed healing. Unknown payload changes must
remain preserved and refused. No new behavior, pruning, receipt rewrite,
unsigned fallback or signature/inventory exemption is authorized by this report.
The completed v0.0.10 delivery remains limited to its two independently accepted
corrections.

[Redacted macOS readback](data/2026-10-28-v010-operator-adoption/macos.json) and
[Linux forensic/repair/adoption summary](data/2026-10-28-v010-operator-adoption/linux.json)
retain timings, exact source identity and proof hashes. Full original receipts,
forensics, quarantine paths and recovery metadata remain privately preserved.
