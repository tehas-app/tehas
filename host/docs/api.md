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
[`tehas status --json`](#tehas-status---json) and
[`tehas:all-hands`](/operating#all-hands), reported in both. The design of the rest is in
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
  "features": ["status", "fixer-grant", "all-hands"],
  "fixer": null
}
```

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `tehas` | The release tag the factory was installed from; `null` when the install came from a checkout with no tag, or not from a git checkout |
| `commit` | The commit it was installed from, seven characters; `null` when not from a git checkout |
| `repo` | `TEHAS_REPO`, the product repository |
| `features` | What this release has, so a program can tell a factory that predates a feature from one where it failed. Today `status`, `fixer-grant` and `all-hands` |
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
| `limit` | `{"state": "none"}`: this release has no `tehas limit` yet |
| `running` | The running cycles: `issue`, `since` (when it started) and `step` (left out when not known yet) |
| `demand.waiting` | Open issues that would start if there were room: `tehas:ready` applied last by an account in `TEHAS_ALLOWED_LABELERS` or the fixer, with no cycle and no planning session open |
| `demand.oldestWait` | Seconds since the oldest of them was labelled; left out when none waits |
| `demand` | `null` when no poll counted it in the last ten minutes, or the last poll could not read the queue |
| `hold.why` | One word for what holds new cycles back: `pause`, `slice`, `unreadable`, `resources`, `count`, `awaiting`, `pressure`, `room` or `full`. Treat it as an open list: later releases add words |
| `hold.text` | The same for a person. Do not parse it |
| `hold` | `null` when nothing holds |
| `room.cycles` | Free slots under `TEHAS_TICKET_CONCURRENCY` |
| `room.maxAwaiting` | Room under `TEHAS_MAX_AWAITING`, the cycles running and pull requests waiting for review together; left out when the last poll did not need to read it |
| `room.slice` | Cycles `tehas.slice` has room for |
| `room` | `null` when no poll got as far as the room check in the last ten minutes, as while the factory is paused |

`demand.waiting` is not the exact count of what would start: the poll still
checks, at a start, that the text is unchanged since the label, that no open
pull request closes the issue, and the fixer's grant.

It reads `~/tehas/live/health.json` and nothing else: no network, no instance
settings, and it writes nothing. Without a health file it prints nothing on
standard output and exits 1. Exit 0 on `pass` and `warn`; 1 on `fail`, a stale
file or no file; 64 on an argument other than `--json`.
