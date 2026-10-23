# First-release safety canaries

The top-level gate runs `security-canaries` and `identity-isolation` from
`tests/contracts.json`. Both copy the portable skill into a disposable world
and invoke its public scripts with argument lists. They do not import product
algorithms, use a live account, contact a server or rely on optional indexes.

| Boundary | Observable check |
| --- | --- |
| Structural routes | Valid context routes retain scope/lifecycle; traversal and unknown busy/interrupted markers refuse without repair. |
| Current guidance | Every inactive lifecycle state, weak trust, hidden visibility, mismatched applicability, supersession and exact validity-time boundaries exclude guidance. |
| Source authority | Unqualified automatic assistant/tool/external claims remain review-required; current file bytes are checked and stale evidence refuses. Byte eligibility does not establish truth or publication. |
| Poisoned imports | Role/permission claims, authority overrides and copied conversations quarantine without returning their bodies or executing embedded commands. |
| Privacy and bounds | Synthetic credentials are redacted; oversized input refuses; private audit omits bodies and content hashes. |
| Terminal deletion | A content-free marker survives an explicit fixture Git transfer and suppresses an older cached body and its hash. |
| Project isolation | Replacing a selected bank with another project cannot silently rebind the checkout or authorize recall, even when the request matches its new header. |
| Shared attribution | Two developers with separate disposable machine state use the same project bank, while a changed developer selection invalidates an older checkpoint proposal. |
| Local work | Governed checkpoint changes the active context while preserving other memory, original conditions/evidence, staged code, index bytes and HEAD. |

These tests cover the file, lifecycle-model, identity, checkpoint and mediated
policy surfaces available now. The fixture owners perform Git transfers and
manual deletion-marker construction explicitly; those setup actions are not
claims that automatic publication or governed forgetting has shipped. Likewise,
the lifecycle evaluator's decision is not a persisted lifecycle mutation.

When admission, retrieval, correction/forgetting, sharing and install/update/
uninstall arrive, their stories MUST extend these canaries through the actual
new public entry points. The full release gate MUST still fail while any other
required suite is pending. Passing these canaries does not prove semantic
completeness, exhaustive secret detection or improved coding outcomes.
