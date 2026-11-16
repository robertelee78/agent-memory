# Active context

## Current focus

v0.0.16 (`fix/v0.0.16`) is a small fix release of the lean `am`: omp is set up
by default when it is on the machine (#268), install works on a fresh machine
and after any `am uninstall` (#269), and a plain `git pull` works after
`am refresh` and `am share` in merge and rebase mode (#270). Next steps:
merge, publish the signed release, check `am update` from v0.0.15 on the
operator's machine (run it twice: v0.0.15 installs v0.0.16, v0.0.16 then sets
up omp), and one real omp session on the cheapest model.

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
