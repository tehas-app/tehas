---
title: Analytics
description: "The factory's own 365 days of runs in SQLite: what is recorded, every column, how to fetch it and how to turn it off."
---

# Analytics

Every factory records each run it makes, with the tehas and pi-rukas versions
it ran, in one SQLite file: `~/tehas/state/analytics.db`. It keeps 365 days.
The question it answers first is "was the last release better than the one
before". The plain dashboard shows a ten-day report from it; a program can
fetch a copy and do deeper analysis elsewhere.

This is about how the factory works, not about the product: nothing in it says
what changed in which file. It is not the visitor analytics of
`TEHAS_ANALYTICS_SRC`, which counts visits to the pages.

## What is recorded, and when

| What | Written by | When |
| --- | --- | --- |
| A work run, its steps and its roles | `tehasbot cycle` | As the cycle ends, after its history line and before its files are cleaned up |
| A planning turn, as a run of kind `plan` | the dispatcher | As the turn ends. No token usage: pi's rpc mode does not report it |
| The issue events `ready` and `started` | the dispatcher | `ready` with the time `tehas:ready` was accepted, `started` as a cycle starts |
| The issue event `ended` | `tehasbot cycle` | As a cycle ends |
| The issue events `awaiting`, `merged`, `closed` and `stopped` | `tehasbot status` | The first time a run of it sees the issue in that state, by comparing with its last snapshot |
| A sample | the dispatcher | At the end of a poll that wrote to GitHub, started, stopped or turned anything: about every two minutes on a busy factory, and also when a cycle ends. A poll that changed nothing takes none, so an idle factory writes no row rather than the same reading again. Some 720 rows a day on a working one |
| `meta` | the program | When it makes the file |

Each event is stored once per issue, the first time. A second run of an issue
adds no event; every run is in `runs`.

Samples are the clock of the ten-day report: `availability` credits each row
with the time to the next one, at most ten minutes, and `energy` with the same
span, so a stretch with no row - a paused or held factory, an idle week -
counts as not available and as no draw rather than as one reading held. A fall
in `availability` or in a report's `n` after a long pause means the factory did
no work, not that the dispatcher is down.

The commands run `~/tehas/libexec/analytics`, a Rust program with SQLite built
in, which holds all of the SQL; `install.sh` builds it. Each call from a cycle
or a poll has a limit of 5 seconds and only warns when it fails: a locked or
broken database costs a missing row, never a cycle. Analytics never changes a
cycle's outcome, label or comment. It adds no mount and no variable to a
cycle's container: it runs on the host after the container has gone.

## 365 days

Rows older than 365 days are deleted on every write and on every poll, and the
file never holds more than 365 days of runs. A run is as old as its end. The
events of an issue that started and has not ended are kept at most a day
longer. 365 days is fixed and is not a setting. A file made when it was ten
days says 365 in `meta` after its next prune; what was deleted before stays
deleted.

## The file

`~/tehas/state/analytics.db`, mode 600, in WAL mode, with its `-wal` and `-shm`
beside it. It is not under `live/` and is not served. Times are UTC Unix
seconds and durations are seconds, as integers, except `runner_ms` and
`run_roles.ms`, which are milliseconds.

`PRAGMA user_version` is the schema version, today 3. Within one version
columns are only added. A change of a column's meaning raises the version.
A file at 2 is migrated in place as the program opens it: one transaction adds
the column `driver` to `runs` and makes the view `v_versions` anew, and every run
stays. Programs that have the file open go on working. A host whose file is still
at 1 keeps it aside under its version and starts a new one. Nothing of the gate
was on the record there before; the new gate columns are NULL where a run made no
gate, which the report reads as no gate rather than a gate of no time.

- **A file that is not a database**, or fails SQLite's check, is moved to
  `analytics.db.broken` and a new one is made. The data is at most 365 days deep.
- **A database of another schema version** is moved, with its side files, to
  `analytics.db.schema-<version>`, where a reader can still fetch it, and a new
  one is made. It is deleted 365 days after its last write. That goes for a
  version newer than the program too: after a rollback to a release of schema 2,
  the file at 3 goes aside whole. Move it back by hand before going forward again:
  nothing says it is still aside, and a second rollback renames the file then at 3
  over it, which loses the runs it held.
