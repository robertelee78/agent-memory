# Shared-memory mission review and root-cause analysis

Date: 2026-10-24. Product baseline: `90dbf6559a026f180bd93eb460d08827fea240b2`
(v0.0.2). This is design input for v0.0.3, not acceptance of that release.

## Outcome and evidence boundary

The operator asked whether actual repository banks support useful shared learning
between separate sessions, worktrees and machines, rather than merely validate.
Three existing banks were inspected read-only using installed CLI diagnostics,
identity resolution, canonical files, source links, Git state and fresh GitHub
reads. All three resolved to distinct repository identities; their bank bytes
matched local committed bytes and freshly observed remote `main`. One checkout
was behind remote code while its bank still matched. Local links resolved and
bank/runtime mechanical checks passed after the separate runtime repair below.

These are useful initial orientation banks. Their lack of standalone lessons
does not prove failed capture: they were recently initialized, and this review
did not observe a complete durable-learning event through capture, independent
landing and recall on a second machine. No cross-project contamination, corrupt
bank, wrong-task action or measured large-bank recall degradation was demonstrated.
Fresh GitHub equality establishes publication of these bytes, not semantic
freshness or correctness of every statement. Private bank contents and operational
evidence remain private; the findings below use public product source and the
agent-memory bank candidate as reproducible references.

## F1: Context routes do not distinguish information needs

**Observed defect.** Newly initialized banks give different context files the
same purpose, `Source-linked initialization orientation`, and trigger,
`When starting or resolving project context`. This appeared in all three banks
and the initial agent-memory bank candidate (`1e65bee0253d163b8574b35483e90b3a35d4c995`).
Its independently reviewed correction (`7b1b4f029af1216039a8496d66ee9f94b0670e50`)
uses distinct purposes and triggers without changing product code.

**Cause.** `initialization.py` hardcodes these fields in both proposal generation
and transaction validation. The validator enforces the same generic output;
schema validity cannot establish useful progressive disclosure. A checkpoint
preparation also uses a generic startup trigger.

**Effect and limit.** An agent cannot use these routes to confidently distinguish
product intent from architecture, implementation evidence or current handoff.
Unnecessary loading and missed context are risks; this review did not measure
either at scale. Mechanical validation did not detect this quality gap.

**Correction and prevention.** Generate and review distinct, bounded routing
metadata. Preserve useful existing routes. Prove that unrelated task needs lead
to different relevant context choices without losing required conditions.

## F2: Observation provenance and applicability need separate treatment

**Observed mechanism.** `initialization.review_value()` requires implementation
observations to carry branch/revision conditions matching the inspected checkout.
`claim_lines()` renders these as applicability. A bootstrap branch or exact HEAD
can therefore become the apparent validity boundary of a useful code observation.
Adding only memory changes HEAD even when referenced code is identical. Source
links in rendered prose can point to mutable relative paths while the observation
is described as pinned to a historical revision.

**Cause.** The current representation uses conditions to establish both where
an observation was made and when the statement applies. This conservatively avoids
claiming main publication, but does not provide a reviewed way to distinguish
unchanged evidence from a truly branch-dependent rule.

**Effect and limit.** Readers may discard still-useful knowledge or wrongly
generalize historical observations. Neither false guidance nor actual loss of
recall was established here. Some inspected source files changed; others did not.
Changed bytes alone do not prove a remembered statement false.

**Correction and prevention.** Preserve immutable observation evidence separately
from semantically reviewed applicability. Explain unsupported, historical or
unavailable evidence. Do not automatically broaden a claim because a branch was
merged, HEAD advanced or a referenced file happens to match.

## F3: Checkpoint portability is not established by file recall

**Observed limitation.** `checkpoint.render()` places task/branch conditions,
evidence and provenance in the Markdown body. `retrieval.retrieve()` gates context
prose using router scope; unlike typed records, it does not interpret these body
conditions as machine selection gates. Exact-path lookup with an unrelated task
returned active context as `orientation-only`, with `trust: null` and an explicit
instruction to qualify its statements. That is not proof of incorrect ordinary
search or admitted guidance: the caller explicitly requested the file.

Some inspected handoffs label machine-local worktrees and unpublished evidence
as local and limited. Those qualifications are appropriate, but a different
machine cannot inspect those artifacts. A checkpoint should not become another
session's assignment or imply that private local evidence has been published.

**Cause.** The product has a file-oriented handoff plus reader qualification.
This audit did not establish the specific proof that
unavailable local checkpoint evidence remains properly qualified through resume
and assembly in another clone. Existing scoped-recall, branch-gate,
orientation-only and assembly tests remain valid within their demonstrated scope.

**Correction and prevention.** Define how scoped handoff is presented separately
from current applicable guidance. Verify ordinary recall, exact inspection,
resume and assembly with different tasks and machines. Preserve historical
inspection while preventing an unrelated next step or inaccessible artifact from
being represented as current shared guidance. Do not silently erase conditions.

## F4: Healthy runtime versus uninitialized repository is confusing

**Observed usability defect.** In agent-memory before bank adoption, `am status`
reported `runtime active`, `memory not_located`, and a warning that project binding
was missing or differed. The operator reasonably asked whether it was broken.

