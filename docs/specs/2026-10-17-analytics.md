# Spec: analytics, ten days of factory data in SQLite

**Status**: Agreed with the owner 2026-10-17. Nothing is built; the build is #318.

## Purpose

We change tehas and pi-rukas every day and cannot tell whether a change made
the factory better. A cycle's facts are spread over `state/cycles/`, `logs/`
and comments on GitHub, and most of them are gone by the next cycle of the same
issue. Nothing counts done, failed or repeated work, and nothing ties a result
to the tehas and pi-rukas versions that produced it.

This spec adds a feedback loop:

1. Each factory records every run it makes, with the versions it ran, in one
   SQLite database. Data is kept for at most ten days.
2. The dashboard shows a ten-day report that can be read at a glance.
3. A program can fetch the database and do deeper analysis elsewhere, for
   example in tehas-industry.

## Goals

1. **One row per run, with both versions.** Every work cycle and planning turn
   records the tehas version and the pi-rukas version and commit it ran with.
   The main question is "was the last release better than the one before",
   and this is what answers it.
2. **Enough facts to tell why.** Duration and step times, tokens, turns, the
   step it stopped in, the cap, the failure kind, whether a retry or the
   fixer started it, and whether the model endpoint was on its backup.
3. **At most ten days.** Rows older than ten days are deleted. The database
   never grows past what ten days of runs produce.
4. **Never in the way.** If a write fails, the run goes on as if analytics
   were off. Analytics never changes a cycle's outcome, label or comment.
5. **Fetchable.** One command writes a consistent copy of the database. The
   schema is documented and versioned, so another program can read it.
6. **A public-safe report.** The dashboard gets aggregates only. Anything
   that may not be public stays in the database.

## Non-goals

- Long-term history. Industry keeps the archive, from `tehas history`,
  `tehas export` and the analytics copy. Ten days on the host is enough to
  compare the last few releases.
- A network API. The database is not served under `/live/`. It is fetched
  with a command, as the factory API spec (`2026-10-17-factories.md`) says.
- Cost accounting in money. Models are self-hosted. Tokens, minutes and,
  where the mesh reports it, energy stand in for cost.
- Analytics about the product, such as what changed in which file. This is
  about how the factory works.
- Replacing #244. The state database planned there holds the factory's
  working state. This database holds measurements. See "Relation to other
  work".

## What exists today

Checked in the repository on 2026-10-17.

- There are no counters and no aggregates. `health.json` counts failures in
  the last 10 endings. `status.json` lists the last 50 issues closed by a
  merged pull request.
- `state/cycles/<n>.history.jsonl` (written at `libexec/cycle:684`) keeps the
  last 50 cycle endings per issue: `at`, `started`, `rc`, `status`, `step`,
  `cap`, `park_reason`, `failure_kind`, `failure_cause`, `provider`, `parked`,
  `pr`, `by`, `how`. Nothing reads it yet.
- The pi-rukas result (schema 3) has `durationMs`, `steps[]` with start and
  end times, and `usage` with `input`, `output`, `cacheRead`, `cacheWrite`,
  `cost`, `turns`, `dispatches` and `byRole`. `tehas_headless_outcome`
  (`lib/headless.sh`) drops the durations, the step times, `cacheWrite` and
  `cost`. The raw file is overwritten by the issue's next cycle.
- The pi-rukas version comes from three places: the result's `buildVersion`
  and `buildCommit`, the image label `tehas.rukas` (saved per cycle in
  `state/cycles/<n>.rukas`), and the `--capabilities` answer's `version`,
  which is fetched every cycle and not saved.
- The tehas version is in `~/tehas/docs/version.txt` as `vX.Y.Z+<sha7>`,
  written by `install.sh`. `live/platform.json` has the platform checkout's
  commit.
- A cycle on the backup model endpoint leaves only a line in
  `logs/work-*.jsonl.preflight`.
- Planning turns run `pi --mode rpc`, not the headless runner. They have no
  result and no usage. Their record `state/plan-<n>.json` is removed when the
  session ends.
- `sqlite3` is not installed by `bootstrap.sh`. The Rust console is built on
  every host by `install.sh`.

## What to measure

The metrics come from software delivery (DORA, flow metrics) and from
manufacturing. The factory is a production line: an issue is a work order,
a cycle is one pass through the line, a step is a station, and a park or
handoff is a part taken off the line.

