---
title: Analytics
description: "The factory's own 365 days of runs in SQLite: what is recorded, every column, how to fetch it and how to turn it off."
---

# Analytics

Every factory records each run it makes, with the tehas and pi-rukas versions
it ran, in one SQLite file: `~/tehas/state/analytics.db`. It keeps 365 days.
The question it answers first is "was the last release better than the one
before". A program fetches a copy and does the analysis elsewhere; the MCP
server reads the runs of the last 30 days and the outcomes of the year from
it.

This is about how the factory works, not about the product: nothing in it says
what changed in which file. It is not the visitor analytics of
`TEHAS_SITE_ANALYTICS_SRC`, which counts visits to the pages.

## What is recorded, and when

| What | Written by | When |
| --- | --- | --- |
| A work run, its steps and its roles | `tehasbot cycle` | As the cycle ends, after its history line and before its files are cleaned up |
| A planning turn, as a run of kind `plan` | the dispatcher | As the turn ends. No token usage: the answer tehas keeps from a turn holds none |
| The issue events `ready` and `started` | the dispatcher | `ready` with the time `tehas:ready` was accepted, `started` as a cycle starts |
| The issue event `ended` | `tehasbot cycle` | As a cycle ends |
| The issue events `awaiting`, `merged`, `closed` and `stopped` | `tehasbot status` | The first time a run of it sees the issue in that state, by comparing with its last snapshot |
| A sample | the dispatcher | At the end of a poll that wrote to GitHub, started, stopped or turned anything: about every two minutes on a busy factory, and also when a cycle ends. A poll that changed nothing takes none, so an idle factory writes no row rather than the same reading again. Some 720 rows a day on a working one |
| `meta` | the program | When it makes the file |

Each event is stored once per issue, the first time. A second run of an issue
adds no event; every run is in `runs`.

A stretch with no sample - a paused or held factory, an idle week - is a time
the factory did no work, not a time the dispatcher was down.

The commands run `~/tehas/libexec/analytics`, a Rust program with SQLite built
in, which holds all of the SQL; `install.sh` builds it. Each call from a cycle
or a poll has a limit of 5 seconds and only warns when it fails: a locked or
broken database costs a missing row, never a cycle. Analytics never changes a
cycle's outcome, label or comment. It adds no mount and no variable to a
cycle's container: it runs on the host after the container has gone.

## 365 days

Rows older than 365 days are deleted on every write, and the
file never holds more than 365 days of runs. A run is as old as its end. The
events of an issue that started and has not ended are kept at most a day
longer. 365 days is fixed and is not a setting. A file made when it was ten
days says 365 in `meta` after its next prune; what was deleted before stays
deleted.

Two things a write does not do belong to `analytics prune`: the sweep of the
`analytics.db.schema-<version>` files a migration left beside the database, and
SQLite's `PRAGMA optimize`. Until #1769 the poll ran that command itself about
every two minutes; nothing schedules it now, so both go undone until someone
asks for them:

```
~/tehas/libexec/analytics prune
```

