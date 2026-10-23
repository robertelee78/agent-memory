"""Render the shared semantic reference from its actual policy definitions.

Only the guide command imports this renderer. Narrative annotations retain the
existing source/authority/applicability rules; enums and authority rows come
from their public executable definitions rather than a second state model.
"""
from pathlib import Path
import json

PACK = Path(__file__).resolve().parents[2]

AUTHORITY_GUIDANCE = r"""A session MUST identify what kind of fact it needs before deciding which source
is stronger. It MUST read the current applicable source, including its evidence,
revision and approval/qualification where relevant. Memory MUST point to that
source rather than replace it or grant new permission. The current operator
request and applicable instruction hierarchy still govern authorized work.

## Authority matrix

The shared definition is `schemas/authority-policy.json`.

{authority_matrix}

This is a domain-specific matrix, not one global ranking. A session MUST NOT
claim that current code rewrites approved intent, a planned issue proves
completion, passing tests prove a release was published, or an old observation
proves current behavior. AWA is the work/delivery record and Vox is the live
coordination authority when those systems are used. Memory MUST NOT prove agent
ownership or delivery. A procedure recalled from memory remains a candidate
until its review, exact version, applicability and approval have been verified.

A remembered statement about operator approval MUST NOT serve as that approval.
A direct user decision about this repository is shared project evidence, not a
personal profile or preference store. Statements from another repository MUST
NOT be imported through this policy.

## Resolve or clarify

A session MUST check existing memory and relevant current sources before storing
a new claim or correction. It MUST retain distinct applicability, source links
and provenance rather than silently treating a newly landed statement as universal.

- If memory says a retry limit is five and current code/tests demonstrate three,
  the session MUST describe three as observed behavior and SHOULD prepare a
  correction candidate with those exact references. If an approved ADR calls for
  five, it MUST preserve that intent and expose the implementation gap; code did
  not revoke the ADR. Expected unfinished implementation is not by itself a
  contradiction requiring a new product decision.
- If memory says a feature shipped while AWA only plans it, the session MUST use
  the actual work/integration/release evidence. It MUST NOT promote an issue's
  plan or a green test result into proof of publication.
- If memory disagrees with the active coordination system, the session MUST
  inspect that system for current ownership instead of claiming ownership from
  recalled text.
- If applicable current sources conflict, or two lessons say the wall should be
  yellow and blue with no verified decision resolving them, the session MUST
  surface the discrepancy to the detecting session's developer and ask a focused
  project question. Landing order alone MUST NOT decide truth. When another
  developer supplied the conflicting knowledge, it SHOULD prompt the detecting
  developer to contact that developer as needed; it MUST NOT send a message on
  their behalf without authorization.

A session MAY automatically tighten or correct a clearly outdated claim within
existing operation/repository rules. It MUST surface consequential corrections,
failures and unresolved conflicts promptly. It MUST NOT add a blanket approval
step to already authorized routine maintenance. It SHOULD prepare revocation
instead when stronger evidence invalidates the old guidance without a reliable
replacement. It MUST preserve immutable historical events as history rather
than rewrite what happened because today's behavior differs."""

