---
title: Commands
description: "Every tehas command and what it does."
---

# Commands

On the host as the tehas user (in the example, `boss` on `tehas-tehas`).
Interactive shells get the environment from `~/tehas/env.sh`; a non-interactive
`ssh boss@tehas-tehas '<command>'` must source it first.

There are two commands on `PATH`, both in `~/tehas/bin`:

```
tehas dispatch [--dry-run]       # one poll of the issue queue, by hand
tehas work <issue> [--restart]   # one cycle for an issue, by hand
tehas plan <issue> ...           # one turn of a planning session, by hand
tehas dev [<name>]               # an interactive session on the host, by hand
tehas index                      # build the code index the agents search
tehas update [--check]           # bring this host up to date with the platform
tehas labels                     # create the tehas labels in the repository
tehas status                     # print the state of the factory
tehas top [--once|--json]        # one screen of what the factory is doing
tehas pause [reason]             # stop this host from starting new work
tehas resume                     # lift this host's pause flag
tehas help [command]             # this list, or the text of one command
```

`tehas` is the one a person types. [`tehasbot`](#tehasbot) is what the timers
and services run. Each of their commands is a script in `~/tehas/libexec`,
installed from `libexec/` in the platform repository; the two commands only
choose a script and run it, and exit with its status. A command neither of
them knows is exit 64 with the list on standard error. `tehas <command> --help`
prints the same text as `tehas help <command>`.

Every script that needs the instance settings loads them first and stops with
exit 78 and one line if a setting it needs is missing or malformed. See
[The host](/host).

## Instance settings

Set in `instance.env` in the instance directory. `instances/example/instance.env`
in the platform repository has every one with a comment.

| Setting                         | Default                                  | Meaning                                                         |
| ------------------------------- | ---------------------------------------- | --------------------------------------------------------------- |
| `TEHAS_REPO`                    | required                                 | `owner/name` of the repository whose issues are the queue       |
| `TEHAS_CONTROL_ISSUE`           | required                                 | Issue that carries `tehas:pause`                                |
| `TEHAS_ALLOWED_LABELERS`        | required                                 | Logins whose labels count, space separated: `tehas:ready`, `tehas:plan` and `tehas:urgent` for the dispatcher, `tehas:prototype` for the Protolab |
| `PI_ENSEMBLE_SUBAGENT_MODEL`    | required                                 | Model for every subagent role without one of its own            |
| `PI_ENSEMBLE_SUBAGENT_PROVIDER` | required                                 | Its provider, as named in `models.json`                         |
| `PI_ENSEMBLE_MODEL_<ROLE>`      | `PI_ENSEMBLE_SUBAGENT_MODEL`             | Model for one role: `EXPLORE`, `DEVELOPER`, `ADVERSARIAL_DEVELOPER`, `CODE_REVIEW_SPECIALIST` (the six lens reviewers) or `OPS`. Another name is refused. See [A model per role](/host#a-model-per-role) |
| `PI_ENSEMBLE_PROVIDER_<ROLE>`   | `PI_ENSEMBLE_SUBAGENT_PROVIDER`          | Its provider. Refused without the role's model                  |
| `TEHAS_HOST_ADDR`               | `127.0.0.1`                              | Address the docs server and the edge router bind to             |
| `TEHAS_INDEXING`                | `0`                                      | `1` lets search engines index the dashboard and the manual      |
| `TEHAS_ANALYTICS_SRC`, `_SITE`  | empty                                    | Optional. The https address of a visitor analytics script and the site name it reports under. Both set: every page loads it |
| `TEHAS_ANALYTICS_COOKIE`        | `0`                                      | `1` adds `data-cookie="true"` to that script tag                |
| `TEHAS_PROTOLAB_URL`            | `https://lab.<domain>`, or port 8081     | Address people reach the Protolab at, when it is published elsewhere, as `https://lab.example`. The dashboard links to it. Without it: `https://lab.<domain>` when `TEHAS_DOMAIN` is set, else port 8081 of the dashboard's host |
| `TEHAS_DOMAIN`                  | none                                     | The domain the edge's names are made under, as `tehas.app`: `uat-<pr>.` for a preview, `lab-<issue>.` for a ticket of the Protolab, `lab.` for its list |
| `TEHAS_PREVIEWS`                | `0`                                      | `1` builds a preview of every open pull request whose branch is in the product repository and that an owner, member or collaborator opened. Needs `TEHAS_DOMAIN` |
| `TEHAS_PREVIEW_MAX`             | `5`                                      | How many previews run at once. The newest pull requests win |
| `TEHAS_PREVIEW_MIN_FREE`        | `5g`                                     | Free disk space below which no preview is built |
| `TEHAS_IMPROVE_REPO`            | empty                                    | Optional. `owner/name` of the repository the [improvement loop](#tehasbot-improve) files its findings in. Empty: no loop |
| `TEHAS_IMPROVE_RUKAS_REPO`      | empty                                    | Optional. `owner/name` of the repository the loop files in what only a change in pi-rukas can fix. Empty: those findings go to `TEHAS_IMPROVE_REPO` with the rest. Needs `TEHAS_IMPROVE_REPO` |
| `TEHAS_IMPROVE_MAX_OPEN`        | `10`                                     | How many issues labelled `tehas:improvement` may be open in one repository before the loop files no more there |
| `TEHAS_WORK_REPO`               | `~/tehas/repos/<name>`                   | The product checkout cycles are cloned from. Must be a clone of `TEHAS_REPO`. A cycle works in `~/tehas/work/<name>-<issue>`, which is not a setting |
| `TEHAS_BRANCH`                  | the checkout's `origin/HEAD`             | Branch cycles start from and pull requests target               |
| `TEHAS_CONCURRENCY`             | `1`                                      | Workstreams side by side inside one cycle. Above 1 only with `TEHAS_TICKET_CONCURRENCY=1`: with both above 1 a command stops with 78 (see [The host](/host#limits-and-concurrency)) |
| `TEHAS_TICKET_CONCURRENCY`      | the host's CPUs minus one, at least 1    | The most cycles side by side. A ceiling, never a divisor: the dispatcher starts fewer while `tehas.slice` has no room, and never more than leave four containers in it (see [Limits that hold work back](/operating#limits-that-hold-work-back)). A value above the host's CPUs is accepted. A container's CPU quota is the host's CPUs less one over this, counted at most 4. Lower it when the model backends cannot serve this many cycles; nothing measures that |
| `TEHAS_MAX_AWAITING`            | `TEHAS_TICKET_CONCURRENCY` plus 2        | Open `tehas:awaiting-merge` issues, plus running cycles, allowed before holding |
| `TEHAS_CYCLE_RESERVE`           | `2g`                                     | Memory one container is given as a floor (`docker --memory-reservation`) and the unit the dispatcher counts room in. A reclaim priority inside `tehas.slice` and the amount admission promises, not a guarantee against the whole host. A Docker size of at least 6m, and at most a quarter of the slice's `MemoryMax`, else the command stops with 78 naming both numbers. With the default the host needs about 11 GiB of memory; a smaller host sets it lower |
| `TEHAS_WORK_TIMEOUT`            | `21600`                                  | Seconds before a cycle is given up. At most 25200: [the cycle's unit](/host#time-limits) stops a cycle after 8 hours |
| `TEHAS_WORK_STALL`              | `3600`                                   | Seconds a cycle may go without a line added to any of its logs before it is stopped as `stalled`. Must be longer than `TEHAS_VERIFY_TIMEOUT`, because a gate can print nothing while it runs; else the command stops with 78 |
| `TEHAS_WORK_IMAGES`             | `0`                                      | `1` gives a cycle and a planning session the images its issue text links: see [tehas work](#tehas-work) and [tehas plan](#tehas-plan). The model must declare `"image"` in its `input` in `models.json` and answer a request with an image, and the sandbox image must say, by the label `tehas.rukas.images` its build writes, that its pi-rukas reads the manifest tehas writes beside them. Else the dispatcher holds the cycles of issues whose text links an attachment; other issues start, and `install.sh` reports what the label says |
| `TEHAS_WORK_DESCRIBE`           | `0`                                      | `1` has the model describe each of those images in words before the cycle or planning session starts, into `images.md` beside them: see [tehas work](#tehas-work). Needs `TEHAS_WORK_IMAGES=1`, else the command stops with 78 |
| `TEHAS_WORK_FILES`              | `0`                                      | `1` gives a cycle and a planning session the text files its issue text links: md, txt, csv, json, yaml, log, html, css and js, at most 16, 1 MB each and 4 MB together. Needs no model that reads images. See [tehas work](#tehas-work) |
| `TEHAS_WORK_COMMENTS`           | `0`                                      | `1` takes the images and files of the issue's comments too: those an allowed account wrote and nobody edited after the approval. Needs `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, else the command stops with 78. See [tehas work](#tehas-work) |
| `TEHAS_VERIFY_TIMEOUT`          | `1800`                                   | Seconds one run of the product's gate may take inside a cycle. A gate that takes longer is stopped and the cycle parks with the cap `verify-timeout:<step>`. Passed to pi-rukas as `PI_ENSEMBLE_VERIFY_TIMEOUT_MS` and `PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS`; pi-rukas alone allows ten minutes. The bound is for one run under the container's CPU quota: a run that waits for another to end does not count its wait. With a `.pi/verify-cmd-fast`, the fast and the full command share it. The cycle's deadline leaves at least this much and 8 minutes before `TEHAS_WORK_TIMEOUT` |
| `TEHAS_RUKAS_WORKAROUNDS`       | empty                                    | Optional. The settings of this instance that only work around a pi-rukas defect, as `NAME@VERSION` separated by single spaces, `VERSION` being the pi-rukas release that fixes it. `tehas work` stops a cycle with 78 once the image's pi-rukas is that release or later, naming the setting: remove it and its entry. An entry for a setting the instance does not set stops every command with 78. Write the pi-rukas issue in a comment beside the setting |
| `TEHAS_PLAN_MAX_TURNS`          | `20`                                     | Turns a planning session may take before it is ended            |
| `TEHAS_PLAN_TIMEOUT`            | `3600`                                   | Seconds before one planning turn is given up. At most 7200: [the poll's unit](/host#time-limits) stops a poll after 3 hours |
| `PI_ENSEMBLE_HOST_ALIASES`      | empty                                    | `name:address` pairs the sandbox must resolve, comma separated  |
| `PI_ENSEMBLE_REVIEW_THRESHOLD`  | unset                                    | Optional. The code-review severity that blocks a change in a cycle: `CRITICAL`, `HIGH`, `MEDIUM` or `LOW`. Read by pi-rukas, not by tehas, and it wins over the severity the product's doctrine (`AGENTS.md` or `CLAUDE.md`) states. Unset, or anything other than those four words: pi-rukas ignores it and uses the severity that doctrine states, and MEDIUM when it states none. tehas does not check this value and does not stop with 78 over it. A `CRITICAL` finding blocks whatever this says |
| `PI_ENSEMBLE_IMAGE`             | `tehas/pi-rukas:uid<uid>`                | The sandbox image                                               |

A line named `PI_ENSEMBLE_*` reaches a cycle's container as it is, except for
the names tehas owns. The same line reaches a planning session and a `tehas dev`
session.

- `PI_ENSEMBLE_IMAGE` and `PI_ENSEMBLE_HOST_ALIASES`: tehas reads these and does
  not pass them on. They pick the image the container runs and its `--add-host`
  lines.
- `PI_ENSEMBLE_NO_DOCKER_SOCKET` and `PI_ENSEMBLE_NO_SSH`: switches of
  pi-rukas's own docker wrapper, which tehas does not use. tehas sets both and
  passes neither on.
- `PI_ENSEMBLE_SANDBOX_MODE` and `PI_ENSEMBLE_WORKSPACE_ROOT`: tehas sets these
  for every container. An instance line for one of them is a second `-e` for a
  name tehas has already fixed, given after tehas' own.
- `PI_ENSEMBLE_MAX_WORKSTREAMS`, `PI_ENSEMBLE_VERIFY_TIMEOUT_MS` and
  `PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS`: tehas sets all three after it
  reads the instance file, the first from `TEHAS_CONCURRENCY` and the two
  timeouts from `TEHAS_VERIFY_TIMEOUT`. An instance line for one of them is
  replaced by tehas' value before the container starts, with no message: set
  the `TEHAS_*` name instead. No instance line gives a cycle the Docker socket
  or SSH keys.
- `PI_ENSEMBLE_EXTRA_IMAGE_DIRS` names the mounted images directory and is
  tehas' to set when `TEHAS_WORK_IMAGES=1`, where an instance line for it is
  dropped (see [tehas work](#tehas-work)).
- `PI_ENSEMBLE_HANDOFF_CHANNEL`, `PI_ENSEMBLE_CYCLE_DEADLINE` and
  `PI_ENSEMBLE_VERIFY_CONCURRENCY` are `tehas work`'s to set for a cycle, and
  `PI_ENSEMBLE_CYCLE_BUDGET_MS` is not passed: an instance line for one of
  them is dropped (see [tehas work](#tehas-work)). A planning session and a
  `tehas dev` session get none of them.

A lever pi-rukas reads under any other name needs no setting of tehas' own, and
tehas neither checks its value nor passes it on under another name. It does
check the name: before a cycle starts, `tehas work` asks the image's runner
(`pi-rukas-headless --capabilities`) which inputs it reads, and a
`PI_ENSEMBLE_*` name the container would be given that the runner does not list
as one a host sets stops the cycle with 78, naming it. Unlike a
`TEHAS_*` setting, a malformed value here is not refused with 78: it reaches
pi-rukas and pi-rukas decides what to do with it, which for
`PI_ENSEMBLE_REVIEW_THRESHOLD` is to ignore it. The channel also carries
pi-rukas levers that loosen a cycle's own guards, `PI_ENSEMBLE_PROTECTED_PATHS`
and `PI_ENSEMBLE_POLICY_JUDGE` among them. tehas sets neither, and setting one
is a person's decision on the instance, never something a cycle can do for
itself.

The example instance sets the five required ones, `TEHAS_HOST_ADDR`,
`PI_ENSEMBLE_HOST_ALIASES`, and, because it is public, `TEHAS_INDEXING` and the
analytics settings. It leaves the rest at the defaults.

There is no CPU or memory budget to set. The limit around every sandbox
container is one systemd slice on the host, `/etc/systemd/system/tehas.slice`,
installed once by root with `sudo ./bootstrap.sh slice`; see
[The host](/host#limits-and-concurrency). An instance that still sets
`TEHAS_WORK_CPUS` or `TEHAS_WORK_MEMORY`, the budget of an earlier version, is
refused with 78 and one line at the first command, and `install.sh --check`
reports `Instance: <directory> INVALID` with that line under it and exits 78:

```
tehas: TEHAS_WORK_MEMORY is set and this version of tehas has no memory budget to set: the limit around all containers is /etc/systemd/system/tehas.slice (see .../instance.env)
```

Delete the line. An instance with no `TEHAS_TICKET_CONCURRENCY` line goes from
one cycle at a time, the default of the earlier version, to the host's CPUs
minus one; set the line to keep a lower ceiling.

## tehas dispatch

One poll of the issue queue. Deterministic, no model. The timer runs the same
script as `tehasbot dispatch`.

```
tehas dispatch --dry-run       # report the decision, change nothing
tehas dispatch                 # start the cycles there is room for, then return
tehas dispatch --help          # print the usage of the command, call nothing
```

`--help` (or `-h`) prints the header comment of the command and exits 0; it
works even on a host with no instance settings yet. Any other argument, or more
than one, is refused: exit 64 with `tehas dispatch: unknown argument: <argument>`
and the usage on standard error, before the instance settings are read. A mistyped
`--dryrun` therefore starts no poll and changes nothing.

Order of rules: the pause switches (this host's flag, then the label on the
control issue) decide that this poll starts nothing. They do not stop it cleaning
up what this host already has: it still stops a cycle whose issue is closed or
whose pull request is merged or closed, stops a cycle nobody has heard from, and
clears an issue marked `tehas:running` with no cycle behind it. The poll ends
there. For a factory that is not paused it goes on: a usable `tehas.slice`,
a model endpoint that answers and offers the model, the free slots
(`TEHAS_TICKET_CONCURRENCY` less the running cycles, after that housekeeping),
the cap on issues awaiting merge or being worked on, the room, the containers'
ceilings and the memory pressure in the slice, who applied `tehas:urgent`,
then who applied `tehas:ready`, then
urgent first and lowest number. A `tehas:urgent` counts only when an allowed
account applied it most recently: one applied by the bot or anyone else is
ignored and logged, and the issue keeps its place by number. The urgent check
runs for every ready issue that carries the label and this poll could still
start, before the free slots cut the queue; who applied `tehas:ready` is read
at the decision, when the issue is picked. A poll never asks GitHub the same
question twice.
What holds cycles back is one `holding:` line: at every dry run, and in the
log once when its reason changes, with `released: cycles may start again`
when it ends. A real poll also writes it to `~/tehas/live/dispatch.json` with
the slice's numbers, for [tehas status](#tehas-status). The holds are listed
under [Troubleshooting](/troubleshooting). Log:
`~/tehas/logs/dispatch.log` when run by systemd, the terminal otherwise. Each
cycle's runner output: `~/tehas/logs/dispatch-work-<issue>.log`.

When it has an issue to start, it puts the product checkout on the default
branch and fast-forwards it, then, if no cycle is running, refreshes the code index with
[tehas index](#tehas-index) when the branch has moved since the last indexing.
An index that cannot be refreshed is a warning in the log; the cycle still
runs. It then marks each issue `tehas:running` and starts
`tehas-cycle@<issue>.service`, which runs [tehasbot cycle](#tehasbot-cycle).
It does not wait for the cycle. If the unit does not come up within 30
seconds, the issue gets `needs-human-attention`, the poll goes on to the
remaining issues, and it ends with exit 1.
The dispatcher runs the index, plan and status scripts from its own directory,
`~/tehas/libexec`, not by a name on `PATH`.

The labels `tehas:plan`, `tehas:planning` and `tehas:question` exist in a
repository only after [tehas labels](#tehas-labels) has been run for the
version that uses them. After pulling a version that adds a label, run it
again on the host. It only creates or updates labels, so running it twice is
safe. Without it the labels are not there to apply, and a stopped cycle cannot get
its question label. The bot then says in a second comment that the boxes
cannot be used.

Beside the queue, a poll also reads two kinds of answer. A box ticked by an
allowed account in the bot's comment on a stopped cycle can start a fresh
cycle, and a reply or a ticked box on a planning question runs the next turn of
that session. A retry waits for a free slot and for the cap, as any cycle does.
A planning turn waits for neither. The poll waits for a planning turn to end,
so no cycle starts while one runs; cycles that are already running go on. A comment is a question only if the host has its id on record:
in `~/tehas/state/plan-<issue>.json` for a planning session, in
`~/tehas/state/park-<issue>.id` for a stopped cycle. Only the last 100
comments of an issue are read; a question further back than that is skipped
with a line in the log. See [Operating](/operating). The dispatcher stops with exit 78 if
the bot's own login is in `TEHAS_ALLOWED_LABELERS`, and with exit 1 when GitHub
answers without one. Every poll reads that login once, before it reads a label
event or an answer and before it changes a label or comments, so a list naming
the bot is refused whatever else the poll would have done. A planning turn's
runner output is in `~/tehas/logs/dispatch-plan-<issue>.log`.

A cycle that a provider outage parked starts again once without a tick.
`tehasbot cycle` asks for it in `~/tehas/state/cycles/<issue>.park`. When
nothing holds cycles back, so the model endpoint answers and there is a slot,
the poll starts the issue and says on it that it does so once. The start rests
on the account that started the cycle that parked, kept in
`~/tehas/state/cycles/<issue>.by`: it must still be allowed, the issue must
still be open and parked, and nobody else may have changed its text since that
cycle began. Otherwise the log says `skip #N: its retry after a provider
outage is not started; it waits for a person` and the park file goes. An issue
that carries `tehas:ready` is left to that label. If the retry's own cycle is
parked by an outage again, it waits for a person.

A poll also reads the open issues marked `tehas:running`. One whose cycle
does not hold its lock on this host, `~/tehas/state/cycles/<issue>.lock`, gets
`needs-human-attention` in place of `tehas:running` and a comment that says
where the run log is. The log line is `park #N: it is marked tehas:running and
no cycle holds its lock`. The issue is read again first: one whose cycle
ended while the poll ran has lost `tehas:running` already and is left, with
the line `skip #N: tehas:running is gone from it since the list was read`. If
the label change fails nothing is said on the
issue and the next poll tries again. A closed issue in that state only loses
`tehas:running`. For either, a container `tehas-work-<issue>` that is still
there is removed, with the line `removed the container tehas-work-N: no cycle
holds the lock of #N`: its client died, and it would work on with the bot's
token and hold the name. The container of a planning turn that was cut off,
`tehas-plan-<issue>`, is removed the same way before the turn is reported. A
paused factory leaves such an issue as it is. Run the dispatcher only on the host that runs the cycles: on any other
host no cycle holds a lock.

The other way round, a cycle that holds its lock but has not been heard from
for 20 minutes is stuck, for example a `tehas work` that hangs after its
container ended. Heard from means its live file,
`~/tehas/live/cycles/<issue>.json`, was written, which `tehas work` does every
10 seconds from the host, also while a long gate runs in the container, or
that it took its lock. The poll logs `WARN: the cycle for #N holds its lock but
has not been heard from since <time>, more than 1200s: stopping
tehas-cycle@N.service` and stops the unit without waiting for it. The cycle
reports as it stops; a later poll finds the issue as above. This is said and
done once for each time the cycle was last heard from. Every ask is recorded in
`~/tehas/state/cycles/<issue>.quiet`: the time the cycle was last heard from,
how many times the poll has asked, and when it asks next. Exit 0
from `systemctl --user --no-block stop` only says systemd accepted the stop
job, so the note means the stop was asked, not that the unit stopped; what
ends a unit that ignores the job is the unit's own `TimeoutStopSec` (300s) and
`RuntimeMaxSec`. A stop that failed or timed out adds `WARN: could not stop
tehas-cycle@N.service (exit <code>): it holds its slot, and the poll asks
again`. While the cycle holds its lock the poll asks again, at most once every
10 minutes (`WARN: ...: the poll asks tehas-cycle@N.service to stop again`),
and after 6 asks it says the episode needs a person and asks no more, so one
wedged unit does not hold the poll. The note goes once the
episode is over: a later poll removes it when it can tell that no cycle holds
that issue's lock, or the cycle is heard from again. A dry run says `would
stop`, stops nothing and removes no note. A paused factory does not stop it.

An issue is taken only when an allowed account applied `tehas:ready` and
nobody else has changed its title or body since, and only while GitHub says the
number is an open issue. A closed issue and a pull request are never taken,
whatever label they carry. The poll's lists are of open issues, so what this
catches is a number that changed since the list was read. The poll asks GitHub
what a number is once a poll, and only for an issue its labels would otherwise
take; the skip is said once a poll at most (`skip #N: it is closed, and a
closed issue is never taken`, or `skip #N: it is a pull request, not an issue
the factory works on`). An answer that cannot be read ends the poll with exit
1.

An issue that an open pull request closes, as GitHub links them (`Closes #N`
in its description, or a link made by hand), is not taken: a cycle would find
the work done after paying for its explore and plan. The poll asks once, for
an issue it would otherwise take, takes `tehas:ready` off and says on the
issue which pull request it is. The log line is `skip #N: pull request <url>
is open and closes it`. A merged or closed pull request holds nothing back. A
call that fails, or an answer that cannot be read, ends the poll with exit 1.

A `tehas:ready` from any other account asks for nothing, so it comes off the
issue with a comment naming who
applied it; the log line is `clear #N: tehas:ready off, applied by ...`. A
`tehas:plan` from such an account comes off the same way. The
comment follows the label change: when that change fails nothing is said on the
issue and the next poll tries again. An issue the poll does not read as a
candidate keeps its labels as they are, which covers one whose own cycle is
alive and every issue while a hold applies. No other label holds it
back: `tehas:awaiting-merge` and `tehas:question` are taken off as the cycle
starts. `needs-human-attention` is left for the cycle, which starts with
`--restart` and has pi-rukas take it off. While the issue's own cycle is alive the log
says `wait #N: its cycle is still running`, and the issue is taken once that
cycle has ended. Every list of issues is read up to 1000. A cycle starts only once the
issue is marked `tehas:running`; if that label change fails, the poll stops.

Before it marks an issue, the poll writes what the start rests on to
`~/tehas/state/cycles/<issue>.approved.json`: `at` and `by`, when and by whom
it was approved, `how` (`label`, `retry-tick`, or `outage-retry` for the one
start after a provider outage, whose `at` is when the parked cycle began),
`body_sha256`, the sha256 of the body that approval was checked against with
carriage returns dropped, and
`links`, the links to GitHub's attachment storage in that body. The body is
read in the same call as the record of who last edited it, so it is the text
the check passed. With `TEHAS_WORK_COMMENTS=1` the poll also lists the
issue's comments and adds `comment_links`: the links of each comment an
allowed account wrote whose `created_at` and `updated_at` are at or before
`at`, in the order written, each as `{link, source, by}` with `source`
`comment:<id>`. [tehas work](#tehas-work) fetches those links and no others.
A record that cannot be written starts nothing: `ERROR: could not write the
approval record of #N, not starting a cycle`, exit 1, and no label changes.
Comments that cannot be read are logged as `ERROR: the comments of #N could
not be read` and stop the start the same way. On a retry tick, with the
setting on, the poll also lists the bot's pull requests that name the issue,
as `tehasbot cycle` finds a cycle's, and their comments: those an allowed
account wrote between the park question and the tick and nobody edited after
the tick. Their links join `comment_links` with `source` `pr-comment:<id>`,
and their text, the first 20 cut at 4000 characters each, goes to
`retry_context` as `{source, by, at, text}`. Pull requests or comments that
cannot be read stop the retry: `ERROR: the comments on the pull request of
#N could not be read`, exit 1, and the tick stays. Only conversation comments
are read, not review comments or reviews.

## tehas work

One pi-rukas `/work` cycle, headless, in the sandbox image.

```
tehas work <issue> [--restart]
```

The container's command is pi-rukas's one-shot runner,
`/opt/pi-rukas/bin/pi-rukas-headless work <issue> --timeout <minutes> --json`,
with `--restart` added for `--restart`. The runner is given
`TEHAS_WORK_TIMEOUT` in whole minutes and stops itself there; a runner that
has not stopped `TEHAS_WORK_GRACE` seconds (300) later is stopped with
`docker stop`. `tehas work` waits for the runner to exit and reads nothing
from the workspace `~/tehas/work/<name>-<issue>`: the outcome is the runner's
`--json` result, which goes to `~/tehas/state/cycles/<issue>.result.json`, and
a copy beside the log. The contract is pi-rukas's `docs/headless.md`: exit
codes v1, result schema 3, NDJSON progress with a stable subset.

Before anything is moved or fetched, it asks the image what it supports,
`pi-rukas-headless --capabilities`. The cycle starts only when the image speaks
exit codes 1, result schema 3 and stable NDJSON 1, the `work` verb takes the
flags passed, and every `PI_ENSEMBLE_*` variable the container would be given
is an input the image lists for a host to set (scope `cycle` or `sandbox`; a
family such as `PI_ENSEMBLE_MODEL_<ROLE>` covers the members the image lists
for it, or any name when it lists none, as v0.12.80 does), and the answer's
`features` say `true` for `handoffChannel`, for `cycleDeadline` when the cycle
has a deadline and for `restartClearsAttention` with `--restart`. Anything else
is exit 78, with what is not supported named:

```
tehas work: tehas/pi-rukas:uid1000 does not support what this cycle is given, not starting a cycle:
    the input PI_ENSEMBLE_SOMETHING
```

Then each entry of `TEHAS_RUKAS_WORKAROUNDS` is checked against the image's
pi-rukas version: one whose release it has reached, or any entry when the
version cannot be read, is exit 78:

```
tehas work: TEHAS_RUKAS_WORKAROUNDS names a workaround this image no longer needs, not starting a cycle:
    PI_ENSEMBLE_COMMENT_RETENTION works around a pi-rukas defect fixed in 0.12.80, and the image's pi-rukas is 0.12.80
    Remove the setting and its entry from /home/tehas/.config/tehas/instance.env.
```

Three inputs are this command's, whatever the instance says:

| Input | Value | Why |
| --- | --- | --- |
| `PI_ENSEMBLE_HANDOFF_CHANNEL` | `host` | pi-rukas posts no handoff comment and applies no label. `tehasbot cycle` posts the one comment of the park from the result's handoff, and labels the issue |
| `PI_ENSEMBLE_CYCLE_DEADLINE` | the start plus `TEHAS_WORK_TIMEOUT` less a margin, in milliseconds | After it the cycle starts no new step and parks with the cap `deadline`, with a handoff. The margin is a quarter of the limit, or one run of the gate (`TEHAS_VERIFY_TIMEOUT`) and 8 minutes when that is longer: a step that started before the deadline may run past it. With the default limit the deadline is 4 h 30 min in. A limit too short for the margin gives no deadline, and the log says so |
| `PI_ENSEMBLE_VERIFY_CONCURRENCY` | one for each two whole CPUs of the container's quota and each 2 GiB of its memory ceiling, the lower, at least 1 | How many runs of the gate the cycle makes at once. pi-rukas's own rule for a host, applied to the container. On 16 CPUs with four cycles side by side that is 1 |

It holds the issue's lock, `~/tehas/state/cycles/<issue>.lock`, while it runs.
Run by hand for an issue whose cycle is running, it refuses before it moves
anything (exit 5). Under `tehasbot cycle` it is handed that lock, already
held, and goes on. While a run by hand holds the lock, the dispatcher counts it
as the issue's cycle and starts no other for the issue. With the lock held, a
container left under the name `tehas-work-<issue>` is no cycle's: it is
removed before the new one starts.

| Exit | Meaning                                                          |
| ---- | ---------------------------------------------------------------- |
| 0    | The runner's: `merged`, or completed with nothing to merge       |
| 10   | The runner's: `handoff`, parked for a human                      |
| 11   | The runner's: `handoff` for clarification (`intent-park`, `explore-needs-clarification`) |
| 20   | The runner's: `aborted`                                          |
| 21   | The runner's: `interrupted` by a signal, a stop here among them  |
| 30   | The runner's: the driver crashed, or the runner's own time limit passed |
| 40   | The runner's: it refused to start the cycle                      |
| 2    | The cycle was stopped here as stalled or at the time limit and grace, or the runner ended without a result |
| 3    | The sandbox cannot push to the repository; no cycle was started  |
| 4    | With `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`: an image or file the issue links could not be fetched, the issue could not be read for them, its text is not the text that was approved, or a file is not the one first fetched for the approval; no cycle was started |
| 5    | A cycle for this issue is running already: its lock `~/tehas/state/cycles/<issue>.lock` is held. Nothing was moved and no container started |
| 64   | The issue is not a number, or no bot token or no git identity; no cycle was started |
| 78   | The instance settings are invalid, `tehas.slice` is missing, has no memory limit, or cannot be used (the message names `sudo ./bootstrap.sh slice` or the reason), the image does not support what the cycle is given, or a workaround in `TEHAS_RUKAS_WORKAROUNDS` has expired; no cycle was started |
| 137  | The container was killed while the cycle was running, and the runner wrote no result: out of memory, or stopped from outside. The cycle is `killed` |

The runner's codes are its own only when it wrote a result for the issue, or a
result with no issue entry at all (a refusal that carries only an error). A
docker client that exits otherwise, after a result was written, gives the
result's `exitCode`; a result without one is exit 2. A runner stopped here at
the time limit and grace is `timeout`, whatever its result says, and the
limit's seconds go to `~/tehas/state/cycles/<issue>.timed`, which `tehasbot
cycle` says on the issue and removes.

The container runs in `tehas.slice` and gets the bot token and the
`PI_ENSEMBLE_*` settings, a memory ceiling of a quarter of the slice with no
swap, `TEHAS_CYCLE_RESERVE` as its floor, a CPU quota of its share of the
host's CPUs, a pids limit that follows the quota, a CPU weight and an init, and
no Docker socket or SSH keys. git inside it is given the
bot's identity and a credential helper that reads the token, through
environment-level configuration; before a cycle starts, a dry-run push proves
that works. The runner's NDJSON progress, its error stream, is kept in
`~/tehas/logs/work-<issue>-<time>.jsonl`. The console and the dashboard's line
read only the kinds the runner declares stable; a line it marks `unstable` is
not shown. At the end, `~/tehas/state/cycles/<issue>.progress.json` holds the
token usage per role, why the cycle stopped at its cap and the kind of that
failure, for `tehasbot cycle`. They come from the result (`usage.byRole` and
`failure`) when the image's `--capabilities` answer says it carries them
(`features.usageByRole`, `features.failureReason`). An image that does not,
v0.12.80, gives the usage of the stable `dispatch-completed` lines, each
counted once by its key, and a reason read from lines the runner marks
unstable, which may be empty.
The time limit is `TEHAS_WORK_TIMEOUT`. The push check, the clone and every
`docker stop` have bounds of their own, listed under
[time limits](/host#time-limits). [The host](/host#limits-and-concurrency)
has the limits in full.

Beside the workspace is `~/tehas/work/<name>-<issue>.state`, a directory of
this cycle's own: `cache` holds its copy of the code index the host built, and
the agents are told where they have it with `CBM_CACHE_DIR`; `memory` is what
its agents learn; `sessions` holds Pi's session files. Nothing a cycle writes
there is what a later cycle of another issue reads. It is made fresh for every
cycle and removed with the workspace.

What a cycle caches under its home otherwise, package downloads among them,
lives in its container's own layer and goes when the container is removed. A
cycle that installs packages fetches them again: for a product whose gates run
`bun` or `npm install`, that is the download of its dependencies once per
cycle, minutes on a cold container and nothing afterwards.

A cycle that writes nothing does not wait for its time limit. When the
runner's progress and the agents' transcripts under
`~/.pi/agent/ensemble-runs/issue-<issue>` all stay unchanged for
`TEHAS_WORK_STALL` seconds, the container is stopped and the cycle is
`stalled`. A step that hangs on a child process, or a gate that never returns,
then frees its slot within the hour. The seconds go to
`~/tehas/state/cycles/<issue>.stalled`, and `tehasbot cycle` says on the issue
how long nothing moved and at which step, parks it and removes the file. A
cycle that keeps writing without getting anywhere is not caught by this: it
runs until its time limit.

While the container runs, Docker keeps its id in `~/tehas/state/cycles/<issue>.cid`,
which is how the dispatcher tells a cycle's container from the others in the
slice; the file goes with the container. When processes in the container were
killed for memory, the count is printed and left in
`~/tehas/state/cycles/<issue>.oom` for `tehasbot cycle`, which says it in the
issue comment and removes the file. The starts of a process that the pids
limit refused go the same way, through `~/tehas/state/cycles/<issue>.forks`.

The bot's git identity is the tehas user's global `user.name` and `user.email`
on the host.

With `TEHAS_WORK_IMAGES=1` the container is given two things more: a
directory with the issue's images, mounted read-only at
`~/tehas/work/<name>.images`, and `PI_ENSEMBLE_EXTRA_IMAGE_DIRS`, which names
that path. This widens what a cycle is given, and it is off unless the
instance sets it. The images are fetched before anything else is made:

- **Which.** Links to GitHub's own attachment storage in the issue text.
  Comments are read only with `TEHAS_WORK_COMMENTS=1`, and then only those an
  allowed account wrote and nobody edited after the approval: see below. Only
  when an allowed account approved the start,
  which approves the text with its links, or when the issue was opened by an
  owner, member or collaborator of the repository. When the dispatcher left
  its approval record, `~/tehas/state/cycles/<issue>.approved.json`, and it
  is a file and not a link, the links are the record's and the approver is its
  `by`. The body is read again and must have the record's hash: a body changed
  since the approval starts nothing, exit 4, said on the issue. A record that
  cannot be read stops the start the same way. Without a record, by hand, the
  links are the body's and the approver is who applied the last `tehas:ready`,
  read from the issue's events; when they cannot be read nothing starts,
  exit 4.
- **Types.** png, jpg, webp and gif, told by the file's first bytes and not by
  its name. Not svg: it is text and can carry instructions.
- **How much.** At most 8 images and 10 megabytes together, counted in the
  order they are linked.

On the host they are in `~/tehas/work/<name>-<issue>.images`, beside the
workspace and not in it, so no patch can commit them. The files are named
`image-1.png`, `image-2.jpg` and so on, and `manifest.json` lists each with
the link it came from, its type, its size and its sha256, the approval it was
fetched for (`at`, `by` and `how` of the record, or null by hand), and every
link that was left out with the reason. The manifest is kept as
`~/tehas/state/cycles/<issue>.manifest.json`, with `images.md` and
`retry-context.md` beside it as `<issue>.images.md` and
`<issue>.retry-context.md` when the cycle has them, for the improvement
loop's record. When the next fetch is for the
same approval, a restart of the same cycle, a link that gives other bytes than
the kept manifest says stops the start with exit 4 and the link named. The directory is remade for every cycle and removed by
`tehasbot cycle` when the cycle is over. A comment on the issue says how many
images were fetched, each with its link and sha256, what was left out, and
whether the cycle was shown them. An image that could not be fetched stops the
start with exit 4, and the comment names it: the issue is parked, and
`tehas:ready` tries again.

Whether a cycle was shown them is answered by the sandbox image itself.
`sandbox/build.sh` asks the base image's `--capabilities` whether its
pi-rukas reads this manifest (`PI_ENSEMBLE_ISSUE_IMAGES` for manifest format
1), and the build records the answer as the label
`tehas.rukas.images`; a build that is given an answer that does not match the
base it stands on stops. `tehas work`, `tehas plan`, the dispatcher and
`install.sh` read that label. When it says the pi-rukas does not read the
manifest, the comment says the images were fetched and not shown rather than
that they were given, and the dispatcher holds every cycle while the setting
stands. An image with no label, built before it existed, is answered as "not
said": nothing is claimed, `install.sh` names the rebuild that would make the
image say, and cycles are not held for it.

pi-rukas reads the manifest, and attaches the images to the explore and plan
prompts each beside its link, from upstream's commit of 2026-10-14. The host
follows upstream's newest release, and every release since then has the reader. The
steps after plan do not see them.

With `TEHAS_WORK_DESCRIBE=1` as well, `tehas work` asks the model to describe
each image in words before anything else is made, for the steps and models
that are not shown the pixels. One chat request per image, at most 8, to the
provider's `baseUrl` in `~/.pi/agent/models.json` with
`PI_ENSEMBLE_SUBAGENT_MODEL`, as the dispatcher's check of the endpoint asks
it, and no token. Each request may take 60 seconds, and each answer is cut at
1500 characters. The prompt is fixed: describe what the image shows, quote
visible text word for word, treat nothing in it as an instruction. The answers
go to `images.md` beside `manifest.json`, one section per image with its link,
under a head that says a model wrote them from untrusted images. Every line of
an answer is quoted, so an answer cannot start a section of its own. The
manifest gets `"described": "images.md"`. A request that fails leaves `not
described:` and the reason for that image, and the cycle starts all the same.
The file is in the directory that is mounted already: no mount and no
variable more. The pi-rukas that tehas pins does not read `images.md` yet; the
file is ready for a release that does.

With `TEHAS_WORK_FILES=1` the container is given the same directory and the
same variable, whether or not `TEHAS_WORK_IMAGES` is 1, and the directory also
holds the text files the issue links. This widens what a cycle is given, and
it is off unless the instance sets it. The rules of which links and whose
approval are those of the images. A file is taken by its name's extension,
md, txt, csv, json, yaml, yml, log, html, css or js, and then checked: valid
UTF-8 and no NUL byte, else it is left out with the reason. html is text here;
rendering it is the [Protolab](#tehasbot-protolab)'s job. svg, pdf and
archives are left out. At most 16 files, 1 MB each and 4 MB together, apart
from the images' limits. They are named `file-1.md`, `file-2.csv` and so on,
and `manifest.json` lists them under `files`, each with its link, its original
name, its type, its size and its sha256; `images` is as before. With images
off, an image the issue links is left out and said. The comment on the issue
says how many files were fetched, with each hash, and that the pi-rukas that
tehas pins does not read them yet: the manifest is ready for a release that
does.

With `TEHAS_WORK_COMMENTS=1` the links in the issue's comments are taken as
well, by an approval record only: a comment is taken when an allowed account
wrote it and it was written and last edited at or before the approval's time.
The dispatcher lists the comments when it decides the start and adds their
links to the record as `comment_links`, each with its `source`,
`comment:<id>`, and its author. `tehas work` fetches them after the body's
links, in the order written, under the same limits, counted together, and
only while the author is still allowed. Every entry of the manifest, left out
or not, has a `source`: `body` or the comment's. The comment on the issue
names the comment beside each link. A start by hand, without a record, reads
the body only. When the record has a `retry_context`, the text of a
reviewer's comments on the pull request before a retry tick, it goes to
`retry-context.md` in the same directory, under a head that says it is
untrusted input, every line quoted, each comment cut at 4000 characters and
the whole at 32000, and the manifest gets `"retryContext":
"retry-context.md"`. The pinned pi-rukas does not read it yet. The setting adds no mount and no variable, and it needs
`TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, else the command stops with
78.

## tehas plan

Runs one turn of a planning session, headless, in the sandbox image. The
dispatcher calls it. By hand it is useful to see what a turn does.

Run by hand it removes and remakes the workspace that a planning turn of the
same issue uses, and it does not take the dispatcher's lock, so do not run it
for an issue whose session the factory is working on.

```
tehas plan 12 --out /tmp/turn.json --keep /tmp/turn.keep --brief brief.md
tehas plan 12 --out /tmp/turn.json --keep /tmp/turn.keep --session <id> --answer answer.md [--comment <id>]
```

The first form starts a session from a file with the issue's title and body.
The second reopens the session and sends the file's content as the operator's
answer. A turn ends when the agent has settled and every subagent it sent out
in the background has reported back, so it ends when the agent has asked. A
subagent that never reports holds the turn until `TEHAS_PLAN_TIMEOUT`.

`--keep` names a directory the session carries from one turn to the next. The
first form makes it anew. It holds the approved title and body, which every
turn's workspace gets as `.tehas/brief.md`, so a later turn never plans from
the issue as it is now, the commit the first turn's workspace was
at, pi-rukas's record of the plans the agent showed (its plain `.json` files
and its `.filed` and `.claim` markers, at most 1 MB each), and with
`TEHAS_WORK_IMAGES=1` the issue's images. Every later turn's workspace is at
that commit and gets that record back. A later turn whose commit is missing or
gone starts at the tip of the branch and says so.

pi-rukas keys that record on `origin/<branch>` after a fetch of its own, and
on what the agent asks for. In a turn's workspace `origin/<branch>` is the
session's commit, and the clone has no fetch refspec, so a fetch inside the
container writes only `FETCH_HEAD` and the key stays put. Pushing works as
before. The container is given `PI_ENSEMBLE_PLAN_CACHE_TTL_MS=0`, whatever
the instance sets, so the record does not expire; a pi-rukas without that
setting expires it after two hours. A plan the operator confirms is filed as
it was shown only when pi-rukas finds it in the record: otherwise the agent
drafts it again, and the draft may differ. The dispatcher keeps the
directory at `~/tehas/state/plan-<issue>.json.keep` and removes it when the
session ends.

With `TEHAS_WORK_IMAGES=1` the first turn fetches the images the issue links,
by the rules of `tehas work`, with `tehas:plan` in place of `tehas:ready`,
and says on the issue what it fetched and whether the session was shown them.
Each turn's container gets a copy of them, read-only, at the path a cycle has
them, with `PI_ENSEMBLE_EXTRA_IMAGE_DIRS` and `PI_ENSEMBLE_PLAN_ISSUE`, the
issue's number. The copy is
`~/tehas/work/<name>-plan-<issue>.images`, removed after the turn. An image
that cannot be fetched starts no session: exit 4, said on the issue. With
`TEHAS_WORK_DESCRIBE=1` the first turn also writes `images.md`, as
`tehas work` does, and every turn's copy holds it. With `TEHAS_WORK_FILES=1`
the first turn fetches the text files too, by the rules of `tehas work`, into
the same directory.

A later turn given `--comment <id>`, the id of the reply its answer came
from, fetches the images and files the answer links into that directory
before it starts: by the same rules and limits, counted with what is there,
numbered on from it, and with `source` `answer:<id>` in the manifest. A link
already there is not fetched again, and the new images are not described. The
dispatcher passes the id when the answer is a reply; the reply counts only
from an allowed account, so its links are approved as the label approves the
body's. The answer sent to the agent ends with the names of the new files. A
link left out is said on the issue. A link that cannot be fetched runs no
turn: exit 4, said on the issue, and the directory stays as it was; a reply
runs the turn again. On exit 0 the `--out` file holds the session id, the agent's last
message and the question it asked, if any. When it asked more than one, they
are put together as one numbered question with no options, so the operator
answers them in a reply, and the log has a warning. On exit 2 it holds the session id
alone when that is known, so the dispatcher can run the turn again in the
same session, and it is not written when no session is known.

A later turn first asks Pi what is in the session it opened. Pi makes a
session anew when its file under `~/.pi/agent/sessions` is gone, and an agent
with no memory of the session would take the answer as a new request. So when
Pi says the session has no messages, the answer is not sent: the turn exits 2
and the `--out` file holds the session id and `"lost": true`. The dispatcher
then ends the session and says on the issue to apply `tehas:plan` again. A
turn whose container never says what is in the session sends no answer
either, and exits 2 as a turn that can be run again.

| Exit | Meaning                                                              |
| ---- | -------------------------------------------------------------------- |
| 0    | The turn settled and said or asked something                         |
| 2    | The turn did not settle, timed out, said and asked nothing, its session is gone, or its session or outcome could not be written down |
| 4    | With `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`: a first turn where an image or file the issue links could not be fetched, or the issue could not be read for them, or a later turn where one its reply links could not be fetched; nothing ran |
| 64   | A usage error, no bot token or no git identity; nothing ran          |
| 78   | The instance settings are invalid, or `tehas.slice` cannot be used; nothing ran |

The container is the one `tehas work` starts, under the name
`tehas-plan-<issue>`: the same image, the same limits in `tehas.slice`, the
same mounts, token and settings, from `lib/sandbox.sh`, and no Docker socket,
SSH keys or `~/.config/tehas`. It works in a workspace of its own,
`~/tehas/work/<name>-plan-<issue>`, made fresh for the turn and removed after
it, its cache, memory and sessions go in `<name>-plan-<issue>.state` beside it
and are removed with it, and its transcripts go to
`~/.pi/agent/ensemble-runs/plan-<issue>`. There is no dry-run push
before a turn. The agent files issues with the token. The RPC stream is
kept in `~/tehas/logs/plan-<issue>-<time>.jsonl`. The limit is
`TEHAS_PLAN_TIMEOUT`, and the session record is
`~/tehas/state/plan-<issue>.json`. Once the outcome is read the session is
ended by closing Pi's input, and only a Pi that has not gone two seconds later
is stopped with `docker stop`. A container left under the turn's name, by
a turn whose client died, is removed before the turn starts.

## tehas dev

An interactive Pi session with pi-rukas, in the sandbox image, in a clone of
the product that is kept. For working by hand; see
[Operating](/operating).

```
tehas dev [<name>] [--author "Name <email>"] [--issue <number>] [-- <pi arguments>]
tehas dev --help
```

`<name>` names the clone, `~/tehas/dev/<name>`: letters, digits, `_` and `-`,
`main` when left out. It is made on first use from the product checkout and
never reset or removed. `--author` is stored in `~/tehas/dev/<name>.author` and
becomes git's name and email in every session of that clone; without one the
bot's are used. What follows `--` is passed to `pi`.

`--issue <number>` gives the session the images and text files that issue
links, with `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, by the rules of
[tehas work](#tehas-work) without an approval record. The person running the
command approved nothing, so the images are taken when an allowed account
applied the issue's last `tehas:ready`, or else when an owner, member or
collaborator opened it. They go to `~/tehas/dev/<name>.images`, remade at
every start with `--issue`, and the container gets that directory read-only
at the same path and `PI_ENSEMBLE_EXTRA_IMAGE_DIRS`, which names it. What was
given and left out is printed; nothing is said on the issue. Without
`--issue` the session gets neither, and an instance's
`PI_ENSEMBLE_EXTRA_IMAGE_DIRS` is not passed on.

| Exit | Meaning                                                                                   |
| ---- | ----------------------------------------------------------------------------------------- |
| any  | The session's own exit status, as Docker reports it                                       |
| 4    | With `--issue`: an image or file the issue links could not be fetched, or the issue or its label events could not be read; no container |
| 64   | A wrong argument, a malformed author, no bot token or no git identity; no container       |
| 78   | The instance settings are invalid, the clone's directory is not a clone, `tehas.slice` cannot be used, or `--issue` was given and the instance gives no images or files; no container |

Arguments are checked before the settings are read, so `--help` works on a
host with no instance and a mistyped argument makes no clone.

The container is the one `tehas work` starts, with a terminal: the same image,
the same limits in `tehas.slice`, mounts, token and settings. It is named
`tehas-dev-<name>`, so a clone has one session at a time. Before it starts, the
command prints how many more cycles the slice has room for, because the session
takes room from them. The command does not take the dispatcher's lock and
writes nothing under `~/tehas/live`. It needs a terminal; Docker refuses to
start it without one.

After the clone is made the host runs no git command in it, because agents
have written there since.

The session's own memory, Pi sessions, transcripts and code index are in
`~/tehas/dev/<name>.state`, beside the clone and kept with it: a person
resumes their sessions and keeps the index their session built, and no cycle
shares either. The host's index of the product is not copied there, because it
is keyed to the path a cycle works at, which is not the path of this clone. A
session that ran before the state directory existed has its history in
`~/.pi/agent/sessions`, which no container is given now: the next session
begins a new history in `<name>.state/sessions`.

### From another account

```
tehas dev [-- <pi arguments>]
```

An account in the group `tehas` types the same words and gets a session in
the clone named after its login, with `user.name` and `user.email` from its
own git configuration as the author. It takes no name and no `--author`.

That `tehas` is another file: `/usr/local/bin/tehas`, installed for everyone by
`sudo ./bootstrap.sh dev`. It reads the account's git identity and asks `sudo`
to run `~/tehas/libexec/dev-login` as the tehas user, which is the one thing
the group's `sudo` rule allows. `dev-login` takes the clone's name from the
account `sudo` was asked by, loads the tehas user's environment and starts the
session above. Any other command typed there is refused with exit 64.

It is a copy of `host/login/tehas`, and only root installs it, so an update of
the platform can leave those accounts typing an older command. No tehas command
replaces it: `install.sh --check` reports the difference as `Login: NOT CURRENT`
and a real install prints `later: sudo ~/tehas/platform/bootstrap.sh dev boss`. Run that command
again after an update that changed `host/login/tehas`.

| Exit | Meaning                                                                                   |
| ---- | ----------------------------------------------------------------------------------------- |
| any  | The session's own exit status                                                             |
| 64   | A wrong argument, or the account has no `user.name` and `user.email`; nothing was started |
| 78   | The host was not set up for it: run `sudo ./bootstrap.sh dev`                             |

## tehas index

Builds the code index that a cycle's agents search, or refreshes it.

```
tehas index                  # build the index
```

pi-rukas tells its agents to find code with the `codebase_memory` tools. They
answer from an index, and nothing builds one on its own: interactive pi-rukas
does it in `/start`, which a headless cycle never runs. Without an index every
such search fails and the agents fall back to reading files.

The index is kept in `~/tehas/state/index`, the host's own directory, under
the name of the tree it was made from. No cycle, planning or manual container
is given that directory.
`tehas work` and `tehas plan` copy it into the cache directory of the container
they start, so every cycle reads the host's index and no cycle changes it for
the others. This command runs the indexer in the sandbox image, over a clone
mounted at the path a cycle works in, so what it writes is the index a cycle
looks for. The clone is made at the tip the product checkout last fetched and
is removed afterwards. The cycle's workspace is not touched.

It refuses, with exit 3, while a cycle is running or while a poll or another
index run holds the dispatcher's lock. The index is only ever written when
nothing is reading it.

| Exit | Meaning                                                |
| ---- | ------------------------------------------------------ |
| 0    | The index was built, or was already current            |
| 1    | The sandbox image is missing, or the indexer failed or did not finish in 30 minutes |
| 3    | A cycle is running, or a poll or another index run holds the dispatcher's lock; nothing was done |
| 64   | An argument this command does not know                 |
| 78   | The instance settings are invalid, or `tehas.slice` cannot be used; nothing was done |

The container gets the clone, read-only, and `~/tehas/state/index`, which is
the only thing it may write: no network, no token and none of the settings. It
runs in `tehas.slice` with a cycle's limits. The indexer's answer is kept in
`~/tehas/logs/index-<time>.log`, and `~/tehas/state/index.json` records the
commit that was indexed.

A cycle is given a copy, not the directory: the databases and the indexer's own
configuration, not its logs and not the staging file of an index run that was
stopped, which is about half again the size of the database it was writing.
`tehas index` removes what earlier runs left when it writes a new index, so the
copy does not grow with the number of runs. For this repository that is a few
megabytes and costs about a second per cycle. A cycle on a host that has never
run this command gets nothing and works as it did before an index existed: its
agents search by reading files, which is slower and finds less.

`install.sh` runs it once, the first time it finds the sandbox image and the
product checkout and no index. After that the dispatcher runs the script with
`--before-cycle` when it starts cycles and no cycle is running: the lock is the
dispatcher's own then, and nothing is done when `~/tehas/state/index` already
holds what the record says is the branch's tip. An empty directory counts as
no index, so a host upgraded from the version that kept the index in a cache
volume builds it into the new one at its next idle start rather than reporting
an index that no cycle is given. Cycles started then begin with an index of
the tree they are cloned from. A cycle started while another runs gets the
index as it is, and the dispatcher's log says `the code index stays as it is:
a cycle is running on #<issue>`. On a host that is never idle the index falls
behind the branch. What a cycle changes, in its workspace or its worktrees, is
never in the index.

The indexer may run for 30 minutes. The dispatcher's lock is held all that
time, so a hung indexer is stopped there and the poll goes on. A container
named `tehas-index` that a killed run left behind is removed before each run.
When the indexer fails, `~/tehas/state/index.json` stays as it was and
`~/tehas/state/index.failed` holds the tip that failed. Before a cycle that tip
is not tried again: the command exits 1 and the dispatcher's log says the index
was not refreshed, until the branch moves.

Run by hand it always rebuilds, a failed tip too, and it holds the dispatcher's
lock while it does, so no cycle starts in the middle. A cycle started by hand
with `tehas work` refreshes nothing.

## tehas labels

```
tehas labels        # one line per label
```

Creates the ten labels tehas uses in the product repository: `tehas:ready`,
`tehas:urgent`, `tehas:running`, `tehas:awaiting-merge`, `tehas:shipped`, `tehas:pause`,
`needs-human-attention`, `tehas:question`, `tehas:plan` and `tehas:planning`.
Run it once for a new product, and again after pulling a version that adds a
label. Run again, it puts their colours and descriptions back. It needs `TEHAS_REPO` and no other
setting. Exit 0 when every label is there, 1 when one could not be made, 64 on
any argument, 78 when the repository is not set.

## tehas pause

```
tehas pause              # pause this host
tehas pause waiting for the gate   # and say why
```

Writes the pause flag, `~/tehas/state/pause.json`, holding who set it, when and
why, in a file with mode 600. While it is there the dispatcher starts nothing:
no cycle, no planning turn, no answer to a planning question, no retry by a
tick. A cycle already running finishes, and the poll still stops one whose issue
is closed or whose pull request is merged or closed, and still clears an issue
marked `tehas:running` with no cycle behind it. The flag is read without asking
GitHub anything, so a host can be paused when GitHub cannot be reached. Pausing a
paused host succeeds and keeps the first flag. The reason is one argument, kept
to 200 characters, and it reaches pages that may be public.

Exit 0 when the host is paused, 64 on more than one argument, 78 when the
instance settings are missing or malformed, which writes nothing, 1 when the flag
could not be written, a directory in its way among them.

## tehas resume

```
tehas resume     # remove this host's pause flag
```

Removes the flag, and succeeds when there is none. It then reports the other
switch: `tehas:pause` on the control issue. Only an account in
`TEHAS_ALLOWED_LABELERS` lifts that label, so when it is gone and its most recent
`tehas:pause` event is a removal by anyone else, or is an application no event
took back, this says so and that the factory stays paused. It never changes a
label.

Exit 0 when the flag is gone, whatever the label says, 64 on any argument, 78
when the instance settings are missing or malformed.

## tehas status

```
tehas status
```

```
factory   pass: Idle, taking work. (40 s ago)
queue     2 in line, 0 building, 1 in review, 0 stopped (40 s ago)
cycle     #12 handoff at step review, 3 min ago
slice     room for 2 cycles (4 GiB of 20 GiB promised, reserve 2g, 40 s ago)
platform  cb6912a, 19 commits behind origin/main since 2026-10-13, fetched 20 min ago
```

Prints the state of the factory from the [health file](#tehasbot-health): is
it taking work, what is in the queue and how old that snapshot is, how far
the platform checkout is behind origin (on a host that has one), the current or last cycle, and what the
dispatcher's last poll read of `tehas.slice`. The `slice` line reads
`holding: ...` when that poll held cycles back, and `unknown: no poll has
reported in 10 min` when no poll read the slice in that time. While the factory
is paused it adds one line for each switch that is set:

```
pause     flag, written 4 min ago: waiting for the gate
pause     label: tehas:pause on the control issue
pause     label: tehas:pause is off the control issue, but no account in TEHAS_ALLOWED_LABELERS took it off
```

It reads
that one file and nothing else: no GitHub call, no instance settings, and it
writes nothing. So it is as fresh as the dispatcher's last poll, and the first
line says how old that is.

A stopped dispatcher cannot report itself: its file just stops changing. A
file older than ten minutes is therefore not believed. The first line then
starts `factory   stale:` and asks whether the timer is running.

Exit 0 on `pass` and `warn`. Exit 1 on `fail`, on a stale file, or when there
is no health file yet. Exit 64 on any argument.

## tehas top

```
tehas top                # one screen, redrawn every 2 seconds
tehas top --once         # one frame as plain text
tehas top --json         # the snapshot a frame is drawn from
tehas top --every 10     # redraw every 10 seconds
```

One screen for a person, in the manner of `top`: is the factory taking work,
what is in the queue and which issues are behind the counts, what the cycles are
doing, which units run, what the model mesh draws, and the tail of a cycle's
console. It draws again every 2 seconds unless `--every` says otherwise; `q` or
Ctrl-C ends it and leaves the terminal as it was. Without a terminal on standard
output, and with `--once`, it prints one frame as plain text: no escape codes.

It reads what the factory already writes under `~/tehas/live/` - the [health
file](#tehasbot-health), the queue snapshot, the cycles, the energy sample and
the console of a cycle - asks `systemctl --user` about its units, and reads the
host's own numbers from `/proc`. No GitHub call, no model call, no Docker call,
no instance settings, and nothing is written. What it shows is as old as those
files, and every section that has an age prints it. A file older than ten
minutes is not believed, the same rule `tehas status` uses.

The frame is drawn from one snapshot and from nothing else. `--json` prints that
snapshot: `generatedAt`, one key per section (`factory`, `queue`, `cycle`,
`host`, `units`, `energy`, `console`) and `reading`, the raw reading of `/proc`
the host section was computed from. A section whose source is missing,
unreadable or too old to believe is `null`; the frame says so in one line and
draws the rest. The factory line and the queue counts are the ones `tehas status`
prints: both commands take them from `lib/status.sh`, so they cannot drift apart.
A section added to the snapshot appears in `--json` without anything else
changing.

The host section compares two readings of `/proc`: the machine's cpu count, its
three load averages, what share of its cpus and of its memory is in use between
them, and up to fifteen processes with their pid, their short name from
`/proc/<pid>/stat`, their share of a cpu, their share of memory and how long they
have been running. A view that redraws hands the reading of one frame to the
next, so a redraw measures the seconds since that frame instead of waiting; only
a first frame, and `--json`, `--once` and `tehasbot top`, take a second reading a
second after the first. The reading a snapshot was computed from is in it under
`reading`, which is how it is handed on; no frame draws it and the published file
does not hold it. A process is named by that short name only: the command line,
and everything else under `/proc/<pid>/`, is never opened. A pid that was reused
between the two readings is not shown. The figures are what changed between the
readings, not the average since a process began. They sit between the cycle rows
and the units; when the terminal is short the process rows go first, before the
console lines.

A frame follows the width and height of the terminal, also after a resize. Long
lines are cut; nothing wraps and nothing scrolls. When the terminal is short,
host process rows are dropped first, then console lines, then unit rows, then
queue rows. Text that came out of a file has its control characters removed
before it reaches the terminal. `COLUMNS` and `LINES` say what the size is when
the terminal cannot be asked.

Exit 0 when a frame was drawn or a snapshot printed, whatever the factory says:
a `fail` or a stale health file is shown here, not returned - that is what
`tehas status` is for. Exit 1 when there is no `~/tehas/live/` directory at all.
Exit 64 on an unknown argument, or on a `--every` that is not a whole number
from 1 to 64.

## tehasbot

What the units run. Nothing here is needed in daily use; it is the place to
look when a timer or service has to be run once by hand.

```
tehasbot dispatch             # one poll; what tehas-dispatch.timer runs
tehasbot cycle <issue>        # one cycle and its report; what tehas-cycle@<issue>.service runs
tehasbot status               # write the queue snapshot
tehasbot health               # write the health file
tehasbot energy [--every]     # sample the model mesh; what tehas-energy.service runs
tehasbot top [--every]        # publish the snapshot of tehas top; what tehas-top.service runs
tehasbot protolab             # build the Protolab; what tehas-protolab.timer runs
tehasbot preview              # keep the previews; what tehas-preview.timer runs
tehasbot preview --sweep      # the second check; what tehas-preview-sweep.timer runs
tehasbot improve              # one run of the improvement loop; what tehas-improve.timer runs
```

The units are `tehas-dispatch.timer`, `tehas-dispatch.path` (a poll as soon
as a cycle ends), `tehas-energy.service`, `tehas-top.service`,
`tehas-protolab.timer`, the previews' `tehas-preview.timer` and
`tehas-preview-sweep.timer`, the improvement loop's `tehas-improve.timer`, and one `tehas-cycle@<issue>.service` per running
cycle. A cycle's container is `tehas-work-<issue>`.

## tehasbot cycle

One cycle for one issue, and its report. The dispatcher starts it as a unit;
it is not typed in daily use.

```
systemctl --user status 'tehas-cycle@*'    # the cycles that are running
systemctl --user stop tehas-cycle@12       # end the cycle for issue 12
```

It runs [tehas work](#tehas-work) for the issue, reads the runner's result
that `tehas work` left in `~/tehas/state/cycles/<issue>.result.json` (status,
the step it stopped in, cap, branch, token usage), finds the pull request the
bot opened, and relabels and
comments: merged, `tehas:awaiting-merge`, or `needs-human-attention`. A cycle
that stopped gets its comment with two boxes and the label `tehas:question`,
and the comment's id goes into `~/tehas/state/park-<issue>.id`, which is what
lets the dispatcher take a tick there. It
then removes the workspace of a cycle that merged or left a pull request, and
keeps the workspace of one that stopped. The images `tehas work` fetched for
the cycle are removed in every case. A runner that started no cycle over an
image (exit 4) is parked with a comment that says so. A retry moves a stopped workspace to
`<name>-<issue>.before-retry`, which is removed and kept by the same rule.
The dispatcher removes a kept workspace once no cycle has run on its issue for
7 days. A
stopped unit stops the container and is reported as a cycle that ended without
a pull request. The report of a stopped cycle tries each call to GitHub once,
cuts it after 20 seconds and does not refresh the status, so it ends within
the unit's `TimeoutStopSec`. A hangup is taken as a stop too. It checks no authorisation:
the dispatcher did that before it started the unit. It does check the number:
it writes `state/cycles/<issue>.taken` once it holds its lock, then asks GitHub
what the number is before a label is changed, a comment is posted or a
container starts, tried three times like its other calls, and refuses a closed
issue and a pull request (exit 65) with one line in the log. A number GitHub
could not answer for at all is refused the same way, with a line that says
that. Because the marker was written first, the poll that started the unit
takes such a cycle as one that came up: the issue keeps `tehas:running`, and
the next poll clears it in silence.

`tehas:awaiting-merge` is given only to a run that ended as `handoff`, with the
runner's exit 10 or 11, has an open pull request of the bot that names the
issue, and stopped on the cap `awaiting-human-merge`: only the merge is left to
a person. Even then it is not given when a stable `cap-hit` line of the
cycle's progress ended the review on `round-cap` and sent it on to `ci` (the
review ran out of rounds with findings open), when the progress cannot be
read, or when the issue carries `needs-human-attention` at the end, as GitHub
answers, or GitHub does not answer. Any other end with a pull request open gets
`needs-human-attention` only, and the comment names the cap and the pull
request. The cap is the result's `cap`, from the vocabulary pi-rukas publishes
in `--capabilities`.

The cycle posts pi-rukas's handoff: the runner is given
`PI_ENSEMBLE_HANDOFF_CHANNEL=host`, so it posts none, and its result carries
the body. It goes into the park comment on the issue, or with the token usage
on the pull request for a merge that waits. It is cleaned like any agent's
words (boxes, markers and @-mentions are escaped) and headed by pi-rukas's
marker `<!-- pi-rukas:handoff issue=N cap=CAP -->`, made from the cap.

An issue that carries `needs-human-attention` when its cycle starts is run with
`--restart`: the start is a person's answer to the handoff. pi-rukas lets that
cycle past its check of the label and takes the label off. Neither the cycle
nor the dispatcher removes it.

The pull request is looked up in every state. One that a person merged while
the cycle ran ends the cycle as merged, whatever the result says: the
issue only loses `tehas:running`, the comment says the pull request was merged
while the cycle ran, nothing is parked, and [tehasbot status](#tehasbot-status-and-the-console)
labels the closed issue `tehas:shipped`. One that a person closed without
merging gets `needs-human-attention` and a plain comment that says so, without
the retry boxes: to try again, apply `tehas:ready`.

What the result says is checked against GitHub. A pull request counts as the
cycle's only when the bot opened it on the branch the result names and it
names the issue: its head is
`feature/issue-<n>-...`, as pi-rukas names a branch, or its body closes `#<n>`
(`Fixes #<n>`, `Closes #<n>`). A pull request on that branch that names
another issue is said in the comment and not taken. A result of `merged` is
reported as merged only when the bot has a merged pull request for the issue
on that branch. Without one the issue is parked, the comment says that the
bot has no merged pull request for the issue, and both workspaces are kept. A
result that is a link, of another schema than 3, or with no entry for the
issue, is not read: the status is then `unknown`. pi-rukas's state file in
the workspace is never read.

The label change that ends a cycle is tried three times, five seconds apart,
and so is the search for the pull request. If GitHub still refuses the label
change, the log says `WARN: label change on #N failed`, the cycle's comment
says that the label could not be changed, and the issue stays `tehas:running`
until the next poll of [tehas dispatch](#tehas-dispatch) finds it. A search
for the pull request that GitHub refuses three times is not taken for a cycle
that left none: the issue is parked with a comment that says the pull request
could not be looked up and names the branch, and the branch stays in
`~/tehas/state/cycles/<issue>.branch` until the issue's next cycle starts.

A parked cycle's cap, from the result, goes into
`~/tehas/state/cycles/<issue>.park` with the failure before it (the reason
`tehas work` wrote to the progress file), and into the
record for the improvement loop as `cap` and `capReason`, reduced to plain
characters. Where the result gives a failure of another kind than `cap`, the
park comment names its kind, role, cause and message. The record also holds `buildCommit`, the pi-rukas commit the
result says the cycle ran; the improvement loop checks a fix against it. A
provider outage is said as one: `Tehas cycle parked by a provider outage`,
with the cap and the reason. That is a result whose `failure.kind` is
`provider`. For an image whose result has no `failure` (v0.12.80) it is
`step-failed:<step>` or `adversarial-infra-failure` whose reason names the
provider, a timeout, a connection or a 5xx status. The
park file then asks [tehas dispatch](#tehas-dispatch) for one retry, unless the
cycle was that retry.

| Exit | Meaning                                              |
| ---- | ---------------------------------------------------- |
| 0    | The cycle ran and was reported, whatever its outcome |
| 3    | A cycle for this issue is already running            |
| 64   | The argument is not an issue number                  |
| 65   | The number is not work the factory takes: a pull request, a closed issue, or an answer GitHub could not give after three tries. Nothing was changed and no container started. The cycle still claimed its place, so a poll that started it does not report a failed start |
| 78   | The instance settings are invalid                    |

## tehasbot status and the console

Feed the [factory floor](/) and the [plain dashboard](/dashboard). `tehasbot status` writes the queue snapshot to
`~/tehas/live/status.json`; the dispatcher runs it at every poll, and a cycle when it ends.
Completed means closed by a merged pull request, whatever labels the issue was
left with. `tehasbot status` gives each such issue `tehas:shipped`, so GitHub
lists the same issues; run `tehas labels` once so the label exists.

`tehasbot status` also writes `~/tehas/live/platform.json`: the commit the
platform checkout `~/tehas/platform` is at, `upstream` (origin's default
branch), how many commits it is `behind`, `behindSince` (when the oldest of
those landed on that branch) and `fetchedAt`. It fetches the checkout at most
once an hour, with a limit of 60 seconds; the time of the last try is in
`~/tehas/state/platform-fetch`. A fetch that fails is no error and is tried
again an hour later. Nothing else in the checkout changes: `tehas update`
does that. A host with no checkout there has no file.

`~/tehas/libexec/console` is started by the work
script and follows the cycle's agents into `~/tehas/live/console-<issue>.txt`, masking
tokens, keys, passwords (also in a URL and after `Bearer`) and private network
addresses, and writing no control character but the line break and the tab.
Invisible format characters, direction overrides among them, are dropped
before the mask looks, so they do not hide a token from it; any that remain
and the Unicode line and paragraph separators are written as U+FFFD. An agent
is named by its role. The mask catches accidents;
it cannot stop an agent that sets out to print a secret another way, so treat
the console of a published dashboard as public. A log line of over 1 MiB is
left out, and the console says how many were. So is a line not yet ended
when the files together already hold 16 MiB of such lines, and a transcript
that is removed is forgotten. A console file removed while the cycle runs is made
again by the console at its next pass. Neither needs running by hand.
What the console itself complains about goes to `~/tehas/logs/work-<issue>-<time>.jsonl.console`,
and the work script says so when the console is missing or ends with an error.
At the end of a cycle the console is given 30 seconds to stop; one that takes
longer is killed, and the work script says so.
The work script's own status line, in `~/tehas/live/cycles/<issue>.json` and from there in `~/tehas/live/running.json`, goes through
the same mask (`console --mask`); without a console it is left empty.

The console, and the mask of the status line, also mask in any case the names
the host knows that an agent can print: each name and address in
`PI_ENSEMBLE_HOST_ALIASES`, the host of the model provider's `baseUrl` in
`models.json`, and the bot's git email. The work script gives them with
`--private`; a name shorter than 4 characters is left out, as it would mask
ordinary words. They are names, not secrets: a token is never on a command
line.

The snapshot also carries the words of the breakroom game, Buzzword Buster.
They are counted over the titles and labels of every issue of the repository,
the newest thousand, not only the ones the factory was given: work going on
beside the factory shows up there. The file holds the hundred most used words
with their counts, and no title or issue number. On a published dashboard
those words are public too. If GitHub does not answer that call, the snapshot
is written without them and the game counts the tickets on the line. A round
shows at most twelve of the forty most used words, picked by chance, and writes
them on up to twenty-four tetrominoes: a wall no wider than sixteen cells and no
taller than six, its cells solid from the top down with no holes in them, so it
is another one every time.

## tehasbot health

```
tehasbot health     # prints nothing; refreshes ~/tehas/live/health.json
curl -i http://tehas-tehas:8080/health
```

Writes the factory's health from what is already on disk: the queue snapshot,
the running cycles, the last one that finished, what the dispatcher's
last poll wrote about the slice, and this host's pause flag. The status script
runs it on exit, so it is
refreshed at every poll and at the end of every cycle. The docs server
serves the file at `/health` and `/healthz` as `application/health+json`.
`tehas status` prints the same file for a person.

`checks.pause` names the switches: `{flag: {at, reason}, label, lifted}`, with
`flag` null when no flag is set, `label` whether `tehas:pause` is on the control
issue, and `lifted` whether the label is off with no account in
`TEHAS_ALLOWED_LABELERS` having taken it off, which holds the host paused. It
carries no `by`: the account that wrote the flag stays on the host, because this
file is served.

| `status` | Meaning                                                                                  | HTTP |
| -------- | ---------------------------------------------------------------------------------------- | ---- |
| `pass`   | Taking work, idle or in a cycle                                                          | 200  |
| `warn`   | The queue snapshot is more than ten minutes old: GitHub did not answer the last polls, and `output` says how old. Or paused (the pause flag on this host, `tehas:pause` on the control issue, or a pause no allowed account took off), or the dispatcher holds everything because the slice is missing or cannot be read, or there are no resources (the model endpoint does not answer or does not offer the model), or too low on disk to build previews, or the free space could not be read, or the platform checkout has been behind origin's default branch for more than a day (`output` says how many commits; run `tehas update`) | 200  |
| `fail`   | The last cycle ended as anything but `merged` or `handoff` and no cycle has started since, or there is no queue snapshot | 503  |

`output` says the same in a sentence. `checks.queue` has the snapshot time and
the counts in line, building, in review and stopped. `checks.cycles` has the
issue, status, step and times of every running cycle; `checks.cycle` is the
first of them, or the cycle that finished last when none is running.

`fail` is about the cycle that finished most recently. It lasts until a cycle
starts after that one ended. A cycle that was already running when the other
one failed does not clear it. A cycle that has written nothing for two
minutes and whose issue's lock (`state/cycles/<issue>.lock`) nobody holds is
recorded as ended with the status `lost`, which is a failure. While
`tehasbot cycle` still holds the lock, the cycle stays running.

`checks.platform` is `live/platform.json`, or `null` on a host without a
platform checkout.

`checks.failed` lists the cycles among the last ten that ended, newest first,
that ended as anything but `merged` or `handoff`: `issue`, `status`, `step`,
`startedAt` and `time`. It does not change the status. An abort that a merge
overtook ten seconds later leaves the status `pass`, and is still in
`checks.failed` until ten more cycles have ended. A monitor that polls less
often than cycles end reads it to see every failure. The ten endings are kept
in `live/endings.json`.

`checks.disk` has the free and the required bytes and `low`, as `tehasbot
preview` last wrote them to `live/previews.json`. It is `null` on a host
without previews, and when that figure is more than half an hour old. When
`low` is true, `output` is "The host is low on disk: previews are not being
built." with a figure, and "The host's free disk space could not be read:
previews are not being built." when `freeBytes` is `null`.

`checks.slice` is `live/dispatch.json` as the dispatcher's last poll wrote
it: `time`, `why`, `hold`, `count`, `room`, `committed`, `max` and `reserve`.
`why` is empty when cycles may start, else one word: `slice`, `unreadable`,
`resources`, `count`, `awaiting`, `pressure`, `room` or `full`, and
`hold` is the line the dispatcher logged; with `unreadable` it names the file
and what was missing in it. `resources` is a model endpoint that does not
answer or does not offer the configured model, or, with `TEHAS_WORK_IMAGES=1`,
a model or sandbox image that cannot take the images. `awaiting` and `room`
are also given, with cycles starting, when the awaiting-merge limit or the
slice's room let fewer start than there are free slots and issues waiting;
`hold` then starts `holding: <n> of <free> free slots start:`. `full` is a slice that holds as
many containers as their ceilings fit. `room` is how many more reserves fit in the slice;
`committed` and `max` are bytes. With `slice` or `unreadable` the status is
`warn` and `output` is the hold. With `resources` the status is `warn` and
`output` starts with `No resources:`. It is `null` when the file is more than ten
minutes old. `tehasbot health` calls neither `systemctl` nor `docker`.

A stopped dispatcher cannot report itself: the file simply stops changing and
the endpoint keeps answering with the last state. A monitor should also treat
a `time` older than about ten minutes as down.

## tehasbot energy

Optional. It feeds the Energy use section of the factory floor and nothing
else: no command reads its file, and a factory without it dispatches and works
the same. It is useful only when the model backend is a
[viiwork](https://github.com/janit/viiwork) mesh. The
command and its unit are installed on every instance, but the service runs
only if you enable it.

```
tehasbot energy            # one sample; prints nothing; writes ~/tehas/live/energy.json
tehasbot energy --every    # one sample at every 15-second mark; what the service runs
systemctl --user enable --now tehas-energy.service
```

Asks the model mesh for its cluster snapshot and keeps what the Energy use
section shows: per machine the draw, kWh and RAM; per model the slots, how
many are busy and what serves it; and the last 30 minutes of draw, with the
mesh's running count of tokens for the factory's models where it reports one. Only machines that are alive
and serve a model the factory uses are kept, and only those models. The
factory's models are the ones named by `PI_ENSEMBLE_SUBAGENT_MODEL` and
`PI_ENSEMBLE_MODEL_*`; with none set, the file has no machines. It works only against a viiwork
mesh, which it recognises by the answer.

| The mesh answers                       | The file                                   |
| -------------------------------------- | ------------------------------------------ |
| A cluster snapshot                     | Rewritten                                  |
| Nothing, a timeout, a server error or a busy answer (408, 429) | Left as it is; the page calls it stale |
| Anything else                          | Removed, and the section with it           |

The mesh is the one the factory's model provider points at. Set
`TEHAS_ENERGY_URL` in `~/.config/tehas/env` to ask another cluster URL, and
`TEHAS_ENERGY=off` to remove the file. The file is readable by anyone who can
open the dashboard and names the machines, their draw and their models. It
also holds the version of the mesh software and, for each model, the name and
the version of the engine behind it. Addresses, node ids and GPUs are not in it.

Each entry of `models` carries an `engines` array: one element per distinct
set of values among the machines that serve the model, sorted.

```json
{ "name": "model-small", "hosts": ["node-a", "node-b"],
  "engines": [{ "version": "2.9.2", "engine": "llamacpp",
                "engineName": "llama.cpp", "engineVersion": "b11371" }],
  "slots": 6, "busy": 2, "queued": 0 }
```

| Field           | What it is                                                              |
| --------------- | ----------------------------------------------------------------------- |
| `version`       | The mesh's version on that machine, without its leading `v`             |
| `engine`        | The mesh's own id for the engine                                        |
| `engineName`    | The engine's name for people. Reported from viiwork v2.9.2              |
| `engineVersion` | The engine's own version on that machine. Reported from viiwork v2.9.2, and not by every engine |

A value the machine does not report is `null`. `engineVersion` is also `null`
when it is not a plain version: only letters, digits and `. + _ -`, at most 40
characters, are copied. An element with no mesh version and no engine is left
out. A mesh in the middle of an upgrade, or a model served by two engines,
gives more than one element.

The model table shows them in its `ENGINES` column: the engine's name, or its
id where the mesh gives no name, then its version where there is one. So
`viiwork 2.9.2 (llama.cpp b11371)` from a machine on v2.9.2 and
`viiwork 2.9.0 (llamacpp)` from an older one. A model with no element shows a
dash.

The file's `source` and `sourceUrl` name the mesh software and its project. The
section ends with a line that says so, `DATA SOURCE: viiwork`. The name is a
link when `sourceUrl` is an `https://` address.

The service reads its settings when it starts: run
`systemctl --user restart tehas-energy.service` after changing either one.
Stopping the service does not hide the section; the last file stays and the
page calls it stale. To hide it, set `TEHAS_ENERGY=off` and restart, or stop the
service and delete `~/tehas/live/energy.json`.

Against a backend that is not viiwork the service still asks
`<provider>/v1/cluster` every 15 seconds and writes nothing. On such a host
leave the service disabled, or set `TEHAS_ENERGY=off`.

## tehasbot top

Optional. It publishes what `tehas top` draws, so that the factory floor and the
plain dashboard can show the same numbers a person sees on the host. Nothing on
the host reads its file.

```
tehasbot top                 # one snapshot; prints nothing; writes ~/tehas/live/top.json
tehasbot top --every         # one snapshot at every 5-second mark; what the service runs
systemctl --user enable --now tehas-top.service
```

It builds the snapshot with the same `lib/top.sh` the terminal view uses, and
writes it as one line of JSON holding the keys the pages draw and nothing else:
`generatedAt`, `factory`, `queue`, `cycle`, `host`, `units` and `energy`. The
list is `tehas_top_publish_keys` in `lib/top.sh`. A key added to the snapshot
later is not in the file until someone puts it on that list, which is how the
console of a cycle and the raw reading a host section was computed from stay
out. The command reads the instance settings, so a setting that is missing or
malformed stops it with 78;
with `--every` the same 78 ends the loop, and systemd shows the failing service.
It writes the file only when `TEHAS_TOP` is `1`; with `0`, which is the default,
it removes the file and exits 0. It writes to `top.json.new` and moves it into
place, so a reader never sees half a file, and a snapshot it cannot write leaves
the one that is there in place.

`TEHAS_TOP` is set in the instance directory. The unit is installed on every
instance and starts only when you enable it; nothing enables it for you. The
service reads its settings when it starts, so run `systemctl --user restart
tehas-top.service` after changing the setting. Stopping the service does not hide
the section: the last file stays until the page calls it stale, and the way to
remove it is `TEHAS_TOP=0` with one run of `tehasbot top`, or deleting the file.

The file is readable by anyone who can open the dashboard. It names the host's
cpu count, load averages, memory, its units, and up to fifteen processes by pid
and short name. It holds no command line, no path outside `/proc`, no token and
nothing about the sandbox: a cycle's container is one process among those rows,
and the console of a cycle is not in it.

The factory floor reads it every 5 seconds, the plain dashboard on its own
10-second refresh. Neither asks while the page is hidden, and both show no host
section where the file is not there: a host that publishes nothing is asked again
once a minute rather than at every period. Both pages work out what the file
means with one module, `host/fun/host-data.js` - the figures, the words for how
old a reading is, and the rule for when to ask again - so the panel and the plain
list cannot say different things about the same file. The panel gives the
readings, then the processes, then the units as `tehas top` lists them: name,
whether systemd has it installed, its state, and when a timer next fires; the
plain dashboard gives the same as a list. A snapshot older than 30 seconds is
called stale rather than live.

## tehasbot protolab

```
tehasbot protolab       # one pass; a line per ticket built or removed
systemctl --user enable --now tehas-protolab.timer
. ~/tehas/env.sh && docker compose -f ~/tehas/edge/router.compose.yaml up -d
```

Builds the Protolab in `~/tehas/protolab`: a page for every open issue
labelled `tehas:prototype` by an account in `TEHAS_ALLOWED_LABELERS`, with the
files attached to it, and the list
`~/tehas/live/protolab.json` that the dashboard links from. The timer runs it
every five minutes; the log is `~/tehas/logs/protolab.log`.

The container is the edge router, on port 8081. It answers for the Protolab
and for the [previews](#tehasbot-preview), and picks by the name it is asked
for: `lab-<issue>.` is that ticket's page, `uat-<pr>.` is the preview of that
pull request, and any other name is the Protolab by path, the list at `/` and
a ticket at `/<issue>/`. It reads its mounts, has a read-only root and no way
to write to the host. With `TEHAS_DOMAIN` set, the list links each ticket at
`lab-<issue>.<domain>`, so every ticket is an origin of its own to a browser.
[The host](/host) has the table of what it answers.

A ticket is at the prototype stage only while its most recent `tehas:prototype`
labelling is by an account in `TEHAS_ALLOWED_LABELERS`, read the same way the
dispatcher reads `tehas:ready`. A label applied by anyone else, the bot
included, counts as no label at all: the ticket gets no page, a page it had is
removed, and the log names the issue and the account. Where a ticket's events
cannot be read the pass builds nothing for that ticket and removes nothing: a
page it had stays as it was. The pass reads its own login before it reads,
builds or removes anything of a ticket, and stops with exit 78 when
`TEHAS_ALLOWED_LABELERS` names it, as the dispatcher does. When GitHub does not
answer that question, the pass ends there with the same nothing built and
nothing removed.

It fetches only links to GitHub's attachment storage, in text written by an
owner, member or collaborator of the repository, and only the types listed
under [Operating](/operating). A ticket gets 10 megabytes and 40 files. An
issue that has not changed since the last pass is not built again, so a quiet
pass fetches no files: one GitHub call for its own account, one for the list,
and one for every ticket at the stage, its label events. When GitHub cannot be
reached, what is there stays.

A prototype is somebody's HTML and script. That is why the Protolab is a
server of its own on another port, and why every attached file is answered
with a sandbox policy: the browser gives it an origin with nothing in it. To
publish the Protolab outside the private network, give it a host name of its
own, never a path under the dashboard's, and set `TEHAS_PROTOLAB_URL`.

Without the timer and the container there is no Protolab, and the panel on the
factory floor says so.

## tehasbot improve

```
tehasbot improve        # one run; one line on standard output
systemctl --user enable --now tehas-improve.timer
```

One run of the improvement loop. It is off, and prints that it is, unless
`TEHAS_IMPROVE_REPO` is set. [Operating](/operating) says what it files and how
to read its line; the timer runs it every three hours, on the hour, and the log is
`~/tehas/logs/improve.log`.

When a cycle ends, `tehasbot cycle` leaves a record of it in
`~/tehas/tmp/improve/window/<issue>-<time>/`: how it ended, the stream without
its word by word updates, the console and the error output. Its `cycle.json`
names the image the cycle's container started from (`image`, its id) and the
pi-rukas commit of that image's label (`rukas`), which `tehas work` reads as
the container starts and leaves in `~/tehas/state/cycles/<issue>.image`. A
rebuild while the cycle runs does not change them. Without that file the
commit is the one the cycle noted at its start. A comment on an issue the loop
filed, when the issue is worked on again, names both. When the cycle was given the issue's images or files, the record also holds
what `tehas work` kept of them in `~/tehas/state/cycles/`: `manifest.json`,
and `images.md` and `retry-context.md` when the cycle had them. Only this
cycle's, only plain files, at most 256 KB each, and never an image or a file
itself. A run fetches the open
issues and the ones closed in the last 30 days, takes the five oldest records
that wait, and starts the analyst. The rest stay for the next run. Then it
checks the one file the analyst leaves, files what passes and removes the
records it took.

The analyst has 60 minutes, and the prompt tells it so and asks it to write
`findings.json` early and again as it reads more. When it is stopped there
and the file passes the check, what the file holds is filed: the records that
a finding or a followup names are done, the others go back to the window. With
no file, or one that does not pass, every record goes back and the run exits 1.
Either way the next run takes half of what this one took, never below one; a
run that finishes in time doubles it again, up to five. The number is in
`~/tehas/state/improve.cap`.

The findings are about tehas and about pi-rukas, so the analyst is plain Pi
with one prompt (`~/tehas/lib/improve-prompt.md`) and no extension. What it
reads holds issue text and model output, which can be written to steer it, so:

| The analyst's container gets | It does not get |
| --- | --- |
| The records and the issue list, read-only | The bot's token, or any other variable of the host |
| One empty directory to write `findings.json` in | The product checkout, a workspace, Pi's sessions or transcripts |
| `models.json`, read-only, and the host aliases | pi-rukas, skills, context files, MCP servers |
| The tools read, grep, find, ls and write | A tool that runs a command or opens a connection |
| 1 CPU, 2 GiB, no swap, 1024 processes, 60 minutes, in `tehas.slice` | A cycle's limits: it is a batch job with a quota of its own, counted in the slice's room like any other container |

The host makes every GitHub call. A finding is filed only if it is the shape
asked for: a title of at most 100 characters, one of six categories, a body, an
excerpt as evidence, and cycles that were in this run. A title that matches an
open issue is a duplicate. In the issue the model's text has every mention
broken and its evidence inside a fence it cannot close. The label is
`tehas:improvement` and no other.

Limits: 3 issues a run; no run files in a repository while
`TEHAS_IMPROVE_MAX_OPEN` (10 by default) open issues there carry the label. The
whole log of each cycle a finding names is gzipped and uploaded to the release
`tehas-improve-logs`, in pieces of at most 5 MB (`ticket-<issue>-<time>.log.gz`,
or `.log.1.gz`, `.log.2.gz`). A log that holds something shaped like a token is
not uploaded.

Exit 0 when the run finished, also with an analyst stopped at its bound that
left a good file, there was nothing to read, the open issues are at their
limit already, or the loop is off.
Exit 1 when the analyst or GitHub failed: the records are kept and the next run
reads them again. What a failed analyst printed is in
`~/tehas/logs/improve-analyst-<stamp>.log`, one file per failed run, named by
the time the run took the records. The run's line names it. Exit 64 on any
argument, 78 on invalid settings, or when there are records to read and
`tehas.slice` cannot be used: the records are kept. A second run while one is going leaves it to
that one.

Each run also removes the per-cycle files and the kept analyst output in
`~/tehas/logs/`, and the records under `~/tehas/tmp/improve/`, older than seven
days, read or not.
## tehasbot preview

```
tehasbot preview            # one run; a line per preview that changed
tehasbot preview --sweep    # the second check of what is left on the host
systemctl --user enable --now tehas-preview.timer tehas-preview-sweep.timer
tail ~/tehas/logs/preview.log
```

Keeps a running preview of every open pull request whose head branch is in
the product repository and that was opened by an owner, member or
collaborator of it: a container `tehas-preview-<pr>` that the edge router
answers with at `uat-<pr>.<domain>`. Deterministic, no model. It needs
`TEHAS_PREVIEWS=1` and `TEHAS_DOMAIN`. Setting it up, and what a preview can
and cannot reach, is on [Previews](/edge).

The two timers are enabled by the operator, with the line above. `tehas update`
restarts only a unit that is already active, so on a host that was installed
before previews existed the timers are installed and do not run until they
are enabled. Both write to `~/tehas/logs/preview.log`.

**A run**, every five minutes:

1. It asks Docker whether it answers and for its version, and makes the
   network `tehas-previews` if it is not there. Then it checks what keeps a
   preview from the host: Docker is 27.1 or later, and the network is
   internal, a bridge, without IPv6 and made with the option that gives the
   host no address on it. If one is missing, the run stops here and the log
   says `the network tehas-previews does not keep the host out (<reason>);
   nothing was built`. [Previews](/edge) lists the reasons. A run that stops
   here still writes `~/tehas/live/previews.json`, with the free space. So
   does one that stops because Docker does not answer, with no figure for the
   free space and marked low, so `tehasbot health` warns. The time in that file is therefore the time of the last run,
   whatever the run could do.
2. It measures the free space: the smaller of the file system of `~/tehas`
   and the one Docker keeps its images on. This is done once Docker has
   answered, before the network is checked.
3. It asks GitHub for the open pull requests, all of them, in pages of 100,
   newest first. It stops when a page is short, which is where GitHub ends.
   When the tenth page comes back full it asks for one more: an empty answer
   means the list ended there, so a thousand open pull requests is a list it
   has read. Anything in that answer means it stopped before the end, and it
   builds and removes nothing: `the open pull requests are more than 1000: the
   list was not read, nothing was built or removed`. No answer, an answer with
   nothing in it, or one that is not a list ends the run the same way: nothing
   is built and nothing is removed. A list with no pull request in it is an
   answer, and every preview is then looked at one by one, as step 5 says. A
   page that does not say who opened one of the repository's own pull requests
   is not a clear answer either: `the list of open pull requests does not say
   who opened each; nothing was built or removed`. Each page is an answer of
   its own, taken at its own moment, so the list can move while it is read: a
   pull request that comes back on two pages counts once.
4. A pull request whose head is in another repository is counted in the log
   and never built: `<k> pull requests from elsewhere, not built`. Nor is
   one whose head is in the repository and that was opened by anyone but an
   owner, member or collaborator of it: `<k> pull requests opened by someone
   outside the repository, not built`. On a public repository anyone can
   open a pull request between two branches that are already there. Of the
   rest, the newest `TEHAS_PREVIEW_MAX` get a preview.
5. What is on the host for a pull request the list does not have is removed:
   the container, the image, the fetched commit, the state and the build log.
   Membership in that list is not enough on its own, because a pull request can
   fall between two pages and look closed. Before it removes anything the run
   asks GitHub about that pull request by itself, as `tehasbot preview --sweep`
   does, and what GitHub answers is what the log says: `#<pr>: pull request no
   longer open, removed` for one that is closed or not there at all, `#<pr>:
   its branch is not in <repo>, removed` and `#<pr>: it was not opened by
   someone of <repo>, removed` for one that is open but not built here. Nothing
   is removed where GitHub does not answer clearly: `#<pr>: no answer from
   GitHub, kept`, nor where it says the pull request is open and one of ours:
   `#<pr>: open though the list did not say so, kept`.
6. Each pull request that is due is fetched and built. The commit is fetched
   into the bare clone `~/tehas/preview/repo.git`, and its
   `preview/Dockerfile` is built with the commit as the context, in at most
   15 minutes. An image that declares a volume outside `/tmp`, `/config`
   and `/data` is not run: that is a failed build, and the image is removed.
   The new container replaces the old one. Five seconds after
   its start Docker is asked whether it still runs and was never restarted.
   One that is not is removed, with its last lines in the build log, and the
   state is `build-failed`. The old container went before the new one was
   started, so nothing serves for that pull request until a commit starts.
7. The pull request is told, at the top of its own description, and
   `~/tehas/live/previews.json` is written for the dashboard and for
   [tehasbot health](#tehasbot-health).

Each pull request has a state, in `~/tehas/preview/<pr>/state.json`:

| State | Meaning | Tried again |
| --- | --- | --- |
| `serving` | A container of that commit was started and was still running five seconds later, without a restart. It is not asked whether it answers | Not needed |
| `building` | The build is under way | |
| `no-recipe` | The commit has no `preview/Dockerfile`. A preview of an earlier commit is removed | At a new commit |
| `build-failed` | The build failed or ran over 15 minutes, or the recipe declares a volume outside `/tmp`, `/config` and `/data`: a preview of an earlier commit keeps serving. Or the container did not start, or it started and stopped within five seconds and was removed: for these two nothing serves for that pull request, not the earlier commit either, until a commit starts | A failed build: at a new commit, not for the same one. A container that did not start or did not stay up: at each of the next three runs, then at a new commit |
| `no-space` | Less than `TEHAS_PREVIEW_MIN_FREE` is free, or the free space could not be read. What serves stays, and removal still runs | At every run, until there is space |
| `over-limit` | More than `TEHAS_PREVIEW_MAX` pull requests are open and this is not one of the newest. A preview it had is removed | At every run |

A commit that could not be fetched gets no new state. The log says
`#<pr>: the commit could not be fetched, tried again at the next run`, and it
is. A new commit is always tried.

A build's output is in `~/tehas/logs/preview-<pr>.log`, written anew at each
build.

**What a preview's container is given.** Nothing of the host: no mount, no
variable, no published port, no token. A read-only root with `/tmp`, `/config`
and `/data` in memory, every capability dropped but the one to bind a port,
256 MB, one CPU, 128 processes, and the network `tehas-previews` only. That
network has the router and the other previews on it, and neither the internet
nor the host. What it prints is kept by Docker in at most two files of 10 MB
(`--log-opt max-size=10m --log-opt max-file=2`).
`test/preview_test.sh` pins these arguments.

It writes nothing else to the host's disk. A `VOLUME` line in a recipe would
have Docker make a volume there, which a read-only root does not stop and the
memory limit does not count. So an image that declares a volume anywhere but
`/tmp`, `/config` and `/data` is not run, and every container is removed
with `docker rm -f -v`, which takes a volume Docker made for it along.

**The notice in the description.** The preview is said at the top of the pull
request's own description, between the marker lines `<!-- tehas-preview -->`
and `<!-- /tehas-preview -->`: the address when the preview serves, or
"Unable to create a preview" with the reason for `build-failed` and
`no-space`. It is said once for each commit and state, so a full disk is not an
edit every five minutes. For `over-limit` and `no-recipe` a pull request whose
description has no notice gets none. One that has a notice from an earlier
state has it changed to "There is no preview" with the reason, so that it does
not go on giving an address that answers 404. `building` is on the dashboard
only.

**Which block belongs to the bot.** The one at the very top of the
description: its opening marker as the first line, and the closing marker under
it. It is the bot's only when everything between those two lines is one of the
notices this command writes: a line beginning `Preview of \``, `Unable to create
a preview` or `There is no preview`, and for a preview that was made or
attempted, the sentence `It is rebuilt when the branch changes and removed when
this pull request is closed.` after a blank line. A notice is written with a
blank line under its opening marker, so that it renders as text and not as part
of the HTML comment, and one is allowed there and nowhere else. All of it or
none of it. Text
between marker lines that this command did not write is not written over, and
neither is a marker line anywhere else in the description: the log says
`#<pr>: the description has a preview notice that is not this one, left alone`,
once for each commit and state. Write your own text outside the two marker
lines. To hand the block back after changing it, remove both marker lines: the
notice comes at the run that sees the change.
Everything after the closing marker line of a block of the bot's is kept as
GitHub gave it, blank lines at the end included.

**What a notice is proof of.** It is proof of what this host decided, read from
`~/tehas/live/previews.json`, the dashboard and `/health`. Anyone can write the
marker lines and the sentence under them, and for a pull request this host does
not build, one from a fork or one opened by someone outside the repository,
nothing reads the description at all. An address at the top of such a
description is the author's own text.

**One record for each commit and state.** What was said about a commit and state
is on record in `~/tehas/preview/<pr>/.notice`, as `<commit> <state> <what>
[attempts] [time]`: `none`, `said`, `refused`, `failed` with how many writes
GitHub did not take, or `gaveup`. The time is the one GitHub gives for that pull
request at the last look. Nothing is written or said twice about a commit and
state: a full disk is not an edit every five minutes, and the log carries one
line for each write GitHub did not take, the last of them saying it is left to
the next commit or state.

The record does not stop the reading. The description is read again whenever
GitHub gives a later time for the pull request than the one on record, which is
what editing a description does. A notice a person removes therefore comes back
at the next run, and so is a block someone wrote at the top in the shape of the
bot's: ownership of a description is only its text, so it is looked at again
whenever the text can have changed. A look that finds nothing to do costs one
read, writes nothing and says nothing. A record with no time on it is looked at
again too.

A call that
failed is tried at the next run: `#<pr>: the description could not be read (no
such pull request (HTTP 404)), tried again at the next run`, with
`GitHub refused the call (HTTP 403)` when GitHub refuses the token, and
`#<pr>: the answer about this pull request was not understood ...` when it
answered with something that is not that pull request's description. Three
writes GitHub did not take are said once, with the third of them, as
`#<pr>: the notice was not written three times, left to the next commit or
state`. A description that already says what
it should say is not written at all.

The description is asked for at the moment of the edit, with `gh api
repos/<repo>/pulls/<pr>`, and not from the list of open pull requests the run
was given minutes ago: an edit made before that read is kept, and so is what
GitHub says about the pull request itself. One merged or closed in between gets
nothing written, and the log says `#<pr>: the pull request is no longer open,
nothing was written`. GitHub takes no condition on a write of a description, so
an edit made in the moment between the read and the write is not kept, and the
later write wins: [The host](/host) says an agent in a cycle may edit a
description with the same token. What the command read is
written to `~/tehas/preview/<pr>/.description`, mode `0600`, before it writes,
and the log says `#<pr>: the notice was written`, so a description written over
is visible on the host and can be copied back from there.

**What one pull request leaves on the host.** Under `~/tehas/preview/<pr>/`:
`state.json` (the state, above), `pull.json` (its title and address), `.retry`
(a commit whose container did not start, and how many starts it had), `.notice`
(what was said about a commit and state) and `.description` (what GitHub had as
the description before the last write). All of it goes with the pull request,
at the next run after it is merged or closed.

**What is not removed.** A notice in a description stays when its pull request
closes: nothing of previews edits the description of a merged or closed pull
request, so a merged one keeps an address that answers 404 from then on. So does
a comment written by the version of this command that commented instead (until
2026-10-11): nothing reads or removes comments now, and those comments are left
where they are.

**The sweep**, every three hours, is the second check that nothing of a merged
or closed pull request stays on the host. It does not work from GitHub's list
of open pull requests. It starts from what it finds on the host: containers
and images with the label `tehas.preview`, fetched commits and state
directories. It waits up to 30 minutes for a run to end, and no run starts
while it works.

- A container with the label that belongs to no pull request of this
  repository is removed, and so is an image with the label that is not tagged
  `tehas-preview:<pr>` or is another repository's. Docker refuses to remove
  an image that a container runs.
- It then asks GitHub for the repository itself. If GitHub does not show it,
  nothing more is removed, and the log says `GitHub does not show <repo>;
  nothing was swept by its word`. A token that lost its access gets a 404 for
  every pull request, and that must not remove every preview.
- It asks about each pull request by itself. Only four answers remove: a
  clear `closed`, a 404, an open pull request whose head is outside the
  repository, or an open one that the answer says was opened by someone who
  is not an owner, member or collaborator (`#<pr>: swept, it was not opened
  by someone of <repo>`). An open one whose answer does not say who opened
  it is kept. Every container that carries the pull request's label is then
  removed by its id, with the image, the fetched commit, the state and the
  build log. When Docker did not remove one of them, the rest stays as well,
  and the log says `#<pr>: a container could not be removed, tried again at
  the next sweep`. Any other answer keeps everything: `#<pr>: no answer from
  GitHub, kept`.
- For a pull request that is open and ours, a container of its number that
  has a name, none of which is `tehas-preview-<pr>`, is removed.
- A container is removed by the id Docker listed it with. A line of the
  listing whose id is not one is left alone: `an id that is not one,
  skipped`.

A line with `swept` in the log is something the five minute run missed. The
time of the last sweep and how many things it removed are `lastSweep` in
`~/tehas/live/previews.json`.

| Exit | Meaning |
| ---- | ------- |
| 0    | Done. Also when GitHub or Docker did not answer and what runs was left running, and when another run holds the lock |
| 1    | It cannot write under `~/tehas` |
| 64   | A wrong argument |
| 78   | The instance settings are invalid, or `TEHAS_PREVIEWS` is not `1` |

The [plain dashboard](/dashboard) lists the previews under "Previews", each
with its address, or with its state and the reason the run wrote into its state
file - which is what tells a recipe whose volume was refused from a build that
failed - and says when the list was written. A time that is more than a few
minutes old means the runs have stopped: see the timer. A host without previews
has no `previews.json` and no such section.

## The gates (run by the cycle, usable by hand)

A product names its gates in `.pi/verify-cmd`, `.pi/verify-cmd-full` and
`.pi/smoke-cmd`: the first line that is not empty or a comment is the command.
Run them by hand from the root of a clone of the product. For the example
product:

```
scripts/verify.sh          # fast: the lint, shellcheck, the shell tests, the console's tests where cargo is
scripts/verify.sh --full   # plus the console's format and lints (rustfmt, clippy) and the front page's tests (node)
```

## tehas update

Bring this host up to date with the platform repository in one command.

```
tehas update             # pull, install, run the follow-ups that apply
tehas update --check     # report what would change; changes no file of the
                         # checkout or of the host (origin/ refs do move)
tehas update --help      # print the usage of the command, touch nothing
```

It runs, in order and one line per step: a `git fetch` and a fast-forward of
the platform checkout (`$TEHAS_HOME/platform`, in the example
`~/tehas/platform`), then `install.sh`, then the follow-ups install.sh asks for.
It takes at most one argument; anything else is refused with exit 64 before the
settings are read. The settings are checked next against the required list in
`lib/config.sh` (`tehas_require_instance_settings`), the same one install.sh and
the dispatcher check, so an instance with a required line still commented out
stops with 78 before git runs. After a fast-forward it checks them again with
the library that came with the code it pulled: a change that adds a required
setting stops with 78 before install.sh runs, rather than after the platform
tree moved. The checkout must be on a branch and have nothing uncommitted, and
only a fast-forward is done: local commits of its own, or a branch that was
rewritten on the remote, are left for a person. When the checkout sits on a
branch that is not the remote's default, it says so and updates the branch it is
on.

While `~/tehas/state/update-hold` exists it changes nothing and exits 1, naming
the file; `--check` says the hold is there and reports as usual. The file is
how a host is kept on an older release: see
[Rolling back tehas](/host#rolling-back-tehas).

One update runs at a time. A real run holds `$TEHAS_HOME/state/update.lock`,
and so does a real run of `install.sh`; a second one started while the first
holds it prints `another update is running` and exits 1 before git, install.sh,
Docker or systemd is called. The update hands the lock to the install.sh it
runs. `--check` takes no lock.

That pre-check covers the required settings only. The rest of the instance
contract - `models.json` readable, valid JSON, listing the models the settings
name, and the settings directory writable by nobody else - is install.sh's, and
install.sh answers it with 78 before it copies a file.

After the follow-ups, pi-rukas. It is not pinned: where the installed `versions.env` sets
`PI_RUKAS_FOLLOW`, the update runs `sandbox/build.sh --if-moved`, which
rebuilds the sandbox image only when the commit it names, the newest release
tag by default, is not the commit the image was built on. It does this also when no file of tehas changed, and
skips it when this run built the image already or a step failed. An image that
cannot be moved, for example because upstream has not published that commit's
image yet, makes the update exit 1. `--check` runs `build.sh --check` and
exits 1 when the commit moved. See
[pi-rukas comes from upstream](/host#pi-rukas-comes-from-upstream).

The follow-ups are steps named by id in `lib/update-steps.sh`, which install.sh
prints as `next: <step line>` and this command runs as its own steps rather than
as shell text. The arguments belong to the step, and `lib/update-steps.sh` says
which ones each id accepts and which of them it requires:

| Step                        | install.sh asks for it when         | tehas update runs it when                                        |
| --------------------------- | ------------------------------------ | ----------------------------------------------------------------- |
| `units <unit>...`           | a unit file changed                  | systemd reloaded its unit files, and the unit is active: restarted, never enabled |
| `sandbox`                   | `sandbox/` or `versions.env` changed | always                                                            |
| `docs --force-recreate`     | `edge/docs.*` changed                | its container is running: recreated with that flag                 |
| `edge --force-recreate`     | `edge/router.*` changed              | its container is running: recreated with that flag. Or the Protolab's server of before it runs, the compose project `tehas-protolab`: that one is taken down and the router is started. If the old one came down and the router did not start, the step stays owed and `~/tehas/state/edge-takeover` is left, holding the time it was made: the next `tehas update` starts the router whatever runs, and removes that file when it did. A mark of more than a day ago is not acted on - it is said, removed, and the step is what it would have been without it |

Both containers are recreated rather than just brought up, because what changed is
the Caddyfile they mount and Caddy reads its config when the process starts:
`docker compose up -d` alone leaves a running container with an unchanged service
definition serving the config it loaded at its start.

A service the host does not run is left alone, so an update adds nothing to it.
Whether one runs is asked of Docker and of systemd, and only their answer counts
as an answer, never their exit status on its own. The `units` step asks systemd to
reload its unit files before it restarts anything - a unit file copied onto the
host is not in effect until that has happened - and only silence next to exit 0
means the reload did: a reload that failed, or answered with text, stops the step
and leaves it owed rather than report a changed unit as applied. `is-active` is
asked without `--quiet` so that systemd names the state, and only its own state
words are read: any state of the active family (`active`, `activating`,
`reloading`, `refreshing`, `maintenance`, `deactivating`) restarts the unit,
`inactive`, `failed` and `unknown` (systemd has no such unit on this host) leave it
alone. An answer that is none of those - no user bus, or a `systemctl` wrapper that
prints an error and still exits 0 - stops the update with exit 1 instead of
reporting the service as absent. Liveness is asked of Docker with `docker ps
--filter label=com.docker.compose.project=...`, which needs the project name each
compose file pins.

A step line whose id or arguments are not ones `lib/update-steps.sh` defines is
not run: it is named in a `WARN:`, stays in the record of steps owed, and the
command exits 1, so a producer and a consumer that have drifted apart cannot both
stay green while a step nobody performed is treated as done. The units on a step
line are checked as unit names and against the units this version of tehas
installs - `tehas_units` - so a step line cannot name some other unit of the host's
for an update to restart. The lines are checked with the checkout's
`lib/update-steps.sh` once install.sh has run, so a unit the pulled version adds
is restarted by the update that pulled it. A compose line that leaves out its flag is refused for
the same reason: `docker compose up -d` without `--force-recreate` is the weaker
action, and running it would renew nothing while the step was counted as done.

The record itself is read before anything runs, and a record this account cannot
open stops the update with exit 1 rather than reading as "nothing is owed":
steps somebody else recorded would otherwise be replaced with what this run
happened to know. Each rewrite of the record goes through a temporary file moved
over it, so a run that died mid-write leaves the old record rather than half of a
new one.

**Steps owed.** install.sh records each follow-up as it copies the file that calls
for it, in `$TEHAS_HOME/state/update-pending`, and `tehas update` takes a line off
the record only once it has run. That record is what makes an update complete: a
run that stops partway, or an install.sh that died with half its files copied,
leaves the rest owed instead of losing it with install.sh's one-shot report of
what changed - the next run compares again and finds those files matching.
`install.sh --check` reports the record as `Follow-ups: NOT DONE` and exits 1
while any of it is left, including a step line no version of tehas here knows.
Steps run in the order `lib/update-steps.sh` lists them - systemd first - whatever
order the lines were recorded in, so a step owed from an earlier run does not jump
ahead of this run's unit reload. A hand-run install.sh leaves the same record
behind; running `tehas update` finishes it, and repeating a renewal that has
already happened is harmless. A hand-run install.sh prints its own report only for
what its files changed: when they all match it says `tehas is up to date` even with
steps still in the record, so `install.sh --check`, or `tehas update --check`, is
what names them. There is no command that marks a follow-up done by hand; running
`tehas update` runs it again and clears it.

Every call outside the command has a limit, and hitting it fails saying how long:
120s for the fetch, 1800s for install.sh (copies, dependency check, tests, and its
one call to a remote), 180s for Docker, 60s for systemctl, 1800s for the sandbox
build. An `install.sh --check` stopped at its limit has copied nothing, and the
line says so. The tests are most of its time; on a busy host run
`./install.sh --check` by hand to see them through.

`--check` contacts the remote with `git fetch --prune`, so the remote-tracking
refs under `origin/` match what the remote has, including branches the remote
deleted; no file of the checkout or of the host changes. It runs `install.sh
--check`, which reports the checkout, the slice, the installed files, any
follow-up an earlier run left undone, and the tests; it merges nothing, installs
nothing, builds nothing and starts nothing, and exits 1 when there is something
to update. A host whose `/etc/systemd/system/tehas.slice` is missing or is not
the file this version ships is something to update, and only root can do it:
`tehas update --check` stays red until `sudo ./bootstrap.sh slice` has been run.

What `install.sh` prints passes through as it printed it, apart from the `next:`
lines this command runs itself. That includes a `Login: NOT CURRENT` report for
`/usr/local/bin/tehas`, the command the other accounts type: root's to install,
so it is named as a line rather than run as a step.

When `install.sh --check` exits 1, `--check` says which kind it found.
`Installed: DRIFT` and `Follow-ups: NOT DONE` are `something to update`.
`Dependencies: NOT OK`, `Slice: MISSING` or `DIFFERS`, `Login: NOT CURRENT` and
`Tests: FAILED` are `something tehas update does not fix`: running the update
leaves them as they are.

| Exit | Meaning                                                                       |
| ---- | ----------------------------------------------------------------------------- |
| 0    | Updated, or already up to date                                                 |
| 1    | Another update is running, the checkout has local changes, is not on a branch, cannot fast-forward, has no commit yet, a step failed or ran out of its limit, `install.sh` ran out of its limit or gave no answer, a follow-up install.sh asked for is one this command does not know, systemd did not reload the unit files, a container or unit gave no answer, the record of steps owed exists but cannot be read, a status of `install.sh` this command does not know (named in the message), or `--check` found something to update (a checkout behind, files that differ, steps left over from an earlier run) or something an update does not fix (the host's tools, the slice, a host's `/usr/local/bin/tehas` that is not the checkout's, the tests) |
| 64   | More than one argument, or an argument this command does not know              |
| 78   | The instance settings are missing or malformed, or there is no platform checkout |

## install.sh

Installs tehas on the host from the platform checkout, for the instance the
host points at. See [The host](/host).

```
cd ~/tehas/platform
./install.sh --check     # change nothing; Instance, Dependencies, Slice, Installed, Login, Follow-ups, Tests
./install.sh
```

Exit 0 when done or nothing to report, 1 when `--check` found something, a
step failed or another update is running, 78 when the instance settings are invalid (nothing was changed).
`Slice: MISSING` and `Slice: DIFFERS` are something found: `--check` exits 1
until root has run `sudo ./bootstrap.sh slice`, because every sandbox container
runs in the slice and none starts without it. A real run installs either way and
prints a `note:` line instead.

After a real run it prints one `next: <step>` line per follow-up its changed
files call for - the ids `lib/update-steps.sh` defines - and starts nothing
itself; `tehas update` is what runs them. Each of those steps is also recorded in
`$TEHAS_HOME/state/update-pending` as the file that calls for it is copied, so a
run that dies partway still leaves its copies owed, and `--check` adds a
`Follow-ups: NOT DONE` line, and exits 1, while any of them is left. It asks
systemd for nothing, but to reload before it removes the earlier layout: a copied unit file is not in effect until systemd has reloaded
it, and that reload belongs to the `units` step, which cannot report a changed unit
as applied when the reload did not happen.

A real run also builds the code index with [tehas index](#tehas-index), once:
when there is none yet and both the sandbox image and the product checkout are
there. Otherwise it prints the command on a `later:` line, which is for a
person and is not a step of `tehas update`; the dispatcher builds the index
when it next starts a cycle with none running. An indexing that fails is a warning and does not
fail the install.

A real run that replaced `versions.env` prints `later: <checkout>/bootstrap.sh
user`. The pins moved, and the tools on the host, pi-rukas among them, follow
them only when that runs; until then `--check` says `Dependencies: NOT OK`. It
is no step of `tehas update`, because it fetches from the network.

It also removes the commands of the earlier layout from `~/tehas/bin`, unless a
cycle is running that still uses them. See [The host](/host).

One file it reports and never touches: `/usr/local/bin/tehas`, the command an
account that is not the tehas user types, which `sudo ./bootstrap.sh dev` puts
there as root from `host/login/tehas`. Where that setup is in place, its marker
`/etc/tehas/manager` names the tehas user, and a copy of the command that is not
this checkout's makes `--check` add a `Login: NOT CURRENT` line naming the command
that installs it (`sudo ~/tehas/platform/bootstrap.sh dev boss`) and exit 1. A
real run prints the same finding on a `later:` line. It is never a `next:` step,
because no tehas command runs as root, and nothing is recorded as owed. The name
in that command is the marker's; a marker holding something other than an account
name leaves `<tehas user>` for the person to fill in. A host where
`bootstrap.sh dev` never ran reports nothing. See [The host](/host).

## bootstrap.sh

Prepares a fresh host with everything tehas and pi-rukas need, at the versions
in `host/versions.env`. See [Bootstrap a host](/bootstrap).

```
sudo ./bootstrap.sh root boss   # system packages, Docker, docker group, lingering, the slice
sudo ./bootstrap.sh slice       # the slice alone, /etc/systemd/system/tehas.slice
./bootstrap.sh user             # the rest, under the user's home
sudo ./bootstrap.sh dev boss    # let other accounts start a session with tehas dev
./bootstrap.sh check            # change nothing; found and expected version per tool, and the slice
```

Exit 0 when done or `check` found everything, 1 when something failed or
`check` found a tool missing or at another version, 64 on a usage error or
`slice` not run as root.

## scripts/test.sh

The checks of the platform itself. Run from the platform checkout; it is not
installed on a host.

```
scripts/test.sh
```

It runs shellcheck over the shell scripts, then every `test/*_test.sh`. Those
need bash, git, jq and shellcheck only: no sandbox image, no product checkout
and no network. The test files run side by side, at most `TEHAS_TEST_JOBS` at
once: by default the CPU quota of the cgroup it runs in, rounded up, or the
number of CPUs where there is none. Each file's output is printed whole, in name
order, and the run ends with a block that says what did not pass:

```
test.sh: failed checks (2), not run (0)
    shellcheck
    test/plan_test.sh
```

Its first line counts the checks that failed and the checks that did not run.
Each line after it names one of them: `shellcheck`, or the path of a failing
test file relative to the checkout; a file that never finished is named with
`it did not finish, no result recorded`; and a line starting `skipped:` gives a
check that did not run, with its reason.

The block is the last thing the run prints, and it is capped to fit whole inside
the lines `scripts/verify.sh` keeps at the end of this step: past the cap it
names the last failing checks and counts the rest, as in `and 7 more, in the
full log`. A log cut short therefore still says how many checks failed. The
block appears only for a run that ends: a test file that hangs holds the whole
run up, with no block and no name. Exit 0 when every check passed, 1 when one
failed, 2 when nothing could be checked.

`scripts/check-generic.sh` is the lint that keeps private network addresses out
of every file, and product and host names out of platform files. The names
come from `scripts/check-generic.names`, a list each working machine keeps for
itself: git ignores it and it is read as data. `scripts/verify.sh` runs the
lint and `scripts/test.sh` together.
