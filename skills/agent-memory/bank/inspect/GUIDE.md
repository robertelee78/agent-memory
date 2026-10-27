---
kind: operation
domain: bank
---

# Inspect current canonical bank files

## Use when

A session SHOULD use this operation to inspect actual context files, omissions, routes and local Git evidence. For task-relevant current recall, select the recall domain instead; structural inspection does not admit or assemble learning.

## Required context

A session using this leaf MUST first load and follow the [bank domain](../GUIDE.md). It MUST use the [shared model](../../model/GUIDE.md) only when an unresolved semantic question requires it.

A session SHOULD use [baseline recall](../../retrieval/GUIDE.md) when selecting
applicable current lessons rather than only inspecting structural bank state.

## Inputs and preconditions

The input is the actual repository path with its already resolved canonical bank. A session MUST preserve an existing bank and unresolved identity rather than create a starter to make inspection succeed.

## Observe

A session MUST read the current bank location and pending lock/transaction state. A session MUST preserve unknown markers and use the domain recovery route before treating an interrupted bank as usable.

## Plan

This operation has no mutation plan: it inspects existing files. A session MUST choose a separate observed-write or governed operation if actual repair is required.

## Apply

With `SKILL_DIR` resolved to the packaged skill directory:

```sh
python3 -I "$SKILL_DIR/scripts/bank_state.py" inspect --repo /path/to/project
```

For deliberate inspection of one existing ordinary context file:

```sh
python3 -I "$SKILL_DIR/scripts/bank_state.py" inspect --repo /path/to/project --path activeContext.md
```

This selector MUST require the same-project identity and current safety/source
checks. It accepts ordinary context paths only, never typed records or arbitrary
paths. The bounded body and hash describe `historical-orientation`, with
`assignment: false` and `usable_as_current_guidance: false`. A session MUST retain
conditions, developer/session provenance and evidence availability qualifications;
inspection cannot assign the remembered task or bypass record invalidation.
Unsafe content MUST remain withheld and interruption markers MUST stop the read.

## Read back

A session MUST read the actual returned contexts, omissions, routes and source hashes before describing the result.

The command validates the bank, reports each recognizable context or its omission
reason, routes and current source hashes, and optionally inspects local Git
context. It writes nothing and reads no memory-generated application state.
It does not assemble task context, admit a claim or prove semantic currentness.
A session MUST still apply [scope](../../scope/GUIDE.md), lifecycle, source-authority
and trust checks before using a record as guidance.

## Verify

Git facts are deliberately limited: `context_head` is the local repository HEAD,
and `index_entries` show current file paths in the index. Index membership does
not prove that the current bytes were committed, pushed, merged or published.
The inspector therefore reports `current_bytes_committed: not-assessed` and
`local-or-pending-sharing`, with publication and freshness unverified. This
means verification is pending, not that the helper proved a published version
absent. A session MUST use actual Git/publication/refresh evidence before
claiming another session can see the learning. Local Git inspection uses fixed
read-only plumbing, disables lazy fetch/network protocols and optional locks,
and does not execute remembered commands or refresh the worktree.
These switches follow the [Git command reference](https://git-scm.com/docs/git/2.53.0.html).

## Outcomes

The returned local inspection is evidence about actual files. A session MUST route to current recall before using selected learning, and to the sharing domain before making a cross-session publication/freshness claim.

## Failure handling

A plain untracked bank or unavailable Git metadata remains readable Markdown.
Missing Git, a failed index read or a different enclosing Git root MUST NOT be
presented as a verified audit trail or silently bind the bank to another project.
The session MUST retain local work and resolve identity/publication through the
appropriate operation. Ordinary source links remain usable independently of Git.

## Diagnostic fallback

If the helper cannot establish a verified result, a session MAY use the [file-only workflow](../../offline/GUIDE.md) to inspect bounded current Markdown with ordinary tools. It MUST retain the domain’s interruption and source-authority qualifications; raw reading does not repair locks or grant mediated guarantees.
