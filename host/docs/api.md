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
[`tehas order`](#tehas-order), [`tehas blockers`](#tehas-blockers),
[`tehas start`](#tehas-start), [`tehas answer`](#tehas-answer),
[`tehas question`](#tehas-question),
[`tehas refresh`](/tools-github#tehas-refresh) (`tehasbot refresh`, its old
name, stays through 1.x),
[`tehas wake`](#tehas-wake), [`tehas drain`](#tehas-drain), [`tehas freeze`](#tehas-freeze-and-tehas-thaw),
[`tehas thaw`](#tehas-freeze-and-tehas-thaw), [`tehas history`](#tehas-history),
[`tehas export`](#tehas-export),
[`tehas state`](#tehas-state), [`tehas state check`](#tehas-state-check),
[`tehas state copy`](#tehas-state-copy),
[`tehas state load`](#tehas-state-load),
[`tehas analytics copy`](#tehas-analytics-copy),
[`tehasbot clean`](#tehasbot-clean), [`tehas disk`](#tehas-disk),
[the finish ask](#the-finish-ask), [the events](#events), [the host
bridge](#the-host-bridge) and [the settings rules](#settings-rules). The design of the rest is in
the spec, `docs/specs/2026-10-17-factories.md` in the source; a command is
listed here when a release has it.

## Principles

1. **Commands are the API; files are not.** What is under `~/tehas` is
   internal and may change in any release. The JSON the docs server publishes
   under `/live/` is for the dashboard, not for a program.
2. **`--json` prints one JSON object** on standard output and nothing else.
   Messages go to standard error. Without `--json` the same command prints for
   a person. An object this page describes is public. `tehas top --json` is not:
   it prints the snapshot a frame of the screen is drawn from, for the
   dashboard, and it may change in any release.
3. **Every object carries `"api": 1`.** Within version 1 fields are only
   added, never renamed, removed or given a new meaning. A breaking change is
   version 2, and the release that brings it says so.
4. **A field without a value is left out**, never `null`, unless this page
   says the field itself is `null` when unknown. A program accepts fields it
   does not know.
5. **Exit statuses**: 0 done; 1 failed, a call, a read or a write did not
   succeed; 3 the command worked and the answer is "no" or "not yet"; 64 a
   bad argument; 78 missing or malformed instance settings, or a feature that
   is off; 141 standard output closed before the command finished printing
   (as in `tehas status | head -1`), with nothing on standard error, as a
   shell command SIGPIPE ended would. A status means the same in every
   command. [The contract](/contract#exit-statuses) has the table of each
   public command.
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
   To say which issue starts at all, with no label of a GitHub account,
   approve it with [`tehas start N`](#tehas-start).
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
  "features": ["status", "limit", "drain", "history", "export", "analytics", "state-move", "mcp-dashboard-keys", "clean", "disk", "state-check", "finish", "status-versions", "state-copy", "state-load", "freeze", "host-bridge", "cycle-writes", "settings-rules", "start-order", "host-message", "refresh-pr", "host-start", "flags", "status-previews", "wake", "start-reasons", "blockers", "answer", "question", "cycle-ids", "questions", "events"],
  "flags": {"lean": false, "plugins": false},
  "millwright": null,
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
  },
  "store": "8f14e45fceea167a5a36dedd4bea2543"
}
```

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `tehas` | The release tag the factory was installed from: `v0.10.2` or `v1.0.0-alpha.1` for a release install, `v0.8.0` for a checkout one. `null` when the install came from a checkout with no tag, or not from a git checkout |
| `commit` | The commit it was installed from, seven characters; `null` for a release install, which records only the tag, and when not from a git checkout |
| `repo` | `TEHAS_REPO`, the product repository |
| `features` | What this release has, so a program can tell a factory that predates a feature from one where it failed. Today `status` ([`tehas status --json`](#tehas-status---json)), `limit` ([`tehas limit`](#tehas-limit)), `drain` ([`tehas drain`](#tehas-drain)), `history` ([`tehas history`](#tehas-history)), `export` ([`tehas export`](#tehas-export)), `analytics` ([`tehas analytics copy --json`](#tehas-analytics-copy)), `state-move` ([`tehas state`](#tehas-state)), `mcp-dashboard-keys` (the read-only MCP server answers a key of `TEHAS_MCP_DASHBOARD_KEYS` with only the four tools that copy from `live/`, [the MCP server](/mcp-server)), `clean` (the hourly cleaner runs on this host, [`tehasbot clean`](#tehasbot-clean)), `disk` ([`tehas disk`](#tehas-disk)), `state-check` ([`tehas state check`](#tehas-state-check)), `finish` ([the finish ask](#the-finish-ask)), `status-versions` (`versions` in [`tehas status --json`](#tehas-status---json)), `state-copy` ([`tehas state copy`](#tehas-state-copy)), `state-load` ([`tehas state load`](#tehas-state-load)), `freeze` ([`tehas freeze` and `tehas thaw`](#tehas-freeze-and-tehas-thaw)), `host-bridge` ([the host bridge](#the-host-bridge)), `cycle-writes` ([`tehas disk --cycles`](#tehas-disk) names what each running cycle writes), `settings-rules` (the release carries [its settings rules](#settings-rules)), `start-order` ([`tehas order`](#tehas-order)), `host-message` ([`tehas host-message`](#tehas-host-message), shown by health, `tehas status`, the MCP server and the dashboard), `refresh-pr` ([`tehas refresh --pr N`](/tools-github#tehas-refresh) asks for the refresh of one pull request, `--json` prints what it asked as one object, and the dispatcher starts it in the refresh lane, not in a cycle slot, [refreshes](/operating#refreshes)), `host-start` ([`tehas start`](#tehas-start) approves an issue from the host), `flags` (the object below is there), `status-previews` (`previews` in [`tehas status --json`](#tehas-status---json)), `wake` ([`tehas wake`](#tehas-wake) asks for a poll now), `start-reasons` (`starts` in [`tehas order --json`](#tehas-order)), `blockers` ([`tehas blockers`](#tehas-blockers)), `answer` ([`tehas answer`](#tehas-answer)), `question` ([`tehas question`](#tehas-question)), `cycle-ids` (`store` here; `cycle`, `how` and `by` on `running` in [`tehas status --json`](#tehas-status---json); `cycle`, `end`, `change`, `signalled` and `recorded` on the work lines of [`tehas history`](#tehas-history), each with its `started`, a line for every start, paged by write order) and `questions` ([`tehas question`](#tehas-question) with no issue lists every question that waits, and with `--after <id>` every question after a number), and `events` (a cycle raises its events, [Events](#events)). A shipped optional feature adds its name of `flags` here; a building one does not |
| `flags` | Every optional feature of the release and whether this instance has it on, `true` or `false`. A feature still being built is always `false` and its switch set to `on` stops every command with exit 78. Only a shipped feature's key is contract: a building feature's key may change or go until the release that ships it, and `api/api.schema.json` requires only the keys of shipped features. Today `lean` (`TEHAS_LEAN`, still being built: [choosing the driver](/operating#choosing-the-driver)) and `plugins` (`TEHAS_PLUGIN`, still being built: [what it leaves visible](/contract#plugins)) |
| `millwright` | The app's login, or `null` before it is known |
| `poll` | What the polls of this factory cost, or `null` when no poll is on record or the state store cannot answer. `last` is the poll that most recently held the dispatcher's lock and `hour` is the sums of the polls that ended in the last hour; both are in the fields below, so a program that keeps a copy of them can add the cost of many hosts up. See [What a poll costs](/operating#what-a-poll-costs) for what each figure counts |
| `store` | The id of this factory's state store, 32 lower-case hex digits, made once when the store was first laid and kept by `tehas state pack`, `unpack` and `load`. A cycle id is unique within one `store`: a program that keeps the cycle ids it has seen keeps them per factory and `store`, and forgets them when `store` changes. `null` when the state store cannot answer. With the feature `cycle-ids` |

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

`api/api.schema.json` in the repository is the schema of the object, with
samples beside it that the command printed. The words of `features` are `api/features.txt`, in the
order the command prints them: within 1.x a word is only added, at the end.

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
  "limit": {"state": "none"},
  "running": [{"issue": 12, "cycle": "12-3", "since": "2026-10-17T11:20:00Z",
               "how": "approved", "by": "alice",
               "progress": {"step": "lens-review", "stepNumber": 7, "stepTotal": 9, "round": 2,
                            "stepSince": "2026-10-17T13:41:00Z"}}],
  "planning": [{"issue": 7, "turn": 2, "since": "2026-10-17T13:47:00Z"}],
  "demand": {"waiting": 2, "oldestWait": 5400},
  "hold": {"why": "pause", "text": "paused by the flag tehas pause wrote: weekend"},
  "held": [],
  "room": {"cycles": 0, "slice": 2},
  "versions": {"platform": {"release": "v0.25.0", "commit": "0123456789abcdef0123456789abcdef01234567", "latest": "v0.26.0", "behind": true},
               "rukas": {"version": "v0.14.54", "latest": "v0.14.54", "behind": false}},
  "previews": [{"number": 31, "state": "serving", "url": "https://uat-31.example.org/"},
               {"number": 30, "state": "building"}]
}
```

| Field | Meaning |
| --- | --- |
| `time` | When the health file was written: at the dispatcher's last poll, or when a cycle last ended |
| `age` | Seconds since `time` |
| `stale` | `true` when `time` is more than ten minutes ago. The exit status is then 3 |
| `state` | `pass`, `warn` or `fail`, as the [health file](/tools-health#tehasbot-health) says it |
| `reasons` | Why the state is not `pass`, for a person; empty on `pass` |
| `paused.local` | The pause flag `tehas pause` writes on this host is set |
| `paused.label` | `tehas:pause` holds the factory from the control issue: it is on the issue, or an account that may not lift it took it off |
| `readOnly` | `true` on a read-only instance (`TEHAS_READ_ONLY=on`), which writes nothing to GitHub (see [read-only](/operating#read-only)). Left out on any other instance, and by a release before the setting |
| `testFeatures` | There while `TEHAS_TEST_FEATURES` is set where health runs or the build is a lab build, as on a lab host (see [proving a building feature on a lab host](/tools-update#proving-a-building-feature-on-a-lab-host)). Left out on any other host, and by a release before it |
| `testFeatures.read` | `true` when this build reads the variable, which only a lab build does; `false` on a release, where it does nothing |
| `testFeatures.features` | The building features the variable names, in the registry's order |
| `testFeatures.lab` | `true` on a lab build (`install.sh --lab-build`), whether the variable is set or not; `false` on any other. Left out by a release before it |
| `pushFault` | The sandbox cannot push: a start's push check was refused, so no new cycle starts (see [the sandbox cannot push](/troubleshooting#the-sandbox-cannot-push)). Each field is `null` when the record does not say. Left out when the sandbox can push, and by a release before it |
| `pushFault.at` | When the fault was found, or when the poll last asked and the push was still refused |
| `pushFault.login` | The bot whose token was tried |
| `pushFault.line` | git's first line of why the push was refused |
| `limit` | The limit, as [`tehas limit --json`](#tehas-limit) prints it without `api` and `reason`: `{"state": "none"}` when none is set |
| `order` | The start order, as [`tehas order --json`](#tehas-order) prints it without `api` and `starts`, or `{"state": "unreadable"}` when the store did not answer. Left out by a factory that predates `start-order` |
| `running` | The running cycles: `issue`, `cycle` (the cycle id, `<issue>-<attempt>` such as `12-3`, which the cycle's line in [`tehas history`](#tehas-history) carries too, unique within one `store` of [`tehas api`](#tehas-api)), `since` (when it started, the second the cycle's history line prints as `started`), `how` and `by` (how and by whom it was asked for, in the words of `tehas history`; `by` is left out for a cycle run by hand), and `progress`, where the cycle is as its driver reports it: `step`, `stepNumber` (the step the cycle is on, from 1, as the driver counts it), `stepTotal` (how many steps the cycle has), `round` (the round of a step that has run before in this cycle, from 2, or the round the driver gave), `roundLimit` (the most rounds the step may run, when the driver gives a limit) and `stepSince` (when the step started). Each is left out when not known yet, and `progress` is left out when none is known. `stepNumber`, `stepTotal` and `roundLimit` are there only when the driver puts them on its `step-started` lines. `progress` is driver-specific and may change in a minor release: show it, do not decide on it |
| `planning` | The planning turns in flight: `issue`, `turn` (the session's turn that runs, from 1) and `since` (when it started; left out for a turn started before this was kept). A session whose last turn ended in error and waits for a person is not in flight and is not here; the text summary has a line for it |
| `demand.waiting` | Open issues that would start if there were room: `tehas:ready` applied last by an account in `TEHAS_ALLOWED_LABELERS` or the millwright, with no cycle and no planning session open, the parked issues where an allowed labeller answered `retry` to the park comment, by a reply or through `tehas answer`, and the refresh requests waiting, unless refreshes are held |
| `demand.oldestWait` | Seconds since the oldest of them was labelled or asked for; left out when none waits |
| `demand.labelAlone` | The issues `tehas:blocked` holds by the label alone, by number, at most 50: the label counts, and no comment of an allowed labeller or the millwright names their blockers, so they never start until one does or a person takes the label off. A marker in the issue's body is not read. Left out when there are none |
| `demand.refreshHold` | Why refresh requests start nothing on this host, for a person: the sandbox image cannot continue a pull request. Left out when nothing holds them or there are none |
| `demand` | `null` when no poll counted it in the last ten minutes, or the last poll could not read the queue |
| `hold.why` | One word for what holds new cycles back: `pause`, `slice`, `unreadable`, `resources`, `limit`, `red`, `count`, `awaiting`, `pressure`, `room` or `full`. Treat it as an open list: later releases add words |
| `hold.text` | The same for a person. Do not parse it |
| `hold` | `null` when nothing holds |
| `held` | The ready issues the last poll held one by one while it had room, each with its own reason; empty when none. An issue here starts nothing until a person clears its hold, so do not show it as next |
| `held.issue` | The issue number |
| `held.why` | One word for the reason: `parked` when its last cycle parked and its body has not changed since. Treat it as an open list |
| `held.text` | The reason for a person. Do not parse it |
| `held.clear` | What a person does to release it, such as editing the body or replying `retry` to the park comment |
| `room.cycles` | Free slots under `TEHAS_MAX_CYCLES`. Below 0 when more cycles run than it allows, as after it was lowered |
| `room.refreshes` | Free slots of the refresh lane, `TEHAS_REFRESH_SLOTS`; left out when no lane is kept: `TEHAS_REFRESH_SLOTS=0`, or no refresh request waiting and no refresh running |
| `room.maxAwaiting` | Room under `TEHAS_MAX_AWAITING`, the cycles running and pull requests waiting for review together. Below 0 when more wait than it allows. Left out when the last poll did not need to read it |
| `room.slice` | Cycles `tehas.slice` has room for |
| `room.limit` | Room under the limit: its `cycles` less the cycles running, 0 when the limit expired or cannot be read; left out without a limit. The others never count the limit |
| `room` | `null` when no poll got as far as the room check in the last ten minutes, as while the factory is paused |
| `versions` | The platform and the pi-rukas this factory runs, as the [health file](/tools-health#tehasbot-health) last read them in `checks.platform` and `checks.rukas`. Always there: a value that cannot be read is `null`, never left out, so a program tells "unknown" from a release without the field. The feature `status-versions` of [`tehas api`](#tehas-api) says a factory has it |
| `versions.platform` | The tehas release: `release`, `commit`, `latest` and `behind` |
| `versions.platform.release` | The release the host runs, such as `v0.25.0` or a prerelease such as `v1.0.0-alpha.1`; `null` on a host that runs a checkout, or with no platform reading |
| `versions.platform.commit` | The commit the release was built from (its `COMMIT` file), or the commit of the checkout; `null` when it is not known |
| `versions.platform.latest` | The newest published release, without prereleases; `null` when it is not known, and on a checkout host |
| `versions.platform.behind` | `true` when `latest` is newer than `release`; on a checkout host, `true` when the checkout is behind its upstream. `null` when it cannot be told |
| `versions.rukas` | The pi-rukas cycles run: `version`, `latest` and `behind`. All three `null` on a host with no pi-rukas reading |
| `versions.rukas.version` | The version the sandbox image's pi-rukas answers, such as `v0.14.53`; `null` when it is not known |
| `versions.rukas.latest` | The version of the pi-rukas commit the host follows (`PI_RUKAS_FOLLOW`); `null` when that commit is no release, as when the host follows a branch |
| `versions.rukas.behind` | `true` when the image is not on the commit the host follows, compared by commit, so it can be `true` while both versions are `null` |
| `previews` | The [previews](/tools-clean#tehasbot-preview) of the open pull requests, newest first, from `live/previews.json` as the previews last wrote it. The preview of the default branch is not one of them. Left out when there is no such list, as on a host that does not run previews (`TEHAS_PREVIEWS` is not `1`). A pull request the previews do not build, such as one from a fork, is not in it. The feature `status-previews` of [`tehas api`](#tehas-api) says a factory has the field |
| `previews.number` | The pull request's number |
| `previews.state` | The preview's own word for where it is: `building`, `serving`, `build-failed`, `no-recipe`, `over-limit` or `no-space`. Treat it as an open list: later releases add words |
| `previews.url` | Where the preview answers, `https://uat-<number>.` and the factory's domain. There only while the state is `serving` |

Everything in the object is as the health file wrote it at `time`, and is
`age` seconds old. The dispatcher writes it at each poll, every two minutes,
and when a cycle ends, so `running` and `room` can miss a cycle that started
since: it is in neither until the next poll. They can be up to ten minutes
old before `stale` is true. A program that acts on a free slot or an idle
issue reads `age` first.

`demand.waiting` is not the exact count of what would start: the poll still
checks, at a start, that the text is unchanged since the label and that no
open pull request closes the issue.

The source holds a JSON Schema of this object, `api/status.schema.json`, and
samples made by the code that prints it, `api/status.busy.json`,
`api/status.quiet.json` and `api/status.readonly.json` among them. Its tests keep
them in step, and a program can copy the samples into its own tests.

It reads `~/tehas/live/health.json`, and `protolab.json` and `previews.json`
beside it when they are there, and nothing else: no network, no instance
settings, and it writes nothing. Without a health file it prints nothing on
standard output and exits 3. Exit 0 on `pass` and `warn`; 3 on `fail`, a stale
file or no file; 64 on an argument other than `--json`.

## tehas limit

```
tehas limit [--json]
tehas limit set --cycles N [--plan on|off] [--for SECONDS | --until TIME] [--reason TEXT] [--json]
tehas limit clear
```

```json
{"api": 1, "state": "ok", "cycles": 1, "plan": false, "until": "2026-10-17T14:05:00Z", "reason": "held for alpha", "set": "2026-10-17T13:05:00Z"}
```

An outside cap on what the factory starts. Every command prints the limit as
it then is. `--json` works with the show form and with `set`, which then
prints the same object in place of its words; `clear` takes none.

`api/limit.schema.json` in the source says what the object holds, and
`api/limit.sample.json`, `api/limit.expired.json`, `api/limit.empty.json` and
`api/limit.unreadable.json` are samples of it, one for each `state`. The four
are what the command prints, so an expired limit prints `api`, `state`,
`cycles`, `plan`, `until`, `reason` and `set`, and an unreadable one prints
only `api` and `state`.

| Field | Meaning |
| --- | --- |
| `state` | `none`, `ok`, `expired` or `unreadable` |
| `cycles` | The most cycles running at once, a whole number from 0 to 9999: `tehas limit set` refuses anything above it with 64 (console/ops/src/holds.rs). Room for a new cycle is the smaller of `cycles` and `TEHAS_MAX_CYCLES`, less the cycles running, then the slice and `TEHAS_MAX_AWAITING` as always |
| `plan` | `false`: no new planning session. An answer to a planning question still runs: someone is waiting on it |
| `until` | When it expires, in the form `2026-10-17T15:00:20Z`: 20 characters, the month 01 to 12, the day 01 to 31, the hour to 23, the minute and second to 59, or 60 for the second. Left out when it lasts until cleared. An `until` of another shape makes the limit `unreadable` |
| `reason` | Shown as the hold's reason. One line of at most 200 characters, counted as characters; control characters become spaces. Left out when empty |
| `set` | When the limit was written, in the form of `until`. The same limit set again keeps it: the same limit is the same `cycles`, `plan` and `reason`, so a `set` that only moves `until` keeps it too, and so does setting an expired limit again. Any other limit takes the time it is written at, or the second after the `set` of the limit it replaces when that is no earlier: `set` is a whole second, and two limits never share one. Left out with no limit, with an unreadable one, and for a limit tehas 1.4 or older wrote, until that limit changes. `tehas limit --json`, `tehas limit set --json` and [`tehas drain --json`](#tehas-drain) print it; the `limit` of `tehas status --json` does not. A `set` in the file that is not a time of that form, or names a day its month does not have or a second 60, makes the limit `unreadable` |

- **An expired or unreadable limit holds every start**: no cycle, planning
  turn or answer. `tehas status` warns and says why. A file
  that cannot be read never counts as no limit. The poll still cleans up.
- **One reading.** `tehas status`, the dispatcher and the health page read the
  file the same way. A file that names a key twice is `unreadable`, and so is
  one whose `cycles` is not plain digits: `1.0`, `01` or `1e2` in a file
  repaired by hand is `unreadable`, never a limit of 0.
- **Running cycles are never stopped.** A lower limit only holds new starts.
- **Pause is separate.** A paused factory stays paused under any limit.
- `set` replaces the whole limit. The same arguments twice leave the same
  limit, and `clear` without a limit is fine. After a `clear` there is no
  `set` to read.
- **A raise asks for a poll.** A `set` or `clear` that lets more cycles run
  than before, while the last poll's demand has an issue waiting, writes
  `state/dispatch.again` as [`tehas wake`](#tehas-wake) does. The poll that
  acts on the new limit comes now, not at the next timer. An expired or
  unreadable limit counts as 0 here, and no limit as no cap.

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
`api/order.sample.json`, `api/order.expired.json`, `api/order.empty.json` and
`api/order.unreadable.json` are samples of it, one for each `state`. A store
that refuses or does not answer is no answer: the command exits 1, or 78 when
the store is not installed or cannot be used. An answer of the store that
holds no order this release can read is `unreadable`, taken as no order.

| Field | Meaning |
| --- | --- |
| `state` | `none`, `ok`, `expired` or `unreadable` |
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

### Start reasons

`tehas order --json`, the show form, adds `starts`: for each issue of
`issues`, in the same order, whether the dispatcher's last poll started it
and, if not, why not. `set` and `clear` print no `starts`.

```json
{"api": 1, "state": "ok", "issues": [12, 8], "only": true,
 "starts": {"at": "2026-11-20T10:00:00Z", "issues": [
   {"issue": 12, "start": true},
   {"issue": 8, "start": false, "reason": {"code": "blocked", "text": "tehas:blocked holds it: #3 is open"}}]}}
```

The reasons are the poll's own: every real poll writes what it decided of
each issue of the order and each ready issue to `state/starts.json`, from the
lines it logged as it held them, and `tehas order` reads that file. Nothing
about which issues start changes. `starts` is left out when no poll has
written the file, or it cannot be read, or the order is not `ok`.

| Field | Meaning |
| --- | --- |
| `starts` | What the last poll decided of the ranked issues |
| `starts.at` | When the poll that decided wrote it |
| `starts.issues` | One entry for each issue of `issues`, in the same order |
| `starts.issues.issue` | The issue, as in `issues` |
| `starts.issues.start` | `true`: that poll chose to start it. `false`: it did not, and `reason` says why |
| `starts.issues.reason` | Why it did not start: `code` and `text`. Left out with `start: true` |
| `starts.issues.reason.code` | One word for the reason. An open list: a later release adds codes, and a program takes a code it does not know as a hold it cannot name |
| `starts.issues.reason.text` | The reason for a person, mostly the poll's own log line. Do not parse it |

The codes today. A program decides on these; `other` is any hold the poll
logged that no code names, with its line in `text`.

| Code | The issue |
| --- | --- |
| `not-ready` | Is not a ready issue: no `tehas:ready` or `tehas:blocked` of an allowed account and no [`tehas start`](#tehas-start) |
| `not-in-order` | Is ready, outside an order with `only` |
| `no-room` | Is ready, but the poll had no room left when it came to it, so its own holds were not read |
| `held` | Was not looked at: the factory held every start, as `hold` of [`tehas status --json`](#tehas-status---json) says |
| `paused` | Was not looked at: the factory is paused |
| `unseen` | Was ranked after the last poll |
| `running` | Has a cycle running |
| `planning` | Has a planning session open |
| `question` | Waits for an answer to its question, or an answer to it cannot be taken |
| `blocked` | Is held by `tehas:blocked`: [`tehas blockers`](#tehas-blockers) shows the chain |
| `blocked-link` | Is held by an open blocked-by link |
| `open-pr` | Has an open pull request that closes it |
| `epic` | Is an epic, which never starts |
| `parked` | Parked, and its body has not changed since; or a retry was answered on an issue that is not parked, or its retry after a provider outage waits for a person |
| `too-large` | Names more files than `TEHAS_SIZE_MAX_FILES` |
| `text-changed` | Its title or text changed since its approval |
| `not-allowed` | Its label was applied by an account that may not authorise work |
| `label-off` | Its label was taken off since it was applied |
| `millwright` | Is held by a rule of the millwright's epics |
| `driver` | Is held by the choice of driver, a retry of a lean park included |
| `attachment` | Is held by an attachment its text links, or by text that says a file goes there and links none |
| `spec` | Is held by its `pi-spec v1` block |
| `read-only` | A read-only host started it before |
| `refresh-lane` | Waits for the refresh lane, or its pull request carries `tehas:refreshing` |
| `control-issue` | Is the control issue |
| `closed`, `not-issue` | Is closed, or is a pull request |
| `unreadable` | Something the rule needs could not be read this poll, or GitHub has not said it yet |
| `no-approval` | Is open and nobody made it ready (an end of a chain in [`tehas blockers`](#tehas-blockers)) |
| `other` | Any other hold the poll logged |

## tehas blockers

```
tehas blockers [--json]
```

```json
{"api": 1, "state": "ok", "at": "2026-11-20T10:00:00Z",
 "edges": [{"issue": 21, "kind": "marker", "on": 11, "by": "alice"},
           {"issue": 22, "kind": "link", "open": 2},
           {"issue": 30, "kind": "marker", "on": 31, "by": "alice"},
           {"issue": 31, "kind": "marker", "on": 30, "by": "bob"}],
 "cycles": [[30, 31]],
 "chains": [{"issue": 21, "cycle": false, "ends": [{"issue": 11, "start": false, "reason": {"code": "question", "text": "it carries tehas:question: it waits for an answer"}}]},
            {"issue": 22, "cycle": false, "ends": [{"issue": 22, "start": false, "reason": {"code": "blocked-link", "text": "2 blocked-by links of it are open; which issues they name is not read"}}]},
            {"issue": 30, "cycle": true, "ends": []},
            {"issue": 31, "cycle": true, "ends": []}]}
```

The holds between issues the dispatcher's last poll read, from the same
`state/starts.json` as the start reasons: no network, and it writes nothing.
A chain of blockers that holds many issues behind one shows here as one
thing: free the issue at its end and the chain starts.

`api/blockers.schema.json` in the source says what the object holds, and
`api/blockers.sample.json` is a sample of it.

| Field | Meaning |
| --- | --- |
| `state` | `ok`; `none` before any poll recorded its holds, which exits 3; `unreadable` when the record cannot be read, which exits 1. With `none` and `unreadable` only `api` and `state` are there |
| `at` | When the poll that read the holds wrote them |
| `edges` | Each hold the poll read |
| `edges.issue` | The issue that is held |
| `edges.kind` | `marker`: the newest `<!-- tehas:blocked by=N -->` comment of an allowed account or the millwright on the issue, edited by nobody else, names `on`, which is not closed. `link`: blocked-by links of the issue are open. An open list, as the codes are |
| `edges.on` | The issue a marker names. A link has none: GitHub's answer the poll reads counts the links and names no issue |
| `edges.by` | Who wrote the marker comment |
| `edges.open` | How many blocked-by links of the issue are open |
| `cycles` | Each cycle among the marker edges, its issues lowest first. None of them starts until one of the comments changes |
| `chains` | One entry for each issue with an edge |
| `chains.issue` | The issue |
| `chains.cycle` | `true` when its chain reaches a cycle |
| `chains.ends` | The issues at the ends of its chains: an issue the chain reaches that names no blocker of its own, or one held by links. Empty when every way leads into a cycle |
| `chains.ends.issue` | The issue at the end |
| `chains.ends.start` | `true` when the last poll chose to start it |
| `chains.ends.reason` | What holds it: its own start reason when the poll decided on it, else what the poll's lists say of it: `running`, `question` (it carries `tehas:question`), `planning`, `open-pr` (it waits for review or merge) or `no-approval` (it is open and nobody made it ready) |
| `chains.ends.reason.code` | The code, from the open list of the start reasons |
| `chains.ends.reason.text` | The reason for a person. Do not parse it |

The edges are those of the ready issues the poll read: a blocked issue that
is also in another flow, such as planning, is not among them. Exit 0; 1 the
record cannot be read; 3 no poll has recorded one; 64 an argument other than
`--json`; 78 without an instance.

## tehas start

```
tehas start <issue> [--json]
tehas start <issue> --clear [--json]
```

```json
{"api": 1, "issue": 12, "approval": {"at": "2026-10-17T14:05:00Z", "by": "host", "how": "host", "bodySha256": "...", "titleSha256": "..."}, "cleared": false}
```

Approves an open issue for a start, from the host, in place of a
`tehas:ready` label from an allowed account. It runs as the factory's user and
needs no GitHub login of its own: the approval is a row of the state store of
its own kind, `host`. It reads the issue from GitHub once, to refuse a number
that is no open issue and to bind the title and text the approval covers.

Nothing starts at once and nothing passes a hold. The next poll starts the
issue under the rules every start passes: the slots and the limit, a pause or
a drain, a blocked-by link or `tehas:blocked`, an open pull request that
closes it, a park on the same text and the start order, which sorts it with
the ready issues. A title or text changed since the approval starts nothing,
and the poll log says to run `tehas start` again. Running it again replaces
the approval that waits; any start of the issue uses it up. A start that
gives up before its cycle runs, on a GitHub error or a busy store, leaves the
approval on record, and the next poll tries again. When the cycle's unit does
not come up, the approval is taken off and the issue's comment says to run
`tehas start N` again.

`--clear` takes back an approval that has not started a cycle, and answers
it, or `null` with `cleared: false` when none waited.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `issue` | The issue |
| `approval` | What was recorded or taken back. `null` from a `--clear` that found none |
| `approval.at` | When it was recorded |
| `approval.by` | `host` |
| `approval.how` | `host` |
| `approval.bodySha256` | The SHA-256 sum of the issue's text the approval covers |
| `approval.titleSha256` | The SHA-256 sum of its title |
| `cleared` | `true` when `--clear` took an approval back |

`api/start.schema.json` in the source says what the object holds, and
`api/start.sample.json`, `api/start.cleared.json` and `api/start.none.json`
are samples of it.

The start comment on the issue, [`tehas history`](#tehas-history) (`"how":
"host"`) and `tehas status` say the cycle was started from the host. A cycle
killed before it reported keeps its record open with its lock free; `tehas
status` says so, `killed before it reported: tehas stop N ends its record`,
in place of `running`. Once it has not been heard from for two minutes, the
next poll ends the record as `lost`, with its line in
[`tehas history`](#tehas-history). The
millwright may ask for its finish as for one a person's label started.

Exit 0 recorded, taken back or nothing to take back; 1 GitHub did not answer,
or the store refused; 3 GitHub has no open issue of that number (none, a pull
request or a closed issue); 64 on a bad argument, which writes nothing; 78
without an instance.

## tehas answer

```
tehas answer <issue> <choice> --by <login> [--json]
tehas answer <issue> --text <file>|- --by <login> [--json]
```

```json
{"api": 1, "issue": 12, "question": {"id": 1, "kind": "park"}, "answer": {"choice": "retry", "option": null, "by": "alice", "how": "answer", "at": "2026-10-17T14:00:20Z"}, "replaced": false}
```

A factory that has the verb lists `answer` in `features` of
[`tehas api --json`](#tehas-api): a program that manages factories of more
than one release checks for that word, not for a version, before it answers
this way, and answers an older factory as it did before.

Answers the question an issue waits on, for a person: the question of a
stopped ticket (a park) or of a planning session. It is the way in for a
program acting for a person, such as an industry or a chat, beside a reply on
the issue (#2782). Tested only, not yet seen on a
host.

`--by` names the person. It is trusted as the host's caller, as `tehas start`
trusts its caller, and must be an account of `TEHAS_ALLOWED_LABELERS`; any
other stops with exit 78 and records nothing. The choices:

| Choice | Of | What the poll does |
| --- | --- | --- |
| `retry` | a stop | Starts the cycle again with the issue and its comments |
| `retry-text` | a stop | Starts it again with the issue text only |
| `park` | a stop | Leaves it stopped, and the question is answered |
| `option:<k>` | a planning question | The option numbered `<k>`, from 1, as the question lists it |
| `update` | a planning question | The option "Update this issue" |
| `end` | a planning question | Ends the planning session |
| `--text <file>` | a planning question | A reply in words, read from the file |
| `--text -` | a planning question | The same, read from standard input, for a program that may not write a file on the host (an industry reaches a factory only through its commands). The words never go in the arguments. At most 1 MiB, as a file; empty input or more stops with 64 |

The answer is a row of the state store bound to the question on record of the
issue, with who gave it, when, and how (`answer`). Nothing happens at once.
The next poll acts on it as on a reply of that account that names the same
choice, under the rules every answer passes: a retry waits for room, and
starts only while the issue is still stopped, its text unchanged and no pull
request closes it. A reply from an allowed account after the answer is the
newer word: it stands, and the answer is taken away. An answer the poll
cannot take (a retry the stop does not offer, an account no longer allowed,
an issue no longer stopped) is taken away, the poll says why on the issue,
and a reply to the question is read as it would be without it. Running it
again before the poll replaces the answer that waits. Once the question is
answered or a new one is asked, an answer to the old one does nothing. A
retry answered lifts a park's hold, also once `tehas:question` is off. A box
ticked in a comment of before 2.0 is no answer, and the poll does nothing
with it.

The cycle a retry starts is recorded with `"how": "answer"` in
[`tehas history`](#tehas-history), started by `--by`, and its start comment
says it was authorised by that account's answer. A planning turn records the
account as the one who answered it.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `issue` | The issue |
| `question.id` | The state store's own number of the question, as [`tehas question`](#tehas-question) prints it. Before 2.0 it was the id of the question's comment |
| `question.kind` | `park` for a stopped ticket, `plan` for a planning question |
| `answer` | What was recorded |
| `answer.choice` | `retry`, `retry-text`, `park`, `option`, `update`, `end` or `text` |
| `answer.option` | The number of the option, from 1, of `option` and `update`; `null` for the rest |
| `answer.by` | The account the answer was given for. With `how` `plugin`, the bot account |
| `answer.how` | `answer`. `plugin` is reserved for 2.1, for a plugin's answer under an `answer:<kind>` grant; no command prints it in 2.0, and a reader never takes it for a person's act |
| `answer.plugin` | The plugin that answered, with `how` `plugin` alone; left out otherwise. Reserved for 2.1 |
| `answer.plugin.name` | The plugin's name |
| `answer.plugin.commit` | The plugin's pinned commit, 40 hex digits |
| `answer.at` | When it was recorded |
| `replaced` | `true` when an answer given before to the same question was waiting, and this one took its place |

`api/answer.schema.json` in the source says what the object holds, and
`api/answer.sample.json` and `api/answer.option.json` are samples of it.
`api/answer.plugin.json` is the shape reserved for a plugin's answer.

Exit 0 recorded; 1 the store refused or is not installed; 3 no question of that
kind waits on the issue, or the question does not offer that option; 64 on a
bad argument, which writes nothing; 78 `--by` may not authorise work, or
without an instance.

## tehas question

```
tehas question <issue> [--json]
tehas question [--after <id>] [--json]
```

```json
{"api": 1, "issue": 12, "question": {"id": 1, "kind": "park", "comment": 4003, "askedAt": "2026-10-17T14:00:20Z", "choices": [{"label": "Retry with the issue and all its comments", "choice": "retry"}, {"label": "Retry with the issue text only", "choice": "retry-text"}, {"label": "Leave it parked", "choice": "park"}]}}
```

The question an issue waits on and the choices it offers, for a program that
answers for a person with [`tehas answer`](#tehas-answer) and draws the
choices for them. Read only: it reads the state store and nothing else.
Tested only, not yet seen on a host.

A planning question offers its options, numbered from 1 as its comment lists
them, and then the end of the session. A stop offers the choices it was asked
with. A stop asked before the store kept its choices (schema 25) lists all
three, and the poll refuses an answer its comment does not offer. A reply in
words (`tehas answer --text`) is not listed: a planning question always
takes one. A factory that has the command lists `question` in `features`.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `issue` | The issue |
| `question` | The question that waits |
| `question.id` | The state store's own number of the question, given in order and never reused. Before 2.0 it was the id of the comment that asked it |
| `question.kind` | `park` for a stopped ticket, `plan` for a planning question |
| `question.comment` | The id of the comment on the issue that asked the question, as the code host numbers it. Left out when the store has no comment for it, never null |
| `question.askedAt` | When it was asked. Before 2.0 the field was `asked_at` |
| `question.choices` | Exactly the choices it offers, in the order it lists them |
| `question.choices.label` | The words a person sees for the choice |
| `question.choices.choice` | The word [`tehas answer`](#tehas-answer) takes for it: `retry`, `retry-text`, `park`, `option:<k>`, `update` or `end` |

With no issue the command lists questions, for a program that follows every
question of the factory instead of asking issue by issue:

```json
{"api": 1, "questions": [{"id": 1, "issue": 12, "kind": "park", "comment": 4001, "cycle": "12-1", "askedAt": "2026-10-17T14:00:20Z", "withdrawnAt": "2026-10-17T14:00:50Z", "choices": [{"label": "Retry with the issue text only", "choice": "retry-text"}, {"label": "Leave it parked", "choice": "park"}]}, {"id": 2, "issue": 12, "kind": "plan", "comment": 4002, "askedAt": "2026-10-17T14:00:20Z", "answeredAt": "2026-10-17T14:01:20Z"}]}
```

`tehas question --json` lists the question each issue waits on, the one
`tehas question <issue>` prints for it. `tehas question --after <id> --json`
lists every question whose number is greater than `<id>`, the answered and
the withdrawn among them: `--after 0` lists them all, and no question is
trimmed. Both are in number order, and an empty list exits 0. A program
keeps the greatest `id` it has seen and asks for the questions after it, so
it reads each question once. The object is `api/questions.schema.json`. A
factory that has the list names `questions` in `features`. Tested only, not
yet seen on a host.

| Field | Meaning |
| --- | --- |
| `questions` | The questions, in the order of `id` |
| `questions.id` | The state store's own number of the question, given in order and never reused |
| `questions.issue` | The issue it was asked on |
| `questions.kind` | `park` for a stopped ticket, `plan` for a planning question |
| `questions.comment` | The id of the comment on the issue that asked it. Left out when the store has no comment for it, never null |
| `questions.cycle` | The cycle that stopped, `<issue>-<attempt>`. A stop's question alone |
| `questions.askedAt` | When it was asked |
| `questions.answeredAt` | When it was answered. An answered question alone |
| `questions.withdrawnAt` | When the host withdrew it, as when a later question took its place. A withdrawn question alone |
| `questions.choices` | The choices it offers, as `question.choices`. Left out for a planning question the issue no longer waits on: the store keeps its options only while it waits |
| `questions.choices.label` | The words a person sees for the choice |
| `questions.choices.choice` | The word [`tehas answer`](#tehas-answer) takes for it |

`api/question.schema.json` in the source says what the object holds, and
`api/question.sample.json` and `api/question.plan.json` are samples of it.

Exit 0 a question waits; 1 the store refused or is not installed; 3 no
question waits on the issue; 64 on a bad argument; 78 without an instance.

## tehas wake

```
tehas wake [--json]
```

```json
{"api": 1, "asked": true}
```

Asks for a poll now and returns at once, for a program that knows the
repository changed: a webhook receiver, a scheduler, a script. It does what
[`tehas dispatch --soon`](/tools-dispatch#tehas-dispatch) does: it writes
`state/dispatch.again`, and `tehas-dispatch.path` starts the poll as its unit,
as the factory's user. It makes no call to GitHub and does not wait for the
poll. A factory whose `tehas api` lists the feature `wake` has it.

- **A poll is running**: the ask waits, and one more poll starts when that one
  ends. Asking again before then is the same one ask, so a caller may ask on
  every event it gets.
- **The factory is paused or drained**: the ask is taken and the poll runs. It
  cleans up as every poll does and starts nothing new.

`api/wake.schema.json` in the source says what the object holds, and
`api/wake.sample.json` is the object.

| Field | Meaning |
| --- | --- |
| `asked` | `true`: the poll is asked for. `false`: the file could not be written, and the exit status is 1 |

Exit 0 the poll is asked for; 1 the file could not be written; 64 an argument
other than `--json`, which writes nothing; 78 without an instance.

## tehas drain

```
tehas drain [--wait SECONDS] [--json]
```

```json
{"api": 1, "drained": false, "running": [12], "limit": {"state": "ok", "cycles": 0, "plan": false, "reason": "draining", "set": "2026-10-17T14:05:00Z"}}
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
| `limit.plan` | `false`: no new planning session |
| `limit.until` | Left out: a drain's limit lasts until `tehas limit clear` |
| `limit.reason` | `draining` |
| `limit.set` | When the limit was written, as [`set`](#tehas-limit) of `tehas limit --json`. A drain over a drain keeps the first one's |

Exit 0 when no cycle is running; 3 when some still are at the end of the
wait, or a poll held its lock for the whole of it; 64 on a bad argument; 78
without an instance.

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
handoff. `tehas-cycle@.service` still stops the cycle 8 hours after it
started, frozen or not, so a freeze of hours late in a cycle can end it while
it is paused, as a cycle that stopped. A stop of the cycle or of the factory still works on a frozen cycle,
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
| Rules | An account of `TEHAS_ALLOWED_LABELERS` may ask. [The millwright](/operating#the-millwright) may ask when a person approved a cycle of the issue, the body is the one they approved, and nobody outside the list changed the title or text since. No cycle of the issue may be running. An ask of anyone else is ignored and the label stays |
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
{"api": 1, "runs": [
   {"cursor": "1760700000-12-41", "kind": "work", "issue": 12, "cycle": "12-3",
    "started": "2026-10-17T11:20:00Z", "ended": "2026-10-17T13:20:00Z",
    "status": 10, "outcome": "handoff", "end": "handoff",
    "pr": "https://github.com/acme/widget/pull/9", "change": 9,
    "progress": {"step": "review", "stepNumber": 9, "stepTotal": 9},
    "cap": "awaiting-human-merge", "parkReason": "merge",
    "failureKind": "review", "failureCause": "findings open",
    "provider": false, "parked": true, "repeat": 1, "by": "alice", "how": "approved"},
   {"cursor": "1760700300-7-42", "kind": "plan", "issue": 7,
    "started": "2026-10-17T13:20:00Z", "ended": "2026-10-17T13:25:00Z",
    "status": 0, "outcome": "finished", "turn": 3, "by": "alice",
    "label": "tehas:refine", "filed": ["https://github.com/acme/widget/issues/41"]}],
 "next": "1760700300-7-42"}
```

What has ended, oldest first: one line, a run, for each cycle and each planning
turn, in the order the state store wrote them.
`kind` is `work` for a cycle and `plan` for a planning turn, so a program reads
the two the same way. `progress` says where the step a cycle ended in was: the
step, and `stepNumber`, `stepTotal` and `round` as its last heartbeat at that
step said, each left out when it is not known, as `running` of
`tehas status --json` leaves them out. Like that one, `progress` is
driver-specific and may change in a minor release. `repeat` is on the park of a run of parks only: the parks
in a row that stopped exactly as this one did, this one not counted, so 1 on the
first repeat. It is counted over every line the store keeps, so a line has the
same count whatever page lists it, and `cycle.parked` carries the same count. It reads the history of both from the state store,
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
program can copy them into its own tests. One schema of `runs` covers both
kinds of line, so a field only one kind prints is optional there and `kind`
says which line it belongs to. The table gives every field of the schema as the
schema names it, under `runs` for a field of a line.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `runs` | The lines, oldest first: one for each cycle and each planning turn that ended. Every start of a cycle has its line: one the cycle refused (`refused`), one `tehas stop` stopped (`stopped`) and one that died without a word (`lost`) too |
| `next` | The cursor to keep, as the last line's `cursor`, or `null` when the listing holds no line and none was asked for with `--after` |
| `runs.cursor` | The seconds of the end, the issue, and the line's place in the order the store wrote the ends, as `1760700000-12-41` |
| `runs.kind` | `work` for a cycle, `plan` for a planning turn |
| `runs.issue` | The issue the line is about |
| `runs.cycle` | The cycle id, `<issue>-<attempt>` such as `12-3`, on a work line: the same as `cycle` of the running cycle in [`tehas status --json`](#tehas-status---json), unique within one `store` of [`tehas api`](#tehas-api). A program keys cycles on it and lines on `cursor`. With the feature `cycle-ids` |
| `runs.ended` | When the cycle or the turn ended |
| `runs.started` | When it started, the second `since` of `tehas status` said while it ran. Always on a work line: a line the store holds no start for (one from the ledgers of v0.30) has its end here. On a plan line, left out when the record holds no start |
| `runs.status` | Its exit status; left out when the unit left no readable one, as a stopped or lost cycle leaves none |
| `runs.outcome` | What it reported: for a cycle the status of its end, for a turn `finished`, `failed`, `interrupted`, `not-started`, `not-readable` or `lost` |
| `runs.end` | The word a program decides on, on a work line, one of a closed set: `offered` (it left an open change and did not park), `merged`, `handoff`, `parked`, `failed`, `refused` or `stopped`. It comes from `outcome`, `status`, `change`, `parked` and `failureKind` by the table of [the end word](#the-end-word); `outcome` keeps the store's status word. A new word is a major change. With the feature `cycle-ids` |
| `runs.turn` | The turn's number in its session, from 1. A field of a plan line |
| `runs.progress` | Where the step a cycle ended in was, as its driver reported it. Driver-specific and may change in a minor release. Left out when nothing of it is known. A field of a work line |
| `runs.progress.step` | The step a cycle ended in |
| `runs.progress.stepNumber` | The step the cycle was on, from 1, as its last heartbeat at that step said. Left out when it is not known, as `running` of [`tehas status --json`](#tehas-status---json) leaves it out |
| `runs.progress.stepTotal` | How many steps the cycle has, from the same heartbeat |
| `runs.progress.round` | The round of a step that ran before in this cycle, from 2, or the round the driver gave |
| `runs.cap` | The cap it stopped at. A field of a work line |
| `runs.parkReason` | Why it was parked. A field of a work line, left out when the end named no reason |
| `runs.failureKind` | The failure the millwright named. A field of a work line |
| `runs.failureCause` | Its cause, as the millwright gave it. A field of a work line |
| `runs.provider` | Whether its provider was asked. A field of a work line |
| `runs.parked` | Whether the cycle parked. A field of a work line |
| `runs.repeat` | The parks in a row that stopped exactly as this one did, this one not counted, so 1 on the first repeat. On the park of a run of parks only. A field of a work line |
| `runs.pr` | The pull request, as an address of the repository the instance names. A field of a work line that had one |
| `runs.change` | The number of the change a work line names, so a program reads no address; `pr` is its address. With the feature `cycle-ids` |
| `runs.by` | Who asked for it. Left out for a cycle run by hand, which names no account |
| `runs.how` | How it was asked for: `approved` (a label an allowed account applied), `refresh`, `outage-retry`, `millwright`, `host` ([`tehas start`](#tehas-start)), `reply` (a retry a person's reply answered), `answer` (a retry answered through [`tehas answer`](#tehas-answer)) or `hand`. A retry ticked before 2.0 was recorded `retry-tick`: the step of the state store to schema 27 rewrites it to `reply` in `state/tehas.db` (cycles, approvals and the cycle history), and analytics rewrites it in `runs.how` of `state/analytics.db` each time it opens the file. A start approved by a label was recorded `label` before 2.0: the state store's step to schema 30 rewrites it to `approved` there, and the analytics' step to schema 7 in `runs.how` |
| `runs.label` | The label that started the turn's session, `tehas:plan` or `tehas:refine`. A field of a plan line |
| `runs.recorded` | When the line was written, on a work line written after its end: a lost cycle's, whose `ended` is when it was last heard from, written by the next start or by the poll. Its place in the cursor's order is when it was written, so `--after` lists it once, after the lines written before it |
| `runs.signalled` | `true` on a work line when a signal stopped the cycle; left out otherwise |
| `runs.auto` | `true` for a refine the factory started on its own when a cycle parked as underspecified or too-large. Left out otherwise. A field of a plan line |
| `runs.filed` | The issues the turn filed, when it filed any. A field of a plan line |

### The end word

`end` takes the first row that matches, top to bottom, from the line's
`outcome`, `status`, `change`, `parked` and `failureKind`. `cap`,
`parkReason` and `signalled` decide no end. A parked line never ends
`merged`, `stopped` or `offered`.

| # | When | End |
| --- | --- | --- |
| 1 | `outcome` is `merged` and `parked` is false | `merged` |
| 2 | `outcome` is `closed`, `interrupted` or `stopped`, and `parked` is false | `stopped` |
| 3 | `outcome` is `refused` | `refused` |
| 4 | `status` is 3, 4, 5, 40, 64 or 78 | `refused` |
| 5 | the line has a `change` and `parked` is false | `offered` |
| 6 | `outcome` is `handoff` | `handoff` |
| 7 | `outcome` is `killed`, `stalled`, `timeout`, `aborted`, `unknown`, `failed` or `lost`, or `failureKind` is set and is not `cap` | `failed` |
| 8 | any other line | `parked` |

Exit 0 when it printed, also with nothing to list; 1 when the state store was
busy, which prints nothing; 64 on a bad argument, which reads nothing; 78
without an instance, without a valid `TEHAS_REPO` or without a state store that
can be used.

## tehas export

```
tehas export [--json] --after CURSOR <dir>
```

```json
{"api": 1, "version": 1, "runs": [{"cursor": "1760700000-12-41", "kind": "work", "issue": 12,
  "cycle": "12-3", "ended": "2026-10-17T13:20:00Z", "started": "2026-10-17T12:40:00Z",
  "status": 0, "outcome": "merged", "end": "merged"}],
 "next": "1760700000-12-41", "to": "/home/boss/archive"}
```

`version` is the layout of the export directory, 1 today. The directory says it
too, in `<dir>/export.json`: `{"version": 1}`, written on every export. A
program that reads an archive reads that file first, and a later layout comes
with a higher number. The rest of the object is the one
[`tehas history --json`](#tehas-history) prints for what was copied, and `to`,
the directory.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `version` | The layout of the export directory, 1 |
| `runs` | The lines copied, each as [`tehas history --json`](#tehas-history) prints it |
| `next` | The cursor to keep: the last line's `cursor`, or the one given to `--after` when nothing was copied |
| `to` | The directory written, an absolute name |

`api/export.schema.json` in the source says what the object holds, and
`api/export.sample.json` and `api/export.empty.json` are samples of it.

The copy a person reads afterwards. For every line `tehas history` gives after
`CURSOR`, it writes `<dir>/cycles/<cursor>/`: the cycle's `result.json`, its
`logs/`, its agents' `transcripts/`, and `history.json`, the line as `tehas history` publishes it. A planning turn brings
`plan.json` and no cycle files: the session as the state store has it now
(`plan-show`), with `issue`, `session`, `turns`, `options`, `pending`,
`pending_outcome`, `failed`, `running`, `started_at`, `comment_id`, `lost`,
`by`, `label`, `brief`, `turn_rc`, `opened_at`, `ended_at` (times in Unix
seconds), `answers` (each `turn`, `by` and `millwright`, whether the millwright
answered it), `filed`, `answer` and `notes`
(`millwright_refused` and `held`). A session that ended stays
on the store until the next `tehas:plan` of the issue replaces it, so a turn
that never made a session still brings the one it ended; a turn whose session
was replaced since, by one opened after the turn ended, brings no `plan.json`. A planning session the store did not
give is a cycle not written, and so is one with a file that cannot be read:
the others are copied, and the command exits 1. A cycle whose logs the cleaner has already removed is not an
error: those files are simply not there.

The console does the copying, and every file goes through its mask, the one
masker tehas has, with the same names of the host a cycle is given (the host
aliases, the model hosts and the bot's email), so a token or a private address
that reached a log does not reach the copy. A file is masked whatever bytes it
holds. A file over 32 MiB is left out: an export is for reading. The mask catches accidents;
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

The tar holds the whole store, so the moved factory keeps its `store` of
[`tehas api`](#tehas-api) and its attempts: a cycle id it gave before the
move is never given again after it.

The tar holds `tehas-state.json` first,
`{"api": 1, "tag": "v1.0.0", "schema": 18}`, then `state/`, `repos/`, `live/`
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
is there (it names the unit). The `systemctl --user` read and each `tar` run
under a bound: a systemd bus that does not answer in 5 seconds is refused, so a
hung bus fails the no-active-unit check closed, and a `tar` that never ends is
stopped at 10 minutes. `--rehearse` skips those refusals, to size or
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
{"api": 1, "verb": "check", "ok": true, "schema": 18, "integrity": "ok", "rows": {"cycles": 412, "...": 0}}
```

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `verb` | `check`, the store's verb that answered |
| `ok` | `true`. A store that fails its check prints no object |
| `schema` | The schema version of the store |
| `integrity` | `ok`: SQLite's integrity check passed. Any other answer is no object: see the exit statuses |
| `rows` | Each table of the store and its number of rows. The keys are the store's table names, which are internal: a later schema may add, rename or drop a table, and its key with it |

`api/state-check.schema.json` in the source says what the object holds, and
`api/state-check.sample.json`, a new store's, is a sample of it.

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
{"api": 1, "target": "/home/tehas/copies/tehas-before-deploy.db", "schema": 18, "rows": 4182, "ms": 37}
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
feature `state-copy`.

## tehas state load

```
tehas state load --from <file> [--json]
```

```json
{"api": 1, "source": "/home/tehas/copies/tehas-before-deploy.db", "schema": 18, "rows": 4182, "ms": 52}
```

The inverse of [`tehas state copy`](#tehas-state-copy): fills the factory's
store from a copy that `tehas state copy` made, to restore a factory on a new
host. It takes the copy's `store` of [`tehas api`](#tehas-api) and its
attempts with it, so the cycle ids of the copy stay its own. Run it as the factory's user, on a factory that is drained, with
[`tehas drain`](#tehas-drain), and whose store is new or empty. A relative
name is taken from the working directory.

It refuses, in one line on standard error and with the store as it was:

- a factory that is not drained, or where a cycle or a planning turn runs;
- a store that holds history: a row of a cycle, a planning turn, an approval,
  a park, a question, an ending or an action. The poll's
  samples and caches, which a drained factory still writes, are not history,
  and the copy's replace them;
- a copy that is not there, is not a store or fails SQLite's
  `PRAGMA integrity_check`;
- a copy of a newer schema than this release reads, naming both numbers;
- a copy older than v0.30.0 wrote, below schema 12: load it into a v0.30
  release first.

A copy of an older schema, 12 or later, is brought forward by the same steps
an update runs on the store. The copy is read from a file of its own beside the store, so
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

## tehas analytics copy

```
tehas analytics copy --to <file> [--json]
```

```json
{"api": 1, "schema": 4, "from": 1792022400, "to": 1792800000, "runs": 42}
```

Writes a consistent copy of the factory's analytics database to `<file>`, mode
600: 365 days of runs, their steps and roles, issue events and poll samples.
The schema, every column and how a reader keeps up with it are on
[Analytics](/analytics). The copy is masked: it holds no token, no URL with a
host and no address. A program runs the command, then copies the file, over
SSH, `incus exec` or `kubectl exec`, as it runs any other.

| Field | Meaning |
| --- | --- |
| `api` | The API version, 1 |
| `schema` | The copy's schema version, `PRAGMA user_version`. A program that knows another version does not read it |
| `from` | The oldest start of a run in the copy, Unix seconds; left out when it has no run |
| `to` | The newest end of a run in the copy (its start for a run that has no end), Unix seconds; left out when it has no run |
| `runs` | How many runs the copy holds |

`api/analytics-copy.schema.json` in the source says what the object holds, and
`api/analytics-copy.sample.json` and `api/analytics-copy.empty.json`, a copy
with no run, are samples of it. The fields are read from the copy, not from the
factory's database, so they describe the file the program fetches. The command takes no lock a cycle holds
and makes no network call. It never replaces a file: name a new one each time.
A copy fetched again holds the same rows, and more: rows are keyed so that
storing them twice is harmless (see [Analytics](/analytics#fetching-it)).

The feature `analytics` in [`tehas api`](#tehas-api) says the command exists.
It does not say analytics is on: exit 0; 1 when the program is not installed,
the database cannot be read, or `<file>` exists or cannot be made; 64 on a bad
argument; 78 without an instance or when the instance has
`TEHAS_ANALYTICS=off`, which writes nothing. `tehas analytics export <file>`, the
name before 1.0, exits 64 and names `tehas analytics copy --to`.

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

The feature `clean` in [`tehas api`](#tehas-api) says the hourly cleaner runs on this host.
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
{"api": 1, "time": "2026-10-31T23:09:32Z", "dockerRoot": "/var/lib/docker",
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
| `time` | When the answer was made, UTC. The command's own reads write nothing, so this says how old the answer on the terminal is |
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
| `full.places.cut` | The bound the walk ran out of: `entries`, `depth`, `time` or `unreadable`. Left out when the figure is the whole of the place |
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
in the source, `api/disk-cycles.sample.json` is a sample of it and
`api/disk-cycles.full.json` one with `--full`. The
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
  carries the object as `hostMessages`, and the banner of the front page
  lists each message: Alert, Warning or Notice, its text, when it was set and
  its link.
- The text and the link are the writer's: the dashboard sets both as text,
  and the terminal leaves out direction overrides and zero-width characters.

The checks, the list and the file are the console's
(`console/ops/src/host_message.rs`).

Exit 0 done; 1 the list could not be read or written, or another
`tehas host-message` held the lock for 10 seconds, which writes nothing; 64 a
bad argument or a 21st message, which writes nothing; 78 without an instance.

## Events

An event is a fact tehas records once, named and typed: a cycle started, a
question was asked, a limit changed. Every event is pull first: the factory
API prints the same fact later, its witness, with the same identity and the
same values. The design is `docs/specs/2026-11-22-events.md` in the source.

What ships today is the contract: the names, the envelope, a schema and two
samples per name in `api/`, and the kit's `events.js`. A factory that names
`events` in `features` raises the events of a cycle: `cycle.started` once
its start is on record, and after its report `cycle.parked`,
`question.asked` (for a park's question), `cycle.ended` and
`change.offered`. The end of every start is raised by whoever wrote its
line of history: the cycle after its report, the cycle for a start it
refused before its runner ran, the next start for an open record it ended
as lost, `tehas stop` for a record it ended, and the dispatcher's poll for
a cycle it ended as lost. A writer that finds the line already written
raises nothing. The dispatcher raises `question.asked` for each question
it records: a planning turn's, a question asked again in 2.0's words and a
park's question a start puts back. A question asked again is a new
question number, so one comment on GitHub can come with two
`question.asked`. tehas raises each end and each question at most once.
A writer whose answer from the state store was lost, a write that
committed but whose answer was not read and was tried again, raises
nothing, so an end or a question can go unraised. `tehas history` and
`tehas question --after` are the record: a consumer reads them for what
it missed. Each event is built from what
the state store then holds, so it carries what the witness prints. The
cycle's and the dispatcher's logs, and for `tehas stop` the factory's log
`logs/dispatch.log`, have one line per event, `event` and its envelope as
JSON. tehas pushes no event to another program and no page listens to one:
a program that runs many factories builds the same envelopes from its
polls, and a page of its own can raise them on the kit. The other names are
raised by later steps of 2.0. `change.offered` is not raised for a line
that ends `refused`, by its word or by its exit status (64 and 78 among
them), even when the line names a change.

The names are `api/events.txt`, one per line, in this order:

| Event | The fact | Key | Witness |
| --- | --- | --- | --- |
| `cycle.started` | A cycle's start is on record | the cycle id, `12-4` | the cycle in `running` of `tehas status --json`, or its first work line in `tehas history --json` |
| `cycle.parked` | A cycle's history line says it parked | the cycle id | a work line with `parked` true |
| `question.asked` | A question is recorded | the store's number of the question, `5` | the question in `tehas question --json --after <id>` |
| `cycle.ended` | A cycle's history line is written | the cycle id | the work line in `tehas history --json` |
| `change.offered` | A cycle left a change for review | the cycle id | a work line with a change, no `signalled`, and an end other than `refused` or `merged` |
| `factory.limit-changed` | A write changed the limit | the limit's `set`, or `observed` when it has none | `tehas limit --json` |
| `ticket.acted` | An allowed account's act is recorded | `<ticket>@<act>@<at>`, `12@Approve@2026-11-22T11:10:00Z` | the act in the ticket's history |
| `ticket.closed` | A ticket the factory follows is recorded closed | `<ticket>@<at>` | the close in the ticket's history |
| `change.merged` | A change linked to a blueprint is recorded merged | the change's number | the merge on the code host |

A reader treats a name it does not know as unknown and skips it. A new name
or a new optional field is a minor change; a renamed or removed name or
field is a major one.

### The envelope

```json
{"api": 1, "event": "cycle.ended", "when": "after", "key": "12-3",
 "at": "2026-11-22T13:20:00Z",
 "data": {"cycle": "12-3", "blueprint": 12, "started": "2026-11-22T11:20:00Z",
          "ended": "2026-11-22T13:20:00Z", "end": "failed", "outcome": "failed",
          "status": 10, "parked": true, "change": 9}}
```

`api/event.schema.json` says what it holds, and checks `data` by `event` and
`when`. `api/event-page.schema.json` is the envelope a page takes: `when`
`after` alone. `console/tests/events_contract.rs` holds the samples to both.

| Field | Meaning |
| --- | --- |
| `api` | The events' contract version, 1 |
| `source` | The name of the factory the event is from, 1 to 64 lower-case letters, digits and `-`. tehas leaves it out; a program that relays many factories' events sets it |
| `event` | The name, a line of `api/events.txt` |
| `when` | `after` a fact. `before` is the side of a gate, an act that waits for an answer |
| `key` | The event's identity within its name, made from the fact, so a program that builds the event from a witness makes the same key. No white space, at most 200 characters |
| `at` | When the fact happened, never when it was raised |
| `data` | The payload, `api/event-<name>.schema.json`, the name's `.` written `-` |

Every time is a whole second in UTC, `2026-11-22T13:20:00Z`: no fraction and
no offset. A field with no value is left out, never null, and so is a
`false` its witness leaves out. A port type keeps the ports' own shape: a
ticket is `{"Number": 12}` or `{"Key": "OPS-12"}`, an act `"Approve"`, a
question's kind `"Park"` or `"Plan"`. `ticket` is left out when the ticket
is the blueprint. Within one cycle the order is `cycle.started`, then
`cycle.parked`, `question.asked`, `cycle.ended` and `change.offered`. Across
processes the only order is `at`.

### The payloads

Each name has `api/event-<name>.schema.json`, a sample with every field,
`.full.json`, and one with only the fields it always has, `.bare.json`.

| Field | Of | Meaning |
| --- | --- | --- |
| `cycle` | the `cycle.*` names, `change.offered`, `question.asked` | The cycle id, `<blueprint>-<attempt>`, unique within one store. A plan's question has none |
| `blueprint` | all but the ticket names | The blueprint, the factory API's `issue` |
| `ticket` | the cycle names, the ticket names, the gate | The ticket |
| `started` | `cycle.started`, `cycle.ended` | When the cycle started |
| `how`, `by` | `cycle.started`, the gate | What started it, and who; `by` is left out for a cycle run by hand |
| `ended`, `recorded` | `cycle.ended` | When it ended, and when a line written later than that was written |
| `end` | `cycle.ended`, `cycle.parked`, `change.offered` | The end word: `offered`, `merged`, `handoff`, `parked`, `failed`, `refused` or `stopped`. A parked cycle ends `handoff`, `parked`, `failed` or `refused` |
| `outcome`, `status`, `parked`, `signalled`, `change` | `cycle.ended` | As the history line prints them |
| `at` | `cycle.parked`, `change.offered`, the ticket names, `change.merged` | When the fact happened |
| `step`, `cap`, `parkReason`, `failureKind`, `failureCause`, `provider`, `repeat`, `change` | `cycle.parked` | As the history line prints them; `step` is its `progress.step` |
| `question`, `kind`, `askedAt` | `question.asked` | The store's number of the question, park or plan, and when it was asked |
| `choices` | `question.asked` | Each `choice` and `label`, in order, while the store keeps the question's options. A planning question's options are kept only while the issue waits on it, so one the issue no longer waits on has none |
| `set`, `observed` | `factory.limit-changed` | Exactly one: when the limit was written, or when a change with no `set` was observed (a clear, or a limit unreadable or gone) |
| `state`, `cycles`, `plan`, `until`, `reason` | `factory.limit-changed` | As `tehas limit --json` prints them |
| `act`, `by` | `ticket.acted` | The act and the account, `realm`, `login`, `id` and `kind` |
| `reason` | `ticket.closed` | `Done`, `NotPlanned`, or `{"Duplicate": <ticket>}` |
| `change` | `change.offered`, `change.merged` | The change's number |

### Gates

A gate asks before tehas acts, and waits. The first, the `before` side of
`cycle.started`, is raised from 2.1; its JSON ships now.
`api/event-cycle-started-before.schema.json` is its `data`: `blueprint`,
`ticket`, `how`, `by` and `approval`, the approval's time, which is the
envelope's `at`. Its key is `<blueprint>@<approval>`. Gates never reach a
page.

A gate's listener answers with `api/gate-answer.schema.json`:

```json
{"api": 1, "answer": "refuse", "why": "the disk is nearly full"}
```

`answer` is `allow` or `refuse`. `why` is at most 140 characters with no
control character; a refusal has it, an allow may.

### On a page

`host/fun/kit/events.js` exports `events`, an `EventTarget` for the page,
`names`, the lines of `api/events.txt`, and `raise(envelope)`. `raise`
dispatches a `CustomEvent` named `tehas:<event>`, such as
`tehas:cycle.ended`, whose `detail` is the envelope. It does not bubble. It
throws a `TypeError` on a name not in the list, an envelope field missing
or of another shape, or any `when` but `after`. It does not check each
name's `data` or the shape of its key: `api/event-page.schema.json` does. `tehas:` is only for the names of `api/events.txt`: a page's own
events use `<element>:<verb>`.

```js
import { events, raise } from "./kit/events.js";
events.addEventListener("tehas:cycle.ended", (e) => show(e.detail.data));
```

## Files under live/

The docs server publishes what a host keeps under `~/tehas/live/`: under
`/live/` at the paths below, and at `/health` and `/healthz` for
`health.json`. The names are one list, and the docs server holds it too: in
`host/edge/docs.Caddyfile` the `/live/` handler answers 404 for any name the
list does not hold, so a file a new writer puts under `live/` is not served
until both the table and the Caddyfile name it. Every file here is plain and
written through a side name and a rename, so a reader sees a whole file or the
last one. The console files mask tokens, keys and passwords as they are
written, and still: treat all of it as public. The two console names are
served only where the instance sets `TEHAS_SITE_CONSOLE=on`; without it they
answer 404 like a name the list does not hold ([The live console on
the site](/host#the-live-console-on-the-site)).

| Name | Written by | Read by |
| --- | --- | --- |
| `status.json` | `console/src/queue.rs`, run as `tehasbot status` ([tehasbot status and the console](/tools-health#tehasbot-status-and-the-console)) | the plain dashboard's islands (`host/fun/factory/`), `host/fun/game-view.js`; the store's `publish`; `console/ops/src/health.rs`, `console/analytics/src/sample.rs`, the MCP server (`factory_status`, `factory_issues`, `factory_issue`, `factory_sessions`) |
| `running.json` | the store's `publish` (`console/store/src/live.rs`) | the plain dashboard's islands (`host/fun/factory/`), `host/fun/game-view.js`, `host/fun/console-tabs.js`; `console/ops/src/health.rs`, `console/src/dev.rs`; the MCP server (`factory_status`, `factory_issue`, `factory_sessions`) |
| `endings.json` | the store's `publish` (`console/store/src/live.rs`) | `console/ops/src/health.rs`; the MCP server (`factory_endings`) |
| `epics.json` | `console/src/epics.rs`, run by `tehasbot status` | `host/fun/epics-view.js`; `console/ops/src/health.rs` |
| `health.json` | `console/ops/src/health.rs` run as `tehasbot health`, published by the store's `publish` ([tehasbot health](/tools-health#tehasbot-health)) | `/health` and `/healthz`, and `host/fun/versions.js`; `console/src/status.rs`, `console/ops/src/update/report.rs`; the MCP server (`factory_status`, `factory_issue`) |
| `health.down` | the store's `publish`, which makes it before a `fail` health report (and takes it back when that report cannot be written) and removes it after any other | the docs server's `/health` and `/healthz`, which answer 503 while it is there |
| `dispatch.json` | `console/dispatch/src/poll.rs` of `tehas dispatch`, published by the store's `publish` ([tehas dispatch](/tools-dispatch#tehas-dispatch)) | `console/ops/src/health.rs`, `console/src/queue.rs`, `console/analytics/src/sample.rs`; the MCP server (`factory_status`) |
| `demand.json` | `console/dispatch/src/poll.rs` of `tehas dispatch`, at every real poll | `console/ops/src/health.rs` |
| `analytics.json` | `console/src/publish.rs`, run by `tehasbot status` from the analytics program's `report --json` | `host/fun/factory/tehas-factory-days.*`, `host/fun/analytics-data.js`; the MCP server (`factory_stats`) |
| `disk.json` | `console/ops/src/disk.rs` with `--publish`, run hourly by `tehasbot clean` ([`tehas disk`](#tehas-disk)) | the MCP server (`factory_disk`) |
| `platform.json` | `console/src/platform.rs`, run by `tehasbot status` | `console/ops/src/health.rs`, which answers it as `checks.platform` |
| `rukas.json` | `console/ops/src/rukas.rs`, run by `tehasbot status` | `console/ops/src/health.rs`, which answers it as `checks.rukas` |
| `previews.json` | `console/src/preview/list.rs` of `tehasbot preview` ([tehasbot preview](/tools-clean#tehasbot-preview)) | `host/fun/factory/tehas-factory-previews.*`; `console/ops/src/health.rs`, `console/src/status.rs`, which answers it as `previews` |
| `cycles/<n>.json` | the store's `publish`, one file per running cycle and the last that finished | the pages that read `running.json` (the plain dashboard's console island, `host/fun/console-tabs.js`) draw the same view; no page asks for the file by its own path |
| `console-<n>.txt` | `console` (`console/src/tail.rs`), started by the cycle's watch (`console/cycle/src/watch.rs`) | `host/fun/factory/tehas-factory-tail.js` and `host/fun/game-view.js`, each by the name in `running.json`, and the console of the MCP server (`factory_console`) |
| `console-<n>.txt.now` | the cycle's watch (`console/cycle/src/watch.rs`), the two lines of the console's status line | `console/cycle/src/watch.rs` and the record of the cycle that ended |

Two of these the edge serves outside `/live/`: `/health` and `/healthz`
rewrite to `/health.json` and answer 503 with the same body while
`health.down` is there. Neither is read through `/live/` by a page.

## The host bridge

```
GET /host/bridge.json
```

```json
{"bridge": 1, "host": "example-host", "time": "2026-11-08T12:00:00Z", "ticketLink": "/host/ticket/{n}", "ticketNotes": [{"issue": 42, "text": "Example note", "link": "/host/ticket/42"}], "menu": [{"id": "m-1", "label": "Example page", "url": "https://host.example/page"}, {"id": "m-2", "label": "Another page", "url": "https://host.example/room", "open": "page"}]}
```

A contract between the dashboard and a host that fronts the factory's domain.
The dashboard asks its own origin for `/host/bridge.json`. tehas puts nothing
under `/host/`: the docs server answers every path there with 404, which the
dashboard reads as no host. A factory without a host looks and works as it
would without this section.

The front page and `/dashboard` ask once when a page loads, then every 60
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
| `ticketNotes` | Optional, at most 200. A short note on a ticket, wherever the dashboard draws that ticket |
| `ticketNotes.issue` | 1 to 999999999. At most 3 notes per ticket; the rest are dropped |
| `ticketNotes.text` | One line, 1 to 60 characters. Control characters become spaces |
| `ticketNotes.link` | Optional. A path under `/host/`, or an `https://` address, at most 512 characters |
| `menu` | Optional, at most 4. Items in the front page's menu, each opening its page in the dock or in a new tab |
| `menu.id` | `a-z`, `0-9` and `-`, 1 to 32 characters, unique in the list |
| `menu.label` | One line, 1 to 24 characters. Control characters become spaces |
| `menu.url` | An `http://` or `https://` address with no user or password, at most 512 characters, not on the dashboard's own origin |
| `menu.open` | Optional. `dock` or `page`. `dock`, or absent: pressing the item opens its page in the dock. `page`: the item is a plain link that opens its page in a new tab, with no frame. Any other value is read as `dock` |

- Not JSON, over 256 KiB, or a bad `bridge` or `time`: the answer is ignored
  whole.
- An element that breaks a rule is dropped alone; the rest is used. Past the
  limit of a list, the elements after it are dropped.
- Unknown fields are ignored, at the top level and in each element. The
  `tokens` and `notices` fields, which earlier releases drew, are among them.
  A host shows a message on the factory with
  [`tehas host-message`](#tehas-host-message).
- Text goes into the page as text only, never as markup.
- A link is used only if it passes its rule. A path under `/host/` opens in
  the same tab; an `https://` address in a new one.

How the dashboard draws it:

- `ticketLink`: every link to an issue of the factory's repository goes there
  instead, in the same tab. Pull requests keep linking to GitHub.
- `ticketNotes`: a small chip beside the ticket's number or title in every
  view that draws the ticket from the live queue, linked when it has a link.
- `menu`: an item each in the menu of the front page, before the plugins
  menu. Pressing one opens its page in the dock at the side. An item with
  `open: "page"` is a link that opens its page in a new tab instead, for a
  page that needs a whole tab or cannot be framed; a dashboard from before
  the field docks it. An item that goes from the answer goes from the menu.

The dock (`host/fun/dock.js`) makes the frame on the first press, never at
load, so it asks a page's host for nothing until a person does:

```html
<iframe title="<label>" src="<url>" referrerpolicy="no-referrer" allow=""
  sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox"></iframe>
```

No `allow-top-navigation`, no message channel between the page and the frame,
and no context but what is in the URL. A page that refuses to be framed shows
an empty frame; the dock's "Open in a new tab" link still works.

The answer is data and never runs code on the dashboard's pages. Pages under
`/host/` share the dashboard's origin: tehas keeps no secret on it, and a host
that fronts the domain is trusted as the factory's operator is. tehas
registers no service worker, and the front page and `/dashboard` unregister
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
    {"name": "TEHAS_SITE_INDEXING", "rule": "one-of", "when": "always", "default": "0", "words": ["on", "off", "1", "0"], "reason": "expected on or off"},
    {"name": "TEHAS_REPO", "rule": "repo", "when": "required", "default": null, "reason": "expected owner/name"}
  ]
}
```

Every release carries the rules of the instance settings and of the host
settings as `api/settings-rules.json`, at `tehas-<tag>/api/settings-rules.json` in the
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
| `settings.rule` | The shape of its value, a kebab-case name such as `repo`, `count` or `one-of`. Rows with the same rule take the same values, except `one-of`, whose words are the row's own, and the two timeouts below whose ceiling only the load checks |
| `settings.when` | When a load checks it: `always` (it has a default, so it always holds something), `given` (when the instance gives it a value), `named` (when the instance names it, even with an empty value), `required` (when a command requires it) |
| `settings.default` | What the load uses when the instance leaves it out or empty, or `null`: it stays unset, or the load works it out |
| `settings.words` | Only for a `one-of` row: the values it takes |
| `settings.reason` | What the check says for a value the rule refuses |
| `settings.building` | `true` on the switch and the own settings of a feature this release is still building, and left out on every other row. Such a row is not contract: it may change or go until the release that ships the feature, which drops the marker. The load refuses the switch set to on, and a line that gives one of the feature's own settings a value, with exit 78 |
| `host` | The settings of the host, in the same fields as `settings`: the environment or `~/.config/tehas/env` gives them, never `instance.env`. Each is checked when it holds a value (`when` is always `given`). `TEHAS_MODEL_KEY_<NAME>` stands for every name with that front. `TEHAS_UPDATE_LOCK_WAIT` is checked by `tehas update` and `install.sh` themselves, before the load, and its `default` is theirs |

An empty value means unset and is not checked against the rule, except for a
`named` setting. An unset setting of `when` `required` is refused by the
command that requires it.

Every other `TEHAS_*` name tehas reads is internal and may change in any
release: the tests' commands and hooks that the environment hands the load,
what the load derives, such as each size setting again in megabytes as
`<name>_MB`, the dispatcher's wait of an install, and what one program hands
the next. The list, with why for
each, is `console/core/src/settings/unruled.rs`, and a test of the workspace
holds every name the installed source reads to a rule or to that list.

The file holds a rule's name and one reason, not the check itself: to check a
value, run `libexec/console settings valid <name> <value>` from the tarball of
the same release. It exits 0 for a value the setting takes, and 1 with the
reason on standard output for one it refuses, the same reason the factory
gives, for any rule. It checks one value against its rule and nothing more.
The load also checks settings against each other and two ceilings that are
not in the rule: `TEHAS_WORK_TIMEOUT` at most 25200 and `TEHAS_PLAN_TIMEOUT`
at most 7200, both rule `count`. `settings valid` exits 0 for a larger count,
and the load refuses it with 78.
