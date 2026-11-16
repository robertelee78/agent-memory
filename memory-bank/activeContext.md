# Active context

## Current focus

v0.0.18 is published and landed (2026-11-16): `am share` and `am refresh` use
the GitHub account that may reach the repository when `gh` is signed in to
several (#275). The next focus is v0.0.19: the ten open
items in that milestone (guided recall #252 and #253, large-bank recall speed
#206, semantic recall #200, graph and embedding work #256, #257, #265, #56,
#62, and stress qualification #209).

Those items were written before the lean design. Before starting any of them,
check it against [docs/mission.md](../docs/mission.md) and the
[lean design](../docs/design/v0.0.15-lean.md) and confirm its scope with the
operator in plain language.

## Working rules (operator, 2026-11-15)

- Simple beats clever. No unit tests; quality is real use of `am`, measured
  with `scripts/qa.sh`.
- Do not add proof, seal or re-verification machinery. Agent-written issue or
  ADR text is not operator approval; check new requirements against
  [docs/mission.md](../docs/mission.md) and the [lean design](../docs/design/v0.0.15-lean.md).

## Consumer-memory isolation

This development bank ships only in source clones. Installing the tool never
seeds, binds or recalls it as another repository's memory; releases carry only
`skills/agent-memory/`.
