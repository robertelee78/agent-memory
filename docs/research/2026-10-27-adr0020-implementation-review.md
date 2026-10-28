# ADR-0020 independent implementation review

Design source: `38ca9facc028d4180ffca58a41d3fc131f34ca54`.
Native callback evidence refinement:
`c97a4c22985ac73397b43fc9dfe887f9a90d15f4`.
Reviewer: independent Codex review agent; the reviewer did not implement the
production code and authored one requested legacy retry regression.
These verdicts cover the exact working-file SHA-256 values below. They do not
establish live client qualification, cross-platform acceptance, production
activation, signed publication or release readiness.

## T1 and Codex T3: ACCEPT

Append-only exclusive source reservations remain independent of the nonblocking
completion lock. Each observation retains its own immutable source digest;
distinct and identical-text submissions receive separate observation UUIDs.
Completion validates the exact observation, rechecks assessment/publication
evidence under the lock, and cannot cover a different source. Encoded output
bounds precede publication. The durable pending index and post-cleanup inventory
recheck retain racing or interrupted sources. Bounded stdin accepts no-learning
only; publication retains its independently verified file path.

Initial review blocked FIFO reads that could hang before file-type validation,
invalid index references accepted as occupied slots, missing prior-pending Stop
notices, and incomplete legacy pending validation. Re-review confirms nonblocking
file opens, a complete bounded index validation pass (including holes before bad
references), concise prior-pending notices, final current-inventory rechecks, and
exact legacy pending schema/raw-digest validation. A further legacy Unicode
encoding issue now has a prewrite 4096-byte bound and readable UTF-8 encoding.

Reviewed limits are 128 observations per turn, 4096 pending-turn slots, 4096-byte
source records, 65536-byte v2 completion/stdin bounds, 60000-byte backlog pages,
2048-byte guidance and at most two durably reserved wakes per native lifecycle.
Completed Stop is quiet when there is no new pending notice; unavailable evidence
remains pending. Legacy completion does not establish coverage of v2 sources.

| File | SHA-256 |
| --- | --- |
| `skills/agent-memory/scripts/agent_memory/turn_sources.py` | `48d3b9f9c5d7f0aedb29f48e79adec0c186092ab26d7fed7a02b295b9d131edf` |
| `skills/agent-memory/scripts/agent_memory/codex_boundary.py` | `a64c851902793b9d0a6f2c7c14742fe80ddd6624e41bfd139b31669add8bb39f` |
| `skills/agent-memory/hooks/codex_prompt.py` | `e2c216f9f320d003261b228428294b543b4f0376dd1035028254caf4808bd1c4` |
| `skills/agent-memory/hooks/codex_stop.py` | `084120fada92f6073935eacd2d644127d3eab9728cdbc37f70725716b439c97d` |
| `tests/contracts/test_turn_sources.py` | `050d95e50a86fb5cdba27aea0f4d45f327e19aa99e4890871ce153cf760e7933` |
| `tests/contracts/test_codex_stop.py` | `4c456c6fd682edf701b430988884994f300aa41635557e77e875f38b290f3ce5` |

The reviewer independently ran the original two focused suites: 11 tests passed
in 38.762 seconds. The original FIFO hang and valid-JSON invalid-index acceptance
were separately reproduced in disposable state. Independent post-fix checks
passed: four targeted FIFO/invalid-index, cleanup-race, Unicode legacy completion
and prior-pending Stop tests in 9.143 seconds, plus the index-hole-before-invalid
reference regression in 1.470 seconds. No broad suite was repeated.

## T2: ACCEPT

The imported-snapshot exception requires exact cursor and actual completed
receipt binding, complete current path/hash equality, and a new remote observation
whose verified base/local/landed snapshots agree. Receipt coverage is recomputed
from the freshly observed local snapshot. Only the untracked-canonical blocker
and old-HEAD/current-bank equality gates are relaxed. Complete HEAD/index equality,
ordinary modes/stage zero, scope/source/cursor/index/recovery pins and final remote
head checks remain. No staging, canonical writes, branch movement or importer
creation-provenance claim is introduced by the quick check.

