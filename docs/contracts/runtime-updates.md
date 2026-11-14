# Verified managed runtime updates

The installed `am update --check` command previews the signed release channel
recorded in the external receipt; `am update` activates an eligible candidate
(`src/updating.rs`). Re-running the trusted bootstrap performs the same guarded
update with the verified bootstrap binary's own bytes. Signed releases are the
only update channel. A Python-era receipt (install receipt v1/v2 from v0.0.14
and older) and a managed Git `--remote`/`--ref` channel refuse with
re-bootstrap guidance; re-running the trusted bootstrap adopts that installation
once. Update does not require an existing project bank or principal setup and
never updates an explicit development source.

The external installation receipt remains the authority for the release source,
version, artifact identity and owned targets. Update refuses dirty or changed
ownership, downgrade, a same version whose signed identity changed, invalid
packages, a binary that differs from its signed manifest and a candidate whose
signed command surface removes `update`, `install`, `recover` or any command the
current runtime declares. Both surfaces are read from signed manifests; the
candidate is never executed to discover them.

For a newer release, the updater downloads the signed manifest, portable pack
and the separately signed per-target `am` into temporary input and verifies them
before any lifecycle write; it never changes the active runtime in place. An
unchanged release reuses the current verified artifact, independent of the
caller's umask, and reconciles allowed outside-sentinel text snapshots without
switching code. It validates the complete package
and its declared schema ranges before activation. The current directory,
registered local worktrees and explicit `--repo /absolute/root` selections use
the established one-bank locator. Discovered banks outside the readable and
bundled-migrator source ranges refuse activation. Unavailable registered paths
are reported as discovery gaps; a bank encountered later still needs its own
ordinary schema guard. Initial v1 has no migrators. No update writes or migrates
a bank, registry, feature branch, index or project code.

Check mode leaves installation targets, receipt/state, registries and banks
unchanged; its temporary candidate is removed. Its JSON reports
current/available revision and version, channel, cleanliness, compatibility,
update availability and changed loaded content. A check is a preview, not a
completed activation.

Activation holds the lifecycle OS lock, stages all owned target deltas and
pre-activation snapshots, then uses the installer's native per-target atomic
exchange/no-replace protocol. The external journal binds the before/after
identities. Candidate activation text is data in the complete package's
`templates/activation-markdown.example.txt`, preserving current user text outside
owned sentinels. Client-visible artifact readback and the actual installed
command's public dispatch precede receipt publication. There is no claim of one
global atomic switch across filesystems or proof of a running client's reload.

The flock inode stays stable; its marker is not truncated to write a new PID.
Current process/start metadata is atomically published in adjacent `owner.json`.
An old timestamp or PID does not override the OS lock. Competing lifecycle
operations report busy and preserve the active installation. A killed metadata
writer cannot leave a torn lock ownership marker.

Failure before activation retains the old runtime. Interrupted activation can
roll back or be verified as committed by `am recover`;
its receipt and target readbacks determine the result, not a historic journal
status alone. Lost success responses converge on retry without duplicate client
activation blocks. Externally modified recovery targets remain preserved.
Backups stay outside the active runtime with exact identities in the receipt.
The runtime root is replaced by native exchange, so a running `am` keeps
executing its previous file.

An unchanged repeat preserves targets and receipt and returns no restart list.
Changed loaded artifact or activation text identifies the affected clients in
`restart_clients`.

The [progressive update guide](../../skills/agent-memory/lifecycle/update/GUIDE.md)
provides the agent workflow. Functional tests in `tests/t2_lifecycle.rs` run
Rust-to-Rust updates from a locally signed release-v2 channel in disposable
homes and isolated client/config roots. Its before-write and after-write phase
tests interrupt, recover and retry every observed logical update write. Each
recorded point must actually be reached. They cover file staging, record
publication, owned payload snapshots and atomic node switches while preserving
a real native bank.

A directory payload copy plus its readback/fsync is one logical staging write;
disposable filesystem probes are not counted as owned publication steps. The
harness does not claim power-loss testing
at every kernel syscall. Both phase suites are required; splitting them retains
all interruption cases within the ordinary per-suite deadline. The broader
all-operation lifecycle harness and complete install/update/uninstall matrix
remain separate #60 and #52 outcomes.