**Cause.** The diagnostic combines absence and mismatch in one warning, without
making the normal first-use state and its safe next step sufficiently clear.
There was no bank to corrupt. Runtime installation does not establish repository
identity and must not silently bind a new project.

**Correction and prevention.** Distinguish uninitialized, binding mismatch,
missing previously known bank and runtime damage. Give an actionable setup or
recovery path without automatic identity creation. Verify plain `am status`
from these user-visible states, retaining complete JSON evidence.

## F5: Ad hoc Python inspection can alter the inspected runtime

**Observed incident.** Doctor found 18 untracked `.pyc` files in the installed
skill's Python package. All 140 receipt-owned files retained their expected bytes.
The extra cache files caused artifact mismatch. Exact caches were backed up and
removed under the existing lifecycle lock after verifying owned bytes and receipt;
doctor then passed. No source or receipt was rewritten.

**Cause and uncertainty.** Direct Python imports can write executable bytecode
inside the inspected package; packaged helpers already disable that behavior.
The cache mechanism explains the mismatch. The process that created these files
has not been authenticated, so attribution to a particular reviewer or command
remains unproven. This is an inspection-workflow gap, not proof normal CLI use
creates drift or that the integrity check is wrong.

**Correction and prevention.** Document and exercise read-only inspections with
bytecode disabled before imports. Keep fail-closed artifact verification: ignoring
executable cache files would weaken the boundary. Preserve unknown drift and
explicit recovery evidence rather than deleting arbitrary runtime contents.

## F6: Lifecycle safeguards are not publisher authentication

**Source-reviewed controls.** The v0.0.2 bootstrap stages a Git checkout before
running its entry point. The installed updater checks the recorded remote/ref,
clean checkout, current receipt and target identities, fast-forward ancestry,
version downgrade, declared compatibility and portable artifact before activation.
Lifecycle mutation uses a shared flock, per-target atomic switches, readback and
an external receipt/journal with rollback. Uninstall preserves modified targets
and surrounding activation text. Data purge is a separate exact-bank,
inventory-bound confirmed operation. These mechanisms are present in source;
this review is not a new complete adversarial lifecycle qualification run.

**Known limitation and expectation gap.** Artifact digests establish byte identity,
not publisher authenticity. Neither bootstrap nor updater verifies a pinned release
signature. The bootstrap executes source obtained through its selected Git channel;
it is not an independent trusted verifier of that source. ADR-0007 explicitly
allowed the initial HTTPS fast-forward channel, and doctor warns about unsigned
distribution. Signed immutable distribution is already tracked by #49, currently
v0.0.4 under ADR-0013. This is not a newly discovered violation of the approved
v0.0.2 signing scope. However, a lifecycle `verified` receipt can be mistaken for
a stronger trust guarantee unless the public documentation makes the boundary clear.

**Correction and prevention.** Publish a concise trust/control matrix distinguishing
source authentication, integrity, ownership, compatibility and recovery; verify the
public refusal/preservation paths locally. Do not describe checksums or a verified
receipt as signature verification. Whether to bring #49 into v0.0.3 is an operator
decision, not an implicit consequence of this review.

## F7: Existing local state permissions are not security-gated

**Observed gap.** An isolated public `install --check` with an existing installation
state directory set to `0777` returned a successful install plan. No installation
targets were changed. The fixture's enclosing directory was private; this shows
policy acceptance, not access by another user or a demonstrated exploit.

**Cause.** `installation.make_state_directory()` checks an existing directory's
type but not its owner or unsafe write permissions. `preflight_path()` checks
writability/searchability and layout rather than who else may modify it.
`diagnostics.permission_check()` likewise checks effective read/write/search
access, not ownership or group/other-write exposure. New leaf state directories
use `0700`, and record files use `0600`; those defaults do not establish the
trustworthiness of pre-existing paths or their containing directories.

**Impact and limit.** The receipt, journal and executable runtime are local trust
inputs. If another principal can replace them through a writable path, hashes
stored in the same mutable trust domain do not independently protect ownership.
No live installation was found compromised, and no cross-user attack was executed.

**Correction and prevention.** Define and enforce a runtime/receipt/ancestor
ownership and permission policy before trusting records, executing candidates or
mutating targets. Keep this separate from intentionally shared repository-bank
permissions. Refuse or explicitly qualify unsafe layouts without silently changing
the user's permissions. Exercise private defaults, hostile writable parents,
symlink changes, and legitimate operator-managed layouts in isolated local checks.

## Planning and unresolved boundaries

[ADR-0014](../adr/0014-useful-shared-memory-quality.md) proposes bounded corrective
outcomes for v0.0.3. Existing daily-use reliability (#99) and growing-bank recall
(#98) retain their broader evaluation ownership; the new work is not a duplicate
stress-test program. Run outcome checks locally with isolated repositories and
client roots. No new expensive hosted matrix is justified by this review.

The agent-memory bank is prepared in draft PR #164, outside `skills/agent-memory`.
The managed installer clones a full source checkout, so a root bank would exist
there after landing, although it is outside the client skill. Whether consumer
managed checkouts must physically omit that bank remains an operator question;
it is not reported as a proven leak or an approved sparse-checkout redesign.
Task #163 remains pending that distribution decision. This review changed none
of the three audited repositories and does not certify automatic capture or the
full cross-machine learning loop from static bank inspection.
