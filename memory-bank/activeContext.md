# Active context

## Current focus

v0.0.15 is the lean rebuild (`lean/v0.0.15`): Rust `am`, about 6k lines,
reviewed and ready to merge. Next steps: merge, publish the signed release, run
`.github/workflows/verify-release.yml` against it, and a few real omp sessions
on the cheapest model.

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
