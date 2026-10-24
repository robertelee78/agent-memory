# Verified managed runtime updates

The installed `agent-memory update --check` command previews the recorded
managed channel; `agent-memory update` activates an eligible candidate. The
portable `scripts/update_runtime.py update [--check]` helper performs the same
operation from a trusted new package, including when an older installed command
does not yet dispatch update. It does not require an existing project bank or
principal setup and never updates an explicit developer checkout.

The external installation receipt remains the authority for the managed source,
origin, branch, current revision, artifact identity and owned targets. Update
refuses dirty or changed ownership, unexpected origin/ref, downgrade,
non-fast-forward history, invalid packages, an unsupported Python minimum and a
candidate that removes the current wrapper's lifecycle entrypoint.

For a newer revision, the updater clones the recorded channel into temporary
input rather than fetching or pulling inside the active source. An unchanged
channel reuses the current verified artifact, independent of the caller's umask,
and reconciles allowed outside-sentinel text snapshots without switching code. It validates the complete package
and its declared schema ranges before activation. The current directory,
registered local worktrees and explicit `--repo /absolute/root` selections use
the established one-bank locator. Discovered banks outside the readable and
bundled-migrator source ranges refuse activation. Unavailable registered paths
are reported as discovery gaps; a bank encountered later still needs its own
ordinary schema guard. Initial v1 has no migrators. No update writes or migrates
a bank, registry, feature branch, index or project code.

Check mode leaves installation targets, receipt/state, active Git metadata,
registries and banks unchanged; its temporary clone is removed. Its JSON reports
current/available revision and version, channel, cleanliness, compatibility,
update availability and changed loaded content. A check is a preview, not a
completed activation or a signature-verification claim. The initial channel
continues to use the explicitly unsigned Git distribution contract.

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
roll back or be verified as committed by the trusted installer `recover` helper;
its receipt and target readbacks determine the result, not a historic journal
status alone. Lost success responses converge on retry without duplicate client
activation blocks. Externally modified recovery targets remain preserved.
Backups stay outside the active source with exact identities in the receipt.

An unchanged repeat preserves targets and receipt and returns no restart list.
A fast-forward change outside the package also produces no client restart advice.
Changed loaded artifact or activation text identifies the affected clients in
`restart_clients`.

The [progressive update guide](../../skills/agent-memory/lifecycle/update/GUIDE.md)
provides the agent workflow. [Functional tests](../../tests/contracts/test_runtime_update.py)
run actual installed/helper commands in disposable homes, isolated client/config
and credential roots, with local file-Git channels. [Before-write](../../tests/contracts/test_update_write_before.py)
and [after-write](../../tests/contracts/test_update_write_after.py) suites kill,
recover and retry every observed logical update write. Each recorded point must
actually be reached. They cover file staging, record publication, owned payload
snapshots and atomic node switches while preserving a real native bank.

A directory payload copy plus its readback/fsync is one logical staging write;
Git-internal temporary-clone writes and disposable filesystem probes are not
counted as owned publication steps. The harness does not claim power-loss testing
at every kernel syscall. Both phase suites are required; splitting them retains
all interruption cases within the ordinary per-suite deadline. The broader
all-operation lifecycle harness and complete install/update/uninstall matrix
remain separate #60 and #52 outcomes.
