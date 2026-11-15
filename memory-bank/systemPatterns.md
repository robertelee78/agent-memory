# System patterns

## The bank

`memory-bank/MEMORY.md` is the route map (one table row per file: purpose,
scope, lifecycle, read-when). Six context files answer different questions;
the bank grows with the project, so missing ones are fine. Lessons are notes in
`topics/<id>.md` with record frontmatter ([schema](../skills/agent-memory/schemas/record.json)),
linked from `MEMORY.md`. A superseded lesson keeps its file with
`status: "superseded"` and loses its route.

## Agent workflow

- **Session start:** the hook (Claude, Codex, omp) injects the project summary
  and how to use `am recall`/`am remember`. Nothing runs on each turn.
- **Milestones:** when a fix is confirmed, a decision is made, the user
  corrects the agent or a task is done, the agent proposes one short memory and
  asks "Save this to project memory?". Only a separate explicit yes leads to
  `am remember`. Agreeing a decision is not permission to save.

## Sharing

`am share` commits only `memory-bank/` on top of the shared branch (a temporary
index; code, HEAD and other index entries untouched) and pushes; a protected
branch gets a memory branch and a pull request. `am refresh` merges others'
memory into the working tree (MEMORY.md row by row, other files with
`git merge-file`) and stages the shared bank so a plain `git pull` works. Both
refuse rather than lose data: links inside the bank, memory staged only in the
index, unresolved conflict markers.

## Install

Signed release (install.sh verifies the binary with `ssh-keygen`; `am` verifies
the release-v2 manifest and pack). A receipt records every owned file; files are
replaced only while unchanged, whole files deleted only if am created them and
nothing else is left. A small leftovers record (with hashes) lets a killed run
be finished safely. A v0.0.14 install is taken over the same way.