The rows need neither - the writes bound the tables - so this is the command
that clears the aside files of [The file](#the-file) below.
When it cannot delete an expired aside file, or its `-wal` or `-shm`, it
deletes the others, names those it could not in one `WARN` line on standard
error and exits 73.

## The file

`~/tehas/state/analytics.db`, mode 600, in WAL mode, with its `-wal` and `-shm`
beside it. It is not under `live/` and is not served. Times are UTC Unix
seconds and durations are seconds, as integers, except `runner_ms` and
`run_roles.ms`, which are milliseconds.

`PRAGMA user_version` is the schema version, today 7. Within one version
columns are only added. A change of a column's meaning, or a column removed,
raises the version. Schema 5 removed `samples.watts`, which nothing writes
since energy use left core in 2.0. Schema 6 removes `runs.contractor_id`:
outsource was dropped in 2.0, and no release ever wrote a value there.
Schema 7 takes the words of 2.0 for one fact everywhere: `runs.outcome`
says `offered` where it said `pr`, in its CHECK and in every row, `runs.how`
says `approved` where it said `label`, and `v_versions` counts `offered` and
names its share of the runs that reached a change `change_share` (it was
`pr_share`). A program that reads a
copy reads the new words; there is no alias.

A file at 4, 5 or 6 is migrated in place the first time it is opened:
`samples.watts` and `runs.contractor_id` are dropped where the file has them,
and `runs` is laid again with the words of schema 7, its views with it;
every other column and row stays. Two programs
that open it at once do not both migrate it. v0.30.0 wrote 4 already, so this
release migrates no older file: a file at 1, 2 or 3 is kept aside under its
version, as any other version is, and a new one is made.

- **A file that is not a database**, or fails SQLite's check, is moved to
  `analytics.db.broken` and a new one is made. The data is at most 365 days deep.
- **A database of another schema version** is moved, with its side files, to
  `analytics.db.schema-<version>`, where a reader can still fetch it, and a new
  one is made. It is deleted 365 days after its last write. That goes for a
  version newer than the program too: after a rollback to a 1.x release, of
  schema 4, the file at 6 goes aside whole. Move it back by hand before going
  forward again: nothing says it is still aside, and a second rollback renames
  the file then at 6 over it, which loses the runs it held.
- Any other failure, a full disk or a lock held too long, leaves the file as
  it is.

A move is said once on standard error, in the log of the command that met it.

The schema is `console/analytics/src/schema.sql` in the source, with a pinned
copy `schema.pinned` beside it that a test compares it with. A reader may pin
that copy too. A change to the schema is agreed first with the programs that read it, as
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
headless runner's result, `progress.json`, the preflight log,
`state/cycles/<n>.lean-state.json` for a lean run, and `docs/version.txt`. The
lean note is the cycle's copy of the driver's own record of its steps, and its
steps stand for the run's steps in place of the stream's; only a cycle whose
analytics are on keeps one, and the next cycle of the issue removes it as it
starts. A run that has no result (killed, stalled, timed out) is
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
| `how` | What started it: `approved` (a label an allowed account applied), `reply` (a retry a person's reply answered), `answer` (a retry `tehas answer` gave), `outage-retry`, `millwright`, `host` (an approval of `tehas start`) or `refresh`, from the approval the start used, else the history line, which says `hand` for a cycle run by hand. A planning turn: `approved` for the first turn, `refine` for the first turn of a session `tehas:refine` started, `answer` or `millwright` for a later one. A run a retry ticked before 2.0 started said `retry-tick`: analytics rewrites it to `reply` in `runs.how` of `state/analytics.db` each time it opens the file, and the state store's step to schema 27 does the same in `state/tehas.db`. A run recorded under the millwright's old name said `fixer`: analytics rewrites it to `millwright` the same way, and the state store's step to schema 28 takes `fixer` from what its rows may say. A start a label approved said `label` before 2.0: the step to schema 7 rewrites it to `approved` in `runs.how`, and the state store's step to schema 30 in `state/tehas.db` | 1 |
| `started`, `ended`, `seconds` | The run's start, its end and the wall time between: the history line's `started` and `at`. A planning turn ends when the poll collects it | 1 |
| `queue_wait` | `started` less when `tehas:ready` was accepted (the approval the start used) | 1 |
| `outcome` | `offered`, `merged`, `handoff`, `parked`, `failed`, `refused` or `stopped`, below | 1 |
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
| `result_schema` | The pi-rukas result's schema number, 3, 4 or 5 | 1 |
| `gate_runs` | How many runs of the product's gate the cycle made, from its stream. NULL when the stream held none, which is no gate rather than a gate of no time; 0 is not stored, so a run whose rows the runner timed of nothing keeps no count either | 2 |
| `gate_ms` | Their milliseconds in all, as the runner timed each of them: the gate's own time, not the wait for another run of it that held the container's CPU share. A single run of it of more than three runs of `TEHAS_VERIFY_TIMEOUT` is no figure, and NULL with no run of it | 2 |
| `driver` | The driver the start's approval bound: `classic` or `lean`. NULL where the approval holds no driver or another word, on every run from before the column, and on a planning turn. NULL reads as classic | 3 |
| `tool_calls`, `tool_errors` | The result's `usage.toolCalls` and `usage.toolErrors`: the tool calls the run's children started, a codemode script's nested calls included, and those that ended in an error. NULL where the result has none: a pi-rukas before v0.14.66 (`features.usageTools`), a result that could not count them, a planning turn | 4 |
| `result_driver` | The result's `driver`: the engine that ran the issue, `classic` or `lean`, beside `driver`, the one the approval bound. NULL where the result names none or another word | 4 |

The outcome of a work run, the first row that matches, top to bottom.
`tehas_core::outcome::end` decides it:

| Outcome | Meaning |
| --- | --- |
| `merged` | The issue's pull request was merged within the run, and the run did not park. A runner that claimed a merge the report did not find parks, and ends by a later row |
| `stopped` | The issue was closed while it ran, it was interrupted, or `tehas stop` stopped it, and the run did not park |
| `refused` | No cycle ran: the runner refused, or `tehas work` started none |
| `offered` | It left an open change (a pull request) and was not parked. Before 2.0 the word was `pr`: the step to schema 7 rewrites it |
| `handoff` | The runner handed off to a person |
| `failed` | tehas stopped it, the runner aborted, it was lost (it died without a word), or a failure other than a cap stopped it |
| `parked` | Any other end that needs a person, such as a cap |

A run that parked never has `merged`, `stopped` or `offered`.

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
| `tool_calls`, `tool_errors` | As on `runs`, for the role, from `usage.byRole`. NULL where the role's row has none | 4 |

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
before schema 2 holds no rows.

### issue_events

| Column | Source | Since |
| --- | --- | --- |
| `at` | When it happened | 1 |
| `issue` | The issue number | 1 |
| `event` | `ready`, `started`, `ended`, `awaiting`, `merged`, `closed` or `stopped` | 1 |

The key is `(issue, event)`: the first time only. Lead time runs from the
first accepted `ready` to the merge. A recorded time ahead of the host's clock keeps its
row: within an hour of now it is stored at now, and the command says so; a time
further ahead is refused and nothing is stored.

### samples

One row per poll, from the live files the poll wrote: `live/status.json` and
`live/dispatch.json`. A file that is missing or older than its poll gives NULL.

| Column | Source | Since |
| --- | --- | --- |
| `at` | When it was taken; the key | 1 |
| `in_line`, `running`, `awaiting`, `stopped` | The issues queued, running, awaiting a merge and stopped, in `status.json` | 1 |
| `planning` | Open issues labelled `tehas:plan` or `tehas:planning`, in `status.json` | 1 |
| `cycle_limit` | The cycles allowed at once, from `dispatch.json`: the concurrency, or the room under `tehas limit` where that is lower | 1 |
| `paused` | 1 when the factory was paused | 1 |
| `held` | The one word of the dispatcher's hold, empty when nothing held | 1 |

Schema 1 to 4 had `watts` too, the model mesh's draw until energy use left core
in 2.0. Schema 5 drops it.

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
| `v_versions` | Per tehas and pi-rukas version pair of work runs that ended: `tehas_version`, `rukas_version` (its commit where it has no version), `runs`, `change_share` (offered or merged), `handoff_share`, `parked_share`, `failed_share`, `median_minutes` and `median_tokens`, the last of the runs that reported usage, NULL where none did | 1 |

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
`~/tehas/state/refined.jsonl`: one line per issue a refining session filed,
`{at, source, filed, url}`, written when the host files it, the last 500 lines
kept. A session's are the lines whose `source` is the session's issue and whose
`at` falls from its start to two minutes after its end. It is left out when
there are none. A line of another shape, or of a hundred thousand bytes or
more, is skipped, and a file that is not a plain file, or one past a MiB, is
no file. The command exits 70 when the
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
or none at all, counts under `unknown`. A failure kind or park reason that
is recorded but is no word counts under `unrecognised`; one not recorded is
left out of its map. The counting is the database's: one
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
tehas analytics copy --to copy.db --json
```

writes a consistent copy with SQLite's `VACUUM INTO`, mode 600, and prints
`{"api":1,"schema":3,"from":...,"to":...,"runs":N}`, read from the copy. It
takes no lock a cycle holds and makes no network call. It never replaces a
file. A file name may not start with `-`: write `./-name` for one that does.
See [the factory API](/api#tehas-analytics-copy) for the fields.

`tehas analytics copy` opens the database as the other
commands do, so it may make an empty one where there is none, or move a
broken file or one of another schema version aside, as [The file](#the-file)
says.

A program fetches it the way it runs any factory command, then copies the file:

```
ssh boss@tehas-tehas '. ~/tehas/env.sh && tehas analytics copy --to /tmp/analytics-$(date +%F).db --json'
scp boss@tehas-tehas:/tmp/analytics-2026-10-19.db .
```

or with `incus exec` or `kubectl exec` in the same way. Remove the copy on the
host afterwards.

The copy holds no token, URL with a host or address: `failure_cause` is masked
when it is written, a pull request is kept as its number, and no other column
holds free text. A test opens an export and checks every text column of every
table for them.

What was agreed with the program that archives the factories' runs (tehas #377):

- Its `archive` job fetches the copy daily per factory, beside
  `tehas export`, and upserts it into its own store on `runs.id`,
  `(run_id, step, round)`, `(run_id, role)`, `(issue, event)` and
  `samples.at`. A fetch repeated is harmless, so there is no cursor.
- It fetches the copy before it runs `tehas update --to` on a factory, so a
  release that raises the schema version loses nothing. A file the release does
  not migrate is kept aside under its version for 365 days as well.
- Events stay once per issue. Its long-term copy keeps what is older
  than 365 days.

`tehas api` lists the feature `analytics` in a release that has the command.

## Turning it off

In the instance's `instance.env`:

```
TEHAS_ANALYTICS=off
```

Every command reads it the next time it runs. `off` stops every write, and
`tehasbot status` removes the MCP server's files under `~/tehas/mcp/`. `tehas analytics` says
analytics is off and exits 78. The database file is left as it is, and pruning
stops with it: to have the data gone, delete `~/tehas/state/analytics.db` and
its `-wal` and `-shm`. `on` is the default; anything else stops every command
with exit 78.
