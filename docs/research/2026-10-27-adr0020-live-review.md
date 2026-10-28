# ADR-0020 independent live qualification review

Runtime source: `a71c102a7a48c88a5368c2769eb10efcc8bddb9d`.
ADR SHA-256: `94eaa1c2df83063c9abaf6e8c5326e4909d14c0efb8c8228be3f6ca863453527`.
Reviewer: independent Codex review agent. This review inspects retained evidence;
it does not claim the reviewer personally executed the native live controllers.

**Prepublication technical criteria: ACCEPT, with the evidence limits below.**
**Complete T4/release acceptance: pending delivery checks.** Signed public
fresh-install, update, no-op and consumer-memory isolation checks must run against
the actual published bytes on macOS and Linux before T4 can complete. Prepared
v0.0.7 consumer fixtures are preparation, not those passing delivery receipts.
This verdict authorizes no claim of completed release delivery or production
activation.

## Native behavior

The reviewer read the three native reports, their structured summaries, completion
and pending evidence, selected native events, source-hash maps and the applicable
ADR criteria. These are actual installed-client observations, supplemented by
contract tests; native queue behavior is not relabeled as Codex steering.

| Client | Supported conclusion | Qualification |
| --- | --- | --- |
| Codex 0.165.0, gpt-6.1-sol | Three native steering calls under the first active turn yield four distinct completed observations, including identical text; a second turn completes separately. Native Stops are quiet. A genuinely blocked tool leaves memory pending with the substantive answer. Native interruption followed by a new session recovers and separately completes the exact original and current sources. | Native model/provider metadata and source coverage are retained. Controllers have bounded deadlines and normal termination. No durable publication is claimed. |
| Claude 2.1.306, claude-fable-5-1 | Two production-async ordinary turns preserve task-only answers and explicit completions. The final callback-witness run retains and completes three observations for distinct/identical callbacks sharing one prompt ID, with zero pending and a substantive final answer. The corrected no-tools case preserves its answer, one bounded continuation and truthful pending. Native interruption/resume completes current work while preserving the unavailable original observation. | Ordinary-turn evidence predates the callback-witness refinement. Final-source queued evidence uses the reviewed current implementation but streamed EOF has no native Stop-response event; quiet Stop is supported separately by native ordinary events and contract tests. Earlier transcript-gate, schema, shell-splitting and empty-result failures remain failed trials. |
| OpenCode 2.1.3, Kimi-K3 | Two final-source ordinary messages have distinct native source IDs, explicit no-learning completion, task answers and exit 0. Denied tools preserve the answer and pending source. Interruption retains the original native IDs; a resumed request produces a substantive answer and a distinct completed source while leaving interrupted work pending. Recorded wake counts show no repetition loop. | The resumed process reaches its 75-second deadline. That is partial process-resume evidence, not clean CLI completion. Earlier GLM stalls remain failed trials; the no-plugin control does not prove their precise cause. Unsafe enrollment blocks input, but the default CLI exposes only a generic 500; actionable detail is retained in the server log. |

ADR-0020 requires original attribution to survive interruption/resume and pending
recovery. It does not require every interruption experiment to exit cleanly, nor
does it require unavailable original evidence to become assessed. OpenCode's
retained original source and separately completed resumed source support that
narrow criterion. The two separate exit-0 ordinary runs establish ordinary
completion. Treating the timed-out resume as a clean completion would exceed the
evidence; requiring the unavailable interrupted source to be acknowledged would
contradict the pending-source requirement.

Claude's ordinary and no-tools source maps record precursor Claude helper/guide
bytes. The final queued source map matches the current reviewed Claude runtime
and guide; the final input checks reject malformed identifiers rather than
altering valid completion coverage. Codex adapter/common-ledger bytes are stable
across its live trials. The per-case maps must remain attached: this review does
not recast every earlier trial as an exact-current-package execution.

## Contracts, refresh and retained limits

The macOS focused contracts and independent implementation review cover bounded
registration, completion/Stop contention, late steering, partial/corrupt/legacy
evidence, capacity carryover, nullable final text, quiet completion and durable
wake limits. Linux retained logs show 66 contracts passing in 63.820 seconds and
seven refresh checks passing in 36.809 seconds. The broader Linux run predates
the final small Claude identifier-validation change; the exact final negative
validation contract separately passes on macOS in 3.716 seconds. This is the
actual scope of cross-platform evidence, not a claim that the final full suite
was rerun on both platforms.

Real Git-backed import then immediate quick synchronization, full no-op then
quick synchronization, imported deletions, local mutations, staged memory,
authority changes and receipt/cursor/branch guards were independently reviewed
and tested in the implementation review. The retained actual-root result also
reports fresh `bank-synchronized`, no canonical/index writes and authority
`711df384ab4420c5bb0347e6b990558d86e5c47a`. It leaves capture/publication unknown
and does not claim semantic truth or relevant-import recall. No-learning native
fixtures establish neither admission nor publication; those claims remain
separate.

The tests use disposable banks, local Git authorities and synthetic task prompts.
They do not establish production GitHub governance, universal model compliance,
invisible native UI, or source reconstruction when original evidence is absent.
Remaining production legacy observations and actual activation must be reported
at delivery. Every reviewed run is bounded below the ADR's 10–15-minute maximum;
the OpenCode timeout remains a timed-out experiment rather than an extra pass.

## Source-bound evidence anchors

Full artifacts are linked from the [Codex](2026-10-27-codex-boundary-live.md),
[Claude](2026-10-27-claude-boundary-live.md) and
[OpenCode](2026-10-27-opencode-boundary-live.md) reports. These critical final
artifacts were read at the following SHA-256 values:

| Artifact | SHA-256 |
| --- | --- |
| `data/2026-10-27-boundary-reliability/claude-queued-final/summary.json` | `bab0c958f3e6556139c49f369d51dcd0ba38abf4e2264f934b1f1cb73f39c046` |
| `data/2026-10-27-boundary-reliability/claude-queued-final/source-hashes.json` | `d54f47c8b171f248b5c3388110c1889d5c8cd49a5bc60e6a082dc00e03998067` |
| `data/2026-10-27-v008-opencode/final-native-summary.json` | `97a4f3a6d4a7b4ff8282731df2136fa8b237b94c92b36d13280ad0e7bc01416e` |
| `data/2026-10-27-v008-opencode/final-native-source-hashes.json` | `302b820d8c289590dd21227eb742efb39b175606ce280c6cdc88bd3ef25e4e33` |

T4 should remain Executing through publication and actual public-canary
verification. Those receipts require their own independent review before full
T4 Acceptance, Release ready or release closure.
