# Bounded context assembly

The portable [assembly guide](../../skills/agent-memory/assembly/GUIDE.md)
defines `assemble_context.py preview` and `check`. They use fresh policy-qualified
recall, preserve complete source metadata and lifecycle labels, group exact
duplicates without losing provenance, and select across useful memory types.
Different conditions or validity remain separate. Whole serialized context,
including its evidence/header, must fit the explicit UTF-8 byte budget.

Known relevant contradictions are caller-declared groups of exact source hashes.
Overlapping groups merge; all members must still qualify under recall's hard
filters. A disputed unit fits whole or assembly refuses. It remains unsettled
evidence for the detecting developer, with another-developer clarification
recommended when applicable; no message is sent automatically.

Token qualification uses an explicit portable boundary: the invoking harness
counts the exact serialized text with its actual tokenizer, then supplies a
strict counter bound to that text's hash. The helper rebuilds fresh context and
checks the declared count against the token budget. It cannot authenticate the
tokenizer or validate a fabricated measurement. Without an actual measurement,
a preview stays unusable; an over-budget check withholds the text. Callers reserve
space for other prompt content separately. This protocol does not promise a
provider tokenizer implementation or semantic contradiction detection.

Public cases use the copied skill, real Git state and isolated private roots.
They exercise diversity, exact duplicate provenance, distinct conditions,
Unicode byte accounting, a real disposable harness tokenizer, stale/invalid
counters, atomic yellow/blue disputes, overlapping groups, excluded dispute
members, orientation labels and changed/locked state. The disposable tokenizer
does not establish counts for any commercial model. Whole-bank hashing still
scales with bank size; large-bank quality and latency require measured workloads.
