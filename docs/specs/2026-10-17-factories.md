# Spec: the factory API, one factory per project

**Status**: Draft 2026-10-17, for the owner's review. Replaces the draft
`docs/specs/2026-10-16-multi-project.md` (branch `multi-project-design`,
never merged).

## Purpose

A client with several projects wants them worked on with the same model
servers, and wants the model time to go to the projects that matter most.

The draft of 2026-10-16 did this inside tehas: one install served several
projects, with a host file, project selection in every command and a
dispatcher split in two. The owner chose a different shape on 2026-10-17:

- **tehas stays a single-project appliance.** One install, one instance
  directory, one product repository, as today. It runs on one machine with
  `install.sh` and nothing else.
- **Several projects are several factories**, each a full tehas install on a
  machine of its own.
- **A separate project, tehas-industry (industry for short),** owns the
  factories: it creates them, decides priority between them, puts their
  dashboards behind a login, keeps their history and scales them. It lives
  in its own private repository, `janit/tehas-industry`, with its design in
  `docs/specs/2026-10-17-industry.md`. None of its code lives here.

This spec is the tehas side: **the factory API**, a small set of commands
with JSON output that make a factory as easy to drive from a program as
from a terminal. Industry is its first program user. A person on a
standalone host uses the very same commands.

## Goals

1. **One API for both.** Everything industry does to a factory, a person can
   do with the same command and the same flags. No command, flag or
   behaviour exists only for industry.
2. **Stable and versioned.** Each command's JSON says its API version. A
   program can ask a factory which version and which features it has before
   it relies on them.
3. **A factory can be limited from outside**: how many cycles it may run and
   whether it may start planning turns and improvement runs, with an expiry.
4. **A factory says what it wants and what it did**: work waiting and for
   how long, pause and all-hands, running cycles, finished cycles and their
   history, which can be exported.
5. **A factory with no limit set behaves exactly as today.** tehas-tehas and
   any standalone install see no change unless a person uses the new
   commands.

## Non-goals

- Several projects inside one install. No host file, no `--project`, no
  project home.
