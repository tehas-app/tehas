---
title: A new project
description: "Installing tehas for another repository: the contract in the product repository, the host, and the install."
---

# Installing tehas for a new project

One install serves one project: one host, one Unix user, one product
repository. Installing it for another repository is a release, one setup
command, a few files in the product, and a label. No script is edited and
nothing of tehas is cloned.

What you get is issue to pull request: the queue, cycles, gates, the dashboard
and the live console. The effort is settings, plus your toolchain in the
sandbox image. This page walks through it with the instance that works on
tehas itself: host `tehas-tehas`, user `boss`, product `tehas-app/tehas`.

## What the product repository must provide

This is the contract. It lives in the product repository and is written by
people. Tehas reads the gate in `.pi/verify-cmd` of a cycle's clone before the
cycle starts, and `preview/Dockerfile` of a pull request's commit when previews
are on; pi-rukas reads the rest of it.

| File                                                     | Purpose                                                                                                                                                                                                          | Required                                |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| `AGENTS.md`                                              | The rules agents work under: architecture rules, how to verify, commit style, what never to touch. No sentence granting merge rights, unless you want auto-merge                                                 | Yes                                     |
| `.pi/verify-cmd`                                         | Fast gate. The first line that is not empty or a comment is run in the cycle's worktree, 30 minute limit by default (`TEHAS_VERIFY_TIMEOUT`). Without it pi-rukas guesses from `package.json` or `Cargo.toml`, and for other stacks verifies nothing. `tehas work` checks the commit the cycle's clone was made at before the cycle starts: with no gate and neither `package.json` nor `Cargo.toml` there the cycle parks and nothing is pushed | Yes                                     |
| `.pi/verify-cmd-full`                                    | Full gate, same format; stands in for CI                                                                                                                                                                         | Recommended                             |
| `.pi/verify-cmd-fast`                                    | Optional developer's gate, same format. With it, pi-rukas tells the developer to run this command and not `.pi/verify-cmd`, and runs it ahead of `.pi/verify-cmd` wherever its driver gates |
| `.pi/smoke-cmd`                                          | Proves the built product starts, same format                                                                                                                                                                     | Recommended                             |
| `.pi/worktree-setup`                                     | Shell script run in each new worktree with no arguments and no environment: install dependencies. Must exit 0                                                                                                    | If a bare checkout cannot run the gates |
| `.pi/settings.json`                                      | Provider and model of the session, when they are not the instance's                                                                                                                                              | Optional                                |
| `.agents/skills/`                                        | Project skills: short instructions plus scripts for things agents get wrong                                                                                                                                      | Optional                                |

This repository is a product too, so its own `AGENTS.md` and `.pi/` are a
small working example to copy from. pi-rukas's own state files in a cycle's
clone (`.pi/work-state/`, `.pi/permissions.json`, `.pi/decisions.json`) are
kept out of git by `tehas work`, so the product needs no `.gitignore` lines for
them. Make the gates print one
line per step and exit non-zero on failure. Before going further, prove them by
hand in a clean checkout.