- Any other failure, a full disk or a lock held too long, leaves the file as
  it is.

A move is said once on standard error, in the log of the command that met it.

The schema is `console/analytics/src/schema.sql` in the source, with a pinned
copy `schema.pinned` beside it that a test compares it with. A reader may pin
that copy too. A change to the schema is agreed with tehas-industry first, as
for the JSON of [the factory API](/api).

## The tables

The columns that came after schema 1 are named by the schema in `Since`; the rest
came in schema 1. A column without a value is NULL.

### runs

One row per run, written when the run ends. A work run takes its facts from
the cycle, which gathers them into a private file for the call
(`tehas-cycle after`): the line of history the cycle made of its end and the
attempt the state store gave its start, and from the state store `at`, `how`
and `driver` of the approval that start used, read before the end goes on record
because a start that never ran takes its approval with it, the run's `settings`
snapshot and the image's pi-rukas commit. It reads the rest from the host: the
headless runner's result, `progress.json`, the preflight log and
`docs/version.txt`. A run that has no result (killed, stalled, timed out) is
stored with tehas's status and its times. A planning turn takes its session
on the state store as its facts, in the shape its record file had before the
store (running, turns, `startedAt`, label, answers), read before the turn's
end is recorded, and reads the turn's output `state/plan-<n>.json.out` from
the host. What docker said of the sandbox image as the dispatcher started the
turn is in `state/plan-<n>.json.image` until the turn's end reads it. Facts that are missing, a link, over
16 MiB or not JSON record nothing, as a run without a history line records
nothing. The private file is `~/tehas/tmp/analytics-facts.*`, removed after the
call; one that a caller killed during the call leaves is removed by
`tehasbot clean` once it is old.

| Column | Source | Since |
| --- | --- | --- |
| `id` | `<kind>-<issue>-<started>`, unique | 1 |
| `kind` | `work` or `plan` | 1 |
| `issue` | The issue number | 1 |
| `attempt` | The run's number for this issue: the attempt the state store gave its start, counting from 1, a start refused or by hand included. Work runs only | 1 |
| `how` | What started it: `label`, `retry-tick`, `outage-retry`, `millwright` (`fixer` on a run recorded before the rename) or `refresh`, from the approval the start used, else the history line, which says `hand` for a cycle run by hand. A planning turn: `label` for the first turn, `refine` for the first turn of a session `tehas:refine` started, `answer` or `millwright` for a later one | 1 |
| `started`, `ended`, `seconds` | The run's start, its end and the wall time between: the history line's `started` and `at`. A planning turn ends when the poll collects it | 1 |
| `queue_wait` | `started` less when `tehas:ready` was accepted (the approval the start used) | 1 |
| `outcome` | `pr`, `merged`, `handoff`, `parked`, `failed`, `refused` or `stopped`, below | 1 |
| `status`, `exit_code` | The runner's status and exit code, or tehas's own (`killed`, `stalled`, `timeout`), from the history and the result | 1 |
| `last_step`, `failed_step` | The step it was in last, and the step that failed, from the result and the history | 1 |
| `cap` | The cap that stopped it, from the result or the history | 1 |
| `park_reason` | Why it was parked, from the result or the history | 1 |
| `review_held` | 1 when the review held the pull request, from `progress.json` | 1 |
| `failure_kind` | The failure's kind, from the history or the result | 1 |
| `failure_cause` | The failure's cause, cut to 200 characters and masked as the console masks: hosts, addresses and keys are replaced | 1 |
| `provider_outage` | 1 when the run stopped because the model provider was out | 1 |
| `endpoint` | `main` or `backup`: whether the model endpoint was on its backup, from the preflight log | 1 |
| `pr` | The pull request's number, parsed from its URL; the URL is not kept | 1 |
| `tehas_version`, `tehas_commit` | `~/tehas/docs/version.txt` as the run ended, `vX.Y.Z` and the seven-character commit | 1 |
| `rukas_version`, `rukas_commit` | The result's `buildVersion` and `buildCommit`; else the commit of the sandbox image's `tehas.rukas` label | 1 |
| `image` | The sandbox image's id, `sha256:<hex>`: for a work run the one its container started from, for a planning turn the one the image name pointed at as the dispatcher started the turn | 1 |
| `input`, `output`, `cache_read`, `cache_write`, `turns`, `dispatches`, `cost` | The result's `usage`. For a planning turn `turns` is its turn in the session, and the rest is NULL | 1 |
| `oom_kills`, `forks_refused` | Processes killed for memory and forks refused in the container, as `tehasbot cycle` reads them | 1 |
| `concurrent` | How many other cycles ran when it started, from `settings` | 1 |
| `runner_ms` | The result's `durationMs`, in milliseconds | 1 |
| `result_schema` | The pi-rukas result's schema number, 3 or 4 | 1 |
| `gate_runs` | How many runs of the product's gate the cycle made, from its stream. NULL when the stream held none, which is no gate rather than a gate of no time; 0 is not stored, so a run whose rows the runner timed of nothing keeps no count either | 2 |
| `gate_ms` | Their milliseconds in all, as the runner timed each of them: the gate's own time, not the wait for another run of it that held the container's CPU share. A single run of it of more than three runs of `TEHAS_VERIFY_TIMEOUT` is no figure, and NULL with no run of it | 2 |
| `driver` | The driver the start's approval bound: `classic` or `lean`. NULL where the approval holds no driver or another word, on every run from before the column, and on a planning turn. NULL reads as classic | 3 |

