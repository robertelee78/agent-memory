# Complete native lookup: repeated locator path construction

The operator clarified that AI thinking time is excluded from the recall target
and explicitly permits v0.0.3 harness semantic recall with standalone terminal
semantics retained for later delivery. Execution now counts the complete native
lookup toward five seconds. The earlier 4.44-second final read is not a complete
lookup pass: start, two navigation nodes and qualification took about 7.77 seconds.
The original report's unsupported narrower approval wording has been corrected;
the independent changes-required verdict remains in issue #98 history.

A source-frozen cProfile run on `6a983d05a94ef9431a7ab24ad7f55ef469a0d660`
identifies repeated repository-locator discovery as avoidable overhead. At 50k,
start calls settings seven times, each node six times, and qualification fourteen
times. `git_roots` repeatedly constructs `PurePosixPath` objects for all ordinary
Git-listed source/record names even though only `MEMORY.md` can be a locator.
In the instrumented run, settings accounts for 1.44/2.42 seconds in start,
1.24/1.44 in a node, and 2.93/8.25 in qualification. Profiling adds overhead;
these are diagnostic costs, not new latency acceptance observations.

The repair still obtains the complete working/index/HEAD filename inventories.
It first uses a cheap spelling check to identify possible router names, including
names ending in a dot component that need normalization, and then applies the
same path, hidden-component, count, symlink, tracked-object, schema and competing-
bank checks. Ordinary names avoid allocating path objects. No inventory source,
locator version or currentness check is removed. There is no new cache, persisted
source authority, changed bank format or less frequent final hash readback.

Raw profiling commands, source-before/after manifests and binary profiles remain
at `/tmp/agent-memory-v003/profile-full-navigation-6a983d0/`; the
[diagnostic export](data/2026-10-25-native-lookup-profile.json) binds their hashes.
New exact-source native controls, isolated complete-lookup measurements and
independent review remain required before any five-second or whole-story pass.

## Complete-path follow-up

The first spelling-only optimization at `244f6d33e68ce00e216ed81d4283d41cba894d24`
passed 59 registered methods on each platform and bounded independent static
review. Its new 50k warm complete lookups measured 6.626/6.840 seconds, with
3.937/4.037-second final reads. It improves the path but still misses five seconds.
Those observations remain separately retained at
`/tmp/agent-memory-v003/locator-filter-244f6d3-cost-50000/`.

The next repair asks Git for the fixed case-insensitive router basename at every
depth in the working/index listing, with NUL-delimited status tags. The documented
[Git status-tag format](https://git-scm.com/docs/git-ls-files#Documentation/git-ls-files.txt--t)
preserves filename whitespace and distinguishes indexed from untracked paths in
one listing. The fixed
[glob pathspec](https://git-scm.com/docs/gitglossary#def_pathspec) includes that
basename in any directory. Unknown/failed output refuses; HEAD's complete tree
inventory and exact working/index/HEAD router versions are still examined. The
common Git adapter defaults to literal paths; only this fixed internal locator
query opts into glob semantics. It does not accept a caller-supplied pattern.

Discovery also derives the bank-relative name from its already resolved actual
`root, bank` pair instead of invoking a third settings discovery just to stringify
that pair. Both original scope/context checks and navigation/source rechecks
remain, including the final fresh native qualifier. This is operation-local path
reuse, not a cached identity/authority decision. A public installed-command
control covers tag-like, tab/newline, Unicode/space filenames and a second mixed-
case locator; existing indexed/HEAD deletion and custom-worktree controls remain.

The tagged-query follow-up at `adf5852b98c2a297e767f556bae4718c1d7c41f9`
measures 5.356/5.373 seconds for the complete 50k native path. It is closer but
still above the target; its source-qualified results are retained separately.
The next narrow changes combine the two local setting reads into one coherent
NUL-delimited Git config read (retaining exact duplicate/empty-value refusal),
and compute each walked directory's bank-relative prefix once instead of once
per file. `os.walk`, every actual file's lstat/open/no-follow/type/hash/budget
checks and the final source/physical inventory comparison remain unchanged.
There is no cross-operation cache or removal of configuration rechecks.