You do not have to write them first: [step 4](#4-the-products-files) checks
them and proposes the missing ones in a pull request.

A repository with no gate is caught before any model time is spent. `tehas
work` reads `.pi/verify-cmd` at the commit the cycle's clone was made at; when
its first line that is not empty or a comment is missing, and that commit holds
neither `package.json` nor `Cargo.toml`, the cycle parks with 78, prints one
line naming the missing file, and pushes nothing. The clone it made stays on the
host beside the workspace of the cycle before it, and neither is removed. With
either of those two files a cycle runs as it did before, because pi-rukas
derives a gate from them.

Two things follow. Only what is in the commit is read, so a file a person leaves
uncommitted or untracked in the product checkout decides nothing about whether a
cycle starts. And the check is one of presence, not of validity: pi-rukas takes
any file at those paths for a gate, so a stub `package.json` or a one-line
`.pi/verify-cmd` clears it while the cycle still verifies nothing. It is a guard
on the cost of a start, not proof that a real gate exists at merge time.

Work lands on the repository's default branch: pi-rukas branches from it and
the pull request targets it. tehas reads the default branch from the checkout.
Set `TEHAS_BRANCH` only to use another one.

## The install

The steps below install from a release, on one Linux machine that can reach
GitHub and an OpenAI-compatible model endpoint. `bootstrap.sh` supports Ubuntu
26.04 on x86_64 only.

### 1. The release

As root:

```
curl -fsSL https://github.com/tehas-app/tehas/releases/latest/download/install | sudo sh -s -- boss
```

The installer downloads the newest release and checks the tarball against its
`.sha256` and its minisign signature, with the key written in the installer,
not one from the download. A download that fails either check is not unpacked.
Then it runs that release's `bootstrap.sh root boss`: it makes the user when it
is missing, installs the packages and Docker, adds the user to the `docker`
group, turns lingering on, installs the slice, and installs the pinned tools as
the user in the same run, so no logout is needed. The release ends up in
`~boss/tehas/versions/<version>`, with `~boss/tehas/current` pointing at it.
`--version vX.Y.Z` installs another release. [Bootstrap a host](/bootstrap)
has what the machine needs.

To install from files you downloaded yourself, for example on a host that
cannot reach GitHub, pass their directory:

```
gh release download v0.9.0 -R tehas-app/tehas --pattern 'tehas-*' --pattern install -D /tmp/tehas-release
sudo sh /tmp/tehas-release/install --version v0.9.0 --from /tmp/tehas-release boss
```

The tehas user ends up in the `docker` group, which makes it root-equivalent on
the host. The boundary that matters is the sandbox: agents never get the Docker
socket.

### 2. What setup asks for

- The identity the factory acts as on GitHub. Make it [a GitHub App](/github-app),
  one per factory: its tokens last an hour, reach only the repositories it is
  installed on, and are renewed by tehas. `tehas setup` takes the app's ID,
  its installation's ID and its private key. The other way is a token: a
  machine user (a second account with a token) still works without the app,
  but GitHub can suspend an account that automation drives hard; the
  [bootstrap page](/bootstrap) has the token trap to avoid.
- The logins whose labels start work. Never the bot.
- The model endpoint's address with its `/v1`, and its key if it takes one.

### 3. tehas setup

As the tehas user. `tehas` is not on the `PATH` until the first install has
put the `env.sh` line in `~/.bashrc`, so the first run names the release:

```
~/tehas/current/bin/tehas setup --repo tehas-app/tehas --labellers "janit" \
  --app-id 5452597 --app-installation 174742089 --app-key-file ~/app.pem \
  --endpoint http://gb1:8086/v1
```

With a token in place of the app, `--token-file - <token.txt`. The token is
read from a file or standard input, never from the command line. It asks for
what it is not given when there is a terminal. In this order, and stopping at
the first step that fails, it:

1. checks that the app's installation lists the repository and may write its
   contents, issues and pull requests (or that the token can push to it), and
   that the endpoint offers the model, and makes the control issue;
2. writes `~/.config/tehas/instance.env` with the required settings only, and
   with an app its three settings, `models.json`, the app's key in
   `~/.config/tehas/github-app.pem` (or the token in `~/.config/tehas/env`),
   and the model key in `~/.config/tehas/env`, mode 600;
3. gives git gh's credential helper for github.com, unless git has one, and
   gives it the bot's name and noreply address, unless it has one; with an app
   the bot is `<slug>[bot]`;
4. clones the product to `~/tehas/repos/<name>`;
5. runs `install.sh` from the release, then `tehas labels`;
6. builds the sandbox image and runs the product's gates in it with
   `tehas check-product --sandbox`;
7. enables the dispatch timer and path, starts the docs server and prints the
   dashboard's address.

A second run changes nothing that is right. A setting that is there and
differs is not replaced without `--force`. See [tehas setup](/tools#tehas-setup).

A program that brings a host current without a terminal runs `install.sh` and
then `tehas update --pending`: the copies, then the follow-ups `install.sh`
recorded in `~/tehas/state/update-pending`. `--pending` runs no git command and
no `install.sh`, so it finishes an install on a host whose checkout is at a
release tag or whose remote is not reachable, where `tehas update` stops. See
[tehas update](/tools#tehas-update).

### 4. The product's files

```
tehas check-product
tehas init-product
```

`tehas check-product` says which files of the contract above the default
branch lacks, and exits 78 when one is missing. `tehas init-product` writes the
missing `AGENTS.md`, `.pi/verify-cmd` and `.pi/worktree-setup` on a branch and
opens a pull request with the bot. The gate is guessed from the stack: check
it, then merge it. See [tehas check-product](/tools#tehas-check-product).

### 5. The sandbox image

The platform's `host/sandbox/Dockerfile` adds shellcheck, tehas's jq and
Playwright's Chromium to the pi-rukas image and remaps the user id. It is part of the
release and is replaced by the next one, so your toolchain does not go there.

Put it in `sandbox.Dockerfile` in your instance directory instead (the one
directory `instance.env` and `models.json` live in, which you keep writable by
the tehas user alone: `install.sh --check` reports it if `instance.env` is not).
When that file is there, `~/tehas/sandbox/build.sh` builds the image above first
and then builds your file on top of it, so the tag cycles run is yours. What the
file says is run as root inside the image, and the image is what every cycle
runs with the bot's token: the file is trusted exactly as much as the platform's
own Dockerfile is, so it belongs to the tehas user and nowhere else. It starts
with these two lines, which take the image to build on as an argument rather
than a name:

```
ARG TEHAS_BASE
FROM $TEHAS_BASE
RUN apt-get update -qq \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends cmake \
 && rm -rf /var/lib/apt/lists/*
```

An `apt-get install` with no version takes whatever the mirror served that day,
so the image then depends on a mirror you do not control. The platform's own
Dockerfile pins what it fetches and checks each download by checksum; for a tool
your gates depend on, name the version (`cmake=3.31.6-1`) or add an apt pin file
beside the file and `COPY` it in.

The build runs with the instance directory as its context, so the file can
`COPY` a script or a config that lives beside it. A `sandbox.Dockerfile` that
changed since the last install makes `install.sh` owe the `sandbox` step, the
same as a change to the tracked Dockerfile or `versions.env`; `tehas update`
then rebuilds. A file that does not build fails the build and the tag cycles run
is left where it was, on the image the host had; a build that is killed partway
leaves it on the platform image, without your layer, so re-run
`~/tehas/sandbox/build.sh` after fixing the file.

After a change to the file, rebuild and check the gates again:

```
~/tehas/sandbox/build.sh
tehas check-product --sandbox
```

`tehas index` builds the code index the agents search, which needs the image.
`install.sh` does it too, as long as there is no index yet. See
[tehas index](/tools).

### 6. Prove the gates in the sandbox

`tehas setup` runs `tehas check-product --sandbox` once. It runs the product's
own gates inside the image, as the unprivileged user, with nothing mounted but a
clone made as a cycle's is, with `origin/main` and `origin/HEAD`, and checks the
contract files first. By hand it is:

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

### 7. Start

Label one small, well-specified issue with `tehas:ready` and watch it on the
dashboard. [Operating tehas](/operating) takes it from there, and
[Updating tehas](/operating#updating-tehas) says how a host moves to a new
release and back.

## Settings by hand

`tehas setup` writes the instance to `~/.config/tehas`, the default place. To
write it by hand, or to keep it in a private repository of your own, copy
`instances/example/` from the release (`~/tehas/current/instances/example/`)
and fill it in. Keep the directory out of `~/tehas`: an update replaces what is
there. When it is not `~/.config/tehas` itself, name it:

```
echo ~/my-instance >~/.config/tehas/instance-dir
```

`instance.env` holds the required settings. `optional.env` beside it lists the
optional ones with their defaults and what each turns on; copy a line over to
turn one on.

| Setting                                | What to put there                                                                                   |
| -------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `TEHAS_REPO`                           | `owner/name` of the product repository                                                              |
| `TEHAS_CONTROL_ISSUE`                  | Number of an issue you create for the pause label. Keep it open                                     |
| `TEHAS_ALLOWED_LABELERS`               | Logins whose labels count, space separated: `tehas:ready`, `tehas:plan`, `tehas:urgent`, `tehas:prototype`. Never the bot |
| `PI_ENSEMBLE_SUBAGENT_MODEL`, `_PROVIDER` | The model the session and every agent role use unless one has its own. Must be listed in `models.json` |
| `PI_ENSEMBLE_MODEL_<ROLE>`, `PI_ENSEMBLE_PROVIDER_<ROLE>` | Optional. A model for one role, such as the lens reviewers, so cycles do not all wait on one model. See [A model per role](/host#a-model-per-role) |
| `PI_ENSEMBLE_HOST_ALIASES`             | Optional. `name:address` pairs the sandbox needs, such as a model host whose name only resolves privately |
| `TEHAS_HOST_ADDR`                      | Optional. The host's private address, to reach the dashboard from other devices. Without it: `127.0.0.1` |

`models.json` next to it names the provider's `baseUrl` and the models it
serves. List the model you use first: an agent that loses its model setting
falls back to the first entry. A model needs only its `id`: `install.sh` fills
`name` (the id), `input` (`["text"]`), `reasoning` (false) and a cost of zero,
and gives a provider with no `compat` today's (`supportsDeveloperRole` and
`supportsReasoningEffort` false). A key you give is kept. The session model is
`PI_ENSEMBLE_SUBAGENT_MODEL` under `_PROVIDER`, unless the product's
`.pi/settings.json` names another; that one must be listed too. An endpoint
that takes a key gets `"apiKey": "$TEHAS_MODEL_KEY_MAIN"` and the token in
`~/.config/tehas/env`, never in `models.json`. A second endpoint for the same
models goes in `backupBaseUrl`: see
[A backup model endpoint](/host#a-backup-model-endpoint).

Everything else has a default. No setting describes the machine: every
sandbox container runs in `tehas.slice`, which the installer set up and which is
limited to three quarters of the host's memory. The dispatcher starts a cycle
only while the slice has room for one more `TEHAS_CYCLE_RESERVE`, 2g by
default, and holds fewer than `TEHAS_SLICE_CONTAINERS` containers, four by
default. A host with less than about 11 GiB of memory needs a lower reserve.
All settings are on the [commands page](/tools).

After a change to the instance, run `~/tehas/current/install.sh --check`, then
`tehas update`, which runs `install.sh` and the follow-ups it asks for. Where the
checkout cannot be moved - it is at a release tag, or its remote does not answer -
`tehas update --pending` runs the follow-ups `install.sh` recorded and no git
command; see [tehas update](/tools#tehas-update).

## Optional parts

The improvement loop reads the cycles that ended and files what could be
better as issues. Set `TEHAS_IMPROVE_REPO` in `instance.env` to the repository
those issues go to and install the factory's app there (or give the bot account
write access there). It cannot be the product repository: a model writes the
findings from what cycles saw, so they wait in a private repository of their
own until a person turns them into tickets. Several factories may share one.
A host
installed before you set it enables the timer by hand with
`systemctl --user enable --now tehas-improve.timer`. The logs of the cycles it
files about are published in that repository. See [Operating](/operating).

`tehas-energy.service` is optional too: set `TEHAS_ENERGY` only when your model
endpoint is a [viiwork](https://github.com/janit/viiwork) mesh. With any other
endpoint leave it unset: an update restarts a unit only where it already runs.
See [the energy sampler](/tools).

## Aside: the tehas-tehas instance

The instance that works on tehas itself has a few extras. Its settings are in
`instances/tehas-tehas/` in this repository; the host keeps its own copy in
`~/.config/tehas/instance`. The start of its `instance.env`, without its
comments:

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

- The two addresses are not in the repository: the `instance.env` reads them
  from `~/.config/tehas/private.env` on the host, a file of the same
  `KEY=value` lines. Your own instance directory can simply hold them.
- `TEHAS_INDEXING=1` and the three analytics lines are there because this
  instance's dashboard is published on the internet, at <https://tehas.app>: it
  lets search engines in and counts visits. Leave them out for an instance that
  is not public.
- Its endpoint is a [viiwork](https://github.com/janit/viiwork) mesh, a meshed
  inference platform for home and enterprise hardware, reached as `gb1` on the
  private network. That is not required: any OpenAI-compatible endpoint that
  serves your model works, and only the energy sampler depends on viiwork.
- It was installed before releases, from a platform checkout at
  `~/tehas/platform`, so `tehas update` pulls that checkout until it is moved
  to releases.
  See [Move a checkout host to releases](/host#move-a-checkout-host-to-releases).
  `models.json` of the instance, with every key written out:

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

## A preview of every pull request

Optional. With `TEHAS_PREVIEWS=1` and `TEHAS_DOMAIN` in `instance.env`, tehas
keeps a running copy of the product for every open pull request whose branch
is in the product repository and that an owner, member or collaborator of
the repository opened. [Previews](/edge) has what the host and the
domain need. The product's part is one file:

- The product adds `preview/Dockerfile`. A repository without it gets no
  previews, and a pull request whose commit does not have it gets none.
  The head of the default branch is previewed the same way, from its own
  recipe, at `uat-main.<domain>`. It stands beside the pull requests'
  previews and shows what is merged and not yet released.
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
- `bootstrap.sh` supports Ubuntu 26.04 on x86_64 only, and releases are built
  for linux x86_64 only.
