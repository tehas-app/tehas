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
[the MCP server](/mcp-server), behind API keys. It is read only. A key gives
what the dashboard shows, plus the sessions of the last 30 days and their
logs, masked: see [what it shows](/mcp-server#what-it-shows). The commands here
remain the way to change anything.

What exists today: [`tehas api`](#tehas-api),
[`tehas status --json`](#tehas-status---json), [`tehas limit`](#tehas-limit),
[`tehas drain`](#tehas-drain), [`tehas history`](#tehas-history),
[`tehas export`](#tehas-export),
[`tehas state`](#tehas-state),
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
sudo ./bootstrap.sh root <user>   # packages, Docker, the docker group, linger, the slice, then the pinned tools as <user>
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
  "features": ["status", "millwright-grant", "fixer-grant", "all-hands", "limit", "drain", "history", "export", "refresh-conflicts", "analytics", "state-move"],
  "millwright": null,
  "fixer": null,
  "poll": {
    "last": {
      "at": 1761400000,
      "wallMs": 24000,
      "cpuMs": 3100,
      "gh": 12,
      "writes": 0,
      "starts": 0,
      "kinds": {"label": 0, "comment": 0, "edit": 0, "other": 0},
      "ageMin": 4
    },
    "hour": {"polls": 14, "cpuMs": 41000, "writes": 2}
  }
}
```

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `tehas` | The release tag the factory was installed from: `v0.10.2` for a release install, `v0.8.0` for a checkout one. `null` when the install came from a checkout with no tag, or not from a git checkout |
| `commit` | The commit it was installed from, seven characters; `null` for a release install, which records only the tag, and when not from a git checkout |
| `repo` | `TEHAS_REPO`, the product repository |
| `features` | What this release has, so a program can tell a factory that predates a feature from one where it failed. Today `status`, `millwright-grant` ([the millwright's grant](/operating#the-millwrights-grant)), `fixer-grant`, `all-hands`, `limit`, `drain`, `history` ([`tehas history`](#tehas-history)), `export` ([`tehas export`](#tehas-export)), `refresh-conflicts` (the factory refreshes its own pull requests that conflict, [refreshes](/operating#refreshes)), `analytics` ([`tehas analytics export --json`](#tehas-analytics-export)) and `state-move` ([`tehas state`](#tehas-state)) |
| `millwright` | `TEHAS_MILLWRIGHT_LOGIN`, or `null` when the instance has no millwright |
| `fixer` | The same as `millwright`. It and the feature `fixer-grant` are the old names, kept for as long as API version 1; a program reads `millwright` and `millwright-grant` |
| `poll` | What the polls of this factory cost, or `null` when no poll is on record or the state store cannot answer. `last` is the poll that most recently held the dispatcher's lock and `hour` is the sums of the polls that ended in the last hour; both are in the fields below, so a program that keeps a copy of them can add the cost of many hosts up. See [What a poll costs](/operating#what-a-poll-costs) for what each figure counts |

`poll` in two objects, in milliseconds and in whole numbers:

| Field | Meaning |
| --- | --- |
| `last.at` | The second the poll ended, Unix seconds. Its age is `ageMin`, so a program never subtracts two times |
| `last.wallMs`, `last.cpuMs` | The wall time of the poll, and its CPU time with that of the commands it waited for, both in milliseconds |
| `last.gh` | The `gh` calls the poll itself made; those of the commands it runs are not in it |
| `last.writes` | The writes the host's `gh` shim logged while the poll ran, or `null` on a host with no write log, which is a host with no GitHub App. `null` is not 0 |
| `last.kinds` | Those writes by kind: `label`, `comment`, `edit`, `other`. `null` with no write log, as `writes` is |
| `last.starts` | The cycles that poll started, 0 when it started none; `null` when the row keeps no answer, which is a poll of an older tehas and is not 0 |
| `last.ageMin` | Whole minutes since the poll ended, counted from the second `tehas api` asked the store, and 0 for a poll that has not ended |
| `hour.polls`, `hour.cpuMs` | How many polls ended in the last hour and their CPU time added up, both 0 when none did |
| `hour.writes` | Their writes added up, or `null` when none of them had a write log |

It reads the instance settings, `~/tehas/docs/version.txt`, which
`install.sh` writes, and the state store, which it reads the way `tehas
status` does: a store that is busy or missing is `"poll": null`, not a
failure of the command. Exit 0; 64 on an argument it does not know; 78 when the
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
| `demand.waiting` | Open issues that would start if there were room: `tehas:ready` applied last by an account in `TEHAS_ALLOWED_LABELERS` or the millwright, with no cycle and no planning session open, and the refresh requests waiting, unless refreshes are held |
| `demand.oldestWait` | Seconds since the oldest of them was labelled or asked for; left out when none waits |
| `demand.refreshHold` | Why refresh requests start nothing on this host, for a person: refreshes are off, or the sandbox image cannot continue a pull request. Left out when nothing holds them or there are none |
| `demand` | `null` when no poll counted it in the last ten minutes, or the last poll could not read the queue |
| `hold.why` | One word for what holds new cycles back: `pause`, `slice`, `unreadable`, `resources`, `limit`, `red`, `count`, `awaiting`, `pressure`, `room` or `full`. Treat it as an open list: later releases add words |
| `hold.text` | The same for a person. Do not parse it |
| `hold` | `null` when nothing holds |
| `room.cycles` | Free slots under `TEHAS_TICKET_CONCURRENCY`. Below 0 when more cycles run than it allows, as after it was lowered |
| `room.refreshes` | Free slots of the refresh lane, `TEHAS_REFRESH_SLOTS`; left out when the factory does not refresh its pull requests |
| `room.maxAwaiting` | Room under `TEHAS_MAX_AWAITING`, the cycles running and pull requests waiting for review together. Below 0 when more wait than it allows. Left out when the last poll did not need to read it |
| `room.slice` | Cycles `tehas.slice` has room for |
| `room.limit` | Room under the limit: its `cycles` less the cycles running, 0 when the limit expired or cannot be read; left out without a limit. The others never count the limit |
| `room.slots` | The counts in force when `TEHAS_TICKET_CONCURRENCY` or `TEHAS_SLICE_CONTAINERS` is `auto`, and why. Left out when both are numbers, which need no telling |
| `room.slots.cycles` | The most cycles side by side the poll decided by, whatever the setting says |
| `room.slots.containers` | The containers `tehas.slice` holds at once, as the poll worked them out |
| `room.slots.cyclesHow` | `auto` when the cycles are the poll's count, `number` when the instance wrote one, `fixed` when the setting is neither |
| `room.slots.containersHow` | The same for the containers |
| `room.slots.cycle` | How the cycles were derived; left out when `cyclesHow` is not `auto` |
| `room.slots.container` | How the containers were derived; left out when `containersHow` is not `auto` |
| `room.slots.cycle.bound` | The one word for what gave the count: `cpu`, `memory`, `ceiling` or `host`. Treat it as an open list |
| `room.slots.cycle.slots` | What the host's readings give before the bounds are put on them; 0 when no reading answered |
| `room.slots.cycle.cpus` | The host's CPUs as `nproc` answered, `null` when it answered nothing |
| `room.slots.cycle.cycleCpus` | `TEHAS_CYCLE_CPUS` as written: the CPUs one cycle is given |
| `room.slots.cycle.containerMemory` | `TEHAS_CONTAINER_MEMORY` as written: the memory one container is given |
| `room.slots.cycle.ceiling` | `TEHAS_CYCLE_CEILING` as written, or its default: the most cycles the endpoint may serve |
| `room.slots.cycle.floored` | `true` when no reading answered and the count is the floor of its setting: 1 cycle, 2 containers |
| `room.slots.container.bound` | The same for the containers: `memory`, `ceiling` or `host` |
| `room.slots.container.slots` | The containers the slice's `MemoryMax` holds at `TEHAS_CONTAINER_MEMORY`, before the bounds |
| `room.slots.container.cpus` | The host's CPUs, read for the line the two counts are shown in; `null` when `nproc` answered nothing |
| `room.slots.container.cycleCpus` | `TEHAS_CYCLE_CPUS` as written, named beside the pair so a person reads both numbers of one derivation together |
| `room.slots.container.containerMemory` | `TEHAS_CONTAINER_MEMORY` as written |
| `room.slots.container.ceiling` | `TEHAS_CYCLE_CEILING` as written, or its default |
| `room.slots.container.floored` | `true` when the memory or the size was no reading and 2 containers are in force |
| `room` | `null` when no poll got as far as the room check in the last ten minutes, as while the factory is paused |

`demand.waiting` is not the exact count of what would start: the poll still
checks, at a start, that the text is unchanged since the label, that no open
pull request closes the issue, and the millwright's grant.

The source holds a JSON Schema of this object, `api/status.schema.json`, and
two samples made by the code that prints it, `api/status.busy.json` and
`api/status.quiet.json`. Its tests keep the three in step, and a program can
copy the samples into its own tests.

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
   {"cursor": "1760700000-12-41", "kind": "work", "issue": 12,
    "started": "2026-10-17T11:20:00Z", "ended": "2026-10-17T13:20:00Z",
    "status": 10, "outcome": "handoff", "pr": "https://github.com/acme/widget/pull/9",
    "step": "review", "cap": "awaiting-human-merge", "parkReason": "merge",
    "failureKind": "review", "failureCause": "findings open",
    "provider": false, "parked": true, "repeat": 1, "by": "alice", "how": "label"},
   {"cursor": "1760700300-7-42", "kind": "plan", "issue": 7,
    "started": "2026-10-17T13:20:00Z", "ended": "2026-10-17T13:25:00Z",
    "status": 0, "outcome": "finished", "turn": 3, "by": "alice",
    "label": "tehas:refine", "filed": ["https://github.com/acme/widget/issues/41"]}],
 "next": "1760700300-7-42"}
```

What has ended, oldest first: one line for each cycle and each planning turn,
in the order the state store wrote them.
`kind` is `work` for a cycle and `plan` for a planning turn, so a program reads
the two the same way. `repeat` is on the park of a run of parks only: the parks
in a row that stopped exactly as this one did, this one not counted, so 1 on the
first repeat. A line of a listing that begins partway through a run counts only
the parks the listing holds. It reads the history of both from the state store,
and nothing else: no network, no lock a running cycle holds, and it writes
nothing. A line it cannot read is left out, not an error. A store that does
not answer prints nothing: never a listing without the cycles.

`cursor` is the seconds of the end, the issue, and the line's place in the
order the store wrote the ends, one number across cycles and planning turns,
as `1760700000-12-41`. Keep `next` and pass it as `--after` to read what was
written since: each line is then offered once, also one that ended in the same
second as a line already read, and every line has a cursor of its own. The
cursor of v0.13 and before, `1760700000-12`, is still taken, and read as it
was: the lines after it on the seconds and the issue. A program that kept one
is given the new shape as `next` once there is a line to list. `--after` takes
a cursor this command printed: one shaped any other
way is a bad argument rather than an empty listing, so a cursor that was
mangled in transit is said instead of read as "nothing has ended". `--issue`
takes one issue's lines, from its cycle and its planning turns together.

`ended` and `started` are when the unit ran and ended. `status` is its exit
status, `outcome` what it reported, and the rest are the fields of the line:
the step and cap it stopped at, why it was parked, the failure the millwright named,
whether its provider was asked, the pull request, and who asked for it and how
(`hand` for a cycle run by hand, which names no account).
A planning turn has `turn`, its number in the session, `label`, the label that
started its session (`tehas:plan` or `tehas:refine`), `auto`, true for a
refine the factory started on its own when a cycle parked as underspecified or
too-large, and `filed`, the issues the turn filed, when it filed any. Its `outcome` is `finished`, `failed`,
`interrupted`, `not-started`, `not-readable`, or `lost` for a turn whose
session pi-rukas no longer had; `status` is there when the turn's unit left an
exit status. A field the line does not have is left out,
never null.

Without `--json`, one line each, oldest first.

Exit 0 when it printed, also with nothing to list; 1 when the state store was
busy, which prints nothing; 64 on a bad argument, which reads nothing; 78
without an instance or without a state store that can be used.

## tehas export

```
tehas export [--json] --after CURSOR <dir>
```

```json
{"api": 1, "cycles": [{"cursor": "1760700000-12-41", "kind": "work", "issue": 12,
  "ended": "2026-10-17T13:20:00Z", "status": 0, "outcome": "merged"}],
 "next": "1760700000-12-41", "to": "/home/boss/archive"}
```

The copy a person reads afterwards. For every line `tehas history` gives after
`CURSOR`, it writes `<dir>/cycles/<cursor>/`: the cycle's `result.json`, its
`logs/`, its agents' `transcripts/`, its Protolab page under `protolab/`, and
`history.json`, the line as `tehas history` publishes it. A planning turn brings
`plan.json` and no cycle files: the session as the state store has it now
(`plan-show`), with `issue`, `session`, `turns`, `options`, `pending`,
`pending_outcome`, `failed`, `running`, `started_at`, `comment_id`, `lost`,
`by`, `label`, `brief`, `turn_rc`, `opened_at`, `ended_at` (times in Unix
seconds), `answers`, `filed`, `answer` and `notes`. A session that ended stays
on the store until the next `tehas:plan` of the issue replaces it, so a turn
that never made a session still brings the one it ended; a turn whose session
was replaced since, by one opened after the turn ended, brings no `plan.json`. A planning session the store did not
give is a cycle not written, and so is one with a file that cannot be read:
the others are copied, and the command exits 1. A cycle whose logs the improvement loop has already removed is not an
error: those files are simply not there.

The console does the copying, and every file goes through its mask, the one
masker tehas has, with the same names of the host a cycle is given (the host
aliases, the model hosts and the bot's email), so a token or a private address
that reached a log does not reach the copy. Only a file of a Protolab page that
is not UTF-8, such as an image, is copied as it is; a log is masked whatever
bytes it holds. A file over 32 MiB is left out: an export is for reading. The mask catches accidents;
it cannot stop what no pattern knows, so an export is still a copy of what a
cycle wrote.

`--after` is required, so a program that keeps the `next` it was given and calls
again copies each cycle once. `<dir>` is required and may not be the factory's
home, a directory in it, or a link to one, which is where the dashboard and the
units look for the originals; it is made if it is not there, readable by anyone,
so the person who opens the archive can be someone else.

Exit 0 when it copied, also when there was nothing to copy; 64 on a bad
argument, an export into the factory's home, or a directory it cannot make,
which copies nothing; 78 without an instance or when the state store is not
installed or cannot be used, which copies nothing; 1 with no console to mask
with, or when the store stayed busy for the history, which copies nothing.

## tehas state

```
tehas state pack [--rehearse] -
tehas state unpack [--rehearse] -
```

Moves a stopped factory's state to another machine. `pack` writes one tar to
standard output; `unpack` reads it from standard input, so a program pipes the
one into the other:

```
set -o pipefail
ssh <source> '. ~/tehas/env.sh && tehas state pack -' |
  ssh <target> '. ~/tehas/env.sh && tehas state unpack -'
```

The tar holds `tehas-state.json` first,
`{"api": 1, "tag": "v0.17.0", "schema": 9}`, then `state/`, `repos/`, `live/`
and `mcp/` of the home. The tag is the release in use, read as `tehas api`
reads it, and the schema is the state store's. The store is not copied as a
file: `state/tehas.db` in the tar is a copy the store made of itself, so what
is still in its write-ahead log is in it, and neither `tehas.db-wal` nor
`tehas.db-shm` is packed. Nothing from `logs/`, `work/` or `~/.config/tehas`
is, and neither is `state/github`, the live token: the target mints its own,
and `unpack` keeps the one it has.

`pack` refuses, with exit 1 and nothing on standard output, while a cycle or a
planning turn runs (it names the issue, read from the state store), when the
store cannot say whether a cycle runs, or while any `tehas-*` unit that
`systemctl --user` reports as active, activating, deactivating or reloading
is there (it names the unit). `--rehearse` skips those refusals, to size or
try a pack on a running factory, and carries on past files that change or
cannot be read while tar reads them. It changes nothing in the factory, but
its tar is not a consistent copy of one, so its manifest says
`"rehearsal": true` and `unpack` refuses it (exit 1, nothing written) unless
given `--rehearse`. `unpack --rehearse` replaces the target's state exactly as
`unpack` does: the old state is gone afterwards, so use it only on a machine
that will be rebuilt. On an unmarked tar it is a normal unpack.

`unpack` refuses, with exit 1 and nothing written, while a `tehas-*` unit is
active (asked again just before the swap); when the tar's release or schema is
not this host's (it names both); when `tehas-state.json` is missing, is not
the first entry or has an `api` it does not know; when tar itself will not
extract the archive (a `..` in a name, for one); when anything but the
manifest and the four directories comes out, or one of them is a link; when a
link inside them has an absolute target or one that leaves its directory (a
relative link that stays in its directory, as a git checkout has, is kept);
when a link has a `..` after a name (`..` may only lead a target); when a
device, pipe or socket comes out, or a setuid, setgid or world-writable entry;
and when the unpacked store is missing or does not pass `store check`. It reads the tar into a directory
beside the home, extracts and checks it there, and only then replaces the
four directories, so a refused or broken tar leaves the factory as it was. If
a move of the swap fails, what moved is put back; if a put-back fails the
directory holding the rest is kept and the error names it. It starts no unit.
SIGTERM, SIGHUP or SIGINT removes the directory it made, except once the
swap has begun.

`live/` is a bind mount of the docs container, so after `unpack` restart it:
`. ~/tehas/env.sh && docker compose -f ~/tehas/edge/docs.compose.yaml up -d --force-recreate`.

`unpack` needs room beside the home for the archive and for the extracted tree
at once, which is about twice the size of the state.

`pack` finishes the tar in a scratch directory and sends it only when tar has
ended well (under `--rehearse`, also when tar only saw files change), so a
`pack` that fails sends nothing and `unpack` has nothing to replace the state
with. Run the pipe with `set -o pipefail`, or the pipe's status is `unpack`'s
alone. After a failed pipe the source's files are untouched; run it again. Its
units stay as they were, which is down: `pack` needs them stopped, and nothing
here starts them. Stop and start a factory with the steps of
[Backups and restore](/operating#backups-and-restore) (drain, stop the
`tehas-*` timers and paths, and start them again afterwards). The scratch tar
needs room for the whole of it in the source's temporary directory, which
under `ssh` is `~/tehas/tmp` when `.bashrc` is read and `/tmp` when it is not.

Neither makes a call to GitHub, Docker or the network. Exit 0 when done; 1 on a
refusal or a failure, said on standard error; 64 on a bad argument; 78 without
an instance, a release or a state store that can be used.

## tehas analytics export

```
tehas analytics export <file> [--json]
```

```json
{"api": 1, "schema": 1, "from": 1792022400, "to": 1792800000, "runs": 42}
```

Writes a consistent copy of the factory's analytics database to `<file>`, mode
600: 365 days of runs, their steps and roles, issue events and poll samples.
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
