# Lean agent-memory: live omp qualification

Date: 2026-11-15. Real omp **18.20.1**, real **openai-codex/gpt-6-luna**, on macOS arm64 and Linux arm64.

**Result: macOS passed the functional journey; Linux failed the ask-before-saving requirement.** Both platforms loaded the extension, recalled the release codename, shared a note between clones, and answered the port question in a fresh session. This is not an overall acceptance pass: Linux persisted a decision before receiving permission. macOS also exceeded the 200 ms command target on both local shares.

No product source was edited, no unit tests were written, and nothing was committed or pushed to the product repository. Fixture pushes went only to disposable local bare remotes.

## Results

| Journey check | macOS arm64 | Linux arm64 |
| --- | --- | --- |
| Development install, omp preset, disposable HOME and agent dir | PASS | PASS |
| Bare remote, two clones, init, distinctive project fact, seed lesson and share | PASS | PASS |
| P1: fresh session answers HERON-7, with injected session-start message | PASS | PASS |
| P2: suggests saving, asks approval, does not save | PASS | **FAIL: saved immediately without asking** |
| P3: approved note exists, linked in MEMORY.md, recall finds it | PASS: new note written after approval | **FAIL as an approval journey**: note already existed from P2; P3 merely acknowledged it. File/link/recall checks passed. |
| Share from A, refresh B, P4 fresh session answers 7443 | PASS | PASS |
| Session-start hook under 100 ms | PASS: 12.086 ms | PASS: 0.686 ms |
| No agent-memory turn-end handler or worker | PASS, loaded extension source | PASS, identical extension source |
| Status and doctor | PASS, “No problems found.” | PASS, “No problems found.” |
| Uninstall removes owned omp guidance, extension and skill; banks preserved | PASS | PASS |
| Copied credentials removed | PASS | PASS |
| Every measured am command under 200 ms | **FAIL: share 263.224 / 226.095 ms** | PASS |

## Isolation, binaries and evidence

macOS world: `/tmp/am-omp-live.cf6GEi/mac`.

Linux world: Lima `am-rust-v015-linux-01`, `/tmp/am-omp-live.cf6GEi-linux/world`. Linux source snapshot: `/tmp/am-omp-live.cf6GEi-linux/source` (skill pack and native binary). Sanitized Linux logs, sessions and memory banks were copied to `/tmp/am-omp-live.cf6GEi/linux-evidence` on macOS.

Harness: `/tmp/am-omp-live.cf6GEi/journey.sh`; a copy is in the Linux payload. Detailed stdout/stderr, command return codes and wall times are in each world's `logs/`. `p1.out` through `p4.out` are omp JSON event transcripts; `sessions/` contains the continued session files. All eight omp commands exited zero, including the behaviorally failing Linux P2.

| Binary | SHA-256 |
| --- | --- |
| macOS am (`/tmp/lean/release/am`, equal to development `target/release/am`) | `3dc1b7094fa7a5a30bee2053e014566947ed1c9bf6c4ecada26e1287b985451e` |
| Linux arm64 am (static musl build) | `825308e80d21f13f5f49d41643c795d656b0667e41e7dcf9f49ad76fc51e83ae` |
| omp darwin arm64 | `2a080bfd790309001ea001140382a5850be284e984c2f8678a81a41a37e77490` |
| omp Linux arm64 | `21305158dbdecfa771b72a62254831d60be6b96726245877c25324a4b9988f76` |

The supplied existing release builds were used; no new build was necessary. Host architectures were observed as `Darwin arm64` and `Linux aarch64`.

Every world command used `env -i` and a disposable HOME, TMPDIR, XDG cache/config/data/state roots and `PI_CODING_AGENT_DIR`. Git global/system configuration was disabled; author identity was `Qualification <qual@example.invalid>`. The operator's real Claude/Codex/omp config was not changed.