The outcome of a work run:

| Outcome | Meaning |
| --- | --- |
| `merged` | The issue's pull request was merged |
| `stopped` | The issue was closed while it ran, or it was interrupted |
| `refused` | No cycle ran: the runner refused, or `tehas work` started none |
| `pr` | It left an open pull request and was not parked |
| `handoff` | The runner handed off to a person |
| `failed` | tehas stopped or lost it, the runner crashed or aborted, or a failure other than a cap stopped it |
| `parked` | Any other end that needs a person, such as a cap |

A planning turn is `handoff` when it asked its question, `stopped` when Pi no
longer had the session, `refused` when no turn ran, and `failed` otherwise.

### run_steps

The steps of a run, from the result's `steps`, else from the `step-started`
lines of the runner's progress log.

| Column | Source | Since |
| --- | --- | --- |
| `run_id` | The run's `id` | 1 |
| `step` | The step's name | 1 |
| `round` | The step's turn in the run: 1 the first time it ran, 2 the second | 1 |
| `started`, `ended`, `seconds` | The step's start, end and duration | 1 |

The key is `(run_id, step, round)`.

### run_roles

The usage of each role in a run, and the model it was given.

| Column | Source | Since |
| --- | --- | --- |
| `run_id` | The run's `id` | 1 |
| `role` | The role's name | 1 |
| `model`, `provider` | The role's model and provider in the run's settings snapshot (on the cycle's record in the state store, in the run's facts) | 1 |
| `dispatches`, `turns`, `input`, `output`, `cache_read` | The result's `usage.byRole`, else `progress.json` | 1 |
| `cache_write`, `cost` | The result's `usage.byRole` | 1 |
| `ms` | The role's time, in milliseconds, from `usage.byRole` | 1 |

The key is `(run_id, role)`.

### run_gate_runs

The runs of the product's gate that the cycle made, from the stable
`verify-completed` lines of the runner's progress log -- the newest
`logs/work-<issue>-<time>.jsonl` written since the run started, the same file
the steps are read from. One row each, in the order they finished. A cycle makes
a handful of them; nothing else is in the table, and no `ms` of one is the wait
for another.

| Column | Source | Since |
| --- | --- | --- |
| `run_id` | The run's `id` | 2 |
| `seq` | The run of the gate's turn in the cycle, counting from 1 | 2 |
| `command` | The command the runner ran, reduced to a word: `scripts/verify.sh --fast` is stored as `scriptsverify.sh--fast` | 2 |
| `tier` | Which gate it was: the `target.kind` the runner named, `fast` and `full` for a product with a fast gate, else its `tier` word where a runner answers with that instead. Neither key is in the stable vocabulary the runner publishes for `verify-completed` (`step`, `command`, `ok`, `ms`, `at`, and `status` on a later runner), so a row of neither says no tier rather than naming one | 2 |
| `step` | The step the gate ran in | 2 |
| `ok` | 1 when the run of the gate passed, 0 when it did not, NULL when the line named neither | 2 |
| `ms` | The run of the gate's own milliseconds, from the runner | 2 |
| `at` | When it ended, Unix seconds | 2 |

