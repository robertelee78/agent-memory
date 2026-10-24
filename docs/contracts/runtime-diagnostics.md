# Runtime and repository diagnostics

The installed CLI and the portable `scripts/diagnose.py` provide `version`,
`status` and `doctor`. All emit `agent-memory/diagnostic-result-v1` JSON;
`--json` is accepted explicitly. Source helpers remain available when an owned
wrapper or managed source is missing. Diagnostics perform no setup, network,
repair, audit-log or canonical-memory writes.

`version` measures the executing portable artifact's version, digest, source
path, Git revision and cleanliness, alongside the external receipt's recorded
installation mode, channel/ref, schema and source. An outside helper can have a
different executing digest than the recorded installation. A digest is an
identity measurement, not a signature or a runtime acceptance claim.

`status --repo PATH` reads bounded local records and router candidates, rather
than scanning every routed lesson. It reports developer selection, checkout
binding, observed bank schema/project ID, root-route counts, enabled client
adapters and pending runtime/bank markers. Complete record counts and uninspected
freshness are explicitly unassessed. `not_located` does not prove absence of an
ignored or unindexed custom bank. Doctor uses the established complete locator
before reporting absence or validating the bank.

`doctor --repo PATH` audits local prerequisites, existing directory and backup
permissions, native atomic-switch API availability, runtime ownership and channel,
wrapper/link/copy/activation identity, artifact routing, bank schema and canonical
format, optional history/tombstones and topic routes. It checks an explicitly
selected `--projection PATH` against the complete current source hashes and
active canonical ID/version/path references without writing a retrieval audit.
Actual applicability remains a normal governed-retrieval decision. Native API
detection is not a filesystem write probe; actual native switch capability is
established by the lifecycle operation's verified activation.

The existing lifecycle lock is inspected with a nonblocking shared flock; no
lock file or owner metadata is created. A live writer defers ownership checks.
Bank writer/transaction markers defer stable content validation; they are never
stolen or removed by doctor. Source, target and receipt failures include bounded
repairs, and command exceptions do not echo private record contents or credential
URLs. Credential-bearing remote URLs are reduced to canonical repository IDs.

Exit 1 denotes failed verification. Exit 0 can contain warnings, including absent
setup, an active writer, pending recovery or the unsigned initial Git channel.
Consumers must inspect warnings as well as failures. Native v1 has no global
projection registry, vector service, authoritative event-log store or bundled
schema migrator. Disabled surfaces are reported honestly rather than checked as
empty successes. Optional GitHub publication is selected by its own operation;
doctor never probes authentication or remote availability.

Repair remains a separate operation: trusted runtime recovery for lifecycle
journals; source-bound bank recovery for memory writes; explicit projection
rebuild for stale derived references. Purge retry requires its original exact
confirmation and deletion authorization; runtime recovery and doctor cannot
authorize it. Missing installation/identity/bank setup does not prevent useful
read-only diagnostics.

`tests/contracts/test_runtime_diagnostics.py` invokes real helpers and installed
commands in disposable homes. It covers healthy and modified installations,
source/channel drift, corrupt ownership, locks and interruption, bank format and
permissions, stale projections, credential redaction and persistent-state
preservation. Complete cross-platform lifecycle release verification remains
the separately tracked lifecycle-matrix story.
