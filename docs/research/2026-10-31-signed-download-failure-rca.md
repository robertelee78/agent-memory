# Signed-download receive failure: diagnosis and correction

The operator saw curl exit56 during a read-only installer preview on an Intel
Mac. The initial bootstrap had already downloaded; the error came from an inner
signed-release request. That implementation captured stderr but discarded it and
reported neither the failed endpoint nor a useful transport reason. It performed
one GET, so a temporary receive failure immediately reached the operator.

The original failing connection cannot be reconstructed from that generic error.
Direct discovery, manifest, signature and archive requests subsequently returned
HTTP200 on the affected machine, and the exact preview passed in the operator's
login environment. This supports a transient failure hypothesis, not attribution
to a particular proxy, VPN, TLS stack or GitHub endpoint. A separate initial SSH
non-login attempt lacked the user's supported pyenv Python; repeating in the
actual login environment removed that diagnostic-environment mismatch.

[Official curl error documentation](https://curl.se/libcurl/c/libcurl-errors.html)
defines exit56 as failure receiving network data. This code alone does not reveal
which peer or network component failed. No signature or source-integrity failure
was demonstrated, and no actual installation was applied on that machine.

ADR-0021 T9 specifies bounded selected-error retries and terminal diagnostics.
Each attempt has a fresh body stream; HTTP status and bounded stderr remain
separate from signed bytes. One monotonic30-second budget retains the previous
slow-transfer allowance across at most three attempts. Certificate, local-resource,
HTTP access/missing-asset and integrity failures stop without insecure fallback.
Final reports identify safe stage/host, actual exit/status/attempts and sanitized
curl detail, retain a private bounded report when possible, and name actionable
operator checks. Report creation failure must preserve the primary error.

Actual Opus and independent design reviews accepted source
`e032b19b898090bf0e349fb13c272c28dee3fa90`; implementation and public read-only
validation remain separate evidence. The initial outer curl fetch precedes this
downloader and is not represented as gaining automatic retries. The corrective
v0.0.13 scope is one task; eight larger roadmap items remain gated in v0.0.14.