The key is `(run_id, seq)`: the order the runner ended them, which is the order
the rows are read back in, so the table needs no index of its own. A line the
reader will not take is no row: one the runner marks `unstable`, one of another
issue, one that named no milliseconds or milliseconds of more than three runs of
`TEHAS_VERIFY_TIMEOUT`, or the rows past the 500th of one cycle. A database made
before schema 2 holds no rows, and the report prints no `Gate` line for it.

### issue_events

| Column | Source | Since |
| --- | --- | --- |
| `at` | When it happened | 1 |
| `issue` | The issue number | 1 |
| `event` | `ready`, `started`, `ended`, `awaiting`, `merged`, `closed` or `stopped` | 1 |

The key is `(issue, event)`: the first time only. Lead time runs from the
first accepted `ready` to the merge. An issue whose `ready` is older than ten
days has no lead time here.

### samples

One row per poll, from the live files the poll wrote: `live/status.json`,
`live/dispatch.json` and `live/energy.json`. A file that is missing or older
than its poll gives NULL.

| Column | Source | Since |
| --- | --- | --- |
| `at` | When it was taken; the key | 1 |
| `in_line`, `running`, `awaiting`, `stopped` | The issues queued, running, awaiting a merge and stopped, in `status.json` | 1 |
| `planning` | Open issues labelled `tehas:plan` or `tehas:planning`, in `status.json` | 1 |
| `cycle_limit` | The cycles allowed at once, from `dispatch.json`: the concurrency, or the room under `tehas limit` where that is lower | 1 |
| `paused` | 1 when the factory was paused | 1 |
| `held` | The one word of the dispatcher's hold, empty when nothing held | 1 |
| `watts` | The model mesh's draw, from `energy.json`, where the mesh reports one | 1 |

### meta

`key` and `value`: `schema`, `retention_days` (365) and `created`, and
`image:<id>` for each image a run used: when it was built, Unix seconds, as
docker says of the id as the run ends. An id names one build, so the first time
recorded stands and every run of the image reads it, also one recorded before
it. A prune drops the key of an image no run names. No host name, address or
token. Since schema 1; the image keys add no column.

## The views

So that a reader does not have to work out the common figures again:

| View | Columns | Since |
| --- | --- | --- |
| `v_lead` | Per shipped issue: `issue`, `merged`, `ready`, `lead_seconds` and `runs`, the work runs it took. Shipped is a `merged` event, or a work run whose outcome was `merged` | 1 |
| `v_daily` | Per UTC day: `day`, `shipped`, `runs` ended, of them `failed`, their `minutes` and `tokens` (input, output and cache read) | 1 |
| `v_versions` | Per tehas and pi-rukas version pair of work runs that ended: `tehas_version`, `rukas_version` (its commit where it has no version), `runs`, `pr_share`, `handoff_share`, `parked_share`, `failed_share`, `median_minutes` and `median_tokens`, the last of the runs that reported usage, NULL where none did | 1 |

## The report

