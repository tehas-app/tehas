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
| `~/tehas/lib/sandbox.sh`             | What a sandbox container is given; sourced by the work and dev scripts  |
| `~/tehas/bin/`                       | The two commands on `PATH`: `tehas` for a person, `tehasbot` for the units. Plus `gh` |
| `~/tehas/libexec/`                   | The scripts behind the two commands, one for each thing they do. Not on `PATH` |
| `~/tehas/platform`                   | Checkout of this repository; `install.sh` runs from it and `tehas update` pulls it |
| `~/tehas/repos/tehas`                | The product checkout, on the default branch. Cycles are cloned from it; no container is given it |
| `~/tehas/work/tehas-<issue>`         | The cycle's workspace: a fresh clone made for each cycle. Removed after a cycle that merged or left a pull request, otherwise kept until that issue's next cycle. `~/tehas/work/tehas` is only its path inside the container |
| `~/tehas/dev/<name>`                 | A person's clone for working by hand, made and used by `tehas dev`, never removed. `<name>.author` next to it holds whose name its commits carry |
| `~/tehas/repos/pi-rukas`             | pi-rukas, pinned to a commit                                            |
| `~/tehas/docs/`, `~/tehas/fun/`      | This site's pages and the front page, installed from `host/`            |
| `~/tehas/sandbox/`, `versions.env`   | Sandbox image definition, build script, and the pinned tool versions    |
| `~/tehas/edge/`                      | The docs server (`docs.compose.yaml`) and the Protolab server (`protolab.compose.yaml`) |
| `~/tehas/protolab/`                  | The Protolab's pages and the files of each prototype, written by `tehasbot protolab` |
| `~/tehas/logs/`, `state/`, `live/`, `tmp/` | Logs, lock and status files, dashboard data, scratch              |
| Docker volume `pi-ensemble-cache`    | Caches of the sandbox, among them the code index `tehas index` builds   |
| `~/.pi/agent/`                       | Pi user configuration (models, MCP servers)                             |
| `~/.config/tehas/`                   | Secrets and the `instance-dir` pointer. Never in git                    |

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
asks for by name (`units`, `sandbox`, `docs`, `protolab`: see [Commands](/tools)).
One line per step, and it stops at the first failure. It checks the instance
settings against the required list in `lib/config.sh`, the same one install.sh
checks, before it calls git. It changes nothing when the checkout has local
changes or cannot be fast-forwarded, and it never enables a unit or starts a
container the host was not already running: a changed unit is reloaded and
restarted when it was active, and a docs or Protolab container is recreated when
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
./install.sh --check     # change nothing, report five results
./install.sh             # copy changed files into place, keep <name>.prev
```

`--check` prints one line each for the instance settings, the host's tools, the
installed files, the follow-ups an earlier run left undone, and the tests:

```
Instance:     /home/boss/tehas/platform/instances/tehas-tehas OK
Dependencies: OK
Installed:    DRIFT (1 files)
  differs: /home/boss/tehas/libexec/dispatch
Follow-ups:   NOT DONE (2 from an earlier run, see /home/boss/tehas/state/update-pending)
Tests:        OK
```

The `Follow-ups:` line appears only while `~/tehas/state/update-pending` still
holds steps that were asked for and never ran - a host with none owed prints four
lines - and while it does, `--check` exits 1, because a host with files matching
and a step unapplied is not current. Running `tehas update` runs them and clears
the record ([Troubleshooting](/troubleshooting) has the case in full).

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

`TEHAS_WORK_CPUS` and `TEHAS_WORK_MEMORY` bound all the cycles of a host
together. CPUs is a time quota (`docker --cpus`), not pinning. Memory is a hard
limit (`docker --memory`); a process that exceeds it is killed. The defaults are
the host's CPUs minus one and half its memory, which on the example host is 7
CPUs and about 7.5 GB. A value above what the host has is rejected.

`TEHAS_TICKET_CONCURRENCY` sets how many issues are worked on side by side,
each in a cycle, a container and a workspace of its own. It defaults to 1. Each
container gets the two limits above divided by it, whether or not the other
slots are in use. A share below one CPU is rejected, so the setting cannot
exceed the CPUs given.

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

To upgrade pi-rukas: pull the new upstream image, put its digest in the `FROM`
line, rebuild, and run the contract test before any cycle uses it. The contract
test proves the product's gates run inside the image, as the unprivileged user,
with nothing mounted but a clone:

```
T=~/tehas/tmp/contract-test
git clone -q ~/tehas/repos/tehas "$T"
docker run --rm --cpus 4 --memory 4g --user "$(id -u):$(id -g)" \
  -e HOME=/home/vscode -v "$T:$T" -w "$T" --entrypoint bash \
  "$PI_ENSEMBLE_IMAGE" -c '
    sh .pi/worktree-setup &&
    sh -c "$(grep -v -e "^#" -e "^$" .pi/verify-cmd-full | head -n 1)" &&
    sh -c "$(grep -v -e "^#" -e "^$" .pi/smoke-cmd | head -n 1)"'
