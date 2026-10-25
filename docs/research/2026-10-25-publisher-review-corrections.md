# Publisher review corrections before first signed delivery

Scope: unpublished v0.0.3 signing task #49. These are pre-release findings,
not evidence of a published AM signing failure, exposed key or changed tag.

## Findings and cause

Independent review of `2dcd8d1ea739a3fd373dcd598641ed945e485508` found that
consumer installation tests did not establish publisher correctness:

- Pending drafts were looked up through the published-tag REST endpoint. The
  existing GitHub CLI uses a separate draft resolver and release ID.
- A draft's target commit and tag spelling were treated as actual tag proof.
  An existing/concurrent tag can name another commit.
- Ancestry was treated as proof that current main still serves the reviewed
  installer/runtime; a later descendant can replace those files.
- Key resolution erased a symlink before checking it, and custody checks did not
  enforce the approved private, single-link, external-key location.
- The signing consumer journeys were not registered in the mandatory local gate;
  publisher behavior had no offline public-command control.

The process error was crossing CLI/REST boundaries without preserving the
existing client's draft handling, and using partial evidence for stronger claims.
It was also insufficient to test only the consumer side of a release transaction.

## Corrections

Reuse `gh release view` to resolve draft/published release identity, then read
that release by ID. Resolve the actual tag reference, including annotated tags,
before publication where present and after public/default readback. Never move
an unknown tag or reuse a published version.

Read current main's content-addressed installer blob and complete portable tree,
then the actual public default installer, with a stable main observation. Unrelated
docs/memory descendants can pass; changed defaults or concurrent incompatible
promotion remain pending without forcing refs or discarding edits.

Reuse the existing Unix trust helpers before key resolution/use; require a private
current-user parent and an ordinary single-link owner-only key outside checkout,
portable artifact and output. The real AM private key stays outside these tests.

Register the two signing suites. Their offline publisher control drives the real
publisher CLI, actual fixture signing, a private TLS release service, a real local
Git origin and a bounded external `gh` adapter. It covers first publication,
partial-upload restart, exact repeat, tag drift, current-content changes and a
push race without creating GitHub qualification releases. Unsafe-key controls
include fixture-only private material in a clean portable tree.

These controls establish bounded local behavior. Actual signed public assets,
tag and default installation still require readback during the one real product
release. The owning task remains #49; this introduces no duplicate outcome or
private release-state authority.
