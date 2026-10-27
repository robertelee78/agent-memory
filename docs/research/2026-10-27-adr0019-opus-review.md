# Review of ADR-0019 (proposed)

I only had the pasted ADR text to work from. This session has no file or shell tools, so I could not check claims against `project_identity.discover`, `scope_binding.require`, ADR-0004/0011/0018, or `test_codex_stop`. I also could not run `awa status --json` or the agent-memory turn workflow. The tracking/issue sync and any memory capture for this turn are still pending.

**Verdict: accept after revision.** The diagnosis is sound and the safety constraints in T1 are well chosen. But the decision handles a routine macOS event as if it were an exception, and several terms that change behavior are not defined.

## Blocking

1. **Device-number changes are probably routine, not rare.** On macOS, APFS volumes get their `st_dev` when they are mounted, and that number can change between boots. 16777234 → 16777230 is the same major number with a different minor, which fits a remount. If so, T1's fallback becomes the normal path after many reboots, and each repair adds another historical key. The ADR should either:
   - re-key on a stable component, such as volume UUID (`ATTR_VOL_UUID`/`getattrlist`) + inode, or a checkout ID stored in the common Git dir combined with an inode check to catch copies; or
   - explicitly reject those options under "Alternatives considered" and explain why.

   As written, the filename promises a "stable checkout binding" but the decision only provides recovery.

2. **T1 says MAY; it needs to say what hooks actually do.** Can `scope_binding.require` in the prompt/stop hooks continue with a recovered historical binding, read-only? Or does the turn stay pending until `am doctor --fix` runs? T4 ("prompt-to-stop-to-acknowledgment with a recovered binding") implies hooks continue. If so, T1 should say MUST for the hook path and state what the hooks report.

3. **"Unique" and "conflicting candidates" need definitions.** Vox already holds two keys for one project. Multiple historical keys that resolve to the *same* project UUID, bank UUID and location should not count as a conflict. Candidates pointing to *different* projects or banks should. Say this explicitly, or the Vox state could be misclassified as ambiguous.

4. **T3 has no persistence model.** "Recent same-repository hook failure metadata" has to be written somewhere, at the very moment the binding has failed. Specify:
   - the key (canonical common-Git location, not project UUID)
   - the storage location, which must be outside the bank so failures never touch bank bytes
   - retention and size limits
   - the fixed set of reason and stage codes, enumerated in the ADR or an appendix

## Should fix

- **"Current authority agree"** is undefined here. Reference the ADR that defines authority, and say which field is compared.
- **History growth:** cap or compact historical keys per project so repeated reboot repairs don't grow `projects.json` without bound.
- **Concurrency:** state that `doctor --fix` apply runs under the same lock that hooks use for registry reads. "Ordinary concurrency guards" is too vague to test.
- **Copied-bank detection:** say which evidence distinguishes a copied bank from the original under T1. With a matching inode and path this is very unlikely, but the rule should be stated.
- **Cross-harness rule:** "Claude, Codex and OpenCode use the same identity rules" is a decision, not a test. Move it into T1.

## Minor

- The title ("Survive filesystem device changes…") doesn't match the filename (`stable-checkout-binding-and-hook-diagnostics`). Align them.
- The note about the main checkout being behind `origin/main` is useful as a ruled-out cause. Label it that way, or drop it.
- Add an "Alternatives considered" section: drop `st_dev`, volume UUID, an in-Git-dir marker, or keep a strict exact match. The blocking item 1 response depends on it.
- The Delivery section is good. The rule against silently claiming unrelated v0.0.7 work and the separation of invocation from published memory in T3/T4 are both strong.

If you'd like, I can draft the revised T1 and T3 text with the enumerated reason codes and the stable-key alternative.
