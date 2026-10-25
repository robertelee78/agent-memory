# Signed release locator: media-type correction

Scope: unpublished #49 implementation on the v0.0.3 release branch. Neither
v0.0.1 nor v0.0.2 shipped this signed-acquisition implementation.

## Observation and impact

The first controlled TLS journeys passed acquisition but a read-only check of the
actual public GitHub latest-release endpoint refused `Accept: application/octet-stream`
with HTTP 415 (curl exit 56 in this environment). The same endpoint accepted
`Accept: application/vnd.github+json` and returned published v0.0.2. The incorrect
header would have stopped first installation before candidate execution; no
release was published and no live installation was changed.

## Root cause

The C2E pattern uses GitHub CLI for release metadata, which selects the API media
type. AM's unauthenticated curl implementation reused its binary-asset header for
the JSON API. The first isolated release server accepted every header, so it did
not exercise GitHub's content negotiation. A locally successful download was
insufficient evidence that the real public locator worked.

## Correction and required proof

The shared downloader distinguishes JSON API requests from binary assets. The
controlled server now returns HTTP 415 for an inappropriate API media type, so
the real one-line/installed journeys exercise this boundary. A fresh read-only
public check using the corrected downloader returned stable v0.0.2; that is API
negotiation evidence, not signed-release acceptance or v0.0.3 publication.

The correction belongs to the existing #49 attempt and ADR-0015 acceptance scope;
it creates no additional outcome or lifecycle state. Qualification must still
bind the final candidate and portable digest, cover the local macOS/Linux
journeys, and verify actual signed public assets before first default activation.
