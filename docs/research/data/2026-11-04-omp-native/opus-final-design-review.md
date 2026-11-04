**Verdict: TECHNICALLY READY FOR OPERATOR APPROVAL.** N1 is closed and I found no blocking contradictions. This verdict covers the design text only. It is not operator approval, and it doesn't qualify any implementation, native behaviour, platform, peer or lifecycle proof.

I could not recompute the SHA256 (`bd7c6c82…643c`) because I can't run commands, so I haven't confirmed the hash. I read the whole plan, with the wake, trigger and acceptance lines (45, 88, 96, 98, 132) in raw form; the earlier review; and the retained `turn_sources.py`, including `reserve_wake` at lines 297–314 and `register` at 154–160.

## N1 closure

| Check | Where in the plan | Result |
|---|---|---|
| The omp root record is the only budget | Line 96: "ONE automatic request per genuine human_root is enforced ONLY by the omp root record". It is created once under the transport lock at `root-wakes/<human_root>.json`. It is keyed by root only, and "successors/new fingerprints cannot reset it". | Closed |
| No core `reserve_wake` | Line 96: "Omp does NOT call shared core reserve_wake for current or earlier namespaces", with the reason given (two slots, repeated fingerprints). Line 132 requires that "no core wake reservation gates delivery". The only other core link is `control_origin_hint` (`register` lines 155–158). Omp doesn't need it because omp guidance is never registered as a source and is recognised by the omp classifier (line 84). | Closed |
| Bounded inline guidance | Line 88: total limit of 8192 UTF-8 bytes; 1024 bytes kept for command, cursor and fault data; at most 7168 bytes of whole entries, never split; stable session/lifecycle/source-reference order. | Closed |
| Explicit cursors and fault codes | Line 88: anything left out is shown through a root-qualified `--list-pending` cursor plus `inventory_complete:false` and fault codes, and "no partial page claims completeness". The full scan runs on later approved tool calls, "not inside a callback", which fits the one-second callback limit on line 98. | Closed |
| A healthy root is unaffected by foreign faults | Line 88: "Busy/faulted foreign namespaces do not refuse a healthy current root". The only refusal is when the current root's own inventory changed, and that doesn't use up the wake. Line 96 adds that "neither old reservations nor foreign contention/faults suppress the healthy new root". | Closed |
| Carry-forward at H2 and H3 | Line 96: an omitted source P is offered at H2, H3 and later roots with its original identity, inline when it fits or through the cursor otherwise. Line 132 tests H2 and H3, a backlog over 8192 bytes, a permanently faulted foreign namespace and contention from a concurrent acknowledgement. | Closed |

Both points the earlier review raised for these lines are fixed:
- the old "refuses reservation without omission" rule is gone, and line 90's "complete-inventory rules" now apply only to status and compatibility clearing;
- "earlier namespaces' fingerprints for core reservations" now appears only as an explicit "does NOT".

## Contradictions and safety

- **No conflicts left.** "Reserve bounded advisory wakes" (line 15) now refers to the omp root record. Trigger suppression is added on top of the root budget (line 45). Integration-only and foreign stops never reserve another wake (line 96). Lost, capped or aborted scheduling stays pending (lines 96 and 98). The wake schema (line 88) refers to the current root and does not claim the whole bank is complete.
- **The earlier non-blocking note on concurrent publication is fixed.** The paragraph after line 135 now requires every consumer to check or search the bank again right before writing. It also says publicly that the same lesson can still be recorded twice in different words.
- **Safety and authority are unchanged.** The callback returns only `continue:true` and has no write authority over canonical memory (lines 15, 88 and 98).

## Non-blocking points (fix during implementation; none blocks approval)

1. **Line 45** still says `foreign-continuation` "permits the other five" triggers, which includes `own-hidden-pure`. The same line also says `own-hidden-pure` creates no segment, so it isn't a real contradiction, but the wording could be tightened. This was carried over from the earlier review.
2. **The cursor syntax is undefined.** Line 88 refers to a "root-qualified `--list-pending` cursor", but line 90 only defines `--list-pending --offset N`. The implementation spec should define that syntax exactly.
3. **The fault codes in wake guidance are not a closed list.** Elsewhere the plan uses fixed codes (enrollment-pending, registration-fault, pending-coverage-gap), so the guidance codes should be fixed the same way.
4. **Inline priority is unclear.** Line 88 doesn't say whether current-root references are inlined before earlier work. Omitted current-root references are still covered by the cursor, so nothing is lost either way, but the order should be stated.

Approval still leaves every obligation in lines 127–139 to be proven on real installations after implementation.