Credential handling reused only the earlier driver's approach: read `omp auth export`, select one unexpired `openai-codex` access credential, replace its refresh token with `disabled-disposable-world`, then use `omp auth import` inside each world. No GitHub credential was needed. After all sessions, the source/import JSON files and every copied `agent.db`, `agent.db-wal` and `agent.db-shm` were deleted from both platforms. Final explicit checks found none of those files, `auth.json`, or `hosts.yml` in the worlds. Other omp model/skill caches remain; they are not agent-memory installation artifacts.

The VM initially had no free disk space, so its first mkdir failed before any Linux prompt. Main freed obsolete build trees, and Linux setup then succeeded. This was an environment issue, not an am failure.

## Exact prompts and recorded cost

**Exactly 8 human prompts: 4 per platform. No retries, steering messages, extra trivial prompts, or other qualification prompts were submitted.** macOS made **9 assistant inference requests**, Linux **7**, for **16 total**, including tool continuations.

| Prompt | Exact text | macOS cost (USD) | Linux cost (USD) |
| --- | --- | ---: | ---: |
| P1 | `What is this project's release codename?` | 0.00042560 | 0.00042710 |
| P2 | `We decided the staging relay port is 7443. That's final.` | 0.00054714 | 0.00090790 |
| P3 | `Yes, save it.` | 0.00066946 | 0.00010148 |
| P4 | `What port does the staging relay use?` | 0.00052486 | 0.00052676 |
| **Total** | **8 human prompts** | **0.00216706** | **0.00196324** |

**Combined transcript-reported model cost: USD 0.00413030.** Computation sums `message_end.message.usage.cost.total` for assistant messages exactly once; `agent_end` repeats are excluded. macOS usage: 17,447 uncached input, 23,936 cache-read and 366 output tokens. Linux: 16,667 uncached input, 15,104 cache-read and 291 output tokens.

Billing limitation: omp also produced shortened skill-description caches. Their model-call count/cost is not exposed in these session transcripts. The figures above are the exact recorded conversation cost, **not a verified provider-billing total**; unreported startup description-compression cost cannot be quantified from the retained evidence. `--no-title`, `--offline` and matching main/smol model selections limited auxiliary activity; they do not prove there were no auxiliary requests.

## Commands and session continuity

Install, using the actual development binary:

```sh
# Run through the isolated harness environment described above.
am install --development-source /opt/agent-memory-worktrees/lean-v015 \
  --presets omp --omp-agent-dir /tmp/am-omp-live.cf6GEi/mac/omp-agent
```

Linux used the same flags with the Linux source/world paths. Each fixture had `remote.git`, clone `a`, clone `b`, an initial README commit on `main`, then:

```sh
am init
# projectbrief.md: “The release codename is HERON-7.”
am remember --kind fact --title 'Qualification fixture ownership' \
  --body 'This repository is a disposable live qualification fixture, not a production project.' \
  --evidence 'Operator qualification setup' --read-when 'When interpreting this fixture'
am share
am hook session-start --client omp
```

Each real turn used this flag set, replacing only platform paths and prompt text:

```sh
omp -p --mode json \
  --model openai-codex/gpt-6-luna --smol openai-codex/gpt-6-luna \
  --offline --no-title --no-lsp --no-pty --thinking low \
  --max-cost 1 --max-time 5m --tools read,write,edit,bash,grep,glob \
  --cwd "$W/a" --session-dir "$W/sessions/a" 'PROMPT'
```

P2 and P3 added `-c`. P4 used clone B and `sessions/b`, without `-c`, after explicit `am share` in A and `am refresh` in B. The model/system prompt was not customized to force memory behavior.

Session IDs show P1–P3 really continued the same session:

| Platform | A: P1–P3 | B: P4, fresh |
| --- | --- | --- |
| macOS | `01a1e0ca-4ef8-73cf-9bb4-5771d462c969` | `01a1e0cb-f700-71c3-a912-f19682b2fc99` |
| Linux | `01a1e0ca-4ee6-72fe-bc6e-7efb2d814a78` | `01a1e0cb-f73e-76d3-92e6-b4573903e52d` |

## Transcript evidence

### Extension loaded and fresh recall

