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
| `~/tehas/lib/sandbox.sh`, `questions.sh`, `planning.sh` | The sandbox container setup shared by `tehas-work` and `tehas-plan`, and the question and planning steps of the dispatcher |
| `~/tehas/bin/`                       | The tehas commands, plus `gh`                                           |
| `~/tehas/platform`                   | Checkout of this repository; `install.sh` runs from it                  |
| `~/tehas/repos/tehas`                | The product checkout, on the default branch. Cycles are cloned from it; no container is given it |
| `~/tehas/work/tehas`                 | The cycle's workspace: a fresh clone made for each cycle, removed before the next |
| `~/tehas/repos/pi-rukas`             | pi-rukas, pinned to a commit                                            |
| `~/tehas/docs/`, `~/tehas/fun/`      | This site's pages and the front page, installed from `host/`            |
| `~/tehas/sandbox/`, `versions.env`   | Sandbox image definition, build script, and the pinned tool versions    |
| `~/tehas/edge/`                      | The docs server (`docs.compose.yaml`) and the Protolab server (`protolab.compose.yaml`) |
| `~/tehas/protolab/`                  | The Protolab's pages and the files of each prototype, written by `tehas-protolab` |
| `~/tehas/logs/`, `state/`, `live/`, `tmp/` | Logs, state, dashboard data, scratch. `state/` holds the dispatch lock, status files, the record of each planning session (`plan-<issue>.json`) and the id of each parked cycle's question (`park-<issue>.id`). No container is given it |
| Docker volume `pi-ensemble-cache`    | Caches of the sandbox, among them the code index `tehas-index` builds   |
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
cd ~/tehas/platform && git pull
./install.sh --check     # change nothing, report four results
./install.sh             # copy changed files into place, keep <name>.prev
```

A version that adds labels needs `tehas-dispatch --init-labels` run again after
the install. The labels `tehas:plan`, `tehas:planning` and `tehas:question`,
for planning on an issue and the retry tick, are not in a repository until it
has been.

`--check` prints one line each for the instance settings, the host's tools,
the installed files and the tests:

```
Instance:     /home/boss/tehas/platform/instances/tehas-tehas OK
Dependencies: OK
Installed:    DRIFT (1 files)
  differs: /home/boss/tehas/bin/tehas-dispatch
Tests:        OK
```

`install.sh` validates the settings first and installs nothing if they are
invalid. It prints the follow-up command when the sandbox image, the docs
server or a systemd unit changed, and restarts nothing itself. A file edited directly on
the host shows up as drift in `--check`.

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

`TEHAS_WORK_CPUS` and `TEHAS_WORK_MEMORY` bound the one container a cycle runs
in. CPUs is a time quota (`docker --cpus`), not pinning. Memory is a hard limit
(`docker --memory`); a process that exceeds it is killed. The defaults are the
host's CPUs minus one and half its memory, which on the example host is 7 CPUs
and about 7.5 GB. A value above what the host has is rejected.

`TEHAS_CONCURRENCY` sets how many workstreams one cycle may run side by side,
inside that one container. It defaults to 4.

A workstream is one developer agent and its review loop in their own worktree.
pi-rukas plans up to that many per issue and folds anything beyond the limit
into the last one, so no work is dropped at a low setting; it is done in
sequence by fewer agents. An issue whose parts touch the same files is planned
as one workstream whatever the limit.

It is a limit inside one cycle, not across cycles: the dispatcher still runs
one cycle at a time. The workstreams share the cycle's CPU and memory limits,
and every one of them is at least one more model call in flight for most of
the cycle. Lower it when the model backends have few slots, when the host is
small, or if cycles start timing out.

```
# in the instance's instance.env, then pull and ./install.sh on the host
TEHAS_CONCURRENCY=1
```

It is read when a cycle starts; a running cycle keeps the value it began with.
Two things it does not change:

- the six review passes on the finished pull request still run together
  (pi-rukas's `PI_ENSEMBLE_SPAWN_CAP`, default 64, is the ceiling there),
- one issue is worked on at a time. Running several issues at once is not
  supported: cycles share one checkout.

## Sandbox image

`tehas/pi-rukas:uid<uid>`: the upstream pi-rukas image plus the in-container
user remapped to the tehas user's id, deno, shellcheck, and Playwright's
Chromium with fonts. The base is pinned by digest in `sandbox/Dockerfile`, the
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
| Energy use sampler, optional | systemd user service `tehas-energy.service`, every 15 s. Enable it only when the model backend is a viiwork mesh | `jq .generatedAt ~/tehas/live/energy.json` |
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
| What else starts a cycle               | The retry box of a parked issue, ticked by one of those accounts. It counts once, and only in the question whose comment id the host has on record for that issue. Like the label, the tick approves the text as it was at that moment |
| Who can start or continue planning     | The same accounts. A session starts when one of them applied `tehas:plan`. A turn runs on a tick or a reply from one of them to the question the session is waiting on |
| What a planning container is given     | The same token, the same mounts and the same approve-everything mode as a work cycle's. The agent is asked to plan and file issues. Nothing in the container holds it to that |
| What an agent can do as the bot        | Post comments, edit the bot's comments and apply labels, with the token. So a comment is the factory's question only if the host has its id on record in `~/tehas/state`, which no container is given, and an answer counts only by who gave it |
| Where an agent's conversation is kept  | In Pi's `sessions` directory. Every later container can read and write it                                                               |
| Does the token enter the cycle         | Yes, as `GH_TOKEN`. pi-rukas pushes the branch and opens the pull request from inside. Agent code can do anything the token can         |
| What else enters the container         | `PI_ENSEMBLE_*` settings, the bot's git name and email, the host aliases. Not the rest of the host environment                          |
| Host paths mounted                     | The cycle's workspace, `~/.vipune`, Pi's `ensemble-runs` and `sessions` (read-write); `models.json`, `mcp.json` (read-only); two caches |
| Not reachable from the container       | The Docker socket, SSH keys, `~/.config/tehas`, the platform checkout, the product checkout, the instance directory                     |
| What the host runs from the workspace  | Nothing. The host runs git only in the product checkout, which no container has written. It reads the cycle's state file from the workspace as data |
| The indexing container                 | `tehas-index` runs the indexer, not an agent: a clone of the product read-only, the cache volume, no network, no token                  |
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

A host that ran cycles before the workspace existed gave its containers the
product checkout itself. Replace that checkout once with a fresh clone, so
nothing an earlier cycle could have left in it remains:

```
mv ~/tehas/repos/tehas ~/tehas/repos/tehas.old
git clone https://github.com/janit/tehas.git ~/tehas/repos/tehas
./install.sh
```
