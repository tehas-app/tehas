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
[`tehas order`](#tehas-order),
[`tehas drain`](#tehas-drain), [`tehas freeze`](#tehas-freeze-and-tehas-thaw),
[`tehas thaw`](#tehas-freeze-and-tehas-thaw), [`tehas history`](#tehas-history),
[`tehas export`](#tehas-export),
[`tehas state`](#tehas-state), [`tehas state check`](#tehas-state-check),
[`tehas state copy`](#tehas-state-copy),
[`tehas state load`](#tehas-state-load),
[`tehas analytics export`](#tehas-analytics-export),
[`tehasbot clean`](#tehasbot-clean), [`tehas disk`](#tehas-disk),
[`tehas panel`](#tehas-panel),
[`tehas:all-hands`](/operating#all-hands),
[the finish ask](#the-finish-ask), [the host bridge](#the-host-bridge) and
[the settings rules](#settings-rules). The design of the rest is in
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
   yet"; 64 a bad argument; 78 missing or malformed instance settings; 141
   standard output closed before the command finished printing (as in
   `tehas status | head -1`), with nothing on standard error, as a shell
   command SIGPIPE ended would.
6. **Reading makes no network call** and takes no lock a cycle holds, so a
   program can call it every few seconds. It reports what the dispatcher's
   last poll saw, with that poll's time.
7. **Setting is safe to repeat.** Running a set command twice with the same
   arguments leaves the same state and exits 0 both times.

## How a program uses it

1. Ask [`tehas api --json`](#tehas-api) once: the API version, and whether
   this release has the feature the program needs.
2. Poll [`tehas status --json`](#tehas-status---json) every minute or so.
   `stale: true` means the dispatcher has not reported for ten minutes:
   what it says may be that old.
3. Share model time with [`tehas limit set`](#tehas-limit), always with an
   expiry (`--for`), and set it again before it runs out. If the program
   stops, the limit expires and the factory starts nothing, rather than
   running without a limit.
4. Hand over the order work starts in with [`tehas order set`](#tehas-order).
   With `--only`, an issue the program leaves out waits. The order stays on
   disk through restarts until it is set again or cleared; give `--for` only
   when an order should lapse if the program stops setting it.
5. Before an upgrade or a move, [`tehas drain --wait`](#tehas-drain), then
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
  "features": ["status", "millwright-grant", "fixer-grant", "all-hands", "limit", "drain", "history", "export", "refresh-conflicts", "analytics", "state-move", "mcp-dashboard-keys", "clean", "disk", "panels", "state-check", "finish", "status-versions", "store-copy", "state-load", "freeze", "host-bridge", "cycle-writes", "settings-rules", "start-order", "host-message", "refresh-pr", "host-message-health", "refresh-pr-starts", "refresh-pr-lane", "refresh-pr-json"],
  "millwright": null,
  "fixer": null,
  "refreshScan": false,
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
| `features` | What this release has, so a program can tell a factory that predates a feature from one where it failed. Today `status`, `millwright-grant` ([the millwright's grant](/operating#the-millwrights-grant)), `fixer-grant`, `all-hands`, `limit`, `drain`, `history` ([`tehas history`](#tehas-history)), `export` ([`tehas export`](#tehas-export)), `refresh-conflicts` (the factory refreshes its own pull requests that conflict, [refreshes](/operating#refreshes)), `analytics` ([`tehas analytics export --json`](#tehas-analytics-export)), `state-move` ([`tehas state`](#tehas-state)), `mcp-dashboard-keys` (the read-only MCP server answers a key of `TEHAS_MCP_DASHBOARD_KEYS` with only the four tools that copy from `live/`, [the MCP server](/mcp-server)), `clean` ([`tehasbot clean`](#tehasbot-clean)), `disk` ([`tehas disk`](#tehas-disk)), `panels` ([`tehas panel`](#tehas-panel)), `state-check` ([`tehas state check`](#tehas-state-check)), `finish` ([the finish ask](#the-finish-ask)), `status-versions` (`versions` in [`tehas status --json`](#tehas-status---json)) `store-copy` ([`tehas state copy`](#tehas-state-copy)), `state-load` ([`tehas state load`](#tehas-state-load)), `freeze` ([`tehas freeze` and `tehas thaw`](#tehas-freeze-and-tehas-thaw)) `host-bridge` ([the host bridge](#the-host-bridge)), `cycle-writes` ([`tehas disk --cycles`](#tehas-disk) names what each running cycle writes), `settings-rules` (the release carries [its settings rules](#settings-rules)), `start-order` ([`tehas order`](#tehas-order)), `host-message` ([`tehas host-message`](#tehas-host-message)), `refresh-pr` ([`tehasbot refresh --pr N`](/tools-github#tehasbot-refresh) asks for the refresh of one pull request), `host-message-health` (health, `tehas status`, the MCP server and the dashboard show [the host messages](#tehas-host-message)), `refresh-pr-starts` (the dispatcher starts a refresh `tehasbot refresh --pr N` asked for while `TEHAS_REFRESH_CONFLICTS` is not 1, [refreshes](/operating#refreshes)), `refresh-pr-lane` (such a refresh runs in the refresh lane, not in a cycle slot, [refreshes](/operating#refreshes)) and `refresh-pr-json` ([`tehasbot refresh --pr N --json`](/tools-github#tehasbot-refresh) prints what it asked as one object, and `refreshScan` is in this answer) |
| `millwright` | The app's login, or `null` before it is known |
| `fixer` | The same as `millwright`. It and the feature `fixer-grant` are the old names, kept for as long as API version 1; a program reads `millwright` and `millwright-grant` |
| `refreshScan` | `true` when the timed scan of `tehas-refresh.timer` looks for conflicting pull requests: `TEHAS_REFRESH_CONFLICTS` is 1. `false` when its runs exit at once, and a refresh starts only when `tehasbot refresh --pr N` asks for one. Whether the timer is enabled is not read |
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
  "running": [{"issue": 12, "since": "2026-10-17T11:20:00Z", "step": "lens-review",
               "stepNumber": 7, "stepTotal": 9, "round": 2, "stepSince": "2026-10-17T13:41:00Z"}],
  "planning": [{"issue": 7, "turn": 2, "since": "2026-10-17T13:47:00Z"}],
  "demand": {"waiting": 2, "oldestWait": 5400},
  "hold": {"why": "pause", "text": "paused by the flag tehas pause wrote: weekend"},
  "held": [],
  "room": {"cycles": 0, "slice": 2},
  "versions": {"platform": {"release": "v0.25.0", "latest": "v0.26.0", "behind": true},
               "rukas": {"version": "v0.14.54", "latest": "v0.14.54", "behind": false}},
  "prototypes": [{"issue": 21, "builtAt": "2026-10-17T09:12:00Z"}]
}
```

| Field | Meaning |
| --- | --- |
| `time` | When the health file was written: at the dispatcher's last poll, or when a cycle last ended |
| `age` | Seconds since `time` |
| `stale` | `true` when `time` is more than ten minutes ago. The exit status is then 1 |
| `state` | `pass`, `warn` or `fail`, as the [health file](/tools-health#tehasbot-health) says it |
| `reasons` | Why the state is not `pass`, for a person; empty on `pass` |
| `paused.local` | The pause flag `tehas pause` writes on this host is set |
| `paused.label` | `tehas:pause` holds the factory from the control issue: it is on the issue, or an account that may not lift it took it off |
| `readOnly` | `true` on a read-only instance (`TEHAS_READ_ONLY=1`), which writes nothing to GitHub (see [read-only](/operating#read-only)). Left out on any other instance, and by a release before the setting |
| `allHands` | `tehas:all-hands` holds on the control issue: it is on the issue and an allowed account applied it. It changes nothing in this factory. Left out when the last snapshot could not read it or is more than ten minutes old |
| `limit` | The limit, as [`tehas limit --json`](#tehas-limit) prints it without `api` and `reason`: `{"state": "none"}` when none is set |
| `order` | The start order, as [`tehas order --json`](#tehas-order) prints it without `api`, or `{"state": "unreadable"}` when the store did not answer. Left out by a factory that predates `start-order` |
| `running` | The running cycles: `issue`, `since` (when it started) and `step`, and where the step is: `stepNumber` (the step the cycle is on, from 1, as the driver counts it), `stepTotal` (how many steps the cycle has), `round` (the round of a step that has run before in this cycle, from 2, or the round the driver gave), `roundLimit` (the most rounds the step may run, when the driver gives a limit) and `stepSince` (when the step started). Each is left out when not known yet. `stepNumber`, `stepTotal` and `roundLimit` are there only when the driver puts them on its `step-started` lines |
| `planning` | The planning turns in flight: `issue`, `turn` (the session's turn that runs, from 1) and `since` (when it started; left out for a turn started before this was kept). A session whose last turn ended in error and waits for a person is not in flight and is not here; the text summary has a line for it |
| `demand.waiting` | Open issues that would start if there were room: `tehas:ready` applied last by an account in `TEHAS_ALLOWED_LABELERS` or the millwright, with no cycle and no planning session open, and the refresh requests waiting, unless refreshes are held |
| `demand.oldestWait` | Seconds since the oldest of them was labelled or asked for; left out when none waits |
| `demand.labelAlone` | The issues `tehas:blocked` holds by the label alone, by number, at most 50: the label counts, and no comment of an allowed labeller or the millwright names their blockers, so they never start until one does or a person takes the label off. A marker in the issue's body is not read. Left out when there are none |
| `demand.refreshHold` | Why refresh requests start nothing on this host, for a person: refreshes are off and a request the scan wrote waits (one `tehasbot refresh --pr N` wrote still starts), or the sandbox image cannot continue a pull request. Left out when nothing holds them or there are none |
| `demand` | `null` when no poll counted it in the last ten minutes, or the last poll could not read the queue |
| `hold.why` | One word for what holds new cycles back: `pause`, `slice`, `unreadable`, `resources`, `limit`, `red`, `count`, `awaiting`, `pressure`, `room` or `full`. Treat it as an open list: later releases add words |
| `hold.text` | The same for a person. Do not parse it |
| `hold` | `null` when nothing holds |
| `held` | The ready issues the last poll held one by one while it had room, each with its own reason; empty when none. An issue here starts nothing until a person clears its hold, so do not show it as next |
| `held.issue` | The issue number |
| `held.why` | One word for the reason: `parked` when its last cycle parked and its body has not changed since. Treat it as an open list |
| `held.text` | The reason for a person. Do not parse it |
| `held.clear` | What a person does to release it, such as editing the body or ticking Retry on the park comment |
| `room.cycles` | Free slots under `TEHAS_TICKET_CONCURRENCY`. Below 0 when more cycles run than it allows, as after it was lowered |
| `room.refreshes` | Free slots of the refresh lane, `TEHAS_REFRESH_SLOTS`; left out when no lane is kept: `TEHAS_REFRESH_SLOTS=0`, or `TEHAS_REFRESH_CONFLICTS` off with no `tehasbot refresh --pr N` request waiting and no refresh running |
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
| `versions` | The platform and the pi-rukas this factory runs, as the [health file](/tools-health#tehasbot-health) last read them in `checks.platform` and `checks.rukas`. Always there: a value that cannot be read is `null`, never left out, so a program tells "unknown" from a release without the field. The feature `status-versions` of [`tehas api`](#tehas-api) says a factory has it |
| `versions.platform` | The tehas release: `release`, `latest` and `behind` |
| `versions.platform.release` | The release the host runs, such as `v0.25.0`; `null` on a host that runs a checkout, or with no platform reading |
| `versions.platform.latest` | The newest published release, without release candidates; `null` when it is not known, and on a checkout host |
| `versions.platform.behind` | `true` when `latest` is newer than `release`; on a checkout host, `true` when the checkout is behind its upstream. `null` when it cannot be told |
| `versions.rukas` | The pi-rukas cycles run: `version`, `latest` and `behind`. All three `null` on a host with no pi-rukas reading |
| `versions.rukas.version` | The version the sandbox image's pi-rukas answers, such as `v0.14.53`; `null` when it is not known |
| `versions.rukas.latest` | The version of the pi-rukas commit the host follows (`PI_RUKAS_FOLLOW`); `null` when that commit is no release, as when the host follows a branch |
| `versions.rukas.behind` | `true` when the image is not on the commit the host follows, compared by commit, so it can be `true` while both versions are `null` |
| `prototypes` | The issues with a [Protolab](/tools-protolab#tehasbot-protolab) page, from `live/protolab.json` as the Protolab last wrote it. Left out when there is no such list, as on a host that does not run the Protolab |
| `prototypes.issue` | The issue number |
| `prototypes.builtAt` | When its page was last built; left out for a page whose time cannot be read |

Everything in the object is as the health file wrote it at `time`, and is
`age` seconds old. The dispatcher writes it at each poll, every two minutes,
and when a cycle ends, so `running` and `room` can miss a cycle that started
since: it is in neither until the next poll. They can be up to ten minutes
old before `stale` is true. A program that acts on a free slot or an idle
issue reads `age` first.

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

`api/limit.schema.json` in the source says what the object holds, and
`api/limit.sample.json`, `api/limit.expired.json`, `api/limit.empty.json` and
`api/limit.unreadable.json` are samples of it, one for each `state`. The four
are what the command prints, so an expired limit prints `api`, `state`,
`cycles`, `plan`, `until` and `reason`, and an unreadable one prints only `api`
and `state`.

| Field | Meaning |
| --- | --- |
| `state` | `none`, `ok`, `expired` or `unreadable` |
| `cycles` | The most cycles running at once, a whole number from 0 to 9999: `tehas limit set` refuses anything above it with 64 (console/ops/src/flags.rs). Room for a new cycle is the smaller of `cycles` and `TEHAS_TICKET_CONCURRENCY`, less the cycles running, then the slice and `TEHAS_MAX_AWAITING` as always |
| `plan` | `false`: no new planning session and no improvement run. An answer to a planning question still runs: someone is waiting on it |
| `until` | When it expires, in the form `2026-10-17T15:00:20Z`: 20 characters, the month 01 to 12, the day 01 to 31, the hour to 23, the minute and second to 59, or 60 for the second. Left out when it lasts until cleared. An `until` of another shape makes the limit `unreadable` |
| `reason` | Shown as the hold's reason. One line of at most 200 characters, counted as characters; control characters become spaces. Left out when empty |

- **An expired or unreadable limit holds every start**: no cycle, planning
  turn, answer or improvement run. `tehas status` warns and says why. A file
  that cannot be read never counts as no limit. The poll still cleans up.
- **One reading.** `tehas status`, the dispatcher and the health page read the
  file the same way. A file that names a key twice is `unreadable`.
- **Running cycles are never stopped.** A lower limit only holds new starts.
- **Pause is separate.** A paused factory stays paused under any limit.
- `set` replaces the whole limit. The same arguments twice leave the same
  limit, and `clear` without a limit is fine.

Exit 0; 64 on a bad argument (a `--until` in the past included), which
writes nothing; 78 without an instance.

## tehas order

```
tehas order [--json]
tehas order set --issues N[,N...] [--only] [--for SECONDS | --until TIME] [--reason TEXT] [--json]
tehas order clear [--json]
```

```json
{"api": 1, "state": "ok", "issues": [12, 8], "only": true, "until": "2026-10-17T15:00:20Z", "reason": "held"}
```

The order ready issues start in, kept in the state store (`state/tehas.db`),
so it stays through a restart and moves with `tehas state pack`. With no order,
ready issues start lowest number first. Every command prints the order as it
then is.

`api/order.schema.json` in the source says what the object holds, and
`api/order.sample.json`, `api/order.expired.json` and `api/order.empty.json`
are samples of it, one for each `state`. A store that cannot be read is no
answer: the command exits 1.

| Field | Meaning |
| --- | --- |
| `state` | `none`, `ok` or `expired` |
| `issues` | The issues to start first, in order: distinct numbers from 1, at most 200. A number that is not a ready issue is passed over. Empty with `only`: nothing starts until the order changes |
| `only` | `true`: a ready issue that is not in `issues` waits. Refreshes of open pull requests are not held |
| `until` | When it expires, in the form `2026-10-17T15:00:20Z`. Left out when it lasts until cleared |
| `reason` | One line of at most 200 characters. Left out when empty |

- **An order only sorts.** An issue starts only on a `tehas:ready` or
  `tehas:blocked` label from an allowed account, and every hold applies as
  without an order.
- **An expired order is no order**: ready issues start lowest number first,
  and the poll log says so. So is one the poll cannot read from the store.
- `set` replaces the whole order. The same arguments twice leave the file as
  it was, so a program may set it on every poll.

Exit 0; 1 the file could not be written or removed; 64 on a bad argument,
which writes nothing; 78 without an instance.

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

`api/drain.schema.json` in the source says what the object holds, and
`api/drain.sample.json` and `api/drain.running.json` are samples of it: nothing
running, and one cycle's still running.

| Field | Meaning |
| --- | --- |
| `drained` | `true` when no poll held its lock and no cycle runs. `false`: a cycle still runs, or a poll held its lock for the whole wait, which counts as work that may start |
| `running` | The issues whose cycles hold their lock, lowest first. Empty when none does |
| `limit` | The limit the command set, as [`tehas limit --json`](#tehas-limit) prints it but for `api`, which the object above already carries. `drain` always sets no cycles, no planning and no expiry |
| `limit.state` | `ok` for the limit `drain` writes; the other three states only when the file it just wrote cannot be read back |
| `limit.cycles` | `0`: no cycle starts |
| `limit.plan` | `false`: no new planning session and no improvement run |
| `limit.until` | Left out: a drain's limit lasts until `tehas limit clear` |
| `limit.reason` | `draining` |

Exit 0 when no cycle is running; 1 when some still are at the end of the
wait; 64 on a bad argument; 78 without an instance.

## tehas freeze and tehas thaw

```
tehas freeze <issue> [--reason TEXT] [--json]
tehas thaw <issue> [--json]
```

```json
{"api": 1, "issue": 12, "running": true, "asked": true, "frozen": true}
```

`tehas freeze` holds one running cycle still without ending it. The watch of
the cycle pauses its container (`docker pause`) on its next look, within 10
seconds. While the container is paused, neither the stall nor the time limit
is checked. `tehas thaw` lets it go on: the watch unpauses the container,
moves the start of the time limit forward by the time it was frozen, and
counts the stall from the thaw. For a caller that watches a host's disk and
would rather hold the cycle that writes most than stop it and lose its work.

`asked` says the freeze is asked; `frozen` says the watch has paused the
container. After a freeze `frozen` turns true on the next look, and after a
thaw it turns false on the next look. A frozen cycle still holds its slot, so
freezing makes no room for another. The runner reads its own deadline,
`PI_ENSEMBLE_CYCLE_DEADLINE`, on the wall clock, so a long freeze can bring the
cycle to that deadline sooner: it then parks at a step boundary with a
handoff. A stop of the cycle or of the factory still works on a frozen cycle,
and a freeze ends with its cycle.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `issue` | The issue number the command was given |
| `running` | The cycle of the issue holds its lock, `state/cycles/<issue>.lock`. A lock that cannot be probed counts as held |
| `asked` | The request `state/cycles/<issue>.freeze` is there: `tehas freeze` wrote it and `tehas thaw`, a stop or the end of the cycle removes it |
| `frozen` | The watch has paused the container and wrote `state/cycles/<issue>.frozen`; the word stays until the watch has unpaused it, so a thaw reports it true |

The three files of the cycle named under `running`, `asked` and `frozen` are the
whole of what the two commands read: they take no lock, make no call and write
nothing.

Both commands print the same object, and the source holds its JSON Schema,
`api/freeze.schema.json`, with four samples beside it that the tests keep in
step with what the commands print: `api/freeze.idle.json` (no cycle running),
`api/freeze.asked.json` (a freeze asked of a running cycle),
`api/freeze.frozen.json` (the watch has paused it, so a freeze again answers
"frozen already") and `api/freeze.thaw.json` (`tehas thaw` from that state).

Both answer 0 when there is nothing left to do: a freeze of an issue with no
running cycle, a freeze asked twice, or a thaw of a cycle that is not frozen.
So a caller that asks every minute changes nothing by asking again. Exit 1
when the request could not be written or removed; 64 on a bad argument; 78
without an instance. `tehas api` lists the feature `freeze`.

## Industry alerts

```
~/tehas/state/industry/alerts.json
```

```json
{"time": "2026-10-17T14:00:00Z", "alerts": [{"kind": "paused", "since": "2026-10-17T10:00:00Z", "text": "Paused for 4 h: tehas:pause on the control issue.", "link": "https://tehas-industry.example/factories/tehas-tehas"}]}
```

The one file a program writes into the factory rather than a command it
runs: tehas-industry writes it on each pass, atomically, with what it sees of
the factory that the factory cannot see of itself. [`tehasbot
health`](/tools-health#tehasbot-health) reads it when `TEHAS_INDUSTRY_VERSION` is
set, into `checks.industry.alerts`, and the front pages show the current
alerts as a banner. The source holds its JSON Schema,
`api/industry-alerts.schema.json`, and a sample, `api/industry-alerts.sample.json`.

| Field | Meaning |
| --- | --- |
| `time` | When the industry wrote the file, RFC 3339. Alerts more than five minutes old are not current |
| `alerts` | The alerts that hold now; an empty list when there are none |
| `alerts.kind` | `paused` (paused or drained for too long), `frozen` (nothing moves) or `held-by-one` (one ticket has held the factory still while work waits). A kind tehas does not know is shown by its word |
| `alerts.since` | When it started, RFC 3339 |
| `alerts.text` | The alert for a person, one line. Health keeps 200 characters of it |
| `alerts.link` | Where the industry shows it, an `http` or `https` address |

A file that is not this shape is unreadable: health says so and warns, and
never reads it as no alerts.

## The finish ask

```
<!-- tehas:millwright issue=<n> action=finish -->
<why the failure is outside the change>
```

then the label `tehas:finish`, from the same account within ten minutes.

The one ask a caller with no SSH makes of the factory through GitHub alone: the
factory opens the draft pull request of a stopped cycle from what its kept
workspace holds, with [`tehas-cycle
finish`](/tools-tehasbot#the-pull-request-of-a-stopped-cycle). A factory whose `tehas
api` lists the feature `finish` takes it; one that does not leaves the label
and the comment alone. [Finishing a stopped
cycle](/operating#finishing-a-stopped-cycle) is the whole of it.

| Part | What it is |
| --- | --- |
| Marker | The comment's first line, `<!-- tehas:millwright issue=<n> action=finish -->`, with the issue's own number. The rest of the comment, trimmed, is the reason, which goes into the draft's body. An empty one is refused. The asker's newest such comment, written in the ten minutes before the label and edited by nobody after it, is the one read |
| Label | `tehas:finish`, which `tehas labels` creates. Who applied it is read from the issue's label events, never from the label alone |
| Rules | An account of `TEHAS_ALLOWED_LABELERS` may ask. The millwright may ask within [its grant](/operating#the-millwrights-grant): a person approved a cycle of the issue, the body is the one they approved, nobody outside the list changed the title or text since, and its starts on the issue are no more than `TEHAS_MILLWRIGHT_MAX_RETRIES`. No cycle of the issue may be running. An ask of anyone else is ignored and the label stays |
| Reply | One comment of the bot's. `<!-- tehas:finish issue=<n> result=opened -->` and the draft's URL, or `<!-- tehas:finish issue=<n> result=refused -->` and the one line of why. Either way the label is off and no cycle started |
| Record | One line of `dispatch.log`, which the MCP server's `factory_decisions` answers as the decision `finish`: `finish #<n>: opened <url>, asked by <who> (<label or millwright>): <reason>`, or `finish #<n>: refused, asked by <who>: <why>` |

An ask the poll cannot read, its label events, comments or approval record,
waits with the label on for the next poll, and the log says `wait #<n>:
tehas:finish waits: ...`.

## tehas history

```
tehas history [--json] [--after CURSOR] [--issue N]
```

```json
{"api": 1, "cycles": [
   {"cursor": "1760700000-12-41", "kind": "work", "issue": 12,
    "started": "2026-10-17T11:20:00Z", "ended": "2026-10-17T13:20:00Z",
    "status": 10, "outcome": "handoff", "pr": "https://github.com/acme/widget/pull/9",
    "step": "review", "stepNumber": 9, "stepTotal": 9,
    "cap": "awaiting-human-merge", "parkReason": "merge",
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
the two the same way. `stepNumber`, `stepTotal` and `round` say where the step
a cycle ended in was, as its last heartbeat at that step said, and are left out
when it is not known, as `running` of `tehas status --json` leaves them out. `repeat` is on the park of a run of parks only: the parks
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

`api/history.schema.json` in the source says what the object holds, with
`api/history.sample.json` and `api/history.empty.json` beside it: a listing of
three ends of two issues and an empty listing. The two are what the command
prints, so `test/json_contract_test.sh` keeps them in step with it, and a
program can copy them into its own tests. One schema of `cycles` covers both
kinds of line, so a field only one kind prints is optional there and `kind`
says which line it belongs to. The table gives every field of the schema as the
schema names it, under `cycles` for a field of a line.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `cycles` | The lines, oldest first: one for each cycle and each planning turn that ended |
| `next` | The cursor to keep, as the last line's `cursor`, or `null` when the listing holds no line and none was asked for with `--after` |
| `cycles.cursor` | The seconds of the end, the issue, and the line's place in the order the store wrote the ends, as `1760700000-12-41` |
| `cycles.kind` | `work` for a cycle, `plan` for a planning turn |
| `cycles.issue` | The issue the line is about |
| `cycles.ended` | When the cycle or the turn ended |
| `cycles.started` | When it started; left out when the record holds no start |
| `cycles.status` | Its exit status; left out for a turn whose unit left no readable one |
| `cycles.outcome` | What it reported: for a cycle the status of its end, for a turn `finished`, `failed`, `interrupted`, `not-started`, `not-readable` or `lost` |
| `cycles.turn` | The turn's number in its session, from 1. A field of a plan line |
| `cycles.step` | The step a cycle ended in. A field of a work line |
| `cycles.stepNumber` | The step the cycle was on, from 1, as its last heartbeat at that step said. Left out when it is not known, as `running` of [`tehas status --json`](#tehas-status---json) leaves it out. A field of a work line |
| `cycles.stepTotal` | How many steps the cycle has, from the same heartbeat. A field of a work line |
| `cycles.round` | The round of a step that ran before in this cycle, from 2, or the round the driver gave. A field of a work line |
| `cycles.cap` | The cap it stopped at. A field of a work line |
| `cycles.parkReason` | Why it was parked. A field of a work line, left out when the end named no reason |
| `cycles.failureKind` | The failure the millwright named. A field of a work line |
| `cycles.failureCause` | Its cause, as the millwright gave it. A field of a work line |
| `cycles.provider` | Whether its provider was asked. A field of a work line |
| `cycles.parked` | Whether the cycle parked. A field of a work line |
| `cycles.repeat` | The parks in a row that stopped exactly as this one did, this one not counted, so 1 on the first repeat. On the park of a run of parks only. A field of a work line |
| `cycles.pr` | The pull request, as an address of the repository the instance names. A field of a work line that had one |
| `cycles.by` | Who asked for it. Left out for a cycle run by hand, which names no account |
| `cycles.how` | How it was asked for: `label`, `refresh`, `retry-tick`, `outage-retry`, `millwright` or `hand` |
| `cycles.label` | The label that started the turn's session, `tehas:plan` or `tehas:refine`. A field of a plan line |
| `cycles.auto` | `true` for a refine the factory started on its own when a cycle parked as underspecified or too-large. Left out otherwise. A field of a plan line |
| `cycles.filed` | The issues the turn filed, when it filed any. A field of a plan line |

Exit 0 when it printed, also with nothing to list; 1 when the state store was
busy, which prints nothing; 64 on a bad argument, which reads nothing; 78
without an instance, without a valid `TEHAS_REPO` or without a state store that
can be used.

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
which copies nothing; 78 without an instance, without a valid `TEHAS_REPO`,
with no console to mask with, or when the state store is not installed or
cannot be used, which copies nothing; 1 when the store stayed busy for the
history, which copies nothing.

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

## tehas state check

```
tehas state check [--json]
```

The state store's integrity and the rows of each table, for a program that
moves a factory and compares the two sides. With `--json` it prints the
store's own answer to `store check`, whole and in its order:

```json
{"api": 1, "verb": "check", "ok": true, "schema": 9, "integrity": "ok", "rows": {"cycles": 412, "...": 0}}
```

| Field | Meaning |
| --- | --- |
| `schema` | The schema version of the store |
| `integrity` | `ok`: SQLite's integrity check passed. Any other answer is no object: see the exit statuses |
| `rows` | Each table of the store and its number of rows. A later schema may add tables |

Without `--json` it prints the same as `schema`, `integrity` and one `rows`
line per table, for a person. It reads only and refuses nothing, so it can be
called on a running factory; the counts are of that moment. No call to GitHub,
Docker or the network. Exit 0 when the store checks; 1, with nothing on
standard output and the store's words on standard error, when its integrity
check fails or it did not answer in time; 64 on a bad argument; 78 without an
instance or a state store that can be used. `tehas api` lists the feature
`state-check`.

## tehas state copy

```
tehas state copy --to <file> [--json]
```

```json
{"api": 1, "target": "/home/tehas/copies/tehas-before-deploy.db", "schema": 10, "rows": 4182, "ms": 37}
```

Writes a copy of the state store, as it is at one moment, to `<file>`, mode
600, without stopping a cycle that runs: the copy is SQLite's `VACUUM INTO` on
a read-only connection, so a write that has not committed is not in it and
nothing waits for it. `<file>` must not be there yet: a name that is taken,
even by a link, is refused and left as it was. A relative name is taken from
the working directory. The copy is a store that `tehas` opens as it is.

| Field | Meaning |
| --- | --- |
| `target` | The file written, an absolute name |
| `schema` | The copy's schema version, `PRAGMA user_version`, the number [`tehas state check`](#tehas-state-check) reports. It makes no judgement about which release may read it |
| `rows` | The rows the copy holds, in every table `tehas state check` counts but `meta`, which holds the installation's own values. A new store is 0 |
| `ms` | How long the copy took, in milliseconds |

The fields are read from the copy, so they describe the file written.
`api/state-copy.schema.json` in the source says what the object holds, and
`api/state-copy.sample.json` and `api/state-copy.empty.json`, a new store's,
are samples of it. Without `--json` it prints the same as
one line for a person. It writes nothing but `<file>` (SQLite's `-wal` and
`-shm` beside it while it is made, gone when it is done) and opens the
factory's store read only.

Exit 0 when the copy is made; 1, with the reason in one line on standard
error and nothing written, when `<file>` is there, its directory is not, it
is under `live/`, which the docs server serves, or the store did not answer in
time; 64 on a bad argument; 78 without an instance or a state store that can
be used. No call to GitHub, Docker or the network. `tehas api` lists the
feature `store-copy`.

## tehas state load

```
tehas state load --from <file> [--json]
```

```json
{"api": 1, "source": "/home/tehas/copies/tehas-before-deploy.db", "schema": 10, "rows": 4182, "ms": 52}
```

The inverse of [`tehas state copy`](#tehas-state-copy): fills the factory's
store from a copy that `tehas state copy` made, to restore a factory on a new
host. Run it as the factory's user, on a factory that is drained, with
[`tehas drain`](#tehas-drain), and whose store is new or empty. A relative
name is taken from the working directory.

It refuses, in one line on standard error and with the store as it was:

- a factory that is not drained, or where a cycle or a planning turn runs;
- a store that holds history: a row of a cycle, a planning turn, an approval,
  a park, a question, an ending or an action. The poll's samples and caches,
  which a drained factory still writes, are not history, and the copy's
  replace them;
- a copy that is not there, is not a store or fails SQLite's
  `PRAGMA integrity_check`;
- a copy of a newer schema than this release reads, naming both numbers.

A copy of an older schema is brought forward by the same steps an update runs
on the store. The copy is read from a file of its own beside the store, so
the copy itself is not changed. The rows then replace the store's in one
transaction: the store's file is not replaced, because the readers of a
drained factory, the dashboard and the poll, hold it open. A failure rolls the
transaction back.

| Field | Meaning |
| --- | --- |
| `source` | The copy read, an absolute name |
| `schema` | The copy's schema version, as [`tehas state check`](#tehas-state-check) reports it, before it was brought forward. The store is at this release's schema afterwards |
| `rows` | The rows loaded, in every table `tehas state check` counts but `meta`. A copy of a new store is 0 |
| `ms` | How long the load took, in milliseconds |

`api/state-load.schema.json` in the source says what the object holds, and
`api/state-load.sample.json` and `api/state-load.empty.json` are samples of
it. Without `--json` it prints the same as one line for a person. The limit
of the drain stays: `tehas limit clear` starts the factory again.

Exit 0 when the copy is loaded; 1, with the reason in one line on standard
error, when it is refused or the store did not answer in time; 64 on a bad
argument; 78 without an instance or a state store that can be used. No call to
GitHub, Docker or the network. `tehas api` lists the feature `state-load`.

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

## tehasbot clean

```
tehasbot clean                # one run
tehasbot clean --dry-run      # name what it would remove, and remove nothing
```

Reclaims what the host can make again, of anything untouched for
`TEHAS_CLEAN_AFTER` hours: build output in a kept workspace, Docker's build
cache, host scratch, old sandbox and base images, and old logs and the state
store's upkeep. The six steps, their age and lock rules and what they never
touch are on
[tehasbot clean](/tools-clean#tehasbot-clean); this page names the command only as
far as a program needs it.

It is not `--json`: one line per removal goes to `~/tehas/logs/clean.log` and is
printed as well, and the log lines are the contract. A real run closes with
`clean: freed <n> bytes`. A dry run measures each path it names with the same
figure a real run counts, on `clean: would remove <path>: <what>, <n> bytes`,
and closes with `clean: would free <n> bytes, which leaves out the build cache,
the image tags and the store's upkeep, whose size is not known beforehand`. The
three steps that cannot size themselves beforehand - Docker's build cache, the
image tags of steps 4 and 5, which share layers with tags that stay, and the
store's upkeep, which answers names and not sizes - say so on their own line.
A dry run removes
nothing, takes no issue's lock and backs the store up not at all, so its figure
is an estimate of a tree that is moving while it is measured.

The feature `clean` in [`tehas api`](#tehas-api) says the command exists.
Exit 0 when every step worked; 1 when a step failed or the log could not be
written, with what could be removed removed; 3 when another run holds the
cleaner's lock, which changes nothing; 64 on an argument the command does not
know; 78 when the settings are missing or malformed or no console is installed
beside the command, before any Docker, find or rm call.

## tehas disk

```
tehas disk [--full] [--cycles] [--json] [--publish]
```

```json
{"api": 1, "generatedAt": "2026-10-31T23:09:32Z", "dockerRoot": "/var/lib/docker",
 "filesystems": [
   {"paths": ["/home/boss/tehas"], "mount": "overlay",
    "freeBytes": 22348619337728, "totalBytes": 30723834994688},
   {"paths": ["/tmp"], "mount": "tmpfs",
    "freeBytes": 5423874048, "totalBytes": 6442450944}
 ]}
```

How full this host is. One entry per file system the factory cares about:
this instance's home, the directory Docker keeps its images in as Docker
itself names it, `/tmp`, and `TEHAS_SCRATCH_DIR` when the instance sets one.
Two paths that stand on one file system are one entry that names both, so the
same free bytes are never counted twice; a path on a device that cannot be
told has an entry of its own.

The source holds a JSON Schema of this object, `api/disk.schema.json`, and two
samples made by the code that prints it: `api/disk.sample.json` is the plain
answer, and `api/disk.full.json` is the one with `full`, with every field of it
present - a file system whose figure could not be read, a place that ran out of
a bound, the daemon's lines, and the containers it holds back. The table gives
every field of the schema as the schema names it, under `filesystems` for a
field of one entry and under `full` for a field of what fills them.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `generatedAt` | When the answer was made, UTC. The command's own reads write nothing, so this says how old the answer on the terminal is |
| `dockerRoot` | The directory Docker keeps its images in, as `docker info` named it. Left out when the daemon named none, which is a host with no Docker and the entry of `/var/lib/docker` all the same |
| `filesystems` | One entry per file system the factory cares about, in the order the paths were asked |
| `filesystems.paths` | Every watched path that stands on this file system, the one that opened the entry first |
| `filesystems.mount` | What `df` names as the file system's own, as `/` or `/dev/sda1`. Left out with no figure to go with it |
| `filesystems.freeBytes` | Bytes free on it, from `df`; `null` with `why` when it could not be read, never 0 |
| `filesystems.totalBytes` | Its whole size in bytes, from `df`; `null` with `why` when it could not be read, never 0 |
| `filesystems.why` | Why the entry has no figure: `df` gave no second line, its word was no number, or the call did not answer in `TEHAS_DISK_WAIT` seconds |
| `full` | What fills them, only with `--full` |
| `full.places` | The biggest places first, at most twelve: the entries of `~/tehas` and of `~/.pi/agent`, each clone's `~/tehas/dev/<name>.state/target` in place of the clone's own tree, and the scratch disk's entries when the instance has one |
| `full.places.path` | The place, as the walk named it |
| `full.places.bytes` | Its bytes; `null` when the walk has no figure for it, which is what `cut` says the reason for |
| `full.places.cut` | The bound the walk ran out of: `whole` for a figure that is the whole of the place, or `entries`, `depth`, `time` or `unreadable` |
| `full.docker` | `docker system df` as the daemon printed it |
| `full.docker.state` | `read` when it answered lines, `none` when it answered none, `unreadable` when the call did not answer |
| `full.docker.lines` | Its lines, in the daemon's words: at most twelve, each cut to 200 characters |
| `full.stoppedContainers` | How many containers the daemon still holds back, from `docker ps --filter status=exited`. 0 when it listed none, and 0 on a host with no daemon: no answer is none. It is a question of the daemon, not of the disk, and `--full` is what asks it |
| `full.containers` | The containers themselves, at most fifty: Docker's own id and image, and its command when the listing had one. Left out when it listed none, since the count above has already said none |
| `full.containers.id` | The container's id, as Docker printed it, at most 64 characters |
| `full.containers.image` | Its image, at most 200 characters |
| `full.containers.command` | Its command, at most 200 characters. Left out when the listing gave no command, which is no command and not an empty one |

A figure that cannot be read is `null` and carries `why`, the reason: the read
of `df` answered nothing, its word was no number, or the call did not answer
in `TEHAS_DISK_WAIT` seconds. It is never `0`, because no figure is never the
same as plenty - the same rule the dispatcher's hold reads by. The figures are
bytes from `df`, and the command removes nothing.

`--full` adds `full`:

```json
{"full": {"places": [{"path": "/home/boss/tehas/logs", "bytes": 204800}],
  "docker": {"state": "read", "lines": ["TYPE\tTOTAL\t...\tSIZE\t..."]},
  "stoppedContainers": 0, "containers": [{"id": "ab1", "image": "x", "command": "y"}]}}
```

`places` is what fills them, biggest first: the entries of `~/tehas` and of
`~/.pi/agent`, each clone's `~/tehas/dev/<name>.state/target` in place of the
clone's own tree, and the scratch disk's entries when the instance has one.
A place that ran out of its bound keeps its figure and says so with `cut`:
`entries`, `depth`, `time` or `unreadable`. `docker` is `docker system df` as
the daemon printed it, `state` `read`, `none` or `unreadable`.
`stoppedContainers` and `containers` name what the daemon still holds back.
They are a question of the daemon, not of the disk, so `--full` is what asks
it: both figures are printed by `tehas disk --full --json` with no `--publish`,
and a reader that must not write the host asks for them with the flag alone.
`--publish` is the one form that writes, and it names them in the file too, of
the same one call of the daemon.

`--cycles` adds `cycles`: what each running cycle writes.

```json
{"cycles": {"state": "read", "list": [
   {"issue": 1872, "container": "tehas-work-1872",
    "writableBytes": 1234567890, "blockWrittenBytes": 1200000000},
   {"issue": 1873, "container": "tehas-work-1873",
    "writableBytes": null, "blockWrittenBytes": 0,
    "why": "writableBytes: docker inspect --size gave no size"}
]}}
```

One entry per container the daemon lists under the name a cycle's container
has: `tehas-work-<n>`, where `<n>` is the issue the cycle works on. The name
is matched exactly, so `tehas-work-51x` and a dev container are no cycle, and
a paused container counts as a running one. The name carries at most nine
digits, the widest issue this platform publishes an issue number as, so a
container of something else that shares the host's daemon is not read as a
cycle. `writableBytes` is the size of the container's writable layer and
`blockWrittenBytes` the bytes its block device has written. A host with no
cycle running has `"list": []`; when the daemon would not answer the listing at
all the object is `{"state": "unreadable", "why": "..."}` and has no `list`.

The same rule as every other figure here: one that could not be read is `null`
with `why` naming which figure and why, never `0`. A cycle that wrote nothing
and a cycle the daemon would not speak of are two different facts, and 0 would
be read as the first of them. The reason comes of the reading that failed: a
size the daemon printed as no size, a `BlockIO` column that was no figure, or
the words the call gave when it made no answer at all.

The figures are read by three calls made once for the whole host, not once per
container: the listing, one `docker inspect --size` for `SizeRw`, and one
`docker stats --no-stream` for the written half of `BlockIO`. Each is bounded
by `TEHAS_DISK_WAIT` as every other call of this command is. Docker prints
those sizes in decimal units - `kB`, `MB`, `GB` - so `blockWrittenBytes` and
`writableBytes` are the figure it printed, rounded to bytes: `1.2GB` is
1 200 000 000 bytes, not 1 288 490 188. The rounding is done in whole numbers,
so a figure with no fraction such as `4.4PB` is the exact 4 400 000 000 000
000 bytes. A figure in a unit Docker does not print is no figure, with a
reason, and so is one too large to count in 64 bits: no reading is `null`,
never the largest number there is.

The object `tehas disk --cycles --json` prints is `api/disk-cycles.schema.json`
in the source, and `api/disk-cycles.sample.json` is a sample of it. The
feature `cycle-writes` of [`tehas api`](#tehas-api) says the release has the
figures; a factory that predates it prints no `cycles` at all. The two keys it
shares with the plain answer, `filesystems` and `full`, are a copy of
`api/disk.schema.json`, restated because api/ has no `$ref`: the one file
names them one way and the other the same way, and the tests keep both in step
with what the command prints.

`--publish` also writes that object to `~/tehas/live/disk.json`, whole, through
a side file renamed into place, so a reader sees a whole answer or the last
one. It is the only form that writes anything, and it prints the reading as
well. On a host the cleaner's `tehas-clean.timer` runs it every hour after the
clean, whether the clean succeeded or not
([`tehas disk`](/tools-status#tehas-disk)), so the file is under an hour old there; a
person or a program can ask for a reading at any moment. The
[`factory_disk` tool](/mcp-server#the-tools) of the MCP server reads that file
and never runs the command, so the server asks the host's disk nothing.

The reading, the bounds and the printing are the console's
(`console/ops/src/disk.rs`), which reads the home, the scratch disk and the bound
from the instance's own file; the figures of the cycles are the module beside
it (`console/ops/src/disk/cycles.rs`). The bound is `TEHAS_DISK_WAIT`, the one the dispatcher's hold and
the previews' measure already read the host's space under: one outside call is
given it, and one walk of a place twice it.

Exit 0 when it printed, and with the file written; 1 when the published file
could not be written, which leaves the last answer there; 64 on an argument the
command does not know, which writes nothing; 78 when the settings are missing
or malformed, before any `df`,
`docker` or walk.

## tehas panel

```
tehas panel [--json]
tehas panel set <id> --label TEXT --url URL
tehas panel clear <id>
```

```json
{"api": 1, "panels": [{"id": "support", "label": "Ask us", "url": "https://support.example.org/f/acme"}]}
```

A page that belongs to the project, such as a support chat, a status page or a
feedback form, shown on the dashboard. Each panel is an item in the menu of both
front pages, and pressing it opens the page in a dock at the side. tehas knows
the label and the URL, nothing else. The table gives every field of the schema
as the schema names it, so a field of an item of the list is named under
`panels`.

| Field | Rule |
| --- | --- |
| `api` | The API version, 1 |
| `panels` | The panels, in the order they were set: at most 4, empty when none is set |
| `panels.id` | `a-z`, `0-9` and `-`, 1 to 32 characters. `set` on an `id` that is there replaces it, in its place |
| `panels.label` | One line, 1 to 24 characters. Control characters become spaces, as in `tehas pause` |
| `panels.url` | `https://` or `http://`, a host, no user or password, at most 512 characters. Its host is not `TEHAS_DOMAIN`: the frame is given `allow-same-origin`, which is safe only on another origin. A sibling name such as `support.<domain>` is another origin and is allowed |

- At most 4 panels. A fifth is refused.
- The list is `~/tehas/state/panels.json`, and the same object is published,
  renamed into place, to `~/tehas/live/panels.json`. With no panels both files
  are gone. `tehas state pack` takes both.
- **A URL is public**, as every file the list names is: put no secret in it.
- `set` and `clear` print the list as it then is; `--json` works with the show
  form only. Setting is safe to repeat, and `clear` of an `id` that is not there
  is fine. Reading makes no network call.

The dashboard reads the file once at load (`host/fun/panels.js`) and checks each
URL again. It makes the frame on the first press, never at load, so it asks a
panel's host for nothing until a person does:

```html
<iframe title="<label>" src="<url>" referrerpolicy="no-referrer" allow=""
  sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox"></iframe>
```

No `allow-top-navigation`, no message channel between the page and the frame,
and no context but what is in the URL. A page that refuses to be framed shows
an empty frame; the dock's "Open in a new tab" link still works.

`api/panel.schema.json` in the source says what the object holds, with
`api/panel.sample.json` and `api/panel.empty.json` beside it: one panel set, and
a factory with none. The two are what the command prints, so
`test/json_contract_test.sh` keeps them in step with it, and a program can copy
them into its own tests.

The checks, the list and the files are the console's (`console/ops/src/panel.rs`).

Exit 0 done; 1 the list could not be read or written; 64 a bad argument, a fifth
panel or a URL on `TEHAS_DOMAIN`, which writes nothing; 78 without an instance.

## tehas host-message

```
tehas host-message [list] [--json]
tehas host-message set <id> --level info|warn|alert --text TEXT [--link URL]
tehas host-message clear <id>
```

```json
{"api": 1, "messages": [{"id": "disk-low", "level": "warn", "text": "Disk 91% full", "link": null, "at": "2026-11-12T18:03:00Z"}]}
```

Short messages for this factory, posted by an operator or a supervisor
process. The table gives every field of the schema as the schema names it, so
a field of an item of the list is named under `messages`.

| Field | Rule |
| --- | --- |
| `api` | The API version, 1 |
| `messages` | The messages, in the order they were first set: at most 20, empty when none is set |
| `messages.id` | `a-z`, `0-9` and `-`, 1 to 32 characters. `set` on an `id` that is there replaces it, in its place |
| `messages.level` | `info`, `warn` or `alert` |
| `messages.text` | One line, 1 to 200 characters. Control characters become spaces, as in `tehas pause` |
| `messages.link` | `https://` or `http://`, a host, no user or password, at most 512 characters; `null` when not given |
| `messages.at` | When the message was last set, UTC, as `2026-11-12T18:03:00Z` |

- At most 20 messages. A 21st is refused.
- The list is `~/tehas/state/host-messages.json`, mode 600, the same object
  as `list --json`, renamed into place under `state/host-message.lock`. With no
  messages the file is gone.
- `set` and `clear` print the list as it then is. `clear` of an `id` that is
  not there is fine. Reading makes no network call.

`api/host-messages.schema.json` in the source says what the object holds, with
`api/host-messages.sample.json` beside it, printed by the command.

Each health write reads the file into `checks.hostMessages` of the
[health file](/tools-health#tehasbot-health): `{"list": [{id, level, text,
link, at}]}`, `{"list": []}` with no file, and `{"list": null, "unreadable":
true}` for a file that does not parse.

- An `alert` or a `warn` message makes the status `warn`; an `info` message
  changes no status. A message holds until it is cleared.
- The output line is the text of the first `alert`, else of the first `warn`
  message, said before a pause.
- A file that cannot be read warns: "The host messages cannot be read:
  state/host-messages.json".
- `tehas status` prints one line for each message, `message   <level>:
  <text>`, after the pause and limit lines. The MCP `factory_status` answer
  carries the object as `hostMessages`, and the banner of the front pages
  lists each message: Alert, Warning or Notice, its text, when it was set and
  its link.
- The text and the link are the writer's: the dashboard sets both as text,
  and the terminal leaves out direction overrides and zero-width characters.

The checks, the list and the file are the console's
(`console/ops/src/host_message.rs`).

Exit 0 done; 1 the list could not be read or written, or another
`tehas host-message` held the lock for 10 seconds, which writes nothing; 64 a
bad argument or a 21st message, which writes nothing; 78 without an instance.

## Files under live/

The docs server publishes what a host keeps under `~/tehas/live/`: under
`/live/` at the paths below, and at `/health` and `/healthz` for
`health.json`. The names are one list, and the docs server holds it too: in
`host/edge/docs.Caddyfile` the `/live/` handler answers 404 for any name the
list does not hold, so a file a new writer puts under `live/` is not served
until both the table and the Caddyfile name it. Every file here is plain and
written through a side name and a rename, so a reader sees a whole file or the
last one. The console files mask tokens, keys and passwords as they are
written, and still: treat all of it as public.

| Name | Written by | Read by |
| --- | --- | --- |
| `status.json` | `console/src/queue.rs`, run as `tehasbot status` ([tehasbot status and the console](/tools-health#tehasbot-status-and-the-console)) | `host/fun/dashboard-view.js`, `host/fun/business-view.js`, `host/fun/game-view.js`; the store's `publish`; `console/ops/src/health.rs`, `console/analytics/src/sample.rs`, the MCP server (`factory_status`, `factory_issues`, `factory_issue`, `factory_sessions`) |
| `running.json` | the store's `publish` (`console/store/src/live.rs`) | `host/fun/dashboard-view.js`, `host/fun/business-view.js`, `host/fun/game-view.js`, `host/fun/console-tabs.js`; `console/ops/src/health.rs`, `console/src/dev.rs`; the MCP server (`factory_status`, `factory_issue`, `factory_sessions`) |
| `endings.json` | the store's `publish` (`console/store/src/live.rs`) | `console/ops/src/health.rs`; the MCP server (`factory_endings`) |
| `epics.json` | `console/src/epics.rs`, run by `tehasbot status` | `host/fun/epics-view.js`; `console/ops/src/health.rs` |
| `health.json` | `console/ops/src/health.rs` run as `tehasbot health`, published by the store's `publish` ([tehasbot health](/tools-health#tehasbot-health)) | `/health` and `/healthz`, and `host/fun/versions.js`; `console/src/status.rs`, `console/ops/src/update/report.rs`; the MCP server (`factory_status`, `factory_issue`) |
| `health.down` | the store's `publish`, which makes it before a `fail` health report and removes it after any other | the docs server's `/health` and `/healthz`, which answer 503 while it is there |
| `dispatch.json` | `console/dispatch/src/poll.rs` of `tehas dispatch`, published by the store's `publish` ([tehas dispatch](/tools-dispatch#tehas-dispatch)) | `console/ops/src/health.rs`, `console/src/queue.rs`, `console/src/energy.rs`, `console/analytics/src/sample.rs`; the MCP server (`factory_status`) |
| `demand.json` | `console/dispatch/src/poll.rs` of `tehas dispatch`, at every real poll | `console/ops/src/health.rs` |
| `energy.json` | `console/src/energy.rs` run as `tehasbot energy` ([tehasbot energy](/tools-health#tehasbot-energy)) | `host/fun/energy-view.js`, `host/fun/business-view.js`; `console/analytics/src/sample.rs` |
| `analytics.json` | `console/src/publish.rs`, run by `tehasbot status` from the analytics program's `report --json` | `host/fun/dashboard-view.js`, `host/fun/business-view.js`, `host/fun/analytics-data.js`; the MCP server (`factory_stats`) |
| `top.json` | `console/src/top.rs` run as `tehasbot top`, when `TEHAS_TOP=1` ([tehasbot top](/tools-health#tehasbot-top)) | `host/fun/host-view.js`, `host/fun/host-data.js`, `host/fun/dashboard-view.js` |
| `panels.json` | `console/ops/src/panel.rs`, the copy of the person's list ([`tehas panel`](#tehas-panel)) | `host/fun/panels.js` |
| `disk.json` | `console/ops/src/disk.rs` with `--publish`, run hourly by `tehasbot clean` ([`tehas disk`](#tehas-disk)) | the MCP server (`factory_disk`) |
| `platform.json` | `console/src/platform.rs`, run by `tehasbot status` | `console/ops/src/health.rs`, which answers it as `checks.platform` |
| `rukas.json` | `console/ops/src/rukas.rs`, run by `tehasbot status` | `console/ops/src/health.rs`, which answers it as `checks.rukas` |
| `previews.json` | `console/src/preview/list.rs` of `tehasbot preview` ([tehasbot preview](/tools-clean#tehasbot-preview)) | `host/fun/dashboard-view.js`; `console/ops/src/health.rs` |
| `protolab.json` | `console/src/protolab/build.rs` of `tehasbot protolab` ([tehasbot protolab](/tools-protolab#tehasbot-protolab)) | `host/fun/business-view.js`, `host/fun/game-view.js`; `console/src/status.rs`, which answers it as `prototypes` |
| `cycles/<n>.json` | the store's `publish`, one file per running cycle and the last that finished | the pages that read `running.json` (`host/fun/dashboard-view.js`, `host/fun/business-view.js`, `host/fun/console-tabs.js`) draw the same view; no page asks for the file by its own path |
| `console-<n>.txt` | `console` (`console/src/tail.rs`), started by the cycle's watch (`console/cycle/src/watch.rs`) | `host/fun/dashboard-view.js`, `host/fun/business-view.js` and `host/fun/game-view.js`, each by the name in `running.json`, and the console of the MCP server (`factory_console`) |
| `console-<n>.txt.now` | the cycle's watch (`console/cycle/src/watch.rs`), the two lines of the console's status line | `console/cycle/src/watch.rs` and the record of the cycle that ended |

Two of these the edge serves outside `/live/`: `/health` and `/healthz`
rewrite to `/health.json` and answer 503 with the same body while
`health.down` is there. Neither is read through `/live/` by a page.

## The host bridge

```
GET /host/bridge.json
```

```json
{"bridge": 1, "host": "example-host", "time": "2026-11-08T12:00:00Z", "ticketLink": "/host/ticket/{n}", "notices": [{"id": "n-1", "level": "info", "text": "Example notice.", "link": "/host/notice/n-1"}], "ticketNotes": [{"issue": 42, "text": "Example note", "link": "/host/ticket/42"}], "menu": [{"id": "m-1", "label": "Example page", "url": "https://host.example/page"}]}
```

A contract between the dashboard and a host that fronts the factory's domain.
The dashboard asks its own origin for `/host/bridge.json`. tehas puts nothing
under `/host/`: the docs server answers every path there with 404, which the
dashboard reads as no host. A factory without a host looks and works as it
would without this section.

The front pages and `/dashboard` ask once when a page loads, then every 60
seconds while the page is visible, with `cache: "no-store"`. A failed ask, or
no answer in 5 seconds, keeps the last good answer for up to 5 minutes, then
drops it. A 404 drops it at once. Every view draws from `live/` first; the
bridge's elements are added when an answer arrives. The source holds the JSON
Schema, `api/host-bridge.schema.json`, a sample with every field,
`api/host-bridge.full.json`, and one with only the required fields,
`api/host-bridge.bare.json`.

| Field | Rule |
| --- | --- |
| `bridge` | The contract's version, `1`. Any other value: the answer is ignored whole |
| `host` | One line, at most 40 characters. Shown nowhere |
| `time` | RFC 3339. An answer more than 5 minutes old, or as far ahead of the server's clock, is ignored whole |
| `ticketLink` | Optional. Where a ticket of the factory's repository links, with `{n}` for its number once. Starts with `/host/`, at most 200 characters. Absent: tickets link to GitHub |
| `notices` | Optional, at most 5. Lines at the top of the front pages |
| `notices.id` | `a-z`, `0-9` and `-`, 1 to 32 characters, unique in the list |
| `notices.level` | `info`, `warn` or `alert` |
| `notices.text` | One line, 1 to 200 characters. Control characters become spaces |
| `notices.link` | Optional. A path under `/host/`, or an `https://` address, at most 512 characters |
| `ticketNotes` | Optional, at most 200. A short note on a ticket, wherever the dashboard draws that ticket |
| `ticketNotes.issue` | 1 to 999999999. At most 3 notes per ticket; the rest are dropped |
| `ticketNotes.text` | One line, 1 to 60 characters. Control characters become spaces |
| `ticketNotes.link` | Optional, as `notices.link` |
| `menu` | Optional, at most 4. Items in the front pages' menu, each opening its page in the dock as a panel does |
| `menu.id` | `a-z`, `0-9` and `-`, 1 to 32 characters, unique in the list. An item with the id of a panel takes that panel's place |
| `menu.label` | One line, 1 to 24 characters. Control characters become spaces |
| `menu.url` | An `http://` or `https://` address with no user or password, at most 512 characters, not on the dashboard's own origin |

- Not JSON, over 256 KiB, or a bad `bridge` or `time`: the answer is ignored
  whole.
- An element that breaks a rule is dropped alone; the rest is used. Past the
  limit of a list, the elements after it are dropped.
- Unknown fields are ignored, at the top level and in each element. A
  `tokens` field, which an earlier release drew, is one of them.
- Text goes into the page as text only, never as markup.
- A link is used only if it passes its rule. A path under `/host/` opens in
  the same tab; an `https://` address in a new one.

How the dashboard draws it:

- `ticketLink`: every link to an issue of the factory's repository goes there
  instead, in the same tab. Pull requests keep linking to GitHub.
- `notices`: a line each at the top of both front pages, in the tone of its
  level, beside the industry's alerts and drawn the same way.
- `ticketNotes`: a small chip beside the ticket's number or title in every
  view that draws the ticket from the live queue, linked when it has a link.
- `menu`: an item each in the menu of both front pages, after the panels of
  `live/panels.json`, four in all. Pressing one opens its page in the dock at
  the side, framed as a panel is; an item that goes from the answer goes from
  the menu.

The answer is data and never runs code on the dashboard's pages. Pages under
`/host/` share the dashboard's origin: tehas keeps no secret on it, and a host
that fronts the domain is trusted as the factory's operator is. tehas
registers no service worker, and the front pages and `/dashboard` unregister
any they find on their origin. `live/` stays public; the bridge does not
change it. `tehas api` lists the feature `host-bridge`.

## Settings rules

```
console settings rules --json
```

```json
{
  "api": 1,
  "required": ["TEHAS_REPO", "TEHAS_CONTROL_ISSUE", "TEHAS_ALLOWED_LABELERS", "PI_ENSEMBLE_SUBAGENT_MODEL", "PI_ENSEMBLE_SUBAGENT_PROVIDER"],
  "roles": ["DEVELOPER", "OPS", "EXPLORE", "ADVERSARIAL_DEVELOPER", "CODE_REVIEW_SPECIALIST"],
  "settings": [
    {"name": "TEHAS_INDEXING", "rule": "one-of", "when": "always", "default": "0", "words": ["0", "1"], "reason": "expected 0 or 1"},
    {"name": "TEHAS_REPO", "rule": "repo", "when": "required", "default": null, "reason": "expected owner/name"}
  ]
}
```

Every release carries the rules of the instance settings as
`api/settings-rules.json`, at `tehas-<tag>/api/settings-rules.json` in the
tarball. It is what `console settings rules --json` of that release prints,
built from the table the load checks `instance.env` with, so a tool with no
factory, such as the CI of a settings repository, can check the values. The
source holds the JSON Schema, `api/settings-rules.schema.json`. `tehas api`
lists the feature `settings-rules`.

| Field | What it holds |
| --- | --- |
| `api` | The file's version, `1` |
| `required` | The settings an instance has to set itself |
| `roles` | The roles that take a model of their own: the suffixes of `PI_ENSEMBLE_MODEL_<ROLE>` and `PI_ENSEMBLE_PROVIDER_<ROLE>`, each a `one-word` value |
| `settings` | One object per setting, in the order a load checks them, which picks the setting an operator is told about first |
| `settings.name` | The setting |
| `settings.rule` | The shape of its value, a kebab-case name such as `repo`, `count`, `one-of` or `low-gate`. Rows with the same rule take the same values, except `one-of`, whose words are the row's own |
| `settings.when` | When a load checks it: `always` (it has a default, so it always holds something), `given` (when the instance gives it a value), `named` (when the instance names it, even with an empty value), `required` (when a command requires it) |
| `settings.default` | What the load uses when the instance leaves it out or empty, or `null`: it stays unset, or the load works it out |
| `settings.words` | Only for a `one-of` row: the values it takes |
| `settings.reason` | What the check says for a value the rule refuses |

An empty value means unset and is not checked against the rule, except for a
`named` setting. An unset setting of `when` `required` is refused by the
command that requires it.

The file holds a rule's name and one reason, not the check itself: to check a
value, run `libexec/console settings valid <name> <value>` from the tarball of
the same release. It exits 0 for a value the setting takes, and 1 with the
reason on standard output for one it refuses, the same reason the factory
gives, for any rule. A `low-gate` value is also refused when it is a path under
the host's own trees, the tehas home, the instance directory and the work
repository, which only the host knows; the console checks those when
`TEHAS_HOME`, `TEHAS_INSTANCE_DIR` and `TEHAS_WORK_REPO` are set.