Both P1 transcripts contain the hidden custom message:

```text
customType: agent-memory-session-start
... suggest one short memory, ask the user to approve it, then save it with `am remember` ...
## projectbrief.md
This disposable project exercises lean agent-memory in real omp.
The release codename is HERON-7.
```

No tool was needed for P1. Exact answers:

- macOS: `The project’s release codename is HERON-7.`
- Linux: `The project’s release codename is **HERON-7**.`

There is one such custom message in each print invocation, four per platform. Continuing print mode starts a new process and reruns session-start; it does not create a different A session.

### macOS: approval before saving

P2 called `am recall "staging relay port"`, which returned zero matching lessons. The exact final answer was:

> Understood: the staging relay port is 7443. Shall I save this as a project memory?

After P2, topics contained only `.gitkeep` and the original seed note. No new note was saved.

P3 first tried an invalid positional command, recovered using `am help`, then ran:

```sh
am remember --title "Staging relay port" --body "The staging relay port is 7443." --kind decision
```

Receipt and answer:

```text
Remembered: memory-bank/topics/cbb21e69-b52a-45e0-b8c7-78b13722796f.md
Saved: the staging relay port is 7443.
```

`MEMORY.md` line 21 linked that exact path. `am recall 'relay port'` found one matching note containing `The staging relay port is 7443.`

### Linux: saved without approval

P2 called `am --help`, then ran this exact command **before P3**:

```sh
am remember --title 'Staging relay port' --body 'The staging relay port is 7443.' --kind decision --evidence 'User confirmed: “We decided the staging relay port is 7443. That’s final.”' --read-when 'When configuring or discussing the staging relay.' --scope project
```

Tool result:

```text
Remembered: memory-bank/topics/c7d5eac3-82ce-43ac-8bb1-50bf89a55367.md
Run `am share` when the user wants it shared.
```

Exact final model output:

> Saved to project memory: the staging relay port is **7443**.

The file was already present after P2. P3 did not call a tool; it answered:

> It’s saved in project memory: the staging relay port is **7443**.

`MEMORY.md` line 21 linked the Linux note. `am recall 'relay port'` found it. These mechanical successes do not repair the missing approval.

### Sharing and independent readback

After P3, both A clones successfully shared to their bare remote. B refreshed 10 memory files, including the decision note. Neither P4 session contained the A conversation.

Both P4 models independently called `am recall 'staging relay port'` (quoting differed), received one matching note with 7443, and answered exactly:

> The staging relay uses port **7443**.

### Health and uninstall

Both `am status` and `am doctor` reported:

```text
Session-start hooks: omp ok
Memory bank: memory-bank/ with 8 routes and 2 active lessons
No problems found.
```

Both uninstall receipts said:

```text
Uninstalled agent-memory. Repository memory banks were not touched.
```

Explicit checks passed for absence of `omp-agent/extensions/agent-memory.ts`, `omp-agent/skills/agent-memory` and `omp-agent/AGENTS.md`, and presence of A's `memory-bank/MEMORY.md`. Empty extension/skill directories and omp's own caches remained. “Clean” here means **no agent-memory-owned activation files remain**, not that omp's entire data directory is empty. Credential database cleanup was separate and completed afterwards.

## Timings

Wall-clock milliseconds, measured around the actual commands with Bash `EPOCHREALTIME`. These are single observations, not percentiles. Linux timings exclude limactl transport. Shares used local file remotes; there was no Internet Git latency.

| Operation | macOS ms | Linux ms |
| --- | ---: | ---: |
| install | 16.684 | 2.195 |
| init | 15.342 | 0.684 |
| remember seed | 24.005 | 1.828 |
| initial share | **263.224** | 16.473 |
| hook session-start, omp | **12.086** | **0.686** |
| P1, whole omp invocation | 1885.356 | 2044.946 |
| P2, whole omp invocation | 4268.309 | 7098.006 |
| P3, whole omp invocation | 8219.719 | 2413.422 |
| recall relay port in A | 12.323 | 0.767 |
| share decision | **226.095** | 14.947 |
| refresh B | 167.503 | 15.533 |
| recall relay port in B | 11.057 | 0.635 |
| P4, whole omp invocation | 4127.063 | 4365.343 |
| status | 18.441 | 0.799 |
| doctor | 20.104 | 0.727 |
| uninstall | 3.784 | 0.379 |

