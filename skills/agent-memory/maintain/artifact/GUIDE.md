---
kind: operation
domain: maintain
---

# Inspect a complete portable artifact

## Use when

A session MUST inspect the complete artifact when identifying a copied package
or preparing its lifecycle preflight. The package is the whole
`skills/agent-memory` directory, including its one root, guides, scripts,
schemas, templates and optional adapter declarations.

## Required context

A session using this leaf MUST first load and follow the
[maintain domain](../GUIDE.md). It MAY open the [shared model](../../model/GUIDE.md)
when a semantic question remains unresolved.

## Inputs and preconditions

The session MUST identify the complete package directory and use a trusted
inspector against an unreviewed candidate. The inspector requires Python 3.11
or later and the standard library. Governed repository operations also require
Git; optional service adapters have their own prerequisites. No agent client
or client API is a core prerequisite.

## Observe

The session MUST preserve the actual candidate for review. Its
[runtime declaration](../../schemas/runtime-compatibility.json) names directly
readable bank schemas and bundled migrator source schemas; the initial artifact
declares `agent-memory/v1` and no migrators. The
[optional GitHub publication adapter](../../adapters/github-publication.json)
declares `gh` and authentication for its service-specific operation. Missing
optional infrastructure does not make local Markdown memory unavailable.

## Plan

The session MUST plan only an inventory/declaration check. A runtime upgrade
MUST NOT implicitly migrate a project bank. Copying an artifact does not
establish installation ownership, a trusted source revision or active clients.

## Apply

With `SKILL_DIR` resolved to the trusted package, the session MUST run:

```sh
python3 -I "$SKILL_DIR/scripts/artifact.py" inspect --pack /path/to/complete/package > /private/path/artifact.json
```

## Read back

The session MUST read the complete result and compare `files`, `file_count`,
`version`, `artifact_digest`, declared `compatibility` and `optional_adapters`
with the actual candidate. The inspector validates the package and rereads its
inventory before returning; it does not import candidate code.

## Verify

The session MUST require exit 0 and `valid: true` for the package it actually
intends to use. The digest binds relative file names, content hashes and file
modes, so an unchanged copied directory has the same identity at another path.
It MUST NOT be treated as a signature, proof of schema behavior, installation
receipt or completed runtime activation. Public operation tests and independent
review remain necessary.

## Outcomes

A structurally valid declared artifact MAY proceed to the next authorized
lifecycle preflight. Exit 1 with `valid: false` means a missing, malformed or
changed asset/declaration MUST be resolved before activation. Initial schema
readability does not imply that an older bank has a bundled migration path.

## Failure handling

The session MUST preserve the refusal and candidate, correct the reported
package defect, and check the complete directory again. It MUST NOT remove
required assets or add a false compatibility declaration to obtain a pass.

## Diagnostic fallback

If ordinary inspection cannot establish a result, the session MAY compare
the bounded reported files against the actual package and use the
[pack-validation operation](../pack/GUIDE.md) to isolate structural defects.
It MUST keep unverified source, behavior and installation claims unverified.