| File | SHA-256 |
| --- | --- |
| `skills/agent-memory/scripts/agent_memory/memory_refresh.py` | `7a87c0db6f3a8b16a945825cf04c0ce3df5ba907c0c01d64595e39940a27c7ae` |
| `skills/agent-memory/sharing/refresh/GUIDE.md` | `b5a19bc05fff18a86c2d83ea29adb0cba1679e0fdfc94b04f6cae0c231463123` |
| `tests/contracts/test_refresh_receipt_snapshot.py` | `13da9d28cb8714e8e7cd1b3878dade2b5b5f405479daad72c1a2b7d5b1c202b9` |

Independent command, run from `tests/`:
`python3 -m unittest contracts.test_refresh_receipt_snapshot`.
All four tests passed in 134.345 seconds. They exercise real import then quick
current, full no-op then quick current, imported deletion, local mutations,
staged memory, authority changes, damaged/mixed receipt/cursor and branch pins,
with feature/index/bank preservation checks. Existing final-source/mode/index/
authority race guards and hold/journal checks were also inspected in code and
their existing contract tests; those broader tests were not rerun by this reviewer.

## Native Claude T3: ACCEPT

The native prompt hook retains identifiers, digest and a private transcript locator
before transcript publication. Under the reviewed callback evidence refinement,
the direct callback is the mechanical source witness for v2 observations. First
completion checks exact retained context, observation and digest; it does not
require a transcript row or invent a native message UUID. The agent's actual
source assessment remains an assertion. Earlier or interrupted source recovery
requires original text/context and an independent digest match; unavailable or
ambiguous evidence must remain pending. An exact completed retry can use its
existing verified receipt. Stdin permits no-learning only. Source and ledger FIFO
reads reject nonregular state without waiting. Native continuation guidance retains
the substantive task answer, limits message size and uses the shared durable wake
reservation; unavailable tools do not authorize invented completion.

Initial follow-up findings concerned incomplete legacy completion validation,
Unicode escape expansion beyond the legacy reader bound, and interrupted retry
compatibility across encoding changes. Re-review confirms strict receipt
fields/outcome/assessment/digest validation, retained pending-byte/source binding,
a 4096-byte UTF-8 encoded-output bound and semantic exact-record comparison for
old escaped records. The requested independent regression creates a v1 escaped
Unicode completion alongside retained original pending bytes, retries the same
assessment, and verifies cleanup without rewriting completion evidence. It passed
in 2.288 seconds. Other Claude contract results supplied by the implementer are
separate evidence; the reviewer did not rerun those passing suites.

The updated prompt enumerates the current observation protocol's exact top-level
fields, and CLI failure output directs the agent to the distinction between new
observation IDs and legacy source UUIDs. The shared guidance explicitly preserves
task answers and suppresses routine bookkeeping narration.

| File | SHA-256 |
| --- | --- |
| `skills/agent-memory/hooks/claude_prompt.py` | `46d4f51b18c56e7284b7fa96de392cebfeec8306dc0c64e481e8696b92e7a616` |
| `skills/agent-memory/hooks/claude_stop.py` | `e2b26d486b6dbc8e4da372bb83299a263791925dc78ea52d4609a8219f2fffa9` |
| `skills/agent-memory/hooks/claude_source.py` | `6bf1777ae03a28e9a73f39349cb3944998f7f98eae6a30f576894f922a659648` |
| `skills/agent-memory/scripts/agent_memory/claude_turns.py` | `464dd31efb437522408b6f70cd0a6821e12ad2c622c912a5601735517e0bf04b` |
| `skills/agent-memory/scripts/agent_memory/claude_boundary.py` | `875138b8cf4b3f9e7e5d0f42fb8ff6e8aa0e5b9a814bce577fd61b0791123e6a` |
| `skills/agent-memory/hooks/boundary-prompt.txt` | `be519ecf51d59ad84e16b52bf11b9458aa7098bef5bb60e1746e79754547527f` |
| `skills/agent-memory/hooks/turn_hook.py` | `af1ea9e70874caf3eb03f80d8c15280b8dd059229eb826fe5547e74a068abe9c` |
| `skills/agent-memory/turn/GUIDE.md` | `e0df0928fd9dc278e0c9edd3b91d0fc0f8f2103b018b3883d68f70285300efff` |
| `tests/contracts/test_claude_source_turns.py` | `fe26d7dd41425e38e784776bc497766c3bfc47228c248612d5e5664271f06fa9` |