SCOPE_GUIDANCE = r"""One canonical project UUID identifies the repository's shared memory bank.
Repository scope is the default; no developer has a separate memory namespace.
A session MUST resolve the intended project before recall or mutation. It MUST
NOT use a repository basename, harness, email or developer alias as identity.
Local identity evidence supports that resolution; it does not authenticate a
person or grant access beyond repository/OS permissions.

A session MUST follow [identity resolution and binding](../identity/GUIDE.md)
for concrete status, developer setup, bank/registry previews and write guards.

## Scope matrix

| Scope or condition | Applies when | Unknown or different context |
| --- | --- | --- |
| repository (default) | Resolved project UUID matches the bank | Exclude another project |
| component:path | Current repository-relative path is exactly that path or below it | Exclude; sibling prefixes do not match |
| branch:value | Explicit branch condition matches | Exclude |
| worktree:value | Explicit worktree identity condition matches | Exclude |
| task:value | Explicit task identity condition matches | Exclude |
| session:value | Explicit session identity condition matches | Exclude |
| revision/version/environment/prerequisite condition | Explicit observed value matches exactly | Exclude; no inferred ranges or prerequisite satisfaction |
| developer identity | Attribution and who can clarify a lesson | Never a selection boundary or personal ACL |
| related project | Future explicit selection and consent | Unavailable in v0.0.1 |

Conditions refine shared knowledge rather than create independent banks. Several
conditions MUST all be known and match; a branch condition does not erase a
component condition. A condition value MUST NOT be inferred from the lesson's
prose or the detecting developer's identity. Unknown applicability MUST remain
unknown and excluded from current context; a session MAY inspect it separately
as explicitly non-current evidence while resolving the uncertainty.

Component paths MUST be canonical relative paths using `/`, without traversal,
empty components, drive letters or aliases such as `src/./api`. Branch, task,
worktree and session values are explicit identifiers, not fuzzy display names.
A session MUST NOT widen branch/task/session observations to repository-wide
truth merely because they landed later on `main`. A correction or consolidation
MUST preserve distinct conditions and evidence.

Cross-repository federation is a later proposal. A session MUST NOT search,
copy or publish memory into another repository through this release, including
between public and private repositories. A future consent mechanism cannot be
simulated by adding a flag or treating shared developer identity as consent.
Personal profiles, preference stores and private per-developer overlays MUST
NOT be created. A direct user decision about this project MAY be retained with
its source and applicability as shared project knowledge."""


def render(lifecycle_text):
    from .security import CLASSES
    authority = json.loads((PACK/'schemas/authority-policy.json').read_text())
    scope = json.loads((PACK/'schemas/scope-policy.json').read_text())
    record = json.loads((PACK/'schemas/record.json').read_text())
    matrix = '| Fact | Authority |\n| --- | --- |\n' + '\n'.join(
        '| '+row['meaning']+' | '+row['authority']+' |' for row in authority['domains'])
    authority_text = AUTHORITY_GUIDANCE.replace('{authority_matrix}', matrix)
    preface, states = lifecycle_text.split('\n## Independent axes\n', 1)
    preface = preface.replace('# Memory lifecycle model', '# Shared memory model', 1)
    kinds = ', '.join('`'+value+'`' for value in record['properties']['kind']['enum'])
    classes = ', '.join('`'+value+'`' for value in sorted(CLASSES))
    conditions = ', '.join('`'+value+'`' for value in scope['condition-kinds'])
    overview = (
        'This conditional reference also derives its kinds, authority rows and scope\n'
        'vocabulary from `schemas/record.json`, `schemas/authority-policy.json`,\n'
        '`schemas/scope-policy.json` and the security policy source classes.\n\n'
        '## Memory kinds and source classes\n\n'
        'Standalone memory kinds: '+kinds+'.\n\n'
        'Governed source classes: '+classes+'.\n\n'
        'The provenance source describes where a claim came from; the governed source\n'
        'class describes the actual invoking source. Neither label authenticates an\n'
        'actor, proves an assertion or substitutes for independent trust, evidence and\n'
        'procedure approval. Context prose need not become a standalone record.\n\n'
        '## Source authority and contradictions\n\n'+authority_text+'\n\n'
        '## Repository scope and applicability\n\n'+SCOPE_GUIDANCE+'\n\n'
        'The executable scope condition vocabulary is '+conditions+'. Its combination\n'
        'rule is `'+scope['condition-combination']+'`; developer attribution remains\n'
        '`'+scope['developer-role']+'`.\n')
    return '---\nkind: model\n---\n\n'+preface+'\n'+overview+'\n## Independent axes\n'+states
