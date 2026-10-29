# v0.0.11 bytecode incident: producer RCA and gated recovery proposal

Scope: ADR-0021 T5 / issue #238. This delivers the incident RCA, reproducible
controls and a proposal for later recovery implementation. **Automatic cache
healing is not implemented or approved by this document.** Current integrity
checks remain strict; no cache exclusion, receipt rewrite or source-mode change
is proposed as a substitute for verification.

## Finding

The Linux producer was a Codex-authored diagnostic command that imported Agent
Memory's installed internal Python package directly. It did not use `am` or an
Agent Memory helper entrypoint. The transcript records plain `python3 -`, with
neither `-I` nor `-B`, followed by this redacted prefix:

```python
import json, sys, traceback
sys.path.insert(0, '<installed-agent-memory>/skills/agent-memory/scripts')
from agent_memory import semantic_sharing
# Later in this same diagnostic: semantic_sharing.compact_review(...)
```

There was no bytecode guard in the command. This bypassed the prevention already
present in `am_console.py` and `reconcile_memory.py`, which set
`sys.dont_write_bytecode = True` before importing the package. Normal Python
imports then created derived caches inside the signed source tree. The next
update correctly refused the changed installed inventory, but its generic
identity diagnosis did not identify verified cache-only drift or offer a
supported automatic recovery route.

This is an externally authored diagnostic invocation, not evidence that the AM
launcher or turn hook created the caches. Direct internal imports are not a
supported public CLI operation, but they are a plausible agent troubleshooting
workflow; the resulting installation footgun merits explicit guidance and a
carefully reviewed recovery design.

## Actual episode and evidence

| Event | UTC on 2026-10-28 |
| --- | --- |
| Signed v0.0.9 installation receipt | 15:12:50.995271 |
| Codex diagnostic tool invocation | 18:09:44.611 |
| Earliest generated cache mtime | 18:09:44.654728 |
| Latest generated cache mtime | 18:09:56.856725 |

The original source event is retained in the private evidence referenced below;
its session identifiers, source log path and unrelated repository inputs are not
published. Session files are organized by session-start date, so a day-directory
search can miss a later event in a resumed session. The event's SHA-256 is
`9117b6bff2da72e59933b6cb284c13508ba3e64a3660a7632ab8d4c86842e3ba`;
the extracted original command's SHA-256 is
`72c1b89057c59db1f687fc104fe33d57bda3a83acd2741aef1637e73894ad90f`.
Raw transcript records and the unredacted command remain private. The public
producer receipt contains only AM-relevant redacted commands and metadata.

The trace establishes an actual cache-producing invocation, not merely a guess
from mtimes. Its first import precedes the first cache by about 44 ms; the full
controlled replay creates the same 35 module-cache names. No kernel process-ID
or system-wide audit attribution is invented. The earlier v0.0.10 incident report
correctly recorded the producer as unknown at that time; this later transcript
inspection supplies the missing attribution.

Prior forensic verification established that all 189 signed v0.0.9 payload
files and modes were unchanged, including the manifest and pinned publisher
signature. The only source additions were 35 owned regular CPython 3.14 caches.
Source revision was `07956881057fe29bb3160deaecc23b3e7abc9f93`; artifact digest
was `992510030a2e163e95d567954c95d6236d2c3ee2e9c22d6e60c3dacfbddbe78b`.
The cache headers, source filenames and recursively compared code attributes
matched recompilation of that signed source under the matching interpreter.
Complete marshal-body consumption and preserved quarantined byte hashes were
checked. **Raw marshal reserialization was not byte-identical for every file**;
code-attribute equivalence must not be described as byte-for-byte equality.

The separately authorized manual recovery preserved all cache bytes in reversible
quarantine and then completed normal signed v0.0.10 adoption. That is historical
forensic evidence, not approval of that manual parser as a production algorithm.
No live profile was changed during this RCA or its laboratory reproductions.

## Public-operation reproduction and controls

One frozen controller was run against a verified copy of the actual signed
v0.0.9 release on both platforms. Each installation and consumer repository was
inside a disposable `World`; originals were read only. The controller uses the
same verified bootstrap handoff as the installer, creates a real public inspection
of a tiny fixture and then reproduces the external direct-import diagnostic.
It does not copy the other repository's private learning, inputs or judgments.

| Platform | Interpreter | Guarded CLI/helper | Direct diagnostic `-I -B` | Plain direct diagnostic | Subsequent public operations |
| --- | --- | --- | --- | --- | --- |
| Linux x86_64 | CPython 3.14.4 | 0 caches | 0 caches | 35 caches | `am update` refuses identity; `am doctor` reports failure |
| macOS arm64 | CPython 3.15.0 | 0 caches | 0 caches | 35 caches | `am update` refuses identity; `am doctor` reports failure |

The environment bytecode guard was deliberately removed for all three controls,
so normal launcher/helper prevention is demonstrated independently of it. The
same direct diagnostic differs only in its interpreter flags for the `-I -B`
control. Both complete runs preserved installation receipt bytes and consumer
bank/index/HEAD, and removed their disposable worlds afterwards. Every command
had a 180-second bound. Completed command times summed to 7.800 seconds on Linux
and 20.731 seconds on macOS, with no timeout counted as success.

The Linux active signed installation had exactly the authenticated release
payload; no AppleDouble files were installed. Its auxiliary fixture-helper tree
came from the earlier isolated test source. macOS used a complete `git archive`
source tree including root wrappers, without AppleDouble metadata. This qualifies
these functional controls, not a new release package or recovery implementation.

