---
kind: operation
domain: sharing
---

# Publish admitted memory independently of feature code

## Use when

At a natural turn boundary after durable capture or maintained memory changes,
a session MUST automatically prepare and attempt publication of the admitted,
independently useful delta. It MUST preserve conditions and evidence for learning
from unfinished code, and MUST NOT imply that the code itself has landed.
It MUST inventory/resume its existing private publication request before making
another proposal. A required human review MUST remain pending; newly captured
local learning MUST NOT cause a duplicate PR at every boundary. Changed sources
require fresh review and an explicit continuation of the pending proposal.
Routine success SHOULD remain quiet. Conflicts, failures and consequential
corrections MUST reach the detecting developer promptly at the boundary.

## Required context

A session using this leaf MUST first load and follow the [sharing domain](../../sharing/GUIDE.md). It MUST open the [shared model](../../model/GUIDE.md) only when an unresolved semantic question requires it.

It MUST also follow [semantic reconciliation](../semantic/GUIDE.md) and the
root's security, admission, comparison and governance routes for their actual
operations. Publication MUST NOT substitute format validity for admission or
an agent's judgment for authenticated truth.

Publication MUST use the configured repository's actual GitHub default branch.
Local memory MUST remain useful when GitHub, credentials, `gh`, network access,
permissions, review or supported rule evaluation are unavailable. The session
MUST retain the delta and report pending sharing when that distinction matters;
it MUST NOT initialize another bank or widen scope to get around a failure.

## Inputs and preconditions

1. The session MUST complete admission/maintenance using the existing governed
   routes. Explicit removal of typed learning MUST follow governance and retain
   safe invalidation. It MUST NOT stage unrelated work or use `git add -A`.
2. The session MUST search/read plausible existing local and landed matches
   through [comparison](../../comparison/GUIDE.md), then inspect and read the
   complete selected changed-learning frontier through v2 semantic reconciliation. It MUST supply
   explicit meaning judgments, provenance-preserving duplicate groups and real
   dependencies. A clean text merge MUST NOT stand in for this review.
3. The session MUST save the complete resulting semantic `proof` in private
   storage outside the checkout, then invoke the portable helper:

An initialized, explicitly bound local bank MAY be published when authority has
never contained a canonical bank. Fresh discovery MUST inspect authority's
other possible bank locations and mainline locator history, including a custom
bank that was later removed. Looking only under the stale branch's configured
prefix MUST NOT establish absence. Discarded side-branch proposals are not
authority-tree ownership. Known hidden/independent repositories remain subject
to the sharing domain's explicit inspection rules. The actual base/landed hash maps
are empty; the observation reports `shared_bank_present: false`. The session
MUST review the actual local bank rather than manufacture a shared starter.
A removed previously occupied bank MUST require historical inspection, not be
mistaken for first initialization. Another clone's different project ID MUST
require explicit identity reconciliation; publication MUST NOT silently split
one repository into two banks or merge unrelated identities.

The shared snapshot/source bounds still apply. Policy and check collections
allow at most four pages of 100 entries; exceeding them requires inspection.
Authority location discovery allows 16 MiB of tree metadata, one million
entries, 64 possible routers and 128 locator-changing mainline commits. This
history bound concerns possible router changes, not the repository's total age.
Incomplete discovery MUST NOT establish absence.
No deterministic helper promises perfect semantic truth or a global transaction
across local edits, GitHub policy and Git transport. The session MUST preserve
source-specific applicability, use the fresh checks above and surface failures.

## Observe

A session MUST observe actual local memory, fresh authority and current repository policy before constructing a publication. It MUST inventory any previous candidate/request and actual landing evidence before deciding that another push or PR is needed.

## Plan

```sh
   python3 -I "$SKILL_DIR/scripts/publish_memory.py" prepare --v2 --repo /path/to/repo --input /private/semantic-proof.json --output /private/prepared-publication.json
   ```

4. The session MUST inspect `sharing_state`. Only `prepared` contains a `request`;
   it MUST save that exact object in private storage, without editing its fields.
   `no-publishable-delta` MAY still contain unresolved conflicts; the session
   MUST surface those and continue independent work. `pending-policy` means a
   rule needs an explicitly supported landing method, not permission to bypass it.

The helper rechecks complete sources, Git/scope pins and current repository
policy, and builds a commit from the actual authority tree with its own private
index. The sole parent is that observed authority commit; the feature HEAD,
working bank and original index are not changed. Only exact reviewed canonical
Markdown blobs change in the proposed tree. Runtime state, credentials, raw
transcripts, excluded artifacts, feature ancestry and unrelated staged paths
MUST NOT enter it. Temporary indexes remain in private state outside the repo.

