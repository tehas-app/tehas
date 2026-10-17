# Spec: one factory per project, limited from outside

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
  directory, one product repository, as today. It still runs on one machine
  with `install.sh` and nothing else.
- **Several projects are several factories**, each a full tehas install on a
  machine of its own.
- **A separate project, tehas-industry (industry for short),** owns the
  factories: it creates them, decides priority between them, puts their
  dashboards behind a login and scales them, on one machine or on
  Kubernetes. It lives in its own private repository, `janit/tehas-industry`,
  with its design in `docs/specs/2026-10-17-industry.md`. None of its code
  lives here.

This spec is the tehas side: the small, generic contract that lets anything
outside a factory limit what it starts and see what it wants.

## Goals

1. A factory can be told from outside how many cycles it may have running,
   and whether it may start planning turns and improvement runs.
2. A factory says, in a file, what it wants: issues waiting, how long the
   oldest has waited, whether it is paused or all-hands.
3. `tehas:all-hands` is a label a factory reads and reports, with the same
   check of who applied it as `tehas:pause`.
4. A factory with no outside limit behaves exactly as today. tehas-tehas and
   any install on one machine see no change.

## Non-goals

- Anything about several projects inside one install. No host file, no
  `--project`, no project home. One install still means one project.
- Sharing a Docker daemon, a set of ports or a Unix user between factories.
  Each factory is a machine of its own (a VM, a system container, a pod that
  behaves as a machine). That is industry's job, and it is why the
  name prefix and the port settings of earlier discussion are not needed.
- Priority, the waiting limit, the login and scaling. All in the
  industry.
- Measuring the model server's load. The owner decided on #215 that cycles
  start without that check; industry's limit is a number of cycles,
  as `TEHAS_TICKET_CONCURRENCY` is.

## The contract

