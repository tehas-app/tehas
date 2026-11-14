---
title: The console, health, energy and top
description: "The files that feed the dashboard, and what watches the host's health, its models and its load."
---

# The console, health, energy and top

## tehasbot status and the console

Feed the [factory floor](/floor) and the [plain dashboard](/dashboard). `tehasbot status` writes the queue snapshot to
`~/tehas/live/status.json`; the dispatcher runs it at the start of a poll only
when that file is five minutes old or missing, and after any poll that changed
the queue, and a cycle when it ends. An idle poll whose file the last poll wrote
keeps it, so the reading a person sees is at most five minutes older than the
last poll that had anything to report.
Each issue in its lists has `number`, `title`, `url`, `createdAt`, `updatedAt`
and `closedAt`.
Completed means closed by a merged pull request, by the close event's closer or
by a merged pull request GitHub lists as closing the issue when the close event
names none. That list is what GitHub derives from the text and the links of the
issue, not what the close event recorded. Whatever labels the issue was left
with: the query and the acceptance rule are the console's
(`console/ops/src/shipped.rs`), the ones `console tidy run` reads too, so
the two commands cannot disagree on what a merge closed. `tehasbot status` reads from the same answer which shipped
issues lack the label, those whose labels do not hold it or could not be read,
and gives each of them the label;
`console tidy run` is the one that logs the read, and both leave the control issue
unlabelled. Each read goes under the same 60 seconds as the count
below, and one that failed on a bad minute (a 5xx, a bare 403, or the bound) is
tried once more, after a second, as the poll's reads are. A read that still
failed is a line `WARN status: gh <verb> failed: <reason>`, with gh's own words
of the fixed list, and goes on the host's record of failed GitHub calls that
[tehasbot health](#tehasbot-health) counts. The snapshot is still written,
with what was read: when the list of issues failed, the lists of work
(`running`, `queued`, `awaiting`, `stopped`, `planned`, `planning`) and
`queueAt`, the time they were read, are the file's of the run before; when the
read of what closed them failed or answered no list, `completed`,
`completedCount` and `completedAt` are. `stale` names each part kept so,
`queue` or `completed`, and is absent when every read answered. A part the
file before did not hold is left out. The pause, all-hands and the words are
this run's. `planned` lists the open issues labelled `tehas:plan` or
`tehas:planning`, found by the same search as the lists, and `planning` counts
them. `completedCount` is a count, not a list: a whole number from 0 of the closed issues that
carry `tehas:shipped`, counted by a search of its own, so it keeps rising past
the hundred `completed` holds. The figure of the Shipped stage of the business
layout is that count where the file has one, and the length of the list where
it has none. The call for that count may take 60 seconds, the same limit the
platform reads go under; a call that fails or runs out of time leaves the key
out of the snapshot. The key is absent whenever the search gave no such number, so
absence is not zero: a reader cannot tell a factory that shipped nothing from
one whose search could not be read. `tehasbot status` gives each such issue `tehas:shipped`, so GitHub
lists the same issues, and `console tidy run` gives it too, from the same read; run `tehas labels` once so the label exists. The control
issue, the one `TEHAS_CONTROL_ISSUE` names, is left out of every list and every
count: it holds the pause label and is not work, whatever label it carries.

`tehasbot status` also writes `~/tehas/live/epics.json`, the epics panel of
both front pages: every open issue with sub-issues, GitHub's own links, newest
first, with GitHub's count of its sub-issues and how many are closed, and its
sub-issues in their order with their state. Only sub-issues of the instance's
repository are named; one elsewhere is counted in `more` and not named. A closed
epic is not read, so it leaves the file at the next poll. The console makes the
read and writes the file (`console epics`, `console/src/epics.rs`), under the
home and repository of the instance it is run for: one GraphQL
call for each hundred open issues, under the same 60 seconds. A read that fails
writes nothing and the file of the run before stays. `unlinked` lists the open
issues labelled `epic` that have no sub-issues, newest first, by number: the
label puts no issue on the panel, and `tehasbot health` warns about these.

`tehasbot status` also records issue events in the factory's analytics,
`~/tehas/state/analytics.db`, by comparing the snapshot it writes with the one
it replaces: `awaiting` and `stopped` for an issue new in that list, at the
time of the run; `merged` for an issue new in completed, and `closed` for one
that was in line, running or stopped before and is now closed and not
completed, both at the time GitHub says it was closed. An issue that was
awaiting a merge is not recorded as closed: GitHub's search for completed work
can lag, and its `merged` comes when it catches up. The program keeps the first of each per issue. A
snapshot before that is missing, not one, or more than an hour old counts as
none: the run then records only `merged` and `closed`. Nothing older than ten
days is recorded. Each call may take 5 seconds, and no call starts after
three such limits, so the calls take about 21 seconds at most. One that fails
or runs out of time is a line `WARN analytics event: exit N` on standard error
and changes nothing in the snapshot. Its event is lost: the next run compares
with the new snapshot and does not try it again.
`TEHAS_ANALYTICS=off` makes no call.

At the end of every run, also one that ends early because GitHub does not
answer, `tehasbot status` writes `~/tehas/live/analytics.json`, the ten-day
report of the factory's analytics, from `~/tehas/libexec/analytics report
--json`. It writes `analytics.json.new` and moves it into place, and keeps only
the top-level keys it names, so a key the program adds later is not published
until someone adds it to that list in `console/src/publish.rs`. The call has the same
5-second limit. A report that fails or is not one is a line `WARN analytics
report: ...` on standard error; the file that is there stays while it is less
than ten minutes old, so one poll that meets a locked database does not empty
the dashboard's section, and is removed after that, so a database that keeps
failing does not show old figures. With `TEHAS_ANALYTICS=off`, or where the
program is not installed, there is no file.

The file is public, as `status.json` is. It holds counts, durations, token
sums, version strings and the words of stop reasons and steps: no issue
number, title, failure cause, account, model or provider. A stop reason or a
step that is not a plain word (`a-z`, `0-9`, `:`, `_`, `-`, up to 60
characters) is counted as `other`. Every figure carries its count `n`, and a
value that cannot be worked out, such as a share of nothing, is left out
rather than written as null. Durations are seconds, times Unix seconds.

The window is ten UTC days: from midnight nine days before today to the time
of the report. Run figures count the work runs that ended in it; planning
turns are only counted.

| Key | What it is |
| --- | --- |
| `report` | The version of this shape, `1` |
| `from`, `to` | The window |
| `shipped` | `n`: issues whose merge is in the window, by their `merged` event or a run that merged |
| `planned` | Work to do, from the latest sample: `n` is `inLine` plus `planning`; `trend` is one `{day, n}` per day, the day's last sample, `n` left out on a day with none. Left out with no sample |
| `runs` | `n` work runs, and how many ended as `pr`, `merged`, `handoff`, `parked`, `failed`, `refused` and `stopped` |
| `planningTurns` | `n` planning turns that ended |
| `handoffRate` | `share` of the `n` runs that ended needing a person: handed off or parked |
| `firstPassYield` | `share` of the `n` shipped issues with a run here that took exactly one work run |
| `leadTime` | `median` and `p90` seconds from the first accepted `tehas:ready` to the merge, over `n` shipped issues whose `ready` is still in the database |
| `cycleTime` | `median` and `p90` seconds of the `n` runs |
| `machineTime` | `seconds` of all `n` runs, and `perShipped` |
| `tokens` | `total`, `input`, `output` and `cacheRead` of the `n` runs that reported usage, and `perShipped`: the total over the `shipped` issues it counts. A shipped issue whose runs all reported no usage is not one of them: an empty count is no zero |
| `availability` | `share` of the `seconds` since the first sample in the window that the factory could take work, from `n` samples. A sample counts for the time to the next, at most ten minutes, and only when it says not paused and either no hold or cycles running (slots that are full are not down time); a stretch with no samples counts as not available |
| `energy` | `kwh` and `perShipped`, from the `n` samples with the mesh's draw: each draw over the time to the next sample, at most ten minutes. Left out where no sample has one |
| `daily` | Ten `{day, shipped, runs, failed, minutes}`, oldest first: issues shipped, runs ended and of them failed, and their minutes |
| `stopReasons` | `top`: the five most common `{reason, n}` among the `n` runs that ended without a pull request, not counting those a person stopped. The reason is the failure kind, the cap for a failure that is a cap, else the park reason, else how the run ended |
| `stations` | `steps`: per step, in the order runs reach it, the `n` runs that reached it, how many `passed` and the `share`. A run that reached a pull request passed every step it ran; one handed off, parked or failed stopped in its failed step, else its last. `n` is the runs counted: refused and stopped runs are not |
| `versions` | One `{tehas, rukas, n}` per version pair, the pair that ran last first, at most ten. With three runs or more also `prShare` (reached a pull request) and `medianSeconds`; with three that reported usage also `medianTokens` (input, output and cache read, of those runs) and their count `tokensN`. A version that is not known is left out |

`tehasbot status` also writes `~/tehas/live/platform.json`: what platform the
host runs. On a host with the platform checkout `~/tehas/platform` that is the
commit it is at, `upstream` (origin's default
branch), how many commits it is `behind`, `behindSince` (when the oldest of
those landed on that branch) and `fetchedAt`. It fetches the checkout at most
once an hour, with a limit of 60 seconds; the time of the last try is in
`~/tehas/state/platform-fetch`. A fetch that fails is no error and is tried
again an hour later. Nothing else in the checkout changes: `tehas update`
does that. On a host that runs a release, where `~/tehas/current` is a link
to a version with a `VERSION`, the file names the release instead: `release`
(that version), `newest` (the newest published release, or null when it could
not be read), `behind: 0` and `upstream: "releases"`. The list of releases is
read at most once an hour, under the same limit and the same stamp, and the
last answer is kept in `~/tehas/state/platform-newest`, so a poll between
reads and a read that fails say the newest it had. A host with neither has no
file.

`~/tehas/libexec/console` is started by the work
script and follows the cycle's agents into `~/tehas/live/console-<issue>.txt`, masking
tokens, keys, passwords (also in a URL and after `Bearer`) and private network
addresses, and writing no control character but the line break and the tab.
Invisible format characters, direction overrides among them, are dropped
before the mask looks, so they do not hide a token from it; any that remain
and the Unicode line and paragraph separators are written as U+FFFD. An agent
is named by its role. The mask catches accidents;
it cannot stop an agent that sets out to print a secret another way, so treat
the console of a published dashboard as public. A log line of over 1 MiB is
left out, and the console says how many were. So is a line not yet ended
when the files together already hold 16 MiB of such lines, and a transcript
that is removed is forgotten. A console file removed while the cycle runs is made
again by the console at its next pass. Neither needs running by hand.
What the console itself complains about goes to `~/tehas/logs/work-<issue>-<time>.jsonl.console`,
and the work script says so when the console is missing or ends with an error.
At the end of a cycle the console is given 30 seconds to stop; one that takes
longer is killed, and the work script says so.
The work script's own status line, recorded with the cycle's heartbeat and published in `~/tehas/live/cycles/<issue>.json` and `~/tehas/live/running.json`, is the two lines the console keeps beside that file in `~/tehas/live/console-<issue>.txt.now`: the step of the last step the cycle started, and the first line of the driver's last notice. The console masks them as it writes, through the same mask as the console file, and rewrites the file only when one of the two changed; the work script reads them and starts nothing to get them. Without the file the step is `starting` and the notice empty. At the end of the cycle the same two lines name the step the cycle was in for the record, when the runner's result names none.

### The live view

The files under `~/tehas/live/` that describe the cycles and the factory's
health have one writer, the state store's `publish`: `cycles/<issue>.json`
(each running cycle and the one that finished last), `running.json`,
`endings.json`, and `dispatch.json` and `health.json` with `health.down` when
the poll and `tehasbot health` hand it their reports. It holds
`~/tehas/state/publish.lock` while it writes and renames each file into place
from a hidden temporary name, so a reader never sees half a file. It removes
the views of the cycles it did not keep before it writes anything, so a
publish that fails on a write leaves no cycle that has ended shown as
running.
`tehas work`, `tehasbot cycle` after a cycle ends, `tehasbot health` and the
poll call it. A running cycle not heard from for two minutes whose lock file
is found free is shown as `lost`, and its end is kept as lost when the issue
starts again. A missing lock file, or a missing `state/cycles`, does not make
a cycle lost: the cycle may still hold its lock on a file that was removed.

The console, the now file of the status line, and the mask of a log, also mask in any case the names
the host knows that an agent can print: each name and address in
`PI_ENSEMBLE_HOST_ALIASES`, the hosts of the model provider's `baseUrl` and
`backupBaseUrl` in `models.json`, and the bot's git email. The work script gives them with
`--private`; a name shorter than 4 characters is left out, as it would mask
ordinary words. They are names, not secrets: a token is never on a command
line.

The snapshot also carries the words of the breakroom game, Buzzword Buster.
They are counted over the titles and labels of every issue of the repository,
the newest thousand, not only the ones the factory was given: work going on
beside the factory shows up there. The file holds the hundred most used words
with their counts, and no title or issue number. On a published dashboard
those words are public too. If GitHub does not answer that call, the snapshot
is written without them and the game counts the tickets on the line. A round
shows at most twelve of the forty most used words, picked by chance, and writes
them on up to twenty-four tetrominoes: a wall no wider than sixteen cells and no
taller than six, its cells solid from the top down with no holes in them, so it
is another one every time.

## tehasbot health

```
tehasbot health     # prints nothing; refreshes ~/tehas/live/health.json
curl -i http://tehas-tehas:8080/health
```

Writes the factory's health from what is already on disk: the queue snapshot,
the running cycles, the last one that finished, the planning turns in flight,
what the dispatcher's last poll wrote about the slice and the demand, and this
host's pause flag. `tehasbot status`
runs it as it ends, so it is
refreshed at every poll and at the end of every cycle. When the poll of the
dispatcher is the one that ran `tehasbot status`, the command does not: the poll
gives it `TEHAS_FROM_POLL=1` in its environment, one of the names the load of
the settings keeps, and the poll runs health itself a moment after the status ends. The
name says the poll ran it and nothing else: it is given whatever token the poll
holds, so a host that acts as a machine user, or a poll whose token read
failed, is still known and does not refresh the file twice. So one
poll refreshes the file once, not twice; a `tehasbot status` a person or a
cycle runs does it on its own exit, as before. The poll that runs it
bounds it at 120 s, the same bound it gives the status and tidy, because health
bounds its own store calls only one at a time; one cut off leaves the file as
the last writer left it, and the poll says
`WARN: the health file could not be written`. The docs server
serves the file at `/health` and `/healthz` as `application/health+json`.
`tehas status` prints the same file for a person. The whole run is the console
beside the command (`console tehasbot health`, `console/ops/src/health.rs`): it
makes the five calls of the state store, the `systemctl --user --failed` read
and the publish itself. `console run` starts it: the settings are
loaded first, so the budget it reads from `TEHAS_POLL_BUDGET` is the
instance's and the load's 78 still fires on a malformed one, and it says so and
exits 78 when no console stands beside it.
`console health report`
builds the report alone from answers given as arguments, and writes nothing.

`checks.pause` names the switches: `{flag: {at, reason}, label, lifted}`, with
`flag` null when no flag is set, `label` whether `tehas:pause` is on the control
issue, and `lifted` whether the label is off with no account in
`TEHAS_ALLOWED_LABELERS` having taken it off, which holds the host paused. It
carries no `by`: the account that wrote the flag stays on the host, because this
file is served.

`checks.epics` is `{time, unlinked}` from `live/epics.json`: the open issues
labelled `epic` with no sub-issues, as the last read of the epics found them.
It is null when that file is missing, not of that shape, or more than ten
minutes old, which is what a failed read leaves, and then it warns of nothing.

| `status` | Meaning                                                                                  | HTTP |
| -------- | ---------------------------------------------------------------------------------------- | ---- |
| `pass`   | Taking work, idle or in a cycle                                                          | 200  |
| `warn`   | The queue snapshot is more than ten minutes old: GitHub did not answer the last polls, and `output` says how old. Or GitHub failed three calls or more in the last ten minutes (`checks.github`; `output` counts them and quotes the last). Or paused (the pause flag on this host, `tehas:pause` on the control issue, or a pause no allowed account took off), or the dispatcher holds everything because the slice is missing or cannot be read, or there are no resources (the model endpoint does not answer or does not offer the model), or too low on disk to build previews, or the free space could not be read, or the platform checkout has been behind origin's default branch for more than a day (`output` says how many commits; run `tehas update`), or the state store has no daily backup or the newest is more than two days old, or an idle poll cost more than its budget (`TEHAS_POLL_BUDGET`, `checks.poll_cost`), or systemd gave up on a tehas unit (`checks.units`, null when that read did not answer), or no cycle runs and the last poll held every issue in line on its own (`checks.slice.held`: issue, why, text, clear), as one whose last cycle parked on the body it still has, or an open issue labelled `epic` has no sub-issues (`checks.epics`; `output` names up to five and says to link their parts as sub-issues), or the MCP server is on and does not answer (`checks.mcp`), or the sandbox image is behind the pi-rukas the host follows (`checks.rukas.behind`; run `tehas update`), or the industry that runs the factory raised an alert in the last five minutes or wrote an alerts file that cannot be read (`checks.industry.alerts`) | 200  |
| `fail`   | The last cycle ended as anything but `merged` or `handoff` and no cycle has started since (a running cycle with no `startedAt` counts as started since), or there is no queue snapshot | 503  |

`output` says the same in a sentence. `checks.queue` has the time its lists
were read (`queueAt`, or the snapshot's time when it has none, whichever is
older) and the counts in line, building, in review and stopped, and `stale`
when the snapshot kept a part from an earlier read: the age of the snapshot is
the age of its lists, so lists kept through ten minutes of failed reads warn as
an old snapshot does. `checks.github` is `{failed, windowSeconds, lastAt, last}`:
the GitHub reads the poll, `tehasbot status` and `console tidy run` could not make
in the last ten minutes, from `~/tehas/state/github-failures`, and the newest
of their log lines. A read cleared by its retry is not on it. `checks.cycles` has the
issue, status, step and times of every running cycle; `checks.cycle` is the
first of them, or the cycle that finished last when none is running.
`checks.plans` has the issue, the turn, `startedAt` and `state` of every
planning turn in flight and of every open session whose last turn ended in
error. `state` is `running` for a session the state store has running
(`plan-show --open`) whose lock (`state/plan-<issue>.json.lock`) its unit holds.
The page asks the one probe, so a lock it cannot open, or a name at that path
that is no lock file, is a turn in flight too: the page reads a probe it could
not make as a turn that has not ended, never as one that has. `state` is
`failed` for a session still open whose last turn ended in error and waits for
a person: the store has it failed, or it is running on record beside an exit
status (`state/plan-<issue>.json.rc`) other than 0 that the next poll posts. A
session that is over, one whose turn finished, and one whose turn was cut off
are left out. A store that does not
answer writes no health file: `tehasbot health` exits 1 when it was busy, 78
when it is not installed or cannot be used. The publish of the report is tried
three times while the store is busy, within 30 s of its own, so that a store
that is slow rather than refusing still leaves a file before the poll's bound
cuts the run off. A check whose read did not answer - the units, the backup or
the idle polls - is left out of `checks` and its own `WARN:` line says so, so a
home that passes with a check missing is a home whose store or systemd did not
answer.

`fail` is about the cycle that finished most recently. It lasts until a cycle
starts after that one ended. A cycle that was already running when the other
one failed does not clear it. A running cycle whose `startedAt` is missing reads
as started after it, because a cycle with no known start is running now.
A cycle that has written nothing for two
minutes and whose issue's lock (`state/cycles/<issue>.lock`) nobody holds is
recorded as ended with the status `lost`, which is a failure. While
`tehasbot cycle` still holds the lock, the cycle stays running.

`checks.platform` is `live/platform.json`: the release a host runs, or the
checkout of a host that has one. It is `null` on a host with neither.

`checks.rukas` is `live/rukas.json`, which `tehasbot status` writes: the
pi-rukas the sandbox image runs against the one the host follows. `follow` is
`PI_RUKAS_FOLLOW` (the instance's over `versions.env`), `target` the commit it
names now and `targetVersion` its release (asked upstream at most once an
hour, and once more as soon as the image is on another commit than the answer
kept, so an image built from a release that came out within the hour gets its
own target at the next status run), `image` the `tehas.rukas` label of `PI_ENSEMBLE_IMAGE`, which is what
cycles run, `imageVersion` what that image's `pi-rukas-headless --capabilities`
answers (asked once per image), and `clone` the commit of
`~/tehas/repos/pi-rukas`. `behind` is true when the image is not on the target
commit and is not a later release on the line followed (an image ahead of its
target is no drift), and makes the health `warn` with `The sandbox image runs pi-rukas
<image> and the host follows <target>: run tehas update.`; `install.sh --check`
reports it as `Sandbox: BEHIND`, unless the image's `tehas.rukas` label is no
longer the `image` the health read: an update built it after the last status
run, and the check says `Sandbox: NOT READ` and finds nothing. A value that could not be read is `null` and
decides nothing. `checks.rukas` is `null` on a host with no pi-rukas.

`checks.industry` is `{version}` of the tehas-industry that runs this factory,
from `TEHAS_INDUSTRY_VERSION` in `~/.config/tehas/env`, which industry writes.
A factory that runs on its own has no `checks.industry`, and its status is
decided as before. A value that is not `vX.Y.Z`, with an optional `-<suffix>`
of letters, digits and dots, is left out the same way, and nothing fails.

With an industry, `checks.industry` also carries the alerts it writes into
`~/tehas/state/industry/alerts.json` on each pass, as [the factory
API](/api#industry-alerts) describes the file: `alerts`, a list of `{kind,
since, text, link}`, with `alertsTime`, when the industry wrote them, and
`alertsStale`, `true` when that is more than five minutes ago. A stale file's
alerts are listed but not current. `kind` is `paused`, `frozen` or
`held-by-one`, and a kind this release does not know is kept by its word.
`since` is in UTC, `text` is one line of at most 200 characters, and `link` is
`null` when the file's is not an `http` or `https` address. No file yet is
`alerts: null`. A file that cannot be read, or is not what the industry
writes, is `alerts: null` with `alertsUnreadable: true`, never an empty list.
Any current alert makes the health `warn`, with the first alert's text as
`output`, ahead of the pause the factory knows of itself; an unreadable file
makes it `warn` with `The industry's alerts cannot be read: ...`.

`checks.backup` is the newest daily copy of the state store, `{time,
ageSeconds}`, or `null` when there is none (see [Backups and
restore](/operating#backups-and-restore)).

`checks.upkeep` is the cleaner's last `store upkeep`, the step that makes the
backup: `{time, ok, word}`, where `word` is the store's word when it failed
(`busy`, `corrupt`, `absent` when the store is not installed, ...) and `null`
when it went well. It is `null` when no upkeep has run yet. The cleaner keeps it
in `state/upkeep.json`, with the store's whole line. When the backup is missing
or old and the last upkeep failed, `output` says when and the word: `There is no
backup of the state store: the last upkeep failed at <time>: <word>.` A failed
upkeep with a backup of under two days is no warning of its own.

`checks.poll_cost` is the cost of the polls of the last hour that started no
cycle, measured against `TEHAS_POLL_BUDGET` by
[`store poll-idle --budget`](/operating#what-a-poll-costed): `{over, budget}`,
where `over` names the figures whose median was over its budget (`cpu`, `wall`
or `writes`) and `budget` is the value the instance runs with. It is `null`
when the store gave no answer, which means the check did not run. When `over`
is not empty, `output` says what tripped it, in the units of the budget:
`An idle poll is over its budget on cpu (TEHAS_POLL_BUDGET=cpu=5,wall=30,writes=0):
of the last hour's 12 idle polls, cpu had a median of 7.25 s against 5 s and 9
over it.`

`checks.units` names the tehas units systemd has failed
(`systemctl --user --failed 'tehas-*'`), and is empty when there are none. It is
`null` when that read did not answer (systemd was not there, or the 10 second
timeout went by), which means the check did not run rather than that no unit
failed; the same shape as `checks.poll_cost`. A failed `tehas-dispatch.path`
starts no poll when a cycle ends, so a slot waits for the timer. It stays failed
until `systemctl --user reset-failed` and a start, which `install.sh` does.

`checks.mcp` says whether the [MCP server](/mcp-server) answers. With
`TEHAS_MCP` unset or `0` it is `{"on": false}` and nothing is asked. With
`TEHAS_MCP=1` it is `{on, installed, answers}`, and `reason` when it does not
answer: `installed` is whether `~/tehas/libexec/tehas-mcp` is there, and
`answers` whether a `POST /mcp` with no key, to port 8082 on `TEHAS_HOST_ADDR`
(or the loopback), got the server's 401. Connecting and the answer are each
bounded at 2 s. A refused connection, no answer in time, or any other status,
such as a proxy's 502, is `answers: false`, and the status is `warn`. A run
of `console health report` that is given no `--mcp` leaves it `null`.

`checks.allHands` is the snapshot's `allHands`: true while
[`tehas:all-hands`](/operating#all-hands) holds, false when it does not, and
`null` when the snapshot is older than ten minutes or could not read it.

`checks.demand` is what the dispatcher's last poll wrote to
`live/demand.json`: `{time, waiting, oldest}`, the issues that would start if
there were room and when the oldest was labelled. It is `null` when no poll
wrote it in the last ten minutes. `checks.slice` also carries `free`, the free
slots, and `awaitRoom`, the room under `TEHAS_MAX_AWAITING` when that poll
read it. [`tehas status --json`](/api#tehas-status---json) is built from these.

`checks.failed` lists the cycles among the last ten that ended, newest first,
that ended as anything but `merged` or `handoff`: `issue`, `status`, `step`,
`startedAt` and `time`. The `status` of an ending whose status names no end at
all (`running`, or none) is `not an end`, and `output` says of the last cycle
that it "ended as not an end". health does not change the overall status. An
abort that a merge overtook ten seconds later leaves the status `pass`, and is
still in `checks.failed` until ten more cycles have ended. A monitor that polls
less often than cycles end reads it to see every failure. The ten endings are
kept on the state store and published in `live/endings.json`.

The store records an ending with the word the cycle reported, except for the two
that say it did not end at all (`running`, and the empty word of a cycle that
never sent a heartbeat): those become `unknown`, in the ending and in the
heartbeat of the end that publish answers from, so a finished run never reads as
one still in flight.

`checks.disk` has the free and the required bytes and `low`, as `tehasbot
preview` last wrote them to `live/previews.json`. It is `null` on a host
without previews, and when that figure is more than half an hour old. When
`low` is true, `output` is "The host is low on disk: previews are not being
built." with a figure, and "The host's free disk space could not be read:
previews are not being built." when `freeBytes` is `null`.

`checks.slice` is `live/dispatch.json` as the dispatcher's last poll wrote
it: `time`, `why`, `hold`, `count`, `room`, `committed`, `max` and `reserve`.
The keys and the words of `why` are declared once, in `tehas-core`'s
`hold` module: the dispatcher builds the file from that type and the page
reads it by the same names, so a reader never keeps a copy of a key of its
own. With either count set to `auto` it carries `slots` too, nested under that
key: the two counts in force, `cyclesHow` and `containersHow` for how each was
arrived at, and `cycle` and `container` with the readings behind them (`bound`
for the one that gave the count, `slots` for what the host's own readings
give, `cpus`, the three settings as written, and `floored` when a reading
answered nothing).
It is absent for a factory that sets numbers for both, which has nothing to
explain.
`why` is empty when cycles may start, else one word: `slice`, `unreadable`,
`disk`, `resources`, `limit`, `red`, `unknown-gate`, `count`, `awaiting`, `pressure`, `room` or `full`, and
`hold` is the line the dispatcher logged; with `unreadable` it names the file
and what was missing in it. `resources` is a model endpoint that does not
answer or does not offer the configured model, or, with `TEHAS_WORK_IMAGES=1`,
a model or sandbox image that cannot take the images. `disk` is less than `TEHAS_CYCLE_MIN_FREE` free on the file system of `~/tehas`
or on the one Docker keeps its images on, or free space that could not be read.
Both warn, as a full disk and an unreadable one hold every start: `output` is
the line the poll logged. A host whose `df` or Docker is broken is told of by
the same word as a host that needs space, and the line says which of the two it
is. `red` is the check
`TEHAS_BRANCH_CHECK` names, failed on the branch's head commit. `awaiting` and `room`
are also given, with cycles starting, when the awaiting-merge limit or the
slice's room let fewer start than there are free slots and issues waiting;
`hold` then starts `holding: <n> of <free> free slots start:`. `full` is a slice that holds as
many containers as their ceilings fit. `room` is how many more reserves fit in the slice;
`committed` and `max` are bytes. The file also holds `endpoint` once the poll
has settled the model endpoint: `main` or `backup`, which endpoint of the
subagent provider the cycles are on. It is not in the health file, and names
no address. With `slice` or `unreadable` the status is
`warn` and `output` is the hold. With `resources` the status is `warn` and
`output` starts with `No resources:`. It is `null` when the file is more than ten
minutes old. `tehasbot health` calls neither `systemctl` nor `docker`.

A stopped dispatcher cannot report itself: the file simply stops changing and
the endpoint keeps answering with the last state. A monitor should also treat
a `time` older than about ten minutes as down.

## tehasbot energy

Optional. It feeds the Energy use section of the factory floor and nothing
else: no command reads its file, and a factory without it dispatches and works
the same. It is useful only when the model backend is a
[viiwork](https://github.com/janit/viiwork) mesh. The
command and its unit are installed on every instance, but the service runs
only if you enable it.

```
tehasbot energy            # one sample; prints nothing; writes ~/tehas/live/energy.json
tehasbot energy --every    # one sample at every 15-second mark; what the service runs
systemctl --user enable --now tehas-energy.service
```

Asks the model mesh for its cluster snapshot and keeps what the Energy use
section shows: per machine the draw, kWh and RAM; per model the slots, how
many are busy and what serves it; and the last 30 minutes of draw, with the
mesh's running count of tokens for the factory's models where it reports one. Only machines that are alive
and serve a model the factory uses are kept, and only those models. The
factory's models are the ones named by `PI_ENSEMBLE_SUBAGENT_MODEL` and
`PI_ENSEMBLE_MODEL_*`; with none set, the file has no machines. It works only against a viiwork
mesh, which it recognises by the answer.

| The mesh answers                       | The file                                   |
| -------------------------------------- | ------------------------------------------ |
| A cluster snapshot                     | Rewritten                                  |
| Nothing, a timeout, a server error or a busy answer (408, 429) | Left as it is; the page calls it stale |
| Anything else                          | Removed, and the section with it           |

The mesh is the endpoint the factory's cycles are on. The dispatcher settles
that endpoint every poll and writes `"endpoint": "main"` or `"endpoint":
"backup"` in `~/tehas/live/dispatch.json`; the sampler asks only that one,
with its token. With no such word in the file, because the dispatcher has not
settled one yet, it asks the main endpoint and then the backup, each with its
token, and samples the first that answers. The file is removed only when the
endpoint it asks is not a mesh and was not just down or busy. Set
`TEHAS_ENERGY_URL` in `~/.config/tehas/env` to ask another
cluster URL, which is asked with no token, and
`TEHAS_ENERGY=off` to remove the file. With neither,
`PI_ENSEMBLE_SUBAGENT_PROVIDER` has to be set: without it the command says so
and exits 78, asks nobody and leaves the file as it is, and `--every` stops
with it. The file is readable by anyone who can
open the dashboard and names the machines, their draw and their models. It
also holds the version of the mesh software and, for each model, the name and
the version of the engine behind it. Addresses, node ids and GPUs are not in it.

Each entry of `models` carries an `engines` array: one element per distinct
set of values among the machines that serve the model, sorted.

```json
{ "name": "model-small", "hosts": ["node-a", "node-b"],
  "engines": [{ "version": "2.9.2", "engine": "llamacpp",
                "engineName": "llama.cpp", "engineVersion": "b11371" }],
  "slots": 6, "busy": 2, "queued": 0 }
```

| Field           | What it is                                                              |
| --------------- | ----------------------------------------------------------------------- |
| `version`       | The mesh's version on that machine, without its leading `v`             |
| `engine`        | The mesh's own id for the engine                                        |
| `engineName`    | The engine's name for people. Reported from viiwork v2.9.2              |
| `engineVersion` | The engine's own version on that machine. Reported from viiwork v2.9.2, and not by every engine |

A value the machine does not report is `null`. `engineVersion` is also `null`
when it is not a plain version: only letters, digits and `. + _ -`, at most 40
characters, are copied. An element with no mesh version and no engine is left
out. A mesh in the middle of an upgrade, or a model served by two engines,
gives more than one element.

The model table shows them in its `ENGINES` column: the engine's name, or its
id where the mesh gives no name, then its version where there is one. So
`viiwork 2.9.2 (llama.cpp b11371)` from a machine on v2.9.2 and
`viiwork 2.9.0 (llamacpp)` from an older one. A model with no element shows a
dash.

The file's `source` names the mesh software. The section ends with a line that
says so, `DATA SOURCE: viiwork`. `TEHAS_ENERGY_SOURCE_URL` in
`~/.config/tehas/env` gives the address that name links; the sampler copies it
into the file's `sourceUrl` only when it is an `https://` address, and the page
shows the name as plain text with no `sourceUrl` in the file. The address is
the operator's to give: the sampler holds none of its own, so an adopter who
wants no link sets nothing.

The service reads its settings when it starts: run
`systemctl --user restart tehas-energy.service` after changing one of them.
Stopping the service does not hide the section; the last file stays and the
page calls it stale. To hide it, set `TEHAS_ENERGY=off` and restart, or stop the
service and delete `~/tehas/live/energy.json`.

Against a backend that is not viiwork the service still asks
`<provider>/v1/cluster` every 15 seconds and writes nothing. On such a host
leave the service disabled, or set `TEHAS_ENERGY=off`.

## tehasbot top

Optional. It publishes what `tehas top` draws, so that the factory floor and the
plain dashboard can show the same numbers a person sees on the host. Nothing on
the host reads its file.

```
tehasbot top                 # one snapshot; prints nothing; writes ~/tehas/live/top.json
tehasbot top --every         # one snapshot at every 5-second mark; what the service runs
systemctl --user enable --now tehas-top.service
```

It builds the snapshot with the same code the terminal view uses, in the
console (`console/src/top.rs`), and writes it as one line of JSON holding the keys the pages draw and nothing else:
`generatedAt`, `factory`, `queue`, `cycle`, `host`, `units` and `energy`. The
list is `PUBLISHED` in `console/src/top.rs`. A key added to the snapshot
later is not in the file until someone puts it on that list, which is how the
console of a cycle and the raw reading a host section was computed from stay
out. The command reads the instance settings, so a setting that is missing or
malformed stops it with 78;
with `--every` the same 78 ends the loop, and systemd shows the failing service.
It writes the file only when `TEHAS_TOP` is `1`; with `0`, which is the default,
it removes the file and exits 0. It writes to `top.json.new` and moves it into
place, so a reader never sees half a file, and a snapshot it cannot write leaves
the one that is there in place.

`TEHAS_TOP` is set in the instance directory. The unit is installed on every
instance and starts only when you enable it; nothing enables it for you. The
service reads its settings when it starts, so run `systemctl --user restart
tehas-top.service` after changing the setting. Stopping the service does not hide
the section: the last file stays until the page calls it stale, and the way to
remove it is `TEHAS_TOP=0` with one run of `tehasbot top`, or deleting the file.

The file is readable by anyone who can open the dashboard. It names the host's
cpu count, load averages, memory, its units, and up to fifteen processes by pid
and short name. It holds no command line, no path outside `/proc`, no token and
nothing about the sandbox: a cycle's container is one process among those rows,
and the console of a cycle is not in it.

The factory floor reads it every 5 seconds, the plain dashboard on its own
10-second refresh. Neither asks while the page is hidden, and both show no host
section where the file is not there: a host that publishes nothing is asked again
once a minute rather than at every period. Both pages work out what the file
means with one module, `host/fun/host-data.js` - the figures, the words for how
old a reading is, and the rule for when to ask again - so the panel and the plain
list cannot say different things about the same file. The panel gives the
readings, then the processes, then the units as `tehas top` lists them: name,
whether systemd has it installed, its state, and when a timer next fires; the
plain dashboard gives the same as a list. A snapshot older than 30 seconds is
called stale rather than live.
