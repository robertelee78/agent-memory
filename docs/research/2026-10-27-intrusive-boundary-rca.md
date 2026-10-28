# Intrusive boundary workflow: operator incident and confirmed causes

Status: investigation; corrective design is proposed, not shipped. v0.0.8's
completed delivery record is preserved. No consumer repository files or runtime
permissions were changed during this investigation.

## Observed incident

The operator supplied a Linux Codex PM conversation showing a task answer,
multiple memory-only tool cards, a failed quick refresh, a successful source
assessment, then the same PM question again. The quoted other-repository product
decision is not copied into this repository's memory or treated as an approval
for Agent Memory design.

## Confirmed refresh defect

Read-only reproduction on the operator's Linux machine (192.168.1.202) used the
installed signed v0.0.8 at c92b605. `am refresh current` for the reported repository
returned pending-inspection / refresh-unverified. Doctor simultaneously reported
a valid bound bank, a verified runtime and no failures. Direct invocation of the
same current function exposed the hidden exception: `Untrusted transaction
recovery location`, in memory_refresh.current.transaction_inventory.

The actual ownership/mode chain includes a current-user-owned home 0700,
.local 0775, .local/state 0700, agent-memory 0775, checkout-state 0775 and
operations 0700. The shared trust_paths.inspect policy recognizes private ancestor
barriers. The quick-refresh descriptor walker instead rejects group-writable
intermediate state directories regardless of that barrier. These policies
contradict each other. This is not evidence of corrupt canonical Markdown, an
identity mismatch or an unsigned installation. Changing existing directory modes
is not the proposed remedy. Permission, owner, link and rebinding checks must be
harmonized with the established protection policy without weakening unprotected
paths or adopting foreign state.

The CLI catches this exception and replaces its specific reason with a generic
refresh-unverified explanation. Doctor does not exercise the failing quick-refresh
check. Its healthy setup result therefore does not resolve or explain this
operation failure.

## Confirmed workflow and presentation gaps

- The installed CLI already supports stdin no-learning assessment, so private
  create + put + file-input acknowledgment were unnecessary for that route.
- Codex boundary completion always prints its full JSON result; it lacks the
  quiet-success option already available to the Claude adapter.
- The turn guide places its routine observation protocol after the much longer
  admission/private-workspace flow, encouraging unnecessary navigation and calls.
- Source assessment validates an agent assertion about a source, not successful
  refresh. Its verified:true can legitimately coexist with failed refresh. It
  must not be presented as complete memory-workflow health.
- Stop can resume an agent after an answer has already streamed. Some fallback
  instructions explicitly require repeating the task answer to protect SDK final
  result consumers; that can duplicate the operator-visible dialogue.
- v0.0.8's arithmetic qualification checked source tracking and answers but did
  not make memory command count, private-workspace chatter or repeated PM
  questions fail acceptance. It was insufficient evidence for unobtrusive use.

## Native constraints and unresolved product choice

Official Codex documentation says suppressOutput is parsed but not implemented;
async completion does not start a new model turn; Stop blocking creates another
continuation prompt. Quiet stdout cannot guarantee hidden tool cards. A portable
CLI does not establish a host-independent private channel for model assessment.

The operator was asked about compact versus invisible calls, then explicitly
requested fixes and a new release. Proceed with the minimal one-command route
within the established skill/CLI contract; do not treat the absence of a preference
vote as a reason to leave clear defects unfixed. Zero visibility is not promised.
Do not silently weaken automatic capture, replace semantics with fixed no-learning
receipts, or promise invisible UI using an unsupported setting.

Primary reference: https://learn.chatgpt.com/docs/hooks
Relevant source: memory_refresh.py, trust_paths.py, codex_boundary.py,
claude_stop.py, codex_stop.py and turn/GUIDE.md in the signed v0.0.8 source.

## Exact dialogue phase evidence

Read-only inspection of the operator-specified native Codex turn found a
687-byte assistant commentary message and a 312-byte final-answer message, then
one task-complete event. These align with the reported full PM response before
the memory commands and repeated question afterward. No other-repository message
body was copied into this RCA. The observed duplication therefore occurs in
ordinary commentary/final emission; a Stop continuation is not necessary to
explain this incident. The separate fallback repetition risks above remain real
but must not be mislabeled as this trace's established cause.

## Later installed-artifact mismatch

Read-only outside verification on 2026-10-28 established that all 188 owned v0.0.8
pack files retained their receipt hashes and modes, and both signed metadata
files matched the trusted release. Only 34 CPython3.14 bytecode-cache files were
extra. Their creation immediately follows our earlier diagnostic import using
`python3 -I` without `-B`. The cache header timestamps/sizes match the original
source files. This is attributable to our diagnostic, not a demonstrated user
edit or changed signed source. Private backups and exact pre/post inventories
were retained before atomically quarantining only those generated cache files.
The receipt and journal were not regenerated or modified.

This integrity refusal is correct: Python can read existing bytecode even with
-B, so skipping executable caches during source verification is not an acceptable
repair. Direct diagnostic imports MUST disable cache writes, and source repair
MUST preserve unknown changes and independently verify signed payloads before
activation. The separate refresh path-policy defect remains to be repaired in
the corrective release.
