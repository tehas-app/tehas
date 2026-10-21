---
title: Troubleshooting
description: "What each stop in a tehas cycle means, and what to do about it."
---

# Troubleshooting

## Where to look

| Question                         | Place                                                                   |
| -------------------------------- | ----------------------------------------------------------------------- |
| Is the factory taking work?      | `tehas status`                                                          |
| Is there room for another cycle? | The `slice` line of `tehas status`; `~/tehas/live/dispatch.json`        |
| What did the dispatcher decide?  | `tehas dispatch --dry-run`, `~/tehas/logs/dispatch.log`                 |
| What did a cycle's runner print? | `~/tehas/logs/dispatch-work-<issue>.log`                                |
| What did the driver say?         | The runner's NDJSON progress, `~/tehas/logs/work-<issue>-<time>.jsonl`, and its result beside it, `.jsonl.result.json` |
| Where is the cycle now?          | While it runs, its step on the [dashboard](/dashboard), from `~/tehas/live/cycles/<issue>.json`; once it ended, `jq . ~/tehas/state/cycles/<issue>.result.json`. The workspace of a cycle that left a pull request is removed; one that stopped is kept, and a retry moves it to `tehas-<issue>.before-retry`. A start that parked before the container ran keeps the clone it made and the one before it: neither is removed. |
| What did one agent do?           | Transcripts under `~/.pi/agent/ensemble-runs/issue-<issue>/<date>/`     |
| What did the improvement loop do? | `~/tehas/logs/improve.log`, one line per run                           |
| Is a cycle running?              | `systemctl --user status 'tehas-cycle@*'`, or `docker ps --filter name=tehas-work` |
| What did the previews do?        | `~/tehas/logs/preview.log`, and `~/tehas/logs/preview-<pr>.log` for one build |
| What state is a preview in?      | `jq . ~/tehas/preview/<pr>/state.json`, or "Previews" on the [dashboard](/dashboard) |

The driver's notices, one per line:

```
jq -r 'select(.type=="extension_ui_request" and .method=="notify") | .message' \
  ~/tehas/logs/work-<issue>-*.jsonl
```

## A command stops with `tehas: ... is not set` or `is not valid`

Exit 78. The instance settings are missing or malformed and the command did
nothing. The line names the setting and the file. Fix it there, then check:

```
cd ~/tehas/platform && ./install.sh --check
```

- `no instance.env in <dir>`: the instance directory is not where the host
  looks. See `cat ~/.config/tehas/instance-dir` and [The host](/host).
- `the default branch of ... is unknown`: the product checkout has no
  `origin/HEAD`. Run `./install.sh`, which sets it, or set `TEHAS_BRANCH` in
  `instance.env`.
- A setting you exported in the shell is ignored: `instance.env` wins, and a
  setting it does not assign is dropped.

The dispatcher fails the same way under systemd; the line is then in
`~/tehas/logs/dispatch.log`.

## Nothing happens after I label an issue

Run `tehas dispatch --dry-run` and read what it prints.

- `paused: issue #N carries tehas:pause`: the control issue holds the pause
  label. Only an account named in `TEHAS_ALLOWED_LABELERS` can lift it: apply
  the label and take it off again, or run `tehas resume` on the host.
- `paused: the flag tehas pause wrote at <time> holds this host`: this host is
  paused by `tehas pause`. `tehas resume` lifts it.
- `paused: tehas:pause was taken off #N by 'x', who may not authorise work here`:
  an account outside `TEHAS_ALLOWED_LABELERS` removed the label, and removing a
  pause is work an allowed account authorises. An allowed account applies
  `tehas:pause` and takes it off again.
- `paused: tehas:pause is not on #N, but its last tehas:pause event only applied
  it: nothing has taken the pause off`: the label left the control issue without
  any account taking it off, which is what deleting or renaming the label itself
  looks like. No account was asked to lift the pause, so the factory stays
  paused. An allowed account applies `tehas:pause` and takes it off again, and
  `tehas labels` puts a deleted label back.
- `WARN: ~/tehas/state/pause.json is not a pause flag: taking it as paused`: the
  flag file is damaged, and a switch that cannot be read is taken as closed.
  `tehas resume` removes it; `tehas pause` writes one that can be read.
- `holding: N cycles are running (limit N)`: the ceiling is reached. Wait for
  a cycle to end, or raise `TEHAS_TICKET_CONCURRENCY`.
- `holding: tehas.slice has no room for another cycle (X GiB of Y GiB promised,
  each cycle is given R)`: the containers in the slice are promised, or use,
  so much that one more reserve does not fit. Every container counts: cycles,
  dev sessions, planning turns, the indexer. Wait for one to end. If one
  container holds far more than its reserve, that is the one to look at:
  `systemd-cgtop /tehas.slice` shows them. A lower `TEHAS_CYCLE_RESERVE` fits
  more cycles and promises each one less.
- `holding: tehas.slice holds as many containers as their ceilings fit (4 of
  Nm)`: four containers are in the slice, each may grow to a quarter of it.
  Every container counts: cycles, dev sessions, planning turns, the indexer.
  Wait for one to end.
- `holding: tehas.slice is under memory pressure (full avg60 N)`: every
  process in the slice waited for memory for more than a tenth of the last
  minute, and more than half of the slice is promised. Cycles start again
  when it falls to 10 or under, or when half or less is promised.
- `holding: the slice's use could not be read: <file> ...`: a file under
  `/sys/fs/cgroup/tehas.slice` is missing or unreadable; the line names it
  and what was missing in it. See "The dispatcher holds on the slice" below.
- Any other `holding:` line that names the slice, Docker, `systemctl` or
  `TEHAS_CYCLE_RESERVE`: the slice cannot be used. See "The dispatcher holds
  on the slice" below.
- `holding: N issues await merge (limit N)`: review and merge, or raise
  `TEHAS_MAX_AWAITING`.
- `holding: N issues await merge and M cycles are running (limit N)`: running
  cycles count towards `TEHAS_MAX_AWAITING` with the issues that await merge.
  Wait, or raise the limit.
- `skip #N: tehas:ready was applied by ...`, then `clear #N: tehas:ready off,
  applied by ...`: the label came from an account that is not allowed. The
  dispatcher took it off and said so on the issue. To ask for a cycle, an
  allowed account applies `tehas:ready`. The same two lines name `tehas:plan`
  when that label came from such an account. With only the `skip` line, the poll was
  a dry run or the label change failed (`WARN: label change on #N failed`), and
  the next poll tries again.
- `skip #N: pull request <url> is open and closes it`: the work is in that
  pull request already. The dispatcher took `tehas:ready` off and said so on
  the issue. Merge or close the pull request, then apply `tehas:ready` if work
  is still needed.