`tehasbot status` writes the ten-day report to `~/tehas/live/analytics.json`
at each poll, and the plain dashboard shows it as "Last 10 days" (see
[Operating](/operating#3-what-happens-next)). It holds aggregates only: no
issue number, title, failure cause, account, model or provider. Its keys are
listed on [Commands](/tools#tehasbot-status-and-the-console).

The same report in a terminal:

```
tehas analytics report
```

See [tehas analytics](/tools#tehas-analytics).

### Gate

`tehas analytics report` has a `gate` key, and the terminal a `Gate` line, that
say what the runs of the product's gate cost, so the goal of a gate that
answers in minutes is read from the report and not counted by hand over the
streams on the host. Of the work runs that recorded the gate it gives `median`
and `p90` seconds for one run of it, `perRun` -- the mean number of them a run
made -- and `share`, the mean of the shares of those of them that recorded a
length of their own (a run that named no time of its own is in `n`, `seconds`,
`median` and `p90` but in no `share`). `n` is the runs counted and `seconds`
their gate's whole time; a run
that recorded no gate is in none of the figures, as a run that reported no
usage is in no token figure, and with no such run the key is absent altogether
and the terminal prints no line. The report reads its figures from
`runs.gate_runs` and `runs.gate_ms`, the two columns `tehasbot cycle` fills from
the cycle's stream as the run ends; the rows of `run_gate_runs` (see
[the tables](#run_gate_runs)) are the per-run detail beside them, for a reader
who fetches a copy of the database and wants one row per run of the gate. The
terminal prints the same four figures in its seconds format, the whole of them as
a count: `Gate median 4m30s, p90 9m, 1.667 runs per run, 46.7% of run time  (n=3, 20m)`.

One run of the gate of a run that made several is that run's recorded gate time
divided across them, because the stream gives no wall time of its own: the
runner reports the gate's own milliseconds, and a run of it that waited for
another to free the container's CPU share does not count that wait. A cycle that
ran the gate once says the same either way.

The line is the report's, not the dashboard's file: `tehasbot status` publishes
only the top-level keys it names and `gate` is not one of them (see [the keys of
the published report](/tools#tehasbot-status-and-the-console)).

### Drivers

`tehas analytics report` has a `drivers` key, and the terminal a `Drivers` table,
that compare the drivers over the same ten days of work runs. It has one object
for `classic` and one for `lean`, both always there. A run with no driver on
record is classic. Each object gives:

| Key | What it says |
| --- | --- |
| `n` | The work runs of the driver, refused and stopped ones with them |
| `ran` | Those of `n` that ran: not refused, not stopped. Neither says anything of the driver |
| `prShare` | The share of `ran` that ended in a pull request a person could merge: outcome `pr` or `merged` |
| `parkedShare` | The share of `ran` handed to a person: outcome `handoff` or `parked`, as `handoffRate` counts them, since a run tehas parks with its pull request open has outcome `handoff` |
| `minutes` | `n` runs of `ran` with a length, their `total` and `median`, whole minutes |
| `tokens` | `n` runs that reported usage, their `total` and `median` (input, output and cache read) |
| `pullRequests` | `n` issues whose runs reached a pull request, how many of them `merged`, and that `share` |

A figure with nothing to work from is left out. An empty token column is not a
zero: lean reports no usage yet, so its `tokens` is `{"n": 0}` and the terminal
prints `- (n=0)`. Where the count is short of the runs the terminal adds it after
the total.

The comparison is on pull requests, not on merges by the driver, because lean
does not merge. A pull request counts once per issue, however many runs followed
it up, since a merge is on record per issue. A merge is a `merged` event, or a
work run that ended merged, at or before the report's `to`. It goes to one
driver: the one whose run last reached a pull request before it. So when lean's
pull request is closed and classic's merges, the merge is classic's. Events stay
once per issue, so the second merge of a reopened issue has no event: a pull
request lean left there reads as not merged, though a classic run that ended
merged would count.

The table is the report's, not the dashboard's file: like `gate`, `drivers` is not
among the keys `tehasbot status` publishes.

### Refine

The report has a `refine` key, and the terminal a `Refine` line, that say how
much `tehas:refine` is used and whether its issues get built. It counts the
refining sessions started in the window (planning runs with `how` `refine`) and
follows the issues they filed: how many started a cycle and how many shipped,
and the lead time of those that shipped, from their `ready` event. Beside them
are the other issues made ready in the window, how many started and shipped,
to compare. The issues a refining session filed are listed in
`~/tehas/state/refined.jsonl`, one line each, `{at, source, filed, url}`,
written when the confirm files them. The file keeps its last 500 lines, as
`sizes.jsonl` does. Like the rest of the report it holds counts only.

### Size

`tehas analytics report` has a `sizes` key, and the terminal a `Size` line,
that say how issues of each size fare, so the threshold of the dispatcher's hold
(`TEHAS_SIZE_MAX_FILES`, see [Instance settings](/host#instance-settings)) can
be set from outcomes rather than guesswork. It reads what each approved start
measured of its issue's body from `~/tehas/state/sizes.jsonl` (see the layout
table of [The host](/host#layout)): one line per start, `{at, issue, files,
tests, deliverables, done_when}`, the line of an issue with the later `at`
winning and a line that does not parse being skipped, the file keeping its last
500 lines. For the starts measured in
the window it gives three buckets of the `files` count -- 1-2, 3-4 and 5+, and
`none` for the bodies that named no file at all -- and for each of them how many
issues there were, how many shipped, and the median lead time of those that
shipped. No ledger is four buckets of nothing. Of what a line holds only `at`,
`issue` and `files` are read, so the other counts and anything a hand-written
line might carry stay in the ledger. A line the reader will not take is no line:
one of a hundred thousand bytes or more, one whose `at` falls outside 2025 or
2100, or one whose `files` is not a whole number of at most nine digits, the
bound `TEHAS_SIZE_MAX_FILES` itself may not pass. A ledger that is not a plain
file, or one past a MiB, is no ledger, and `sizes.jsonl` keeps only its last 500
lines so a long-lived host does not reach that. Like the rest of the report it
holds counts only: no issue number, and nothing of what the issue asked for.

Two windows meet in one bucket. `n` counts the starts the ledger measured in
the window. A ship is the issue's `v_lead` row, and only a merge at or after
the measured start counts as one, so an issue reopened and measured again after
an earlier cycle of it merged is not a ship of this window. The view's own
window is the wider one, so an issue whose first accepted `ready` lies before
these ten days ships without a lead time: where a bucket's `leadTime.n` falls
short of its `shipped`, that many ships brought no lead. Read the median only
where the two counts agree.

The key belongs to the report a person reads, not to the file the dashboard
shows: `tehasbot status` publishes only the top-level keys it names, and
`sizes` is not one of them (see [the keys of the published
report](/tools#tehasbot-status-and-the-console)).

## Sessions

```
analytics sessions [--out <directory>]
```

The program `~/tehas/libexec/analytics` prints the runs of the last 30 days as
JSON Lines, for the MCP server. It is not a `tehas` command. Line 1 is
`{"api":1,"sessions":1,"days":30,"generatedAt":"..."}`. Each line after it is
one session, newest first: the run's columns in camelCase (`queueWait`,
`failureKind`, `rukasVersion`), times as ISO 8601 UTC, `reviewHeld` and
`providerOutage` as booleans, a key left out where the value is empty, its
`steps` and `roles`, and `logs`: the name and size of each log file the run
left in `~/tehas/logs` (`transcript`, `err`, `preflight`, `console`, `result`
and `dispatch` for a work run, `progress`, `result` and `dispatch` for a
planning turn). A log belongs to a run by the time in its name. The
failure cause is as stored, which is masked. A planning session also has
`filed`, `[{"issue":N,"url":"..."}]`, the issues that turn filed, read from
`~/tehas/state/refined.jsonl` (see [Refine](#refine)): the lines whose `source`
is the session's issue and whose `at` falls from its start to two minutes after
its end. It is left out when there are none. The command exits 70 when the
database cannot be read.

With `--out` the lines go to `<directory>/sessions.jsonl` instead of standard
output, mode 600 in a directory of mode 700, made from a name of this run and
moved into place: the modes are the program's, not the caller's umask. This is
how `tehasbot status` asks for the snapshot (see [The
host](/host#what-is-in-the-home)).

Before reading the runs the command compares the database with a line it leaves
beside the file, `<directory>/sessions.jsonl.stamp`, mode 600: `runs=<count>;
ended=<latest end>;day=<YYYY-MM-DD>`. When that line is what the database says
now, the day of the window has not moved and the file is there, the command
writes neither the file nor the line and leaves both mtimes alone. The line is
left away while any run has no end, because the logs of a live run grow with no
row changing to say so, and every poll of one writes again. A missing file, a
missing line or a line of other words each writes both again.

## Outcomes

```
analytics outcomes [--tehas <version>] [--rukas <version>]
                   [--since <day or time>] [--until <day or time>]
                   [--by version|week] [--out <directory>]
```

The same program prints the counts behind the MCP tool `factory_outcomes`
as JSON Lines, as far back as the database reaches: 365 days, where the
sessions above reach 30. It is not a `tehas` command. Line 1 is
`{"api":1,"outcomes":1,"file":"outcomes.jsonl","days":365,
"by":"version|week","generatedAt":"..."}`. Each line after it is one group, and a group holds
only counts: `runs`, and the maps `outcomes`, `failureKinds` and
`parkReasons` from each word to how many runs of the group carry it. Grouped
by version (the default) a group is a pair of `tehasVersion` and
`rukasVersion`, the most runs first, and a run that recorded neither is one
group with neither key. Grouped by `week` a group is `week`, the Monday that
opens the UTC week, `YYYY-MM-DD`, newest first. A run is counted on its end,
and a run that never ended on its start. An outcome the schema does not allow,
or none at all, counts under `unknown`. The counting is the database's: one
row per group of the window leaves SQLite, not one per run of it, which is what
keeps the whole-year scan short enough for the poll that asks for it.

Nothing else is in the file: no issue number, no title, no failure cause and
no log content. `--tehas` and `--rukas` ask for one release each, matched
exactly, and group with them, so under `--by week` they answer to nothing: a
week is no release, and a week's counts are not cut between the releases that
made them. A run that recorded no version belongs to no release, so it
answers to neither and joins no entry of a filtered call. `--since` and
`--until` bound the days counted, each as `YYYY-MM-DD` (its own start) or a
full UTC time `YYYY-MM-DDTHH:MM:SSZ`, and read as far as themselves. A day is
the stricter of the two shapes: a date the calendar does not hold, such as
2026-02-30, is an error as a day, and is read as its parts as a full time, as
the snapshot's lines read theirs. `--by`
is `version` or `week`. An option given twice, or a value that is no word or
no time, is an error. The command exits 70 when the database cannot be read.

With `--out` the lines go to `<directory>/outcomes.jsonl` instead of standard
output, mode 600 in a directory of mode 700, made from a name of this run and
moved into place, as the sessions' `--out` does. It is skipped and written by
the same line beside it, `<directory>/outcomes.jsonl.stamp`, as the snapshot
is: the two have a line each, so neither answer speaks for the other file.

`tehasbot status` writes what the unfiltered command prints to
`~/tehas/mcp/outcomes.jsonl`, mode 600 in a 700 directory, beside the
snapshot and never under `live/`, which is public. Only `factory_outcomes`
reads it (see [the MCP server](/mcp-server)).

## Fetching it

```
tehas analytics export copy.db --json
```

writes a consistent copy with SQLite's `VACUUM INTO`, mode 600, and prints
`{"api":1,"schema":3,"from":...,"to":...,"runs":N}`, read from the copy. It
takes no lock a cycle holds and makes no network call. It never replaces a
file. A file name may not start with `-`: write `./-name` for one that does.
See [the factory API](/api#tehas-analytics-export) for the fields.

`tehas analytics report` and `export` open the database as the other
commands do, so either may make an empty one where there is none, or move a
broken file or one of another schema version aside, as [The file](#the-file)
says.

A program fetches it the way it runs any factory command, then copies the file:

```
ssh boss@tehas-tehas '. ~/tehas/env.sh && tehas analytics export /tmp/analytics-$(date +%F).db --json'
scp boss@tehas-tehas:/tmp/analytics-2026-10-19.db .
```

or with `incus exec` or `kubectl exec` in the same way. Remove the copy on the
host afterwards.

The copy holds no token, URL with a host or address: `failure_cause` is masked
when it is written, a pull request is kept as its number, and no other column
holds free text. A test opens an export and checks every text column of every
table for them.

What was agreed with tehas-industry (tehas #377, tehas-industry #26):

- Industry's `archive` job fetches the copy daily per factory, beside
  `tehas export`, and upserts it into its own store on `runs.id`,
  `(run_id, step, round)`, `(run_id, role)`, `(issue, event)` and
  `samples.at`. A fetch repeated is harmless, so there is no cursor.
- It fetches the copy before it runs `tehas update --to` on a factory, so a
  release that raises the schema version loses nothing. A file the release does
  not migrate is kept aside under its version for 365 days as well.
- Events stay once per issue. Industry's long-term copy keeps what is older
  than 365 days.

`tehas api` lists the feature `analytics` in a release that has the command.

## Turning it off

In the instance's `instance.env`:

```
TEHAS_ANALYTICS=off
```

Every command reads it the next time it runs. `off` stops every write, and `tehasbot status` removes
`live/analytics.json`, so the dashboard's section goes. `tehas analytics` says
analytics is off and exits 69. The database file is left as it is, and pruning
stops with it: to have the data gone, delete `~/tehas/state/analytics.db` and
its `-wal` and `-shm`. `on` is the default; anything else stops every command
with exit 78.