Two files in `$TEHAS_HOME`. The factory reads one and writes the other. How
they travel between the machine and industry (a shared volume, a
copy over SSH, the dashboard's HTTP) is industry's business. tehas
makes no network call for either.

### The allowance: `state/allowance.json`, read by the factory

Written by something outside the factory, atomically (write a new file and
rename it). The factory never writes it.

```json
{
  "version": 1,
  "cycles": 1,
  "plan": true,
  "until": "2026-10-17T14:05:00Z",
  "reason": "held for priority: tehas-tehas is high"
}
```

| Field | Meaning |
| --- | --- |
| `version` | 1. Any other value is a malformed file. |
| `cycles` | The most cycles this factory may have running. A whole number, 0 or more. |
| `plan` | `false` holds new planning turns and improvement-loop runs. |
| `until` | UTC time after which the file is stale. |
| `reason` | Optional. Shown as the hold reason when the allowance is what holds a start. At most 200 characters, printable text only; anything else is dropped and a fixed text is shown. |

What the factory does with it:

- **No file**: today's behaviour. Nothing else changes.
- **A valid, fresh file**: room for new cycles is
  `min(TEHAS_TICKET_CONCURRENCY, cycles) - running`, then the slice's room
  and `TEHAS_MAX_AWAITING` as today. A planning turn or an improvement run
  starts only when `plan` is true.
- **A stale or malformed file**: the factory starts no cycle, planning turn
  or improvement run, and its hold reason says why (`allowance expired at
  ...` or `allowance unreadable`). This is rule 2 of AGENTS.md: a limit that
  was set and cannot be read does not fall back to no limit. It is not exit
  78: the poll still does its cleanup, questions and requeues, as it does
  when it holds for any other reason.
- **Running cycles are never stopped.** A lower `cycles` only holds new
  starts. Draining a factory is `cycles: 0` and waiting for `running` to
  reach 0.
- Answers to a person waiting on a planning question are not held by
  `plan: false` *(my call, for the owner to check)*. They are short and a
  person is waiting. Only new turns are.

The dispatcher reads the file on every poll and `tehasbot improve` before
each run; nothing is cached. A missing file is
checked with a plain test, so loading settings stays free of side effects
(rule 3).

### The demand: `live/dispatch.json`, written by the factory

It exists today (`libexec/dispatch`, `report_hold`) with the hold reason,
the running count and the slice's numbers, and the docs server already
serves it at `/live/dispatch.json`. It gains:

| Field | Meaning |
| --- | --- |
| `waiting` | Issues that would start now if there were room: accepted, and through every check a start makes today. |
| `oldestWait` | Seconds the first of those has waited since its label was accepted. 0 when none. |
| `paused` | The local or GitHub pause holds. |
| `allHands` | `tehas:all-hands` holds (below). |
| `allowance` | The allowance as read this poll: `none`, `ok`, `expired` or `unreadable`. |

Today the file is written only by a poll that gets as far as the room
check. It is to be written by every poll that gets past the settings, a
paused one included, so `paused` and `allHands` are always current.
`time` already says when it was written; a reader treats an old file as a
factory that is down.

### All-hands

- `tehas labels` creates `tehas:all-hands` beside `tehas:pause`.
- It holds when the latest event that applied it is by an account in
  `TEHAS_ALLOWED_LABELERS` and it has not been removed since, read from the
  label events of the control issue as `tehas_pause_holds` does (rule 5).
  A label applied by anyone else is ignored and logged once.
- **On its own it changes nothing in the factory.** The factory reports it.
  What it means for other projects is industry's rule. A factory
  with no industry shows it on its dashboard and in `tehas status` and
  otherwise ignores it.

## Dashboard and status

- `tehas status` and the dashboard show the allowance line when a file
  exists: `allowance  1 cycle, planning on, until 14:05` or the reason it
  holds.
- The hold reason from `reason` goes where the other hold reasons go.
- Nothing about login. A factory's dashboard behind industry's
  sign-in proxy is bound to an address only the proxy reaches, with today's
  `TEHAS_HOST_ADDR`.

## What a factory machine needs

Industry runs factories as machines that look like today's host.
The product promises nothing new here, but the manual's host page says what
a factory needs, so industry has one place to read it:

- Linux with systemd, `loginctl enable-linger` for the factory's user, and a
  Docker daemon of its own.
- `bootstrap.sh` then `install.sh <instance>` run unattended, and running
  them again on the same machine is safe.
- `~/.config/tehas/env` with the tokens; nothing else from outside but the
  instance directory.
- Its own `TEHAS_DOMAIN` when it publishes previews and the Protolab.
- Behind industry's model gateway, a provider's `baseUrl` is the gateway and
  its `apiKey` the factory's own gateway key, through `$TEHAS_MODEL_KEY_<NAME>`
  as today. No tehas change; the energy panel then needs `TEHAS_ENERGY_URL`
  unless the gateway passes `/v1/cluster` through.

Whether `install.sh` really runs unattended in a fresh system container is
industry's first acceptance test. What it finds broken is fixed here,
one pull request each.

## Tests

In `test/dispatch_test.sh`, with GitHub and Docker stubbed as today:

- No allowance file: the same starts as before, for each existing case.
- `cycles` 0, 1 and above `TEHAS_TICKET_CONCURRENCY`, with 0, 1 and 2
  running.
- `plan: false` holds a new planning turn and an improvement run, and not an
  answer to a waiting question.
- Stale, malformed, wrong version, negative `cycles`, a `reason` with
  control characters: nothing starts, the hold reason says why, cleanup
  still runs.
- `live/dispatch.json` has the new fields on a paused poll, a held poll and
  a poll that starts something, and `waiting` and `oldestWait` match the
  stubbed issues.
- `tehas:all-hands` by an allowed labeller holds; by anyone else it does
  not and is logged once; removed, it no longer holds.

In `test/improve_test.sh`: a run waits on `plan: false` and on a stale
file. In `test/labels_test.sh`: the label is created. In
`test/status_test.sh`: the allowance line.

## Order of work

Each step is one pull request, the fast gate passes after each, and an
install with no allowance file behaves as today after every one.

1. **The demand fields** in `live/dispatch.json`, written on every poll.
2. **`tehas:all-hands`**: the label, the rule 5 check, the report, status.
3. **The allowance**: the read, the room, `plan`, the hold reasons, status,
   the manual (`host/docs/` page on running under industry, and
   `features.md` marked covered by tests only).
4. **The factory machine page** in the manual, and whatever the
   industry's first install test finds.

## What happens to the earlier work

- The spec branch `multi-project-design` is kept for its decisions, which
  industry spec carries over: priority levels, the waiting limit,
  all-hands, dashboards per project with a GitHub login, tehas-tehas public.
- Branch `multi-project-1` (the project home and `tehas_name`) is not needed
  and is parked, not merged *(my call)*. It changes no behaviour, so nothing
  waits on it; it can be revived as a cleanup if wanted.

## What is left to the builder

- The exact text of the hold reasons and the status line.
- How `oldestWait` is measured. The time of the accepted `tehas:ready`
  event is the natural start; the label events give it.
- Test helper names.
