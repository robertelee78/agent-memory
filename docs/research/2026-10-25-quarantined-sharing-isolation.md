# Quarantined record bodies blocked independent sharing

Work: #99, `ADR-0009:daily-use-reliability`. This is one repair within the
approved daily-use design, not acceptance of the complete workload or release.

## Cause and observed predecessor

The v2 changed-learning review used the legacy complete-bank safety reader
before selecting and withholding evidence. A single unsafe standalone record
therefore refused inspection before the agent could review an unrelated safe
update. The initial boundary, Git-tree reader, merge validator and publication
readback each enforced this whole-bank refusal. Merely redacting a disclosure
page could not make the complete workflow usable.

The retained predecessor experiment executes actual public `inspect-v2` from
`747518d07c31104c8de6c3dcdbf4d035ad3808e0`, with a fixture-only unsafe local
record and independent landed orientation. Its source hashes, argv, failure and
original-state comparison are retained in
`/tmp/agent-memory-v003/quarantine-predecessor-747518d/`. It is a failure
observation, not an acceptance pass.

## Repair boundary

Only the v2 review and its versioned publication/refresh consumers may observe
an opaque unsafe standalone-record body. Complete format, size, topology,
identity, metadata and navigation checks still run. Unsafe structural captions,
paths or metadata remain a refusal; unsupported isolation is never silently
treated as safe. Legacy v1 behavior remains strict.

Selected hidden/unsafe bodies and metadata are withheld from review pages and
remain held even when the agent supplies a `related` label. The held identity
closure includes exact linked older claims: failing to import a replacement
cannot make its obsolete claim current guidance. Safe independent changes may
continue when their actual plausible-match review does not depend on withheld
evidence. Artificially selecting unknown evidence as a dependency correctly
keeps that change pending; the helper cannot establish its meaning for the agent.

Complete private candidate banks retain originals on their existing side.
Neither candidate composition nor publication replay may introduce changed
unsafe payloads to another side. Imported recall excludes held paths. A v2
check without an explicit private output omits complete candidate banks from
stdout; ordinary page/summary output must not disclose retained raw originals.
Existing transaction, audit, repository-rule and source-freshness checks remain.

## Controls and remaining work

`quarantined-sharing-v2` exercises actual public inspection, pages, review,
check, refresh preparation/application and publication/readback. Its authored
mechanical controls cover local and landed unsafe bodies, independent useful
updates, hidden/unsafe replacement nonresurrection, unchanged feature state and
structural refusal. Synthetic fixture relationships are not general semantic
understanding or exhaustive secret detection.

The complete 12-session/1,000-lesson and 24-session/10,000-lesson daily-use
schedules, final native qualification and independent acceptance are still
required. This repair alone does not close #99.
