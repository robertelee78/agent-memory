# Keep navigation catalog maintenance off ordinary recall

Work: #98, `ADR-0005:growing-bank-recall`.

The combined `56cd5846484b798787d86680265968fb67eaac6f` literal-regression
observation found that every ordinary reader generated and republished the
optional semantic-navigation catalog. This performed extra whole-bank hashing
and derived writes even though ordinary lexical/exact recall never consumed
that catalog. Source qualification itself remained correct.

At 1,000 lessons the two recorded ordinary-reader samples were 1.135 and
1.166 seconds, with four complete canonical payload reads per operation and
`catalog_updated:true`. Other local evaluation processes were active, so these
samples are diagnostic observations, not an uncontended performance pass.
The fixed literal baseline and its earlier approved improvement target remain
separate from the operator's under-five-second 50k warm semantic-qualification
target. Neither target is waived by this finding.

Catalog generation/publication now occurs only for a request carrying the
explicit semantic-discovery trace. Its missing/corrupt-cache path still falls
back to complete native qualification and can rebuild the optional witness.
Ordinary recall continues complete native qualification, preserves any existing
navigation catalog and truthfully reports no derived cache writes. Canonical
hashes, applicability/invalidation gates and the final source recheck remain.

The public `recall-catalog` control starts with a genuinely primed navigation
catalog, then performs ordinary recall and compares exact catalog bytes,
modification times, returned current identity and unchanged canonical/feature
state. Performance and final-source qualification must be measured separately;
this repair alone does not close #98.
