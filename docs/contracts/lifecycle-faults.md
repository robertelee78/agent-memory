# Enumerated lifecycle write and lost-response contract

The v0.0.1 fault harness covers the shipped lifecycle: installation, update,
uninstall, explicit recovery, and separately confirmed data purge. This follows
the approved #60 acceptance scope: lifecycle failpoints first, with the general
governed-bank harness extended in v0.0.2. Baseline bank/sharing integrity and
their existing public interruption tests remain first-release requirements.
This contract does not defer those guarantees or claim the later harness is done.

Each lifecycle operation establishes an explicit write context before its lock
or activation writes. In a validated disposable test World, every instrumented
logical mutation has an ordinal `NAME-write-N-before` and
`NAME-write-N-after` boundary. Trace JSONL records the reached point, ordinal,
phase and mutation kind. The harness records a real clean operation, enumerates
its observed points, then actually terminates the public command at each selected
point. It does not pass from an inventory alone or infer that an unreached point
was tested.

Logical mutations include state/target directory creation, lock seed publication,
owner and transaction/receipt records, staged payload files/copies/links,
pre-change backups, native per-target switching and rollback, unpublished-stage
cleanup, and confirmed purge journal, lock and entry/root deletion. A complete
unpublished payload copy and its fsyncs is one logical stage; no individual-byte
or syscall crash model is claimed. Private Git candidate input, explicit temporary
filesystem probes and trace telemetry are not additional durable runtime writes.

`AGENT_MEMORY_FAULT_TRACE` enables trace capture. `AGENT_MEMORY_FAULT_STAGE`
selects one actual point; `AGENT_MEMORY_FAULT_MODE=exit` exits with code 86 there.
An independent `AGENT_MEMORY_FAULT_ERROR_STAGE` raises an operation error at a
selected boundary, allowing the primary interruption to exercise its actual
error-cleanup branch. The error branch is reached through the ordinary record
writer, not a fake cleanup implementation. Every control is test-only: World
identity/token, possible write paths and trace destination are checked before
the lifecycle context can mutate state. Targets or traces in another World are
refused before bootstrap writes. Production creates no trace.

`install-response-lost`, `update-response-lost`, `uninstall-response-lost`,
`recover-response-lost` and `purge-response-lost` terminate only after the real
operation returns a verified committed result and before its public response.
They distinguish successful durable state with a lost acknowledgement from an
operation that never committed. Previews do not gain mutation authority from
these controls.

The install/uninstall sweeps use both copy and link modes. Recovery sweeps start
from real interrupted removal, covering rollback and committed receipt
finalization. Existing update sweeps cover real fast-forward activation.
Purge sweeps retain the exact original consent, include directories, generated
files and hardlink aliases, and retry that same confirmed operation; runtime
recovery never authorizes deletion. Separate cleanup sweeps reach an actual
failed record publication and interrupt its unpublished-stage removal.
Refusal-path sweeps interrupt the release of a purge lock with an invalid
confirmation. Retrying that same refused request releases only its proven stale
same-host lock after exclusive-lock/inode/byte checks; it remains refused and
leaves canonical memory unchanged and readable. A live, foreign, replaced or
modified lock is preserved. A detached quarantine retains its journal-bound
lock and pending evidence on refusal. Recovery from a refused request never requires
granting fresh deletion consent.

After each actual interruption the tests run public recovery/retry commands,
verify the reached trace event, compare the final owned target/source/receipt
semantics with a clean operation, and repeat the command to verify stable
receipts. Canonical memory remains byte/mode unchanged for runtime operations.
Purge verifies the consent-bound deletion outcome and retention of unrelated
paths. New transaction IDs/timestamps, private Git clone metadata and retained
rollback archives may differ; they are not treated as duplicate canonical memory.
Source revision/digest, real client identities, activation blocks, stable existing
installation ID and repeated receipt bytes are retained in the comparison.

The registered suites are separate bounded shards by operation, mode and phase;
each keeps the normal 300-second suite deadline. Their raw stderr records every
actually interrupted/recovered point as well as observed/executed totals. Empty,
skipped, unreached, failed and timed-out cases do not pass. The top-level gate
starts each suite in its own process group and stops remaining descendants
before removing the World, both on completion and timeout. A real worker/child
socket fixture verifies descriptor closure; a preliminary reproduction retained
a live child after the old direct-worker timeout and required explicit cleanup.

The full `scripts/test_gate.py --release --json` still requires all named v1
groups, including the separately tracked portable-lifecycle and complete
installed-bank roundtrip gates. These fault shards supplement those gates;
they do not clear pending groups or certify Linux/live services. Supported-runtime
matrix failures and observation timeouts remain release evidence to resolve.
