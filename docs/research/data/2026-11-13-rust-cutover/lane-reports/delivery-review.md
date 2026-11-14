# Release delivery independent review

## Source, scope and method

Reviewed immutable source: `2457005f8447d32bd1f4958654001f323c29ea34`. Reviewer: `ReviewReleaseDelivery`, `anthropic/claude-opus-5-5:high`; report integrated by `omp-beast`.

Read in full: `src/release_distribution.rs` (1503 lines), `src/cache_recovery.rs` (597), `src/bin/am_dev/release.rs` (855), `skills/agent-memory/schemas/release-v2.json`, `tests/t2_delivery.rs` (1229); baseline `release_distribution.py` (408), `cache_recovery.py` (226), `scripts/distribution/release.py` (280); ADR-0029, the cutover design's release/bootstrap/binary-binding/artifact-digest sections, lane brief and delivery report. Supporting descriptor/trust/process/artifact/codec/CLI surface/bootstrap and baseline output-encoding call sites were inspected only as needed.

No Python, builds, tests, runtime proof, lint or formatter ran. Findings below are static evidence, not observed release operations. No live GitHub publication was attempted. The explicitly known pending cache-recovery per-target receipt digest/install-receipt-v3 change is excluded.

## Findings

### F1 — P1 — Publication readback compares insertion-order and sorted-order command surfaces

**Rust evidence:** `src/bin/am_dev/release.rs:340-348` builds the manifest with the command-surface provider's insertion order; `:350` encodes sorted signed bytes, `:354` verifies/re-parses those bytes into `verified`, but `:367` returns the original `manifest`. Publication later requires `release::acquire(...) == manifest` at `:799-801`, after publishing the draft as latest (`:770-781`). JSON `Value::Object(Vec<...>)` equality is order-sensitive (`src/codec/json.rs:58-67`).

**Baseline:** `scripts/distribution/release.py:230` compares Python dictionaries with order-independent equality. The signed encoder's sorting did not make its source object unequal there.

**Concrete sequence/input:** the real surface from `src/cli/surface.rs:167-184` starts `schema, program, aliases, root, commands, helpers`, unlike its sorted form `aliases, commands, helpers, program, root, schema`. Nested native entries also differ. [INFERENCE from these exact constructions] a real publish creates/uploads/publishes the release, then refuses `Public release differs from signed publication`; a resumed run reaches the same mismatch. `main` promotion and `verify_default` are never reached. This is a deterministic non-idempotent publication failure, not a signature bypass.

**Minimal fix:** return the verified/re-parsed manifest (`Ok(verified)`), or compare canonical encoded manifests rather than raw ordered JSON values. Do not globally weaken JSON equality, because object insertion order is a separate byte contract elsewhere.

**Missed regression:** delivery fixtures use a one-key surface (`tests/t2_delivery.rs:118-121`), so the acquire equality at `:487-490` cannot distinguish sorting. No publisher test exists. Cover nested unsorted keys and the builder-returned/publicly-read manifest through a deterministic isolated publication simulator.

Confidence: high static evidence; parent confirmed the builder return, equality check and actual provider key order. No publication executed.

### F2 — P2 — A 64 KiB capture/parser limit is applied to full GitHub API payloads

**Rust:** all `gh` commands are capped by `release::MAX_METADATA` (65,536 bytes) at `src/bin/am_dev/release.rs:87-103`; `api:126-128` parses with the same bound. `compare_status:465-475` requests the full comparison response, and release-record/latest lookups request full release objects. The cap is a process-output refusal, so later field selection never runs.

**Baseline:** `scripts/distribution/release.py:22-28,54,189,224,236,246` accepts full compare/release JSON. A new finite bound is allowed by the Rust design, but it must cover the intended operation rather than refusing otherwise accepted releases.

**Concrete failing inputs, not executed:** a successful `gh api` JSON response exceeding 65,536 bytes, even when only `.status` is needed, necessarily causes refusal. Compare payloads can contain up to 250 commit objects plus file patches; a large Rust cutover is a plausible witness, but its actual response size was not measured. A release response containing 70,000 ASCII note bytes necessarily exceeds the cap, whereas this builder accepts note files up to 2 MiB (`:600`). [INFERENCE] the latter can leave a created draft whose subsequent readback/resume always refuses. The reviewer estimated smaller real-response thresholds; those estimates are not treated as measured evidence.

**Minimal fix:** project required fields in `gh` before capture: compare with `--jq .status`, and release reads with a bounded field selection for tag/draft/prerelease/target/html URL/assets name+size. Preserve explicit output bounds and deadlines; do not simply remove limits.

**Missed regression:** no publisher/gh simulator tests exercise realistic large compare/release responses, although downstream download tests exist. Add a credential-free deterministic gh fixture whose unprojected payload exceeds the metadata cap and whose projected data remains bounded.

### F3 — P3 — Download failure details reverse a baseline field order

**Rust:** `src/release_distribution.rs:1109-1113` constructs `schema, stage, host, attempt_count, attempts, ...`.

**Baseline:** `release_distribution.py:280-281` inserts `attempts` before `attempt_count`. Installation/updating embed this details object (`installation.py:1446-1448`, `updating.py:397`), and baseline console insertion-order indent-2 output preserves that order (`console.py:267`).