The follow-up Stop wording explicitly requires a continuation's final response
to repeat the substantive task answer, permitting only a brief pending
qualification; empty text or bookkeeping alone is prohibited. The reordered
observation protocol's assessment-schema cross-reference now points below.
These final wording changes were read and accepted at the hashes above.

An actual streamed-input follow-up later demonstrated three callback observations
sharing one native prompt ID. Only the first source appeared as a plain-text user
transcript entry; queued follow-ups used private queue-removal records without
that prompt ID. The former mandatory transcript projection rejected legitimate
observations. That run remains failed pre-refinement evidence. Independent review
of selected row metadata confirmed that queue ordering cannot prove a binding
between callback observations and native message UUIDs.

The reviewer independently accepts refinement `c97a4c2` and the exact implementation
hashes above. V2 source/request fields remain strict and exclude `source_uuid`;
the retained native-source hint is only lifecycle correlation. V1 transcript-UUID
requirements and records remain unchanged. Wrong source digests, unknown IDs and
cross-context lookups cannot establish coverage; assessment/publication gates are
unchanged. The implementer's updated five-contract run passed in 16.247 seconds,
covering completion without a later transcript and three distinct local
observations for distinct/identical callbacks, with separate completions reducing
pending counts from three to zero before quiet Stop. This suite was not rerun by
the reviewer.

The external Opus review of this refinement is conditional ACCEPT, not an
unconditional implementation verdict. Its conditions include strict v1/v2 fields,
observation identity rather than prompt-ID identity, actual original-source and
context recovery with digest verification, interrupted/undelivered inputs remaining
pending until assessed, and no mandatory reconstructed transcript identity. Those
mechanical field/coverage boundaries and guidance obligations are present. Actual
source recovery and semantic assessment remain agent obligations rather than
mechanically authenticated claims; callback existence or a hash alone cannot
establish them. Live queued-source and ambiguous-recovery qualification remain
separate from this static implementation acceptance.

A later queued live attempt failed when the model relied on zsh word splitting
and submitted a combined word list as an observation ID with an empty digest.
That failure remains recorded; it did not establish completion. The final
hardening now validates one UUID and an exact lowercase 64-character digest
before lookup, with actionable field-specific errors. Guidance requires one JSON
object per observation, recommends a structured encoder and prohibits dependence
on shell word splitting. The reviewer read and accepts these small changes and
their malformed-input regression at the updated hashes above. Their targeted
execution and corrected queued live attempt were still in progress when this
static rebind was recorded; no passing result is inferred.

The implementer separately reports a 36.76-second native interruption/recovery
run that completed current work while retaining an earlier source whose original
evidence was absent. It also reports 66 Linux contract tests passed in 63.82
seconds before the final input-validation hardening. These are implementer-reported
results rather than independent reviewer execution, and do not turn the failed
queued attempt into passing evidence.

