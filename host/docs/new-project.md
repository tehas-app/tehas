---
title: A new project
description: "Installing tehas for another repository: the contract in the product repository, the host, and the install."
---

# Installing tehas for a new project

One install serves one project: one host, one Unix user, one product
repository. Installing it for another repository means preparing a host and
filling in an instance directory. No script is edited.

What you get is issue to pull request: the queue, cycles, gates, the dashboard
and the live console. The effort is settings, plus your toolchain in the
sandbox image. This page walks through it with the instance that works on
tehas itself: host `tehas-tehas`, user `boss`, product
`tehas-app/tehas`. Its settings are in `instances/tehas-tehas/` in this
repository.

## What the product repository must provide

This is the contract. It lives in the product repository, is written by people,
and is the only thing tehas reads from it.

| File                                                     | Purpose                                                                                                                                                                                                          | Required                                |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| `AGENTS.md`                                              | The rules agents work under: architecture rules, how to verify, commit style, what never to touch. No sentence granting merge rights, unless you want auto-merge                                                 | Yes                                     |
| `.pi/verify-cmd`                                         | Fast gate. The first line that is not empty or a comment is run in the cycle's worktree, 30 minute limit by default (`TEHAS_VERIFY_TIMEOUT`). Without it pi-rukas guesses from `package.json` or `Cargo.toml`, and for other stacks verifies nothing | Yes                                     |
| `.pi/verify-cmd-full`                                    | Full gate, same format; stands in for CI                                                                                                                                                                         | Recommended                             |
| `.pi/smoke-cmd`                                          | Proves the built product starts, same format                                                                                                                                                                     | Recommended                             |
| `.pi/worktree-setup`                                     | Shell script run in each new worktree with no arguments and no environment: install dependencies. Must exit 0                                                                                                    | If a bare checkout cannot run the gates |
| `.pi/settings.json`                                      | `defaultProvider` and `defaultModel` for the session                                                                                                                                                             | Yes                                     |
| `.gitignore` entries                                     | `.pi/work-state/`, `.pi/permissions.json`, `.pi/decisions.json`                                                                                                                                                  | Yes                                     |
| `.agents/skills/`                                        | Project skills: short instructions plus scripts for things agents get wrong                                                                                                                                      | Optional                                |

This repository is a product too, so its own `AGENTS.md`, `.pi/` and
`.gitignore` are a small working example to copy from. Make the gates print one
line per step and exit non-zero on failure. Before going further, prove them by
hand in a clean checkout.

Work lands on the repository's default branch: pi-rukas branches from it and
the pull request targets it. tehas reads the default branch from the checkout.
Set `TEHAS_BRANCH` only to use another one.

## The install

### 1. A host

One Linux machine, reachable over a private network, that can reach GitHub and
an OpenAI-compatible model endpoint. [Bootstrap a host](/bootstrap) prepares it:

```
sudo ./bootstrap.sh root boss
./bootstrap.sh user
./bootstrap.sh check
```

The tehas user ends up in the `docker` group, which makes it root-equivalent on
the host. The boundary that matters is the sandbox: agents never get the Docker
socket.

### 2. Credentials

- A GitHub machine account that is a write collaborator on the product
  repository. Its token goes in `~/.config/tehas/env`, mode 600, as
  `GH_TOKEN=...`. Use one bot account per instance: the token enters the cycle's
  container, so it should reach this product and nothing else.
- A read-only deploy key on the platform repository, if it is private.
- The bot's git identity for commits: `git config --global user.name` and
  `user.email` for the tehas user.

The [bootstrap page](/bootstrap) has the details and the token trap to avoid.

### 3. Two clones

The platform checkout is this repository at `~/tehas/platform`, where you ran
`bootstrap.sh` from. Clone the product next to it, with the bot's token:

```
git clone <the product repository> ~/tehas/repos/<name>
```

The product checkout is what cycles are cloned from. The platform checkout is where
`install.sh` runs from. Keep them apart even when, as in the example, they are
the same repository: the factory then changes only when a person pulls the
platform checkout and installs.

### 4. An instance directory

Copy `instances/example/` and fill it in. It can live anywhere the tehas user
can read: next to the secrets in `~/.config/tehas` (the default place), in a
private repository of your own, or, as the example does, in the platform
checkout.

```
echo ~/tehas/platform/instances/tehas-tehas >~/.config/tehas/instance-dir
```

The start of the example's `instance.env`, without its comments. The rest of
it turns on optional parts: previews, the improvement loop and limits of its
own.

