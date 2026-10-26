**APPROVED FOR STATIC SCOPE**, within the limits listed in the prior review.

I compared the two files line by line, and the guard meets the condition I set:

- **The only change is the guard.** `public-release-journey.py:5` adds `if not __debug__: raise SystemExit('Public canary requires assertions; rerun without Python optimization')`. Every other line matches `prior-canary.py`, just moved down by one, so the B1/B2 repairs I confirmed earlier are unchanged.
- **It catches optimise mode.** `__debug__` is False under `-O`, `-OO` or a non-empty `PYTHONOPTIMIZE`, which is exactly the trigger I described.
- **It runs before anything has an effect.** It comes before argparse, imports from `--source`, reading `retained-world.json`, the `open('x')` attempt marker, the `mkdir` of `--out`, `World()` and every product command. With asserts stripped, the run stops before any evidence is written, any World is touched or deleted, or any `passed: True` is recorded. It also sits outside the `try`, so no `failure.txt` or `passed: False` output is written either. That's correct for a refusal.
- **What you observed is what the code should do.** A `SystemExit` with a string message prints it to stderr and exits with status 1. That happens before argparse can handle `--help`, so exit 1 with the refusal and zero product commands is the expected result.

This is static review only. It isn't a public run, release acceptance or delivery, and the post-publication runs are still pending.