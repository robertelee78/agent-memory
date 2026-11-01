# Secondary bank and hook design review

The requested actual Opus reviews used `claude -p --model opus --safe-mode
--tools '' --effort low --max-turns 1 --output-format json`. All three single-turn
calls completed with exit 0, provider subtype `success`, and reported
`claude-opus-5-5` model usage. No reviewer tools were enabled. This is static
design review, not implementation acceptance, release assignment, actual bank
repair, terminology capture, or proof of live hook execution.

The [structured record](data/2026-10-31-secondary-memory-rca/opus-review.json)
retains actual parsed verdicts, complete notes, reviewed file/prompt hashes and
raw-provider output digests without provider session identifiers. Exact private
prompts and responses remain retained for this review. No CLI response was
relabeled as a semantic admission or memory receipt.

## Initial blocker and revision

Source `9888ed9d0f4922bd8c683c9efe4bc72f6130fcc9` received **BLOCK** in 44.43 seconds.
The sole material blocker was circular sequencing in ADR-0010: content needing
admission could neither stay in an invalid bank nor enter ordinary admission
while that bank was invalid, and a backup alone did not authorize omission.

The revision defines source-bound private pending-admission units, visible
pending status, layout restoration distinct from semantic completion, and
normal admission after eligibility and applicable pending-frontier review. It
forbids clearing unreviewed units, asserting complete absence/coverage from them,
downgrading prior admitted guidance to make a layout fit, or inventing authority
from archived bytes. Acceptance now includes the oversized unsupported handoff
through pending layout restoration and separately sourced admission, plus
unconfirmed, omitted-frontier and guidance-downgrade controls. Diagnostic,
parser, identity and retention notes were clarified without raising normal caps.

Full revised source `8d90b4a3179420a1a47135c2ca2eb04bcc3c03d1` received **ACCEPT** in
35.69 seconds, with no material blockers.

## Cross-clone publication edge

The full review noted that a local private pending ledger cannot govern another
clone's recall after repaired shared content lands. The owner selected the
bounded publication hold: recovery-aware publication stays pending until every
private unit has a reviewed terminal disposition and semantic recovery is
complete. Local layout restoration and admission can resolve those units first.
No shared pending federation, marker protocol or cross-machine private-ledger
dependency is introduced.

The exact 11-line restriction and second-clone acceptance addition received a
narrow **ACCEPT** in 22.18 seconds at frozen source
`c8412b3fe9e1eb72f77cc8d0b92fcabd08957720`. The other reviewed files were checked
byte-identical to the full accepted revision by the review controller; the
reviewer explicitly limited its own inspection to the supplied delta, full
ADR-0010 and prior review. The final file hashes are retained in the structured
record. There are no unresolved material design blockers.

## Scope and task-design obligations

The accepted proposals remain separate:

- `ADR-0018:bank-validation-consistency`;
- `ADR-0010:source-preserving-layout-recovery`;
- `ADR-0007:owned-hook-health`.

Historical ADR frontmatter still covers only its original approvals. The new
sections remain proposals, unselected for the v0.0.13 installer work. Secondary
tasks remain **Designing** until their actual approvals are recorded; static
ACCEPT is not authorization to run a recovery or ship implementation.

The retained nonblocking notes must be addressed before implementation Ready:

- Bind the applicable pending-source frontier into admission proofs and define
  meaningful, separately sourced decomposition for units beyond normal limits.
  Unresolved units and publication blocked by them remain visible.
- Make local admission or reviewed terminal-disposition receipts sufficient to
  resolve units before publication; correlate publication receipts afterwards.
  Do not introduce a circular requirement for a publication receipt to unlock
  publication.
- Specify the all-resolved gate for local-only repair when authority is already
  valid as well as for invalid shared authority; no clone may silently lose
  coverage through machine-local pending material.
- State how complete eligible-bank observation relates to explicitly incomplete
  pending-learning coverage; do not weaken the normal comparison prerequisite.
- Directly exercise `client_hooks.parse` duplicate-key behavior in public
  acceptance; exact `locate` matching is not JSON validation.
- Rerun relevant public cases on the frozen implementation. Research hashes and
  static review are not executable implementation acceptance.

The operator-reported AGENTS.md substitution remains a separate execution and
orchestration failure under the existing completion contract. It does not create
a fourth technical feature, prove an actual terminology capture, or authorize
removing unknown current instructions. No actual bank was repaired here.
