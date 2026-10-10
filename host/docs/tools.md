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
tehas update [--check]           # bring this host up to date with the platform
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
minutes during a cycle. `~/tehas/libexec/console` is started by the work
script and follows the cycle's agents into `~/tehas/live/console-<issue>.txt`, masking
tokens, passwords and private network addresses. The mask catches accidents;
it cannot stop an agent that sets out to print a secret another way, so treat
the console of a published dashboard as public. A log line of over 1 MiB is
left out, and the console says how many were. Neither needs running by hand.
What the console itself complains about goes to `~/tehas/logs/work-<issue>-<time>.jsonl.console`,
and the work script says so when the console is missing or ends with an error.

The snapshot also carries the words of the breakroom game, Buzzword Buster.
They are counted over the titles and labels of every issue of the repository,
the newest thousand, not only the ones the factory was given: work going on
beside the factory shows up there. The file holds the hundred most used words
with their counts, and no title or issue number. On a published dashboard
those words are public too. If GitHub does not answer that call, the snapshot
is written without them and the game counts the tickets on the line. A round
shows at most twelve of the forty most used words, picked by chance, so the
wall is another one every time.

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
scripts/verify.sh --full   # plus the console's tests (cargo) and the front page's (node)
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
| `protolab --force-recreate` | `edge/protolab.*` changed            | its container is running: recreated with that flag                 |

Both servers are recreated rather than just brought up, because what changed is
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
--check`, which reports the checkout, the installed files, any follow-up an
earlier run left undone, and the tests; it merges nothing, installs nothing,
builds nothing and starts nothing, and exits 1 when there is something to update.

| Exit | Meaning                                                                       |
| ---- | ----------------------------------------------------------------------------- |
| 0    | Updated, or already up to date                                                 |
| 1    | The checkout has local changes, is not on a branch, cannot fast-forward, has no commit yet, a step failed or ran out of its limit, `install.sh` ran out of its limit or gave no answer, a follow-up install.sh asked for is one this command does not know, systemd did not reload the unit files, a container or unit gave no answer, the record of steps owed exists but cannot be read, a status of `install.sh` this command does not know (named in the message), or `--check` found something to update (a checkout behind, or steps left over from an earlier run) |
| 64   | More than one argument, or an argument this command does not know              |
| 78   | The instance settings are missing or malformed, or there is no platform checkout |

## install.sh

Installs tehas on the host from the platform checkout, for the instance the
host points at. See [The host](/host).

```
cd ~/tehas/platform
./install.sh --check     # change nothing; Instance, Dependencies, Installed, Follow-ups, Tests
./install.sh
```

Exit 0 when done or nothing to report, 1 when `--check` found something or a
step failed, 78 when the instance settings are invalid (nothing was changed).

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