- `ignore #N: tehas:urgent was applied by ...`: the label came from an account
  that is not allowed, so it moves nothing: the issue keeps its place by
  number. It is said at every poll until someone removes the label.
- `holding: <n> of <free> free slots start: ...`: some cycles started, fewer
  than there are free slots and issues waiting. The rest of the line is the
  one limit that cut it: issues awaiting merge (`TEHAS_MAX_AWAITING`), or the
  slice's room. It is said on every such poll. See
  [Limits that hold work back](/operating#limits-that-hold-work-back).
- `holding: main is red at <sha>`: the check `TEHAS_BRANCH_CHECK` names
  failed on the head commit of the branch. No cycle, retry or refresh
  starts; running cycles and planning turns go on. Fix the branch: once the
  check passes on a new head, or is run again and passes, the next poll
  starts cycles and logs `released: cycles may start again`.
- `the check <name> on main could not be read: cycles start`, `main has no
  check named <name> on its head commit: cycles start` or `the check <name>
  on main at <sha> has not finished: cycles start`: the poll could not tell
  whether the branch is red, so it did not hold. Said once each time it
  changes. A name that is never found is a wrong `TEHAS_BRANCH_CHECK`, or a
  product repository that runs no such check.
- `stop #N: its issue is closed` or `stop #N: its pull request <url> is
  merged`: the cycle's work was done, so the poll stopped it and gave its slot
  to the next issue. Nothing more is needed.
- `holding: no resources: the model endpoint ...`: the model endpoint does not
  answer, offers no models, or does not offer the configured model, and
  neither does its backup where `models.json` names one. Nothing
  starts, no cycle, no planning turn and no retry. The issues stay queued and
  are taken as soon as the endpoint answers. See "The model endpoint does not
  answer" below.
- `hold #N: its text links an attachment and TEHAS_WORK_IMAGES is 1, but
  <model> does not declare image input in models.json`: the instance gives a
  cycle images and its `models.json` does not list `"image"` in the model's
  `input`. Only issues that link an attachment wait; the rest start. Add
  `"image"`, or set `TEHAS_WORK_IMAGES=0`, and run `install.sh`.
- `hold #N: ... but <model> does not answer an image`: the model endpoint
  refused a request with a one-pixel image, or took more than 30 seconds. The
  model runs without its vision part, or the backend is busy. The request is
  sent only for an issue that links an attachment and could start now, at
  most once a poll.
- `hold #N: ... but the pi-rukas in the sandbox image <commit> does not read
  them`: the sandbox image says, by the label its build wrote, that its
  pi-rukas has no reader for the manifest tehas writes beside the images.
  Only issues that link an attachment wait, and the model is not asked.
  Run `tehas update`, which rebuilds onto upstream's newest release, which has
  the reader, or set `TEHAS_WORK_IMAGES=0` and run `install.sh`. An image put
  back from before 2026-10-14 is what starts the hold: see
  [Rolling back pi-rukas](/host#rolling-back-pi-rukas).
- `hold #N: its text says <the words> and links no attachment, so the cycle
  would be given none; attach the file or write out what it shows, before
  applying tehas:ready`: the issue was written around a file nobody attached.
  Its text says `SCREENSHOT GOES HERE`, `IMAGE_GOES_HERE`, `the image goes
  here` or the like, and no text of the issue links a file at all. The
  dispatcher started no container: the cycle would have been given no image and
  no word of the gap, and would have parked after a full explore with the park
  looking like a problem with the issue's premise. Nothing on the issue changed
  and no label was touched, so the queue is as it was. Attach the file, or write
  out in words what it shows and remove the line, and the next poll starts the
  cycle. `TEHAS_WORK_IMAGES` makes no difference: with the setting off the
  instance gives a cycle no attachments at all, which is the same gap. The
  words of prose are no reason to hold: "the value goes here in the code" starts
  as any other issue does, because the name of a file is what stands before
  `goes here`. See [Attachments](/operating#attachments).
- `nothing to do`: the issue is closed, or does not carry `tehas:ready`.
- `wait #N: its cycle is still running`: `tehas:ready` was applied while the
  issue's cycle runs. A fresh cycle starts when that one has ended.
- `would park #N: it is marked tehas:running and no cycle holds its lock`
  (`park #N` in the log of a real poll): the cycle of that issue ended without
  a report, or never started. The next real poll gives the issue
  `needs-human-attention`. Read `~/tehas/logs/dispatch-work-N.log`.
- `WARN: the cycle for #N holds its lock but has not been heard from since
  <time>, more than 1200s: stopping tehas-cycle@N.service`: the cycle's live
  file has not been written for 20 minutes, so the poll asked its unit to
  stop. Read `~/tehas/logs/dispatch-work-N.log` and the end of the cycle's
  `~/tehas/logs/work-N-*.jsonl` for where it hung. `WARN: could not stop
  tehas-cycle@N.service (exit <code>)` after it means the stop did not go
  through: the poll asks again, at most once every 10 minutes, and after 6
  asks it says the episode needs a person. Either way the unit's own
  `TimeoutStopSec` (300s) is what ends a unit that ignores the stop job. To
  stop it now: `tehas stop N`, which clears what the cycle left on the host as
  well, or `systemctl --user stop tehas-cycle@N.service` on its own, and read
  `journalctl --user -u tehas-cycle@N`.
- `N cycles running, room for M more`, then `would run a cycle for #N ...`:
  the issues the next poll starts. In the log you may also see `the code index
  stays as it is: a cycle is running on #N`. That is no reason an issue waits.
- `ERROR: systemd cannot load tehas-cycle@.service (LoadState: ...)`: the poll
  cannot start units, so it changed no label. Run `install.sh` on the host so
  its units step installs the cycle unit. Run `tehas dispatch` in the tehas
  user's own session, not in a `sudo -iu` shell, which has no user bus.
- `another poll holds the dispatcher's lock`: a poll is under way. Try again
  in a minute. A running cycle, a planning turn and an index run do not hold
  this lock.
- `holding: the code index is refreshed first, as tehas-index.service`, then
  `holding: the cycles wait for the code index`: no cycle was running and the
  branch had moved, so the index is rebuilt before cycles start. It takes
  seconds to minutes; its end starts the poll that starts them. See
  `journalctl --user -u tehas-index` and `~/tehas/logs/dispatch-index.log`.
- `WARN: the dispatcher's lock has been held since <time>, more than Ns: a
  poll hangs, and no poll runs until it ends`: logged once by the first poll
  that finds the lock held longer than half an hour. No sweep, start or
  planning turn runs meanwhile. Look for the hung process with
  `systemctl --user status tehas-dispatch.service`; the unit stops a poll
  after an hour by itself. `~/tehas/state/dispatch.taken`
  holds when the lock was taken, in seconds since 1970.
- `ERROR: gh ... failed`: a GitHub call did not answer, or the control issue
  does not exist. The poll ends there on purpose: the pause switch, the limit
  and the labeller check are never decided on an answer that did not come.
- `ERROR: <repo> is now <other> on GitHub`: the repository was renamed or
  moved. Until it is put right the factory floor shows an empty board. Set
  `TEHAS_REPO` to the new name, and `TEHAS_IMPROVE_REPO` too if it named the
  same repository. Point the product checkout's `origin` at it, move the
  checkout if its directory was named after the old one, and run
  `install.sh`. When the product is tehas itself, point the platform
  checkout's `origin` at it as well, or `tehas update` pulls from the old
  name. Previews follow the product checkout on their next run. The
  dispatcher checks this at every poll because GitHub answers a list call
  under the old name with an empty list, not an error.
- `tehas: ... is a checkout of ..., not of ...` or `no product checkout at ...`:
  the product checkout is missing or is a clone of another repository than
  `TEHAS_REPO`. Nothing is polled until that is put right.

### A cycle stops with `Cannot fork` in its log

The cycle's progress log has `/bin/sh: 1: Cannot fork`, or a transcript has
`fork: retry: Resource temporarily unavailable` or `EAGAIN`. The container was
at its `--pids-limit`. The comment that reports the cycle says how many starts
were refused, and the kernel log on the host has `fork rejected by pids
controller` with the container's scope. Nothing the cycle ran after the first
refusal can be trusted, its gate included, and the handoff is often not posted.

Before v0.7.1 a cycle's container had no init, and every pass of the shell
tests left several hundred finished processes that were never collected. With
the init a pass peaks at under twenty. A cycle that still reaches the limit is
starting processes that stay alive: look at what is running in the container
(`docker top tehas-work-<issue>`) before raising the limit.

### A cycle stops in its first step with `provider-severed`

The cycle's progress log (`~/tehas/logs/work-<issue>-*.jsonl`) shows retries
with `cause=provider-severed` and an agent's transcript an `errorMessage` starting with
`429`. The model endpoint listed the model but had no free slot for it. That
happens when another job is using the model, and also when the model has only
one slot: a cycle's driving session and its step agents call the model at the
same time, so a cycle needs at least two conversations served at once. The
dispatcher's check before a cycle only asks whether the model is listed, so it
cannot see either. Ask the endpoint directly, twice at the same moment:

```
curl -s http://<model host>/v1/chat/completions -H 'content-type: application/json' \
  -d '{"model":"<model>","messages":[{"role":"user","content":"ready?"}],"max_tokens":5}'
```

When several cycles run, their agents all ask for the same model unless roles
have their own. The six lens reviewers of one cycle alone call it at once.
Give the busiest roles a model on other machines: see
[A model per role](/host#a-model-per-role).

When it answers, apply `tehas:ready` to the issue again. Set `PI_ENSEMBLE_DEBUG=1` in `instance.env` to get the
cause into the error stream in the first place.

If the dry run says it would run but nothing starts within a few minutes, the
timer is not polling:

```
systemctl --user list-timers tehas-dispatch.timer
tail ~/tehas/logs/dispatch.log
```

`systemctl --user enable --now tehas-dispatch.timer` turns it on;
`systemctl --user disable --now tehas-dispatch.timer` turns polling off without
touching a running cycle.

## The dispatcher holds on the slice

`tehas status` says `warn` and its `slice` line starts with `holding:`. No
cycle and no planning turn starts, no label changes and nothing is said on an
issue. The queue is kept and is taken up when the hold ends. The line is in
`~/tehas/logs/dispatch.log` once, not at every poll.

| The line says | What to do |
| --- | --- |
| `no slice file on this host (sudo ./bootstrap.sh slice)` | Root has not installed the slice. Run that command once. See [Bootstrap a host](/bootstrap#slice). |
| `tehas.slice has no memory limit (sudo ./bootstrap.sh slice)` | The unit systemd has loaded is not the installed file. The same command reloads it. |
| `Docker runs containers with the ... cgroup driver on cgroup v..., tehas needs systemd on v2` | Set `"exec-opts": ["native.cgroupdriver=systemd"]` in `/etc/docker/daemon.json` and restart Docker. A host on cgroup v1 cannot run tehas. |
| `systemctl gave no answer`, `docker gave no answer` | The command failed or was silent for ten seconds. Run `systemctl show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax` and `docker info` by hand. |
| `... cgroup.controllers does not list cpu, memory and pids` | The host's cgroup tree lacks a controller. Read `/sys/fs/cgroup/cgroup.controllers`. |
| `TEHAS_CYCLE_RESERVE=... is more than a container may use here` | The reserve is above a quarter of the slice's limit. Set a lower `TEHAS_CYCLE_RESERVE` in `instance.env`. The line names the host size the reserve needs. |
| `the slice's use could not be read: ...` | The rest of the line names the file and what was missing: `.../memory.pressure cannot be read` or `has no full avg60`, `.../memory.stat has no number for anon` (or `shmem`, `slab_reclaimable`), `.../memory.low cannot be read` or `is not a number`. Read that file as the tehas user. A kernel booted with `psi=0` has no `memory.pressure`: remove that option from the kernel command line. A kernel too old for the `kernel` line in `memory.stat` is read without it: kernel memory is then not counted. If the directory is missing while systemd says the slice is active, look at `systemctl status tehas.slice`. A slice that is not active has no directory, and its cycles that have no container yet count at one reserve each. |

The `slice` line reads `unknown: no poll has reported in 10 min` when no poll
got as far as reading the slice in that time. That is so while the factory
is paused, while a planning turn takes longer than that, and when the timer
is not running. `tehas dispatch --dry-run` reads the slice afresh and prints
the same hold.

A hold, and its end, reach `tehas status` in the poll that finds them. The
numbers of a `room for N cycles` line can be one poll older than the age the
line gives for the health file: the health file is written at the start of a
poll, after a start, and when the reason for a hold changes.

## The cycle parked (`needs-human-attention`)

Read the bot's comment on the issue first: it names the cap that fired, and
pi-rukas's handoff follows in the same comment.
The common ones:

| Reason                              | What it means                                       | What to do                                               |
| ----------------------------------- | --------------------------------------------------- | -------------------------------------------------------- |
| underspecified, needs clarification | The issue does not say enough                       | Add acceptance criteria, relabel                         |
| `explore-bodies-empty`              | `gh` could not read the issue                       | Check the bot token: `gh auth status`                    |
| verify or smoke failure             | The gates failed on the developer's change          | Read the gate output in the handoff; often a real defect |
| protected paths                     | The patch touched `.pi/`, `.github/`, `CODEOWNERS` or `agents.json` | By design. Make that change yourself                     |
| review round cap                    | The review kept finding problems                    | Read the review on the pull request                      |
| developer timeout                   | An agent ran out of time                            | Usually model endpoint capacity; retry later             |
| `wall-clock`, `stall`               | tehas ended the cycle: its time limit `TEHAS_WORK_TIMEOUT`, or no log moved for `TEHAS_WORK_STALL` | Read the run log the comment names; see [a cycle stops at a cap](#a-cycle-stops-at-a-cap) |

The caps this factory has posted are explained one by one in the next
section.

An edit to `AGENTS.md` or `CLAUDE.md` stops nothing here, and its absence from
the row above is not an oversight: pi-rukas does not halt a cycle for a policy
edit. The edit ships and you see it in the diff, and the cycle is judged
against the doctrine at the commit it started from, so an edit of its own
changes nothing for that cycle. See [Operating tehas](/operating).


## A cycle stops at a cap

pi-rukas's handoff, in the bot's comment, has the heading "Cap hit", and its
line `**Cap**:` names the cap. Under "What this cap means" are pi-rukas's own
words for this stop, and under "Worktree state at handoff" whether the work is
on a branch and whether that branch was pushed.

Read the rest of the comment with three things in mind:

- Its paths are the ones inside the cycle's container. The workspace that is
  `~/tehas/work/tehas` there is `~/tehas/work/tehas-<issue>` on the host.
- Its "Concrete recovery commands" are written for pi-rukas run by hand. On
  the host a retry does what they do: tick `Retry from a clean start` in the
  bot's comment, or apply `tehas:ready`. Both start a fresh cycle from the
  first step, in a new workspace. Commits that were never pushed are kept in
  `tehas-<issue>.before-retry` until the next retry. See step 5 of
  [Operating](/operating).
- The workspace was written by the cycle's container, its git settings and
  hooks included. tehas never runs git in it on the host: git there runs what
  the cycle left in those settings. Read its files instead.

A retry approves the issue text as it is then, so change the issue first when
the cap asks for it. A cap posted on a pull request came from the review
stage, after the pull request was opened. When that pull request is still open
at the end of the cycle, the issue gets `needs-human-attention`, and the
comment names the cap and the pull request. Only the cap `awaiting-human-merge`
gives `tehas:awaiting-merge`, and only when the review did not run out of
rounds with findings open.

The cap `deadline` means the cycle came to the deadline tehas gives it, a
quarter of `TEHAS_WORK_TIMEOUT` before the end, and started no new step. Its
branch and pull request are as the handoff says. Retry when the issue can be
done in the time, or split it.

Two caps are tehas's own, not pi-rukas's: `wall-clock` when the cycle reached
`TEHAS_WORK_TIMEOUT`, whether the runner stopped itself or tehas stopped it,
and `stall` when none of the cycle's logs changed for `TEHAS_WORK_STALL`
seconds and tehas stopped it. See [`wall-clock`](#wall-clock) below. A stop
tehas made takes the place of a cap the runner named for the step it was
stopped in, and the record's `capReason` says so: `tehas work stopped the
container at the time limit of 21600s, step develop in flight; the runner had
named the cap verify-failed:develop`. Only a cap of an end the runner reached
before the stop, such as a handoff, is kept. Both say the step the cycle was
in, from the runner's progress where its result names none. A cycle stopped this way has usually done less
than its comment claims; read the run log named there before you retry, and
retry when the cause of the stall is gone.

These are the caps the factory posted on its own issues from 10 to 14 October
2026, most frequent first.

### `verify-failed:develop`

After the develop step, the driver ran the checks itself, and they did not
back the developer's claim that the work is done. The lines under "What this
cap means" say which check. Four kinds were seen:

- `touched out-of-scope path <file> - declared fence violated
  (issue-fenced)`. The plan marked that file as out of scope for the work,
  and the developer changed it anyway. On this factory the files were tests
  (`test/*_test.sh`), manual pages (`host/docs/*.md`), the front page's style
  sheets (`host/fun/*.css`), and in one cycle scripts as well. Tests and
  manual pages are what the repository's rules require a change to touch, so
  the plan's fence and the rules disagree
  (tehas-app/tehas#159, trail-openers/pi-rukas#1250). Name in the issue every file
  the change will touch, the test file and the manual page included, as
  step 1 of [Operating](/operating) says.
- `verify command ... failed on the CONSOLIDATED tree`, or `exceeded its
  10-min timeout`. The product's gate failed on the change, or ran past its
  time. The gate's output follows in the comment. Often a real defect. If the
  failure is in code the cycle did not touch, see "The verify gate fails on
  code the cycle did not touch" below. When the failing line is a shellcheck
  note, the severity the lint runs at and the one suppression it accepts are
  under `scripts/test.sh` in [Tools](/tools).
- The fix pass after the adversarial review did not clear: it changed
  nothing, or it removed lines it should not have.
- `consolidated verify was refused  repoRoot is dirty (...)`. Not a gate
  failure: the driver would not run the consolidated gate at all because it
  found files in the checkout that git does not ignore. When the names are
  under `target/` they are cargo's own build output and lock files, left by a
  gate that built Rust with the repository as its workspace root, sometimes by
  the driver's own earlier pass. The repository's `.gitignore` now ignores
  `/target/` at the root, so these no longer count. Check the names in the cap
  before you delete anything: dirt the gate did not make may be work. When the
  driver wrote no reason, the cycle's `err.txt` says so in one line naming the
  cap. (#248, #285.)

The work is committed on a local branch in the workspace and was not pushed.
There is no pull request. Change the issue where the cause is in it, then
retry.

### `intent-park`

The cycle stopped before the plan. Having read the issue and the code, the
driver did not settle on what to build. No branch was made and nothing
was written. The first sentence of "What this cap means" says which verdict,
and "Resolver's rationale" says why.

| The comment says | Verdict | What to change |
| --- | --- | --- |
| "does not say enough to build from" | underspecified | Add a `## Acceptance criteria` section, or say concretely what should change |
| "central claim is contradicted by the code as it actually is" | contradicted-by-code | What the issue says about the code is not what the code does now. The issue may be stale, or describe a bug already fixed. Read the evidence in the comment, then correct the issue or close it |
| "rests on a premise the driver could not substantiate" | premise-unsound | Name where the API or behaviour the issue relies on is, or correct the premise |
| "appears to be already implemented" | already-implemented | Close the issue, or say what is still missing |
| "is too large to execute as one cycle" | too-large | Split it into several issues |

The last one is in pi-rukas and was not seen here. After the change, retry.

### `wall-clock`

The cycle reached the host's own time limit, `TEHAS_WORK_TIMEOUT` (6 hours by
default), and nothing else stopped it first. This cap is tehas's, not
pi-rukas's: the runner stopped itself at the limit with no cap of its own, or
it did not stop and tehas stopped its container 300 seconds later. The status
is `timeout`, and the comment says which of the two happened and the step the
cycle was in. That step was cut off in the middle, so what it wrote may be
half done. The deadline before the limit should have parked the cycle at a
step boundary with the cap `deadline`; a cycle that reaches `wall-clock` was in
one step that outlasted the margin. Read the run log the comment names for
what that step was doing. Raise `TEHAS_WORK_TIMEOUT` only when the work was
progressing and needs more time; when the step was stuck, find out why, or
split the issue.

Until result schema 4 (pi-rukas, 18 October 2026) pi-rukas used the name
`wall-clock` for its review loop's budget. An older comment on a pull request
that names `wall-clock` is the cap below.

### `lens-review-timeout`

The review stage's rounds of review and fix ran past pi-rukas's budget for
them, `PI_ENSEMBLE_REVIEW_WALL_CLOCK_MS` (90 minutes by default). It is the
review loop's own clock, not the cycle's time limit: raising
`TEHAS_WORK_TIMEOUT` does not change it. A `PI_ENSEMBLE_REVIEW_WALL_CLOCK_MS`
line in `instance.env` does. The record's `capReason` says how long the loop
ran, in which round, and the budget.

The comment is on the pull request. The pull request is open, with what was
pushed before. The comment names the worktrees in the workspace that hold the
cycle's commits; what was not pushed is only there. Read the review on the
pull request, then merge it, finish it by hand, or close it. Nothing in the issue needs to change. Close
the pull request before you retry: while it is open, a new cycle stops at
`existing-pr-detected`.

### `child-timeout:<step>` and `spawn-backstop`

`child-timeout:develop` or `child-timeout:explore`: one child of that step ran
past its own run bound. pi-rukas told it to wrap up at the bound and ended it
10 minutes later. Read the run log for what the child was doing. When it was
progressing, split the issue; when it was stuck, find out why.

`spawn-backstop`: a child ran past pi-rukas's 2-hour backstop,
`PI_ENSEMBLE_SPAWN_TIMEOUT_MS`, which only catches a child nothing else ended.
Until result schema 5 (pi-rukas v0.14.0) this cap was named
`developer-timeout`.

### `step-failed:adversarial`

The adversarial review could not run: its dispatch failed twice. It says
nothing about the code. All three times it was seen here, on 14 October, the
model endpoint was timing out (tehas-app/tehas#147,
trail-openers/pi-rukas#1243). The work is committed on a local branch in the
workspace and was not pushed. There is no pull request. Check the endpoint,
as in "The model endpoint does not answer" below, and retry once it answers.
The issue needs no change. The retry starts from the first step: the work
already done is not reused.

### `develop-incomplete-deliverables`

The gate passed, but the change lacks a part the plan declared. The comment
names the missing parts by id and path, for example `d5
(host/docs/dashboard.frag)`. pi-rukas has already tried once to add them.
The work is committed on a local branch in the workspace and was not pushed.
There is no pull request. Retry: a new cycle gets a new attempt at the
missing parts. If a missing part is one the issue does not want, say so in
the issue first.

### `lens-fix-not-integrated`

In the review stage, a round of fixes was made in a worktree and could not be
committed onto the branch. Both times it was seen, git said `Committing is
not possible because you have unmerged files`. The comment is on the pull
request. The pull request is open with the code from before that round; the
fixes are only in the workspace. Read the review's findings on the pull
request, then merge it, fix it by hand, or close it. Close it before you
retry, as for `lens-review-timeout`.

### `lens-fix-reposition`

In the review stage, the next round of fixes could not fetch the branch from
GitHub (`couldn't find remote ref`). Both times it was seen, the pull request
had been merged while the review ran, and its branch was gone. Then nothing
is left to do. If the pull request is not merged, look for why the branch is
not on GitHub.

### `integration-worktree-violation`

```
integrate worktree could not be created: refusing to force-remove existing
worktree .../.worktrees/issue-<N>-<name> - it holds unrecoverable work
```

This message is the second failure, not the first. pi-rukas commits the cycle's
work to the feature branch and pushes it. When the push (or opening the pull
request) fails, it falls back to an agent working in a fresh "integrate"
worktree, and creating that worktree is refused because the cycle's own
worktrees hold commits. So look for why the push failed.

The first two real cycles died this way: git in the sandbox had no credential
helper and every push ended in
`could not read Username for
'https://github.com'`. `tehas work` now passes the
helper and the bot's identity, and checks with a dry-run push before starting.

Check the push by hand:

```
tehas work <issue>      # exits 3 at once, with git's message, if it cannot push
```

The work of a parked cycle is not lost. Its commits are in the cycle's own
workspace, `~/tehas/work/<product>-<N>/.worktrees/issue-<N>-*`, and the
driver's combined commit is on the local branch `feature/issue-<N>-...` there.
A retry moves that workspace to `<product>-<N>.before-retry`.
Once the push works, retry.

### `existing-pr-detected`

Before any work, pi-rukas found a pull request already open for the issue:
its branch names the issue, or its description closes it (`Fixes #<N>`).
Nothing was written. Here it was once a pull request of an earlier cycle and
once a person's own. Merge that pull request, or close it and retry. A retry
while it is open stops the same way.

### `review-incomplete`

In the review stage, at least one of the six review passes failed every
retry, so the review did not finish. That is not a rejection. The comment is
on the pull request, which is open with the work pushed. Review the change
yourself, then merge or close it.

### The runner died and wrote no result

The bot's comment says `Tehas cycle handed off with its pull request open and
its review unfinished`, or `Tehas cycle stopped without a pull request`, with
`last step` followed by `read from its run log` and a `runner exit` that is not
one of the ends of the contract (10, 11, 20, 21). The runner of that cycle died
before it could write its result, so tehas read the step it was in from its run
log and asked GitHub for the bot's pull request of the branch that
`commit-pr` pushed.

- The comment names the pull request when GitHub lists one. Read it: the change
  is up for review, and a clean retry redoes what it already holds. That
  comment has no retry boxes, and the issue carries `needs-human-attention`.
- The comment says no pull request was found when GitHub lists none. The
  change is then either unpushed or pushed without a pull request. The
  commits are in the cycle's workspace, on the local branch
  `feature/issue-<N>-...` there: `git -C ~/tehas/work/<product>-<N> log
  feature/issue-<N>-...`, then push it and open the pull request by
  hand, or retry for a fresh cycle.
- The step is `-` when the run log has no step for this cycle at all, which is
  what says `tehas work` stopped before it started the container. Read
  `~/tehas/logs/dispatch-work-<N>.log`. A run that stopped for want of a gate is
  of that kind: the log has the line naming `.pi/verify-cmd`, and the issue has
  no comment, because a cycle that never began has nothing to hand off. See
  [A new project](/new-project) for what the product has to hold.

```
jq . ~/tehas/state/cycles/<N>.result.json      # empty, or missing: no result
tail -n 40 ~/tehas/logs/work-<N>-*.jsonl        # the steps the runner reached
```

The log of the run says what the runner was doing; the comment says what tehas
made of it, and the two are read in that order.

## A provider error about context length

```
This model's maximum context length is 16384 tokens
```

A child agent fell back to the first model in `models.json`, a small one.
`PI_ENSEMBLE_SUBAGENT_MODEL` and `PI_ENSEMBLE_SUBAGENT_PROVIDER` did not reach
it. Check them in the instance's `instance.env` and run `./install.sh --check`
in `~/tehas/platform`. Listing the intended model first in `models.json`
removes the trap.

## The model endpoint does not answer

```
curl -s -m 10 "$(jq -r '.providers.fleet.baseUrl' ~/.pi/agent/models.json)/models" | jq -r '.data[].id'
```

`fleet` is the provider's name in the example. An endpoint that takes a token
answers 401 without one: add `-H "Authorization: Bearer $TEHAS_MODEL_KEY_MAIN"`
from a login shell. For the backup, ask `.providers.fleet.backupBaseUrl` the
same way. The dispatcher makes this call
at every poll. While the call fails, or the answer lists no models or not the
configured one, it starts nothing: `tehas status` says `warn: No resources`,
the dashboard says `no resources`, and the dispatcher's log has one line that
starts with `holding: no resources:`. It logs `released` at the first poll
that finds the model again. A cycle that was already running when the endpoint
went away is not stopped: it stalls and times out.

If the call works on the host but agents cannot reach the endpoint, its host
name does not resolve inside the sandbox: add it to `PI_ENSEMBLE_HOST_ALIASES`.

## The state file says `running` but nothing is running

A cycle that died leaves `status: running` behind. Confirm with
`docker ps --filter name=tehas-work`, then start clean:

```
tehas work <issue> --restart
```

The new run starts in a fresh workspace. The old one, its state file
included, is moved to `~/tehas/work/<product>-<issue>.before-retry`, and the
next run replaces that. Branches pushed to GitHub are kept.

## The verify gate fails on code the cycle did not touch

The base is broken. Confirm on a clean checkout of the default branch, with
the product's own full gate (the first line of `.pi/verify-cmd-full`). For the
example product:

```
scripts/verify.sh --full
```

Fix the default branch first; every cycle will fail the same way until then.

When the gate stops on a shellcheck note, the severity floor the lint runs at
and the one suppression it accepts (`SC2016`, for the line whose single quotes
are the point) are under `scripts/test.sh` in [Tools](/tools).

## The improvement loop files nothing

Read the last lines of `~/tehas/logs/improve.log`.

| The line says | What it means |
| --- | --- |
| Nothing new for hours | The timer is not enabled: `systemctl --user list-timers tehas-improve.timer` |
| `TEHAS_IMPROVE_REPO is not set` | The loop is off for this instance |
| `cycles=0, nothing to read` | No cycle has ended since the last run. A cycle that stopped before Pi started leaves no record |
| `waiting: 10 open issues carry tehas:improvement` | Every repository the loop files in is at the limit. Close some or build some, or raise `TEHAS_IMPROVE_MAX_OPEN` |
| `findings=0`, or `duplicates` equal to `findings` | The model found nothing new |
| `invalid=` above 0 | Findings were dropped for not being the shape asked for |
| `failed: GitHub did not give the issue stack` | The token or the network. The records wait for the next run |
| `stopped=124 returned=2` at the end of a line that filed | The analyst hit its bound of 60 minutes, and what its file held was filed. The records it did not name went back to the window, and the next run takes half as many as this one. What it printed is in the file `output=` names |
| `failed: the analyst ended with 124 at its bound of 60 minutes` | The model took too long for the records it was given and left no file that passes the check; the line says which. The next run takes half as many, as the line says. What it printed is in the file the line names. The records wait |
| `failed: the analyst ended with ...` | Pi or the model. What it printed is in the file the line names, `~/tehas/logs/improve-analyst-<stamp>.log`. The records wait |
| `failed: findings.json is not the shape asked for`, or `left no findings.json` | The model did not write the file it was asked for. Its output is in the same file. The records wait |
| `failed: GitHub refused an issue` | The bot account cannot write to that repository. Issues filed before the refusal are named; the records wait, so a later run may file the same finding again under another title |

A run that failed kept its records, and every later run reads them again until
one succeeds or they are seven days old. A run reads at most five, the oldest
first, and fewer after one was stopped at its bound; `left=` on a line says how
many wait.

## An improvement issue says a log was not uploaded

The issue names the reason, and `improve.log` has it at the end of that run's
line.

- `secret-shape=<cycle>`: the log holds something shaped like a token, so it
  was kept on the host. Read `~/tehas/logs/work-<cycle>.jsonl` there, and find
  out how a secret got into a log.
- `not-uploaded=<cycle>`: GitHub refused the release or the upload. The bot
  account needs write access to the repository.
- `not-packed=<cycle>`: one line of the log is over 5 MB compressed.
- "the log is no longer on the host": it was removed before the run, by hand
  or by the seven day sweep.

## install.sh --check says `Follow-ups: NOT DONE`

A follow-up that was asked for has not run. The lines are in
`~/tehas/state/update-pending`: what `install.sh` recorded as it copied files, and
what `tehas update` has not finished. Run `tehas update`: it runs the steps in the
order `lib/update-steps.sh` lists them and clears the record. If it names a step
line in a `WARN:` instead, the checkout asks for a step this installed
`tehas update` does not know. Pull and run it again, since `install.sh` installs
the commands before the follow-ups run.

The lines are step lines, not shell text: `docs` on its own is not one, because
that renewal needs its `--force-recreate` flag. A line `lib/update-steps.sh` does
not accept is named in a `WARN:`, is not run, and stays in the record.

A record this account cannot open, written by another account or with a mode
that blocks reading, stops the update with exit 1 rather than read as "nothing
owed": it holds steps, and writing over it would lose them. Make it readable
again, or delete it once you have run those steps yourself.

## `tehas update` stopped partway

Each step prints one line, and the update stops at the first that fails, with
exit 1. What ran before it stays done: files `install.sh` copied stay
installed, each replaced file kept as `<name>.prev`. The steps not yet run stay
in `~/tehas/state/update-pending`, and `install.sh --check` reports them as
`Follow-ups: NOT DONE`. Fix what the failed line names and run `tehas update`
again. It runs the owed steps even when the checkout has not moved.

A line that starts `settings:` and exit 78 right after the fast-forward mean
the new code asks for a setting the instance does not have. The checkout is
at the new code and nothing was installed. Add the setting to `instance.env`
and run `tehas update` again.

## `install.sh` printed a `later:` line

A `later:` line is for a person. `tehas update` passes it through and does not
run it, and it is not recorded as owed. Run the command it names yourself:

- `later: <checkout>/bootstrap.sh user`: `versions.env` changed, and the tools
  on the host follow the pins only when this runs, as the tehas user. Until
  then `install.sh --check` says `Dependencies: NOT OK`.
- `later: ~/tehas/bin/tehas index`: the code index was not built. It needs the
  sandbox image and the product checkout.
- `later: sudo <checkout>/bootstrap.sh dev <user>`: root's copy of the command
  behind `tehas dev` for other accounts is not this checkout's. Only root can
  install it.
- `later: ./install.sh again ...`: old files were left in place. The line says
  when to run it.

## `scripts/tag-deploy.sh` fails with `~/tehas/env.sh: No such file or directory`

The script runs `tehas update` on the host over ssh, as the account ssh logs in
as. Without a user in the destination or in `~/.ssh/config`, that is your local
login name, not the tehas user. Put the host's user and key in `~/.ssh/config`:

```
Host tehas-tehas
  User boss
  IdentityFile ~/.ssh/<key>
```

Then run `scripts/tag-deploy.sh tehas-tehas` again. `TEHAS_SSH` changes the
ssh command itself.

## The edge router does not start

A host that ran the Protolab's server under its old name has a container of
the project `tehas-protolab` holding port 8081. `tehas update` takes it down
and starts the router in its place. When the old one came down and the router
did not start, the update says `edge: docker compose up exited <n>; the step
is not done`, and leaves `~/tehas/state/edge-takeover` holding the time it was
made. Fix what the lines above that message say and run `tehas update` again:
while that file is less than a day old it starts the router whether or not
anything runs, and says `edge: finishing the takeover`. Past a day the mark is
not acted on: the update says `edge: the mark of the takeover is <n> hours
old, and was left to you`, removes the file, and renews the router only where
it already runs, so a router you stopped on purpose stays stopped. Start it by
hand then: `docker compose -p tehas-protolab down`, then
`. ~/tehas/env.sh && docker compose -f ~/tehas/edge/router.compose.yaml up -d`.
A mark left by an older tehas has no time on it: it is honoured once, and gets
the time of that run.

The router also needs the network `tehas-previews`, which `install.sh` makes. If
compose says the network is not found, make it. `install.sh` does not make it
on a Docker older than 27.1, and says so; check `docker version` first, and
update Docker before making it by hand:

```
docker network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews
```

## A preview is not there

The dashboard lists the open pull requests that have a state under
"Previews", with the address, or with the state and the reason written into
the state file. The same is in
`~/tehas/preview/<pr>/state.json`, and what the runs did is in
`~/tehas/logs/preview.log`. Look for the state first.

**Nothing is listed at all.** The section is missing when the host has no
`~/tehas/live/previews.json`. Either `TEHAS_PREVIEWS` is not `1`, or the timer
was never enabled: `systemctl --user list-timers tehas-preview.timer`. An
update does not enable it. See [Previews](/edge). A pull request from a fork
is never listed and never built; the log counts it as `from elsewhere`. Nor
is one that someone who is not an owner, member or collaborator of the
repository opened, even between two of its own branches; the log counts it
as `opened by someone outside the repository`.

**`no-recipe`.** The pull request's commit has no `preview/Dockerfile`. Add
the file to the branch; the next commit is tried. See
[A new project](/new-project).

**`build-failed`.** Read `~/tehas/logs/preview-<pr>.log`. The reason in the
state says which of four things happened:

- The build failed or ran over 15 minutes. The same commit is not built
  again. Push a new commit and that one is tried. A preview of the earlier
  commit keeps serving meanwhile.
- `The recipe declares a volume outside /tmp, /config and /data. Its path is
  in logs/preview-<pr>.log on the host.` The build went through and the image was not
  run, and it was removed. That log line names the path: `The recipe declares
  a volume at <path>. A preview may write to /tmp, /config and /data only.`
  It is text of the recipe's own, so it stays on the host. A `VOLUME` line, in
  the recipe or in the image it
  starts from, names a directory other than those three, and Docker would
  put it on the host's disk. Take the line out, or start from an image
  without it, and push. `The image's volumes could not be read.` is the same
  outcome when Docker did not say what the image declares. Both are treated
  as a failed build: the same commit is not tried again, and a preview of
  the earlier commit keeps serving.
- The container did not start: Docker could not create or start it, such as
  an image whose command is not in it. That can also be the host and not the
  commit, so it is tried again at each of the next three runs, and then no
  more for that commit.
- The container started and stopped: five seconds after its start it was not
  running, or Docker had started it again already. It was removed, and the
  last 40 lines it wrote are at the end of the log, under `docker logs of
  tehas-preview-<pr>`. The usual causes are an image that writes outside
  `/tmp`, `/config` and `/data`, and one that does not start without a
  variable. It is tried again as the case above is: at each of the next three
  runs, then no more for that commit. Fix the recipe and push; the new commit
  is built.

For a container that did not start or that stopped, nothing serves for that
pull request, not the preview of the earlier commit either: the old container
is removed before the new one is started. The name answers 404 until a commit
starts. Only a failed build, or a recipe that declares a volume, leaves the
earlier preview serving.

To have the same commit built again, remove the state and let the next run
find the pull request anew: `rm -r ~/tehas/preview/<pr>`.

**`no-space`.** Less than `TEHAS_PREVIEW_MIN_FREE` is free on the file system
of `~/tehas` or on Docker's. Nothing is built, what serves stays, and `/health`
says `warn`.

```
df -h ~/tehas
docker system df
docker image prune      # removes images with no tag that nothing uses
```

A sandbox build removes date tags beyond its newest three itself.

The next run builds. tehas does not prune Docker's build cache;
`docker builder prune` does, and the next build of the sandbox image is then
slower.

**`over-limit`.** More than `TEHAS_PREVIEW_MAX` pull requests are open, and
the newest ones win. Close one, or raise the setting in `instance.env`.

**No new state, and the log says `the commit could not be fetched`.** A pull
request that had a state keeps it, and a new one has none. The bare
clone `~/tehas/preview/repo.git` could not fetch from the product's origin.
It is tried again at every run. The git message is in
`~/tehas/logs/preview-<pr>.log`. Check that `git -C ~/tehas/preview/repo.git
ls-remote origin` answers.

**The log says `the network tehas-previews does not keep the host out
(<reason>); nothing was built`.** A preview runs the pull request's own code,
so no run does anything on a network from which it could reach the host.
`install.sh` says the same as `The network tehas-previews does not keep the
host out (<reason>)`. The reason says what to do:

- `Docker <version> is older than 27.1`, or `Docker's version could not be
  read`: update Docker. A Docker before 27.1 does not know the option that
  keeps the host out. Docker 29.1.3 was seen to accept an option it does not
  know, so a network an older one made may look right and not be. That was
  not tried on an older Docker, and remaking the network there does not help
  either way. `install.sh` says this as `The network
  tehas-previews was not made or checked`. After the update, remake the
  network with the lines below, since the old Docker may have made it.
- `it could not be read`: `docker network inspect tehas-previews` failed.
  Check `docker info` as the tehas user.
- `it is not internal`, `its driver is not bridge`, `it has IPv6`, or `it is
  not isolated from the host`: the network was made by hand, or by an install
  from before these were checked. Without `--internal` a preview could reach
  the internet and the model host. With IPv6, or without the option, it could
  reach every service the host listens on at all addresses. Take the router
  and the previews off it, remove it and make it again:

```
docker compose -f ~/tehas/edge/router.compose.yaml down
docker ps -aq --filter label=tehas.preview | xargs -r docker rm -f -v
docker network rm tehas-previews
docker network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews
. ~/tehas/env.sh && docker compose -f ~/tehas/edge/router.compose.yaml up -d
```

The next run builds the previews again.

**The log says `Docker does not answer`.** Nothing was built or removed. The
list for the dashboard is still written, without a figure for the free space,
and the dashboard and `/health` say the free space could not be read.
Check `docker info` as the tehas user.

**The state is `serving` and the name answers 404** with "No preview is
running at this address". `serving` means the container was still running
five seconds after its start, not that it answers. The router did not reach
it.

```
docker ps -a --filter name=tehas-preview-<pr>
docker logs tehas-preview-<pr>
docker network inspect tehas-previews --format '{{range .Containers}}{{.Name}} {{end}}'
```

- The first line says `Restarting`, or an age of seconds that never grows:
  the process stopped later than those five seconds, and Docker starts it
  again each time, because a preview runs with `--restart unless-stopped`.
  The state stays `serving` and the pull request keeps its notice with the
  address while that goes on. The second line says why. Fix the recipe and
  push; the new commit is built. A process that stops within the five seconds
  is `build-failed`, above.
- The container is up: the image does not answer on port 8080, or the third
  line does not name both the router and the preview.

**502 from outside.** The proxy in front of the host got no answer from port
8081: the edge router is not running, or it is bound to another address than
the proxy sends to. See "The edge router does not start" above.

**521 or 525 from outside.** The proxy or its certificate. The table in
[Previews](/edge) says which.

**Nothing in the pull request's description.** The notice is written for
`serving`, `build-failed` and `no-space` only, once for each commit and state.
For `over-limit` and `no-recipe` a notice that is already there is changed to
say there is no preview, and none is made. What was said about a commit and
state is on record in `~/tehas/preview/<pr>/.notice` and is not said twice, but
the description is looked at again whenever GitHub gives a later time for the
pull request than the one on record, so a notice a person removed comes back at
the next run. The rules are in
[tehasbot preview](/tools). The log line says which case it is:

- `#<pr>: the description could not be read (no such pull request (HTTP 404)),
  tried again at the next run`, or with `GitHub refused the call (HTTP 403)`.
  Check `gh auth status` as the tehas user: a token that lost access, or one
  that cannot write pull requests, answers that way for every pull request.
- `#<pr>: the description has a preview notice that is not this one, left
  alone`. Someone wrote the marker lines, or put text of their own inside a
  notice. The description is left as it is, once for each commit and state.
- `#<pr>: the answer about this pull request was not understood`. GitHub
  answered with something that is not that pull request's description.
- `#<pr>: the notice was not written, tried again at the next run`, and after
  three of them `#<pr>: the notice was not written three times, left to the next
  commit or state`. What GitHub had as the description before the last write is
  in `~/tehas/preview/<pr>/.description`.
- `#<pr>: the pull request is no longer open, nothing was written`. It was
  merged or closed between the list the run was given and the read.
- `#<pr>: the description was not backed up, the notice was not written, tried
  again at the next run`. The host could not write
  `~/tehas/preview/<pr>/.description`, so nothing was sent to GitHub. Check that
  `~/tehas/preview` belongs to the tehas user and has room.

A comment written by the version of previews that commented instead (until
2026-10-11) is not edited or removed by anything now.

**The dashboard's "List written" time is old.** Every run writes the list,
also one that builds nothing. An old time means no run came by:
`systemctl --user list-timers tehas-preview.timer`, and
`tail ~/tehas/logs/preview.log`.

## A preview is still there after its pull request closed

The five minute run removes it at its next turn, if GitHub answered with a
list. The sweep is the second check, every three hours. Its lines in
`~/tehas/logs/preview.log` have the word `swept`, and the time of the last
one is `jq .lastSweep ~/tehas/live/previews.json`. Run it by hand:

```
tehasbot preview --sweep
```

A run asks GitHub about a pull request by itself before it removes anything of
it, because the list of open pull requests is read in pages and can move while
it is read. What it removed is in the log as `#<pr>: pull request no longer
open, removed`, `#<pr>: its branch is not in <repo>, removed` or `#<pr>: it was
not opened by someone of <repo>, removed`. What it kept is said too: `#<pr>: open
though the list did not say so, kept`, or `#<pr>: no answer from GitHub, kept`
for a call that failed or answered unclearly.

It removes only on a clear answer, so read what it printed:

- `GitHub does not show <repo>; nothing was swept by its word`: the bot's
  token cannot see the repository. Check `gh auth status`.
- `#<pr>: no answer from GitHub, kept`: the call about that pull request
  failed or was unclear. The next sweep asks again.
- `#<pr>: a container could not be removed, tried again at the next sweep`:
  Docker refused to remove a container of that pull request, so its image
  and state were left too. `docker ps -a --filter label=tehas.preview=<pr>`
  shows it; remove it by hand with `docker rm -f -v`.
- `the lock was not free in 30 minutes, nothing was swept`: a run was
  building all that time.

If neither timer runs, nothing is removed:
`systemctl --user list-timers 'tehas-preview*'`.

## This site shows an old page

The pages are installed copies in `~/tehas/docs`. After a change is merged:

```
tehas update      # pull the platform checkout, install, renew what already runs
```

No restart is needed; each request reads the page from disk. If the host has no
platform checkout yet, run `./install.sh` from wherever the repository is kept.
