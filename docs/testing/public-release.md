# Manual public release checks

`scripts/evaluations/public_release.py` checks the real anonymous public channel
in disposable World homes. It deliberately allows HTTPS for its own fixture Git
transport. It is separate from the offline contract gate: it requires network
access and an actual published release, and never uses production credentials,
client configuration, banks or signing material. Run with assertions enabled,
from a trusted frozen source checkout, with private output outside that checkout.

Before promoting main, prepare two distinct baselines on each supported platform:

```sh
python3 -B scripts/evaluations/public_release.py prepare-legacy --source . --out /tmp/am-public-old-one-line
python3 -B scripts/evaluations/public_release.py prepare-legacy --source . --out /tmp/am-public-old-cli
```

Preparation executes the current public one-line installer, requires a managed
v0.0.2 installation from official main, and retains its isolated World and exact
receipt. Each World includes staged consumer code, a separate explicitly bound
consumer bank and existing client instructions. Preparation is not evidence of
a v0.0.3 upgrade or publisher authentication.

After the publisher has verified signed public assets and default activation,
replace `FULL_RELEASE_COMMIT` below with that exact signed source identity:

```sh
python3 -B scripts/evaluations/public_release.py upgrade-legacy --source . --out /tmp/am-public-old-one-line --expected-source FULL_RELEASE_COMMIT
python3 -B scripts/evaluations/public_release.py upgrade-via-cli --source . --out /tmp/am-public-old-cli --expected-source FULL_RELEASE_COMMIT
python3 -B scripts/evaluations/public_release.py fresh --source . --out /tmp/am-public-fresh --expected-source FULL_RELEASE_COMMIT
```

The first upgrade fetches the public bootstrap anew. The direct-CLI journey
instead executes the already installed v0.0.2 updater, requires its first
transition to remain explicitly unsigned/managed, then invokes the newer updater
to adopt the signed source. The old executing process cannot authenticate its
own first transition. The [security contract](../contracts/lifecycle-security.md)
defines that boundary.

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

## Current evidence

Four public v0.0.2 preparations succeeded on macOS/Linux at default source
`31f10a29fcd2e03c668cdc9fc5f163ea45dc8cb5`. Their
[source-bound summaries and apparatus reviews](../research/data/2026-10-26-public-lifecycle/)
retain the two original test-evidence gaps, their repairs and final Opus static
approval. The optimized-Python guard also refused before any product command.
The public v0.0.3 journeys and release delivery remain pending; preparation and
static approval do not substitute for those executions.
