---
title: tehasbot and tehasbot cycle
description: "The command the timers and services run, and the unit that runs one cycle."
---

## tehasbot

What the units run. Nothing here is needed in daily use; it is the place to
look when a timer or service has to be run once by hand.

```
tehasbot dispatch             # one poll; what tehas-dispatch.timer runs
tehasbot cycle <issue>        # one cycle and its report; what tehas-cycle@<issue>.service runs
tehasbot status               # write the queue snapshot
tehasbot health               # write the health file
tehasbot energy [--every]     # sample the model mesh; what tehas-energy.service runs
tehasbot top [--every]        # publish the snapshot of tehas top; what tehas-top.service runs
tehasbot protolab             # build the Protolab; what tehas-protolab.timer runs
tehasbot preview              # keep the previews; what tehas-preview.timer runs
tehasbot preview --sweep      # the second check; what tehas-preview-sweep.timer runs
tehasbot improve              # one run of the improvement loop; what tehas-improve.timer runs
tehasbot refresh              # ask for refreshes of conflicting pull requests; what tehas-refresh.timer runs
tehasbot review               # one review of the branch; what tehas-review.timer runs
tehasbot clean                # remove what the host can make again; what tehas-clean.timer runs
tehasbot github-token [--check]  # the GitHub App's token, or a check of it
tehasbot github-token --cycle [--check]  # the cycle's token: the product repository only
tehasbot github-writes        # the GitHub writes of the last day, by hour
```

The units are `tehas-dispatch.timer`, `tehas-dispatch.path` (a poll as soon
as a cycle ends), `tehas-energy.service`, `tehas-top.service`,
`tehas-protolab.timer`, the previews' `tehas-preview.timer` and
`tehas-preview-sweep.timer`, the improvement loop's `tehas-improve.timer`, the refreshes'
`tehas-refresh.timer`, the review loop's `tehas-review.timer` (weekly, and a
run does nothing unless `TEHAS_REVIEW=1`), the cleaner's `tehas-clean.timer` (installed, and enabled
by a person), and one `tehas-cycle@<issue>.service` per running
cycle. A cycle's container is `tehas-work-<issue>`.

## tehasbot cycle

One cycle for one issue, and its report. The dispatcher starts it as a unit;
it is not typed in daily use.

```
systemctl --user status 'tehas-cycle@*'    # the cycles that are running
systemctl --user stop tehas-cycle@12       # end the cycle for issue 12
```

The unit runs `tehas-cycle run <issue>`, which gives the cycle its own copy of
the code: it hardlinks `~/tehas/bin`, `~/tehas/libexec` and `~/tehas/lib` into
`~/tehas/run/cycle-<issue>-<pid>/` and runs `tehasbot cycle` from there. An
update replaces each file by renaming a new one over it, so the copy keeps the
files the cycle started with, and the cycle ends on the version it began on.
The state store and the analytics program are the exception: the cycle calls
the installed ones, because they own the databases. `dispatch.log` says
`tehas-cycle: #N runs from <copy>` when the cycle starts. A copy that cannot be
made is said there as `#N runs from the install: its own copy could not be made
(<why>)`, and the cycle runs from `~/tehas` as before. Each start removes the
copies whose process has ended.

`tehasbot cycle` is `tehas-cycle cycle`, which the console execs with the
settings loaded. It takes `state/cycles/<issue>.lock`, waiting up to 5 seconds
for it (exit 3 when another cycle of the issue holds it), and hands it down to
`tehas work` and the docker client, so a cycle killed outright keeps its place
until they are gone too. Then it puts the start on record, asks what the number
is, runs `tehas work` and sees to the report. A stop reaches it as the main
process of the unit. With no `libexec/tehas-cycle` nothing starts: exit 78.

**The clock a cycle is measured on.** `tehas-cycle clock` counts a cycle on a
monotonic clock: the seconds since boot, read from the first number of
`/proc/uptime`. The wall clock is not read at all, so the time a VM was saved
counts against nothing. `tehas-cycle clock now` prints those seconds;
`tehas-cycle clock mark <file>` writes them into a file, which is how a cycle
marks its start and the last growth of its logs; `tehas-cycle clock check
<file> --limit <s> --stall <s> --since <s>` reads the mark of the start in
`<file>` and the mark of the last log growth in `--since` (a number `clock now`
printed), prints `ok`, `stalled` or `over` and exits 0, 1 or 2, counting the
limit before the stall. A mark that is missing or is not one number, or a time
that would run backwards (a mark ahead of the clock, as after the boot was
redone), prints `unknown` and exits 3: a caller stops nothing on a guess. The
seconds are whole seconds since boot truncated down, and one mark file holds
one number; a mark file of more than 32 bytes, or one that is not a regular
file, is no mark. The watch of `tehas-cycle work` asks the same clock in its own process.

