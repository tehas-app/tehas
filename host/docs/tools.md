---
title: Commands
description: "Every tehas command and what it does."
---

# Commands

On the host as the tehas user (in the example, `boss` on `tehas-tehas`).
Interactive shells get the environment from `~/tehas/env.sh`; a non-interactive
`ssh boss@tehas-tehas '<command>'` must source it first. The scripts live in
`~/tehas/bin`, installed from `bin/` in the platform repository.

Every command loads the instance settings first and stops with exit 78 and one
line if a setting it needs is missing or malformed. See [The host](/host).

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
| `TEHAS_PLAN_MAX_TURNS`          | `20`                                     | Turns a planning session may take before it is ended            |
| `TEHAS_PLAN_TIMEOUT`            | `3600`                                   | Seconds before one planning turn is given up                    |
| `PI_ENSEMBLE_HOST_ALIASES`      | empty                                    | `name:address` pairs the sandbox must resolve, comma separated  |
| `PI_ENSEMBLE_IMAGE`             | `tehas/pi-rukas:uid<uid>`                | The sandbox image                                               |

The example instance sets the five required ones, `TEHAS_HOST_ADDR`,
`PI_ENSEMBLE_HOST_ALIASES`, and, because it is public, `TEHAS_INDEXING` and the
analytics settings. It leaves the rest at the defaults.

## tehas-dispatch

One poll of the issue queue. Deterministic, no model.

```
tehas-dispatch --dry-run       # report the decision, change nothing
tehas-dispatch                 # run at most one cycle, then return
tehas-dispatch --init-labels   # create the tehas labels in the repository
tehas-dispatch --help          # print the usage of the command, call nothing
```

`--help` (or `-h`) prints the header comment of the command and exits 0; it
works even on a host with no instance settings yet. Any other argument, or more
than one, is refused: exit 64 with `tehas-dispatch: unknown argument: <argument>`
and the usage on standard error, before the instance settings are read. A mistyped
`--dryrun` therefore starts no poll and changes nothing.

Order of rules: pause label on the control issue, the cap on issues awaiting
merge, who applied `tehas:ready`, then urgent first and lowest number. Log:
`~/tehas/logs/dispatch.log` when run by systemd, the terminal otherwise. Each
cycle's runner output: `~/tehas/logs/dispatch-work-<issue>.log`.

