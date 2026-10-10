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
tehas dev [<name>]               # an interactive session on the host, by hand
tehas index                      # build the code index the agents search
tehas labels                     # create the tehas labels in the repository
tehas status                     # print the state of the factory
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
| `TEHAS_ALLOWED_LABELERS`        | required                                 | Logins whose `tehas:ready` counts, space separated              |
| `PI_ENSEMBLE_SUBAGENT_MODEL`    | required                                 | Model for every subagent role                                   |
| `PI_ENSEMBLE_SUBAGENT_PROVIDER` | required                                 | Its provider, as named in `models.json`                         |
| `TEHAS_HOST_ADDR`               | `127.0.0.1`                              | Address the docs server and the Protolab bind to                |
| `TEHAS_INDEXING`                | `0`                                      | `1` lets search engines index the dashboard and the manual      |
| `TEHAS_ANALYTICS_SRC`, `_SITE`  | empty                                    | Optional. The https address of a visitor analytics script and the site name it reports under. Both set: every page loads it |
| `TEHAS_ANALYTICS_COOKIE`        | `0`                                      | `1` adds `data-cookie="true"` to that script tag                |
| `TEHAS_PROTOLAB_URL`            | port 8081 of the dashboard's host        | Address people reach the Protolab at, when it is published elsewhere, as `https://lab.example`. The dashboard links to it |
| `TEHAS_WORK_REPO`               | `~/tehas/repos/<name>`                   | The product checkout cycles are cloned from. Must be a clone of `TEHAS_REPO`. The cycle itself works in `~/tehas/work/<name>`, which is not a setting |
| `TEHAS_BRANCH`                  | the checkout's `origin/HEAD`             | Branch cycles start from and pull requests target               |
| `TEHAS_WORK_CPUS`               | the host's CPUs minus one, at least 1    | `docker --cpus` of a cycle's container                          |
| `TEHAS_WORK_MEMORY`             | half the host's memory                   | `docker --memory` of a cycle's container                        |
| `TEHAS_CONCURRENCY`             | `4`                                      | Workstreams side by side inside one cycle                       |
| `TEHAS_MAX_AWAITING`            | `3`                                      | Open `tehas:awaiting-merge` issues allowed before holding       |
| `TEHAS_WORK_TIMEOUT`            | `21600`                                  | Seconds before a cycle is given up                              |
| `PI_ENSEMBLE_HOST_ALIASES`      | empty                                    | `name:address` pairs the sandbox must resolve, comma separated  |
| `PI_ENSEMBLE_IMAGE`             | `tehas/pi-rukas:uid<uid>`                | The sandbox image                                               |

The example instance sets the five required ones, `TEHAS_HOST_ADDR`,
`PI_ENSEMBLE_HOST_ALIASES`, and, because it is public, `TEHAS_INDEXING` and the
analytics settings. It leaves the rest at the defaults.

## tehas dispatch

One poll of the issue queue. Deterministic, no model. The timer runs the same
script as `tehasbot dispatch`.

```
tehas dispatch --dry-run       # report the decision, change nothing
tehas dispatch                 # run at most one cycle, then return
tehas dispatch --help          # print the usage of the command, call nothing
```

`--help` (or `-h`) prints the header comment of the command and exits 0; it
works even on a host with no instance settings yet. Any other argument, or more
than one, is refused: exit 64 with `tehas dispatch: unknown argument: <argument>`
and the usage on standard error, before the instance settings are read. A mistyped
`--dryrun` therefore starts no poll and changes nothing.

Order of rules: pause label on the control issue, the cap on issues awaiting
merge, who applied `tehas:ready`, then urgent first and lowest number. Log:
`~/tehas/logs/dispatch.log` when run by systemd, the terminal otherwise. Each
cycle's runner output: `~/tehas/logs/dispatch-work-<issue>.log`.

