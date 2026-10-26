# Epic #244: handover

The session that built #250 and #251 on 2026-10-24 wrote this as its handover.
A new session starts here. The epic's own body (#244, "Status (2026-10-24)")
says the same in short.

## Owner decisions

- One session builds the rest of #244 and may merge each #253, #254 and #256
  pull request itself once CI's `verify` is green (2026-10-26). The factory and
  the backlog session stay off #244.
- Session tehas-69 took #256, the cycle supervisor, at the owner's request
  (2026-10-26). Its plan is `docs/plans/2026-10-26-cycle-supervisor.md` on its
  branch. This session keeps #254.
- Running `TEHAS_DISPATCHER=rust` live on tehas-tehas needs the owner's yes.

## Where things are (2026-10-26, evening)

**Done:** #245 to #253, #255, #257. The store is live on tehas-tehas
(v0.13.0 and later); v0.14.2 is the latest release.

**#253, the dispatcher's rules in Rust:** complete. The shadow runs on
tehas-tehas while `~/tehas/state/dispatch-shadow.on` is there, since
2026-10-26 13:23:06Z. Its 7 days end 2026-11-02 13:23Z. Differences go to
`~/tehas/logs/dispatch-shadow.log`.

**#254, the dispatcher acting in Rust:** plan
`docs/plans/2026-10-26-dispatch-apply.md`. Merged: part 1 (#785, journal
verbs and plain writes), 2a starts (#788), 2b stops (#789), 3a planning end
(#794), 3b planning start (#796), 4a the poll's frame (#798), 4b the start
gates (#804), and 4c (#831, merged 20:06Z): the `TEHAS_DISPATCHER=rust`
switch and the journal settled at the start of a poll. #254's code is
complete.

How the Rust poll is tested: `T_RUST_POLL=1 bash test/dispatch_<x>_test.sh`
runs every poll of a dispatch test file in Rust. All 28 files pass. The page
stub in `test/dispatch_lib.sh` answers its GraphQL page from the bash's
fixtures. Checks of the bash's own REST reads are marked `bash_reads`, each
with a comment saying why.

## Left, in order

1. #831 merged 2026-10-26 20:06Z.
2. After 2026-11-02 13:23Z: read the shadow log for the week. Explain or fix
   every difference.
3. With the owner's yes: `TEHAS_DISPATCHER=rust` on tehas-tehas for 7 days
   (a settings change in industry-settings, then a pull on the host).
4. A pull request that makes `rust` the default, then one that removes
   `libexec/dispatch`'s poll (the bash ceiling drops a lot).
5. #256 is tehas-69's. It will want the journal's apply in a shared crate and
   asks before it moves anything in `console/dispatch`.

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

- `~/work/tehas-244-notes` (`epic-244-notes`): this file.
- `~/work/tehas-254g` (`dispatch-journal-settle`): #831.
- `~/work/tehas-253*`, `~/work/tehas-254`, `-254b` to `-254f`, `-252`, `-257`:
  merged work, can be removed.
- `~/work/tehas` is a shared checkout other sessions switch; it is stale.
  Read code only from a worktree.

## Housekeeping found on the way

- PID 195735 is a `scripts/verify.sh` that has been running for more than 3 days
  from a deleted directory, `~/work/tehas-rel-489`. It belongs to another session.
  Not killed; the owner was asked.
- `install_fresh_test.sh` "waits for it and installs when it is let go" has a 9 s
  bound that fails under load. It passes alone.