- A network API. tehas runs no server that takes commands, so there is no
  authentication to build or get wrong. A caller runs the command as the
  factory's user: in a terminal, over SSH, with `incus exec` or `kubectl
  exec`. Getting there is the caller's business.
- Sharing a Docker daemon, ports or a Unix user between factories. Each
  factory is a machine of its own.
- Priority, the waiting limit, the login, scaling, backups. All in industry.
- Measuring the model server's load (#215). A limit is a number of cycles.

## Principles of the API

1. **Commands are the API; files are not.** What is under `$TEHAS_HOME`
   stays internal and may change in any release. A program calls a command
   and reads its standard output. The JSON the docs server publishes under
   `/live/` is for the dashboard, not the API.
2. **`--json` prints one JSON object** on standard output and nothing else.
   Messages go to standard error. Without `--json` the same command prints
   for a person, as today.
3. **Every object carries `"api": 1`.** Within version 1 fields are only
   added, never renamed, removed or given a new meaning. A breaking change
   is version 2, and a release that brings it says so.
4. **Exit statuses are the existing ones**: 0 done, 1 the command ran and
   the answer is "no" or "not yet" (as `--check` uses it), 64 a bad
   argument, 78 a missing or malformed setting (rule 2). Each command's
   header lists its own.
5. **Setting is safe to repeat.** Running a set command twice with the same
   arguments leaves the same state and exits 0 both times.
6. **Reading makes no network call** and takes no lock a cycle holds, so a
   program can call it every few seconds. It reports what the last poll saw,
   with that poll's time.
7. **Names say what happens in the factory, not who asks.** `tehas limit`,
   not "allowance from industry".
8. **The tests pin the shapes.** Each `--json` command has a test that checks
   its fields and types, so a change that breaks a caller fails the gate.

## The commands

### `tehas api [--json]` (new)

What this factory speaks. A program calls it first, the way `tehas work`
asks the sandbox image for its capabilities before a cycle.

```json
{
  "api": 1,
  "tehas": "v0.8.0",
  "commit": "cb6912a",
  "repo": "janit/tehas",
  "features": ["status", "limit", "drain", "history", "export", "all-hands", "update-to", "fixer-grant"],
  "fixer": "tehas-fixer"
}
```

`features` names what this release has, so a program can tell a factory
that predates a feature from one where it failed. `fixer` is
`TEHAS_FIXER_LOGIN`, or null when the instance has no fixer. It reads the instance's
`TEHAS_REPO` without calling GitHub, and stops with 78 if the settings are
missing.

### `tehas status --json` (new flag)

Today's `tehas status` reads `live/health.json` and prints lines. `--json`
prints the same facts as one object, with the demand added:

```json
{
  "api": 1,
  "time": "2026-10-17T13:59:40Z",
  "age": 40,
  "stale": false,
  "state": "pass",
  "reasons": [],
  "paused": {"local": false, "label": false},
  "allHands": false,
  "limit": {"state": "ok", "cycles": 1, "plan": true, "until": "2026-10-17T14:05:00Z"},
  "running": [{"issue": 12, "since": "2026-10-17T11:20:00Z", "step": "review"}],
  "demand": {"waiting": 2, "oldestWait": 5400},
  "hold": {"why": "limit", "text": "held by the limit: 1 cycle"},
  "room": {"cycles": 3, "maxAwaiting": 4, "slice": 2}
}
```

- `stale` is true when the last poll is older than ten minutes, as today;
  the exit status is then 1, as today.
- `limit.state` is `none`, `ok`, `expired` or `unreadable`.
- `demand.waiting` counts issues that would start now if there were room:
  accepted and through every check a start makes today. `oldestWait` is
  seconds since the first of them was accepted.
- `hold` is null when nothing holds. `hold.why` is an open list of words
  (`pause`, `slice`, `unreadable`, `resources`, `count`, `awaiting`,
  `pressure`, `room`, `full`, and `limit` with step 3); `hold.text` is for a
  person and not parsed.
- A field without a value is left out, never null: `limit` with no limit is
  `{"state": "none"}`, an unreadable one `{"state": "unreadable"}`, an
  expired one keeps what was set; `running[].step` and `demand.oldestWait`
  are left out when unknown or when nothing waits. `demand` is null when no
  poll counted it in ten minutes, `room` when no poll got to the room check.
- `room` is the room left under each of the factory's own caps: `cycles`
  under `TEHAS_TICKET_CONCURRENCY` alone, `maxAwaiting` under
  `TEHAS_MAX_AWAITING`, `slice` in `tehas.slice`. A figure the poll did not
  read is left out. The limit never enters these: a program that sets the
  limit reads them as what the factory could start without it. Step 3 adds
  the room under the limit as a field of its own, `room.limit`.
- *(Built in #299.)* `demand.waiting` counts `tehas:ready` applied last by an
  allowed account or the fixer, with no cycle or planning session open. The
  checks a start makes on top (text unchanged, no open pull request, the
  fixer's grant) are not repeated for the count: they cost a call each per
  poll.

For this the dispatcher writes the demand, pause, all-hands and limit
state where `tehas status` reads them on **every** poll that gets past the
settings, a paused poll included. Today some of it is written only by a
poll that gets as far as the room check.

### `tehas limit` (new)

An outside cap on what the factory starts. A person uses it to keep a host
quiet for a weekend; industry uses it for priority and drains.

```
tehas limit [--json]                         show the limit, or "none"
tehas limit set --cycles N [--plan on|off] [--for SECONDS | --until TIME] [--reason TEXT]
tehas limit clear
```

`tehas limit --json` prints status's `limit` object with `"api": 1` and
`reason` added.

- `--cycles N`: the most cycles running at once. Room for new cycles is
  `min(TEHAS_TICKET_CONCURRENCY, N) - running`, then the slice's room and
  `TEHAS_MAX_AWAITING` as today.
- `--plan off`: no new planning turn and no improvement run. Answers to a
  person waiting on a planning question are not held *(my call, for the
  owner to check)*: they are short and someone is waiting.
- `--for` / `--until`: when the limit expires. Without either, it lasts until
  cleared.
- `--reason`: shown as the hold reason. At most 200 characters, one line;
  control characters become spaces, as in `tehas pause`.
- **An expired or unreadable limit holds everything**: no cycle, planning
  turn or improvement run starts, and the hold reason says why. A limit that
  was set and cannot be read does not fall back to no limit (rule 2). This
  is why a program that refreshes a limit sets an expiry: if the program
  dies, the factory stops starting work instead of running unlimited. A
  person sets a limit without an expiry and it simply stays. It is not exit
  78: the poll still does its cleanup, questions and requeues.
- **Running cycles are never stopped.** A lower limit only holds new starts.
- The dispatcher reads it on every poll and `tehasbot improve` before each
  run. Stored in `state/` (internal), written atomically.

**Limit and pause stay separate.** `tehas pause` is a person's switch and
`tehas:pause` the label; industry never uses them. A paused factory stays
paused whatever its limit says, so a person's pause cannot be lifted by a
program refreshing a limit.

### `tehas drain [--wait SECONDS] [--json]` (new)

`tehas limit set --cycles 0 --plan off --reason "draining"`, then, with
`--wait`, wait up to that long for the running cycles to finish. Exit 0
when none is running, 1 when some still are at the end of the wait. For a
person before a reboot or a disk change; for industry before an upgrade or
a move. `tehas limit clear` ends it.

### `tehas history --json [--after CURSOR] [--issue N]` (new)

The finished cycles, oldest first, from the results the factory keeps:

```json
{
  "api": 1,
  "cycles": [
    {"cursor": "1760700000-12", "kind": "work", "issue": 12, "started": "...",
     "ended": "...", "status": 10, "outcome": "handoff", "pr": "https://github.com/...",
     "step": "review", "cap": "...", "parkReason": "...", "failureKind": "...",
     "failureCause": "...", "provider": false, "parked": true,
     "by": "alice", "how": "label"}
  ],
  "next": "1760700000-12"
}
```

`cursor` orders cycles by when they ended; `--after` returns only later
ones. A program keeps `next` and asks again. `--issue N` lists one issue's
cycles. Planning turns are listed the same way with `"kind": "plan"`.

The fields after `pr` are what a fixer needs to tell a park that a retry
can pass from one it cannot: the last step, the cap and park reason, the
failure, whether a provider outage stopped it, and who started it and how
(`label`, `retry-tick`, `outage-retry` or `fixer`). A field the cycle did
not have is left out.

### `tehas export --after CURSOR <dir>` (new)

Copies everything about the cycles `history` would list into `<dir>`, one
directory per cycle: its result, its logs, its agent transcripts and its
Protolab pages. Prints the `history` object of what it copied. Changes
nothing in the factory. Industry calls it to fill its archive; a person
calls it to keep a copy before a machine is replaced. It copies what a
person reading a cycle needs and never a token: files are masked as the
console masks them.

### `tehas update --to <tag>` (new flag)

Today `tehas update` fast-forwards the platform checkout to the head of its
branch. `--to <tag>` moves it to a release tag instead, then installs as
today. `--check --to <tag>` reports what that would change. This lets a
person pin a host to a release and lets industry upgrade one factory before
the others. *(Builder's call: how the checkout sits at a tag, since `update`
today refuses a checkout that is not on a branch.)*

### The fixer's grant (#294, PR #297)

Not a command, but part of what a program relies on, so it is versioned
here too. A program acting as `TEHAS_FIXER_LOGIN` may apply `tehas:ready`,
and the dispatcher starts a cycle only within the grant described in
`host/docs/operating.md`, "The fixer's grant". The program's side of it:

- the marked comment `<!-- tehas:fixer issue=<n> action=retry|change -->`,
  written in the ten minutes before the label and not edited after, reaches
  the cycle as `retry_context`;
- a refused label comes off with a comment that says why;
- `tehas:fixer-escalated` is the program's label; a start takes it off.

`api --json` lists `fixer-grant` when the release has it. A change to the
marker, the label or the grant's conditions is an API change.

### `tehas pause`, `tehas resume`, `tehas labels` (unchanged)

`tehas labels` also creates `tehas:all-hands`.

## All-hands

- `tehas:all-hands` on the control issue holds when the latest event that
  applied it is by an account in `TEHAS_ALLOWED_LABELERS` and it has not
  been removed since, read from the label events as `tehas_pause_holds`
  does (rule 5). A label applied by anyone else is ignored and logged once.
- **On its own it changes nothing in the factory.** It is reported in
  `tehas status`, its JSON and the dashboard. What it means for other
  projects is industry's rule.

## Installing a factory

Part of the API, because a program installs factories too:

- `bootstrap.sh` then `install.sh <instance>` run unattended on a fresh
  machine, and running them again is safe.
- `install.sh --check` exits 0 on a machine that is installed as the
  instance says.
- What a factory machine needs (systemd, linger, its own Docker daemon,
  `~/.config/tehas/env`, its own `TEHAS_DOMAIN`, and pointing `baseUrl` and
  `$TEHAS_MODEL_KEY_<NAME>` at industry's model gateway when it uses one)
  goes in the manual's host page, so a person and industry read the same
  list. Behind the gateway the energy panel needs `TEHAS_ENERGY_URL` unless
  the gateway passes `/v1/cluster` through.
- Whether `install.sh` really runs unattended in a fresh system container is
  industry's first acceptance test. What it finds broken is fixed here.

## The manual

A new page, `host/docs/api.md`: the principles above, each command with its
flags, JSON and exit statuses, and how a program uses them together (ask
`api`, poll `status`, set `limit` with an expiry, `drain` before an
upgrade, `history` and `export` for the archive). `operating.md` gets the
person's side: `limit`, `drain`, `export`, `update --to`. `features.md`
marks each as covered by tests until seen on a host.

## Tests

- `test/api_test.sh`: `tehas api --json` fields, `fixer` set and null; 78
  without settings.
- `test/status_test.sh`: `--json` fields and types on a fresh, a stale, a
  paused, a held and a busy factory; the exit statuses match the text mode.
- `test/limit_test.sh`: set, show, clear, repeat; bad arguments are 64; the
  reason is cleaned.
- `test/dispatch_test.sh`: no limit gives today's starts in every existing
  case; `cycles` 0, 1 and above `TEHAS_TICKET_CONCURRENCY` with 0, 1 and 2
  running; `plan off` holds a new turn and not an answer; expired,
  unreadable, wrong version: nothing starts, the hold says why, cleanup
  still runs; a paused factory stays paused under any limit; the demand and
  all-hands fields on every kind of poll.
- `test/improve_test.sh`: a run waits on `plan off` and on an expired limit.
- `test/drain_test.sh`: exit 0 with none running, 1 when the wait ends with
  cycles running.
- `test/history_test.sh`, `test/export_test.sh`: order, cursors, nothing
  repeated or skipped, `--issue`, the park fields, nothing written in the
  factory, no token in an exported file.
- `test/update_test.sh`: `--to` a tag, `--check --to`, a tag that does not
  exist.
- `test/labels_test.sh`: the all-hands label is created.

## Order of work

Each step is one pull request, the fast gate passes after each, and a
factory nobody limits behaves as today after every one.

1. **`tehas api` and `tehas status --json`**, with the demand written on
   every poll, and `host/docs/api.md` started.
2. **`tehas:all-hands`**: the label, the rule 5 check, the report.
3. **`tehas limit` and `tehas drain`**: the command, the dispatcher and
   improvement loop honouring it, the hold reasons.
4. **`tehas history` and `tehas export`.**
5. **`tehas update --to`.**
6. **The host page** on what a factory machine needs, and whatever
   industry's first install test finds.

Industry can start on its side after step 3: priority needs only `api`,
`status` and `limit`.

## What happens to the earlier work

- The spec branch `multi-project-design` is kept for its decisions, which
  the industry spec carries over: priority levels, the waiting limit,
  all-hands, dashboards per project with a GitHub login, tehas-tehas public.
- Branch `multi-project-1` (the project home and `tehas_name`) is not needed
  and is parked, not merged. It changes no behaviour; it can be revived as a
  cleanup if wanted.

## What is left to the builder

- The exact text of hold reasons and status lines.
- How `oldestWait` is measured; the accepted `tehas:ready` event is the
  natural start.
- The cursor's format, as long as it orders and never repeats.
- Where `tehas history` reads from. The fixer's grant (PR #297) adds one
  line per cycle end to `state/cycles/<issue>.history.jsonl`, the last 50
  per issue, with fields only added; when that is merged first, history
  builds on it rather than adding a second record.
- Test helper names.
