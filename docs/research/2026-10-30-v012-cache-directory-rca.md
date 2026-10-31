# Protected cache-directory refusal: RCA and v0.0.12 qualification

The actual signed v0.0.11 repair attempt on Beast failed before activation in
1.425 seconds. Python had created a current-owner `__pycache__` directory with
mode 0775 under umask 0002; its 25 cache files were 0644 and the source root was
0700. The existing T1 descriptor walker accepted that protected path, but the
new qualifier applied a second blanket directory-mode rejection. That duplicate
condition contradicted the shared private-ancestor policy and prevented repair.
No unsafe payload or reachable group-write condition was established.

The release tests missed the case because their cache fixtures chose restrictive
directory modes. One existing negative even asserted that a protected 0777 cache
directory should refuse, encoding the mistaken rule. Earlier inventory checks
compared payload and cache files without turning actual directory modes into
acceptance cases, and actual-profile preflight was not run before v0.0.11
publication. Synthetic success therefore did not establish normal umask support.
The failed adoption remains retained; its result is not reported as success.

ADR-0021 T8 removes exactly the duplicate directory-mode predicate. The same
pinned T1 root-to-leaf policy still enforces reachability, ownership and path
binding. All signature, signed payload/mode, cache file-mode/link/name, inventory,
source fingerprint, transaction, backup and race checks remain unchanged. No
cache is parsed or executed and no permission or receipt is edited. The existing
root-owned sticky-directory exception is inherited unchanged.

Actual Opus design review accepted source
`36a521b82285b91f005d2c46aad3a46b24cc96b1`; independent design and implementation
review accepted final candidate `c38bf307c13e90f192bcfd9bb30ff44a0fa7d10e`.
Independent public fixtures passed three cases and ten commands in 29.890 seconds
(maximum command 3.99 seconds): real Python `py_compile` under umask 0002 produces
0775/0644, protected 0775 and 0777 recover with exact old-tree backups and clean
no-ops, and a truly reachable writable cache path refuses with the T1-specific
reason. The worker separately reproduced 0777 through actual umask 0000. Existing
foreign-owner policy simulation remains a simulation, not a privileged public
ownership-change trial.

Before publishing v0.0.12, a clean archive of the reviewed helper ran read-only
against Beast's actual installation. All 25 caches (481,850 bytes) and all 13
targets qualified in 3.726 seconds. Source bytes/modes/owners/link counts, exact
receipt bytes, bank, index and HEAD were unchanged. It selected the authentic
published v0.0.11 candidate and performed no activation: this proves eligibility,
not signed v0.0.12 delivery or actual healing.

[Retained proof identities and redacted preflight](data/2026-10-30-v012-cache-directory/)
keep actual compatibility repository inventory private. The historical v0.0.11
record contains three completed public journeys/24 commands (Mac fresh and signed
v10 CLI upgrade, Linux fresh). A missing Linux retained v10 baseline stopped its
controller before a CLI command; that upgrade is not claimed. Beast's v11 repair
failed. Historical T5/T6/T7 delivery can only describe their narrower published
contract, with T8's correction explicit. Actual signed v12 adoption and its no-op
must be verified separately after publication; the v11 tag/assets stay immutable.
