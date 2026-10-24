---
id: ADR-0013
status: accepted
approval-scope: v0.0.2
date: 2026-10-24
related: [ADR-0004, ADR-0005, ADR-0006, ADR-0007, ADR-0008, ADR-0009]
---

# ADR-0013: AWA-style public CLI and corrective releases

## Context and product authority

The operator requested the installation and lifecycle experience established by
`agent-work-accountability`, then explicitly selected `am` as the primary command.
A checkout-only bootstrap, a macOS temporary-path failure, raw diagnostic JSON,
and rejection of `am --version` showed that backend checks did not establish this
public product contract. The operator requested a new release, corrected release
tracking, updated ADRs, and tasks in `v0.0.2` on 2026-10-24.

This ADR amends the public presentation and release planning of ADR-0007. It
does not replace its ownership, atomic activation, recovery or data-preservation
requirements. Original release evidence and source-bound approvals remain
historical records.

The operator authorized this corrective scope and ADR/task update on 2026-10-24.
Independent technical review approved a thin presentation adapter preserving
wrapper ownership, machine helpers, parser exit codes and old-updater behavior.
Acceptance here approves the design; execution and publication require their
own source-bound evidence.

## Public command contract

The public interface SHALL use `am`, retain `agent-memory` compatibility, and
follow AWA's conventional command behavior:

```text
curl -fsSL https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh | bash
am
am help
am --help
am -h
am --version
am -V
am version
am install [--check]
am update [--check]
am uninstall [--check]
am status [--repo PATH]
am doctor [--repo PATH]
```

- No arguments, `help`, `--help` and `-h` MUST show useful public help and exit 0.
- `--version`, `-V` and `version` MUST agree and print the actual executing
  version, revision/cleanliness and source, without identity/bank setup or network
  access. Public help and errors MUST identify `am` or the invoked compatibility
  command, rather than leak the `installer.py` implementation name.
- Installed lifecycle and diagnostic commands, and the one-line bootstrap, MUST
  default to concise human-readable output. Success MUST follow actual verified
  results. Missing setup, preserved user edits, warnings, failures and repairs
  MUST remain explicit. A preview MUST NOT imply an applied change.
- `--json` MUST expose the complete existing machine result without losing
  receipts, qualifications or operation data. `--verbose` MUST make additional
  lifecycle detail available without changing the operation.
- Agent-facing memory operations MUST retain complete structured proposals and
  results; presentation MUST NOT discard recalled content or mutation evidence.
  Packaged Python helpers SHALL retain their JSON interface.
- Syntax errors MUST exit 2, refused/failed operations MUST stay nonzero, and
  output formatting MUST NOT turn a backend failure into success.
- Ordinary uninstall MUST preserve repository memory, identity metadata,
  unrelated content and modified targets. Purge MUST remain separately previewed
  and explicitly confirmed.

The CLI SHALL be a thin presentation adapter over the existing operations. It
MUST NOT duplicate memory policy or invent a second installer/updater. Existing
installed wrappers and old updater processes MUST remain upgradeable; tests MUST
execute a frozen actual previous runtime rather than simulate an old receipt
under new code.

## Corrective release and roadmap

`v0.0.2` SHALL be the corrective installation and public-CLI release. It SHALL
include the landed one-line bootstrap (#153), native macOS and owned-`am` fixes
(#155), the complete public contract above, production-style acceptance, and a
published release gate. These tasks MUST be visible in its release milestone.

The operator-approved insertion preserves the later scopes and shifts their
targets as follows. This table supersedes earlier future-version labels without
changing their underlying requirements or claiming approval of deferred work:

| Previous target | Current target | Preserved scope |
| --- | --- | --- |
| v0.0.2 | v0.0.3 | Reliability/recovery, recall quality at scale, explicit bank migration (#99, #98, #48) |
| v0.0.3 | v0.0.4 | Turn-boundary hooks, matched outcome evaluation, signed distribution (#19, #36, #49) |
| v0.0.4 | v0.0.5 | Deferred federation/graph/optional-store proposals (#55, #56, #57, #62) |

AWA SHALL own assignments, phase/evidence transitions and release close. Every
release SHALL have one release milestone named exactly as its tag. The new
release gate MUST stay incomplete until the full GitHub Release is published,
its tag contains the accepted changes, assets/version match, and AWA verifies
the delivered work and closes the release milestone. Merge-delivered fixes MAY
remain Done while belonging to the release milestone; their merge evidence MUST
NOT be misrepresented as release-publication evidence.

## Acceptance and validation

Acceptance MUST exercise the command lines above, their output and exit codes,
with no implicit test-harness formatting override. Machine consumers in tests
MUST request JSON explicitly. The exact installed command `am --version` MUST
be a regression case.

The release MUST verify fresh/repeat installation, a real old-runtime upgrade,
update preview/apply/no-change/refusal, uninstall preview/apply/preserved data,
both command names, human/JSON output and recovery. macOS tests MUST retain
native `/var` versus `/private/var` temporary-path spelling; Linux acceptance
MUST run on the available operator-controlled Linux host. Every test MUST use
isolated homes/client roots and preserve real user installations.

Heavy qualification SHALL run locally on the operator's supported machines.
This corrective release MUST disclose its focused coverage and inherited
evidence; it MUST NOT claim a complete new release-matrix run from partial
checks. No expensive hosted matrix is required merely to publish this patch.

## Exclusions

This release does not add personal memory, cross-repository sharing, MCP,
harness hooks, a new storage engine, schema migration or signed distribution.
The later proposals remain distinct. The default trusted Git channel continues
to follow `main`, as AWA does; the release tag and assets provide an immutable
published checkpoint.