| Metric | Manufacturing / software name | What it is here |
| --- | --- | --- |
| Shipped | Throughput | Issues closed by a merged factory pull request, per day |
| Lead time | Lead time (DORA, flow) | From the accepted `tehas:ready` to the merge |
| Cycle time | Cycle time | From the start of a run to its end |
| Queue wait | Wait time | From the accepted `tehas:ready` to the first start |
| First-pass yield | FPY | Shipped issues that needed exactly one run |
| Runs per shipped issue | Rework rate | All runs divided by shipped issues |
| Station yield | Rolled throughput yield | Per step: runs that got through it divided by runs that reached it |
| Stop reasons | Pareto of defects | Failure kinds, caps and park reasons, by count |
| Availability | OEE availability | Share of time the factory could take work: not paused, not held, model reachable |
| Utilisation | Capacity use | Busy cycle-minutes divided by the cycle-minutes the limit allowed |
| Work in progress | WIP | Running, in line and in review, sampled |
| Tokens and minutes per shipped issue | Unit cost | Total tokens and machine minutes divided by shipped issues |
| Energy per shipped issue | Energy per unit | kWh from the mesh divided by shipped issues, where the mesh reports it |
| Handoff rate | Change failure rate (loosely) | Runs that ended needing a person |

DORA's change failure rate and time to restore need to know which merged
change broke something later. tehas does not know that, so they are left out.

With ten days of data the numbers are small. Every figure in the report shows
its count (`n`), and the version comparison shows nothing for a version with
fewer than three finished runs.

## Design

### The database

`$TEHAS_HOME/state/analytics.db`, mode 0600, WAL. It is not under `live/` and
not served. Times are UTC Unix seconds, durations are seconds, as integers.
`PRAGMA user_version` is the schema version. Within one version columns are
only added. A release that changes a column's meaning raises the version.

It is a file of its own, separate from the `state/tehas.db` planned in #244.
It has a different lifetime (ten days), a different reader (a program
elsewhere) and different contents (measurements, not state). Fetching it must
not copy the factory's working state.

Tables:

**`runs`**: one row per run, written when the run ends.

| Column | From |
| --- | --- |
| `id` | `<kind>-<issue>-<started>`, unique |
| `kind` | `work` or `plan` |
| `issue` | issue number |
| `attempt` | the run's number for this issue: earlier lines in `history.jsonl`, plus one |
| `how` | `label`, `retry-tick`, `outage-retry` or `fixer` (`approved.json`) |
| `started`, `ended`, `seconds` | the cycle's start, end and wall time |
| `queue_wait` | `started` minus when `tehas:ready` was accepted (`approved.json`) |
| `outcome` | `pr`, `merged`, `handoff`, `parked`, `failed`, `refused`, `stopped` |
| `status`, `exit_code` | the runner's status and exit code, or tehas's own (`killed`, `stalled`, `timeout`) |
| `last_step`, `failed_step` | from the result |
| `cap`, `park_reason`, `review_held` | from the result and progress |
| `failure_kind`, `failure_cause` | from the result; the cause cut to 200 characters and masked as the console masks |
| `provider_outage` | the run stopped because the model provider was out |
| `endpoint` | `main` or `backup` |
| `pr` | pull request number, parsed from the URL |
| `tehas_version`, `tehas_commit` | `version.txt` at the run's start |
| `rukas_version`, `rukas_commit` | the result's `buildVersion` and `buildCommit`; else the image label |
| `image` | the sandbox image id |
| `input`, `output`, `cache_read`, `cache_write`, `turns`, `dispatches`, `cost` | the result's `usage` |
| `oom_kills`, `forks_refused` | as `libexec/cycle` reads them |
| `concurrent` | how many other cycles ran when it started |

**`run_steps`**: `run_id`, `step`, `round`, `started`, `ended`, `seconds`. From
the result's `steps[]`, else from `step-started` lines in the NDJSON.

**`run_roles`**: `run_id`, `role`, `model`, `provider`, `dispatches`, `turns`,
`input`, `output`, `cache_read`, `ms`. Usage is from `usage.byRole`. The model
and provider are from the run's settings snapshot (`state/cycles/<n>.settings`).

**`issue_events`**: `at`, `issue`, `event`. Events are `ready` (accepted),
`started`, `ended`, `awaiting`, `merged`, `closed`, `stopped`. Lead
time and first-pass yield come from here. The dispatcher writes `ready` and
`started`. The cycle writes `ended`. The status run writes `awaiting`,
`merged`, `closed` and `stopped` the first time it sees each one, by comparing
with the last snapshot.

**`samples`**: one row per dispatcher poll (every 2 minutes, about 7200 rows
in ten days): `at`, `in_line`, `running`, `awaiting`, `stopped`, `planning`,
`limit` (cycles allowed), `paused`, `held` (the hold reason word, or empty),
`watts` (the mesh draw, where `energy.json` has it). `planning` counts open
issues labelled `tehas:plan` or `tehas:planning`, which `status.json` does not
list today; `libexec/status` adds that count.

