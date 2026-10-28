# v0.0.9 refresh repair qualification

The actual operator Linux host reproduced the installed v0.0.8 refresh refusal
under an owned 0700 home/state barrier with 0775 runtime intermediates. The
candidate quick check now reports bank-synchronized and remote freshness verified,
with no canonical bank or original index writes. An earlier before/after snapshot
also verified unchanged bank bytes, HEAD, index and worktree state. The installed
runtime remains v0.0.8 until the corrective signed release is published and adopted.

The final focused contract runs passed on macOS and the operator's Linux host:

| Suite | Cases | macOS | Linux |
| --- | ---: | ---: | ---: |
| Shared path policy / refresh diagnostics | 10 | 21.97s | 8.05s |
| Combined turn checks / stage health | 4 | 37.89s | 10.40s |

There were no skips, expected failures or unexpected successes. Coverage includes
protected 0775 paths, refusal of unsafe unprotected state, symlinks/foreign
ownership, barrier permission/rebinding changes, retained orphan journals, safe
specific diagnostics, and real final-source mutation through both API and public
CLI. Source changes preserve the exact mutation and refuse stale freshness.
Doctor reports the exercised local prerequisites and labels remote freshness as
not exercised. It does not convert a setup check into proof of remote freshness.

A separate installed-source receipt discrepancy was caused by a diagnostic Python
import creating 34 bytecode files, not by changed owned payload bytes. The exact
cache was independently verified and atomically quarantined with retained private
manifest and hashes. No signed-source verifier exemption or receipt rewrite was
introduced. Installed doctor subsequently validated the original publisher
signature with zero warnings. Direct imports use -I -B; documented packaged helper
examples now use the existing version-selecting, bytecode-safe python3.sh launcher.

Public signed v0.0.8 installations were prepared on both platforms for actual
public v0.0.9 upgrade checks after publication. Preparation itself passed in
7.78s on macOS and 5.15s on Linux; these are not claims that v0.0.9 was delivered.

Independent review and exact implementation hashes are retained in the
[ADR-0021 review](2026-10-28-adr0021-implementation-review.md). Native PM dialogue
and signed delivery qualification remain separate acceptance stages.
