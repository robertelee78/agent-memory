# Memory-only publication

The public portable helper is `skills/agent-memory/scripts/publish_memory.py`;
`scripts/publish_memory.py` is the checkout entrypoint. The
[operation guide](../../skills/agent-memory/sharing/publication/GUIDE.md) defines
agent responsibilities and pending/recovery behavior.

`prepare --repo REPO --input PROOF` validates the complete source-bound semantic
proof against fresh authority, evaluates current GitHub rules and builds an
immutable memory-only commit whose sole parent is the observed authority head.
It uses a private index outside the repository and exact reviewed blobs. A
`prepared` result contains the exact private `request` consumed by
`land --repo REPO --input REQUEST`. The request schema is
`agent-memory/publication-request-v1`: `repo_root`, `project_id`, `base`,
`candidate`, `proof` and `schema`, with no editable transport or policy override.

The land operation inventories previous success first, repeats fresh semantic
and policy checks, and uses ordinary fast-forward Git transport. Supported
protected workflows use an actual memory-only PR and GitHub-generated merge
object, bound to the exact base/head/tree, required reviews and actual required
checks. It does not invoke an admin merge, auto-approve, force push, rewrite
feature history, stage the original index, apply a refresh, or select truth by
last writer. Unsupported/unreadable policy remains explicit pending work.

Readback verifies actual first-parent authority trees containing the full
intended memory delta, including removals. Historical success is distinct from
whether a later correction still leaves those bytes current. Lost responses
and disposable-state loss do not require a duplicate push.

An actually never-occupied authority bank prefix supports first publication
from a bound local bank, with explicit empty base/landed sources. A prefix with
removed historical content requires inspection. Unstaged canonical deletions
are proved through the complete prior HEAD bank and exact indexed blob; unknown
tracked artifacts are still excluded, without writing the original index.

Fresh authority discovery also checks other canonical bank locations and
mainline locator history. A custom bank that was later removed is still prior
ownership; an unlanded side proposal discarded by a merge is not. Tree metadata
is bounded to 16 MiB/one million entries, 64 possible routers and 128 mainline
locator changes, independently of total repository age. Incomplete inventories
cannot establish first initialization.

Policy/check collections are complete within four pages of 100 records. The
operation remains pending on unknown/unsupported rules, missing exact required
results, missing App attribution, wrong source SHA, stale evidence or a source/
authority race. Regular pre-push hooks run; unrelated tags and submodule pushes
are disabled. GitHub `gh` authentication and normal Git credentials are observed
without changing the selected account.

The isolation fixtures exercise real local Git publication and server rejection,
with an offline GitHub protocol adapter. Primary interface references:
[active branch rules](https://docs.github.com/en/rest/repos/rules),
[protected branch requirements](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches),
[exact required check behavior](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks).
