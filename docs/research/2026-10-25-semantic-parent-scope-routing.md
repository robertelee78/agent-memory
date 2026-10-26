# Advertise existing component scopes before expanding a topic

Work: #98, `ADR-0005:growing-bank-recall`. These observations supplement the
retained map-coverage RCA; they do not relabel earlier failures as passes.

The `56cd584` map-corrected corpus completed all 16 questions at each scale.
At 1k, all nine positive questions found their expected records, with one extra
related deadline record on the retry question. Larger runs included adapter
failures: a free-text response where JSON was expected and node batches above
the controller's eight-node cap. Follow-up structured-output trials retained
their own failures, including rejected API schema variants and exhausted
per-call fixture budgets. Original artifacts remain under
`/tmp/agent-memory-v003/combined-56cd584-*`; their failures are unavailable
operations, not successful empty recall. A later wider-node/structured adapter
also exposed growing context and two budget failures. Raising the budget alone
does not establish efficient semantic recall.

One avoidable source of growth was visible in the fixture itself. Component
lessons had their actual narrow scope at each record route, but their enclosing
topic/leaf routes all said `repository`. A harness had to expand many headings
before seeing that those sources could not qualify without a component. This
is a map quality issue, not evidence that a keyword reader understood meaning.

Fixture version `semantic-navigation-v3-parent-scopes` derives each parent
component prefix from **all actual child record scopes**, without queries,
expected answer labels or special topic-name rules. Mixed/repository children
retain a repository parent. Native bank validation and exhaustive checks confirm
that each added parent constraint is already implied by the record's existing
scope. Every record payload, metadata, condition, identity and evidence byte is
unchanged at 1k/10k/50k; the original frozen literal benchmark is unchanged.
The retained verification is
`/tmp/agent-memory-v003/semantic-parent-scope-preservation/result.json`.

Unknown applicability remains unknown. A parent scope neither proves that the
question needs that component nor makes an excluded lesson absent. The harness
must retain those limits and inspect applicable repository knowledge elsewhere.
New fixed-question runs must identify the new fixture/adapter versions and
report selected context, model cost, uncertainty and failures separately from
native qualification latency. No universal recall or five-second harness claim
is made here.
