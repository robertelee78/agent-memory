# Active context

## Current focus

v0.0.16 is published: omp set up by default (#268), install on a fresh machine
and after any `am uninstall` (#269), and a plain `git pull` after `am refresh`
and `am share` (#270). v0.0.17 (`fix/recall-precision`) is a small fix release:
`am recall QUERY` prints only the notes that have every query word, without
the project summary in front. The larger guided-recall, graph and embedding
work is planned for v0.0.18.

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