rm -rf "$T"
```

## Services

| Service     | How it runs                                                | Check                                                  |
| ----------- | ---------------------------------------------------------- | ------------------------------------------------------ |
| This site   | `docker compose -f ~/tehas/edge/docs.compose.yaml up -d`   | `curl -i http://tehas-tehas:8080/health`               |
| Dispatcher  | systemd user timer `tehas-dispatch.timer`, every 2 minutes | `tail ~/tehas/logs/dispatch.log`                       |
| Energy use sampler, optional | systemd user service `tehas-energy.service`, every 15 s. Enable it only when the model backend is a [viiwork](https://github.com/janit/viiwork) mesh | `jq .generatedAt ~/tehas/live/energy.json` |
| Protolab pages | systemd user timer `tehas-protolab.timer`, every 5 minutes | `tail ~/tehas/logs/protolab.log` |
| Protolab server | `docker compose -f ~/tehas/edge/protolab.compose.yaml up -d` | `curl -i http://tehas-tehas:8081/` |

The docs server binds to `TEHAS_HOST_ADDR`, port 8080. Without the setting it
stays on `127.0.0.1`. `docker compose` reads the address from the environment,
so run it from a shell that has sourced `~/tehas/env.sh`.

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
| Does the token enter the cycle         | Yes, as `GH_TOKEN`. pi-rukas pushes the branch and opens the pull request from inside. Agent code can do anything the token can         |
| What else enters the container         | `PI_ENSEMBLE_*` settings, the bot's git name and email, the host aliases. Not the rest of the host environment                          |
| Host paths mounted                     | The cycle's own workspace, `~/tehas/work/<name>-<issue>`, at the same path in every container; its own transcripts, `~/.pi/agent/ensemble-runs/issue-<issue>`; `~/.vipune` and Pi's `sessions` (read-write); `models.json`, `mcp.json` (read-only); two caches |
| Not reachable from the container       | The Docker socket, SSH keys, `~/.config/tehas`, the platform checkout, the product checkout, the instance directory                     |
| What the host runs from the workspace  | Nothing. The host runs git only in the product checkout, which no container has written. It reads the cycle's state file from the workspace as data |
| The indexing container                 | `tehas index` runs the indexer, not an agent: a clone of the product read-only, the cache volume, no network, no token                  |
| The network                            | Not restricted. The container reaches GitHub, the model endpoint and whatever else the host can route to                                |
| Merging                                | The token could merge where the repository lets it. Nothing does, because `AGENTS.md` forbids it and the dispatcher never merges        |

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
| Who can start one           | Anyone who can log in as the tehas user. That account already holds the bot token and is root-equivalent           |
| The tree it works in        | `~/tehas/dev/<name>`, kept between sessions, where a cycle gets a fresh clone                                       |
| git identity                | The stored author of that clone, else the bot's. A label: it is not checked against anything                       |
| What else enters            | A terminal, and `TERM` and `COLORTERM` from the shell. Nothing else of the host environment, and not `~/.config/gh` |
| What the host runs from the clone | Nothing. The host runs git there once, to make it, and never again                                           |

`test/dev_test.sh` pins the session's `docker run` the same way. Both commands
take the mounts and variables from `lib/sandbox.sh`, so they cannot differ by
accident.

A host that ran cycles before the workspace existed gave its containers the
product checkout itself. Replace that checkout once with a fresh clone, so
nothing an earlier cycle could have left in it remains:

```
mv ~/tehas/repos/tehas ~/tehas/repos/tehas.old
git clone https://github.com/janit/tehas.git ~/tehas/repos/tehas
./install.sh
```
