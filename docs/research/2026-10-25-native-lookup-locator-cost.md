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
