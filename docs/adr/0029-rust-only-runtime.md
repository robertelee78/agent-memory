---
id: ADR-0029
status: accepted
approval-scope: operator directive 2026-11-13 — Rust-only implementation and removal of Python from the repository; the detailed cutover design's contract changes await operator confirmation
date: 2026-11-13
amends: [ADR-0004, ADR-0007, ADR-0009]
related: [ADR-0013, ADR-0015]
---

# ADR-0029: Rust-only runtime, tooling and tests

## Why

Through v0.0.14 the runtime, hooks, installer, tests and evaluation tooling were
Python: 560 tracked files and 110,274 lines at `78eaf70`. On 2026-11-13 the
operator rejected publishing a Python v0.0.15 and directed a complete port to
Rust with Python purged from the repository
([#249](https://github.com/robertelee78/agent-memory/issues/249#issuecomment-6846182027),
[#206](https://github.com/robertelee78/agent-memory/issues/206#issuecomment-6846202226),
[#244](https://github.com/robertelee78/agent-memory/issues/244#issuecomment-6846203027)).

## Decision

- The product MUST be implemented in Rust: runtime, CLI, harness boundaries,
  installer, updater, release tooling, contract tests and evaluations.
- The repository and every release payload MUST NOT contain Python source,
  bytecode, interpreter launchers, interpreter probes or Python build, test or
  CI requirements, including inside fixture archives. Git history is not
  rewritten.
- There MUST NOT be a Python subprocess, embedded interpreter, transpiled
  runtime, compatibility shim or Python-only test oracle.
- Consumer installation MUST NOT require Cargo or any interpreter. Native
  harness adapters that the harness itself requires in another language
  (the OMP TypeScript extension, OpenCode JavaScript plugins) remain thin
  adapters that call the Rust executable.
- The language change MUST NOT reduce the product. Existing `agent-memory/v1`
  banks are read without rewriting. Safety, provenance, recovery, review and
  sharing contracts keep their meaning. Durable byte formats keep their bytes.
- Python candidate `78eaf70` and draft PR #263 are superseded and MUST NOT be
  released. Their qualification evidence stays historical and does not qualify
  the Rust implementation.

The [cutover design](../design/v0.0.15-rust-cutover.md) defines adoption of
Python-era installations, the interpreter-free bootstrap and `release-v2`
manifest, named codecs, tiers, coexistence rules and coverage obligations.

## Consequences

- ADR-0004's "skill and CLI" contract is unchanged in meaning; its
  implementation language is no longer Python and `.py` helper paths are not
  preserved as aliases.
- ADR-0007's lifecycle contracts remain; the updater's Python-version
  compatibility check and Python command-surface discovery are replaced by a
  per-target signed binary and a signed declared command surface. Installations
  of v0.0.14 and older cannot self-update and re-run the bootstrap.
- ADR-0009's contract registry and fault injection remain obligations. Every
  baseline obligation is accounted for in
  `tests/obligations/python-baseline-v1.json` before any Python file is
  removed.
- #206, #244 and #249 remain open obligations of v0.0.15; the rewrite does not
  complete them.

## Alternatives considered

- **Release Python v0.0.15, port later:** rejected by the operator.
- **Rust launcher over the Python runtime, or a Python bridge release:**
  rejected; both keep an interpreter dependency.
- **Port the runtime but keep Python tests or tooling:** rejected; the
  directive covers the whole repository.