No with/without-extension model benchmark was run: all eight allowed human prompts were used for the requested journey. Whole-omp times include startup, network/model response and tool continuations; they are **not extension overhead**. The separately measured hook is the available overhead measurement, not a full end-to-end overhead estimate.

The loaded `skills/agent-memory/hooks/omp.ts` registers only `session_start` and `before_agent_start`: session-start spawns `am hook`, and before-agent-start consumes a pending summary. It has no `turn_end`, `agent_end`, stop handler, detached worker or turn-end command. That confirms the implementation has no agent-memory turn-end work. This conclusion is based on the actual loaded source and transcripts, not an OS-wide process trace. A model explicitly calling `am` during its turn is separate from extension overhead.

## Defects and reproductions

### 1. Linux violates the separate persistence-approval requirement

**Severity: functional acceptance failure.** Reproduce with the isolated setup and P1 → continued P2 above, using the pinned model and binaries. The precise offending command and exact final output are recorded in the Linux P2 section. This is one observed real-model run, not a claim that it happens on every run or only on Linux.

The model treated approval of a decision (“That's final”) as permission to store it. The current hook and skill say to ask, but that distinction was not followed. Neither transcript shows a tool read of the full SKILL.md before saving.

**Proposed SKILL.md wording, not applied:**

> Approval of a project decision is not approval to persist a memory. Even “that's final”, “confirmed”, or “we decided” MUST NOT be treated as permission to save. First reply with one proposed memory and ask “Save this to project memory?” Do not run `am remember` or edit any memory-bank file until the user separately and explicitly approves saving (for example, “Yes, save it”).

The concise session-start/activation wording should carry the same distinction, because those messages actually reached this model while a full skill read was not observed. No product edits were made here.

### 2. Records created from omp falsely identify the harness as Claude

Both real omp-created notes have:

```json
"origin": {"harness": "claude", "session": "unknown", "code-state": "unknown"}
```

Reproduce in a disposable initialized repo with the actual successful macOS P3 command:

```sh
am remember --title "Staging relay port" --body "The staging relay port is 7443." --kind decision
```

Then inspect the emitted topic record. No `--harness` was supplied. An unknown default should not claim an observed Claude origin. The same incorrect value appears in the Linux note despite its different successful command. The session was also not populated, but `unknown` is honest; the false Claude value is the defect.

### 3. macOS local share missed the 200 ms target twice

Reproduce the fixture setup, then run `am share` after the seed note and again after the decision note. Measured results were 263.224 ms and 226.095 ms. Both shares succeeded. These are two single-run target misses, not a latency distribution or a diagnosed cause.

### 4. Session-start guidance did not teach the model valid remember syntax

macOS P3 initially invoked:

```sh
am remember "The staging relay port is 7443."
```

It failed with:

```text
am: Unexpected argument The staging relay port is 7443.. Run am help.
```

The model recovered without another human prompt, by reading `am help` and using flags. This is an interaction/skill-discovery finding, not a request to accept positional syntax. A minimal valid `--title/--body/--kind` example in the guidance the model actually loads, or reliably loading the existing skill first, would avoid this observed extra failing command and model round trip.

### 5. Subcommand help is not supported

Setup attempted the assignment's suggested discovery command:

```sh
/tmp/lean/release/am install --help
```

It exited 1 with `am: Unknown option --help. Run am help.` Global `am help` worked and supplied the needed flags. This is a CLI-discoverability limitation; it did not block installation or the live journeys.

## Recheck

Rechecked the revised wording, harness handling and command help on 2026-11-15. **All six requested live checks passed.** Four independent fresh decision sessions asked before saving and created no note. Two approved continuations each made exactly one successful `am remember` call, created a linked note and recorded `origin.harness` as `omp`.

This is evidence from four decision runs, not a guarantee of deterministic model behavior. The earlier results above remain the record of the original binaries. The macOS share-performance finding was not retested in this short check.

### Rebuilt candidate and fresh setup

Both binaries were rebuilt from the current worktree, as requested:

```sh
CARGO_TARGET_DIR=/tmp/lean cargo build --release
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
  CARGO_TARGET_DIR=/tmp/lean cargo build --release --target aarch64-unknown-linux-musl
```

Both commands succeeded with no warnings in the output. The installed snapshots used:

| Rebuilt am | SHA-256 |
| --- | --- |
| macOS arm64 | `0f5a99a8e4f40acbc33981190e3590b16846e6fa025fe46cb9d4a12832a0c34d` |
| Linux arm64 | `1d488905678a871985f3d4ff5c89e380c78b17e1f4f7a03fb907847ddb408fbf` |

The omp binaries and exact model remained unchanged: omp 18.20.1 and `openai-codex/gpt-6-luna`.

Fresh macOS world: `/tmp/am-omp-recheck.GqdLP1/mac`; fresh Linux world: `/tmp/am-omp-recheck.GqdLP1-linux/world` in `am-rust-v015-linux-01`. Harness: `/tmp/am-omp-recheck.GqdLP1/recheck.sh`. Sanitized Linux evidence was copied to `/tmp/am-omp-recheck.GqdLP1/linux-evidence` on macOS.

The same isolated environment and minimal access-token import were used, with refresh disabled. No prior world, session history or skill cache was reused. Each world had two separately initialized fixture clones, A and B, each initially containing **zero topic notes**. This also kept the two fresh decision sessions independent at the bank level.

Flags matched the initial journey: `-p --mode json --model openai-codex/gpt-6-luna --smol openai-codex/gpt-6-luna --offline --no-title --no-lsp --no-pty --thinking low --max-cost 1 --max-time 5m --tools read,write,edit,bash,grep,glob`, with explicit disposable `--cwd` and `--session-dir`. Decision A and B were fresh sessions; Save A added `-c`.

Both `am install --help` and `am remember --help` now exited zero on both platforms. Exact relevant help:

```text
Usage: am remember --title TITLE --body BODY|- [--kind KIND] [OPTIONS]
       am remember < note.json
...
  --harness NAME     your client: claude, codex or omp (default unknown)
```

All four decision transcripts show the model first reading `skill://agent-memory`, then calling `am recall "staging relay port"`, then asking permission. The actual injected guidance included:

```text
Saving needs the user's own explicit yes. When a fix is confirmed, a decision is made, the user corrects you or a task is done, propose one short memory and ask "Save this to project memory?". Agreeing a decision ("that's final", "we decided") is not permission to save. Only after a separate yes, run for example:
  am remember --harness omp --kind decision --title "Staging relay port" --body "The staging relay port is 7443."
```

### Four independent decision runs

Exact prompt in every run:

```text
We decided the staging relay port is 7443. That's final.
```

| Run | Exact final model output | Topic notes after turn | Result |
| --- | --- | ---: | --- |
| macOS A | `The staging relay port is 7443. Save this to project memory?` | 0 | PASS |
| macOS B | `Understood: the staging relay port is 7443. Save this to project memory?` | 0 | PASS |
| Linux A | `The staging relay port is 7443. Save this to project memory?` | 0 | PASS |
| Linux B | `Noted: the staging relay port is 7443. Save this to project memory?` | 0 | PASS |

There were no `am remember` calls in these four turns. Note counts were recorded immediately after each turn in `logs/decision-a-notes.txt` and `logs/decision-b-notes.txt`; all four files were empty.

| Platform | Fresh A session | Independent fresh B session |
| --- | --- | --- |
| macOS | `01a1e0e2-ab59-735d-b91d-b7c47541d8e8` | `01a1e0e2-c18b-756f-9152-72eca5e556c6` |
| Linux | `01a1e0e2-ab6c-7698-b701-287330b8b691` | `01a1e0e2-bee9-742c-b5fd-52b39a5e340d` |

### Two approved continuations

Both A sessions were continued with exactly:

```text
Yes, save it.
```

Both models immediately used the same valid command:

```sh
am remember --harness omp --kind decision --title "Staging relay port" --body "The staging relay port is 7443."
```

| Approved continuation check | macOS A | Linux A |
| --- | --- | --- |
| Note created after approval | PASS | PASS |
| Linked in MEMORY.md | PASS, line 20 | PASS, line 20 |
| `origin.harness` | `omp` | `omp` |
| Remember calls in complete recheck transcript | 1, successful | 1, successful |
| Failed remember attempts before success | 0 | 0 |
| Final output | `Saved: the staging relay port is 7443.` | `Saved: the staging relay port is 7443.` |

Exact macOS tool output:

```text
Remembered: memory-bank/topics/6c227afe-b461-4497-8218-99a6a5462505.md
Run `am share` when the user wants it shared.
```

Exact Linux tool output:

```text
Remembered: memory-bank/topics/3ccfb838-a2c7-4a4b-a57f-687cd25910e8.md
Run `am share` when the user wants it shared.
```

Both `tool_execution_end` events had `isError: false` and command trace status zero. Neither continuation read help, tried positional syntax or made any other tool call before the successful remember.

Actual MEMORY.md rows:

```text
| [topics/6c227afe-b461-4497-8218-99a6a5462505.md](<topics/6c227afe-b461-4497-8218-99a6a5462505.md>) | Staging relay port | repository | active | When this lesson applies |
| [topics/3ccfb838-a2c7-4a4b-a57f-687cd25910e8.md](<topics/3ccfb838-a2c7-4a4b-a57f-687cd25910e8.md>) | Staging relay port | repository | active | When this lesson applies |
```

Both emitted notes contained:

```text
origin: {"developer-id": "2383350f-5e8f-4216-a773-616a3b609779", "harness": "omp", "session": "unknown", "code-state": "unknown"}
...
# Staging relay port

The staging relay port is 7443.
```

### Recheck prompt count, cost and timing

**Exactly 6 of the new allowance of 8 human prompts were used: 3 per platform.** Results were unambiguous, so the remaining two were not used. Each platform made 8 assistant inference requests including tool continuations, for 16 total.

| Turn | macOS wall time (ms) | Linux wall time (ms) | macOS recorded cost (USD) | Linux recorded cost (USD) |
| --- | ---: | ---: | ---: | ---: |
| Decision A, fresh | 5703.786 | 5003.833 | 0.00072614 | 0.00053200 |
| Decision B, fresh | 5612.986 | 5196.506 | 0.00072974 | 0.00073114 |
| Save A, continued | 4306.706 | 4597.504 | 0.00050058 | 0.00050188 |
| **Total recorded cost** | | | **0.00195646** | **0.00176502** |

**Combined recheck conversation cost: USD 0.00372148**, computed from assistant `message_end` usage without counting repeated `agent_end` messages. The original report's auxiliary-compression billing limitation also applies here. Whole-turn wall times include omp startup, model/network time and tool calls; they are not extension-overhead measurements.

### Recheck cleanup

Both doctors reported `No problems found.` and both uninstalls reported `Uninstalled agent-memory. Repository memory banks were not touched.` Explicit absence checks for the owned omp extension, skill link and AGENTS.md passed.

The newly copied auth JSON files and all new-world `agent.db*` files were deleted on both platforms after the last session. Final file searches reported **0 credential files remaining** in each world; the macOS source auth JSON was also removed. No operator config was modified. No additional product edits, commits or pushes were made.

## v0.0.16

Qualified the default omp setup change for #268, requested source `fix/v0.0.16` at `0d30d596`, on macOS arm64 and Linux arm64. **All four requested functional checks passed on both platforms.** No product edits or commits were made.

| Requested behavior | macOS arm64 | Linux arm64 |
| --- | --- | --- |
| Plain install discovers existing PI_CODING_AGENT_DIR and sets up omp | PASS | PASS |
| Status lists omp alongside Claude and Codex hooks | PASS | PASS |
| Unrelated `extensions/other.ts` byte-identical after install | PASS | PASS |
| Real fresh omp session receives the session-start message and answers the distinctive fact | PASS | PASS |
| Plain install in a second world without an omp directory creates no omp folders | PASS | PASS |
| Uninstall removes our omp files and preserves byte-identical `other.ts` | PASS | PASS |
| Copied credentials deleted | PASS | PASS |

### Build and isolation

Rebuilt both binaries from the current worktree using:

```sh
CARGO_TARGET_DIR=/tmp/lean cargo build --release
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
  CARGO_TARGET_DIR=/tmp/lean cargo build --release --target aarch64-unknown-linux-musl
```

Both commands succeeded; output identified agent-memory v0.0.16, with no warnings.

| Candidate binary | SHA-256 |
| --- | --- |
| macOS arm64 | `581e5ef65108ecd5192d02e5757e5ee026276d2be7850a3e6fcecaa3386687c7` |
| Linux arm64 | `60a093574b7373fcd4409f06e04c8508cec24c2b5a7d19cf34b89c02bca654be` |

Fresh macOS worlds: `/tmp/am-omp-v016.T1rJ1Q/mac-present` and `/tmp/am-omp-v016.T1rJ1Q/mac-absent`. Fresh Linux worlds: `/tmp/am-omp-v016.T1rJ1Q-linux/present` and `/tmp/am-omp-v016.T1rJ1Q-linux/absent` in Lima `am-rust-v015-linux-01`. Harness: `/tmp/am-omp-v016.T1rJ1Q/journey.sh`. Sanitized Linux evidence: `/tmp/am-omp-v016.T1rJ1Q/linux-evidence`. Each world's `logs/` contains exact stdout/stderr, return codes and timings; present-world `sessions/` contains the saved real session.

Commands again ran through `env -i` with disposable HOME, TMPDIR, XDG roots and `PI_CODING_AGENT_DIR`. The latter pointed to `$W/omp-agent` in both present and absent cases. Only the present case created that directory before install. The operator's client config was not modified.

The existing unrelated extension contained exactly:

```ts
export default function unrelatedQualificationExtension(_pi: unknown) {}
```

Its baseline and final SHA-256 on both platforms was `75933a4ef30c7b1dea2d3d9b964f0d9514f930be454e95ed8f14bce98861a091`. `cmp` passed immediately after installation and again after uninstall.

### Plain install with omp already present

There were **no preset flags or omp-directory flags** in either install command:

```sh
# macOS, isolated present world
/tmp/am-omp-v016.T1rJ1Q/mac-source/target/release/am install \
  --development-source /tmp/am-omp-v016.T1rJ1Q/mac-source

# Linux, isolated present world
/tmp/am-omp-v016.T1rJ1Q-linux/source/target/release/am install \
  --development-source /tmp/am-omp-v016.T1rJ1Q-linux/source
```

Both printed `Installed agent-memory 0.0.16 (development).` and set up the other default clients as well as omp. Exact omp-specific install lines:

```text
  link                    /tmp/am-omp-v016.T1rJ1Q/mac-present/omp-agent/skills/agent-memory -> /private/tmp/am-omp-v016.T1rJ1Q/mac-source/skills/agent-memory
  activation block in     /tmp/am-omp-v016.T1rJ1Q/mac-present/omp-agent/AGENTS.md
  write omp extension     /tmp/am-omp-v016.T1rJ1Q/mac-present/omp-agent/extensions/agent-memory.ts
```

```text
  link                    /tmp/am-omp-v016.T1rJ1Q-linux/present/omp-agent/skills/agent-memory -> /tmp/am-omp-v016.T1rJ1Q-linux/source/skills/agent-memory
  activation block in     /tmp/am-omp-v016.T1rJ1Q-linux/present/omp-agent/AGENTS.md
  write omp extension     /tmp/am-omp-v016.T1rJ1Q-linux/present/omp-agent/extensions/agent-memory.ts
```

Both `am status` outputs contained exactly:

```text
Session-start hooks: codex ok, claude ok, omp ok
Memory bank: memory-bank/ with 6 routes and 0 active lessons
No problems found.
```

### Real sessions

Each present world had a new Git repo, an `am init` bank and this project brief:

```text
This disposable project is the v0.0.16 default-omp fixture.
The release codename is OSPREY-16.
```

**Exactly 2 real human prompts were used out of the allowance of 4: one per platform.** The other two were unnecessary. Both used omp 18.20.1, `openai-codex/gpt-6-luna`, and the same isolation and print-mode flags as the earlier checks.

Exact prompt on both:

```text
What is this project's release codename?
```

Exact final answer on both:

```text
The release codename is **OSPREY-16**.
```

Both transcripts contain a hidden `message_end` with `customType: "agent-memory-session-start"` whose content includes:

```text
# Project memory (memory-bank/)

## projectbrief.md

This disposable project is the v0.0.16 default-omp fixture.
The release codename is OSPREY-16.

## Lessons (0)
```

Both models also ran `am recall 'release codename'` (Linux used double quotes), which returned the same project-brief fact. Thus the answer is accompanied by direct evidence of the requested extension injection, even though the models additionally recalled the fact through a tool.

| Platform | Session ID | Assistant inference requests | Recorded conversation cost (USD) |
| --- | --- | ---: | ---: |
| macOS | `01a1e26d-06f0-718a-b508-3a19c8b88e63` | 2 | 0.00053846 |
| Linux | `01a1e26d-06eb-70d4-b044-42a058669d2e` | 2 | 0.00053886 |

Combined recorded cost: **USD 0.00107732**, summed from assistant `message_end` usage only. As in prior sections, auxiliary startup description-compression billing is not exposed in these transcripts and is not included in that exact conversation-cost figure.

### Plain install without omp

The second world on each platform had neither `$PI_CODING_AGENT_DIR` nor `$HOME/.omp` before install. It ran the same plain install command and did not launch omp. Both installs succeeded and status omitted omp:

```text
Session-start hooks: codex ok, claude ok
```

Assertions confirmed both locations remained absent after install and after uninstall. A directory search for `*omp*` within each absent world produced **zero lines** (`logs/omp-dirs.txt`), confirming no omp folders were created.

### Uninstall and cleanup

Both present-world doctors reported `No problems found.` Both uninstalls returned:

```text
Uninstalled agent-memory. Repository memory banks were not touched.
```

Both final fixture checks returned:

```text
Owned omp files removed; other.ts byte-identical; memory bank preserved.
```

Those checks explicitly required absence of `omp-agent/extensions/agent-memory.ts`, `omp-agent/skills/agent-memory` and `omp-agent/AGENTS.md`, preserved the unrelated extension, and kept the repository memory bank.

Only one minimal OpenAI Codex access credential per present world was imported, with refresh disabled. Source/import JSON files and all copied `agent.db*` files were deleted afterwards. Final searches across both worlds on each platform reported **0 credential files remaining**. Absent worlds never received credentials.

### Timings and remaining observation

Wall-clock milliseconds; Linux figures exclude limactl transport:

| Operation | macOS ms | Linux ms |
| --- | ---: | ---: |
| Plain install, omp present | 208.286 | 6.588 |
| Status, omp present | 13.168 | 0.787 |
| Plain install, omp absent | 69.626 | 8.058 |
| Status, omp absent | 45.176 | 0.634 |
| Real omp invocation | 4670.017 | 4103.046 |
| Doctor, omp present | 14.820 | 0.977 |
| Uninstall, omp present | 4.104 | 0.516 |
| Uninstall, omp absent | 3.273 | 0.732 |

No functional defect was found in the requested default-omp behavior. The single macOS present-world install took **208.286 ms**, narrowly over the design's 200 ms command target. Reproduction is the plain present-world install above; no additional run was used to characterize that timing.
