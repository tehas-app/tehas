# Spec: several projects on one host, with priority

**Status**: Draft 2026-10-16, for the owner's review.

## Purpose

A tehas host serves one product repository today. A client with several
projects needs a host each, or has to wait while one project's queue takes
the host. The goal is one host that works on several projects of the same
client, and that puts the model server on the projects that matter most.

Each project keeps what it has today: its own GitHub repository with its own
issues, its own allowed labellers, its own control issue, its own models and
its own dashboard. What the projects share is the machine: the CPUs, the
slice, the sandbox image, the bot account and the model servers.

Who decides: the owner gave the goals, the priority behaviour, the dashboard
and login rules and the order of work. Everything marked *(builder's call)*
or *(my call)* below was settled while writing and can be changed.

## Who the tenants are

The projects belong to one client and are trusted. They share one Unix user,
one bot account and one GitHub token. The separation between projects is
about not mixing their state, their models and their dashboards. It is not a
security boundary between them. The security boundary stays where it is: the
sandbox container, which gets nothing from another project.

## Goals

1. Several projects on one host, each a full instance directory as today,
   each with its own GitHub repository and issues.
2. A host with one instance directory and no host file works exactly as
   today: the same settings, paths, unit names, container names and edge
   names. No migration, no new required setting. The one change it sees is
   the dashboard's privacy default (see [Dashboards](#dashboards)).
3. Priority between projects: free room on the host goes to the
   higher-priority project first, a lower-priority project still gets some
   work done, and one project can claim the whole host (`all-hands`).
4. Models chosen per project.
5. A dashboard per project at a name of its own, private by default, readable
   by anyone who can read the project's repository.

## Non-goals

- Untrusted tenants. No per-project Unix user, bot account or token for now.
  Nothing here should make those harder later, but none of it is built.
- Priority inside the model server. Nothing counts model slots or measures
  the model server's load: the owner decided on #215 that cycles start
  without that check and agents retry longer on a busy server. Priority acts
  only on what starts next.
- CPU or memory quotas per project. The slice and the host ceiling are
  shared, first come first served, in the order priority gives.
- Weighted shares of the host (for example A gets 3 of 4 cycles). Hosts run
  3 to 7 cycles of several hours each: the numbers are too small for ratios
  to come out right. On 2 slots a 3:1 split rounds to 1:1.
- Waiting for the SQLite and Rust work of epic #244. This is built in
  today's bash. #244 later takes the behaviour over as it is.

## Anti-goals

These would make the work a failure even if everything else works:

- A running cycle stopped, paused or slowed for priority. **Every cycle
  always finishes.** Priority and all-hands only decide who starts next.
- A single-project host that behaves differently, other than the dashboard
  default.
- One project's issues, logs, console, models or queue visible on another
  project's dashboard, or given to another project's container.
- A public dashboard that names the client's other projects.
- A setting that falls back to another project's repository, labellers or
  models when it is missing. Rule 2 of AGENTS.md holds per project.

## Concepts

**Single mode** is today's host: one instance directory, found as today
(`TEHAS_INSTANCE_DIR`, `~/.config/tehas/instance-dir`, `~/.config/tehas`).

**Multi mode** is a host with a host file, `~/.config/tehas/host.env`. Its
presence is the switch. A multi-mode host with one project is still multi
mode, with project names in every path and unit: there is no third state
that behaves like one or the other depending on how many projects there are.

**A project** is one instance directory plus a short name. The name is set
in the instance's `instance.env` as `TEHAS_PROJECT=<name>`: lowercase
letters, digits and single hyphens, at most 24 characters, unique on the
host. It is not taken from the repository name, because two repositories of
different owners can share a name (`a/api` and `b/api`) and the work
directory already collides on that today (`config.sh:252,256`).

## Settings

### The host file

`~/.config/tehas/host.env`, shell `KEY=value` lines like `instance.env`, no
secrets, writable by the tehas user only. It lists the projects and holds the
settings that are about the machine:

```
TEHAS_INSTANCES="/home/boss/.config/tehas/projects/project-a /home/boss/.config/tehas/projects/project-b"
TEHAS_TICKET_CONCURRENCY=6
TEHAS_CYCLE_RESERVE=2g
TEHAS_HOST_ADDR=192.0.2.5
TEHAS_DOMAIN=tehas.app
TEHAS_MAX_WAIT=86400
```

Host-wide in multi mode, so refused in a project's `instance.env` (exit 78
naming the setting and saying it belongs in `host.env`):
`TEHAS_TICKET_CONCURRENCY`, `TEHAS_CYCLE_RESERVE`, `TEHAS_HOST_ADDR`,
`TEHAS_DOMAIN`, `PI_ENSEMBLE_IMAGE`, `TEHAS_RUKAS_WORKAROUNDS` (tied to the
image), `TEHAS_TOP`, `TEHAS_PREVIEW_MAX`, `TEHAS_PREVIEW_MIN_FREE`,
`TEHAS_MAX_WAIT`. `TEHAS_ENERGY` and `TEHAS_ENERGY_URL` stay where they are,
in `~/.config/tehas/env`.

