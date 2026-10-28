# Dispute capacity: public reproduction and root cause

Issue #203, ADR-0018. Research began from
`bdfadd269799b3f487f336270d5e4092dc9f0c07` without modifying runtime code.
All private state below was in disposable `World` profiles. Templates came from
actual public v2 source inspection, contradiction review and attributed
clarification review. Bulk templates were deterministic structural replicas of
that fixture evidence, not claims that thousands of distinct real disputes had
been reviewed. No consumer state or installed source was seeded.

## Existing behavior reproduced

| Dimension | Supported boundary, accepted by public review | Excess from valid current state | Old public diagnosis |
| --- | --- | --- | --- |
| Dispute records | 4,095 to 4,096 | 4,096 to 4,097 | `semantic-review-unverified` |
| Predecessor history per record | 4,095 to 4,096 | 4,096 to 4,097 | `semantic-review-unverified` |
| Complete sealed registry | Exactly 16,777,216 encoded bytes | Exactly 16,777,217 proposed bytes | `semantic-review-unverified` |

Every excess left the registry, recovery copy, anchor, canonical bank, original
index and HEAD unchanged. Each public boundary/excess operation took between
5 and 15 seconds in the bounded apparatus. Limits were not lowered. The byte
fixture measured the real extra-record encoded size, then padded schema-valid
cause reasons within their existing 4,096-byte limit; it measured the complete
sealed registry, including its checksum.

The raw old response called the review "incomplete, changed or unsafe" and
suggested inspecting/repeating review. That loses the exhausted dimension and
cannot explain that replaying recovery will not create more capacity. The
refusal preserved evidence; this reproduction demonstrates a diagnostic defect,
not data loss.

## Cause and bounded correction

`sharing_holds.registry_value` applied one validation path both to persisted
state and to a new successor derived from verified current state plus an actual
validated review. Count and byte checks were grouped with schema/checksum
failure; predecessor-history length was grouped with incomplete history. These
raised ordinary `ContractError`, which the public semantic interface correctly
redacted into its generic unsafe-review response, hiding the recognized limit.

ADR-0018 distinguishes those two trust contexts. Persisted readers retain their
existing safety/recovery guards. Only the complete proposed successor can first
finish structural/checksum validation, then report typed record/history/byte
capacity exhaustion. No limits, retention policy or recovery ownership rule
change. Exhaustion must precede journal replacement and preserve all existing
learning and predecessor evidence. Exact-boundary replay remains a no-op;
rejected-excess replay remains the same typed refusal. Multiple excesses use
record, then history, then sealed-byte precedence.

## Apparatus and evidence

Portable controllers and compact immutable old-result evidence are under
`docs/research/data/2026-10-28-dispute-capacity/`. The summary records original
controller hashes, original runtime hash, limits and raw private proof locations.
Command summaries retain exit/timing and structured-output hashes; raw large
proofs remain in the referenced private directories. These are authored fixture
comparisons and mechanical capacity tests, with no model semantic-quality claim.

The first count apparatus inherited 142 separate page CLI reloads for one
already complete manifest. It was stopped and retained as an incomplete
apparatus run, never counted as acceptance. The bounded version reads all actual
public manifest source rows and known descriptors once, authors the complete
fixture judgment and acknowledges all exact page hashes. It still exercises
public `inspect-v2` and `review-v2`; no runtime gate, source check or limit is
bypassed.

Focused contracts in `tests/contracts/test_known_hold_capacity.py` qualify the
reviewed correction separately from these retained old failures. They cover the
three real public boundaries and excesses, idempotent exact/excess retries,
retained-state hashes, malformed proposed and persisted state, interrupted
persisted state, and multiple-limit precedence. Final qualification results must
be bound to the exact candidate source separately; the old reproductions above
do not claim the corrected implementation passed.

The first contract apparatus run is also retained, separately from runtime
qualification. Its two clarification-based cases incorrectly continued to label
the wall pair contradictory while proposing an explicit resolution; the existing
semantic guard refused that invalid review before any capacity seed. The fixture
was corrected to review those interactions as related after stronger evidence,
matching the existing known-holds contract and successful original reproduction.
A top-level imported `TestCase` also caused five existing tests to be collected
again; changing that import to a module alias restores the intended five-case
suite. Neither apparatus correction changed production code. The retained first
run completed ten collected tests in 285.993 seconds, with those two fixture
failures and eight passes; it is not accepted qualification of the corrected
candidate.

## Corrected macOS qualification

The intended five-case suite passed once in 199.306 seconds against the runtime
from `31a8a35` and v0.0.10 skill metadata from `b2e7025`. Exact runtime/test/skill
hashes and each test's duration are retained in
`data/2026-10-28-dispute-capacity/macos-corrected-contract-result.json`.

Three cases use actual public inspect/review/check commands for the individual
record, predecessor-history and sealed-byte boundaries, excesses, identical-proof
success and refused-excess retries. A fourth case exercises public inspection of
persisted checksum damage, oversized state and interrupted recovery. The fifth
is an **internal validator control**, using real-review-derived candidate shapes
to test combined-dimension precedence and malformed proposed data; it is not
claimed as an end-to-end public review. Original bank/HEAD/index and retained
private-state hashes are asserted throughout. The corrected result contains
exactly five collected tests, with zero errors/failures and no timeout passes.
