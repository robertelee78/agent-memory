# Manual public release checks

The public-release evaluator checks the real anonymous public channel in
disposable World homes. It deliberately allows HTTPS for its own fixture Git
transport. It is separate from the offline contract tests: it requires network
access and an actual published release, and never uses production credentials,
client configuration, banks or signing material. It runs from a trusted frozen
source checkout, with private output outside that checkout.

The evaluator has no Rust port. It is an unported evaluation obligation in
`tests/obligations/python-baseline-v1.json`; until it is ported, none of the
journeys below can be run against the Rust release, and earlier results qualify
only the releases they observed. The journeys remain the release obligation.

Before promoting main, the evaluator prepares two distinct legacy baselines on
each supported platform: one later upgraded by the one-line installer and one
later upgraded through the installed CLI. Each World includes staged consumer
code, a separate explicitly bound consumer bank and existing client
instructions. Preparation is not evidence of an upgrade or publisher
authentication.

After the publisher has verified signed public assets and default activation,
the upgrade journeys and a fresh installation run against that exact signed
source identity. For v0.0.15, an installation of v0.0.14 or older cannot update
itself: its upgrade journey re-runs the trusted bootstrap, which adopts the
existing installation. The old updater's refusal of the Rust release, with its
exact message, MUST be observed with the real v0.0.14 runtime before
publication. The [security contract](../contracts/lifecycle-security.md)
defines the authentication boundary.

Final checks require the exact release source, fresh pinned-signature observation,
portable-only installed layout, no development bank or Git metadata, a verified
no-op update, successful uninstall with no unexpectedly retained runtime targets,
restored client text and unchanged consumer repository bytes/modes including its
staged index. Intended receipt-listed rollback backups are separate retention.
The test removes its owned World only after successful readback.

Each prepared World permits one transition attempt. A changed receipt or prior
attempt/output/failure refuses reuse; never overwrite a failure with a reinstall
and call it a successful upgrade. Raw command streams and attempted operation
records remain outside the checkout. Retain failed Worlds for inspection. Keep
the trusted support source available independently of any other job's temporary
cleanup. Do not point these commands at a real user home or live bank.

## Original v0.0.3 preparation evidence

Four public v0.0.2 preparations succeeded on macOS/Linux at default source
`31f10a29fcd2e03c668cdc9fc5f163ea45dc8cb5`. Their
[source-bound summaries and apparatus reviews](../research/data/2026-10-26-public-lifecycle/)
retain the two original test-evidence gaps, their repairs and final Opus static
approval. The baseline runtime's optimized-interpreter guard also refused before
any product command. These are historical preparation records, not the current
release status. Preparation and static approval do not substitute for actual
delivery checks; consult each release's source-bound qualification report for
subsequent results.


## Signed-to-signed release checks

Signed-to-signed checks prepare a signed baseline once on each supported platform
before publication, then, after publication and verified activation, pass the
exact signed release commit as the expected source to the installed-CLI upgrade
against that retained output and to a fresh installation in a separate new output
directory. A failed attempt is retained, never overwritten. Run these probes in
disposable profiles, not operator homes. Each public command stream and its
bank/code/index preservation are checked independently of source-contract
qualification. From v0.0.15 on, both sides of such a check are Rust releases;
v0.0.14 → v0.0.15 is the bootstrap adoption journey above.

A setup error before a product assertion is apparatus evidence, not a runtime
pass or failure.

## Disposable #244 publication and retention qualification

The recovery publication/retention tests use the built public `am`, exact
79,054-byte synthetic authority context, real bare Git transport, source ledgers,
separate review inputs, original feature/index/HEAD checks, and a second clone
with an empty private recovery profile. The offline GitHub protocol adapter is
only apparatus for exercising policy branches: **it is not actual hosted
permissions, independent semantic acceptance, macOS/Linux qualification or a
release-delivery receipt**. These commands are a runnable recipe, not a claim
that this revision has passed.

From the trusted frozen checkout, run the publication and layout-recovery test
targets; they create their own disposable Worlds. The ledger records the
baseline publication and retention suites as only partially ported:

```sh
cargo test --features dev-tools --test t2_publication --test t2_layout_recovery
```

The publication test repairs the local layout first; retains independently
source-bound all-side review; refuses an omitted source, unresolved frontier,
offline authority and stale local result; builds the exact authority-derived
memory candidate; lands through the existing policy engine; inventories exact
retry; and inspects/recalls in a separately bound empty-profile second clone.
Raw-source controls separately prove that oversized/unsupported sources can be
observed without a successful normal snapshot and reject overflow, portable
aliases, redirects and committed runtime holds. Candidate controls inspect the
entire tree delta and preserve the original index.

The retention test uses a completed real layout receipt, separately reviews an
exact dependency/file preview, injects an unlink-before-progress failure,
resumes the same cleanup UUID, and reads the actual historical layout/cleanup
receipts. Pending units, unknown backups (including empty directories), unsafe
modes/aliases, changed private bytes and impossible missing-file states refuse.
Cleanup is not recursive and never claims removed originals remain verified.

For final acceptance, retain exact commands, source/runtime identity, stdout,
stderr, exits and private preservation manifests on **both macOS and Linux**.
Repeat the publication flow in a controlled real hosted repository using its
actual permissions, reviews and checks; required human approval MUST remain
pending until genuinely supplied. Never use the protocol adapter, mock network
responses or a passing small primitive test as that hosted-policy evidence.
Use a fresh second clone/profile with no copied publisher ledger, no committed
runtime marker and normal explicit project binding. Verify qualified recall and
ordinary capture after fully terminal publication; local repair alone is not
publication or cross-machine import.

The standard release check remains `cargo test --features dev-tools` on each release triple for the
integrated immutable candidate. Do not run these workers concurrently with
milestone performance/stress qualification or enlarge their existing deadlines.
