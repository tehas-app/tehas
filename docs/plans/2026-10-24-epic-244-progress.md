# Epic #244: progress and recovery notes

Kept by the session that builds the rest of epic #244. Updated as units land.
If that session stops, a new one starts here.

## Decisions by the owner (2026-10-24)

- One session builds the remaining units (#250, #251, #252, #257, #253, #254,
  #256) and may merge them. The factory and the backlog session stay off #244.
- No #244 work reaches a host before the whole feature set is in. v0.11.0 is
  not tagged until then. Patch releases (v0.10.6 and later) are cut from the
  last release tag plus the fixes, never from main while #244 pieces sit there.
- #244 units stay on their stacked branches and merge to main together, just
  before v0.11.0.

## Already on main

#245, #246, #247, #248, #249, #255 closed. #614 (verbs), #619 (store
concurrency), #622 (install.sh makes state/tehas.db), #623 (CHANGELOG names
v0.11.0) merged. #631, #632, #633 closed as duplicates of #614 and unlinked
from #250.

## #250 and #251: the shell callers move onto the store

The plan is `docs/plans/2026-10-24-store-callers.md`, on branch
`store-callers-store-v2` and every branch after it. Units, each a branch
stacked on the one before:

| # | Branch | State |
| --- | --- | --- |
| 1 | `store-callers-store-v2` (schema 2, new verbs) | done, reviewed, pushed |
| 2 | `store-callers-wrapper` (`lib/store.sh`, test harness) | done, reviewed, pushed |
| 3 | `store-callers-analytics-facts` | done, reviewed, pushed |
| 4 | `store-callers-cycle-records` (approvals, start, image, end, history) | built and pushed, in review |
| 5 | `store-callers-park-records` | not started |
| 6 | `store-callers-quiet-notes` | not started |
| 7 | `store-callers-plan-records` (#251) | not started |
| 8 | `store-callers-old-paths` (old-path guard, manual) | not started |
| - | `store-import` (from main: `store import`, install.sh runs it) | done, reviewed, pushed |

Work happens in `~/work/tehas-244-finish` (the caller chain) and
`~/work/tehas-250-import` (the importer). Each unit: build, then three
reviewers (behaviour, fail-closed and trust, tests and spec), then a fix pass,
then the fast gate. The plan's section 8 lists what the owner signs off in the
pull requests.

## How to recover after a crash

1. `git fetch origin` and list `origin/store-callers-*` and `origin/store-import`:
   the newest pushed unit is the last safe point.
2. In `~/work/tehas-244-finish`, `git status`: uncommitted changes there are the
   unit that was building. They survive a session crash. Read the unit in the
   plan, finish it, run the fast gate
   (`TMPDIR=/home/janit/work/.gate-tmp scripts/verify.sh >"$TMPDIR/gate.out" 2>&1; echo rc=$?`),
   commit, push.
3. Continue with the next unit of the plan on a new branch stacked on it.

## After #250 and #251

#252 and #257 side by side, then #253 (decide plus fixtures, then snapshot plus
shadow), #254, #256. #254 and #256 each need 7 days of shadow and 7 days live
on tehas-tehas before the bash they replace is removed.
