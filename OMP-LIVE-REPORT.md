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
