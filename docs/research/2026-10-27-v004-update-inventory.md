# v0.0.4 update inventory qualification

The v0.0.2-to-v0.0.3 update took 6 minutes 25 seconds on the operator's
installation. This measures the initial migration, not a claim about every update.
On the same machine a read-only compatibility pass from v0.0.3 (`bb61835`)
took 41.94 seconds; the v0.0.4 candidate (`d9d9563`) took 15.35 seconds.
The baseline observed 599 repositories, 145 banks and 70 discovery gaps; the
candidate observed 601 repositories, 147 banks and the same 70 gaps. Two
worktrees were created between observations. Every baseline repository, bank
and gap was present with matching metadata in the candidate output. These are
single-pass observations under nonidentical live load, not a full update-time
claim or a performance guarantee.

An initial cache prototype could hide missing or redirected Git objects; it was
rejected. The candidate uses bounded parallel reads and deliberately retains no
Git object cache. Independent controls reproduced the previous failures and
confirmed missing or redirected objects still fail closed. A four-worker bound,
complete root inventory and error propagation are exercised by
`tests/contracts/test_update_parallel_inventory.py`.

The candidate's public lifecycle controls and discovery-gap suite passed on
macOS and Linux, including unsupported bank schema refusal and an update check
followed by activation and repeat. These fixture tests are separate from the
real-inventory timing above. Further signed-channel, changing-state and final
integrated-source controls are recorded separately before release.
