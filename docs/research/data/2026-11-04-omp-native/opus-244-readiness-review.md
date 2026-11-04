**Verdict: TECHNICALLY READY FOR OPERATOR APPROVAL.**

This is a technical design verdict only. It does not approve operation or implementation. I made no writes and ran nothing, so I couldn't confirm the plan's SHA256 (`74b50c67…`). I reviewed `244-plan.md` as it is now: lines 117, 129–169, and the test rows at 277–291. I recomputed the calculator's outputs by hand.

## The prior blocker is resolved
The plan no longer limits metadata and scope chain by their standalone size. The fixture constraint is now that each canonical decision plus its full target, as actually serialized and nested inside the proof, fits in 5,723 bytes (lines 167 and 169). Line 169 says that if any real value is larger, the fixture is adjusted without losing source meaning, or apply refuses. This is the fix the previous review asked for.

## Calculator checked against the pinned source
- **Serializer.** It matches `developer_identity.encoded`: sorted keys, `ensure_ascii=False`, `indent=2`, a trailing newline, counted as UTF-8 bytes.
- **Target shape.** It matches `judge` (`comparison.py:196-199`): `{path, sha256, id, version, required_preserved_metadata, scope_chain}`. `scope_chain` is a list of scope strings (`bank_layout.py:99-121`), so `['repository']` is a valid shape.
- **Metadata.** The calculator's metadata has the keys `records.record` requires, including `admission-reason`, `trust` and `visibility`, which active records must carry (`records.py:130-131`). It also follows the shape `admission.draft` produces plus the extra fields admission adds.
- **Nesting arithmetic, recomputed.**
  - Inside the proof, each metadata line is indented 6 bytes more than when encoded alone. The metadata spans 33 lines, so 803 − 1 (trailing newline) + 33×6 = **1,000** bytes. ✓
  - The scope chain nested is `[\n` + 8 spaces + `"repository"` + `\n` + 6 spaces + `]` = **30** bytes. ✓
  - The calculator finds this value by swapping the field for `null` and adding back 4 bytes, which measures it exactly.
- **The per-row costs are true upper bounds.**
  - Each row's cost is measured as the first item added to an empty list. That first item costs more than any later item by the width of the closing-bracket indent, so it is a safe ceiling.
  - The outputs confirm this. The gap between the bound and the full serialization is 314 bytes = 280 (8,500 − 8,220) + 7×4 + 4 + 2. In the post-resolution case it is 316 = 280 + 6×4 + 2×4 + 2×2. Both match exactly, so the numbers are consistent with each other.
- **Classifications.** Only `equivalent`, `refinement` and `scope-correction` produce targets, and `scope-correction` is the longest of those. `distinct-conditions` is the longest pending classification and `partial` the longest coverage value. Each rationale is capped at 2,048 bytes by `safe_text`. Escaping can at most double the size because all control characters except `\n` and `\t` are rejected.
- **Totals.**
  - At apply: 8,500 + 8×5,532 + 2×5,723 = **64,202**, leaving 1,334 bytes.
  - After resolution: 8,500 + 7×5,532 + 3×5,723 = **64,393**, leaving 1,143 bytes.
- **Fixed overhead.** The measured overhead of 8,220 bytes uses the `create-distinct` disposition. The longest disposition, `provenance-identity-review-required`, adds 20 bytes. The 280-byte reserve covers that.

## Earlier clarifications are now settled
- **Decision shapes.** Canonical decisions stay exactly `{path, classification, rationale}` and only pending decisions carry a claim (line 131).
- **Digest locations.** `reference_map_sha256` and `review_digest` sit in `judgment.observation.recovery_frontier`, empty values are stated explicitly, and the circularity of the review digest is handled (line 117). This matches the calculator's frontier object field for field.
- **Disclosure row.** The 5,000-byte limit on the canonical disclosure row now depends on a concise, independently reviewed candidate body (line 165). Disclosure is measured as standalone `encoded(row)`, which matches `comparison.py:124`.

## Non-blocking notes (not required for readiness)
- **The newly admitted record may come close to 5,723 bytes.** The third canonical row after resolution is the newly admitted record, and real admission adds metadata the calculator doesn't model (`admission.py:205-215`; my rough estimate):
  - Timestamps from `datetime.isoformat()` carry microseconds, adding about 24 bytes.
  - A `repo:…#sha256=…` evidence reference adds about 70 bytes or more.
  - The `admission-reason` is built from several assessment fields, so its size depends on that text.
  
  Its real path, `topics/<uuid>.md` (46 bytes), saves about 144 bytes compared with the 64-byte all-quotes test path, which appears twice. The plan already handles an overflow by adjusting the fixture or refusing, so this isn't a soundness gap. Measuring that record's real size early would avoid a late fixture change.
- **Wording.** Line 167 says "actual nested proof contribution". It should be read with line 161, where `d(x)` assumes the largest allowed rationale and only the metadata and scope chain are taken from the real row. Saying this explicitly on line 167 would remove the ambiguity.