Per project, in each `instance.env`: everything else, including
`TEHAS_PROJECT`, `TEHAS_PRIORITY`, `TEHAS_DASHBOARD_PUBLIC`,
`TEHAS_DASHBOARD_HOST`, `TEHAS_MAX_AWAITING` (it counts that repository's
pull requests), `TEHAS_INDEXING` and `TEHAS_ANALYTICS_*` (they belong to a
dashboard, and dashboards are per project), `PI_ENSEMBLE_HOST_ALIASES`
(models are per project), the improvement loop's settings, the gate and
timeout settings, and `TEHAS_CONCURRENCY`.

In single mode none of this changes: every setting is legal in
`instance.env`, as today. `TEHAS_PROJECT`, `TEHAS_PRIORITY` and
`TEHAS_MAX_WAIT` are accepted and have no effect *(my call: a single-mode
instance can be written ready for a later move)*.

`TEHAS_CONCURRENCY` above 1 together with `TEHAS_TICKET_CONCURRENCY` above 1
stops every command today (`config.sh:389-392`). In multi mode the check
compares the project's `TEHAS_CONCURRENCY` with the host's ceiling.

### Loading

Rule 3 of AGENTS.md holds: loading has no side effects and `tehas_load`
never exits.

- In multi mode `tehas_load` reads `host.env` first, then the selected
  project's `instance.env`. The project is selected by `TEHAS_PROJECT` in the
  environment (added to the short list of names taken from the environment,
  `config.sh:214-222`) or by a `--project <name>` argument that commands turn
  into that variable.
- With no project selected, only the host file is loaded. A command that
  needs a project then stops with 78 and lists the project names. It does
  not pick one, even when there is only one *(rule 2)*.
- A project name not in `TEHAS_INSTANCES`, an instance whose
  `TEHAS_PROJECT` does not match its entry, two instances with the same
  name, or a host key in an `instance.env`, stops with 78.
- Login shells (`host/env.sh`) load the host file only in multi mode.

## Layout on the host

The idea that keeps the change small: **each project gets its own home for
what is per project**, and everything inside it keeps today's names.

`TEHAS_HOME` stays the install: `bin/`, `libexec/`, `lib/`, `platform/`,
`sandbox/`, `docs/`, `fun/`, `edge/`, `repos/pi-rukas`, and a host-level
`state/`, `logs/` and `live/`.

A new derived variable, `TEHAS_PROJECT_HOME`, holds what is per project:
`state/`, `live/`, `logs/`, `tmp/`, `work/`, `repos/<repo>`, `dev/`,
`protolab/`, `preview/`.

| Mode | `TEHAS_PROJECT_HOME` |
| --- | --- |
| Single | `$TEHAS_HOME`, so every path is what it is today |
| Multi | `$TEHAS_HOME/projects/<project>` |

Every command that writes `$TEHAS_HOME/state`, `live`, `logs`, `tmp` or
`work` today writes under `$TEHAS_PROJECT_HOME` instead. Issue-keyed files
(`state/cycles/<n>.*`, `state/park-<n>.id`, `state/plan-<n>.json`,
`logs/work-<n>-*`, `live/cycles/<n>.json`, `live/console-<n>.txt`, the
workspaces `work/<repo>-<n>`) then cannot collide between projects, because
they are in different directories. `tehas_cycles_running` and
`tehas_cycle_note_files` (`lib/cycles.sh:47-68`) keep globbing one
directory, which is now one project's.

Host-level files stay in `$TEHAS_HOME`: the host dispatch lock,
`update.lock`, `update-pending`, `edge-takeover`, `platform-fetch`, the host
dispatch log, `live/top.json`, `live/energy.json`, and the host's health.

