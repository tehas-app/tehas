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

## Running now: #252 and #257

Workflow `wf_f24ccaaa-d4e`, started about 22:30 on 2026-10-24. Its script is
`~/.claude/projects/-home-janit-work-tehas/9526b511-ba3a-4d83-9ae6-e061ca20ba60/workflows/scripts/epic-244-publish-retention-wf_f24ccaaa-d4e.js`.
For each issue it does this:

1. Two designs, then a plan committed to `docs/plans/2026-10-25-*.md`.
2. Units built one after another, each on a branch stacked on the one before.
3. Three reviewers per unit, then a fix pass.
4. A final check against the issue's Done-when list.

| Issue | Worktree | Base branch | Unit branches |
| --- | --- | --- | --- |
| #252 one publisher for `live/` | `~/work/tehas-252` | `publish-live` (from 6a1e775) | `publish-live-<unit>` |
| #257 retention, backups, log rotation | `~/work/tehas-257` | `host-retention` (from 6a1e775) | `host-retention-<unit>` |

The workflow only resumes inside the session that started it. If that session is
gone:

1. In each worktree, run `git log` and `git status`. The newest branch is the last
   unit; uncommitted changes are the unit that was being built.
2. Read the plan file there and finish that unit. Run the fast gate, commit,
   then continue with the next unit.
3. When an issue's units are done, open one PR from its last branch to main and
   merge it once CI is green.

## Left after that, in order

1. **#253**, the dispatcher in Rust, part 1: snapshot and decide, run in shadow.
   Split it as its resume notes say: decide plus fixtures in one PR, then snapshot
   plus shadow in a second. Measure the GraphQL cost against spec section 6.
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
