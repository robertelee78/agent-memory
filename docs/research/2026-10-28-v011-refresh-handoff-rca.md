# Direct prepared-refresh handoff RCA

The reported to-auth refresh had already succeeded after its retry: the retained
apply receipt reported refreshed, relevant imports read and fresh authority,
with no canonical file changes. This report records the interface friction;
it does not assess that other repository's source observations or repair its bank.

The agent generated JavaScript to extract a nested proposal and compare proposed
files with the current bank. An unquoted dotted object key caused a parser error;
quoting it alone would still leave await inside a non-async callback. The corrected
retry parsed and ran. These are model-generated script errors, not an observed
AM parser or storage failure.

AM's refresh guide tells the agent to save the exact nested proposal and shows a
different refresh-proposal.json file in apply. That invites an extra extraction
step even though existing bank_transactions.proposal_value already accepts the
full prepared result and unwraps its proposal. The refresh v2 input reader finds
the nested proof, and output preflight uses the same inner repository pins.
The saved complete private prepare response can be passed directly to apply.
The concise stdout artifact summary is not that stored response.

Preparation already returns changed_memory_paths. The final transaction receipt
also reports its changed paths and writes. Independent text comparisons do not
prove refreshed authority, relevant recall, or an applied/cursor completion.
A no-byte-write refresh can legitimately establish checked source/recall/cursor
state. Skipping apply merely because files compare equal would lose that proof.

The bounded proposed correction documents direct private-file handoff, exact
retention of the governed result, native delta fields and stage distinctions.
It changes no source authority, memory content, CLI schema or workflow gates.
Public-command qualification must cover the full envelope in v2 private output,
legacy compatibility, unchanged and changed imports, pinned-source refusal and
safe output paths. No tests will replay or modify the original to-auth operation.
