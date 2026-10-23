# Retain incidents and observed guide evidence

`tests/incidents.json` connects the actual user-visible legacy-capture resurrection
and discarded merge-side publication incidents to their minimal public regressions,
first observed bad candidate, independently accepted correction, requirement,
actual integration and merge delivery. These are historical evidence links,
not a second writable copy of current AWA work state.

From a checkout, replay either retained symptom through real public interfaces:

```sh
python3 -I scripts/run_incident.py compacted-legacy-capture-reappears
python3 -I scripts/run_incident.py discarded-merge-side-claims-publication
```

The runner invokes the named actual contract cases in isolated `World` processes.
The first case captures/consolidates, revokes, compacts and attempts legacy replay;
the second exercises discarded and actually retained merge outcomes. A successful
receipt has `passed: true`, a nonzero test count and no skipped/expected-failure
substitutes. The incident catalog names the actual work keys. These commands
passed during implementation; the surrounding contract gate reruns them.

An in-scope failure SHOULD first receive the smallest public-interface reproduction
that fails on the observed old behavior. Its regression name SHOULD retain the
user-visible symptom. A fix MUST retain that executable reproduction and connect
its outcome, acceptance, integration and delivery to the actual work item. The
catalog MUST use real durable evidence URLs, not a branch name or a speculative
completion claim. Independent review SHOULD first exercise representative leaves
from the copied runtime instructions before reading implementation/tests/criteria;
the catalog links the actual reviews performed that way.

## Capture guide examples from actual commands

```sh
python3 -I scripts/capture_guide_examples.py > /private/observed-examples.json
```

The script executes three public `bank_contract.py` commands in a credential-free
isolated world. It records the actual argument vector, result/status hashes and
observed JSON fields or stderr, plus the complete portable artifact file hashes.
Its `command` field is a display template: `$SKILL_DIR` and input filenames replace
actual disposable paths. Those are placeholders, not a claim that a shell expanded
them during capture. `observed_file_paths` lists actual keys in the rendered
`files` map; it does not claim that the product returned an array there.

The retained [observed example projection](observed-guide-examples.json) names the
actual runtime source commit and complete artifact hashes. Volatile raw argument
paths stay outside the repository; the projection retains the capture digest.
It is evidence for that source version, not an assertion that every future
version behaves identically. A quoted guide command, result field, error or
exit meaning MUST come from observed output for the relevant runtime version.
A changed command/result MUST be recaptured and its affected examples reviewed.

Observed examples from that capture:

- A useful router `render --input REVIEWED_INPUT.json` returned exit 0, operation
  `render`, and `writes_performed: false`; its file-map keys were `MEMORY.md` and
  `projectbrief.md`. This is rendering, not an applied write or admission.
- `render --input EMPTY_INPUT.json`, where the file contained `{}`, returned exit
  1, `valid: false`, and the exact error
  `Render input needs project-id, router-updated, contexts and omitted`.
- `unknown-operation` returned exit 2 and argparse's actual invalid-choice error.
  The retained projection contains its exact stderr and available operations.

## Preserve every normative rule through a refactor

Keep the immutable old source available, for example in a separate detached
worktree. Before editing, capture its actual portable pack:

```sh
python3 -I scripts/guide_evidence.py snapshot --pack /path/to/old/skills/agent-memory > /private/before.json
```

After editing, inspect the new snapshot and prepare an explicit mapping for each
changed normative paragraph:

```sh
python3 -I scripts/guide_evidence.py snapshot --pack skills/agent-memory > /private/after.json
python3 -I scripts/guide_evidence.py check --before /private/before.json --before-pack /path/to/old/skills/agent-memory --pack skills/agent-memory --map /private/rule-map.json
```

A map is an object keyed by an old snapshot rule ID. Each entry has exactly
`new_rules` (one or more actual new rule IDs), `reason` (the actual meaning review)
and `meaning_preserved: true`. Identical text can move paths or lines and is mapped
automatically; changed text MUST have an explicit reviewed map. A refactor MUST
retain every normative instruction at equal or stronger RFC 2119 force, its
conditions and meaning, and its progressive route reachability. A stronger word
MUST NOT be used to conceal a changed requirement or lost exception.

The checker compares the saved before evidence with the still-preserved actual
old source, inventories RFC 2119 terms outside fenced code, and requires complete
old-rule coverage. It checks force and obligation/prohibition polarity, and that
every current runtime Markdown guide remains reachable from `SKILL.md` through
existing relative links within the portable artifact. It refuses missing,
weakened, inverted, unreachable or substituted evidence. Paragraphs containing
several normative terms retain all their force obligations; a map may split one
paragraph across several target paragraphs.

The checker does not understand English equivalence or authenticate the old Git
revision. The reviewer MUST verify the old source's immutable Git binding and
inspect each changed wording/condition/exception map; `meaning_preserved` is that
reviewer's assertion. `semantic_equivalence_verified` remains false. The source
files and their Git history remain authoritative; private snapshots/maps are
review evidence and MUST NOT become a second writable requirements store.

The incident/evidence contract tests prove exact-text moves, an explicit wording
map, refusal of lost/weakened/inverted/unreachable instructions, actual old-source
readback, complete current-pack reachability and the observed public examples.
Exit 0 from the checker means these mechanical checks passed; exit 1 means the
source/map needs review. No checker or example command changes canonical memory
or live client configuration.
