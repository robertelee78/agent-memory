# Wrong-kind negative recall in daily-use qualification

Independent review of the completed frozen `81fec82` required 12-session/1,000-lesson
run found two vacuous recall assertions. Command916 checks deleted seed0 and
command973 checks older seed1 after a withheld replacement. Both requests selected
`user-feedback`; both actual seed targets are `semantic-fact`. An empty result
therefore did not prove deletion nonresurrection or withholding of the older claim.
This is a qualification defect, not evidence of a product defect.

The shared helper's default matched newly admitted ordinary/wall learning. It was
reused for seeded facts without an explicit kind override. The assertions checked
absence without first demonstrating that the same kind/ID/component selection
could retrieve the original eligible source. Counting completed named events hid
that mismatch. Review checked actual request bytes against actual source metadata,
rather than accepting the event names or exit0 as semantic coverage.

The repair keeps ordinary `user-feedback` selection unchanged. Seed negatives
explicitly request `semantic-fact`. Before deletion, positive controls in the
originating and receiving older worktrees assert the exact seed metadata/version
and body under component `src/seed/0`. Before introducing the withheld replacement,
the same receiving session positively recalls exact seed1 metadata/version/body
under `src/seed/1`. The later negatives use the same record kind/ID/component.
No selector or product safety/applicability gate is relaxed.

The first unexecuted patch (`22aa98e`) accessed a nonexistent positive-result
`metadata` field. Static review caught it before qualification; `c3e82e9` uses
the public `record` field. This pre-execution apparatus correction is retained,
rather than reported as a product failure or successful test run.

The original exit0 run, all974 calls and independently verified functional/source
and preservation evidence remain retained with the two gaps plainly marked in
[its qualification status](data/2026-10-26-daily-use-final/required12/qualification-status.json).
The source-bound independent report verifies1699 compressed/decompressed blobs,
82 review-page acknowledgements,444 ordinary-learning payload comparisons across
24 refreshed bank/round maps,48 seed inventories and164 feature observations.
Its31 refresh-apply result files were not retained as individual blobs; those
peer assertions remain executed-controller/input-plan evidence, not newly reread
raw result bodies. Neither that limitation nor the wrong-kind gaps are hidden.

The in-progress older required24/10k semantic run is superseded rather than allowed
to consume its remaining hours with the same deficient checks. Its parent is
paused; existing owned descendant commands finish before SIGINT/resume permits
the controller's ordinary exception/finally cleanup. Interruption evidence records
zero observed live descendants before that signal. Its KeyboardInterrupt/driver
exit and partial results remain an agent-controlled qualification abort, not a
product timeout, corruption claim, successful cohort or final-state proof.

Fresh complete required12/1k and24/10k semantic schedules must use the corrected,
independently reviewed frozen controller, after a small full-schedule preflight.
The two core-only independent variations never enter `post_round_semantics`;
their exercised default requests retain the same values, so source-bound impact
review can carry them forward without an automatic repetition. Product runtime,
contract-test registry and native suites are unchanged. The completed/current
frozen81 native gates retain their exact-source scope; this controller-only fix
does not justify repeating them. Existing #99 owns this repair and qualification;
it remains Executing / At risk until whole independent acceptance. The seven-second
warm recall budget and precision/recall priority are unchanged.