Before a cycle it puts the product checkout on the default branch and
fast-forwards it, then refreshes the code index with
[tehas index](#tehas-index) when the branch has moved since the last indexing.
The work script then clones the cycle's workspace from the checkout. An index
that cannot be refreshed is a warning in the log; the cycle still runs. The
dispatcher runs the index, work and status scripts from its own directory,
`~/tehas/libexec`, not by a name on `PATH`.

An issue is taken only when an allowed account applied `tehas:ready` and
nobody else has changed its title or body since. A cycle starts only once the
issue is marked `tehas:running`; if that label change fails, the poll stops.

## tehas work

One pi-rukas `/work` cycle, headless, in the sandbox image.

```
tehas work <issue> [--restart]
```

It starts `pi --mode rpc --approve` in a container, sends `/work <issue>` and
waits until `.pi/work-state/<issue>.json` in the checkout leaves `running`.

| Exit | Meaning                                                          |
| ---- | ---------------------------------------------------------------- |
| 0    | The cycle ended as `merged` or `handoff` (parked for a human)    |
| 1    | The cycle ended as `aborted`                                     |
| 2    | Pi exited, or the time limit passed, while the cycle was running |
| 3    | The sandbox cannot push to the repository; no cycle was started  |
| 64   | No bot token or no git identity; no cycle was started            |
| 78   | The instance settings are invalid; no cycle was started          |

The container gets the bot token and the `PI_ENSEMBLE_*` settings, a CPU and
memory limit, and no Docker socket or SSH keys. git inside it is given the bot's
identity and a credential helper that reads the token, through environment-level
configuration; before a cycle starts, a dry-run push proves that works. The RPC
stream is kept in `~/tehas/logs/work-<issue>-<time>.jsonl`. The limits are
`TEHAS_WORK_CPUS`, `TEHAS_WORK_MEMORY` and `TEHAS_WORK_TIMEOUT`.

The bot's git identity is the tehas user's global `user.name` and `user.email`
on the host.

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
| 78   | The instance settings are invalid, or the clone's directory is not a clone; no container  |

Arguments are checked before the settings are read, so `--help` works on a
host with no instance and a mistyped argument makes no clone.

The container is the one `tehas work` starts, with a terminal: the same image,
limits, mounts, token and settings. It is named `tehas-dev-<name>`, so a clone
has one session at a time. The command does not take the dispatcher's lock and
writes nothing under `~/tehas/live`. It needs a terminal; Docker refuses to
start it without one.

After the clone is made the host runs no git command in it, because agents
have written there since.

## tehas index

Builds the code index that a cycle's agents search, or refreshes it.

```
tehas index                  # build the index
```

pi-rukas tells its agents to find code with the `codebase_memory` tools. They
answer from an index, and nothing builds one on its own: interactive pi-rukas
does it in `/start`, which a headless cycle never runs. Without an index every
such search fails and the agents fall back to reading files.

The index is kept under the path of the tree it was made from, in the cache
volume every cycle mounts. So the command runs the indexer in the sandbox
image, over a clone mounted at the path a cycle works in. The clone is made at
the tip the product checkout last fetched and is removed afterwards. The
cycle's workspace is not touched.

| Exit | Meaning                                                |
| ---- | ------------------------------------------------------ |
| 0    | The index was built, or was already current            |
| 1    | The sandbox image is missing, or the indexer failed    |
| 3    | A cycle is running; nothing was done                   |
| 64   | An argument this command does not know                 |
| 78   | The instance settings are invalid; nothing was done    |

The container gets the clone, read-only, and the cache volume: no network, no
token and none of the settings. It runs under the cycle's CPU and memory
limits. The indexer's answer is kept in `~/tehas/logs/index-<time>.log`, and
`~/tehas/state/index.json` records the commit that was indexed.

`install.sh` runs it once, the first time it finds the sandbox image and the
product checkout and no index. After that the dispatcher runs the script with
`--before-cycle` ahead of every cycle: the lock is the dispatcher's own then,
and nothing is done when the recorded commit is already the branch's tip. So a
cycle starts with an index of the tree it is cloned from. What the cycle
changes afterwards, in its workspace or its worktrees, is not in the index.

Run by hand it always rebuilds, and it holds the dispatcher's lock while it
does, so no cycle starts in the middle. A cycle started by hand with
`tehas work` refreshes nothing.

## tehas labels

```
tehas labels        # one line per label
```

Creates the six labels tehas uses in the product repository: `tehas:ready`,
`tehas:urgent`, `tehas:running`, `tehas:awaiting-merge`, `tehas:pause` and
`needs-human-attention`. Run it once for a new product. Run again, it puts
their colours and descriptions back. It needs `TEHAS_REPO` and no other
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
```

Prints the state of the factory from the [health file](#tehasbot-health): is
it taking work, what is in the queue, and the current or last cycle. It reads
that one file and nothing else: no GitHub call, no instance settings, and it
writes nothing. So it is as fresh as the dispatcher's last poll, and the first
line says how old that is.

A stopped dispatcher cannot report itself: its file just stops changing. A
file older than ten minutes is therefore not believed. The first line then
starts `factory   stale:` and asks whether the timer is running.

Exit 0 on `pass` and `warn`. Exit 1 on `fail`, on a stale file, or when there
is no health file yet. Exit 64 on any argument.

## tehasbot

What the units run. Nothing here is needed in daily use; it is the place to
look when a timer or service has to be run once by hand.

```
tehasbot dispatch             # one poll; what tehas-dispatch.timer runs
tehasbot status               # write the queue snapshot
tehasbot health               # write the health file
tehasbot energy [--every]     # sample the model mesh; what tehas-energy.service runs
tehasbot protolab             # build the Protolab; what tehas-protolab.timer runs
```

The names of the units did not change with the commands: they are still
`tehas-dispatch.timer`, `tehas-energy.service` and `tehas-protolab.timer`, and
a cycle's container is still `tehas-work-<issue>`.

## tehasbot status and the console

Feed the [factory floor](/) and the [plain dashboard](/dashboard). `tehasbot status` writes the queue snapshot to
`~/tehas/live/status.json`; the dispatcher runs it at every poll and every two
minutes during a cycle. `~/tehas/libexec/console.ts` is started by the work
script and follows the cycle's agents into `~/tehas/live/console-<issue>.txt`, masking
tokens, passwords and private network addresses. The mask catches accidents;
it cannot stop an agent that sets out to print a secret another way, so treat
the console of a published dashboard as public. Neither needs running by hand.

## tehasbot health

```
tehasbot health     # prints nothing; refreshes ~/tehas/live/health.json
curl -i http://tehas-tehas:8080/health
```

Writes the factory's health from what is already on disk: the queue snapshot
and the current or last cycle. The status script runs it on exit, so it is
refreshed at every poll and every two minutes during a cycle. The docs server
serves the file at `/health` and `/healthz` as `application/health+json`.
`tehas status` prints the same file for a person.

| `status` | Meaning                                                                                  | HTTP |
| -------- | ---------------------------------------------------------------------------------------- | ---- |
| `pass`   | Taking work, idle or in a cycle                                                          | 200  |
| `warn`   | Paused (`tehas:pause` on the control issue)                                              | 200  |
| `fail`   | The last cycle ended as anything but `merged` or `handoff`, or there is no queue snapshot | 503  |

`output` says the same in a sentence. `checks.queue` has the snapshot time and
the counts in line, building, in review and stopped; `checks.cycle` has the
issue, status, step and times of the current or last cycle.

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
section shows: per machine the draw, kWh and RAM; per model the slots and how
many are busy; and the last 30 minutes of draw, with the mesh's running count
of tokens for the factory's models where it reports one. Only machines that are alive
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
open the dashboard and names the machines, their draw and their models.

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
. ~/tehas/env.sh && docker compose -f ~/tehas/edge/protolab.compose.yaml up -d
```

Builds the Protolab in `~/tehas/protolab`: a page for every open issue
labelled `tehas:prototype`, with the files attached to it, and the list
`~/tehas/live/protolab.json` that the dashboard links from. The timer runs it
every five minutes; the log is `~/tehas/logs/protolab.log`. The container
serves the directory on port 8081 and does nothing else: it reads its mounts,
has a read-only root and no way to write to the host.

It fetches only links to GitHub's attachment storage, in text written by an
owner, member or collaborator of the repository, and only the types listed
under [Operating](/operating). A ticket gets 10 megabytes and 40 files. An
issue that has not changed since the last pass is skipped, so a quiet pass is
one GitHub call. When GitHub cannot be reached, what is there stays.

A prototype is somebody's HTML and script. That is why the Protolab is a
server of its own on another port, and why every attached file is answered
with a sandbox policy: the browser gives it an origin with nothing in it. To
publish the Protolab outside the private network, give it a host name of its
own, never a path under the dashboard's, and set `TEHAS_PROTOLAB_URL`.

Without the timer and the container there is no Protolab, and the panel on the
factory floor says so.

## The gates (run by the cycle, usable by hand)

A product names its gates in `.pi/verify-cmd`, `.pi/verify-cmd-full` and
`.pi/smoke-cmd`: the first line that is not empty or a comment is the command.
Run them by hand from the root of a clone of the product. For the example
product:

```
scripts/verify.sh          # fast: the lint, shellcheck, the shell tests
scripts/verify.sh --full   # plus deno lint over the TypeScript
```

## install.sh

Installs tehas on the host from the platform checkout, for the instance the
host points at. See [The host](/host).

```
cd ~/tehas/platform
./install.sh --check     # change nothing; Instance, Dependencies, Installed, Tests
./install.sh
```

Exit 0 when done or nothing to report, 1 when `--check` found something or a
step failed, 78 when the instance settings are invalid (nothing was changed).

A real run also builds the code index with [tehas index](#tehas-index), once:
when there is none yet and both the sandbox image and the product checkout are
there. Otherwise it prints the command as a follow-up. An indexing that fails
is a warning and does not fail the install.

It also removes the commands of the earlier layout from `~/tehas/bin`, unless a
cycle is running that still uses them. See [The host](/host).

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
