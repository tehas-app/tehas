---
title: The host
description: "How a tehas factory is laid out on its machine, and the sandbox contract test."
---

# The host

One Linux machine and one Unix user. In the example: `tehas-tehas`, user `boss`
(`ssh boss@tehas-tehas`), 8 CPUs and 15 GB of memory. That user is in the
`docker` group, which makes it root-equivalent on the host. The boundary that
matters is the sandbox: agents only ever run inside it, without the Docker
socket.

Preparing a fresh machine is on the [bootstrap page](/bootstrap).

## Layout

| Path                                 | Content                                                                 |
| ------------------------------------ | ----------------------------------------------------------------------- |
| `~/tehas/env.sh`                     | Environment for every tehas process; sourced from `~/.bashrc`           |
| `~/tehas/lib/config.sh`              | Loads and checks the instance settings; sourced by every command        |
| `~/tehas/lib/sandbox.sh`             | What a sandbox container is given; sourced by the work, plan and dev scripts |
| `~/tehas/lib/slice.sh`               | Reads the slice: whether it is usable, a container's limits, the room left. Every command that starts a sandbox container reads it |
| `~/tehas/lib/questions.sh`, `planning.sh` | The question and planning steps of the dispatcher. A cycle's report uses the first too |
| `~/tehas/bin/`                       | The two commands on `PATH`: `tehas` for a person, `tehasbot` for the units. Plus `gh` |
| `~/tehas/libexec/`                   | The scripts behind the two commands, one for each thing they do. Not on `PATH` |
| `~/tehas/platform`                   | Checkout of this repository; `install.sh` runs from it and `tehas update` pulls it |
| `~/tehas/repos/tehas`                | The product checkout, on the default branch. Cycles are cloned from it; no container is given it |
| `~/tehas/work/tehas-<issue>`         | The cycle's workspace: a fresh clone made for each cycle. Removed after a cycle that merged or left a pull request, otherwise kept, and moved to `tehas-<issue>.before-retry` when that issue is retried. `~/tehas/work/tehas` is only its path inside the container |
| `~/tehas/dev/<name>`                 | A person's clone for working by hand, named after their account, made and used by `tehas dev`, never removed. `<name>.author` next to it holds whose name its commits carry |
| `~/tehas/repos/pi-rukas`             | pi-rukas, pinned to a commit                                            |
| `~/tehas/docs/`, `~/tehas/fun/`      | This site's pages and the front page, installed from `host/`            |
| `~/tehas/sandbox/`, `versions.env`   | Sandbox image definition, build script, and the pinned tool versions    |
| `~/tehas/edge/`                      | The docs server (`docs.compose.yaml`) and the edge router (`router.compose.yaml`) |
| `~/tehas/protolab/`                  | The Protolab's pages and the files of each prototype, written by `tehasbot protolab` |
| `~/tehas/preview/`                   | The state of each preview, and a bare clone the pull requests' commits are fetched into, written by `tehasbot preview` |
| `~/tehas/logs/`, `state/`, `live/`, `tmp/` | Logs, state, dashboard data, scratch. `state/` holds the dispatch lock, status files, the record of each planning session (`plan-<issue>.json`) and the id of each parked cycle's question (`park-<issue>.id`). No container is given it |
| `~/tehas/tmp/improve/`, `~/tehas/state/improve.lock` | Where the improvement loop is on: the record of each cycle that ended, until a run has read it, and the lock of a run |
| Docker volume `pi-ensemble-cache`    | Caches of the sandbox, among them the code index `tehas index` builds   |
| `~/.pi/agent/`                       | Pi user configuration (models, MCP servers)                             |
| `~/.config/tehas/`                   | Secrets and the `instance-dir` pointer. Never in git                    |

Where the improvement loop is on, each of its runs removes the per-cycle files
in `~/tehas/logs/` that are older than seven days (`work-*`, `dispatch-work-*`,
`index-*`), and the records that old under `~/tehas/tmp/improve/`. Nothing else
removes logs.

In the example the platform and the product are the same repository. They are
still two clones. A merged pull request changes the running factory only when a
person pulls `~/tehas/platform` and runs `install.sh`.

## Instance settings

Everything that differs between instances is in the instance directory:

```
<instance dir>/
  instance.env     settings, shell KEY=value lines, no secrets
  models.json      Pi provider and model list, installed to ~/.pi/agent/
  settings.json    optional: Pi's own settings for this host
```

The directory is found in this order: `TEHAS_INSTANCE_DIR` in the environment,
the one-line file `~/.config/tehas/instance-dir`, then `~/.config/tehas` itself.
The example points at a directory in the platform checkout:

