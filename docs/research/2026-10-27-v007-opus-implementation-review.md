I reviewed this from the diff alone. This session has no shell or file tools, so I couldn't run the tests or `awa status --json`, and I couldn't run the agent-memory turn workflow. Your diff doesn't include `codex_prompt.py`, `codex_stop.py`, `GUIDE.md` or the three test files, so I couldn't check them either.

The historical-device matching is careful overall. A match needs the same inode, a matching location, a single decision, a matching bank project ID and matching remotes. `observed_checkout_key` is left in the `binding_guard` comparison, so a device change between preview and apply still fails the guard. Here's what I'd change, most important first.

## Should fix

**1. One corrupt failure record permanently blocks recording and clearing** (`hook_failure.write`). Before every write, `write()` validates the old file with `read_one()`. If that file is unreadable, nothing can replace or remove it:
- `json.loads` raises a raw `JSONDecodeError` or `UnicodeDecodeError`, not `ContractError`.
- `validate()` raises `TypeError` if `stage` or `reason` is a list or dict, because `x in STAGES` needs a hashable value.
- Nothing in `doctor --fix` repairs this.

Suggested fix: in `write()`, check only that the file is safe (regular, owned, `nlink == 1`, mode, size), not that its content is valid. Make the read paths turn any parse error into `ContractError`.

**2. Worktrees share one failure record.** The key in `hook_failure.paths()` is `sha256(common_git_dir + harness + adapter)`, and all worktrees share the common dir. That causes three problems:
- A failure in worktree A shows up in `am doctor` for worktree B.
- A successful prompt in B clears A's real failure.
- Hooks running at the same time in two worktrees compete for the same `flock(LOCK_NB)` and get `BlockingIOError`.

You work in worktrees, so this will happen. Add the resolved repo top-level to the key, or explain in a comment why it's meant to be shared per repository.

**3. Rebind or fork on a historical binding changes the old alias** (`project_identity.apply`). Only `link` is redirected to `observed_checkout_key`. When the evidence is `historical-device-change`, `rebind` and `fork` still write to the old historical key, so the current device never gets an exact entry. The old alias's project also changes underneath any other aliases. Either send every action to the observed key in this case, or refuse anything except `link` until the alias is saved.

**4. `apply` can overwrite an existing entry under the observed key.** `updated["checkouts"][observed] = deepcopy(historical)` runs unconditionally. If another process registered the observed key for the same project between preview and apply, `binding_guard` passes because the project IDs match. That entry's `history` and `locations` are then silently replaced. Make apply refuse if the key already exists, or merge into it.

## Worth fixing

5. **`doctor_fix` reports `writes_performed = True` whenever a repair was attempted.** That's wrong when `preview()` fails before writing anything. Use `True` only for `repaired` and `None` for `refused`, matching the existing pattern.
6. **The `refused` message says "preserve the original proposal".** Doctor never had a proposal. Also, if `apply` succeeded and only the readback in `install.require` failed, the binding may already be saved, so the message should say the state is uncertain.
7. **Possible conflict with lifecycle locks.** The identity repair runs before the bank-transaction repair, and nothing checks whether a lifecycle operation is pending or whether `doctor_fix` already holds the lifecycle lock. If `scope_binding.apply` takes the same lock, it will deadlock or refuse. Confirm the ordering, or skip this repair when `pending` is set.
8. **Race when reading failure records.** `read_one` checks `lexists` and then calls `open`. If a hook clears the record in between, `FileNotFoundError` turns `hook.outcomes` in `am doctor` into an error. Treat `ENOENT` at open as `None`.
9. **Every successful hook creates a lock file.** `clear()` always creates `<key>.json.lock`, whether or not a record exists, and lock files are never removed. That's one file per repo and adapter, plus a `flock` and a directory fsync on every prompt. Check `lexists(record)` before taking the lock (and check again once you hold it).
10. **Hard-coded harness name in `diagnostics.hook_outcomes`.** It builds `'hook.codex.' + adapter`; use `item['harness']`. Also put the timestamp in the message, because a "last observed" failure is misleading without its age.

## Minor

- **Relative `XDG_STATE_HOME` is rejected.** The XDG spec says to ignore a relative value and fall back to the default. Rejecting it disables hook diagnostics.
- **The location check doesn't cover the common git dir.** `root.resolve().is_relative_to(repo)` only checks the worktree. State placed under the main checkout or the common `.git` dir passes.
- **The project history event logs the old key.** It records `proof["checkout_key"]` (the historical key), not the new alias. Recording `observed_checkout_key` would make the audit trail show the alias step.
- **Device numbers can pile up.** Each device change adds a permanent alias. After a later explicit rebind, the next device change will find two different decisions for the same inode and location, and fail with "Conflicting historical checkout bindings". That's the safe outcome, but it's worth documenting in `GUIDE.md`.
- **Unused code in `diagnostics.project()`.** The `bank` argument isn't used any more, so the bank's `project_id` no longer takes priority. `data` is now only validated. Check that dropping the bank priority is intended.

## Tests I'd want in the untracked files

- A corrupt record (bad JSON, and a list-valued `stage`) followed by `record()` and `clear()`.
- Two worktrees sharing a common dir.
- `rebind` and `fork` when the evidence is historical.
- The observed key being registered between preview and apply.
- Old pending proposals with no `binding_source` or `observed_checkout_key`. These should still pass `binding_guard`, because the comparison only loops over keys in `original`.