### Names that are global on the host

These are not inside a directory, so in multi mode they carry the project.
In single mode they keep today's names.

| Name | Single mode (today) | Multi mode |
| --- | --- | --- |
| Cycle unit | `tehas-cycle@12.service` | `tehas-cycle@project-a:12.service` |
| Cycle container | `tehas-work-12` | `tehas-work-project-a-12` |
| Planning container | `tehas-plan-12` | `tehas-plan-project-a-12` |
| Manual session container | `tehas-dev-<name>` | `tehas-dev-project-a-<name>` |
| Index container | `tehas-index` | `tehas-index-project-a` |
| Improve container | `tehas-improve` | `tehas-improve-project-a` |
| Preview container and image | `tehas-preview-12`, `tehas-preview:12` | `tehas-preview-project-a-12`, `tehas-preview:project-a-12` |
| Agent transcripts | `~/.pi/agent/ensemble-runs/issue-12` | `~/.pi/agent/projects/project-a/ensemble-runs/issue-12` |
| Pi models and settings | `~/.pi/agent/models.json`, `settings.json` | `~/.pi/agent/projects/project-a/models.json`, `settings.json` |
| Edge names | `uat-12.<domain>`, `lab-7.<domain>`, `lab.<domain>` | `project-a-uat-12.<domain>`, `project-a-lab-7.<domain>`, `project-a-lab.<domain>` |

systemd allows `:` in an instance name; `tehasbot cycle` splits `%i` on it.
One helper in `lib/config.sh` (or a new `lib/names.sh`) makes every one of
these names, so no script builds one by hand. The edge name helper
`tehas_edge_name` is already the one place for edge names, but its logic is
copied into jq at `preview:695` and `protolab:111`; those copies go.

Docker labels on preview containers gain `tehas.project=<name>`.

## Models per project

- Each instance directory keeps its own `models.json` and optional
  `settings.json`, checked by `install.sh` as today (`install.sh:105-146`),
  per project, including the check of the product's own
  `.pi/settings.json` model against that project's `models.json`.
- In multi mode `install.sh` installs them to
  `~/.pi/agent/projects/<project>/`. `mcp.json` stays one generic file.
- Every reader takes the project's copy through one helper: the dispatcher's
  mesh and image checks (`dispatch:289,293,346`), `work:555`,
  `lib/describe.sh:68`, `improve:235`, the sandbox mounts
  (`lib/sandbox.sh:94,110,112,120`).
- A container is given its own project's `models.json` and `settings.json`
  and nothing of another project's.

**This changes the source of two mounts, which rule 4 of AGENTS.md calls a
security change.** It does not widen what a container gets: the same seven
mounts, the same environment names, nothing new. `test/work_test.sh`,
`plan_test.sh` and `dev_test.sh` keep their single-mode pins unchanged, and
gain multi-mode cases that pin the multi-mode paths exactly, including that
no path of another project appears. The pull request says this in its text.

## Priority

### The setting

`TEHAS_PRIORITY` in each project's `instance.env`: `high`, `normal` or
`low`; default `normal`. Anything else stops with 78. *(My call: three words
read better on a dashboard than a number, and more levels would not change
much with a handful of projects.)*

Above every level is **all-hands**, a temporary state switched on GitHub (see
below).

### What priority decides

Only which project's ready issue starts in a free slot. Running cycles are
never touched. Free room is counted as today, host-wide: the host ceiling
`TEHAS_TICKET_CONCURRENCY` minus the cycles running in all projects, then
the slice's room, then each project's own `TEHAS_MAX_AWAITING`.

The rule, applied one slot at a time:

1. **All-hands.** If any project is all-hands, only all-hands projects may
   start anything. Room they do not use stays empty.
2. **The waiting limit.** Otherwise, if a project with work waiting has
   waited longer than `TEHAS_MAX_WAIT` (host setting, seconds, default
   86400), it gets the slot. If several have, the one that has waited
   longest.
3. **Priority.** Otherwise the highest-priority project with an issue
   waiting gets the slot.
4. **Ties** between projects of equal priority go to the project whose last
   start is the oldest, so they take turns.

Within a project the order is today's: issues with an accepted
`tehas:urgent` first, then the lowest number (`dispatch:1603-1631`).

**How long a project has waited** *(my call)* counts only the time that
priority held it back. The limit exists so a lower-priority project still
gets some work done, so it counts what priority took away and nothing else:

