# ADR-0020 independent delivery review

**T4 delivery verdict: ACCEPT** for signed v0.0.8, release source
`c92b605fb09f2058f9258258bbec00f41c76eeb7`.

The previously outstanding public delivery checks now have passing receipts on
macOS and Linux. Together with the accepted
[implementation review](2026-10-27-adr0020-implementation-review.md) and qualified
[prepublication live review](2026-10-27-adr0020-live-review.md), the evidence
supports T4 Acceptance for this exact release. This review does not itself write
tracker state, complete later memory publication, or claim that every old source
has been assessed.

## Published source and public canaries

The reviewer independently read the public GitHub release metadata, tag reference
and downloaded release manifest. The public non-draft
[v0.0.8 release](https://github.com/robertelee78/agent-memory/releases/tag/v0.0.8)
and tag resolve to the exact source above. Its manifest records artifact digest
`cbf4c9db4f04babe9163d16d21258f6ec9804edc6500257894693ab35d27e714`.
The manifest's archive SHA-256
`6a3a1ec166dd26b9ff9745bdb142f5f7397f53b928eacb33c1dd5d25eab2ec79`
matches the public GitHub archive asset digest. The installed actual source
manifest also matches that public manifest. Publisher authentication/signature
verification is established by the retained canary and actual-install receipts;
this reviewer did not rerun their signature verifier.

There is no runtime, script or test difference between the previously reviewed
candidate `a71c102a7a48c88a5368c2769eb10efcc8bddb9d` and release source C92.
Later documentation or memory commits are separate from this immutable release
binding. The publisher receipt records main activation at C92 at publication;
it is not a claim that main can never advance afterward.

All four actual-public canary receipts report the expected C92 source, verified
publisher, development-memory exclusion, preserved consumer memory and staged
code, verified public no-op update, restored client text and successful uninstall.
The reviewer also read the canary implementation's assertions. Fresh installs
exercise the actual public default channel; upgrades exercise the existing CLI.

| Platform | Fresh public install | Signed 0.0.7 CLI upgrade |
| --- | --- | --- |
| macOS | PASS, 16.239 seconds | PASS, 18.056 seconds |
| Linux | PASS, 16.947 seconds | PASS, 19.385 seconds |

The sanitized baseline artifacts establish both old installations as verified
release 0.0.7 at `9e1a497d67203f0918c66276671b672f988d7630`, with retained
manifest hash and actual prior-version output. Their command records show the
existing `am update` and all subsequent version/authentication, no-op, status and
uninstall commands exiting zero. Update output explicitly records 0.0.7 to
0.0.8. These are completed transitions, not merely prepared legacy fixtures.
The reviewer additionally inspected the original macOS baseline/command records;
Linux review used the retained sanitized counterparts.

## Actual activation and limits

The actual installation reports verified release 0.0.8 at C92. Its subsequent
no-op exits zero in 5.929 seconds. The approximate 130.26-second initial actual
upgrade measurement is explicitly log-timestamp based and unprofiled; the short
disposable canaries do not explain its cause. Doctor reports ready, publisher
authentication verified, signature verification performed, and no warnings or
failures.

The actual installed native Codex check exits zero in 39.946 seconds, returns
only `4`, and records an explicit assessment of its exact observed source with
verified no-learning completion. Its refresh result is freshly
`bank-synchronized` against C92, with no canonical or original-index writes.
The retained source digest matches the recorded native input. Assessment remains
an agent assertion; neither this arithmetic check nor its receipt proves semantic
truth, durable admission or publication.

**Three older legacy observations remain pending and preserved.** Current v2
pending is empty and pending faults are empty. This is accurate backlog
reporting, not retroactive completion or relabeling of old sources. Native UI
visibility, OpenCode's partial timed-out resume and generic-error presentation,
mixed historical live-test source maps, synthetic/local-authority limits and the
earlier failed trials retain the qualifications in the prepublication review.
No universal model-compliance claim follows from this delivery acceptance.

## Receipt hashes

All paths below are relative to `data/2026-10-27-v008-delivery/`.

| Receipt | SHA-256 |
| --- | --- |
| `publisher.json` | `0a29b7a43f22cabfb9e8d3c7aad8183e703b8c2c3096cfb3d45328b9d7a748e2` |
| `mac-fresh.json` | `87ed093fe5c2197a0f38ed778ad8a02191e5825db5e9c127fde3e82867625c96` |
| `mac-upgrade.json` | `4f750d90545ed6588135b7244767e877fc64a3c70a23c39e387781594551b79b` |
| `mac-upgrade-baseline.json` | `11a68142d04f654203e8fe3e30d0dd6e6dc81db3f2f4a4a1a005e25a199d0cce` |
| `linux-fresh.json` | `a338619a01e8de90f27c59294d6b068f839b9e9af21456bd877228325d327f95` |
| `linux-upgrade.json` | `ff27e338d3644ff243059f90ff373133e8353dbafbf262a4f988002cfa24a96b` |
| `linux-upgrade-baseline.json` | `d7294d260f4e7147b90341f5ded5669dfdd5e956ecb43b698d391aa324d507bc` |
| `actual-activation.json` | `fb03f24267d4301de9cb26dd9ea813771dd3fee401458485fa2a498ba2332a90` |
| `actual-native-codex.json` | `78e36e59771e1a8378bcb0d9ce6f802cfe0a57917edaf938c19c3313653d0218` |