**Input:** a terminal download failure such as HTTP 404. [INFERENCE] once emitted by installation through the intended insertion-order codec, public output bytes differ; sorted private failure records are unaffected.

**Minimal fix:** insert `attempts` before `attempt_count`.

**Missed regression:** current tests access keys and counts, not the emitted details byte order. An exact captured consumer failure receipt would detect this.

### F4 — P2 — A relative signing-key path is refused although the baseline accepts it

**Rust:** `src/bin/am_dev/release.rs:133-134` applies `expanduser` then `trust::record`; the trust reader requires an absolute path (`src/fs/trust.rs:86-88`). Argument parsing passes a relative `PathBuf` unchanged.

**Baseline:** `scripts/distribution/release.py:105` uses `os.path.abspath(path.expanduser())` before trust inspection.

**Input:** `am-dev release build --private-key ./release.key ...`, with an otherwise valid protected pinned key. [INFERENCE] Rust refuses the non-absolute path before signing; baseline accepts the same key. This report classifies it P2 under the requested wrong-refusal/parity rubric; the independent reviewer originally marked the impact minor/P3.

**Minimal fix:** after tilde expansion, anchor a relative key at the current directory without resolving its symlink away. Preserve final-entry symlink refusal and all key ownership/mode/pinning checks.

**Missed regression:** every key in `developer_builder_refuses_unsafe_and_unpinned_signing_keys_before_output` is absolute. Add a relative path to a real private fixture key and verify the same trust behavior, including relative symlinks refusing.

### F5 — P2 — Version-declaration parsing accepts/refuses different source text

**Rust:** `src/release_distribution.rs:668-676` counts every line beginning `metadata: ` using `str::lines`, which also removes a trailing carriage return.

**Baseline:** `release_distribution.py:146` matches only `^metadata: (\{[^\n]+\})$` in multiline mode.

**Inputs:** a valid metadata declaration plus a prose line `metadata: fields are described below` has one baseline match but two Rust candidates; CRLF metadata has no baseline regex match but is accepted after Rust strips CR. Neither witness was executed. Today's tracked SKILL.md has one ordinary declaration, so impact is limited, but these are observable accepted/refused-input gaps (P2 under the requested rubric; reviewer originally P3).

**Minimal fix:** split on literal newline and require the exact `metadata: {` prefix plus `}` line ending, preserving the baseline nonempty object-body condition. Do not normalize CR before matching.

**Missed regression:** fixtures have exactly one well-formed LF declaration. Cover extra prose metadata and CRLF before claiming version-parser parity.

## Focus areas with no defect identified by this review

This is source review, not runtime qualification:

- Candidate binaries/caches are not executed before verification. The first unauthenticated manifest only selects a stable tag; archive size/digest checks precede decompression.
- Manifest and binary signature namespaces are distinct fixed constants with the single pinned key source. Both cross-namespace rejection directions have tests in the reviewed tree.
- Archive extraction checks ordinary component paths, prefix confinement, duplicates, links/devices/PAX/GNU-long-name refusal, allowed modes, finite inflation/per-file/aggregate limits, strict zero trailer and one gzip member. Descriptor-relative exclusive no-follow creation avoids overwrites; CRC/ISIZE and multi-member behavior assessment relies partly on flate2's documented contract and author tests, not a new run.
- Bootstrap binding reads one bounded single-link binary and binds version/size/hash; installed verification includes mode, per-target runtime digest and exact inventory.
- curl restricts original/redirect protocols to HTTPS, disables curlrc and bounds redirects; file transport requires an explicit option. Retry/permanent-status precedence and finite owned-child deadlines match the intended baseline.
- Cache inventory/binding rechecks and pending-slot accounting retain the reviewed safety structure. The instructed known receipt change was excluded.
- Publication asset encoding is deterministic, existing assets are checked against expected names/size/bytes and uploads are draft-only; F1/F2 prevent complete idempotent publication.

## Integration observations, not additional accepted defects

- No installation caller exists at this base. It must fetch the tag matching the already-verified bootstrap binary, not accidentally reselect latest through the generic probe API.
- No current mechanism ties a supplied prebuilt binary's own declared version/source, or the builder's parser table, to `source_commit`; choosing a non-executing qualification mechanism needs a design decision. Runtime binding would refuse a wrong-version binary after publication.
- The builder/verifier artifact inventories appear compatible, but their digest equivalence is not asserted directly.
- A Rust-only pack cannot contain the Python module needed to qualify a `__pycache__` entry; Python-era receipts also lack the current target field. Installation should determine whether this recovery path remains needed; synthetic cache fixtures are not real signed Rust-pack proof.
- The builder no longer checks tracked install.sh's allowed-signers against the release key directly. A working-tree bootstrap test covers equality, unlike the baseline builder's render-equality gate. This is noted as a proof-boundary question, not an asserted exploit.

## Verdict and limitations

**Changes required.** F1 deterministically breaks successful/resumed publication after public mutation; F2 rejects oversized full API responses. F3–F5 are smaller parity gaps. Overall independent review confidence 0.85. No signature bypass, archive escape or runtime trust bypass was identified in the reviewed focus areas; absence of findings is not proof of absence.

No code or runtime tests were changed/run by the reviewer. GitHub sizes and integrated installation behavior are unmeasured; no credentials/network mutation were used. Cross-platform behavior was assessed statically. The pending receipt-v3/per-target cache change remains explicitly outside this verdict.