- The clock runs while the project has an issue on offer and a host pass
  gave the slot it could have used to a higher-priority project.
- It does not run while the project is paused, is at its own
  `TEHAS_MAX_AWAITING` limit, has nothing on offer, or while any project is
  all-hands. So a project that was paused for three days is not overdue the
  moment it resumes, and nobody is overdue when all-hands ends.
- It goes back to zero when the project starts a cycle.

Each host pass adds the seconds since the previous pass to every project it
held for priority. The total is kept in the project's
`state/priority-held`, written by the host pass only. It is an input of the
pure decision like the offers, so the tests set it directly.

So once an overdue project gets a cycle, it is next overdue after another
full `TEHAS_MAX_WAIT` of being held.

What this gives, with A `high` and B `low`, both busy all week: B starts one
cycle a day. When A has nothing waiting, B uses all the room. With equal
priority, A and B alternate. On a single-project host the rule always picks
the one project, so nothing changes.

### All-hands

- The label `tehas:all-hands` on a project's control issue (the issue that
  already takes `tehas:pause`). `tehas labels` creates it.
- It counts only when an account in that project's
  `TEHAS_ALLOWED_LABELERS` applied it, read from the label events as for
  `tehas:pause` (`tehas_pause_holds`). Rule 5 of AGENTS.md holds. A label
  applied by anyone else is ignored and noted in the dispatch log, as for
  `tehas:urgent`.
- While a project is all-hands, no other project starts a cycle, a planning
  turn or an improvement-loop run. Their running cycles finish. Their
  previews, Protolab pages, index builds and the cleanup of finished cycles
  go on: they do not use the model server.
- The waiting limit does not apply while any project is all-hands.
- Two projects all-hands at once: they share the host by the normal rule
  between themselves, and every private dashboard warns that more than one
  project is all-hands.
- Removing the label ends it on the next poll.

### Planning and the improvement loop

*(My call, for the owner to check.)* A planning turn has a person waiting
for the answer and holds no slot today, so priority does not hold it back.
All-hands does. The improvement loop runs per project (below); a project's
run waits while another project is all-hands.

### Pausing

- `tehas:pause` on a project's control issue pauses that project, as today.
- `tehas pause` and `tehas resume` take `--project <name>`, or `--all` for
  every project. Without either, in multi mode, they stop with 78.

## The dispatcher

Today one pass (`libexec/dispatch`, 1694 lines) runs over one project's
globals, holds one lock and exits early in several places: after one
planning turn, one retry or one requeue (`dispatch:1420-1431, 1512-1600`).
Priority needs one decision across projects, so the pass splits in two.
The timer, the `dispatch.again` path unit and the one short pass that exits
stay. That is the model the tests and the manual are built on.

**Per project, the survey.** Everything that is about one project, in
today's order, under that project's lock (`$TEHAS_PROJECT_HOME/state/dispatch.lock`):
the local and GitHub pause, cleanup of quiet, stuck and finished cycles,
the sweep of workspaces, questions and planning, outage requeues. It ends
by writing, instead of starting cycles, the project's **offer** to
`$TEHAS_PROJECT_HOME/state/offer.json`: the ordered candidates (each already
through approval, the open-plan check, the open-pull-request check and the
image check), the time the oldest has waited, its `TEHAS_MAX_AWAITING` room,
whether it is paused, whether it is all-hands, and its priority. A retry
tick or a requeue that would start a cycle today becomes the head of the
offer instead of starting it.

**Planning turns leave the poll** *(my call)*. Today a turn runs inside the
poll and the poll waits for it (`dispatch:1516,1549`), up to
`TEHAS_PLAN_TIMEOUT`, which is why the dispatch unit is allowed 3 hours.
With several projects, one project's turn would hold back every other
project's starts, all-hands included, for up to two hours. So the survey
starts a turn as a unit of its own, `tehas-plan@<n>.service` in single mode
and `tehas-plan@<project>:<n>.service` in multi mode, the same way cycles
became units, and goes on. The rules of one turn per poll and one session
per poll stay. A project's survey does not start a turn while another
project is all-hands, which it reads from the previous host pass's record.
The dispatch unit's `TimeoutStartSec` can then come down, and
`TEHAS_PLAN_TIMEOUT`'s limit moves to the new unit. In single mode this is
the one internal change that is not a pure refactor; what a person sees on
the issue stays the same.

