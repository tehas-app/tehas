# Epic #244: handover

The session that built #250 and #251 on 2026-10-24 wrote this as its handover.
A new session starts here. The epic's own body (#244, "Status (2026-10-24)")
says the same in short.

## Owner decisions

- One session builds the rest of #244 (#252, #257, #253, #254, #256) and may
  merge each unit itself. The factory and the backlog session (tehas-4a) stay
  off #244 and its sub-issues.
- Pull requests merge once CI's `verify` is green. The build host is often
  overloaded (load 40 to 50), so a local gate can fail on timeouts alone. CI on
  the runner VM is the clean run.
- **Open question for the owner: tag v0.12.0 now?** On 2026-10-24 the session
  recommended tagging main (6a1e775) as v0.12.0 and deploying it to tehas-tehas.
  Reason: the store has to be live there before #253's shadow and the proof weeks
  of #254 and #256 can start. The deploy would be: drain, install, check the
  import counts and `store check`, then `limit clear`. Coordinate the timing with
  the session moving tehas-tehas to the GitHub App (work-66). No answer yet.
  Don't tag without the owner's yes.

## Where things are

**Closed:** #245, #246, #247, #248, #249, #250, #251, #255.

**#250 and #251 merged** as PR #683 (6a1e775). That PR:
- moved the shell commands onto the store's verbs through `lib/store.sh`
- took the store to schema 2
- added `store import`, which runs on install
- added the old-path guard (`test/store_paths_test.sh`, and the poll stops at 78
  on an old record file)

The design is `docs/plans/2026-10-24-store-callers.md`. Its section 8 lists
what the owner should sign off; the PR text repeats it. Nothing is signed off
yet.

**Releases:** v0.11.0 (PR #668) and v0.11.1 (0fa178f) were cut from main by
other sessions before #683. They carry #622 only: the store is installed and
`tehas.db` made, but nothing uses it. The next release from main is the first
that migrates and imports. It needs the drain step in CHANGELOG Unreleased,
Adopter action.

**#676** (the fixer grant renamed to millwright) touches the store. Its store half
must be a schema 3 migration that still accepts `fixer` rows. The backlog session
splits it and was told it can start now that #683 is merged.

## Now: #252 and #257 are PRs

Workflow `wf_f24ccaaa-d4e` died about a minute after it started, when its
session ended (2026-10-24 22:19 local), and left nothing. The owner chose to
build both by hand in one session.

- **#252: PR #702**, merged 2026-10-25. Sign-off asked: D7
  (dispatch.json and health.json go through the publisher but are still
  computed by their commands). Schema 3 is taken here; #676 needs schema 4.
- **#257: PR #711** (branch `host-retention`, `~/work/tehas-257`), main merged
  in on 2026-10-25, CI green, waits for the owner's merge (auto mode refuses
  `gh pr merge`). Sign-off
  asked: D4 (unused for 30 days, as the host's evidence, in place of "closed").
  Left open: a size bound for long-lived logs, and pruning the poll tables once
  #253 and #254 write them.
- Both add an Unreleased changelog entry with a drain in Adopter action.
- `test/dispatch_start_test.sh` flakes on CI (the start-wait cases), on main
  as well; a rerun passes.

## Left after that, in order

1. **#253**, the dispatcher in Rust: split in four pull requests (owner's
   choice, 2026-10-25), plan `docs/plans/2026-10-25-dispatch-decide.md`. Part 1
   is PR #747, merged 2026-10-25. Part 2 is PR #758, merged 2026-10-26. Part 3
   is split in two (owner's choice, 2026-10-26): 3a is PR #764 (branch
   `dispatch-rule4`), merged 2026-10-26. 3b (PR #771) and 4a (PR #778, the
   snapshot and the `tehas-dispatch` binary) merged 2026-10-26. The owner
   lets this session merge #253/#254/#256 PRs on green CI (2026-10-26).
   Next: 4b, the host probes left (model endpoints, image, sweep), the
   shadow beside each poll, `logs/dispatch-shadow.log`, install, manual.
   D7 measured: a page of 100 issues costs 4 points.
2. **#254**, part 2: apply through the action journal, then a setting chooses
   bash or rust. It needs 7 days of shadow and then 7 days live on tehas-tehas
   before a separate PR removes `libexec/dispatch`.
3. **#256**, the cycle supervisor in Rust, with the same proof. It is a security
   change (rule 4).

## How the work was run

- Each unit was built tests first, reviewed by three lenses, fixed, then gated:
  `TMPDIR=/home/janit/work/.gate-tmp scripts/verify.sh >"$TMPDIR/gate.out" 2>&1; echo rc=$?`
  (never piped).
- A 5-minute check pushed every new commit and snapshotted uncommitted work to
  `wip/<branch>` with `git stash create`, so the agent's branch was never touched.
  To get a snapshot back: `git fetch origin wip/<branch>`, then
  `git stash apply FETCH_HEAD` on the unit's branch.
- Main moves fast. Merge main into the stack before opening a PR, and again if
  the PR shows conflicts: CI does not run on a conflicting PR.

## Worktrees from this session

- `~/work/tehas-244-finish` (`epic-244`) and `~/work/tehas-250-import`
  (`store-import`): merged, can be removed.
- `~/work/tehas-244-notes` (`epic-244-notes`): this file.
- `~/work/tehas-252`, `~/work/tehas-257`: in use.
- Old remote branches `store-callers-*`, `store-import`, `epic-244` and `wip/*`
  can be deleted once #252 and #257 are merged.

## Housekeeping found on the way

- PID 195735 is a `scripts/verify.sh` that has been running for more than 3 days
  from a deleted directory, `~/work/tehas-rel-489`. It belongs to another session.
  Not killed; the owner was asked.
- `install_fresh_test.sh` "waits for it and installs when it is let go" has a 9 s
  bound that fails under load. It passes alone.
