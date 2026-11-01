# v0.0.13 prepublication qualification

Implementation accepted: `6f31b9542bdd324a16360ccb7753c0f67321cb2d`.

The independent implementation reviewer found and verified corrections for
redaction edge cases and HTTP-status precedence. Fourteen focused tests passed
on macOS (11.389 seconds) and Linux (8.959 seconds) at the preceding candidate.
The final regex-only correction was checked with the three targeted regressions,
independently passing in 0.004 seconds. The generated bootstrap matched the shared
verifier byte for byte. Portable pack validation passed.

The affected Intel Mac's read-only candidate preview passed, with explicit
not-installed guidance and no restart advice. At that later preview an installed
wrapper and receipt were already present through another actor. This task did
not apply an installation there. The original failed connection remains
unattributed; later successful requests do not prove its exact cause.

Actual Opus design review accepted the bounded-download design and final preview
amendment. No initial outer-curl retry, weaker authentication or memory-bank
repair is claimed. The installer correction is ADR-0021 T9 / issue242; separate
bank recovery and hook-health defects are not part of this implementation.

Public postpublication journeys are separate evidence and will be recorded only
after they actually run. No prospective public success is asserted here.