**Per host, the allocation.** Under the host lock: count the running
cycles of every project, read the slice's room, read every offer, decide by
the rule above, and start the chosen cycles through the project's own
`start_cycles` (`dispatch:749-799`), which keeps its order: refresh the
checkout, index before the first cycle, the approval record, the
`tehas:running` label, the unit. It writes the decision and each project's
hold reason to `$TEHAS_HOME/live/dispatch.json` and each project's own part
to `$TEHAS_PROJECT_HOME/live/dispatch.json`.

**The decision is a pure function**, `lib/priority.sh`: offers, room and the
clock in as JSON, starts and hold reasons out as JSON, no GitHub, Docker or
files. It gets its own test file with the cases in this spec. This is also
the shape #244 wants (a pure decide), so #244 can port it as it is.

In single mode the host pass runs with one offer, and the result is today's
behaviour. The early exits that end the whole poll today end only that
project's survey.

*(Builder's call)* whether the survey and the allocation stay one script
with two entry points or become two scripts.

The hold reasons are new lines on the dashboard:

- `held for a higher-priority project` (a public dashboard never names it)
- `held: project-a is all-hands` (private dashboards only; a public one says
  `held: another project is all-hands`)
- `overdue: starts in the next free slot`
- `waiting: starts by <time> at the latest`

## Services that run once per host

These are host timers today and stay host timers. In multi mode each run
goes through the projects, in priority order, one at a time:

- **Previews** (`tehasbot preview`): per project, with its own preview
  state and bare repository under its project home. The shared
  `tehas-previews` network and `TEHAS_PREVIEW_MAX` and `TEHAS_PREVIEW_MIN_FREE`
  are host-wide. **The sweep today removes any preview container whose repo
  label is not this `TEHAS_REPO` (`preview:966-979`).** In multi mode it
  removes only containers labelled with a project that no longer exists,
  and a project's run touches only its own containers.
- **Protolab**: per project, its pages under its project home.
- **Improvement loop**: per project. Each project files into its own
  `TEHAS_IMPROVE_REPO`, reads only its own cycles and names its own product.
  One host lock, so runs do not overlap. Skipped for a project while another
  is all-hands.
- **Index**: per project, under the project's lock instead of the host's.
- **Energy and top**: host-level, as they measure the machine. Energy today
  filters to one instance's models (`energy:63-101`); in multi mode it takes
  the union of the projects' models.
- **Health and status**: each project writes its own `live/status.json` and
  `live/health.json`. The host writes its own health from the projects' and
  its own checks.

## Dashboards

### One per project

- In multi mode each project's dashboard answers at `<project>.<domain>`,
  or at `TEHAS_DASHBOARD_HOST` when the project sets one (tehas-tehas would
  keep `tehas.app`).
- One docs server on port 8080, as today. It picks the project by the Host
  header and serves that project's `live/` at `/live/*` and its health at
  `/health`. A name that is not a project is a 404. The pages read fixed
  paths (`live/status.json` and so on, `dashboard.frag`, `host/fun/*.js`)
  and need no change, because the server maps `/live/` per project.
- The manual (`host/docs/`) is the same at every name and is public.
- The host panels (top, energy) show on private dashboards. On a public
  dashboard in multi mode they are off *(my call: the top list and the
  model names would show the client's other work)*.
- Single mode: one dashboard on 8080, as today.

### Private by default, login with GitHub

- A dashboard is private unless its project sets `TEHAS_DASHBOARD_PUBLIC=1`.
  Private means: before any page, live file or console of that project, the
  visitor signs in with GitHub.
- **A signed-in visitor is let in when their GitHub account can read that
  project's repository.** Nothing on the host lists viewers.
- The same applies to a private project's previews and Protolab pages: they
  are that client's code running.
- The manual stays public.
- Signing in grants viewing only. Starting work stays a `tehas:ready` label
  from an account in `TEHAS_ALLOWED_LABELERS`.
- A dashboard bound to a loopback address (`TEHAS_HOST_ADDR=127.0.0.1`, the
  default) is reachable from the host only and needs no login *(my call:
  login needs a domain and an OAuth app, and a host-only dashboard has
  neither)*.

**This changes single mode too.** Today's dashboards are public. After this
change a dashboard on a non-loopback address is private until its instance
says `TEHAS_DASHBOARD_PUBLIC=1`. tehas-tehas must have that line before the
update that brings this in, or tehas.app goes behind the login. Rule 8 of
AGENTS.md means the pull request proposes the line, the owner adds it.
`install.sh --check` and `tehas update --check` report a non-loopback,
non-public dashboard that has no login configured, and the docs server then
serves the manual only, never the dashboard *(fail closed)*.

### How the login works

- One sign-in service per host, in front of the docs server and the edge
  router, through Caddy's `forward_auth`. The dashboard pages do not change.
- One GitHub OAuth app per host, registered by the owner. Its client id and
  secret go in `~/.config/tehas/env`, never in an instance directory or the
  repository (rule 10).
- The OAuth app asks for no scope beyond the visitor's identity. The check
  "can this account read the repository" is made with the bot's token, which
  already has access: the repository's collaborator permission for that
  login. A private repository needs a permission of read or more. For a
  public repository any signed-in account passes; the manual says so.
  *(Builder's call to confirm the exact GitHub endpoint and how it answers
  for organisation members and for public repositories, and to choose the
  check that fails closed.)* Asking for the visitor's own token with the
  `repo` scope is not chosen: that scope gives write access to all their
  private repositories.
- The answer is kept for ten minutes. A person removed from the repository
  loses access within that time. If GitHub cannot be reached and no fresh
  answer is kept, the visitor is refused.
- **Cookies are per name, not per domain** *(my call)*. Previews are
  agent-written pages on the same domain. A cookie for the whole domain
  could be overwritten by a preview's script (cookie tossing), and the
  router stripping it would not stop that. So the sign-in service lives at
  `login.<domain>` and keeps its own host-only session there. Each dashboard,
  preview and Protolab name gets its own host-only cookie (`__Host-` prefix,
  HttpOnly, Secure, SameSite=Lax), set through a redirect to
  `login.<domain>` and back with a one-time code. A visitor signs in to
  GitHub once; every other name signs them in with a redirect and no
  prompt.
- **The edge router strips the session cookie before it proxies to a
  preview container**: a preview is code an agent wrote, and it must not
  receive a visitor's session.
- Signing out is a POST with a token, never a link, so a page on another
  name cannot sign a visitor out.
- What stays: a preview's script can still send a visitor's requests to
  other names on the domain, as `host/docs/edge.md` already says (one site
  to a browser). Accepted, because the projects are one client's and
  trusted, and no dashboard page takes an action on a GET.
- **Protolab files of a private project** *(my call)*. A ticket's files are
  served with `Content-Security-Policy: sandbox` and no `allow-same-origin`
  (`router.Caddyfile`), so the page has an opaque origin and the browser
  sends no SameSite=Lax cookie with its requests for its own files. The
  sandbox stays. Instead, after the login check on the ticket's page, the
  router serves the ticket under a path that carries a signed token,
  `/t/<token>/`, valid for that one project and ticket for a few hours.
  Relative links inside the prototype keep the token without knowing about
  it. A request with a valid token needs no cookie; one without a token or
  with an expired one gets the login.
- `/health` and `/healthz` of a private dashboard stay outside the login
  *(my call)*, so external monitors keep working, and answer only the status
  code and one word (`ok`, `warn`, `down`). The full health, with its
  reasons, is behind the login at `/live/health.json`. A public dashboard's
  `/health` is unchanged.
- *(Builder's call)* the service itself: a small Rust program beside
  `console/` (the direction #244 takes) or an off-the-shelf proxy such as
  oauth2-proxy if one can do the per-Host repository check. It runs as a
  container in the edge compose project or as a user unit, not in the slice.

### Edge names

Cloudflare's free certificate covers one level under the domain
(`host/docs/edge.md`, "Why the names are one level deep"). So in multi mode
previews and Protolab pages are flat names: `project-a-uat-12.<domain>`,
`project-a-lab-7.<domain>`, `project-a-lab.<domain>`. The router reads the
project and the number from the first label. The proxy in front of the host
already sends `*.<domain>` on; its matcher in `edge.md` step 4 widens to the
new names and to the dashboards' names. The manual's edge page gets the
multi-mode version of that step.

## Commands

- Commands about one project take `--project <name>` in multi mode:
  `tehas work`, `plan`, `dev`, `index`, `labels`, `pause`, `resume`, and
  `tehasbot cycle` (which gets it from its unit name).
- `tehas status` in multi mode prints one block for the host (ceiling,
  room, all-hands, the slice) and one line per project: priority, running,
  waiting, held and why. `--project` narrows it.
- `tehas dev --project <name>` clones that project's repository into its
  project home. `host/login/tehas`, the sudo entry for other accounts,
  passes the project through.
- `tehas update` and `install.sh` check every project's settings and the
  host file before anything else, and install every project's models.

## Adding and moving projects

- **Adding a project to a multi-mode host:** write its instance directory,
  add it to `TEHAS_INSTANCES`, run `install.sh` (which creates its project
  home, clones its repository, builds its first index) and `tehas labels
  --project <name>`. The manual page "A new project" gets this as a second
  path.
- **Removing one:** take it out of `TEHAS_INSTANCES` when it has no running
  cycle. Its project home stays until a person removes it. The preview sweep
  removes its preview containers.
- **Moving a single-mode host to multi mode:** `tehas host adopt <name>`
  with the factory paused and no cycle running. It writes `host.env` from
  the instance's host-wide keys, moves those keys out of the instance file
  (it prints the edit for an instance directory kept in git instead of
  making it), moves `state/`, `live/`, `logs/`, `tmp/`, `work/`, `repos/<repo>`,
  `dev/`, `protolab/`, `preview/` into `projects/<name>/`, and moves
  `~/.pi/agent/models.json`, `settings.json` and `ensemble-runs/` into
  `~/.pi/agent/projects/<name>/`. It stops with nothing moved if any check
  fails, and it is the last phase of the work.

## Tests

- `test/lib.sh` gains a helper for a multi-mode host with two projects
  (`t_host` and `t_project`, *builder's call* on names), beside
  `t_instance`, which stays for single mode.
- Every existing test runs unchanged in single mode. That is the check that
  goal 2 holds.
- `test/priority_test.sh` covers the pure decision: priority order, ties
  alternating, the waiting limit and its reset, the most overdue first,
  all-hands holding everyone else, two all-hands projects, a paused
  project, a project at its awaiting limit, zero room, a single project.
- `test/priority_test.sh` also covers the held clock: it runs only while
  priority holds the project, not while paused, at the awaiting limit or
  during all-hands, and it resets on a start.
- Multi-mode cases for: loading (each refusal above exits 78 before any
  call to GitHub, Docker or git), the names helper, the dispatcher's two
  phases with stubbed GitHub, planning turns started as units, the container mounts (rule 4), the preview
  sweep leaving another project's containers alone, the docs server's
  routing by Host, and the login's decision table (stubbed GitHub answers,
  the cache, GitHub unreachable, a stripped cookie at the router, the
  Protolab token, the minimal `/health`).
- Tests need bash, git, jq and shellcheck only, as rule 6 says. The login
  service's tests live beside it.

## Order of work

Each step is its own pull request, and the fast gate passes after each.
Single mode behaves as today after every step except 6, whose one change is
the dashboard default.

1. **Project home, no behaviour change.** `TEHAS_PROJECT_HOME` and the names
   helper, both equal to today's values. Every script moves to them. No test
   changes.
2. **The host file and project selection.** `host.env`, `TEHAS_INSTANCES`,
   `TEHAS_PROJECT`, `--project`, the refusals, models per project, unit and
   container names in multi mode, the sandbox mounts (the rule 4 note).
3. **The dispatcher in two phases, with priority and all-hands.** Planning
   turns as units first, in their own pull request. Then the pure
   decision and its tests, the offers, the host pass, `tehas:all-hands` in
   `tehas labels`, the hold reasons, `tehas status`.
4. **Host services per project.** Previews (with the sweep fix), Protolab,
   the improvement loop, index, energy, health.
5. **The sign-in service**, tested on its own, not yet in front of anything.
6. **Dashboards per project behind the login.** Routing by Host, the
   privacy default, `TEHAS_DASHBOARD_PUBLIC`, the edge names, the manual's
   edge page. The pull request names the line tehas-tehas needs first.
7. **`tehas host adopt`** and the manual: operating, the host, a new
   project, commands, troubleshooting, and features (marked as covered by
   tests only, until seen on a host).

## What is left to the builder

- Whether the dispatcher's two phases are one script or two.
- The sign-in service's language and packaging, and the exact GitHub
  endpoint for the read check.
- Test helper names.
- Log file names for the host pass.
- How `tehas status` lays out its lines.
