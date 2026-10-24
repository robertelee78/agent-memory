# Shared learning across sessions and machines

The first-release acceptance matrix binds the approved
[ADR](../adr/0009-contract-tests-and-fault-injection.md) and
[execution contract](../design/v0.0.1-execution.md) to public operations against
isolated real Git repositories. The bank remains one shared repository bank.
Machine profiles have separate local identity/configuration/state roots, while
each session uses its own clone or linked worktree and feature branch.

Three installed-command journeys add three participating clones, a linked
worktree, two developer identities and Claude/Codex/OpenCode/custom fixture
attribution. These are harness labels and isolated machine profiles, not live
client integrations or tests on physical separate computers.

- Compatible edits to different sections of one memory file are prepared against
  the same authority. A stale publication is refused, fresh reconciliation lands
  both edits, and another clone recalls both qualified lessons at its next
  successful boundary. A mismatched component cannot recall them. Repeating the
  refresh does not write duplicate memory.
- Two installed publication clients reach a real Git pre-push rendezvous with
  the same remote base. One lands and the other reports pending reconciliation.
  A fresh semantic review consolidates their differently worded duplicate
  lessons before retry lands, preserving a stable active identity, both origins
  and history. Real remote readback verifies the result. The fixture provider
  serializes its own metadata updates and publishes complete JSON atomically;
  the competing Git pushes and public product outcomes are real.
- A detecting developer refreshes while their unpublished yellow-wall guidance
  conflicts with another developer's landed blue-wall guidance. Provenance and
  the recommendation to seek clarification remain visible. Only disputed
  memory and explicit dependent work are held; independent recall and memory
  publication continue. The test sends no messages to either developer.

All three journeys snapshot feature files, the index, branch/head and unrelated
session notes. They verify that publication excludes feature ancestry and that
sharing leaves these states intact. Typed lessons in these journeys are existing
fixture data; automatic admission is separately exercised by the registered
capture/publish/refresh loop, including its installed CLI variant.

`tests/shared-learning-matrix.json` binds all seventeen approved obligations to
mandatory registered methods. Besides the new journeys, it retains automatic
durable capture, comparison before writes, deduplication before landing,
conditional lessons about unmerged code, evidence-based pruning that preserves
older-branch guidance, initialization from code/docs/ADRs and AWA evidence,
offline and lost-success-response recovery, later ordinary code integration and
terminal deletion without resurrection.

Bindings name coverage obligations, not extra executions or proof that an
assertion is correct. The binding test rejects missing methods, unregistered
suites and optional/pending substitutions. The public gate must actually execute
all registered suites; discovery alone, one selected journey or a previous
source revision cannot certify the complete matrix.

Run `scripts/test.sh --release --json` from the final integrated checkout with
Python 3.11+ and Git. Each suite retains the ordinary 300-second deadline and
requires nonempty execution without skips or expected failures. All failures
and timeouts remain evidence. Milestone release acceptance additionally requires
every approved story's independent verdict and delivery evidence, plus the
actual published release.
