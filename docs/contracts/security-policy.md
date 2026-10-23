# Mediated project security policy

The portable [security guide](../../skills/agent-memory/security/GUIDE.md)
defines source classification, project authorization, bounded scans, quarantine,
separate procedure-approval eligibility and private content-free audit. Checkout
and copied pack expose the same `scripts/security_policy.py`.

The evaluator distinguishes current user statements, assistant claims, tool
observations, external content and explicitly verified repository source bytes.
It resolves the actual bank before evaluation, checks selected source bytes
again and pins the complete bank observation. Matching a source hash establishes
bytes, not truth, test success, publication or human authentication. Unqualified
automatic assistant/tool/external admission remains review-required. A source
label inside memory cannot change the invoking session's trusted classification.

Listed token/private-key/credential patterns are redacted; suspected instruction
or permission claims and raw conversation quarantine. Quarantined instruction,
conversation and terminally deleted bodies are omitted from returned context.
The checker has bounded heuristic coverage and makes no exhaustive detection or
credential-validity claim. Separate current-operator approval binds procedure
eligibility to exact content, source version and project; eligibility does not
install instructions, admit a record or grant new task authorization.

Initialize and checkpoint previews reject flagged proposed content without
canonical writes. They recheck under the bank lock before persistence; initial
transaction recovery finish retains the original evidence and applies the same
content guard. Known-base rollback remains available when evidence changed.
Public single/multi observed writers record mechanical operations, without
claiming that raw replacement admitted or semantically qualified their payload.
Scope binding records its checked actor and proposed target as an explicit
namespace operation, not personal memory or authentication.

Audit files contain only fixed metadata, random identifiers, source classes,
phases and reason codes. They contain no content, titles, conversation, source
quotes or low-entropy content hashes. Exclusive atomic file publication and
readback make replay idempotent. Started/checked events never prove completion;
a required pre-write audit failure stops the write. A post-write audit failure
remains pending and must be repaired/replayed using the same operation. The
logs do not authorize or observe raw filesystem/Git access.

This foundation suite does not complete the broader first-release poisoning
canaries or outcome evaluation. Governed admission, retrieval, forgetting and
publication must use these guards when their separate milestone stories land.
No personal/global bank, cross-project federation, MCP service or executing
retrieved commands is implemented by this policy layer.
