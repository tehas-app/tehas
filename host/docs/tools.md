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
| `PI_ENSEMBLE_SUBAGENT_MODEL`    | required                                 | Model for every subagent role                                   |
| `PI_ENSEMBLE_SUBAGENT_PROVIDER` | required                                 | Its provider, as named in `models.json`                         |
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
| `TEHAS_CONCURRENCY`             | `4`                                      | Workstreams side by side inside one cycle                       |
| `TEHAS_TICKET_CONCURRENCY`      | the host's CPUs minus one, at least 1    | The most cycles side by side. A ceiling, never a divisor: the dispatcher starts fewer while `tehas.slice` has no room. A value above the host's CPUs is accepted. Lower it when the model backends cannot serve `TEHAS_CONCURRENCY` times this many workstreams; nothing measures that |
| `TEHAS_MAX_AWAITING`            | `TEHAS_TICKET_CONCURRENCY` plus 2        | Open `tehas:awaiting-merge` issues, plus running cycles, allowed before holding |
| `TEHAS_CYCLE_RESERVE`           | `2g`                                     | Memory one container is given as a floor (`docker --memory-reservation`) and the unit the dispatcher counts room in. A reclaim priority inside `tehas.slice` and the amount admission promises, not a guarantee against the whole host. A Docker size of at least 6m, and at most a quarter of the slice's `MemoryMax`, else the command stops with 78 naming both numbers. With the default the host needs about 11 GiB of memory; a smaller host sets it lower |
| `TEHAS_WORK_TIMEOUT`            | `21600`                                  | Seconds before a cycle is given up. At most 25200: [the cycle's unit](/host#time-limits) stops a cycle after 8 hours |
| `TEHAS_WORK_STALL`              | `3600`                                   | Seconds a cycle may go without a line added to any of its logs before it is stopped as `stalled`. Must be longer than `TEHAS_VERIFY_TIMEOUT`, because a gate can print nothing while it runs; else the command stops with 78 |
| `TEHAS_WORK_IMAGES`             | `0`                                      | `1` gives a cycle and a planning session the images its issue text links: see [tehas work](#tehas-work) and [tehas plan](#tehas-plan). The model must declare `"image"` in its `input` in `models.json` and answer a request with an image, else the dispatcher holds every cycle |
| `TEHAS_WORK_DESCRIBE`           | `0`                                      | `1` has the model describe each of those images in words before the cycle or planning session starts, into `images.md` beside them: see [tehas work](#tehas-work). Needs `TEHAS_WORK_IMAGES=1`, else the command stops with 78 |
| `TEHAS_WORK_FILES`              | `0`                                      | `1` gives a cycle and a planning session the text files its issue text links: md, txt, csv, json, yaml, log, html, css and js, at most 16, 1 MB each and 4 MB together. Needs no model that reads images. See [tehas work](#tehas-work) |
| `TEHAS_VERIFY_TIMEOUT`          | `1800`                                   | Seconds one run of the product's gate may take inside a cycle. A gate that takes longer is stopped and the cycle parks. Passed to pi-rukas as `PI_ENSEMBLE_VERIFY_TIMEOUT_MS` and `PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS`; pi-rukas alone allows ten minutes |
| `TEHAS_PLAN_MAX_TURNS`          | `20`                                     | Turns a planning session may take before it is ended            |
| `TEHAS_PLAN_TIMEOUT`            | `3600`                                   | Seconds before one planning turn is given up. At most 7200: [the poll's unit](/host#time-limits) stops a poll after 3 hours |
| `PI_ENSEMBLE_HOST_ALIASES`      | empty                                    | `name:address` pairs the sandbox must resolve, comma separated  |
| `PI_ENSEMBLE_IMAGE`             | `tehas/pi-rukas:uid<uid>`                | The sandbox image                                               |

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

Order of rules: pause label on the control issue, a usable `tehas.slice`,
the free slots (`TEHAS_TICKET_CONCURRENCY` less the running cycles), the cap
on issues awaiting merge or being worked on, the room and the memory pressure
in the slice, who applied `tehas:urgent`, then who applied `tehas:ready`, then
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

A poll also reads the open issues marked `tehas:running`. One whose cycle
does not hold its lock on this host, `~/tehas/state/cycles/<issue>.lock`, gets
`needs-human-attention` in place of `tehas:running` and a comment that says
where the run log is. The log line is `park #N: it is marked tehas:running and
no cycle holds its lock`. If the label change fails nothing is said on the
issue and the next poll tries again. A closed issue in that state only loses
`tehas:running`. For either, a container `tehas-work-<issue>` that is still
there is removed, with the line `removed the container tehas-work-N: no cycle
holds the lock of #N`: its client died, and it would work on with the bot's
token and hold the name. The container of a planning turn that was cut off,
`tehas-plan-<issue>`, is removed the same way before the turn is reported. A
paused factory leaves such an issue as it is. Run the dispatcher only on the host that runs the cycles: on any other
host no cycle holds a lock.

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

A `tehas:ready` from any other account asks for nothing, so it comes off the
issue with a comment naming who
applied it; the log line is `clear #N: tehas:ready off, applied by ...`. A
`tehas:plan` from such an account comes off the same way. The
comment follows the label change: when that change fails nothing is said on the
issue and the next poll tries again. An issue the poll does not read as a
candidate keeps its labels as they are, which covers one whose own cycle is
alive and every issue while a hold applies. No other label holds it
back: `needs-human-attention`, `tehas:awaiting-merge` and `tehas:question` are
taken off as the cycle starts. While the issue's own cycle is alive the log
says `wait #N: its cycle is still running`, and the issue is taken once that
cycle has ended. Every list of issues is read up to 1000. A cycle starts only once the
issue is marked `tehas:running`; if that label change fails, the poll stops.

Before it marks an issue, the poll writes what the start rests on to
`~/tehas/state/cycles/<issue>.approved.json`: `at` and `by`, when and by whom
it was approved, `how` (`label` or `retry-tick`), `body_sha256`, the sha256 of
the body that approval was checked against with carriage returns dropped, and
`links`, the links to GitHub's attachment storage in that body. The body is
read in the same call as the record of who last edited it, so it is the text
the check passed. [tehas work](#tehas-work) fetches those links and no others.
A record that cannot be written starts nothing: `ERROR: could not write the
approval record of #N, not starting a cycle`, exit 1, and no label changes.

## tehas work

One pi-rukas `/work` cycle, headless, in the sandbox image.

```
tehas work <issue> [--restart]
```

It starts `pi --mode rpc --approve` in a container, sends `/work <issue>` and
waits until `.pi/work-state/<issue>.json` in the workspace `~/tehas/work/<name>-<issue>` leaves `running`.
It then gives the driver 20 seconds to post its handoff and ends the session
by closing Pi's input. Only a Pi that has not gone five seconds later is
stopped with `docker stop`.

It holds the issue's lock, `~/tehas/state/cycles/<issue>.lock`, while it runs.
Run by hand for an issue whose cycle is running, it refuses before it moves
anything (exit 5). Under `tehasbot cycle` it is handed that lock, already
held, and goes on. While a run by hand holds the lock, the dispatcher counts it
as the issue's cycle and starts no other for the issue. With the lock held, a
container left under the name `tehas-work-<issue>` is no cycle's: it is
removed before the new one starts.

| Exit | Meaning                                                          |
| ---- | ---------------------------------------------------------------- |
| 0    | The cycle ended as `merged` or `handoff` (parked for a human)    |
| 1    | The cycle ended as `aborted`                                     |
| 2    | Pi exited, the time limit passed, or the cycle stalled, while it was running |
| 3    | The sandbox cannot push to the repository; no cycle was started  |
| 4    | With `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`: an image or file the issue links could not be fetched, the issue could not be read for them, its text is not the text that was approved, or a file is not the one first fetched for the approval; no cycle was started |
| 5    | A cycle for this issue is running already: its lock `~/tehas/state/cycles/<issue>.lock` is held. Nothing was moved and no container started |
| 64   | No bot token or no git identity; no cycle was started            |
| 78   | The instance settings are invalid, or `tehas.slice` is missing, has no memory limit, or cannot be used (the message names `sudo ./bootstrap.sh slice` or the reason); no cycle was started |
| 137  | The container was killed while the cycle was running: out of memory, or stopped from outside. The cycle is `killed` |

The container runs in `tehas.slice` and gets the bot token and the
`PI_ENSEMBLE_*` settings, a memory ceiling of a quarter of the slice with no
swap, `TEHAS_CYCLE_RESERVE` as its floor, a pids limit of 4096, a CPU weight
and no CPU quota, and no Docker socket or SSH keys. git inside it is given the
bot's identity and a credential helper that reads the token, through
environment-level configuration; before a cycle starts, a dry-run push proves
that works. The RPC stream is kept in `~/tehas/logs/work-<issue>-<time>.jsonl`.
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

A cycle that writes nothing does not wait for its time limit. When Pi's RPC
stream, its error stream and the agents' transcripts under
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
  Comments are not read: `tehas:ready` approves the text, and a comment can
  arrive after the label. Only when an allowed account approved the start,
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
`~/tehas/state/cycles/<issue>.manifest.json`. When the next fetch is for the
same approval, a restart of the same cycle, a link that gives other bytes than
the kept manifest says stops the start with exit 4 and the link named. The directory is remade for every cycle and removed by
`tehasbot cycle` when the cycle is over. A comment on the issue says how many
images the cycle was given, each with its link and sha256, and what was left
out. An image that could not be
fetched stops the start with exit 4, and the comment names it: the issue is
parked, and `tehas:ready` tries again.

Whether the agents are shown them is pi-rukas's part: a release reads the
manifest only when it has the file
`/opt/pi-rukas/extension/src/work-driver-issue-images.ts` in the sandbox
image. Before the comment, `tehas work` asks the image for that file, in a
container of a few seconds that is given no mount, no variable and no network
and pulls nothing. The answer is kept per image id in
`~/tehas/state/images-read/`, since an id names fixed content, and the last
one in `~/tehas/state/images-read.json` for [tehasbot health](#tehasbot-health).
When the image does not have it, the comment says the images were fetched but
its agents are not shown them; when the image could not be asked, it says
that too. The images are mounted either way.

pi-rukas reads the manifest and attaches the images to the explore and plan
prompts, each listed beside its link. The steps after plan do not see them.

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

## tehas plan

Runs one turn of a planning session, headless, in the sandbox image. The
dispatcher calls it. By hand it is useful to see what a turn does.

Run by hand it removes and remakes the workspace that a planning turn of the
same issue uses, and it does not take the dispatcher's lock, so do not run it
for an issue whose session the factory is working on.

```
tehas plan 12 --out /tmp/turn.json --keep /tmp/turn.keep --brief brief.md
tehas plan 12 --out /tmp/turn.json --keep /tmp/turn.keep --session <id> --answer answer.md
```

The first form starts a session from a file with the issue's title and body.
The second reopens the session and sends the file's content as the operator's
answer. A turn ends when the agent has settled and every subagent it sent out
in the background has reported back, so it ends when the agent has asked. A
subagent that never reports holds the turn until `TEHAS_PLAN_TIMEOUT`.

`--keep` names a directory the session carries from one turn to the next. The
first form makes it anew. It holds the commit the first turn's workspace was
at, pi-rukas's record of the plans the agent showed (its plain `.json` files,
at most 1 MB each), and with `TEHAS_WORK_IMAGES=1` the issue's images. Every
later turn's workspace is at that commit and gets that record back, so a plan
the operator confirms is filed as it was shown. A later turn whose commit is
missing or gone starts at the tip of the branch and says so, and the agent
then reviews the plan again before it files it. The dispatcher keeps the
directory at `~/tehas/state/plan-<issue>.json.keep` and removes it when the
session ends.

With `TEHAS_WORK_IMAGES=1` the first turn fetches the images the issue links,
by the rules of `tehas work`, with `tehas:plan` in place of `tehas:ready`,
and says on the issue how many it was given and whether the sandbox image's
pi-rukas reads them.
Each turn's container gets a copy of them, read-only, at the path a cycle has
them, with `PI_ENSEMBLE_EXTRA_IMAGE_DIRS`. The copy is
`~/tehas/work/<name>-plan-<issue>.images`, removed after the turn. An image
that cannot be fetched starts no session: exit 4, said on the issue. With
`TEHAS_WORK_DESCRIBE=1` the first turn also writes `images.md`, as
`tehas work` does, and every turn's copy holds it. With `TEHAS_WORK_FILES=1`
the first turn fetches the text files too, by the rules of `tehas work`, into
the same directory. On exit 0 the `--out` file holds the session id, the agent's last
message and the question it asked, if any. On exit 2 it holds the session id
alone when that is known, so the dispatcher can run the turn again in the
same session, and it is not written when no session is known.

| Exit | Meaning                                                              |
| ---- | -------------------------------------------------------------------- |
| 0    | The turn settled and said or asked something                         |
| 2    | The turn did not settle, timed out, said and asked nothing, or its session or outcome could not be written down |
| 4    | With `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`: a first turn where an image or file the issue links could not be fetched, or the issue could not be read for them; nothing ran |
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
tehas dev [<name>] [--author "Name <email>"] [-- <pi arguments>]
tehas dev --help
```

`<name>` names the clone, `~/tehas/dev/<name>`: letters, digits, `_` and `-`,
`main` when left out. It is made on first use from the product checkout and
never reset or removed. `--author` is stored in `~/tehas/dev/<name>.author` and
becomes git's name and email in every session of that clone; without one the
bot's are used. What follows `--` is passed to `pi`.

| Exit | Meaning                                                                                   |
| ---- | ----------------------------------------------------------------------------------------- |
| any  | The session's own exit status, as Docker reports it                                       |
| 64   | A wrong argument, a malformed author, no bot token or no git identity; no container       |
| 78   | The instance settings are invalid, the clone's directory is not a clone, or `tehas.slice` cannot be used; no container |

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
and a real install prints `later: sudo ./bootstrap.sh dev boss`. Run that command
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
| 1    | The sandbox image is missing, or the indexer failed    |
| 3    | A cycle is running; nothing was done                   |
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
`--before-cycle` ahead of every cycle: the lock is the dispatcher's own then,
and nothing is done when `~/tehas/state/index` already holds what the record
says is the branch's tip. An empty directory counts as no index, so a host
upgraded from the version that kept the index in a cache volume builds it into
the new one at its next cycle rather than reporting an index that no cycle is
given. So a cycle starts with an index of the tree it is cloned from. What the
cycle changes afterwards, in its workspace or its worktrees, is not in the
index.

Run by hand it always rebuilds, and it holds the dispatcher's lock while it
does, so no cycle starts in the middle. A cycle started by hand with
`tehas work` refreshes nothing.

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

## tehas status

```
tehas status
```

```
factory   pass: Idle, taking work. (40 s ago)
queue     2 in line, 0 building, 1 in review, 0 stopped
cycle     #12 handoff at step review, 3 min ago
slice     room for 8 cycles (4 GiB of 20 GiB promised, reserve 2g, 40 s ago)
```

Prints the state of the factory from the [health file](#tehasbot-health): is
it taking work, what is in the queue, the current or last cycle, and what the
dispatcher's last poll read of `tehas.slice`. The `slice` line reads
`holding: ...` when that poll held cycles back, and `unknown: no poll has
reported in 10 min` when no poll read the slice in that time. Where the
instance gives a cycle images (`TEHAS_WORK_IMAGES=1`), an `images` line says
whether the sandbox image's pi-rukas reads them, as the last cycle or planning
session found: `read by ...`, `fetched but not read: ...` or `unknown: ...`.
It reads that one file and nothing else: no GitHub call, no instance settings, and it
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
the console of a cycle - and asks `systemctl --user` about its units. No GitHub
call, no model call, no Docker call, no instance settings, and nothing is
written. What it shows is as old as those files, and every section that has an
age prints it. A file older than ten minutes is not believed, the same rule
`tehas status` uses.

The frame is drawn from one snapshot and from nothing else. `--json` prints that
snapshot: `generatedAt` and one key per section (`factory`, `queue`, `cycle`,
`units`, `energy`, `console`). A section whose source is missing, unreadable or
too old to believe is `null`; the frame says so in one line and draws the rest.
The factory line and the queue counts are the ones `tehas status` prints: both
commands take them from `lib/status.sh`, so they cannot drift apart. A section
added to the snapshot appears in `--json` without anything else changing.

A frame follows the width and height of the terminal, also after a resize. Long
lines are cut; nothing wraps and nothing scrolls. When the terminal is short,
console lines are dropped first, then unit rows, then queue rows. Text that came
out of a file has its control characters removed before it reaches the terminal.
`COLUMNS` and `LINES` say what the size is when the terminal cannot be asked.

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
tehasbot protolab             # build the Protolab; what tehas-protolab.timer runs
tehasbot preview              # keep the previews; what tehas-preview.timer runs
tehasbot preview --sweep      # the second check; what tehas-preview-sweep.timer runs
tehasbot improve              # one run of the improvement loop; what tehas-improve.timer runs
```

The units are `tehas-dispatch.timer`, `tehas-energy.service`,
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

It runs [tehas work](#tehas-work) for the issue, reads the cycle's state file
from the workspace, finds the pull request the bot opened, and relabels and
comments: merged, `tehas:awaiting-merge`, or `needs-human-attention`. A cycle
that stopped gets its comment with two boxes and the label `tehas:question`,
and the comment's id goes into `~/tehas/state/park-<issue>.id`, which is what
lets the dispatcher take a tick there. It
then removes the workspace of a cycle that merged or left a pull request, and
keeps the workspace of one that stopped. The images `tehas work` fetched for
the cycle are removed in every case. A runner that started no cycle over an
image (exit 4) is parked with a comment that says so. A retry moves a stopped workspace to
`<name>-<issue>.before-retry`, which is removed and kept by the same rule. A
stopped unit stops the container and is reported as a cycle that ended without
a pull request. A hangup is taken as a stop too. It checks no authorisation:
the dispatcher did that before it started the unit. It does check the number:
it writes `state/cycles/<issue>.taken` once it holds its lock, then asks GitHub
what the number is before a label is changed, a comment is posted or a
container starts, tried three times like its other calls, and refuses a closed
issue and a pull request (exit 65) with one line in the log. A number GitHub
could not answer for at all is refused the same way, with a line that says
that. Because the marker was written first, the poll that started the unit
takes such a cycle as one that came up: the issue keeps `tehas:running`, and
the next poll clears it in silence.

`tehas:awaiting-merge` is given only to a run that ended as `handoff` and has
an open pull request of the bot that names the issue. Any other end with a
pull request open gets `needs-human-attention`, and the comment names the
pull request.

The pull request is looked up in every state. One that a person merged while
the cycle ran ends the cycle as merged, whatever the state file says: the
issue only loses `tehas:running`, the comment says the pull request was merged
while the cycle ran, nothing is parked, and [tehasbot status](#tehasbot-status-and-the-console)
labels the closed issue `tehas:shipped`. One that a person closed without
merging gets `needs-human-attention` and a plain comment that says so, without
the retry boxes: to try again, apply `tehas:ready`.

The state file is written inside the container, so what it says is checked
against GitHub. A pull request counts as the cycle's only when the bot opened
it on the branch the state names and it names the issue: its head is
`feature/issue-<n>-...`, as pi-rukas names a branch, or its body closes `#<n>`
(`Fixes #<n>`, `Closes #<n>`). A pull request on that branch that names
another issue is said in the comment and not taken. A state of `merged` is
reported as merged only when the bot has a merged pull request for the issue
on that branch. Without one the issue is parked, the comment says that the
bot has no merged pull request for the issue, and both workspaces are kept. A
state file reached through a link, or under a linked `.pi` directory, is not
read: the status is then `unknown`.

The label change that ends a cycle is tried three times, five seconds apart,
and so is the search for the pull request. If GitHub still refuses the label
change, the log says `WARN: label change on #N failed`, the cycle's comment
says that the label could not be changed, and the issue stays `tehas:running`
until the next poll of [tehas dispatch](#tehas-dispatch) finds it.

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
`~/tehas/libexec/console` is started by the work
script and follows the cycle's agents into `~/tehas/live/console-<issue>.txt`, masking
tokens, passwords and private network addresses, and writing no control
character but the line break and the tab. An agent of a workstream is named
with it, as in `developer[task-a]`. The mask catches accidents;
it cannot stop an agent that sets out to print a secret another way, so treat
the console of a published dashboard as public. A log line of over 1 MiB is
left out, and the console says how many were. Neither needs running by hand.
What the console itself complains about goes to `~/tehas/logs/work-<issue>-<time>.jsonl.console`,
and the work script says so when the console is missing or ends with an error.
The work script's own status line, in `~/tehas/live/cycles/<issue>.json` and from there in `~/tehas/live/running.json`, goes through
the same mask (`console --mask`); without a console it is left empty.

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
the running cycles, the last one that finished and what the dispatcher's
last poll wrote about the slice. The status script runs it on exit, so it is
refreshed at every poll and at the end of every cycle. The docs server
serves the file at `/health` and `/healthz` as `application/health+json`.
`tehas status` prints the same file for a person.

| `status` | Meaning                                                                                  | HTTP |
| -------- | ---------------------------------------------------------------------------------------- | ---- |
| `pass`   | Taking work, idle or in a cycle                                                          | 200  |
| `warn`   | Paused (`tehas:pause` on the control issue), or the dispatcher holds everything because the slice is missing or cannot be read, or there are no resources (the model endpoint does not answer or does not offer the model), or too low on disk to build previews, or the free space could not be read, or the images a cycle is given are fetched but the sandbox image's pi-rukas does not read them | 200  |
| `fail`   | The last cycle ended as anything but `merged` or `handoff` and no cycle has started since, or there is no queue snapshot | 503  |

`output` says the same in a sentence. `checks.queue` has the snapshot time and
the counts in line, building, in review and stopped. `checks.cycles` has the
issue, status, step and times of every running cycle; `checks.cycle` is the
first of them, or the cycle that finished last when none is running.

`fail` is about the cycle that finished most recently. It lasts until a cycle
starts after that one ended. A cycle that was already running when the other
one failed does not clear it. A cycle that has written nothing for two
minutes is recorded as ended with the status `lost`, which is a failure.

`checks.disk` has the free and the required bytes and `low`, as `tehasbot
preview` last wrote them to `live/previews.json`. It is `null` on a host
without previews, and when that figure is more than half an hour old. When
`low` is true, `output` is "The host is low on disk: previews are not being
built." with a figure, and "The host's free disk space could not be read:
previews are not being built." when `freeBytes` is `null`.

`checks.images` has `reads` and the `time` it was found, from
`state/images-read.json`, which `tehas work` and `tehas plan` write when
the instance gives images and remove when it does not. `reads` is `true`,
`false`, or `null` when the image could not be asked. It is `null` on an
instance without images. When `reads` is `false`, `output` is "Images are
fetched, but the sandbox image's pi-rukas does not read them."

`checks.slice` is `live/dispatch.json` as the dispatcher's last poll wrote
it: `time`, `why`, `hold`, `count`, `room`, `committed`, `max` and `reserve`.
`why` is empty when cycles may start, else one word: `slice`, `unreadable`,
`resources`, `count`, `awaiting`, `pressure` or `room`, and `hold` is the line
the dispatcher logged. `resources` is a model endpoint that does not answer or
does not offer the configured model. `room` is how many more reserves fit in the slice;
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
its word by word updates, the console and the error output. A run fetches the
open issues and the ones closed in the last 30 days, takes the records that
wait, and starts the analyst. Then it checks the one file the analyst leaves,
files what passes and removes the records.

The findings are about tehas and about pi-rukas, so the analyst is plain Pi
with one prompt (`~/tehas/lib/improve-prompt.md`) and no extension. What it
reads holds issue text and model output, which can be written to steer it, so:

| The analyst's container gets | It does not get |
| --- | --- |
| The records and the issue list, read-only | The bot's token, or any other variable of the host |
| One empty directory to write `findings.json` in | The product checkout, a workspace, Pi's sessions or transcripts |
| `models.json`, read-only, and the host aliases | pi-rukas, skills, context files, MCP servers |
| The tools read, grep, find, ls and write | A tool that runs a command or opens a connection |
| 1 CPU, 2 GiB, no swap, 1024 processes, 30 minutes, in `tehas.slice` | A cycle's limits: it is a batch job with a quota of its own, counted in the slice's room like any other container |

The host makes every GitHub call. A finding is filed only if it is the shape
asked for: a title of at most 100 characters, one of six categories, a body, an
excerpt as evidence, and cycles that were in this run. A title that matches an
open issue is a duplicate. In the issue the model's text has every mention
broken and its evidence inside a fence it cannot close. The label is
`tehas:improvement` and no other.

Limits: 3 issues a run; no run files while 10 open issues carry the label. The
whole log of each cycle a finding names is gzipped and uploaded to the release
`tehas-improve-logs`, in pieces of at most 5 MB (`ticket-<issue>-<time>.log.gz`,
or `.log.1.gz`, `.log.2.gz`). A log that holds something shaped like a token is
not uploaded.

Exit 0 when the run finished, there was nothing to read, 10 issues are open
already, or the loop is off.
Exit 1 when the analyst or GitHub failed: the records are kept and the next run
reads them again. What a failed analyst printed is in
`~/tehas/logs/improve-analyst.log` until a run succeeds. Exit 64 on any argument, 78 on invalid settings. A second run
while one is going leaves it to that one.

Each run also removes the per-cycle files in `~/tehas/logs/` and the records
under `~/tehas/tmp/improve/` older than seven days, read or not.
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

That pre-check covers the required settings only. The rest of the instance
contract - `models.json` readable, valid JSON, listing the models the settings
name, and the settings directory writable by nobody else - is install.sh's, and
install.sh answers it with 78 before it copies a file.

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
for an update to restart. A compose line that leaves out its flag is refused for
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
build.

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

| Exit | Meaning                                                                       |
| ---- | ----------------------------------------------------------------------------- |
| 0    | Updated, or already up to date                                                 |
| 1    | The checkout has local changes, is not on a branch, cannot fast-forward, has no commit yet, a step failed or ran out of its limit, `install.sh` ran out of its limit or gave no answer, a follow-up install.sh asked for is one this command does not know, systemd did not reload the unit files, a container or unit gave no answer, the record of steps owed exists but cannot be read, a status of `install.sh` this command does not know (named in the message), or `--check` found something to update (a checkout behind, steps left over from an earlier run, or a host's `/usr/local/bin/tehas` that is not the checkout's) |
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

Exit 0 when done or nothing to report, 1 when `--check` found something or a
step failed, 78 when the instance settings are invalid (nothing was changed).
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
before a cycle in any case. An indexing that fails is a warning and does not
fail the install.

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
sudo ./bootstrap.sh root boss   # system packages, Docker, docker group, lingering
./bootstrap.sh user             # the rest, under the user's home
./bootstrap.sh check            # change nothing; found and expected version per tool
```

## scripts/test.sh

The checks of the platform itself. Run from the platform checkout; it is not
installed on a host.

```
scripts/test.sh
```

It runs shellcheck over the shell scripts, then every `test/*_test.sh`. Those
need bash, git, jq and shellcheck only: no sandbox image, no product checkout
and no network. Exit 0 when every check passed, 1 when one failed, 2 when
nothing could be checked.

`scripts/check-generic.sh` is the lint that keeps private network addresses out
of every file, and product and host names out of platform files. The names
come from `scripts/check-generic.names`, a list each working machine keeps for
itself: git ignores it and it is read as data. `scripts/verify.sh` runs the
lint and `scripts/test.sh` together.
