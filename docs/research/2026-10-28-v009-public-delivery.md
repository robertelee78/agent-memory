# v0.0.9 public delivery qualification

All four actual public-channel canaries passed on 2026-10-28. These were single
attempts against the published release, using disposable consumer profiles and
repositories. Neither the operator's installed profile nor its memory banks were
used by these canaries.

Published source: `07956881057fe29bb3160deaecc23b3e7abc9f93`.
Portable artifact digest:
`992510030a2e163e95d567954c95d6236d2c3ee2e9c22d6e60c3dacfbddbe78b`.
Release: <https://github.com/robertelee78/agent-memory/releases/tag/v0.0.9>.

| Platform | Actual transition | Result | Total seconds | Completed commands | Longest command, seconds |
| --- | --- | --- | ---: | ---: | ---: |
| macOS 27.2 arm64 | Default public one-line fresh install | PASS | 15.872 | 8 | 5.298 |
| macOS 27.2 arm64 | Signed v0.0.8 `am update` to v0.0.9 | PASS | 18.100 | 8 | 6.095 |
| Linux x86_64, glibc 2.43 | Default public one-line fresh install | PASS | 18.501 | 8 | 6.548 |
| Linux x86_64, glibc 2.43 | Signed v0.0.8 `am update` to v0.0.9 | PASS | 21.001 | 8 | 6.701 |

The four canaries ran concurrently; all 32 completed command exits were zero.
Every subprocess had the existing 180-second bound. No timeout was counted as
success, and no transition was retried or its evidence overwritten.

## What the checks establish

Each canary verified the exact published source revision, pinned publisher
signature and matching portable artifact digest. A fresh consumer installation
used the actual public default command:

```sh
curl -fsSL https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh | bash
```

The upgrade canaries used the installed signed v0.0.8 CLI's actual `am update`.
Before publication, both retained v0.0.8 receipts were checked against their
saved originals, with no prior upgrade-attempt evidence. Their original source
was `c92b605fb09f2058f9258258bbec00f41c76eeb7`.

All canaries checked human-readable version/status, authenticated JSON version,
current-version update check and no-op update, uninstall preview and actual
uninstall. Consumer memory, staged application code, HEAD/index and unrelated
client instructions were preserved; uninstall restored the original client
text. The installed source had exactly `skills`, `release-manifest.json` and
`release-manifest.json.sshsig` at its root, with no development `memory-bank` or
`.git` inherited by the consumer. Disposable worlds were removed only after
these assertions passed.

Controllers used `python3 -I -B`, and the installed CLI's verified executable
inventory matched the release artifact digest on both platforms. No bytecode
integrity exclusion was introduced or used. This records the validated installed
inventory before uninstall; it does not claim a post-uninstall scan of removed
source directories.

## Reproduction and retained evidence

Runner: `scripts/evaluations/public_release.py`, SHA-256
`3ae630243176d524844d59aafb4e2c03cde70667c45cde1bc12e3a27c45d0d54`
on both machines. Operations were `fresh` and `upgrade-via-cli`, with
`--legacy-version 0.0.8 --release-version 0.0.9 --expected-source` set to the
full published source above. Linux used the prepared source/test tree at
`/tmp/am-v009-linux-final-8jwzj12b`; macOS used the feature worktree. Runtime
payloads came from the published release rather than either test tree.

Original evidence remains on each host in `/tmp/am-v009-public-fresh` and
`/tmp/am-v009-public-upgrade`. Each includes command exit/timing/hash records,
stdout/stderr and the terminal result. The local collected proof is
`/tmp/am-v009-public-delivery-proof/summary.json`, with four platform/transition
subdirectories containing command records, version diagnostics and results.
The canaries qualify public delivery; cross-harness conversational behavior is
covered by the separate native qualification reports.

Durable [public command, result and version receipts](data/2026-10-28-quiet-boundaries/public-delivery/) are retained with this report. They contain only disposable fixture profiles; operator-profile recovery is a separate incident and is not conflated with these four delivery canaries.