**The pull request of a stopped cycle.** `tehas-cycle finish <issue> --home
<dir> --repo <owner/name> --default <branch> --reason <text>` takes what a
stopped cycle left and opens it as a draft pull request, so the factory does
the push and nobody outside the host needs write access to the code. It finds
the kept workspace `$TEHAS_WORK_DIR-<issue>`, which is
`~/tehas/work/<name>-<issue>` for a host that keeps its settings in `~/tehas`,
and the branch `feature/issue-<issue>-*` in it.
It makes a repository of its own beside the workspace, fetches the
workspace's branches into it with `git fetch <workspace>`, rebases the
branch's commits onto the remote's `<branch>` there, pushes them to a new
branch `finish/issue-<issue>` of `https://github.com/<owner/name>` with no
force, and opens the draft with `gh pr create --draft`. It prints the URL. No
git command runs in the workspace: the cycle's container wrote it, git
configuration included, so nothing that configuration names (a hook, a
filter, a signing program, a credential helper, a remote) is used. Whether
work was left uncommitted is read with the workspace as the work tree of the
run's own repository; its `.git/info/exclude` is copied as text and not read
through a link. The kept workspace is only ever read, the cycle's own branch
never moves, and the run's repository and the body file go on every path,
the refusals included.
The draft's body says `Fixes #<issue>.`, that the factory opened it from a
stopped cycle, and that its gate did not pass, with the `--reason` as the asker
put it: whether the failure was outside the change is the asker's judgement, and
the draft never says the gate was green. Of the settings it reads only
`TEHAS_WORK_DIR`, the name the workspace was given by; the rest it is given, so
what it works on is what the caller named. It asks no authorisation: who asked
is what [the poll's finish step](/operating#finishing-a-stopped-cycle) checks
before it calls this.

It refuses with one line on standard error and exit 1, and changes nothing,
when: the workspace or the branch is missing or is no git repository, the
working tree holds work that was never committed (nobody has checked it), the
branch holds no commit over `origin/<branch>`, `origin/<branch>` is not a branch
of the remote, `finish/issue-<issue>` already stands on the remote, the issue
already has an open pull request of its branch or one whose body closes it (a
body that denies the closing word, `this does not fix #<n>`, names no pull
request of the issue), or the rebase conflicts. A push the remote refuses is not
tried again. A refusal
names the rule and, where the command said something, its first line. On a
host the poll runs it when an issue asks with a marked comment and
`tehas:finish`; see [finishing a stopped cycle](/operating#finishing-a-stopped-cycle).

It runs [tehas work](/tools-work#tehas-work) for the issue, reads the runner's result
that `tehas work` left in `~/tehas/state/cycles/<issue>.result.json` (status,
the step it stopped in, cap, branch, token usage), finds the pull request the
bot opened, and relabels and
comments: merged, `tehas:awaiting-merge`, or `needs-human-attention`. A cycle
that stopped is parked on the state store first (`store park`: why it parked,
its step and its pull request), then gets its comment with three boxes and the
label `tehas:question`. The comment's id goes on the store as the park's
question, with the sha256 of the body posted, which is what lets the
dispatcher take a tick there. A park the store does not take asks no
question: the comment is the plain one that says to apply `tehas:ready`, with
no boxes and no `tehas:question`, and the log says `WARN: could not record the
park of #N (...), so it asks no question`. A question the store does not take
is posted all the same, and the log says its boxes will not work. A question
whose label cannot be added is taken off the record again, and the issue is
told the boxes cannot be used. It
then removes the workspace of a cycle that merged or left a pull request, and
keeps the workspace of one that stopped. The images `tehas work` fetched for
the cycle are removed in every case. A runner that started no cycle (exit 4) is
parked as a cycle that never ran: `tehas work` has already said on the issue which reason it was (an image
or file it could not be given, or an issue whose text or title is not the approved one),
and tehasbot cycle's own line names the exit status, who approved the start and
when from the approval its start used, and where the run log is, naming no
status, step, duration or token usage as that cycle's. The approval of such a
start is taken off the record with the report: the millwright, a refresh and a later
start never rest on an approval that started nothing.
A retry moves a stopped workspace to
`<name>-<issue>.before-retry`, which goes when that retry ends.
The dispatcher removes a kept workspace at once when its issue is closed, and
otherwise once no cycle has run on its issue for `TEHAS_WORKSPACE_KEEP_DAYS`
days (7 by default), three issues a poll at most. With the workspace go what a cycle leaves
outside it: the agents' transcripts in
`~/.pi/agent/ensemble-runs/issue-<issue>`, the console
`~/tehas/live/console-<issue>.txt`, and a planning session's transcripts in
`~/.pi/agent/ensemble-runs/plan-<issue>`, all of them the names `tehas stop`
removes. An issue whose workspace is already gone loses those three on their
own: the sweep looks for them whether or not the issue still has a workspace.
The two guards of `tehas stop` and `tehasbot clean` hold for all of it: the
issue's lock has to be a lock file, and every name it goes by has to be one the
poll can date, with one `stat` for all of them. Each name is built from the issue number, so the sweep of 5 touches nothing of
51, and the dated directories of `~/.pi/agent/ensemble-runs` are the runner's
and stay for a person. `tehas export` reads `~/.pi/agent/ensemble-runs/issue-
<issue>`, so an export made after the sweep has no transcripts of that issue.
A
stopped unit stops the container and is reported as a cycle that ended without
a pull request. The report of a stopped cycle tries each call to GitHub once,
cuts it after 20 seconds and does not refresh the status, so it ends within
the unit's `TimeoutStopSec`. A hangup is taken as a stop too. It checks no authorisation:
the dispatcher did that before it started the unit. It does check the number:
once it holds its lock it starts its record in the state store, then asks
GitHub what the number is before a label is changed, a comment is posted or a
container starts, tried three times like its other calls, and refuses a closed
issue and a pull request (exit 65) with one line in the log, ending its record
as refused. A number GitHub could not answer for at all is refused the same
way, with a line that says that. Because the start was on record first, the
poll that started the unit takes such a cycle as one that came up: the issue
keeps `tehas:running`, and the next poll clears it in silence.

**The start on record.** The record holds the attempt, when it started, the
approval it used and who gave it, how (`label`, `retry-tick`, `millwright`,
`refresh`, `outage-retry`, or `hand`), whether it is the one retry after an
outage, and the pi-rukas commit of the sandbox image. Started by
`tehas-cycle@.service`, which sets `TEHAS_CYCLE_FROM=unit` (a unit file not
refreshed yet is known by systemd's `INVOCATION_ID`), the start needs the
approval the dispatcher recorded and uses it; without one it runs nothing.
Typed by hand, with neither, the start is on record as one by hand: it uses no
approval, and its history says `hand` and names no account. When the store
will not start the record, the cycle exits before any label, comment or
container: 75 when the store stayed busy through three tries, 77 when it
refused the start (no approval waiting, or the issue's lock not held as the
store sees it), 78 when the store is not installed or cannot be used. The
dispatcher's wait then finds no new attempt, finds the lock free and gives the
start up. `tehas work` is run with `TEHAS_CYCLE_ATTEMPT`, the attempt.

**The end on record.** Once its comments and labels are done, the cycle ends
its record: the status, step and exit, the runner's own exit, the counts of
memory kills, refused starts, a stall and a time limit, the branch, the pull
request by its number, the cap, park reason and failure, whether a provider
outage stopped it and whether it parked, and the result and progress that
`tehas work` left, each kept as a claim only when the store can read it (a
link is refused, and progress older than the start is not kept). It writes one
line of history per end, the last 50 of each issue, of which `store prune` takes
those of an issue whose cycle ended more than its window ago, keeping that
issue's last row of each history. A store that does not take the end after five
tries is `ERROR: the end of #N is not on record (...)` with
every argument named; the record stays open, the issue's next start replaces
it, and the cycle ends with exit 70 rather than 0, so the unit does not read as
a cycle that got its end on record. tehas work's files that carried these are
read and removed as before, except the result and the progress, which stay
until the next start for
`tehas export`.

`tehas:awaiting-merge` is given only to a run that ended as `handoff`, with the
runner's exit 10 or 11, has an open pull request of the bot that names the
issue, and stopped on the cap `awaiting-human-merge`: only the merge is left to
a person. Even then it is not given when a stable `cap-hit` line of the
cycle's progress ended the review on `round-cap` and sent it on to `ci` (the
review ran out of rounds with findings open), when the progress cannot be
read, or when the issue carries `needs-human-attention` at the end, as GitHub
answers, or GitHub does not answer. Any other end with a pull request open gets
`needs-human-attention` only, and the comment names the cap and the pull
request. The cap is the result's `cap`, from the vocabulary pi-rukas publishes
in `--capabilities`.

The cycle posts pi-rukas's handoff: the runner is given
`PI_ENSEMBLE_HANDOFF_CHANNEL=host`, so it posts none, and its result carries
the body. It goes into the park comment on the issue, or with the token usage
on the pull request for a merge that waits. It is cleaned like any agent's
words (boxes, markers and @-mentions are escaped) and headed by pi-rukas's
marker `<!-- pi-rukas:handoff issue=N cap=CAP -->`, made from the cap.

An issue that carries `needs-human-attention` when its cycle starts is run with
`--restart`: the start is a person's answer to the handoff. pi-rukas lets that
cycle past its check of the label and takes the label off. Neither the cycle
nor the dispatcher removes it.

The pull request is looked up in every state. One that a person merged while
the cycle ran ends the cycle as merged, whatever the result says: the
issue only loses `tehas:running`, the comment says the pull request was merged
while the cycle ran and names the runner's exit, the cycle's record ends
`merged` with exit status 0 (the runner's own stays its `runner_exit`), nothing
is parked, and the tail of every poll,
[`console tidy run`](/tools-dispatch#tehas-dispatch), and
[tehasbot status](/tools-health#tehasbot-status-and-the-console)
label the closed issue `tehas:shipped`. One that a person closed without
merging gets `needs-human-attention` and a plain comment that says so, without
the retry boxes: to try again, apply `tehas:ready`.

What the result says is checked against GitHub. A pull request counts as the
cycle's only when the bot opened it on the branch the result names and it
names the issue: its head is
`feature/issue-<n>-...`, as pi-rukas names a branch, or its body closes `#<n>`
(`Fixes #<n>`, `Closes #<n>`). A pull request on that branch that names
another issue is said in the comment and not taken. A result of `merged` is
reported as merged only when the bot has a merged pull request for the issue
on that branch. Without one the issue is parked, the comment says that the
bot has no merged pull request for the issue, and both workspaces are kept. A
result that is a link, of another schema than 4, or with no entry for the
issue, is not read: the status is then `unknown`. pi-rukas's state file in
the workspace is never read.

A result file with nothing in it is no result: the runner died before it could
write one. Such a cycle is still reported, from what the host has of its own,
and is never left as `tehas:running`. Its last step is the last step its run log
`~/tehas/logs/work-<issue>-<time>.jsonl` says it started, and the comment says
`read from its run log`, which is the runner's own word and not the verdict of a
result. Only a cycle `tehas work` watched is read this way, and only its own
newest log: the summary `tehas work` wrote of the run, in
`state/cycles/<issue>.progress.json`, is what says the log is of this cycle, and
`state/cycles/<issue>.runner_exit`, which it writes with the runner's own exit,
is what says the run was watched at all. The
comment names that exit, because `tehas work` exits 2 for a runner that died
however it died and 137 only for a container it saw killed.

When those steps reach `commit-pr`, the step name right after the last of them
is the branch pi-rukas pushed, and GitHub is asked for the bot's pull request of
that branch. The last, because a run that pushed twice (a retry round, or a
refresh that pushes to the same pull request) ends on its last push, and that is
the branch GitHub holds.
Only a name of this issue's own branch, `feature/issue-<issue>-...`, is asked
about: a step of any other shape is a step and not a branch, and asks GitHub
nothing. GitHub decides whether one exists; a line of a log only says which
branch to ask about, so no log can report a pull request GitHub does not list.
With one found the cycle is reported as a handoff whose review did not finish:
`needs-human-attention`, the pull request named, and no box for a clean retry,
which would redo what the open pull request already holds. With none found, or
with no branch to ask about, the cycle is parked as any other that stopped
without a pull request. Nothing of a log is read for a cycle that wrote a
result: that result names its step and its branch, and the two are not mixed.

The label change that ends a cycle is tried three times, five seconds apart,
and so are the cycle's comment on the issue and the search for the pull
request. If GitHub still refuses the label change, the log says
`WARN: label change on #N failed`, the cycle's comment says that the label
could not be changed, and the issue stays `tehas:running` until the next poll
of [tehas dispatch](/tools-dispatch#tehas-dispatch) finds it. A comment GitHub still refuses
after three tries is one line `WARN: comment on #N failed` in the log, and the
cycle is reported as far as it could be. A search for the pull request that
GitHub refuses three times is not taken for a cycle
that left none: the issue is parked with a comment that says the pull request
could not be looked up and names the branch, and the branch stays on the
cycle's record in the state store until the issue's next cycle starts.

A parked cycle's cap, from the result, goes into its history line and its
park on the state store (the park's reason is `provider-outage` for an outage,
else the cap, else the status; a cap or status that reads `provider-outage` is
recorded as `cap:provider-outage`, as only tehas names an outage), and with the failure before it (the reason
`tehas work` wrote to the progress file) into the
record for the improvement loop as `cap` and `capReason`, reduced to plain
characters. The reason is kept whole up to 2000 characters, as many as the
condensed stream keeps of a text; a longer one ends ` (cut)`, and so does the
park comment's. The record's `handoffDelivered` says whether pi-rukas's
handoff reached a comment, on the issue or on its pull request: `true` when
the cycle's park or comment that carries it was posted, or lean posted its own
report; `false` when every post of it failed, and the log then says
`WARN: the handoff of #N reached no comment: it is only in the run log`;
`null` when the cycle wrote no handoff. A cycle that ended at tehas's own bound gets tehas's cap:
`wall-clock` for the time limit, `stall` for the stall, `runner-never-started`
for the container that ended before the driver wrote its first event, `signal`
for a container stopped on a `TERM`, `INT` or `HUP`, and a
`capReason` that names the bound and the step. A cap the runner named mid-step goes into
the reason; a cap of an end it reached before the stop is kept. Where the result gives a failure of another kind than `cap`, the
park comment names its kind, role, cause and message. The record also holds `buildCommit`, the pi-rukas commit the
result says the cycle ran; the improvement loop checks a fix against it. A
provider outage is said as one: `Tehas cycle parked by a provider outage`,
with the cap and the reason. That is a result whose `failure.kind` is
`provider`. For an image whose result has no `failure` (v0.12.80) it is
`step-failed:<step>` or `adversarial-infra-failure` whose reason names the
provider, a timeout, a connection or a 5xx status. The
park then owes the issue one retry by [tehas dispatch](/tools-dispatch#tehas-dispatch), unless
the cycle was that retry or was started by hand; the comment says which. An
outage park the store would not take owes nothing, and its comment says it
waits for a person.

Once its history line is written, the cycle records the run in the factory's
analytics, `~/tehas/state/analytics.db`: `~/tehas/libexec/analytics run` with
the run's facts (its history line, approval, settings snapshot and the image's
commit, gathered into a private file for the call), the image, its pi-rukas
commit and the counts of kills and refused forks it read, then `analytics
event <issue> ended`. Each call may take 5 seconds. One that fails or runs out
of time is a line `WARN analytics <subcommand>: exit N` in the log, facts that
cannot be written are `WARN analytics run: no facts written`, and neither
changes anything in the report. A stopped cycle is recorded too.
`TEHAS_ANALYTICS=off`, or a host where the program is not installed, makes no
call.

| Exit | Meaning                                              |
| ---- | ---------------------------------------------------- |
| 0    | The cycle ran and was reported, whatever its outcome |
| 3    | A cycle for this issue is already running            |
| 64   | The argument is not an issue number                  |
| 65   | The number is not work the factory takes: a pull request, a closed issue, or an answer GitHub could not give after three tries. Nothing was changed and no container started. The cycle still claimed its place, so a poll that started it does not report a failed start |
| 70   | The state store did not take the cycle's end after five tries. The cycle reported what it could and its record stays open, which the issue's next start replaces. The `ERROR:` line names every argument of the `cycle-end` it tried |
| 78   | The instance settings are invalid                    |
