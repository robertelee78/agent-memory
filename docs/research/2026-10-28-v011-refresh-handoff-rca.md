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


## Source binding and newly selected consistency guard

The deployed source baseline is immutable signed v0.0.10 commit
`76a7886408e8a295adca7f3df2536efdb1a107b7`:

- [`bank_transactions.py:50`](https://github.com/robertelee78/agent-memory/blob/76a7886408e8a295adca7f3df2536efdb1a107b7/skills/agent-memory/scripts/agent_memory/bank_transactions.py#L50)
  unwraps the full result's proposal before validating transaction fields.
- [`memory_refresh.py:489`](https://github.com/robertelee78/agent-memory/blob/76a7886408e8a295adca7f3df2536efdb1a107b7/skills/agent-memory/scripts/agent_memory/memory_refresh.py#L489)
  recognizes a nested v2 source-bound proof; its
  [output preflight at line527](https://github.com/robertelee78/agent-memory/blob/76a7886408e8a295adca7f3df2536efdb1a107b7/skills/agent-memory/scripts/agent_memory/memory_refresh.py#L527)
  obtains the repository from the inner proposal.
- [`bank_transactions.py:138`](https://github.com/robertelee78/agent-memory/blob/76a7886408e8a295adca7f3df2536efdb1a107b7/skills/agent-memory/scripts/agent_memory/bank_transactions.py#L138)
  computes the planned delta from retained observed/final hashes without reading
  current files; this permits metadata validation without breaking valid retries.

An independent public baseline probe confirmed valid full-file import/no-op,
retry, legacy inner compatibility and source/output refusal controls. A separate
baseline confirmed outer operation, changed-path and extra-repository metadata
are currently ignored when the inner proposal is valid. That is not an inner
pin or authorization bypass, but it can mislead a reader about the prepared stage
or delta. Promoting the full response as the preferred handoff therefore includes
a small new report-consistency guard, not only a documentation edit.

The new guard will validate known prepared metadata and pure inner-derived delta
before output preflight or apply. It will not perform fresh semantic/source checks
before historical retry inventory. Existing proof, transaction, authority and
under-lock machinery still decides all actual changes. A wrapper cannot grant
permission, claim completed freshness or substitute for an applied receipt.
