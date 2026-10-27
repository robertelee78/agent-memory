---
kind: operation
domain: maintain
---

# Validate the portable instruction package

## Use when

A session MUST validate the complete candidate package before publishing or
activating it, and after changing its instructions, dispatch, schemas or assets.
This operation checks the skill package; it does not inspect a project's bank.

## Required context

A session using this leaf MUST first load and follow the
[maintain domain](../GUIDE.md). It MAY open the [shared model](../../model/GUIDE.md)
when an unresolved semantic question requires it.

## Inputs and preconditions

The session MUST identify the complete candidate directory. Python 3.11 or later
is sufficient; the checker MUST NOT import or execute the candidate's code.
For an unreviewed candidate, the session MUST run its already trusted validator
with `--pack` naming that candidate. No service, bank, client or credentials are
needed. A package can contain internal navigation cycles; mandatory loading
cycles and instructions to load the whole tree are invalid.

## Observe

The session MUST retain the actual candidate source for review. The checker
reads the complete bounded inventory and checks it again before returning.
It checks one root registration, restricted frontmatter/version, links/anchors,
reachability, domain ownership, ordered nonempty operation sections, asset
references/modes, supported literal dispatch, and mechanical RFC forms.

Loading checks recognize positive MUST/SHALL read, load, open, preload and follow
clauses, including first/always modifiers. Explicit when/if/unless clauses,
natural-boundary events and leading qualified/eligible state clauses are
conditional. Purpose words alone do not make a requirement conditional. A
session MUST express an actual loading condition explicitly and preserve its
meaning, rather than weaken an obligation to obtain a pass.

Navigation and loading use the same inline, reference and HTML link resolver.
The documented shell profile accepts one literal command and an optional
trailing stdout-file redirection. The quoted `SKILL_DIR` helper-path expansion
is supported; other expansions, pipelines and command substitutions are
refused. These checks do not execute candidate shell commands.

## Plan

The session MUST plan only the check. Validation writes no package or bank files.
It MUST retain a prior source snapshot and rule/strength/location map for an
instruction refactor, plus independent meaning review and representative use;
a structurally valid package is insufficient evidence of preserved meaning.

## Apply

With `SKILL_DIR` resolved to the trusted package, the session MUST run:

```sh
python3 -I "$SKILL_DIR/scripts/validate_pack.py" --pack /path/to/candidate > /private/path/pack-check.json
```

## Read back

The session MUST read the complete result, including `valid`, `issues` on refusal,
the successful file hashes/modes, `mandatory_loads`, `documented_commands`,
`file_operations`, and the explicit limitations.

## Verify

An observed valid result has exit 0, `valid: true`, `writes_performed: false`,
`candidate_code_executed: false` and `semantic_completeness_verified: false`.
The session MUST bind the hashes to the actual candidate being reviewed or
activated; a check of another directory does not qualify it.

## Outcomes

A valid package MAY proceed to independent behavioral review or the next
authorized lifecycle operation. Exit 1 with `valid: false` means the package
MUST remain unactivated until the reported defect is corrected and rechecked.
The result MUST NOT be presented as proof of memory truth, English equivalence,
complete argument semantics, or the runtime behavior of arbitrary Python code.

## Failure handling

The session MUST preserve the candidate and the refusal result. It MUST correct
the reported format, route, asset or dispatch problem before retrying, rather
than delete required material to obtain a pass. Unsupported syntax needs an
explicit validator extension and independent review, not silent skipping.

## Diagnostic fallback

If the normal check cannot establish a result, the session MAY compare the
reported relative path against the actual candidate files, supported Markdown
forms and literal argparse declarations. It MUST NOT run unreviewed candidate
code to bypass a static refusal or weaken a prior rule without authorization.