A legitimately removed ordinary memory file can remain in the feature index.
The boundary inspector proves its old canonical ownership from the last complete
HEAD bank and matching ordinary index blob. It does not stage that deletion or
accept an arbitrary missing tracked artifact. Unknown ownership, malformed prior
banks and nonordinary/unmerged entries MUST remain for inspection.

## Apply

```sh
python3 -I "$SKILL_DIR/scripts/publish_memory.py" land --v2 --repo /path/to/repo --input /private/publication-request.json --output /private/landed-publication.json
```

## Read back

Success MUST be reported only from actual authority readback proving the whole
intended delta. A successful command exit alone is not publication. Publication
MUST NOT refresh/rebase the feature branch or imply its code is current. The
session MUST follow the separate [refresh operation](../refresh/GUIDE.md) when
importing landed memory. If that import is not verified, it MUST report refresh
as pending and MUST NOT substitute a whole-branch pull or snapshot overwrite.

## Verify

The session MUST inventory a previous attempt before retrying. The helper first
checks actual first-parent authority trees for the complete proposed memory
delta, including deletions. `already-landed` MUST NOT cause another push. A
later correction MAY make `intended_delta_current: false` while the historical
publication remains proved; the session MUST preserve that correction.

For a new landing, the helper repeats deduplication/semantic proof validation
against freshly observed authority and current sources. A changed source/head
requires new comparison and review; the session MUST NOT edit an old proof or
retry its old candidate blindly. It MUST follow current permissions, classic
branch protection and all pages of active inherited rules within the bounded
reader. Unknown, unreadable or exceeded inventories MUST remain pending.

For an unprotected supported branch, the helper uses a normal fast-forward Git
push of the prepared commit. For supported protected workflows it creates or
reuses a memory-only PR, leaves required human reviews/checks pending, and never
approves it. Once GitHub reports the exact PR head/base eligible, the helper
checks the actual required check names, latest results, recency and configured
App identities. Where a name has both a status and a check, both MUST pass.
Checks on the generated test merge take precedence when present. A clean rollup
alone MUST NOT substitute for missing required results.

The protected landing uses GitHub's actual generated merge object only when its
parents are exactly the reviewed authority base and memory-only PR head and its
tree equals that proposed memory tree. It repeats current eligibility/checks
and source pins before a regular fast-forward push. A racing authority advance
is rejected and MUST trigger fresh reconciliation. The session MUST NOT use
`--force`, an admin override, automatic approval or an unpinned future auto-merge.
Normal pre-push hooks remain active; follow-tags and submodule pushes are disabled.
Only the explicit memory publication ref is updated.

Merge queues, required deployments/signatures/linear history, restricted push
actors, disallowed merge methods and unknown rule types currently require a
supported explicit continuation. The session MUST retain their pending state
rather than claim they passed or attempt a privileged bypass. It MAY use the
ordinary repository workflow only after fresh source review and actual required
approvals/checks; that separate action MUST supply its own landing readback.

## Outcomes

- `landed` or `already-landed`, with `publication_verified: true`: the complete
  delta has a real authority-tree witness. The session MUST distinguish historical
  landing from the separate `intended_delta_current` fact.
- `pending-repository-rules`: the session MUST surface the PR and missing review,
  checks or eligibility, preserve local work and wait through the normal process.
- `pending-reconciliation`: the session MUST inspect current authority, search and
  read matches again, resolve/consolidate meaning, and prepare a new exact request.
- `pending-policy` or exit 1/`pending-inspection`: the session MUST retain the
  request/local learning and inspect the failure. Unknown Git metadata writes
  MUST NOT be reported as no writes. The original bank/index are never recovery
  scratch space.

## Failure handling

A lost push/PR-create response MUST be inventoried, not blindly repeated. The
same candidate branch and exact PR head/base query recover an existing PR;
actual Git tree readback recovers publication even after disposable state loss.
The session MUST NOT copy private requests/proofs into canonical memory or Git.
Prepared commits and transport observations are mechanical evidence, not an ACL.
The repository and OS continue to control access. Local Git signing/credential
configuration remains the operator's; the helper does not switch accounts.

## Diagnostic fallback

The session SHOULD use the public copied-pack publication, policy, pending,
linked-worktree and recovery suites for changes to this operation. Their GitHub
adapter is an offline protocol fixture; real local Git remotes and server hooks
supply the write/readback and race evidence. Tests MUST NOT require real
credentials or alter live client/repository policy.

If normal publication cannot establish verified authority readback, the session MUST keep the request, local memory and feature state intact. It MAY inspect bounded actual policy, first-parent trees and operation receipts, but MUST NOT force a push, bypass review/checks or infer success from candidate reachability alone.