```
cat ~/.config/tehas/instance-dir
/home/boss/tehas/platform/instances/tehas-tehas
```

Rules that hold for every command:

- `instance.env` wins. A setting of the same name in the environment is
  dropped. Only `TEHAS_HOME`, `TEHAS_INSTANCE_DIR` and three hooks the tests
  use are taken from the environment.
- A missing or malformed setting stops the command with exit 78 and one line
  naming the setting, before it calls GitHub, Docker or git.
- Loading settings runs no git command, makes no network call and writes
  nothing.
- `instance.env` is shell, sourced as written. Keep it writable by the tehas
  user only. `./install.sh --check` reports it if it is not.

The settings and their defaults are listed on the [commands page](/tools) and,
with comments, in `instances/example/instance.env`.

## Configuration comes from git

The platform repository is the source. Change it there, merge, then on the
host:

```
tehas update --check     # report what would change; changes no file of the
                         # checkout or of the host (origin/ refs do move)
tehas update             # pull, install, run the follow-ups that apply
```

`tehas update` is the three steps that took hand work before: fast-forward
`~/tehas/platform` from its remote, run `install.sh`, then run each follow-up it
asks for by name (`units`, `sandbox`, `docs`, `edge`: see [Commands](/tools)).
One line per step, and it stops at the first failure. It checks the instance
settings against the required list in `lib/config.sh`, the same one install.sh
checks, before it calls git. It changes nothing when the checkout has local
changes or cannot be fast-forwarded, and it never enables a unit or starts a
container the host was not already running: a changed unit is reloaded and
restarted when it was active, and a docs or edge router container is recreated when
it is up - Caddy reads its config when the process starts, so a changed Caddyfile
needs the container brought back up. Asking Docker or systemd whether one runs and
getting no answer stops the update; only a real answer counts, not an exit status
on its own, and a systemd that did not reload the unit files stops the run rather
than report a changed unit as applied. A step that fails leaves the ones before it
applied, and what is still owed is kept in `~/tehas/state/` (`update-pending`)
until it has run, so the next update finishes it instead of calling the host
current.

The steps are still there on their own. `install.sh` is what puts tehas on a host
the first time, before `tehas update` exists there:

```
cd ~/tehas/platform && git pull
./install.sh --check     # change nothing, report up to seven results
./install.sh             # copy changed files into place, keep <name>.prev
```

A version that adds labels needs `tehas labels` run again after the install.
The labels `tehas:plan`, `tehas:planning` and `tehas:question`, for planning on
an issue and the retry tick, are not in a repository until it has been.

`--check` prints one line each for the instance settings, the host's tools, the
slice, the installed files, the command the other accounts type where root
installed one, the follow-ups an earlier run left undone, and the tests. Two of
the seven appear only where there is something to say, and the sample below is a
host with a follow-up owed and no root-installed copy:

```
Instance:     /home/boss/tehas/platform/instances/tehas-tehas OK
Dependencies: OK
Slice:        MISSING (sudo ./bootstrap.sh slice)
Installed:    DRIFT (1 files)
  differs: /home/boss/tehas/libexec/dispatch
Follow-ups:   NOT DONE (2 from an earlier run, see /home/boss/tehas/state/update-pending)
Tests:        OK
```

The `Follow-ups:` line appears only while `~/tehas/state/update-pending` still
holds steps that were asked for and never ran - and while it does, `--check`
exits 1, because a host with files matching and a step unapplied is not current.
Running `tehas update` runs them and clears the record
([Troubleshooting](/troubleshooting) has the case in full).