```
TEHAS_REPO=tehas-app/tehas
TEHAS_CONTROL_ISSUE=1
TEHAS_ALLOWED_LABELERS="janit"

PI_ENSEMBLE_SUBAGENT_MODEL=qwen3.8-flash-next
PI_ENSEMBLE_SUBAGENT_PROVIDER=fleet
TEHAS_INDEXING=1

# from ~/.config/tehas/private.env on the host
PI_ENSEMBLE_HOST_ALIASES=gb1:<the model host's private address>
TEHAS_HOST_ADDR=<this host's private address>

TEHAS_ANALYTICS_SRC=https://ea.trippi.app/ea.js
TEHAS_ANALYTICS_SITE=tehas
TEHAS_ANALYTICS_COOKIE=1
```

The two addresses are not in the repository: the example's `instance.env` reads
them from `~/.config/tehas/private.env` on the host, a file of the same
`KEY=value` lines. Your own instance directory can simply hold them.
`TEHAS_INDEXING=1` and the three analytics lines are there because this
instance's dashboard is published on the internet, at <https://tehas.app>: it
lets search engines in and counts visits. Leave them out for an instance that
is not public.

| Setting                                | What to put there                                                                                   |
| -------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `TEHAS_REPO`                           | `owner/name` of the product repository                                                              |
| `TEHAS_CONTROL_ISSUE`                  | Number of an issue you create for the pause label. Keep it open                                     |
| `TEHAS_ALLOWED_LABELERS`               | Logins whose labels count, space separated: `tehas:ready`, `tehas:plan`, `tehas:urgent`, `tehas:prototype`. Never the bot |
| `PI_ENSEMBLE_SUBAGENT_MODEL`, `_PROVIDER` | The model every agent role uses unless it has its own. Must be listed in `models.json`           |
| `PI_ENSEMBLE_MODEL_<ROLE>`, `PI_ENSEMBLE_PROVIDER_<ROLE>` | Optional. A model for one role, such as the lens reviewers, so cycles do not all wait on one model. See [A model per role](/host#a-model-per-role) |
| `PI_ENSEMBLE_HOST_ALIASES`             | `name:address` pairs the sandbox needs, such as a model host whose name only resolves privately     |
| `TEHAS_HOST_ADDR`                      | The host's private address, to reach the dashboard from other devices. Without it: `127.0.0.1`      |

`models.json` next to it names the provider's `baseUrl` and the models it
serves. List the model you use first: an agent that loses its model setting
falls back to the first entry. The model in the product's `.pi/settings.json`
must be listed too. The example's, with one provider and one model:

```
{
  "providers": {
    "fleet": {
      "api": "openai-completions",
      "baseUrl": "http://gb1:8086/v1",
      "apiKey": "unused",
      "compat": { "supportsDeveloperRole": false, "supportsReasoningEffort": false },
      "models": [
        {
          "id": "qwen3.8-flash-next",
          "name": "Qwen3.8 Flash Next (fleet)",
          "reasoning": false,
          "input": ["text", "image"],
          "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
          "contextWindow": 262144,
          "maxTokens": 32768,
          "compat": {
            "supportsReasoningEffort": false,
            "supportsDeveloperRole": false,
            "maxTokensField": "max_tokens",
            "thinkingFormat": "qwen-chat-template"
          }
        }
      ]
    }
  }
}
```

`gb1` is the model host's name on the private network. The endpoint takes no
key, so `apiKey` is a placeholder. An endpoint that takes one gets
`"apiKey": "$TEHAS_MODEL_KEY_MAIN"` and the token in `~/.config/tehas/env`,
never in `models.json`. A second endpoint for the same models goes in
`backupBaseUrl`: see [A backup model endpoint](/host#a-backup-model-endpoint). The example's endpoint is a
[viiwork](https://github.com/janit/viiwork) mesh: a flexible meshed inference platform for both home and enterprise grade hardware.
That is not required: any OpenAI-compatible endpoint that serves
your model works, and only the optional Energy use section depends on viiwork.

Everything else has a default. No setting describes the machine: every
sandbox container runs in `tehas.slice`, which step 1 installed and which is
limited to three quarters of the host's memory. The dispatcher starts a cycle
only while the slice has room for one more `TEHAS_CYCLE_RESERVE`, 2g by
default, and holds fewer than four containers. A host with less than about 11 GiB of memory needs a lower reserve.
All settings are on the [commands page](/tools).

### 5. Install

```
cd ~/tehas/platform
./install.sh --check
./install.sh
sed -i '1i [ -f ~/tehas/env.sh ] \&\& . ~/tehas/env.sh' ~/.bashrc
```

The last line puts `env.sh` first in `~/.bashrc`. It has to come before the
check many distributions have there that returns early for non-interactive
shells, or `ssh host '<command>'` would run without the tehas environment.

`--check` first tells you what is missing or malformed in the instance, what
tools the host lacks, and what would be installed. A real run with invalid
settings installs nothing and exits 78.

### 6. The sandbox image

`host/sandbox/Dockerfile` in the platform checkout adds shellcheck and
Playwright's Chromium to the pi-rukas image and remaps the user id. Add whatever your gates need. Then:

```
~/tehas/sandbox/build.sh
tehas index
```

`tehas index` builds the code index the agents search, which needs the image.
A later `install.sh` would do it too, as long as there is no index yet. See
[tehas index](/tools).

### 7. Prove the gates in the sandbox

Before the first issue. This runs the product's own gates inside the image, as
the unprivileged user, with nothing mounted but a clone:

```
T=~/tehas/tmp/contract-test
git clone -q ~/tehas/repos/<name> "$T"
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

### 8. Start

`install.sh` ends with one `next:` line per follow-up its changed files call for,
named by id (`units`, `sandbox`, `docs`, `edge`) rather than as a command, and
records those steps as owed in `~/tehas/state/update-pending` while it copies. It
starts nothing itself. On a first start the units are not enabled yet, and no
update enables them, so:

```
docker compose -f ~/tehas/edge/docs.compose.yaml up -d --force-recreate  # this site and the dashboard
tehas labels
tehas dispatch --dry-run
systemctl --user daemon-reload
systemctl --user enable --now tehas-dispatch.timer tehas-dispatch.path tehas-protolab.timer
tehas update        # runs the follow-ups install.sh recorded, and clears them
```

`install.sh` names `tehas-improve.timer` too. It belongs to the improvement
loop, which reads the cycles that ended and files what could be better as
issues. It is optional: set `TEHAS_IMPROVE_REPO` in `instance.env` to the
repository those issues go to, give the bot account write access there, and
enable the timer with `systemctl --user enable --now tehas-improve.timer`.
The logs of the cycles it files about are published in that repository. See
[Operating](/operating).

`install.sh` names `tehas-energy.service` in its `units` step too. That service is
optional: enable it only when your model endpoint is a [viiwork](https://github.com/janit/viiwork) mesh, as the
example's `fleet` provider is. With any other endpoint leave it disabled: an
update restarts a unit only where it already runs. See [the energy sampler](/tools).

Then label one small, well-specified issue with `tehas:ready` and watch it on
the dashboard. [Operating tehas](/operating) takes it from there.

## A preview of every pull request

Optional. With `TEHAS_PREVIEWS=1` and `TEHAS_DOMAIN` in `instance.env`, tehas
keeps a running copy of the product for every open pull request whose branch
is in the product repository and that an owner, member or collaborator of
the repository opened. [Previews](/edge) has what the host and the
domain need. The product's part is one file:

- The product adds `preview/Dockerfile`. A repository without it gets no
  previews, and a pull request whose commit does not have it gets none.
- It is built with the commit as its context: the files of that commit as
  git has them, and nothing else. The file is read from the pull request's
  own commit, so a pull request that changes how the product is served is
  previewed as it will be.
- The image must answer HTTP on port 8080.
- It runs with a read-only root. Only `/tmp`, `/config` and `/data` can be
  written, and they are in memory and empty at every start.
- It must not declare a volume anywhere else: no `VOLUME` line in the recipe
  or in the image it starts from, but for those three directories. Docker
  would make such a volume on the host's disk, where a preview may write
  nothing. A recipe that has one is not run, and the state is `build-failed`
  with the path. `docker image inspect --format '{{json .Config.Volumes}}'
  tehas-preview:local` shows what an image declares.
- What it prints is kept in at most two files of 10 MB.
- It must need no network and no variable. A preview is given no setting and
  no token, and it reaches nothing but the other previews: no database, no
  API, not the internet, not the host. What it shows has to be in the image.
- It gets 256 MB of memory, one CPU and 128 processes.
- The build has 15 minutes and does have the network.

This repository's own `preview/Dockerfile` is the example: the manual and the
dashboard on sample data, served by the same server a host uses. To try a
recipe by hand, from the product's checkout:

```
git archive HEAD | docker build -f preview/Dockerfile -t tehas-preview:local -
docker run --rm -p 127.0.0.1:18080:8080 --read-only --tmpfs /tmp --tmpfs /config --tmpfs /data \
  --cap-drop ALL --cap-add NET_BIND_SERVICE --security-opt no-new-privileges:true \
  --memory 256m --cpus 1 --pids-limit 128 --log-opt max-size=10m --log-opt max-file=2 \
  tehas-preview:local
```

That is what `tehasbot preview` does, but for the port: a real preview has
none published and is reached through the edge router, on a network that
has the other previews on it and leads nowhere else.

## What is not there yet

- A private database copy per cycle or per preview.
- More than one project per host. A second project needs its own Unix user, or
  better its own machine.
- `bootstrap.sh` supports Ubuntu 26.04 on x86_64 only.