**`meta`**: `key`, `value`. Holds `schema`, `retention_days` (10) and
`created`. No host name, address or token.

The database also defines a few views, so that a reader does not need to
re-derive the common figures: `v_daily` (per day: shipped, runs, failed,
minutes, tokens), `v_lead` (per shipped issue: lead time, runs), and
`v_versions` (per tehas and pi-rukas version pair: runs, share by outcome,
median minutes, median tokens).

### Who writes what

All SQL is in one small Rust program, `analytics`, in the `console/` Cargo
workspace beside the console. It uses rusqlite with the bundled SQLite, so a
host needs no `sqlite3`. It follows #244's rule that bash never writes SQL.
`install.sh` builds it and installs it as `~/tehas/libexec/analytics`, the
same way it installs the console. The bash commands call it:

| Command | Called by | Does |
| --- | --- | --- |
| `analytics run <issue>` | `libexec/cycle`, after the history line | Reads the raw `result.json`, `progress.json`, `settings`, `approved.json`, `.rukas`, the preflight log and `version.txt`, and inserts the run, its steps and roles |
| `analytics plan <issue>` | `lib/planning.sh`, when a planning turn ends | Inserts a `plan` run with its times, turns and outcome. No usage until pi's rpc mode reports it |
| `analytics event <issue> <event>` | the dispatcher, the cycle, `libexec/status` | Inserts an issue event, once per issue and event |
| `analytics sample` | the dispatcher, once per poll | Reads `live/status.json`, `live/dispatch.json`, `live/energy.json` and inserts a sample |
| `analytics prune` | the dispatcher, once per poll | Deletes everything older than ten days and runs `PRAGMA optimize` |
| `analytics report --json` | `libexec/status` | Prints the ten-day report |
| `analytics export <file>` | `tehas analytics export` | `VACUUM INTO` a consistent copy |

Every caller runs it with a time limit and ignores its exit status, after a
`WARN` line in its own log. A locked or broken database costs a missing row,
never a cycle. When the file is not a valid database, `analytics` moves it
aside to `analytics.db.broken` and starts a new one: the data is only ten
days deep.

`analytics run` reads the cycle's files before `libexec/cycle` cleans up, so
it is called before the cleanup and after the outcome is known. It adds no
mount and no variable to the cycle's container. It runs on the host, after the
container has gone (rule 4 is not touched).

Retention is checked on every write as well as on every poll: a write never
leaves a row older than ten days behind. A run still going when its start
turns ten days old is kept until it ends, then pruned on the next poll.

### Settings

`TEHAS_ANALYTICS`, `on` (default) or `off`, in the instance's settings,
validated by `lib/config.sh`. `off` stops all writes and removes
`live/analytics.json`. The database file is left as it is, and pruning stops
with it, so a person who turns analytics off and wants the data gone deletes
the file. Retention is fixed at ten days and is not a setting: the owner
asked for a maximum, not a default.

### The dashboard report

`libexec/status` writes `live/analytics.json` from `analytics report --json`
at each poll. A new section, "Last 10 days", on `/dashboard`
(`host/docs/dashboard.frag`) shows it.

At a glance, one row of figures:

- **Shipped**: issues merged, with the day-by-day bars beneath.
- **Planned**: work the factory is to do in the future: open issues in line
  and open issues in planning, now, with the ten-day trend from `samples`.
- **Runs**: total, split into pull request, handoff, parked and failed.
- **First-pass yield**: share of shipped issues done in one run.
- **Lead time**: median and 90th percentile, ready to merged.
- **Cycle time**: median minutes per run.
- **Machine time**: total cycle-minutes, and per shipped issue.
- **Tokens**: total, and per shipped issue (input, output, cache read).
- **Availability**: share of the ten days the factory could take work.
- **Energy**: kWh per shipped issue, only where the mesh reports energy.

Below that:

- A ten-day chart: shipped and failed runs per day, with cycle-minutes as a
  line.
- Top five stop reasons (failure kind, cap or park reason) with counts.
- Station yield: one bar per step, the share of runs that got through it.
- Versions: one row per tehas and pi-rukas version pair that ran, with its
  runs, the share that reached a pull request, median minutes and median
  tokens. A pair with fewer than three runs shows its count only.

`live/analytics.json` holds aggregates only: counts, durations, token sums,
version strings and the vocabulary words of stop reasons. No issue title, no
failure cause text, no account, no model host and no model name. Issue
numbers are already public in `status.json`, but the report needs none. The
test pins its keys, as `top-publish` does for `top.json`.