One file of a host is not tehas's to install. `sudo ./bootstrap.sh dev` puts
`host/login/tehas` at `/usr/local/bin/tehas` as root, and an update can change
that source while the copy on the host stays as it was. When the two differ,
`--check` adds a `Login:` line naming the command that installs this checkout's
copy, and exits 1; a real install prints the same finding as a `later:` line.
`install.sh` writes nothing under `/usr/local/bin` or `/etc/tehas` and asks
`sudo` for nothing, so the difference is reported rather than repaired. A host
where `bootstrap.sh dev` never ran says nothing about it. See
[Other accounts](#other-accounts).

The `Slice:` line compares `/etc/systemd/system/tehas.slice` with
`host/slice/tehas.slice` in the checkout: `OK`, `MISSING` or `DIFFERS`. The
last two make `--check` exit 1, and so `tehas update --check` stays red, until
root has run [`sudo ./bootstrap.sh slice`](/bootstrap#slice): every sandbox
container runs in the slice and none starts without it. A real run prints a
`note:` line for the last two, installs everything else and installs no slice,
which is root's step.

`install.sh` validates the settings first and installs nothing if they are
invalid. It prints one `next:` line per follow-up its changed files call for, and
restarts nothing itself; `tehas update` is what runs them, and it records each of
them as owed while it copies the file that calls for it - which is why a run that
dies partway does not leave the host looking current. It asks systemd for nothing,
but for the one case below:
a copied unit file is not in effect until systemd has reloaded it, and that reload
belongs to the step that restarts the units. A file edited directly on the host
shows up as drift in `--check`.

One file is written, not copied: `version.txt`, beside the pages in
`~/tehas/docs` and `~/tehas/fun`. It holds the newest tag of the checkout and
the commit it was at when `install.sh` last ran, as `v0.1.1+11b669d`, and the
footer of every page shows it. `--check` does not look at it.

One installed file is compiled, not copied: the console of a running cycle, a
Rust program in `console/`. A real run builds it with `cargo` in the checkout
and installs it as `~/tehas/libexec/console`. Beside it, `console.stamp`
holds two checksums: of the source it was built from and of the program. It
is built again only when the source has changed or the program on the host is
not the one that was installed. The first build fetches its crates from the
network and takes a minute. `--check` compares the checksums and builds
nothing, so a console replaced on the host shows up as drift like any other
file. A console that does not build stops the install before it has changed
anything, and so does a host without `cargo`. The build is given no
environment variable whose name holds TOKEN, SECRET, PASSWORD or API_KEY. The Deno
script it replaces, `~/tehas/libexec/console.ts`, is removed the way the old
commands below are.

Before there was a `tehas` command, `~/tehas/bin` held one file for each
command (`tehas-dispatch`, `tehas-work` and so on). `install.sh` removes those
and `--check` lists them as stale. A cycle that was running before the install
still calls them by name. So the install removes them only while it holds the
dispatcher's lock itself. When a cycle or a poll holds it, they are left in
place and the install prints `later: ./install.sh again when the running cycle
has ended`. The units systemd holds name them too, so this is the one case
where the install asks systemd to reload, and it leaves the files and prints a
`later:` line when the reload did not happen. A `later:` line is for a person
and is not a step: `tehas update` prints it and finds the files again on its
next run. Removing the old energy sampler asks for the `units` step, which
restarts a running one from the new file. An old command that cannot be
removed fails the install.

One thing it does besides copying, on a real run only: the product checkout's
`origin/HEAD` is refreshed, so the commands know the default branch without
asking the network. To pin another branch, set `TEHAS_BRANCH`. If the
repository's default branch changes later, run `install.sh` again.

## Models

Three files name a model and they do different things:

| Where                                                    | Controls                                                         |
| -------------------------------------------------------- | ---------------------------------------------------------------- |
| `.pi/settings.json` in the product repository            | Provider and model of the session, the agent driving the cycle   |
| `PI_ENSEMBLE_SUBAGENT_MODEL`, `_PROVIDER` in `instance.env` | Every subagent role                                           |
| `models.json` in the instance directory                  | What the provider is and which models it offers                  |

Child agents do not inherit the session's model. Without the instance setting
they fall back to the first model in `models.json`, which may be a small one.
Changing the instance setting does not change the session's model.
`./install.sh --check` verifies that both named models are listed in
`models.json`.

In the example every role and the session use `qwen3.8-flash-next` on the
provider `fleet`. One role differently: `PI_ENSEMBLE_MODEL_<ROLE>` and
`PI_ENSEMBLE_PROVIDER_<ROLE>` in `instance.env`.

What the endpoint serves right now:

```
curl -s "$(jq -r '.providers.fleet.baseUrl' ~/.pi/agent/models.json)/models" | jq -r '.data[].id'
```

A host name that only resolves on a private network is not known inside the
sandbox. Give the sandbox its address with `PI_ENSEMBLE_HOST_ALIASES`
(`name:address`, comma separated).

## Limits and concurrency

Every sandbox container tehas starts runs in one systemd slice,
[`tehas.slice`](#the-slice): a cycle, a planning turn, a `tehas dev` session,
the indexer and the improvement loop's analyst. The slice's `MemoryMax` bounds
them all together. No setting describes the machine.

A cycle, a planning turn, a dev session and the indexer each get the same six
limits, twelve words on the `docker run` line:

| Option | Value | What it does |
| --- | --- | --- |
| `--cgroup-parent` | `tehas.slice` | Puts the container in the slice |
| `--memory` | a quarter of the slice's `MemoryMax` | A hard ceiling. At it the kernel kills the largest process in the container. Four containers have to run away to fill the slice on their own |
| `--memory-swap` | the same value | No swap for the container |
| `--memory-reservation` | `TEHAS_CYCLE_RESERVE`, `2g` unless set | The container's floor. When the slice is full the kernel frees memory above the floors first |
| `--pids-limit` | `4096` | The most processes and threads in the container |
| `--cpu-shares` | `1024` | A CPU weight, the same on every container |

There is no CPU quota. One container alone may use every CPU; several busy
ones share them by weight, about equally. The weight is also what makes the
kernel share CPU inside the slice at all, so every container carries it.

The floor is a priority when the slice frees memory, not a guarantee against
the whole host: when the host itself runs short, the host comes first. With
the default reserve the host needs about 11 GiB of memory, because the reserve
may not be more than a container's ceiling. A smaller host sets
`TEHAS_CYCLE_RESERVE` lower.

The analyst is a batch job with limits of its own: the slice, one CPU
(`--cpus 1`), `--memory 2g`, the same for `--memory-swap`, `--pids-limit 1024`
and the same CPU weight. The check that the sandbox can push, which `tehas
work` runs for a few seconds before a cycle, is not in the slice and has no
limits. Previews, image builds and the edge router are outside the slice too.

Nothing starts outside the slice. `tehas work`, `tehas plan`, `tehas dev`,
`tehas index` and `tehasbot improve` ask once whether the slice is usable:
the slice is loaded from its file and has a `MemoryMax`, Docker uses the
systemd cgroup driver on cgroup v2, the `cpu`, `memory` and `pids` controllers
are there, and the reserve fits under the ceiling. If not, the command stops
with exit 78 and one line that says what is missing, before it clones
anything, calls GitHub or starts a container:

```
tehas work: no slice file on this host (sudo ./bootstrap.sh slice)
```

A cycle's container has its id written to `~/tehas/state/cycles/<issue>.cid`
while it runs. That is how the cycle's container is told from the others in
the slice.

`tehas dev` prints how many more cycles the slice has room for before it
starts its session. The session's memory counts in the slice like any other
container's, so a session that uses a lot leaves room for fewer cycles.

#### A container that is killed

Memory is the one thing a container is killed for. A container that grows
reaches its own ceiling first, and the kernel kills the largest process in it.
When several grow and the slice reaches `MemoryMax`, the kernel frees file
cache, the floors last, and then kills the largest process in the slice. If
the process killed is a build step, the cycle goes on with a failed step. If
it is Pi itself, the container ends with status 137.

`tehas work` reads the container's own count of such kills while the cycle
runs. A count above 0 is in its output and in the comment that reports the
cycle:

```
tehas work: 2 processes in this cycle's container were killed for memory
```

A container that ends with 137 while the cycle was still running is reported
as `killed`: `tehas work` exits 137, and the issue is parked with
`needs-human-attention` and status `killed` in the comment. That is a
container that ran out of memory, or one that was stopped from outside, for
example with `docker kill`. It is not `killed` when `tehas work` stopped the
container itself, at the time limit or on a signal, nor when the cycle's state
already said how it ended. The record for the improvement loop carries the
count as `oomKills`.

`TEHAS_TICKET_CONCURRENCY` sets the most issues worked on side by side, each
in a cycle, a container and a workspace of its own. It defaults to the host's
CPUs minus one, at least 1, and a value above the host's CPUs is accepted. It
is a ceiling: the dispatcher starts fewer while the slice has no room. A
container's limits do not depend on it. There is no CPU or memory budget to
set; an instance that still sets `TEHAS_WORK_CPUS` or `TEHAS_WORK_MEMORY` is
refused with 78 and one line naming the slice file.

`TEHAS_CONCURRENCY` sets how many workstreams one cycle may run side by side,
inside that one container. It defaults to 4.

A workstream is one developer agent and its review loop in their own worktree.
pi-rukas plans up to that many per issue and folds anything beyond the limit
into the last one, so no work is dropped at a low setting; it is done in
sequence by fewer agents. An issue whose parts touch the same files is planned
as one workstream whatever the limit.

It is a limit inside one cycle. With `TEHAS_TICKET_CONCURRENCY` above 1, up to
that many cycles run at once and each has its own `TEHAS_CONCURRENCY`
workstreams, so the two multiply: 2 tickets and 4 workstreams are up to 8
developer agents calling the model backends. Nothing checks that against what
the backends can serve. Lower either one when the backends have few slots, when
the host is small, or if cycles start timing out.

```
# in the instance's instance.env, then pull and ./install.sh on the host
TEHAS_CONCURRENCY=1
```

It is read when a cycle starts; a running cycle keeps the value it began with.
Two things it does not change:

- the six review passes on the finished pull request still run together
  (pi-rukas's `PI_ENSEMBLE_SPAWN_CAP`, default 64, is the ceiling there),
- how many issues are worked on at once. That is `TEHAS_TICKET_CONCURRENCY`.

Cycles that run side by side know nothing of each other:

- Two issues that touch the same files give two pull requests that conflict.
  Label such issues one after the other.
- The code index is refreshed only when no cycle is running. On a host that
  is never idle it falls behind the branch; the dispatcher's log says so each
  time.
- `~/.vipune`, Pi's `sessions` and the two caches are shared between cycles.

### The slice

`tehas.slice` is one systemd slice for every sandbox container tehas starts.
Root installs it once with [`sudo ./bootstrap.sh slice`](/bootstrap#slice),
from `host/slice/tehas.slice` to `/etc/systemd/system/tehas.slice`.

Every sandbox container is started in it, with the limits above. Without it
no container starts.

Its one limit is `MemoryMax=75%`: everything in the slice together may use
three quarters of the host's memory. It is a share, not a number, so the file
is the same on every host and no setting describes the machine. The other
quarter is for the host: Docker itself, the edge router, the previews, image
builds and the tehas user's own units. `CPUWeight=100` is systemd's default,
written out, so the slice competes for CPU with the rest of the system on
equal terms. There is no CPU quota on it.

What the file leaves out, on purpose:

- No `MemoryHigh`. Over that limit the kernel slows every process in the slice
  and kills nothing, so a full slice would stall. At `MemoryMax` the kernel
  frees file cache and then kills the largest process in the slice.
- No `MemoryLow`. When the whole host is short of memory, the host wins.
- No `ManagedOOM` lines. tehas neither uses nor needs systemd-oomd. Where
  Ubuntu's systemd-oomd package is installed, it ships a setting that lets
  oomd kill a user's own service manager under pressure, the tehas user's
  among them. That is the package's doing, not this slice's.

To read what is in force:

```
systemctl show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax -p CPUWeight
```

`FragmentPath` is the file it was loaded from and `MemoryMax` is a number of
bytes. A slice with an empty `FragmentPath` and `MemoryMax=infinity` is one
systemd made up for a name in use: the root step has not been run. An inactive
slice still answers from its file.

Another limit for one host goes in
`/etc/systemd/system/tehas.slice.d/override.conf`, by root, followed by
`systemctl daemon-reload`. tehas never writes that file.

## Sandbox image

`tehas/pi-rukas:uid<uid>`: the upstream pi-rukas image plus the in-container
user remapped to the tehas user's id, shellcheck, and Playwright's
Chromium with fonts. cargo's registry belongs to that user, so a gate that
builds Rust can fetch its crates; they go with the container. The base is pinned by digest in `sandbox/Dockerfile`, the
tools by version in `versions.env`. A product whose gates need another
toolchain adds it to the Dockerfile.

```
~/tehas/sandbox/build.sh
```

Each build is also tagged with its date, so
`docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid$(id -u)` rolls back.

To upgrade pi-rukas: put the new image's digest in the `FROM` line and its
commit in `versions.env`, rebuild, and run the contract test before any cycle
uses it. Where the image comes from is described under
[pi-rukas comes from a fork](#pi-rukas-comes-from-a-fork). The contract
test proves the product's gates run inside the image, as the unprivileged user,
with nothing mounted but a clone:

```
T=~/tehas/tmp/contract-test
git clone -q ~/tehas/repos/tehas "$T"
docker run --rm --cgroup-parent tehas.slice --memory 4g --memory-swap 4g --user "$(id -u):$(id -g)" \
  -e HOME=/home/vscode -v "$T:$T" -w "$T" --entrypoint bash \
  "$PI_ENSEMBLE_IMAGE" -c '
    sh .pi/worktree-setup &&
    sh -c "$(grep -v -e "^#" -e "^$" .pi/verify-cmd-full | head -n 1)" &&
    sh -c "$(grep -v -e "^#" -e "^$" .pi/smoke-cmd | head -n 1)"'
rm -rf "$T"
```

The container runs in the slice like the factory's own, so the dispatcher
counts its memory while it is there.

## pi-rukas comes from a fork

tehas takes pi-rukas from a [fork](https://github.com/janit/pi-rukas) of
[upstream](https://github.com/trail-openers/pi-rukas), so a fix reaches the
factory when it is ready here, not when upstream releases it. The fork is
never the last stop: every change in it is also offered upstream.

The fork's `main` is what a host runs: upstream's `main` plus the fixes
merged in the fork that upstream does not have yet. `PI_RUKAS_COMMIT` in
`versions.env` is a commit on it, and the base in `sandbox/Dockerfile` is the
fork's image built from the same commit. A fix is written on a branch of its
own and reaches `main` through a pull request in the fork. To take in what
upstream has done, merge upstream's `main` into the fork's.

Problems are tracked in the fork. Where the instance sets
`TEHAS_IMPROVE_RUKAS_REPO` to it, the [improvement loop](/operating#the-improvement-loop)
files what it finds about pi-rukas there, each issue naming the product's
issues whose cycles showed it. No cycle on the product fixes pi-rukas.

When a problem in pi-rukas comes up:

1. Have an issue in the fork that describes it: the loop's, or one you write.
2. Write the fix in the fork, on a branch from `main`, with pi-rukas's own
   tests, and merge it into the fork's `main` with a pull request that names
   the issue.
3. The merge runs the fork's `publish-image` workflow. It publishes
   `ghcr.io/janit/pi-rukas:sha-<commit>`, with the first 12 characters of the
   commit.
4. In this repository, in one pull request, set `PI_RUKAS_COMMIT` to that
   commit and the `FROM` line of `sandbox/Dockerfile` to that image's digest.
   The two always name the same commit, or the host and the sandbox run
   different code.
5. On the host: pull, run `./bootstrap.sh user`, rebuild the sandbox image,
   run the contract test above, then let one cycle run before trusting it.
   To check the fix, apply `tehas:ready` again to a product issue the fork's
   issue names. The loop's next run comments on the fork's issue whether the
   problem came back, with the pi-rukas commit that cycle ran. Close the
   fork's issue when it did not.
6. Only after the fix is verified here, offer it upstream: an issue there and
   a pull request from the branch of step 2, as
   `gh pr create --repo <upstream> --head <fork owner>:<branch>`. Name the
   fork's issue in full, as `owner/name#12`; a bare number is upstream's.
7. When upstream merges the fix, merge upstream's `main` into the fork's and
   repeat steps 3 to 5.

Keep the fork's `main` close to upstream. A change that upstream turns down is
either reworked until they take it or removed from the fork; it does not stay
as a permanent difference.

## Services

| Service     | How it runs                                                | Check                                                  |
| ----------- | ---------------------------------------------------------- | ------------------------------------------------------ |
| This site   | `docker compose -f ~/tehas/edge/docs.compose.yaml up -d`   | `curl -i http://tehas-tehas:8080/health`               |
| Dispatcher  | systemd user timer `tehas-dispatch.timer`, every 2 minutes | `tail ~/tehas/logs/dispatch.log`                       |
| Energy use sampler, optional | systemd user service `tehas-energy.service`, every 15 s. Enable it only when the model backend is a [viiwork](https://github.com/janit/viiwork) mesh | `jq .generatedAt ~/tehas/live/energy.json` |
| Protolab pages | systemd user timer `tehas-protolab.timer`, every 5 minutes | `tail ~/tehas/logs/protolab.log` |
| Improvement loop, optional | systemd user timer `tehas-improve.timer`, every 3 hours. It does nothing unless `TEHAS_IMPROVE_REPO` is set | `tail ~/tehas/logs/improve.log` |
| Edge router | `docker compose -f ~/tehas/edge/router.compose.yaml up -d` | `curl -i http://tehas-tehas:8081/` |
| Previews | systemd user timer `tehas-preview.timer`, every 5 minutes | `tail ~/tehas/logs/preview.log`, `jq . ~/tehas/live/previews.json` |
| Preview sweep | systemd user timer `tehas-preview-sweep.timer`, every 3 hours | `jq .lastSweep ~/tehas/live/previews.json` |
| One preview | a container `tehas-preview-<pr>` | `docker ps --filter label=tehas.preview` |

The docs server binds to `TEHAS_HOST_ADDR`, port 8080. Without the setting it
stays on `127.0.0.1`. `docker compose` reads the address from the environment,
so run it from a shell that has sourced `~/tehas/env.sh`.

The edge router binds to the same address, port 8081, and picks by the first
label of the name it is asked for:

| Name asked for | Answer |
| --- | --- |
| `uat-<pr>.` and anything after it | The preview of that pull request, the container `tehas-preview-<pr>`. With no such container, a 404 that says no preview is running |
| `lab-<issue>.` and anything after it | That ticket's page in the Protolab, from `~/tehas/protolab/<issue>/` |
| Any other name | The Protolab by path: the list at `/`, a ticket at `/<issue>/` |

The number is one to nine digits and does not start with a zero. Another `uat-` or
`lab-` name is a 404. Every answer of the router says `noindex`. Its
configuration is a fixed file and holds no domain: nothing writes it when a
preview comes or goes. It is on two Docker networks: one of its own for the
published port, and `tehas-previews`, an internal one that the previews are
on and the host has no address on. `install.sh` makes that network when
Docker is 27.1 or later, and says so when it is not: an older Docker does
not know the option that keeps the host out, and what it would make was not
tried. Where the network is there already
and does not keep the host out, `install.sh` leaves it and says why and that
it has to be remade. [Troubleshooting](/troubleshooting) has the lines.

Every unit and both containers are started by the operator. `tehas update`
restarts a unit that is already active and recreates a container that already
runs, and adds nothing. So a host installed before previews existed has the
units `tehas-preview.timer` and `tehas-preview-sweep.timer` installed and not
running until they are enabled. The one exception is a host that runs the
Protolab's server under its old compose project, `tehas-protolab`: an update
takes that container down and starts the edge router in its place.
[Previews](/edge) has the steps.

Search engines are told to stay out: every page carries `noindex` and
`/robots.txt` disallows everything. `TEHAS_INDEXING=1` in `instance.env` turns
that off for an instance that is published and meant to be found, and adds
`/sitemap.xml` with the front page and the manual; the live data
under `/live/` stays unlisted either way. The server reads the setting when its
container is created, so run the `docker compose ... up -d` line again after
changing it.

Visitor analytics are optional and off by default. With `TEHAS_ANALYTICS_SRC`
(the https address of a script) and `TEHAS_ANALYTICS_SITE` (the name it reports
under) in `instance.env`, every page of the dashboard and the manual loads that
script, asynchronously. `TEHAS_ANALYTICS_COOKIE=1` adds `data-cookie="true"`
to the tag; a script that then sets a persistent cookie usually needs the
visitor's consent. The example instance uses this for its public pages. The
same `docker compose ... up -d` applies after a change.

## Secrets and trust

- `~/.config/tehas/env`: the bot's GitHub token (`GH_TOKEN`), mode 600.
- `~/.config/tehas/deploy_key`: the read-only deploy key the platform checkout
  is pulled with.

Both are outside the repository and the tehas tree.

| Question                               | Answer                                                                                                                                  |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Who can start a cycle                  | Only an account in `TEHAS_ALLOWED_LABELERS`. The dispatcher checks who most recently applied `tehas:ready`, not just that it is there   |
| What the label approves                | The title and body as they were when it was applied. An issue whose text was changed afterwards by any other account is skipped until the label is applied again |
| What else starts a cycle               | The retry box of a parked issue, ticked by one of those accounts. It counts once, and only in the question whose comment id the host has on record for that issue. Like the label, the tick approves the text as it was at that moment |
| Who can start or continue planning     | The same accounts. A session starts when one of them applied `tehas:plan`. A turn runs on a tick or a reply from one of them to the question the session is waiting on |
| What a planning container is given     | The same token, the same mounts and the same approve-everything mode as a work cycle's. The agent is asked to plan and file issues. Nothing in the container holds it to that |
| What an agent can do as the bot        | Post comments, edit the bot's comments, edit the description of any pull request and apply labels, with the token. So a comment is the factory's question only if the host has its id on record in `~/tehas/state`, which no container is given, and an answer counts only by who gave it |
| Where an agent's conversation is kept  | In Pi's `sessions` directory. Every later container can read and write it                                                               |
| Does the token enter the cycle         | Yes, as `GH_TOKEN`. pi-rukas pushes the branch and opens the pull request from inside. Agent code can do anything the token can         |
| What else enters the container         | `PI_ENSEMBLE_*` settings, the bot's git name and email, the host aliases. Not the rest of the host environment                          |
| Host paths mounted                     | The cycle's own workspace, `~/tehas/work/<name>-<issue>`, at the same path in every container; its own transcripts, `~/.pi/agent/ensemble-runs/issue-<issue>`; `~/.vipune` and Pi's `sessions` (read-write); `models.json`, `mcp.json` (read-only); two caches |
| Not reachable from the container       | The Docker socket, SSH keys, `~/.config/tehas`, the platform checkout, the product checkout, the instance directory                     |
| What the host runs from the workspace  | Nothing. The host runs git only in the product checkout, which no container has written. It reads the cycle's state file from the workspace as data |
| The indexing container                 | `tehas index` runs the indexer, not an agent: a clone of the product read-only, the cache volume, no network, no token                  |
| The improvement loop's container       | `tehasbot improve` runs plain Pi, not pi-rukas: the records of the cycles read-only, one directory to write its findings in, and `models.json`. No token, no repository, no tool that runs a command. The host makes every GitHub call and checks the findings against a fixed shape before it files one |
| The network                            | Not restricted. The container reaches GitHub, the model endpoint and whatever else the host can route to                                |
| Merging                                | The token could merge where the repository lets it. Nothing does, because `AGENTS.md` forbids it and the dispatcher never merges        |

The improvement loop files with the bot's token, from the host. For it the bot
account needs write access to the repository `TEHAS_IMPROVE_REPO` names: it
creates a label, issues, comments, and one release that holds the logs. In the
repository `TEHAS_IMPROVE_RUKAS_REPO` names it creates the label, issues and
comments.
The triage role is enough there once someone has created the label
`tehas:improvement`. Issues must be switched on in that repository: a fork has
them off until its owner turns them on.

Two recommendations follow from that table:

- Give each instance its own bot account, a collaborator on its product only.
  A bot account shared by two instances means a cycle on one product holds a
  token that can write to the other. The example instance shares one for now;
  it is listed as a known limit on the [features page](/features).
- Put a branch protection rule on the product's default branch that requires a
  person's review. That is what enforces "nothing merges on its own" on
  GitHub's side. Requiring one approval is not enough: the bot has write
  access, and its approval of somebody else's pull request counts. Require
  review from code owners and name people, not the bot, in `CODEOWNERS`.
- Apply `tehas:ready` only to an issue whose text you have read. The label is
  the approval of that text.

`test/work_test.sh` in the platform repository pins what the container is
given: the mounts, the variables and every other option of both `docker run`
calls.

### A session by hand

`tehas dev` starts the same container for a person, and the table above holds
for it with these differences:

| Question                    | Answer                                                                                                             |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Who can start one           | The tehas user, and the accounts in the group `tehas`, through one `sudo` rule. See below                           |
| The tree it works in        | `~/tehas/dev/<name>`, kept between sessions, where a cycle gets a fresh clone                                       |
| git identity                | The stored author of that clone, else the bot's. A label: it is not checked against anything                       |
| What else enters            | A terminal, and `TERM` and `COLORTERM` from the shell. Nothing else of the host environment, and not `~/.config/gh` |
| What the host runs from the clone | Nothing. The host runs git there once, to make it, and never again                                           |

`test/dev_test.sh` pins the session's `docker run` the same way. Both commands
take the mounts and variables from `lib/sandbox.sh`, so they cannot differ by
accident.

### Other accounts

`sudo ./bootstrap.sh dev boss alice` lets the account `alice` start a session.
It puts three things on the host, outside any home:

| Path                         | Content                                                                                   |
| ---------------------------- | ----------------------------------------------------------------------------------------- |
| `/etc/sudoers.d/tehas-dev`   | The group `tehas` may run `~boss/tehas/libexec/dev-login` as `boss`, without a password. Nothing else |
| `/etc/tehas/manager`         | One line: the tehas user's name                                                           |
| `/usr/local/bin/tehas`       | The command those accounts type. It holds no setting and no secret                        |

What a member of the group gets, and does not:

| Question                         | Answer                                                                                                    |
| -------------------------------- | --------------------------------------------------------------------------------------------------------- |
| The bot token                    | Yes, inside the session, as in any session. A member can push to the product and open pull requests as the bot. Add an account only if you would give it that |
| Docker, or a shell as the tehas user | No. The rule names one script, and the account is not put in the `docker` group                       |
| Which clone                      | The one named after the account. `sudo` sets that name; the caller cannot. Nobody enters another account's clone |
| The author                       | What the account's git configuration says. Not checked against anything                                   |
| The arguments                    | An author, checked for its shape, and arguments for `pi`, which run inside the container                  |
| The environment                  | `sudo` clears it. `dev-login` loads the tehas user's own, as the timers do. `TERM` and `COLORTERM` are kept |

`gpasswd -d alice tehas` takes an account out. `test/login_test.sh` and
`test/bootstrap_test.sh` hold the rule and the two scripts to this.

`/usr/local/bin/tehas` is a copy of `host/login/tehas`, so an update of the
platform can leave the accounts typing an older command. No tehas command runs as
root, so none replaces it: `install.sh --check` reports it as `Login: NOT CURRENT`
and a real install prints `later: sudo ./bootstrap.sh dev boss`. Run that command
again after an update that changed `host/login/tehas`.

A host that ran cycles before the workspace existed gave its containers the
product checkout itself. Replace that checkout once with a fresh clone, so
nothing an earlier cycle could have left in it remains:

```
mv ~/tehas/repos/tehas ~/tehas/repos/tehas.old
git clone https://github.com/janit/tehas.git ~/tehas/repos/tehas
./install.sh
```
