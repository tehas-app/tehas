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

What exists today: [`tehas api`](#tehas-api),
[`tehas status --json`](#tehas-status---json), [`tehas limit`](#tehas-limit),
[`tehas drain`](#tehas-drain) and [`tehas:all-hands`](/operating#all-hands). The design of the rest is in
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

## tehas api

```
tehas api [--json]
```

```json
{
  "api": 1,
  "tehas": "v0.8.0",
  "commit": "cb6912a",
  "repo": "janit/tehas",
  "features": ["status", "fixer-grant", "all-hands", "limit", "drain", "refresh-conflicts"],
  "fixer": null
}
```

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `tehas` | The release tag the factory was installed from; `null` when the install came from a checkout with no tag, or not from a git checkout |
| `commit` | The commit it was installed from, seven characters; `null` when not from a git checkout |
| `repo` | `TEHAS_REPO`, the product repository |
| `features` | What this release has, so a program can tell a factory that predates a feature from one where it failed. Today `status`, `fixer-grant`, `all-hands`, `limit`, `drain` and `refresh-conflicts` (the factory refreshes its own pull requests that conflict, [refreshes](/operating#refreshes)) |
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
| `demand.waiting` | Open issues that would start if there were room: `tehas:ready` applied last by an account in `TEHAS_ALLOWED_LABELERS` or the fixer, with no cycle and no planning session open, and the refresh requests waiting, unless refreshes are held |
| `demand.oldestWait` | Seconds since the oldest of them was labelled or asked for; left out when none waits |
| `demand.refreshHold` | Why refresh requests start nothing on this host, for a person: refreshes are off, or the sandbox image cannot continue a pull request. Left out when nothing holds them or there are none |
| `demand` | `null` when no poll counted it in the last ten minutes, or the last poll could not read the queue |
| `hold.why` | One word for what holds new cycles back: `pause`, `slice`, `unreadable`, `resources`, `limit`, `count`, `awaiting`, `pressure`, `room` or `full`. Treat it as an open list: later releases add words |
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