This is implementation review only. Earlier live Claude observations reported by
the implementer used a synchronous controller Stop configuration and do not
qualify the installed asynchronous `asyncRewake` configuration. The implementer
subsequently reports two normal turns under the production asynchronous settings
(20.85 and 10.61 seconds), task-only final answers, matching completed receipts
and quiet Stops. A restricted-tools attempt then produced an empty SDK result
after a substantive initial answer; its source remained truthfully pending.
After the explicit final-answer-preservation wording change, the implementer
reports a successful restricted-tools repeat in 7.5 seconds with the substantive
answer and brief pending qualification, one bounded fallback and retained pending
source. The earlier failure is retained as a failed pre-fix observation. These
live results are implementer-reported evidence, not independently replayed by
this reviewer; interruption/resume and broader release qualification remain
separate requirements. No complete live acceptance is asserted here.

## Native OpenCode T3: ACCEPT

Native prompt enrollment now precedes the completion/context callback and retains
the original session/message identifiers. New and existing enrollment paths
verify durable evidence, namespace identity and repository scope before acceptance.
Queued work retains the native callback model and does not claim same-turn Codex
steering. Wake reservations are durable before queue submission, serialized with
completion, limited to one per native source fingerprint and at most two per
native lifecycle. Completion and recovery retain original source attribution;
cross-session recovery checks the same canonical repository and re-observes the
original native source window. Unavailable or interrupted windows stay pending.
Marker and lock FIFO paths are nonblocking and reject unsafe state.
The final helper also rejects private state inside the active checkout, common
Git directory or conventional linked-worktree main checkout before creating
ledger files. The reviewer read that guard and independently ran its no-write
regression: one test passed in 2.035 seconds. Its updated helper/test hashes below
supersede the earlier pagination-only hashes. The implementer reports the expanded
19-test suite passed in 8.69 seconds.

Initial independent review found that the first 16 pending sources could hide
later retained sources indefinitely when earlier evidence was unavailable.
The corrected helper and `am.pending_sources` expose validated bounded offsets,
stable sorted pages, `next_offset` and an inventory digest. The reviewer read the
final implementation and independently ran the 21-source pagination regression:
one test passed in 0.031 seconds. The implementer separately reports 18 focused
native/queue/legacy ledger tests passing in 5.728 seconds; the reviewer did not
repeat that passing suite.

Reviewed bounds include 4096-byte marker readers, 2048-byte generated guidance,
two durable wake records, 16-item discovery pages, at most 4096 source IDs and
4096 session directories in a backlog inventory and 20000 scanned entries per session.
Capacity or malformed state returns pending. Legacy source uncertainty is counted
without manufacturing missing original native IDs. Existing source-window tools
retain their separate 16 KiB evidence limit and truncation qualifications.

| File | SHA-256 |
| --- | --- |
| `skills/agent-memory/hooks/boundary_ledger.py` | `aa899b8f0cc93d28dedf02b4e27fac34f78aad4694fa76d11028951b713de6b9` |
| `skills/agent-memory/hooks/opencode-v2-queue.js` | `d9d0ee35739e6672b60f663f19e2b0eed7c552a4f3273204c814bbbafcf0c2e0` |
| `skills/agent-memory/hooks/opencode-v2.js` | `0a6f44d842940840325fb21d54df1b31e547903cd4d1dc638e6cb0c6cd0ea19a` |
| `tests/contracts/test_opencode_native_boundary.py` | `b2f7e6dca9f01dd68603777b15244591c025c4b362000cd9b80630bb71a06400` |

This verdict is static implementation acceptance. Native callback mocks and
helper tests are not live model qualification. An earlier actual client attempt
reported two native source IDs enrolled while the model stalled at step start;
that attempt demonstrates enrollment only. The implementer subsequently reports
two ordinary Kimi turns succeeded; detailed final-source/live edge-case evidence
is still being qualified. Required repeated live answers,
truthful completed assessments, final-answer preservation, bounded unavailable-
tools behavior and interruption recovery remain to be demonstrated on the actual
installed client. No OpenCode live acceptance is asserted here.