An initial early-validation diagnostic created 31 caches before reaching deeper
review imports; it proved the mechanism but was not the full observed closure.
The complete manifest replay produced all 35, matching the incident's module
names. Two initial controller setup errors (temporary script-location default
and an empty fixture authority) were corrected before the complete probe; their
private logs and hashes are retained separately rather than counted as product
failures or passing controls.

The controller and redacted receipts are in
`data/2026-10-28-v011-bytecode-rca/`. Raw private evidence is retained at
`/tmp/am-v011-cache-producer-28873qt6` on Linux and the locally collected
`/tmp/am-v011-cache-producer-private`. The public binding receipt records original
hashes and scope without publishing unrelated transcript contents.

## Immediate diagnostic guidance

Prefer public `am` operations and the documented helper launchers. If diagnosing
**known-trusted source** by direct Python import is necessary, use an isolated
interpreter with bytecode generation disabled before the import, such as
`python3 -I -B`. Do not rely only on `PYTHONDONTWRITEBYTECODE` with `-I`, because
isolated mode ignores Python environment variables. These flags prevent this
cache-generation mistake; they do not authenticate an existing installation or
make its cached code safe to import. [Python command-line documentation](https://docs.python.org/3.14/using/cmdline.html)

In particular, `-B` suppresses cache writes, not cache reads. A suspect installation
must be inspected from an independently trusted complete helper, without importing
its package. A possible future startup isolation using a fresh cache-prefix tree
would also need review; it is not selected here. Python documents that
`sys.pycache_prefix` redirects cache reads and writes and ignores source-tree
`__pycache__` directories. [Python cache-prefix documentation](https://docs.python.org/3.14/library/sys.html#sys.pycache_prefix)

## Gated proposal for a future recovery operation

This proposal is a review input. It does not authorize or ship automatic healing.
Any future `doctor --fix` or mutating updater recovery must satisfy all of these
conditions before it can be accepted:

1. **Authenticate the original payload first.** An independently trusted verifier
   validates pinned publisher signature, manifest schema, source/channel identity,
   every declared payload hash and exact mode, and the corresponding receipt.
   It inventories every extra entry. This is diagnostic classification of a
   failing installed tree, not an integrity exclusion that makes it pass.
2. **Restrict the candidate class.** Only ordinary, single-link, appropriately
   owned cache files in the expected cache directory can be candidates. Any
   changed payload, mode, signature, unexpected file/type, link, foreign ownership,
   unknown version or inconsistent receipt remains refused and preserved with an
   exact actionable reason. It must not delete files or chmod parents to proceed.
3. **Keep parser trust unresolved until independently proven.** Ownership, a valid
   cache header and matching source timestamp do not authenticate unsigned cache
   bytes. No in-process `marshal.load/loads` of unproved caches is permitted in a
   live/privileged verifier. Non-execution and timeouts alone are not a sandbox.
   Review either conservative bytewise comparison against regenerated caches
   (which may refuse legitimate alternate encodings), or an independently reviewed
   non-executing parser isolated from operator files, credentials and networking.
   Resource limits alone cannot establish that isolation. Unsupported isolation
   or interpreter versions must refuse safely rather than fall back.
4. **Bound the selected verification mechanism.** Before implementation approval,
   define and test finite file-count, per-file/aggregate-byte, time, memory,
   nesting and object-count limits. Require the supported native code format,
   header flags, exact applicable interpreter, filename/source binding and full
   semantic code-attribute comparison if that strategy is selected. Consume the
   complete body and reject trailing bytes, malformed input and mismatches. Never
   execute cached code, import the suspect package or reconstruct a callable.
5. **Serialize and pin the repair.** Respect the existing lifecycle flock and any
   pending activation/recovery ownership. Retain no-follow directory descriptors
   from trusted ancestors to leaf; pin owner/mode/inode and named bindings, then
   recheck them and exact candidate hashes immediately before mutation. A changing
   source, cache, receipt, journal or identity is pending work, not permission to
   overwrite it.
6. **Quarantine reversibly.** Move only proven candidate bytes using the existing
   supported native no-replace primitive on the same filesystem, into a private
   quarantine outside the installed payload. Preserve exact bytes and operation
   evidence; never discard them or overwrite unknown destinations. A failed rename,
   unsupported filesystem or interrupted repair must retain originals/receipts
   and report the precise pending state. Recovery/idempotence for a crash between
   rename and receipt publication must be designed and tested before shipping.
7. **Reverify normally.** Preserve the installer receipt, source declaration,
   existing journal and feature/memory work. After quarantine, the unmodified full
   integrity verifier must pass before update may resume or doctor may claim a
   repair succeeded. A read-only check must not silently mutate state.
8. **Qualify the refusal controls too.** Exercise benign derived caches plus
   malformed headers/bodies, trailing bytes, wrong interpreter, changed source
   and modes, symlinks/hardlinks, foreign ownership, live locks, rebindings/races,
   unknown pending state, interrupted quarantine and failed native rename on both
   supported platforms. Prove exact bank/index/HEAD and evidence preservation.

The parser decision is a real gate, not an implementation detail to waive.
Python explicitly warns that marshal is unsafe for malformed or malicious input,
that code objects are version-sensitive, and that `loads` ignores extra bytes.
A matching-version forensic comparison is therefore not sufficient evidence for
an unrestricted product parser. [Python marshal documentation](https://docs.python.org/3.14/library/marshal.html)
