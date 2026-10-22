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
| A planning turn, as a run of kind `plan` | the dispatcher, `lib/planning.sh` | As the turn ends. No token usage: pi's rpc mode does not report it |
| The issue events `ready` and `started` | the dispatcher | `ready` with the time `tehas:ready` was accepted, `started` as a cycle starts |
| The issue event `ended` | `tehasbot cycle` | As a cycle ends |
| The issue events `awaiting`, `merged`, `closed` and `stopped` | `tehasbot status` | The first time a run of it sees the issue in that state, by comparing with its last snapshot |
| A sample | the dispatcher | At the end of each poll: about every two minutes, and also when a cycle ends. Some 720 rows a day, 263000 in 365 days |
| `meta` | the program | When it makes the file |

Each event is stored once per issue, the first time. A second run of an issue
adds no event; every run is in `runs`.

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

`PRAGMA user_version` is the schema version, today 1. Within one version
columns are only added. A change of a column's meaning raises the version.
There are no migrations:

- **A file that is not a database**, or fails SQLite's check, is moved to
  `analytics.db.broken` and a new one is made. The data is at most 365 days deep.
- **A database of another schema version** is moved, with its side files, to
  `analytics.db.schema-<version>`, where a reader can still fetch it, and a new
  one is made. It is deleted 365 days after its last write.
- Any other failure, a full disk or a lock held too long, leaves the file as
  it is.

A move is said once on standard error, in the log of the command that met it.

The schema is `console/analytics/src/schema.sql` in the source, with a pinned
copy `schema.pinned` beside it that a test compares it with. A reader may pin
that copy too. A change to the schema is agreed with tehas-industry first, as
for the JSON of [the factory API](/api).

## The tables

Every column below came in schema 1. A column without a value is NULL.

### runs

One row per run, written when the run ends. A work run reads what the cycle
left on the host: its history line in `state/cycles/<n>.history.jsonl`, the
headless runner's result, `progress.json`, `settings`, `approved.json`, the
image's pi-rukas commit, the preflight log and `docs/version.txt`. A run that
has no result (killed, stalled, timed out) is stored with tehas's status and
its times. A planning turn reads its record `state/plan-<n>.json`.

| Column | Source | Since |
| --- | --- | --- |
| `id` | `<kind>-<issue>-<started>`, unique | 1 |
| `kind` | `work` or `plan` | 1 |
| `issue` | The issue number | 1 |
| `attempt` | The run's number for this issue: its line in the history, counting from 1. The history keeps 50 lines, so it is at most 50. Work runs only | 1 |
| `how` | What started it: `label`, `retry-tick`, `outage-retry` or `fixer`, from `approved.json`, else the history line. A planning turn: `label` for the first turn, `refine` for the first turn of a session `tehas:refine` started, `answer` or `fixer` for a later one | 1 |
| `started`, `ended`, `seconds` | The run's start, its end and the wall time between: the history line's `started` and `at`. A planning turn ends when the poll collects it | 1 |
| `queue_wait` | `started` less when `tehas:ready` was accepted (`approved.json`) | 1 |
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
| `image` | The sandbox image's id, `sha256:<hex>` | 1 |
| `input`, `output`, `cache_read`, `cache_write`, `turns`, `dispatches`, `cost` | The result's `usage`. For a planning turn `turns` is its turn in the session, and the rest is NULL | 1 |
| `oom_kills`, `forks_refused` | Processes killed for memory and forks refused in the container, as `tehasbot cycle` reads them | 1 |
| `concurrent` | How many other cycles ran when it started, from `settings` | 1 |
| `runner_ms` | The result's `durationMs`, in milliseconds | 1 |
| `result_schema` | The pi-rukas result's schema number, 3 or 4 | 1 |

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
| `model`, `provider` | The role's model and provider in the run's settings snapshot (`state/cycles/<n>.settings`) | 1 |
| `dispatches`, `turns`, `input`, `output`, `cache_read` | The result's `usage.byRole`, else `progress.json` | 1 |
| `cache_write`, `cost` | The result's `usage.byRole` | 1 |
| `ms` | The role's time, in milliseconds, from `usage.byRole` | 1 |

The key is `(run_id, role)`.

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

`key` and `value`: `schema`, `retention_days` (365) and `created`. No host
name, address or token. Since schema 1.

## The views

So that a reader does not have to work out the common figures again:

| View | Columns | Since |
| --- | --- | --- |
| `v_lead` | Per shipped issue: `issue`, `merged`, `ready`, `lead_seconds` and `runs`, the work runs it took. Shipped is a `merged` event, or a work run whose outcome was `merged` | 1 |
| `v_daily` | Per UTC day: `day`, `shipped`, `runs` ended, of them `failed`, their `minutes` and `tokens` (input, output and cache read) | 1 |
| `v_versions` | Per tehas and pi-rukas version pair of work runs that ended: `tehas_version`, `rukas_version` (its commit where it has no version), `runs`, `pr_share`, `handoff_share`, `parked_share`, `failed_share`, `median_minutes` and `median_tokens` | 1 |

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
analytics sessions
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

## Fetching it

```
tehas analytics export copy.db --json
```

writes a consistent copy with SQLite's `VACUUM INTO`, mode 600, and prints
`{"api":1,"schema":1,"from":...,"to":...,"runs":N}`, read from the copy. It
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
  release that raises the schema version loses nothing. The old file is kept
  aside under its version for 365 days as well.
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