The game page (`/`) is not changed by this spec.

### Fetching the database

`tehas analytics export <file>` writes a consistent copy with `VACUUM INTO`,
mode 0600, and prints, with `--json`, `{"api": 1, "schema": N, "from": ...,
"to": ..., "runs": N}`. It takes no lock that a cycle holds and makes no
network call. A program fetches the copy the way it runs any factory command:
over SSH, `incus exec` or `kubectl exec`, then copies the file.

The copy is masked before it is written: `failure_cause` is already masked on
insert, and nothing in the schema holds a token or an address. A test opens an
export and checks that no column holds a value that looks like a token, a URL
with a host, or an address.

`tehas api` lists the feature `analytics`. The schema is described in
`host/docs/analytics.md`, with each column, its source and the version it came
in. A change to the schema is agreed with tehas-industry first, as for the
factory API's JSON shapes.

`tehas analytics report` prints the same report as the dashboard, for a
person in a terminal.

## Relation to other work

- **Factory API (#299-#304).** `tehas history` reads `history.jsonl` as its
  spec says. It is not built on this database, because history must not lose
  a cycle that industry has not fetched yet, and this database forgets after
  ten days. Where the two share a fact, the column names follow history's
  field names (`park_reason` here, `parkReason` there). `tehas export` copies
  files; `tehas analytics export` copies measurements. Industry may use both.
- **#244, the state database.** When it lands, its `state` program and this
  `analytics` program share the SQLite setup code. The files stay separate.
  #257 (retention) does not cover this file; this file prunes itself.
- **The improve loop.** Its `cycle.json` records overlap with `runs`. Once
  this is built, `lib/improve.sh` can read runs from the database instead of
  its own window. That is a follow-up, not part of this spec.

## Tests

The bash side follows rule 6: bash, git, jq and shellcheck only. The bash
tests stub `analytics` with `t_stub` and check that each caller calls it with
the right arguments, at the right moment, and carries on when it fails or
hangs. The Rust tests, beside the source, check the rest:

- `run` reads each fixture result (`test/fixtures/headless-*`) and stores the
  right row, steps and roles, including the fields `tehas_headless_outcome`
  drops.
- A run with no result (stalled, killed, timeout) is stored with tehas's
  status and its times.
- `attempt` counts earlier runs of the same issue; `how` is kept.
- `prune` removes rows older than ten days and keeps a running run's events.
- Each event is stored once per issue.
- A broken file is moved aside and a new database is made.
- `report --json` matches a pinned fixture: its keys, and its figures for a
  known data set, including first-pass yield and the version table's
  three-run floor.
- `export` writes a database that opens, has the same `user_version`, and
  holds no token, host or address.
- A schema change without a raised `user_version` fails a test that compares
  the schema with a pinned copy.

The dashboard section's logic is an ES module with a node test, as
`host-data.js` is.

`analytics` is Rust, so its tests run only where `cargo` is installed, as the
console's do. This is a weaker gate than the bash tests until #246 makes cargo
required. The owner decides whether to wait for #246.

## Manual

- `host/docs/analytics.md`: what is recorded, the schema, how to fetch it,
  how to turn it off.
- `host/docs/operating.md`: `tehas analytics report` and `export`.
- `host/docs/features.md`: analytics as built, not proven, until one real
  cycle on tehas-tehas has appeared in the report and an export has been
  opened on another machine.

## Build order

One pull request each:

1. `analytics` with the schema, `run`, `prune`, `export`, and its tests.
   `install.sh` builds and installs it.
2. `libexec/cycle` calls `run` and `event ended`; the dispatcher calls
   `event ready`, `event started`, `sample` and `prune`. `TEHAS_ANALYTICS` in
   `lib/config.sh`.
3. `libexec/status` writes `awaiting`, `merged`, `closed` and `stopped`
   events and the planning count. Planning turns call `plan`.
4. `report --json`, `live/analytics.json` and the dashboard section.
5. `tehas analytics report` and `export`, the `analytics` feature in
   `tehas api`, and the manual.

## Decided

- One SQLite file, `state/analytics.db`, separate from the planned state
  database.
- Ten days, fixed, pruned by the factory itself.
- Every run records the tehas version and commit and the pi-rukas version and
  commit.
- Written by a Rust program with bundled SQLite; bash calls it and never
  writes SQL.
- The database is fetched with a command, not served over HTTP.
- The dashboard gets aggregates only.
- "Planned" is work for the future: issues in line and issues in planning.
  It is not a count of planning turns.
- Planning turns are recorded without token usage. pi's rpc mode does not
  report usage today; adding it is left for later.
- Not built now. The build is #318.