Before a cycle it puts the product checkout on the default branch and
fast-forwards it, then refreshes the code index with
[tehas-index](#tehas-index) when the branch has moved since the last indexing.
`tehas-work` then clones the cycle's workspace from the checkout. An index
that cannot be refreshed is a warning in the log; the cycle still runs.

The labels `tehas:plan`, `tehas:planning` and `tehas:question` exist in a
repository only after `tehas-dispatch --init-labels` has been run for the
version that uses them. After pulling a version that adds a label, run it
again on the host. It only creates or updates labels, so running it twice is
safe. Without it the labels are not there to apply, and a stopped cycle cannot get
its question label. The bot then says in a second comment that the boxes
cannot be used.

Beside the queue, a poll also reads two kinds of answer. A box ticked by an
allowed account in the bot's comment on a stopped cycle can start a fresh
cycle, and a reply or a ticked box on a planning question runs the next turn of
that session. A comment is a question only if the host has its id on record:
in `~/tehas/state/plan-<issue>.json` for a planning session, in
`~/tehas/state/park-<issue>.id` for a stopped cycle. Only the last 100
comments of an issue are read; a question further back than that is skipped
with a line in the log. See [Operating](/operating). The dispatcher stops with exit 78 if
the bot's own login is in `TEHAS_ALLOWED_LABELERS`. It checks this when it
reads a question, so the error shows once a question is open. A planning turn's
runner output is in `~/tehas/logs/dispatch-plan-<issue>.log`.

An issue is taken only when an allowed account applied `tehas:ready` and
nobody else has changed its title or body since. A cycle starts only once the
issue is marked `tehas:running`; if that label change fails, the poll stops.

## tehas-work

One pi-rukas `/work` cycle, headless, in the sandbox image.

```
tehas-work <issue> [--restart]
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

## tehas-plan

Runs one turn of a planning session, headless, in the sandbox image. The
dispatcher calls it. By hand it is useful to see what a turn does.

```
tehas-plan 12 --out /tmp/turn.json --brief brief.md
tehas-plan 12 --out /tmp/turn.json --session <id> --answer answer.md
```

The first form starts a session from a file with the issue's title and body.
The second reopens the session and sends the file's content as the operator's
answer. A turn ends when the agent has settled, so it ends when the agent has
asked. On exit 0 the `--out` file holds the session id, the agent's last
message and the question it asked, if any. On exit 2 it holds the session id
alone when that is known, so the dispatcher can run the turn again in the
same session, and it is not written when no session is known.

| Exit | Meaning                                                              |
| ---- | -------------------------------------------------------------------- |
| 0    | The turn settled and said or asked something                         |
| 2    | The turn did not settle, timed out, or said and asked nothing        |
| 64   | A usage error, no bot token or no git identity; nothing ran          |
| 78   | The instance settings are invalid; nothing ran                       |

The container gets the same mounts and variables as a work cycle's, from
`lib/sandbox.sh`: the bot token, and no Docker socket, SSH keys or
`~/.config/tehas`. The agent files issues with the token. The RPC stream is
kept in `~/tehas/logs/plan-<issue>-<time>.jsonl`. The limit is
`TEHAS_PLAN_TIMEOUT`, and the session record is
`~/tehas/state/plan-<issue>.json`.

## tehas-index

Builds the code index that a cycle's agents search, or refreshes it.

```
tehas-index                  # build the index
tehas-index --before-cycle   # what tehas-dispatch runs
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
product checkout and no index. After that the dispatcher runs it with
`--before-cycle` ahead of every cycle: the lock is the dispatcher's own then,
and nothing is done when the recorded commit is already the branch's tip. So a
cycle starts with an index of the tree it is cloned from. What the cycle
changes afterwards, in its workspace or its worktrees, is not in the index.

Run by hand it always rebuilds, and it holds the dispatcher's lock while it
does, so no cycle starts in the middle. A cycle started by hand with
`tehas-work` refreshes nothing.

## tehas-status and tehas-console.ts

Feed the [factory floor](/) and the [plain dashboard](/dashboard). `tehas-status` writes the queue snapshot to
`~/tehas/live/status.json`; the dispatcher runs it at every poll and every two
minutes during a cycle. `tehas-console.ts` is started by `tehas-work` and
follows the cycle's agents into `~/tehas/live/console-<issue>.txt`, masking
tokens, passwords and private network addresses. The mask catches accidents;
it cannot stop an agent that sets out to print a secret another way, so treat
the console of a published dashboard as public. Neither needs running by hand.

## tehas-health

```
tehas-health        # prints nothing; refreshes ~/tehas/live/health.json
curl -i http://tehas-tehas:8080/health
```

Writes the factory's health from what is already on disk: the queue snapshot
and the current or last cycle. `tehas-status` runs it on exit, so it is
refreshed at every poll and every two minutes during a cycle. The docs server
serves the file at `/health` and `/healthz` as `application/health+json`.

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

## tehas-energy

Optional. It feeds the Energy use section of the factory floor and nothing
else: no command reads its file, and a factory without it dispatches and works
the same. It is useful only when the model backend is a viiwork mesh. The
command and its unit are installed on every instance, but the service runs
only if you enable it.

```
tehas-energy            # one sample; prints nothing; writes ~/tehas/live/energy.json
tehas-energy --every    # one sample at every 15-second mark; what the service runs
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

## tehas-protolab

```
tehas-protolab          # one pass; a line per ticket built or removed
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

A real run also builds the code index with [tehas-index](#tehas-index), once:
when there is none yet and both the sandbox image and the product checkout are
there. Otherwise it prints the command as a follow-up. An indexing that fails
is a warning and does not fail the install.

## bootstrap.sh

Prepares a fresh host with everything tehas and pi-rukas need, at the versions
in `host/versions.env`. See [Bootstrap a host](/bootstrap).

```
sudo ./bootstrap.sh root boss   # system packages, Docker, docker group, lingering
./bootstrap.sh user             # the rest, under the user's home
./bootstrap.sh check            # change nothing; found and expected version per tool
```

## tehas-test

The checks of the platform itself. Run from the platform checkout.

```
bin/tehas-test
```

It runs shellcheck over the shell scripts, then every `test/*_test.sh`. Those
need bash, git, jq and shellcheck only: no sandbox image, no product checkout
and no network. Exit 0 when every check passed, 1 when one failed, 2 when
nothing could be checked.

`scripts/check-generic.sh` is the lint that keeps private network addresses out
of every file, and product and host names out of platform files. The names
come from `scripts/check-generic.names`, a list each working machine keeps for
itself: git ignores it and it is read as data. `scripts/verify.sh` runs the
lint and `bin/tehas-test` together.
