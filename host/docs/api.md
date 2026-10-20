---
title: The factory API
description: "The commands a program uses to drive a tehas factory, and the JSON they print."
---

# The factory API

A factory is driven by commands, from a terminal or from a program. A program
runs the same commands a person does, with `--json`, as the factory's user: in
a terminal, over SSH, with `incus exec` or `kubectl exec`. tehas runs no server
that takes commands. There is nothing to authenticate, and getting to the
machine is the caller's business.

An agent that is not on the machine can read how the factory is doing through
[the MCP server](/mcp), behind API keys. It is read only and shows what the
dashboard shows; the commands here remain the way to change anything.

What exists today: [`tehas api`](#tehas-api),
[`tehas status --json`](#tehas-status---json), [`tehas limit`](#tehas-limit),
[`tehas drain`](#tehas-drain), [`tehas history`](#tehas-history),
[`tehas export`](#tehas-export),
[`tehas analytics export`](#tehas-analytics-export) and
[`tehas:all-hands`](/operating#all-hands). The design of the rest is in
the spec, `docs/specs/2026-10-17-factories.md` in the source; a command is
listed here when a release has it.

## Principles

1. **Commands are the API; files are not.** What is under `~/tehas` is
   internal and may change in any release. The JSON the docs server publishes
   under `/live/` is for the dashboard, not for a program.
2. **`--json` prints one JSON object** on standard output and nothing else.
   Messages go to standard error. Without `--json` the same command prints for
   a person.
3. **Every object carries `"api": 1`.** Within version 1 fields are only
   added, never renamed, removed or given a new meaning. A breaking change is
   version 2, and the release that brings it says so.
4. **A field without a value is left out**, never `null`, unless this page
   says the field itself is `null` when unknown. A program accepts fields it
   does not know.
5. **Exit statuses**: 0 done; 1 the command ran and the answer is "no" or "not
   yet"; 64 a bad argument; 78 missing or malformed instance settings.
6. **Reading makes no network call** and takes no lock a cycle holds, so a
   program can call it every few seconds. It reports what the dispatcher's
   last poll saw, with that poll's time.
7. **Setting is safe to repeat.** Running a set command twice with the same
   arguments leaves the same state and exits 0 both times.

## How a program uses it

1. Ask [`tehas api --json`](#tehas-api) once: the API version, and whether
   this release has the feature the program needs.
2. Poll [`tehas status --json`](#tehas-status---json) every minute or so.
   `stale: true` means the dispatcher has not reported for ten minutes.
3. Share model time with [`tehas limit set`](#tehas-limit), always with an
   expiry (`--for`), and set it again before it runs out. If the program
   stops, the limit expires and the factory starts nothing, rather than
   running without a limit.
4. Before an upgrade or a move, [`tehas drain --wait`](#tehas-drain), then
   `tehas limit clear` once it is done.

## Creating a factory

A factory is a machine of its own: its own Docker daemon, its own published
ports, its own Unix user, its own `TEHAS_DOMAIN`. Nothing is shared with
another factory, because sharing any of those puts one factory's containers,
units or names under another's control. What such a machine needs is the list
on [The host](/host#what-a-factory-machine-needs), which is the same list a
person reads.

A program creates one by running the two installers a person runs, in this
order, and asking one question afterwards:

```
sudo ./bootstrap.sh root <user>   # packages, Docker, the docker group, linger, the slice
./bootstrap.sh user               # the pinned tools, as the tehas user
./install.sh --check              # must exit 0 once the instance is in place
```

Neither asks a question, so both run with nothing on standard input. An
instance that will not load stops `install.sh` with exit 78 and one line
naming the setting, and installs nothing; a machine that still owes a
follow-up from an earlier run keeps `--check` at exit 1. So the test of a
factory is not that the commands ran, it is that `install.sh --check` returns
0 twice in a row, and that [`tehas api --json`](#tehas-api) then names the
release, the commit and the repository it was installed for. The steps, what
each one refuses, and what an unattended run has to get past are on
[Installing a factory](/host#installing-a-factory).

## tehas api

```
tehas api [--json]
```

```json
{
  "api": 1,
  "tehas": "v0.8.0",
  "commit": "cb6912a",
  "repo": "tehas-app/tehas",
  "features": ["status", "fixer-grant", "all-hands", "limit", "drain", "history", "export", "refresh-conflicts", "analytics"],
  "fixer": null
}
```

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `tehas` | The release tag the factory was installed from; `null` when the install came from a checkout with no tag, or not from a git checkout |
| `commit` | The commit it was installed from, seven characters; `null` when not from a git checkout |
| `repo` | `TEHAS_REPO`, the product repository |
| `features` | What this release has, so a program can tell a factory that predates a feature from one where it failed. Today `status`, `fixer-grant`, `all-hands`, `limit`, `drain`, `history` ([`tehas history`](#tehas-history)), `export` ([`tehas export`](#tehas-export)), `refresh-conflicts` (the factory refreshes its own pull requests that conflict, [refreshes](/operating#refreshes)) and `analytics` ([`tehas analytics export --json`](#tehas-analytics-export)) |
| `fixer` | `TEHAS_FIXER_LOGIN`, or `null` when the instance has no fixer |

It reads the instance settings and `~/tehas/docs/version.txt`, which
`install.sh` writes. Exit 0; 64 on an argument it does not know; 78 when the
settings are missing or malformed.

## tehas status --json

```
tehas status --json
```

```json
{
  "api": 1,
  "time": "2026-10-17T13:59:40Z",
  "age": 40,
  "stale": false,
  "state": "warn",
  "reasons": ["The factory is paused."],
  "paused": {"local": true, "label": false},
  "allHands": false,
  "limit": {"state": "none"},
  "running": [{"issue": 12, "since": "2026-10-17T11:20:00Z", "step": "review"}],
  "planning": [{"issue": 7, "turn": 2, "since": "2026-10-17T13:47:00Z"}],
  "demand": {"waiting": 2, "oldestWait": 5400},
  "hold": {"why": "pause", "text": "paused by the flag tehas pause wrote: weekend"},
  "room": {"cycles": 0, "slice": 2}
}
```

| Field | Meaning |
| --- | --- |
| `time` | When the health file was written: at the dispatcher's last poll, or when a cycle last ended |
| `age` | Seconds since `time` |
| `stale` | `true` when `time` is more than ten minutes ago. The exit status is then 1 |
| `state` | `pass`, `warn` or `fail`, as the [health file](/tools#tehasbot-health) says it |
| `reasons` | Why the state is not `pass`, for a person; empty on `pass` |
| `paused.local` | The pause flag `tehas pause` writes on this host is set |
| `paused.label` | `tehas:pause` holds the factory from the control issue: it is on the issue, or an account that may not lift it took it off |
| `allHands` | `tehas:all-hands` holds on the control issue: it is on the issue and an allowed account applied it. It changes nothing in this factory. Left out when the last snapshot could not read it or is more than ten minutes old |
| `limit` | The limit, as [`tehas limit --json`](#tehas-limit) prints it without `api` and `reason`: `{"state": "none"}` when none is set |
| `running` | The running cycles: `issue`, `since` (when it started) and `step` (left out when not known yet) |
| `planning` | The planning turns in flight: `issue`, `turn` (the session's turn that runs, from 1) and `since` (when it started; left out for a turn started before this was kept) |
| `demand.waiting` | Open issues that would start if there were room: `tehas:ready` applied last by an account in `TEHAS_ALLOWED_LABELERS` or the fixer, with no cycle and no planning session open, and the refresh requests waiting, unless refreshes are held |
| `demand.oldestWait` | Seconds since the oldest of them was labelled or asked for; left out when none waits |
| `demand.refreshHold` | Why refresh requests start nothing on this host, for a person: refreshes are off, or the sandbox image cannot continue a pull request. Left out when nothing holds them or there are none |
| `demand` | `null` when no poll counted it in the last ten minutes, or the last poll could not read the queue |
| `hold.why` | One word for what holds new cycles back: `pause`, `slice`, `unreadable`, `resources`, `limit`, `red`, `count`, `awaiting`, `pressure`, `room` or `full`. Treat it as an open list: later releases add words |
| `hold.text` | The same for a person. Do not parse it |
| `hold` | `null` when nothing holds |
| `room.cycles` | Free slots under `TEHAS_TICKET_CONCURRENCY` |
| `room.maxAwaiting` | Room under `TEHAS_MAX_AWAITING`, the cycles running and pull requests waiting for review together; left out when the last poll did not need to read it |
| `room.slice` | Cycles `tehas.slice` has room for |
| `room.limit` | Room under the limit: its `cycles` less the cycles running, 0 when the limit expired or cannot be read; left out without a limit. The other three never count the limit |
| `room` | `null` when no poll got as far as the room check in the last ten minutes, as while the factory is paused |

`demand.waiting` is not the exact count of what would start: the poll still
checks, at a start, that the text is unchanged since the label, that no open
pull request closes the issue, and the fixer's grant.

It reads `~/tehas/live/health.json` and nothing else: no network, no instance
settings, and it writes nothing. Without a health file it prints nothing on
standard output and exits 1. Exit 0 on `pass` and `warn`; 1 on `fail`, a stale
file or no file; 64 on an argument other than `--json`.

## tehas limit

```
tehas limit [--json]
tehas limit set --cycles N [--plan on|off] [--for SECONDS | --until TIME] [--reason TEXT]
tehas limit clear
```

```json
{"api": 1, "state": "ok", "cycles": 1, "plan": false, "until": "2026-10-17T14:05:00Z", "reason": "held for alpha"}
```

An outside cap on what the factory starts. Every command prints the limit as
it then is, and `--json` works with the show form only.

| Field | Meaning |
| --- | --- |
| `state` | `none`, `ok`, `expired` or `unreadable` |
| `cycles` | The most cycles running at once. Room for a new cycle is the smaller of `cycles` and `TEHAS_TICKET_CONCURRENCY`, less the cycles running, then the slice and `TEHAS_MAX_AWAITING` as always |
| `plan` | `false`: no new planning session and no improvement run. An answer to a planning question still runs: someone is waiting on it |
| `until` | When it expires; left out when it lasts until cleared |
| `reason` | Shown as the hold's reason. One line of at most 200 characters; control characters become spaces. Left out when empty |

- **An expired or unreadable limit holds every start**: no cycle, planning
  turn, answer or improvement run. `tehas status` warns and says why. A file
  that cannot be read never counts as no limit. The poll still cleans up.
- **Running cycles are never stopped.** A lower limit only holds new starts.
- **Pause is separate.** A paused factory stays paused under any limit.
- `set` replaces the whole limit. The same arguments twice leave the same
  limit, and `clear` without a limit is fine.

Exit 0; 64 on a bad argument (a `--until` in the past included), which
writes nothing; 78 without an instance.

## tehas drain

```
tehas drain [--wait SECONDS] [--json]
```

```json
{"api": 1, "drained": false, "running": [12], "limit": {"state": "ok", "cycles": 0, "plan": false, "reason": "draining"}}
```

The same as `tehas limit set --cycles 0 --plan off --reason draining`, a limit
that does not expire, then with `--wait` waiting up to that many seconds for
the running cycles to end. `running` lists the issues whose cycles still run.
`tehas limit clear` ends it.

Exit 0 when no cycle is running; 1 when some still are at the end of the
wait; 64 on a bad argument; 78 without an instance.

## tehas history

```
tehas history [--json] [--after CURSOR] [--issue N]
```

```json
{"api": 1, "cycles": [
   {"cursor": "1760700000-12", "kind": "work", "issue": 12,
    "started": "2026-10-17T11:20:00Z", "ended": "2026-10-17T13:20:00Z",
    "status": 10, "outcome": "handoff", "pr": "https://github.com/acme/widget/pull/9",
    "step": "review", "cap": "awaiting-human-merge", "parkReason": "merge",
    "failureKind": "review", "failureCause": "findings open",
    "provider": false, "parked": true, "by": "alice", "how": "label"},
   {"cursor": "1760700300-7", "kind": "plan", "issue": 7,
    "started": "2026-10-17T13:20:00Z", "ended": "2026-10-17T13:25:00Z",
    "status": 0, "outcome": "finished", "turn": 3, "by": "alice"}],
 "next": "1760700300-7"}
```

What has ended, oldest first: one line for each cycle and each planning turn.
`kind` is `work` for a cycle and `plan` for a planning turn, so a program reads
the two the same way. It reads the history files the units write and nothing
else: no network, no lock a running cycle holds, and it writes nothing. A line
it cannot read is left out, not an error, so an older or a newer file breaks no
listing.

`cursor` is the seconds of the end and the issue, as `1760700000-12`. Keep
`next` and pass it as `--after` to read what ended since: each line is then
offered once, and two lines that ended in the same second have an order of
their own. `--after` takes a cursor this command printed: one shaped any other
way is a bad argument rather than an empty listing, so a cursor that was
mangled in transit is said instead of read as "nothing has ended". `--issue`
takes one issue's lines, from its cycle and its planning turns together.

`ended` and `started` are when the unit ran and ended. `status` is its exit
status, `outcome` what it reported, and the rest are the fields of the line:
the step and cap it stopped at, why it was parked, the failure the fixer named,
whether its provider was asked, the pull request, and who asked for it and how.
A planning turn has `turn`, its number in the issue. A field the line does not
have is left out, never null.

Without `--json`, one line each, oldest first.

Exit 0 when it printed, also with nothing to list; 64 on a bad argument, which
reads nothing; 78 without an instance.

## tehas export

```
tehas export [--json] --after CURSOR <dir>
```

```json
{"api": 1, "cycles": [{"cursor": "1760700000-12", "kind": "work", "issue": 12,
  "ended": "2026-10-17T13:20:00Z", "status": 0, "outcome": "merged"}],
 "next": "1760700000-12", "to": "/home/boss/archive"}
```

The copy a person reads afterwards. For every line `tehas history` gives after
`CURSOR`, it writes `<dir>/cycles/<cursor>/`: the cycle's `result.json`, its
`logs/`, its agents' `transcripts/`, its Protolab page under `protolab/`, and
`history.json`, the line as `tehas history` publishes it. A planning turn brings
`plan.json` and no cycle files: the session record as that turn left it, which
outlives a session that never came into being, since it is copied to
`~/tehas/state/plan-<issue>.turn` before the record is rewritten or removed. A
turn of a session that went on has no copy of its own, and brings the record as
it stands. A cycle whose logs the improvement loop has already removed is not an
error: those files are simply not there.

Every text file goes through the console's mask, the one masker tehas has, with
the same names of the host a cycle is given (the host aliases, the model hosts
and the bot's email), so a token or a private address that reached a log does not
reach the copy. Only a file the mask itself will not read is copied as it is, and
one over 32 MiB is left out: an export is for reading. The mask catches accidents;
it cannot stop what no pattern knows, so an export is still a copy of what a
cycle wrote.

`--after` is required, so a program that keeps the `next` it was given and calls
again copies each cycle once. `<dir>` is required and may not be the factory's
home, a directory in it, or a link to one, which is where the dashboard and the
units look for the originals; it is made if it is not there, readable by anyone,
so the person who opens the archive can be someone else.

Exit 0 when it copied, also when there was nothing to copy; 64 on a bad
argument, an export into the factory's home, or a directory it cannot make,
which copies nothing; 78 without an instance; 1 with no console to mask with.

## tehas analytics export

```
tehas analytics export <file> [--json]
```

```json
{"api": 1, "schema": 1, "from": 1792022400, "to": 1792800000, "runs": 42}
```

Writes a consistent copy of the factory's analytics database to `<file>`, mode
600: ten days of runs, their steps and roles, issue events and poll samples.
The schema, every column and how a reader keeps up with it are on
[Analytics](/analytics). The copy is masked: it holds no token, no URL with a
host and no address. A program runs the command, then copies the file, over
SSH, `incus exec` or `kubectl exec`, as it runs any other.

| Field | Meaning |
| --- | --- |
| `schema` | The copy's schema version, `PRAGMA user_version`. A program that knows another version does not read it |
| `from` | The oldest start of a run in the copy, Unix seconds; left out when it has no run |
| `to` | The newest end of a run in the copy (its start for a run that has no end), Unix seconds; left out when it has no run |
| `runs` | How many runs the copy holds |

The fields are read from the copy, not from the factory's database, so they
describe the file the program fetches. The command takes no lock a cycle holds
and makes no network call. It never replaces a file: name a new one each time.
A copy fetched again holds the same rows, and more: rows are keyed so that
storing them twice is harmless (see [Analytics](/analytics#fetching-it)).

The feature `analytics` in [`tehas api`](#tehas-api) says the command exists.
It does not say analytics is on: exit 0; 64 on a bad argument; 69 when the
instance has `TEHAS_ANALYTICS=off` or the program is not installed, which
writes nothing; 70 when the database cannot be read; 73 when `<file>` exists or
cannot be made; 78 without an instance.
