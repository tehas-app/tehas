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
| `~/tehas/env.sh`                     | Environment for every tehas process, its tool directories taken from `lib/paths.sh`; sourced from `~/.bashrc`           |
| `~/tehas/lib/config.sh`              | Loads and checks the instance settings; sourced by every command        |
| `~/tehas/lib/paths.sh`               | The home of each tool this host installs, and the `PATH` that finds them. `env.sh`, `bootstrap.sh` and `install.sh` all take the list from here |
| `~/tehas/lib/sandbox.sh`             | What a sandbox container is given, and the one name of the sandbox and base images; sourced by the work, plan and dev scripts, and beside `lib/rukas.sh` by `tehasbot clean` |
| `~/tehas/lib/slice.sh`               | Reads the slice: whether it is usable, a container's limits, the room left. Every command that starts a sandbox container reads it |
| `~/tehas/lib/headless.sh`            | What tehas reads of pi-rukas's headless runner: its `--capabilities` answer, its progress and its result. `tehas work` and `tehasbot cycle` use it |
| `~/tehas/lib/attachments.sh`         | Which links of an issue are attached files and how one is fetched. The Protolab and `tehas work` use it |
| `~/tehas/lib/questions.sh`, `planning.sh` | The question and planning steps of the dispatcher. A cycle's report uses the first too |
| `~/tehas/bin/`                       | The two commands on `PATH`: `tehas` for a person, `tehasbot` for the units. Plus `gh` |
| `~/tehas/libexec/`                   | The scripts behind the two commands, one for each thing they do. Not on `PATH` |
| `~/tehas/platform`                   | Checkout of this repository; `install.sh` runs from it and `tehas update` pulls it |
| `~/tehas/repos/tehas`                | The product checkout, on the default branch. Cycles are cloned from it; no container is given it |
| `~/tehas/work/tehas-<issue>`         | The cycle's workspace: a fresh clone made for each cycle. Removed after a cycle that merged or left a pull request, otherwise kept, and moved to `tehas-<issue>.before-retry` when that issue is retried. A start that stopped before the container ran keeps the clone it made, as does the one it moved aside. A kept workspace, its `.before-retry` and what else is left beside it go once no cycle has run on the issue for 7 days. Its build output goes sooner: `tehasbot clean` removes what it names as build output once nothing of the issue has been touched for `TEHAS_CLEAN_AFTER` hours, in this tree, in the one a retry moved aside, and in the worktrees of a cycle under either at the depth the runner lays them out, and the 7-day sweep still takes the rest. `~/tehas/work/tehas` is only its path inside the container |
| `~/tehas/work/tehas-<issue>.state`   | The cycle's own: `cache`, a copy of the code index the host built; `memory`, what its agents learn; `sessions`, Pi's session files. Made fresh for each cycle, mounted in its container, removed with the workspace. A cycle reads no memory an earlier cycle wrote, even one of the same issue, so a `vipune search` at its start answers `{"results": []}`. `~/tehas/work/tehas-plan-<issue>.state` is a planning turn's, removed after the turn. Of these, only `cache` is build output: `tehasbot clean` removes it when the issue has been idle for `TEHAS_CLEAN_AFTER` hours, and `memory` and `sessions` stay |
| `~/tehas/work/tehas-<issue>.images`  | With `TEHAS_WORK_IMAGES=1`, the images of the cycle's issue and their `manifest.json`, and with `TEHAS_WORK_DESCRIBE=1` the model's descriptions of them, `images.md`. With `TEHAS_WORK_FILES=1`, the issue's text files too, `file-<n>.<ext>`, with or without the images. Made for each cycle, read-only in its container at `~/tehas/work/tehas.images`, removed when the cycle is over |
| `~/tehas/work/tehas-plan-<issue>.images` | With `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, a copy of a planning session's images and files for one turn, read-only in its container at `~/tehas/work/tehas.images`, removed after the turn |
| `~/tehas/work/tehas-plan-<issue>.keep` | What a planning session carries from one turn to the next: the approved text, the commit it began at, pi-rukas's record of the session, the last turn's exit status, the issue's images, and a later turn's answer as `answer.md`. Removed when the session ends. No container is given it; each turn copies what it needs into its workspace |
| `~/tehas/dev/<name>`                 | A person's clone for working by hand, named after their account, made and used by `tehas dev`, never removed. `<name>.author` next to it holds whose name its commits carry, `<name>.state` its memory, sessions, transcripts, code index and cargo build output (`target`, kept so a later session of that clone builds on what an earlier one built; see [tehas dev](/tools#tehas-dev)), and `<name>.images` the images and files of the issue named by `--issue`, read-only in the session's container |
| `~/tehas/repos/pi-rukas`             | pi-rukas, at the commit `PI_RUKAS_FOLLOW` names                         |
| `~/tehas/docs/`, `~/tehas/fun/`      | This site's pages and the front page, installed from `host/`            |
| `~/tehas/sandbox/`, `versions.env`   | Sandbox image definition, build script, the Playwright npm pin, and the pinned tool versions |
| `~/tehas/edge/`                      | The docs server (`docs.compose.yaml`) and the edge router (`router.compose.yaml`) |
| `~/tehas/protolab/`                  | The Protolab's pages and the files of each prototype, written by `tehasbot protolab` |
| `~/tehas/preview/`                   | The state of each preview, and a bare clone the pull requests' commits are fetched into, written by `tehasbot preview` |
| `~/tehas/logs/`, `state/`, `live/`, `tmp/` | Logs, state, dashboard data, scratch. `state/` holds the dispatch lock, status files, the files a planning turn passes on (`plan-<issue>.json.out`, `.json.rc` and `.json.lock`), the state store (`tehas.db`), which holds what each start rests on, each cycle's record and its history, each park and the id of its question, each planning session and the history of its turns, and the manifest of each start's images (`cycles/<issue>.manifest.json`, with `cycles/<issue>.images.md` and `cycles/<issue>.retry-context.md` when the cycle had them). What each approved start measured is kept in `state/sizes.jsonl`, one line per start, mode 600: when, which issue, and the four counts of its approved body, the last 500 lines. The report's size figures read it. What each refining session filed is kept beside it in `state/refined.jsonl`, likewise the last 500 lines. No cycle, planning or manual container is given it; the indexing container is given `state/index` to write |
| `~/tehas/mcp/`                       | The sessions snapshot for the MCP server, `sessions.jsonl`, written by `tehasbot status`. Mode 700: it holds what the agents saw, and is never published |
| `~/tehas/tmp/improve/`, `~/tehas/state/improve.lock` | Where the improvement loop is on: the record of each cycle that ended, until a run has read it, and the lock of a run |
| `~/tehas/state/tehas.db`, `state/backup/` | The state store's database and the copies made of it before each migration. See [The state store](#the-state-store) |
| `~/tehas/state/imported-<YYYYMMDD>/` | The record files of an older tehas that `install.sh` imported into the state store that day, each under its path in `state/`. Nothing reads them; remove the directory once satisfied. See [The state store](#the-state-store) |
| `~/tehas/state/index`                | The code index `tehas index` builds, under the name of the tree it was made from. The indexing container is given it to write; no cycle, planning or manual container is given it, and a cycle is given a copy of it in its own state directory |
| `~/.pi/agent/`                       | Pi user configuration (models, MCP servers)                             |
| `~/.config/tehas/`                   | Secrets and the `instance-dir` pointer. Never in git                    |

Where the improvement loop is on, each of its runs removes the per-cycle files
in `~/tehas/logs/` that are older than seven days (`work-*`, `dispatch-work-*`,
`index-*`), and the records that old under `~/tehas/tmp/improve/`. Nothing else
removes logs.

**The cleaner.** `tehasbot clean`, from `tehas-clean.timer` once an hour, removes
what the host can make again once nothing has touched it for
`TEHAS_CLEAN_AFTER` hours (24 unless the instance says otherwise): the build
output of a kept workspace - every directory anywhere under it whose name is one
of `TEHAS_CLEAN_DIRS`, the toolchain names listed in [Commands](/tools) - and the
cycle's `cache`, its copy of the code index. The same step walks the trees beside
the workspace that hold build output of their own: `tehas-<issue>.before-retry`,
the workspace a retry moved aside, and the worktrees of a cycle under either - a
worktree being a directory at the depth the runner lays them out whose `.git` is
a file naming a gitdir, which is what a cycle makes inside the workspace it was
given and leaves where it stands with a clone of its own and that clone's build
output under it. A worktree
sits under a dot-name, so the walk of the workspace never reaches it: each is
asked as a root of its own, and one that is a symlink is not walked at all. An
issue with no workspace left, only the tree a
retry moved aside, is asked too, since that is all of it the disk still holds.
What the workspace keeps is what a person reads: `.git`, the sources, what the
agents wrote there, uncommitted changes, and the `memory` and `sessions` of its
`.state`. The cleaner runs no git command in a workspace except one: `git -C
<worktree> rev-parse --absolute-git-dir`, the call that names a worktree's gitdir
for the age rule, in a tree a cycle wrote. Git and the bound around it are named
by absolute path and run under a cleared environment that reads no config file at
all, and the answer is used only when it names an existing directory under
`~/tehas/work`; an issue is left whole when git cannot answer. It follows no
symlink, and enters no dot-name, which is what
keeps those trees out of the removal. It holds the issue's lock while it removes,
so no cycle starts under it, and it removes nothing for an issue whose lock file
is not there to hold. When the age rule passes, an issue is left alone for one of
two reasons and the log line names which: `kept the build output of #<issue>:
no lock to hold, so nothing of it was removed` when its trees held build output and no
lock could be taken, and `nothing to remove in #<issue>` when none of its trees held
build output at all. Two more lines name a reason of their own: `its lock is
held`, for an issue a cycle is running before the age is asked, and `git could not
name the gitdir of one of its worktrees`, for a worktree record that no longer
matches a tree. The 7-day sweep of the dispatcher still takes the workspace
itself. The build output of a manual session, `~/tehas/dev/<name>.state/target`,
is the one build output the cleaner never takes: it walks no directory under
`~/tehas/dev` for build-output names, and that tree is a person's call (see
[tehas dev](/tools#tehas-dev)).

The same run prunes Docker's build cache of that age with `docker builder prune`.
That is the build cache of the whole host's Docker daemon, not only tehas's:
tehas is one project on one host, and the cache of anything else on it is made
again too.

It removes images in two steps, and no volume. The first takes the dated tags of
the sandbox image whose image is older than `TEHAS_CLEAN_AFTER` hours, keeping
the newest dated tag and any tag a container or a `uid` tag was made from. The
build keeps its newest three date tags (see [Commands](/tools)); what is older
than the age besides the newest is this step's. The second lists the `sha-` tags
of the base repository, the repository of the image the build pulls from
upstream, and removes those of that age with one `docker rmi` each: the current
commit's base, a base a container was made from, and a base a kept `release-`
tag of the sandbox image stands on all stay: the kept releases are the
`release-` tags of that image in docker's own list, so no version is assumed,
none is left out, and a host that holds two of them keeps the base of each. Nothing else is offered to docker:
every name it is handed is a tag of that one repository, and the step never
removes the images that have lost their tag. A container is what keeps an image
here: `tehasbot work` runs its cycles from the sandbox image, so one an existing
container was made from stays whatever its age. Preview images are
`tehasbot preview`'s and no step here touches them; a docker that answers no
image list, or nothing about the containers on the host, removes nothing, since
an unknown is never a reason to take an image away. Volumes are a person's call.

The third step is the host's scratch: old files under `~/tehas/tmp/dl`, where
`bootstrap.sh` keeps what it downloads, and under `~/tehas/tmp/node-compile-cache`,
old `~/tehas/tmp/analytics-facts.*` files a killed analytics call left, and old
`~/tehas/tmp/tehas-test.*` directories a run left behind. Nothing else
under `~/tehas` is touched, and nothing outside it: not `~/tehas/tmp/improve`,
`state`, `logs`, `dev`, `repos`, `preview` or `protolab`. One line per removal and
a closing line with the bytes freed go to `~/tehas/logs/clean.log`; `tehasbot
clean --dry-run` logs what it would remove and removes nothing. The cleaner starts
no work, so the pause label does not hold it.

In the example the platform and the product are the same repository. They are
still two clones. A merged pull request changes the running factory only when a
person pulls `~/tehas/platform` and runs `install.sh`.

## What a factory machine needs

A factory is one machine. Nothing of it is shared with another factory: not
the Docker daemon, not the ports the edge publishes, not the Unix user, not
the name it is known by. A program that creates factories, such as
[tehas-industry](/api), gets one machine per factory and runs the same two
commands a person does.

| Needs                                          | Why                                                                                                                    |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| systemd, and lingering on for the tehas user    | The dispatcher is a systemd user timer. `loginctl enable-linger` is what lets it poll when nobody is logged in. `sudo ./bootstrap.sh root` enables it |
| A Docker daemon of its own                     | Every cycle, planning turn and index run is a container here. Sharing a daemon with another factory would put its containers in this factory's slice, and let one factory stop another's cycles |
| The published ports to itself                  | The docs server answers on port 8080 of `TEHAS_HOST_ADDR` and the edge router on 8081. Two factories on one host would want the same ports |
| One Unix user who is in the `docker` group     | Everything tehas runs belongs to that user. The group makes it root-equivalent on this host, which is why the host is not shared |
| `~/.config/tehas/env`, mode 600                | The bot's GitHub token, the model endpoints' tokens, and the energy switches. Never in git. See [Secrets and trust](#secrets-and-trust) |
| An instance directory of its own               | `instance.env` and `models.json`, found through `~/.config/tehas/instance-dir`. See [Instance settings](#instance-settings) |
| Its own `TEHAS_DOMAIN`                         | The edge makes `uat-<pr>.` and `lab-<issue>.` names under it. Two factories under one domain answer each other's previews |
| The model gateway's `baseUrl` and key, when it uses one | `baseUrl` of the provider in `models.json`, with the token as `$TEHAS_MODEL_KEY_<NAME>` in `~/.config/tehas/env`. See [A backup model endpoint](#a-backup-model-endpoint) |
| `TEHAS_ENERGY_URL`, when the model is behind a gateway | The energy sampler asks `<baseUrl>/v1/cluster` with the model's token. A gateway that does not pass that path through needs the cluster URL named here, asked with no token. See [`tehasbot energy`](/tools#tehasbot-energy) |
| Disk for the sandbox image                     | About 10 GB once built, plus the product's own needs. [Bootstrap](/bootstrap#what-the-machine-needs) has the rest |

## Installing a factory

A new host installs from a release: the installer, then `tehas setup`, as
[A new project](/new-project#the-install) describes. The installer unpacks the
release into `~/tehas/versions/<version>`, points `~/tehas/current` at it and
runs that release's `bootstrap.sh root <user>`; `tehas setup` writes the
instance and runs `~/tehas/current/install.sh`.

Underneath are the two commands below, the same from a release and from a
platform checkout, run in this order, and run again as often as the platform
changes. Neither asks a question, so a program can run them with nothing on
standard input and nothing to answer:

```
sudo ./bootstrap.sh root <user>   # packages, Docker, the docker group, linger, the slice, then the pinned tools as <user>
./bootstrap.sh check              # one line per tool, and one for the slice
# the instance's instance.env and models.json are in place, and
# ~/.config/tehas/instance-dir names the directory, or instance.env sits in
# ~/.config/tehas itself; the bot's token and the model tokens are in
# ~/.config/tehas/env, mode 600
./install.sh                      # copy what this checkout has that the host lacks
./install.sh --check              # must exit 0 on a machine installed as the instance says
```

`install.sh` takes the instance from the host, as every command does: from
`TEHAS_INSTANCE_DIR`, else the one-line `~/.config/tehas/instance-dir`, else
`~/.config/tehas` itself. It takes no argument, so the first install and the
hundredth are the same command. See [Instance settings](#instance-settings).

Re-running them is safe. `install.sh` copies only the files that differ, keeps
the old one as `<name>.prev`, and takes `~/tehas/state/update.lock` so two runs
cannot overlap; a run that dies partway leaves its follow-ups owed and
`--check` says so. `bootstrap.sh` leaves a tool it did not install alone.

What an unattended run refuses to get past, and what each one means:

| The run stops with                             | What the machine is missing                                                      |
| ---------------------------------------------- | -------------------------------------------------------------------------------- |
| `tehas: <NAME> is not set (see …/instance.env)`, exit 78 | The instance directory, or a setting of it. Nothing is installed and nothing is written |
| `tehas: cargo is not installed…`               | `./bootstrap.sh user` has not run, so the build tools are not there. `install.sh` puts the tool directories `bootstrap.sh user` used on `PATH` itself, so it finds them without a login shell |
| `tehas: no slice file on this host`            | `sudo ./bootstrap.sh slice`, which is root's, and `--check` stays red until it has run |
| a tool directory named as writable by everyone or not owned by the user | One of the directories the tools live in (`~/.cargo/bin` among them). The run builds nothing until its mode is fixed (`chmod o-w`), because what cargo builds there is installed where every cycle runs it |
| a model key finding                            | `~/.config/tehas/env` does not hold a `TEHAS_MODEL_KEY_<NAME>` that `models.json` names |
| `tehas: the commit of <tree> is not known…`    | The tree is neither a git checkout with a commit nor a release with a `COMMIT` file. Nothing is installed. See [The state store](#the-state-store) |
| `tehas: the store could not init…` or `…migrate…`, exit 1 | The store's line above it says why: `corrupt`, `unreadable`, `future` or `busy`. The files are installed and the database is not changed. See [The state store](#the-state-store) |

`--check` exits 0 only when the instance loads, the slice file is the one this
checkout ships, the installed files match it, and no follow-up is still owed.
The lines it prints and what each one reports are in
[Configuration comes from git](#configuration-comes-from-git).

Whether this really works on a machine no one has prepared by hand is proven
on the [features page](/features); the test there is industry's first
acceptance test, and what it finds broken is fixed here.

`install.sh` refuses a `TEHAS_HOME` it cannot prove is tehas's, because it
removes what it takes for stale under it: the home carries the marker
`.tehas-home`, which a run leaves and nothing removes, and a home that exists,
is not empty and holds no marker is refused with exit 78 and one line naming the
path (`--check` says the same and exits 1).

## Instance settings

Everything that differs between instances is in the instance directory:

```
<instance dir>/
  instance.env     settings, shell KEY=value lines, no secrets
  models.json      Pi provider and model list, installed to ~/.pi/agent/
                   with the keys a model leaves out filled
  settings.json    optional: Pi's own settings, installed to ~/.pi/agent/
                   with the session model filled in from instance.env
  sandbox.Dockerfile  optional: a layer the sandbox image is built on top of,
                      by sandbox/build.sh. See
                      [the sandbox image](/new-project#5-the-sandbox-image)
```

The directory is found in this order: `TEHAS_INSTANCE_DIR` in the environment,
the one-line file `~/.config/tehas/instance-dir`, then `~/.config/tehas` itself.
The tehas-tehas host keeps its instance beside the secrets:

```
cat ~/.config/tehas/instance-dir
/home/boss/.config/tehas/instance
```

Rules that hold for every command:

- `instance.env` wins. A setting of the same name in the environment is
  dropped. Only `TEHAS_HOME`, `TEHAS_INSTANCE_DIR`, the energy switches
  `TEHAS_ENERGY` and `TEHAS_ENERGY_URL`, and seven hooks the tests use are
  taken from the environment.
- A missing or malformed setting stops the command with exit 78 and one line
  naming the setting, before it calls GitHub, Docker or git.
- Loading settings runs no git command, makes no network call and writes
  nothing.
- `instance.env` is shell, sourced as written. Keep it writable by the tehas
  user only. `./install.sh --check` reports it if it is not, and reads the
  file no further once it has a finding: the owner and mode of the instance
  directory, of `instance.env` and of `sandbox.Dockerfile` are checked before
  the settings are read. The last is not sourced, but it is built into the
  image every cycle runs, so its bytes carry the same trust.

The settings and their defaults are listed on the [commands page](/tools) and,
with comments, in `instances/example/instance.env`.

## Configuration comes from git

The platform repository is the source. Change it there, merge, then on the
host:

```
tehas update --check     # report what would change; changes no file of the
                         # checkout or of the host (origin/ refs do move)
tehas update             # pull, install, run the follow-ups that apply
tehas update --pending   # run only the follow-ups the record lists: no git
```

You do not have to remember to look. `tehas status` has a `platform` line
that says how many commits the checkout is behind origin's default branch,
which `tehasbot status` fetches at most once an hour, and `/health` says
`warn` once it has been behind for more than a day.

`tehas update` is the three steps that took hand work before: fast-forward
`~/tehas/platform` from its remote, run `install.sh`, then run each follow-up it
asks for by name (`units`, `sandbox`, `docs`, `edge`, `labels`: see
[Commands](/tools)).
One line per step, and it stops at the first failure. It checks the instance
settings against the required list in `lib/config.sh`, the same one install.sh
checks, before it calls git. It changes nothing when the checkout has local
changes or cannot be fast-forwarded, and it never enables a unit or starts a
container the host was not already running: a changed unit is reloaded and
restarted when it was active, and a docs or edge router container is recreated when
it is up. Caddy reads its config when the process starts, so a changed Caddyfile
needs the container brought back up. Asking Docker or systemd whether one runs and
getting no answer stops the update; only a real answer counts, not an exit status
on its own, and a systemd that did not reload the unit files stops the run rather
than report a changed unit as applied. A step that fails leaves the ones before it
applied, and what is still owed is kept in `~/tehas/state/` (`update-pending`)
until it has run, so the next update finishes it instead of calling the host
current.

The third form is for the host that cannot do the first two: `--pending` runs the
steps `update-pending` lists and calls no git command and no `install.sh`. A
checkout at a release tag, or a remote that is a local path which stopped
answering, stops a plain update at its first step and leaves the follow-ups of
the install it did run owed; `--pending` finishes them. `install.sh` and then
`tehas update --pending` is what a program runs to bring a host current without
moving the checkout. See [tehas update](/tools#tehas-update).

One update runs at a time. `tehas update` and a real run of `install.sh` hold
`~/tehas/state/update.lock`; a second one started meanwhile waits
`TEHAS_UPDATE_LOCK_WAIT` seconds for it (10 by default, 0 to refuse at once,
at most 300), and if it is still held prints `another update is running` and
exits 1 before it changes anything. The update hands its lock to the
`install.sh` it runs. `--check` takes no lock.

From a checkout of tehas on another machine, two scripts release and deploy.
`scripts/tag-release.sh` runs the gate on the head of the remote's default
branch in a temporary worktree, tags that commit as the next `vX.Y.Z` with an
annotated tag, and pushes the tag. It refuses a commit whose `CHANGELOG.md`
has no "Adopter action" for the release ([Releases](/tools#release-notes-and-the-adopter-action)).
The pushed tag starts the release workflow, which publishes the signed tarball.
When the head carries a polyfill that the newest pi-rukas no longer needs, it
opens a pull request that removes it and tags nothing until that is merged
([Polyfills](/tools#polyfills-of-pi-rukas-features)).
`scripts/tag-deploy.sh tehas-tehas` refuses
unless the newest tag is the head of that branch, runs
`. ~/tehas/env.sh && tehas update` on the host over ssh, then checks that the
host's checkout is at the tag. With `--check` it runs `tehas update --check`
instead. The ssh destination has to log in as the tehas user. Put the user and
key in `~/.ssh/config`, or give the ssh command in `TEHAS_SSH`:

```
Host tehas-tehas
  User boss
  IdentityFile ~/.ssh/<key>
```

Logged in as another account, the remote command fails with
`~/tehas/env.sh: No such file or directory`.

The steps are still there on their own. `install.sh` is what puts tehas on a host
the first time, before `tehas update` exists there:

```
cd ~/tehas/platform && git pull
./install.sh --check     # change nothing, report up to nine results
./install.sh --check --tests   # the same, and run the shell tests
./install.sh             # copy changed files into place, keep <name>.prev
```

A version that adds labels needs `tehas labels` run again after the install.
The labels `tehas:plan`, `tehas:planning` and `tehas:question`, for planning on
an issue and the retry tick, are not in a repository until it has been.

`--check` prints one line each for the home itself, the instance settings, the
host's tools, the
slice, whether the sandbox image reads a cycle's images (`Images:`), the
installed files, the command the other accounts type where root installed one,
the follow-ups an earlier run left undone, and the tests. The tests run only
with `--tests`; otherwise the line says `not run`. Three of the nine
appear only where there is something to say, so a current host prints five
lines, each `OK`. The sample below is a host with a follow-up owed and no
root-installed copy:

```
Instance:     /home/boss/tehas/platform/instances/tehas-tehas OK
Dependencies: OK
Slice:        MISSING (sudo ./bootstrap.sh slice)
Installed:    DRIFT (1 files)
  differs: /home/boss/tehas/libexec/dispatch
Follow-ups:   NOT DONE (2 from an earlier run, see /home/boss/tehas/state/update-pending)
Tests:        not run (./install.sh --check --tests runs them)
```

The installed files are compared by content and by mode: a command that matches
the checkout byte for byte but is mode 644 where the file list says 755 is not
installed. `--check` says `mode: <path> is 644, wants 755` and exits 1; a real
run sets the mode with `chmod` and copies the file again only when its content
differs too.

A home the script cannot prove is tehas's answers with a `Home: REFUSED` line,
the reason under it, and `not checked` for the three lines that would have read
the home; it exits 1 and looks at nothing else. `tehas update --check` reports
that as something an update does not fix: a person fixes the home or the setting
that named it.

The `Follow-ups:` line appears only while `~/tehas/state/update-pending` still
holds steps that were asked for and never ran. While it does, `--check`
exits 1, because a host with files matching and a step unapplied is not current.
Running `tehas update` runs them and clears the record
([Troubleshooting](/troubleshooting) has the case in full).

The record is also the way to finish an install on a host that cannot run an
update. A checkout at a release tag has no branch to fast-forward, and a remote
that is a local path can stop answering, while the copies `install.sh` made are
already in place. `tehas update --pending` reads that record, runs the steps it
lists in the order `lib/update-steps.sh` gives them, and calls no git command and
no `install.sh`, so it needs no checkout and no remote and no hold to be removed.
A program finishes an install with `install.sh` and then that command; see
[tehas update](/tools#tehas-update).

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
restarts nothing itself; `tehas update` is what runs them. `install.sh` records
each of them as owed while it copies the file that calls for it. That is why a run
that dies partway does not leave the host looking current. It asks systemd for nothing,
but for the one case below:
a copied unit file is not in effect until systemd has reloaded it, and that reload
belongs to the step that restarts the units. A file edited directly on the host
shows up as drift in `--check`.

A run that copies a changed `versions.env` also prints
`later: <checkout>/bootstrap.sh user   (versions.env changed; until it runs, --check says Dependencies: NOT OK)`.
`tehas update` rebuilds the sandbox image for that change but does not run
`bootstrap.sh user`: it fetches from the network and replaces the tools under
the tehas user's home and the pi-rukas checkout. Run it yourself, as the tehas
user, once the update has finished and no cycle is running:

```
cd ~/tehas/platform && ./bootstrap.sh user
```

The line is printed only by the run that copied the file. After that only
`--check` says it, as `Dependencies: NOT OK`, and only for a tool at another
version: a changed npm lockfile with the same version is not reported there
(see [Bootstrap](/bootstrap#3-check)).

pi-rukas is followed and not pinned, so its commit can move with no
`versions.env` to copy. `tehas update` prints the same line, once per run, when
the commit the sandbox image was built on is not the commit the host's clone in
`~/tehas/repos/pi-rukas` is on: the image moves to what is followed, the clone
moves only when you run the line. It reads the image's `tehas.rukas` label and
the clone's `HEAD`, and says nothing where either has no answer, so a host with
no clone or an image with no label is left as it was.

Each file is copied to `<name>.new` beside the installed one and renamed over
it. A cycle that starts during an install runs the old file or the new one,
never part of one, and an install that is stopped or fails partway leaves the
file it was copying as it was.

One file is written, not copied: `version.txt`, beside the pages in
`~/tehas/docs` and `~/tehas/fun`. It holds the newest tag of the checkout and
the commit it was at when `install.sh` last ran, as `v0.1.1+11b669d`, and the
footer of every page shows it. `--check` does not look at it.

Four installed files are compiled, not copied: the console of a running cycle,
the analytics program, the MCP server and the state store, Rust programs in the
Cargo workspace `console/` (the others are in `console/analytics/`,
`console/mcp/` and `console/store/`). A real run builds each with `cargo` in
the checkout and installs them as `~/tehas/libexec/console`,
`~/tehas/libexec/analytics`, `~/tehas/libexec/tehas-mcp` and
`~/tehas/libexec/store`. A host installed from a release tarball has neither
`cargo` nor `console/`: its programs are the ones the release packed
([Releases](/tools#releases)), and a release without a store installs only on a
host that has one. On a host with none, `--check` says so on a `Store: REFUSED`
line rather than as drift. Beside each, `<name>.stamp` holds two checksums:
of the source it was built from and of the program. A program is built again
only when its source has changed or the program on the host is not the one
that was installed. The console's source is `console/` without its tests and
without `console/analytics/`, `console/mcp/` and `console/store/`, so a change
to one of those alone builds that program alone. The store's source is
`console/store/` without its tests, and the workspace's `Cargo.toml` and
`Cargo.lock`. The analytics source is `console/analytics/` without its tests, the
workspace's `Cargo.toml` and `Cargo.lock`, and the console's mask
(`console/src/mask.rs`), which it shares. Each build asks `cargo` for
`--offline` first, so a host that has fetched once installs with no network.
The build that fetches is the slow one and takes a minute or two: analytics
compiles SQLite. When cargo says a crate is missing from its cache the build
is run once more without `--offline`, with a line that says so; a build that
fails again is reported from both attempts. A build that fails for any other
reason, a compile error among it, is reported as it is and not tried again.
One build that outlives `TEHAS_BUILD_TIMEOUT` is cut off and named as cut off;
the remedy is `./bootstrap.sh user`, which installs cargo and its crate cache,
or a longer `TEHAS_BUILD_TIMEOUT`. `--check` compares the checksums and builds
nothing, so a program replaced on the host shows up as drift like any other
file. Every program that is due is built before any is installed, so one that
does not build stops the install before it has changed anything, and so does a
host without `cargo`. The build is
given no environment variable whose name holds TOKEN, SECRET, PASSWORD or
API_KEY. The cycle and the dispatcher call `~/tehas/libexec/analytics`
([tehasbot cycle](/tools#tehasbot-cycle), [tehas dispatch](/tools#tehas-dispatch)). The Deno
script the console replaced, `~/tehas/libexec/console.ts`, is removed the way
the old commands below are.

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

### Updating tehas

A host installed from a release updates from releases: `tehas update --check`
says which version is installed, which is newest and what each release between
asks the adopter to do, and `tehas update` moves to the newest. Each version is
unpacked beside the others in `~/tehas/versions/<version>`, and
`~/tehas/current` points at the one in use. A host that develops tehas sets
`TEHAS_UPDATE_FROM=git` and updates from its platform checkout. See
[tehas update](/tools#tehas-update).

### Move a checkout host to releases

A host installed before releases has a platform checkout at `~/tehas/platform`
and no `~/tehas/current`, so `tehas update` keeps pulling the checkout. To move
it to releases, once a signed release is published:

1. Install minisign, which checks the signature: `sudo apt-get install minisign`.
2. Check that `~/tehas/release/minisign.pub` is there. `install.sh` has put it
   there since the key was added; run `tehas update` once from the checkout if
   it is missing.
3. Set `TEHAS_UPDATE_FROM=release` in `instance.env`.
4. Pause and let running cycles end (`tehas pause`), since an update that
   changes a unit restarts the units that run.
5. Run `tehas update --check`, read the Adopter action of each release it
   lists, then run `tehas update`, then `tehas resume`.

The checkout stays where it is and is no longer read. To go back to it, set
`TEHAS_UPDATE_FROM=git` and run `./install.sh` from the checkout.

### Rolling back tehas

On a host that updates from releases, go back with
`tehas update --to <version>`. A version still in `~/tehas/versions` is not
downloaded again, and its sandbox image is put back if it still has its tag. To
stay there, hold the host, or the next `tehas update` moves it to the newest
again:

```
touch ~/tehas/state/update-hold     # tehas update refuses while this exists
tehas update --to v0.8.0            # --to runs with the hold there
```

`install.sh` keeps one `<name>.prev` beside each file it replaces, and only
one: a second install that changes the file replaces it. Copying a `.prev`
back shows as drift, and the next install undoes it. On a host that updates
from its checkout, the way back is the checkout. Hold it first, or the next
`tehas update` fast-forwards it to the release you left:

```
touch ~/tehas/state/update-hold     # tehas update refuses while this exists
cd ~/tehas/platform
git log --oneline                   # the release to go back to
git reset --hard <commit>
./install.sh
```

`git checkout <tag>` does not do: `tehas update` refuses a detached checkout.
While `~/tehas/state/update-hold` exists, `tehas update` exits 1 naming the
file and changes nothing, and `tehas update --check` says the hold is there.
`tehas update --pending` runs with the hold there and from a detached checkout,
because it moves nothing: the `next:` lines `install.sh` printed are its, not
steps you have to work through yourself. Run by hand, they are:

| Step      | By hand                                                                          |
| --------- | -------------------------------------------------------------------------------- |
| `units`   | `systemctl --user daemon-reload`, then `systemctl --user restart` the units named |
| `sandbox` | `~/tehas/sandbox/build.sh`, or put an older image back by its date tag, as in [Rolling back pi-rukas](#rolling-back-pi-rukas) |
| `docs`    | `docker compose -f ~/tehas/edge/docs.compose.yaml up -d --force-recreate`         |
| `edge`    | `docker compose -f ~/tehas/edge/router.compose.yaml up -d --force-recreate`       |
| `labels`  | `~/tehas/bin/tehas labels`                                                          |

They stay recorded in `~/tehas/state/update-pending`, and `install.sh --check`
reports them, until `tehas update --pending` or a `tehas update` runs them. When a fixed release is out,
remove the hold and run `tehas update`.

### The state store

`~/tehas/state/tehas.db` is the state store's database: an SQLite file, mode
600, in `state/`, which is kept mode 700. With WAL on, its `-wal` and `-shm`
files beside it hold rows too, so a readable `state/` would be a readable copy.
No container is given it. Only one program writes it: the store,
`~/tehas/libexec/store`, which `install.sh` builds and installs with the
console, or takes from the release.

What is in it today: the schema, version 2 (`PRAGMA user_version`); `meta`,
which says when the database was made, by which version and commit of tehas,
for which instance and repository, and when records were last imported
(`imported_at`); and the cycles' records: the approval
each start rests on (the dispatcher's), each cycle's start and end with its
image, settings, branch, result and progress (`tehasbot cycle`, `tehas work`,
`tehas stop`), and the last 50 lines of history of each issue; each park and
its question (`tehasbot cycle`, the dispatcher); and each planning session,
with its brief, its answers, the issues it filed, its open question and the
last 50 lines of history of its turns (the dispatcher). What a planning turn
carries between turns is a directory beside its workspace,
`~/tehas/work/<name>-plan-<issue>.keep`. The commands run the store through `~/tehas/lib/store.sh`;
`tehas dispatch`, `tehasbot cycle`, `tehas stop` and `tehas history` stop with
exit 78 before anything else when it is not installed.

`install.sh` makes and migrates it on every real run, under
`state/update.lock`:

- When there is no `state/tehas.db`, it runs `store init`.
- Otherwise it runs `store migrate --backup ~/tehas/state/backup`. The store
  first copies the database into `state/backup/tehas-<version>-<time>.db`, mode
  600, with SQLite's `VACUUM INTO`, and then brings the schema forward. A copy
  that cannot be made changes nothing. A database already at the version the
  store writes is still copied, so every install leaves one copy. Nothing
  removes old copies yet; remove them by hand when there are too many. A
  store at version 1 is migrated to 2 in place, and the install keeps the
  database as it was under both names, `tehas-1-<time>.db` and
  `tehas-2-<time>.db`.
- Then it runs `store import`: the records an older tehas kept as files (the
  cycle's under `state/cycles/`, `state/park-<n>.id`, and each planning
  session's `state/plan-<n>.json` with its brief, answer and history) become
  rows, and the files move to `state/imported-<YYYYMMDD>/`, under their paths
  in `state/`, an issue's files together once all their rows read back. Nothing
  is deleted. Each history keeps an issue's 50 newest rows, as the store does
  for new ones. The files a cycle hands to the next step (`result.json`,
  `progress.json`, a plan's `.out` and the rest) and `state/refined.jsonl` stay
  where they are. A run again changes nothing that is in the store already, and
  finishes a run that stopped partway.

The store is told the version and commit of the tree `install.sh` runs from,
the instance directory's name and `TEHAS_REPO`. A checkout gives the commit from
git; a release carries it in its `COMMIT` file. `install.sh --check` does not
run the store and writes nothing.

A store that fails stops the install with exit 1 and its own line, which starts
with one of these words:

| Word         | What it means                                                         |
| ------------ | --------------------------------------------------------------------- |
| `corrupt`    | The file is not an SQLite database, or fails its check                 |
| `unreadable` | The file or its directory cannot be read or written, or a backup cannot be made |
| `future`     | The database has a newer schema than this store knows, as after going back to an older release |
| `busy`       | Another program held the database for more than five seconds          |

The store never moves a damaged file aside. Read what the database holds with:

```
TEHAS_HOME=~/tehas ~/tehas/libexec/store check
```

It prints the schema version, SQLite's integrity check and the rows in each
table, and writes nothing.

**Backing it up.** Copy the file with SQLite, not with `cp`, while tehas runs:
`cp` of a WAL database can take the file without the rows still in its `-wal`.
The copies in `state/backup/` are made that way. For one more, run
`./install.sh` (`~/tehas/current/install.sh` on a host on releases), or, with
`sqlite3` installed:

```
sqlite3 ~/tehas/state/tehas.db "VACUUM INTO '/path/to/tehas-copy.db'"
```

Keep the copy mode 600: it holds what the database holds.

**Restoring it.** Pause the factory and let running cycles end, so nothing
writes the database while it is replaced. Then put the copy in place, remove
the old database's `-wal` and `-shm`, and run the install again, which migrates
the copy to the schema of the installed store:

```
tehas pause
cp ~/tehas/state/backup/tehas-<version>-<time>.db ~/tehas/state/tehas.db.new
chmod 600 ~/tehas/state/tehas.db.new
rm -f ~/tehas/state/tehas.db-wal ~/tehas/state/tehas.db-shm
mv ~/tehas/state/tehas.db.new ~/tehas/state/tehas.db
./install.sh            # or ~/tehas/current/install.sh on a host on releases
TEHAS_HOME=~/tehas ~/tehas/libexec/store check
tehas resume
```

After going back to an older release, the install stops with `future` when the
newer release had migrated the database. Restore the copy that migration made
first: a migration that changes the schema leaves the database as it was under
two names, `tehas-<old>-<time>.db` and `tehas-<new>-<time>.db`, and either is
the one to restore.

## Models

Three files name a model and they do different things:

| Where                                                    | Controls                                                         |
| -------------------------------------------------------- | ---------------------------------------------------------------- |
| `PI_ENSEMBLE_SUBAGENT_MODEL`, `_PROVIDER` in `instance.env` | The session, the agent driving the cycle, and every subagent role that has no model of its own |
| `.pi/settings.json` in the product repository, optional  | Provider and model of the session, over the instance's           |
| `PI_ENSEMBLE_MODEL_<ROLE>`, `PI_ENSEMBLE_PROVIDER_<ROLE>` in `instance.env` | One role                                       |
| `models.json` in the instance directory                  | What the provider is and which models it offers                  |

Child agents do not inherit the session's model. Without the instance setting
they fall back to the first model in `models.json`, which may be a small one.
Changing the instance setting does not change the session's model.
`./install.sh --check` verifies that both named models are listed in
`models.json`.

In the example every role and the session use `qwen3.8-flash-next` on the
provider `fleet`.

What the endpoint serves right now:

```
curl -s "$(jq -r '.providers.fleet.baseUrl' ~/.pi/agent/models.json)/models" | jq -r '.data[].id'
```

An endpoint that needs a token (see below) answers 401 to this. Add
`-H "Authorization: Bearer $TEHAS_MODEL_KEY_MAIN"`, from a login shell, which
has the tokens.

## A backup model endpoint

A provider in `models.json` may name a second endpoint that serves the same
models, and a token for each:

```
"fleet": {
  "api": "openai-completions",
  "baseUrl": "https://llm.example/v1",
  "apiKey": "$TEHAS_MODEL_KEY_MAIN",
  "backupBaseUrl": "https://fleet.example/v1",
  "backupApiKey": "$TEHAS_MODEL_KEY_FLEET",
  "models": [ ... ]
}
```

- An `apiKey` is a word, such as `unused` for an endpoint that takes no key,
  or `$TEHAS_MODEL_KEY_<NAME>`. The token itself goes in `~/.config/tehas/env`,
  mode 600, as `TEHAS_MODEL_KEY_MAIN=...`. Never in `models.json` or
  `instance.env`: both are in a repository. `instance.env` that sets a
  `TEHAS_MODEL_KEY_` stops every command with exit 78.
- Any other `$` or `!` in an `apiKey` stops every command that starts a
  container, and the dispatcher, with exit 78, and so does a name that is not
  set. `./install.sh --check` reports both. It reads the tokens from
  `~/.config/tehas/env` itself, so a first install, or an update from a
  version that did not know them, finds them there.
- A running service keeps the tokens it started with. After you change one
  in `~/.config/tehas/env`, run `systemctl --user restart tehas-energy`; the
  dispatcher and each cycle read the file afresh. An update that changes
  `env.sh` restarts the running units itself.
- `backupApiKey` defaults to `apiKey`.
- Either endpoint can be any OpenAI-compatible endpoint, a
  [viiwork](https://github.com/janit/viiwork) mesh or not. Both are asked the
  same way: `GET /models`, with the token.

At each poll the dispatcher asks the main endpoint whether it offers the
models the agents use on that provider. When it does not answer or does not
offer one, it asks the backup. The poll holds only when both fail:

```
holding: no resources: the model endpoint does not answer, and its backup offers no models
```

While the backup is in use, each poll logs
`model endpoint: provider fleet is on its backup, the main endpoint does not answer`.

`tehas work`, `tehas plan` and `tehas dev` ask again when they start a
container. They give it a `models.json` of its own,
`<state directory>/models.json`, on the endpoint that answered and without
the backup keys, and the tokens it names as variables, by name. `tehas work`
prints `tehas work: model endpoint: provider fleet is on its backup` and writes
it to the preflight log. A container stays on its endpoint: when the main one
fails mid-cycle, Pi retries it as [Provider retry](#provider-retry) says, and
the next cycle starts on the backup. The improvement loop and the image
descriptions pick the same way.

The energy sampler asks the endpoint the dispatcher settled on for the
subagent provider, the one word in `~/tehas/live/dispatch.json`, and samples
it only when it is a viiwork mesh. The cycles and the meter are then the same
endpoint. With no word in that file, because the dispatcher has not settled
one yet, it asks the main endpoint and then the backup as before, so an update
of the dispatcher never blanks the floor. An endpoint that is not viiwork gives
no meter, even when the backup is one: the section hides.

## Provider retry

Pi retries a failed model request only as its `settings.json` says. The sandbox
image has no `settings.json`, so every container is given the one `install.sh`
put in `~/.pi/agent`, read-only: the instance's, or the generic one in
`host/pi/settings.json`, with `defaultProvider` and `defaultModel` from
`PI_ENSEMBLE_SUBAGENT_PROVIDER` and `_MODEL` when the file does not name them.
The generic one sets:

| Key                              | Value  | Without it (Pi 1.0)                          |
| -------------------------------- | ------ | -------------------------------------------- |
| `retry.provider.timeoutMs`       | 600000 | the HTTP idle timeout, 300000                |
| `retry.provider.maxRetries`      | 8      | 0: a failed request is not retried           |
| `retry.provider.maxRetryDelayMs` | 60000  | 60000                                        |
| `retry.maxRetries`               | 10     | 3                                            |
| `retry.baseDelayMs`              | 5000   | 2000                                         |
| `retry.maxAgentDelayMs`          | 120000 | 60000                                        |

The `retry.provider` keys retry one HTTP request. The other three are Pi's
second layer: when a turn still ends in a retryable error, such as `429 no
free slot`, the agent waits and runs the turn again, doubling the wait up to
`maxAgentDelayMs`. Against a gateway that holds each request 20 s for a slot,
Pi's defaults give up after about 80 s. These values keep trying for at least
16 minutes. A busy model is a reason to wait, not to end the cycle. The
dispatcher does not count model slots before it starts a cycle: how busy the
model is changes faster than a poll.

A product's own `.pi/settings.json` is merged over it, so a product can change
these for its cycles. `tehas work` writes the values in effect to the cycle's
preflight log and prints them:

```
tehas work: provider retry: timeoutMs=600000 maxRetries=8 maxRetryDelayMs=60000 httpIdleTimeoutMs=unset agentMaxRetries=10 agentBaseDelayMs=5000 agentMaxDelayMs=120000
```

`unset` means Pi's default. Without `~/.pi/agent/settings.json`, `tehas work`,
`tehas plan` and `tehas dev` stop with exit 78 before they call Docker or git
or write anything: run `install.sh`.

A host name that only resolves on a private network is not known inside the
sandbox. Give the sandbox its address with `PI_ENSEMBLE_HOST_ALIASES`
(`name:address`, comma separated).

### A model per role

Every agent of every cycle asks for the same model by default. Several cycles
at once, each with six lens reviewers, can take every slot that model has, and
then calls are refused with `429 no free slot`. Give a role a model of its own
to spread the calls. In `instance.env`, where `second-model` is another model
`models.json` lists under `fleet`:

```
PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=second-model
PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST=fleet
```

| Role (`<ROLE>`)          | What it does in a cycle                         |
| ------------------------ | ----------------------------------------------- |
| `EXPLORE`                | Reads the code before the plan                  |
| `DEVELOPER`              | Writes the change, one per workstream           |
| `ADVERSARIAL_DEVELOPER`  | Reviews the change and argues against it        |
| `CODE_REVIEW_SPECIALIST` | The lens review: all six lenses are this role   |
| `OPS`                    | Makes the branch, writes the handoff            |

These are the roles the pinned pi-rukas reads a model for. The session (the
project manager, which plans) is not among them: its model is the product's
`.pi/settings.json`. The six lenses are one role, so they share one model;
the pinned pi-rukas cannot give one lens a model of its own.

- A role that is not set uses `PI_ENSEMBLE_SUBAGENT_MODEL`. With none set,
  nothing changes.
- `_PROVIDER_<ROLE>` defaults to `PI_ENSEMBLE_SUBAGENT_PROVIDER`.
- Any other role name, a provider without its model, or a value of more than
  one word stops every command with exit 78 before it does anything. A
  misspelt role would otherwise run on the default model without a word.
- `./install.sh --check` reports a role's model that `models.json` does not
  list under its provider. The dispatcher holds starts while an endpoint does
  not offer one of the models, as it does for the default model.
- The settings reach the cycle's container by name, as every `PI_ENSEMBLE_*`
  setting does.

A role's model is still a model that can be busy. Nothing counts the slots a
model has; spread the roles over models that different machines serve.

## Limits and concurrency

Every sandbox container tehas starts runs in one systemd slice,
[`tehas.slice`](#the-slice): a cycle, a planning turn, a `tehas dev` session,
the indexer and the improvement loop's analyst. The slice's `MemoryMax` bounds
them all together. No setting describes the machine.

A cycle, a planning turn, a dev session and the indexer each get the same
seven limits, fourteen words on the `docker run` line:

| Option | Value | What it does |
| --- | --- | --- |
| `--cgroup-parent` | `tehas.slice` | Puts the container in the slice |
| `--memory` | a quarter of the slice's `MemoryMax` | A hard ceiling. At it the kernel kills the largest process in the container. The dispatcher starts no cycle while four containers are in the slice, so their ceilings never sum to more than the slice |
| `--memory-swap` | the same value | No swap for the container |
| `--memory-reservation` | `TEHAS_CYCLE_RESERVE`, `2g` unless set | The container's floor. When the slice is full the kernel frees memory above the floors first |
| `--cpus` | the host's CPUs less one, divided by `TEHAS_TICKET_CONCURRENCY` counted at most 4, never below 1 | A CPU quota. The container may use no more CPU time than that many CPUs |
| `--pids-limit` | 1024 for each CPU of the quota, rounded up, from `2048` to `4096` | The most processes and threads in the container |
| `--cpu-shares` | `1024` | A CPU weight, the same on every container |

On a host with 16 CPUs and `TEHAS_TICKET_CONCURRENCY=4` a container gets
`--cpus 3.75 --pids-limit 4096`; with 8 CPUs and the default ceiling of 7,
`--cpus 1.75 --pids-limit 2048`.

Every sandbox container also runs with `--init`: a cycle, a planning turn, a
dev session, the indexer and the analyst. Docker's init is the first process
of the container, and it collects every process whose parent has gone. Pi is
the command of the container and collects only its own children. A process that
outlives its parent, as git's background housekeeping does, is handed to the
first process of the container, and without an init it is never collected and
counts against `--pids-limit` until the container ends. One pass of this
repository's shell tests leaves about 950 that way, so a cycle reached the
limit at its fifth pass and could start no process after that.

The quota is a ceiling, not CPUs held for the container: what one container
leaves idle the others may use, and several busy ones share by weight, about
equally. The weight is also what makes the kernel share CPU inside the slice
at all, so every container carries it. Without the quota one container's gate
could take every CPU of the host, the model backends' among them. The cycles
are counted at their ceiling, not as they run, so a cycle that starts first
gets no more than one that starts later. At most four containers are in the
slice at once (see [Limits that hold work back](/operating#limits-that-hold-work-back)),
which is why the ceiling counts at most 4.

The pids limit follows the quota because a container's processes are mostly
what its gate runs side by side, and a gate sized by its CPUs runs fewer on
fewer. Measured on this repository's shell tests with an init: at most 108
processes at once with 16 test files side by side, 38 with 4. The floor of
2048 is for the agents, their tools and git around the gate.

Inside the container `nproc` still says how many CPUs the host has: a quota
does not change it. A gate that runs its tests side by side should size
itself from `/sys/fs/cgroup/cpu.max`, as this repository's `scripts/test.sh`
does, or it starts more than the quota can run.

The quota also sets how many runs of the gate pi-rukas makes at once inside the
cycle: `tehas work` passes `PI_ENSEMBLE_VERIFY_CONCURRENCY`, one for each two
whole CPUs of the quota and each 2 GiB of the memory ceiling, the lower, at
least 1. That is pi-rukas's own rule for a host, applied to the container. A
run that waits for another does not count its wait against
`TEHAS_VERIFY_TIMEOUT`.

The floor is a priority when the slice frees memory, not a guarantee against
the whole host: when the host itself runs short, the host comes first. With
the default reserve the host needs about 11 GiB of memory, because the reserve
may not be more than a container's ceiling. A smaller host sets
`TEHAS_CYCLE_RESERVE` lower.

The analyst is a batch job with limits of its own: the slice, one CPU
(`--cpus 1`), `--memory 2g`, the same for `--memory-swap`, `--pids-limit 1024`
and the same CPU weight. The check that the sandbox can push, which `tehas
work` runs for a few seconds before a cycle, is not in the slice and has no
limits. It is given the security options below. Previews, image builds and the
edge router are outside the slice too.

### What a container may not do

A cycle, a planning turn, a `tehas dev` session and the push check before a
cycle all get the options below, from `tehas_sandbox_security` in
`lib/sandbox.sh`, beside the mounts and the variables. The list is
capability-shaped options and this round's bounded `/tmp`. The indexer and the
analyst are not given them yet (issue #54 leaves them to a follow-up), so the
bounded `/tmp` below is not in those two containers either: when that follow-up
calls the function, it must say what each of the two gains, and not take the
`--tmpfs` as a whole bundle by default.

Each of these containers also starts as the tehas user, with
`--user "$(id -u):$(id -g)"`, not as root. The image's entrypoint drops from
root to its own user with `setpriv`, which needs `SETUID` and `SETGID`; started
as that user it skips the drop. The image is built with the tehas user's uid
(`tehas/pi-rukas:uid<uid>`), so the uid has a name and a home inside it.

| Option | Value | What it does |
| --- | --- | --- |
| `--cap-drop` | `ALL` | Takes away every capability Docker grants by default: `CHOWN`, `DAC_OVERRIDE`, `FOWNER`, `FSETID`, `KILL`, `NET_BIND_SERVICE`, `SETFCAP`, `SETGID`, `SETPCAP`, `SETUID`, `SYS_CHROOT`, `MKNOD`, `AUDIT_WRITE`, `NET_RAW` |
| `--cap-add` | `CHOWN` | `npm install` drops a package to its owner with `chown(2)`, and Playwright's install of Chromium copies a cache tree that way. Both run in a gate of the product. Without it the gate fails with `EPERM` |
| `--cap-add` | `FOWNER` | `npm` moves its staging directory onto its destination with `rename`, which is refused for a file one does not own even when the directory is writable. `tar`'s unpack, `cargo install` of a built crate and a gate's own `mv` do the same |
| `--security-opt` | `no-new-privileges:true` | No process in the container gains rights through its mode bits or file capabilities, whatever it runs |
| `--tmpfs` | `/tmp:rw,exec,nosuid,nodev,size=8g,mode=1777` | A `/tmp` that counts against the container's memory instead of the host's disk: a runaway log there ends the container, where the writable layer it replaces sits on the host's root disk with no limit. The host keeps its own scratch under `$TEHAS_HOME/tmp` and sets `TMPDIR` there for its own commands, and `TMPDIR` is not among the variables given to a container, so a tool inside one that honours `TMPDIR` writes to this bounded tmpfs. `size=8g` is an upper bound that binds only above a container ceiling of 8 GiB, which needs a host of about 43 GiB since the ceiling is a quarter of a slice set at 75% of the host: on a smaller host the `--memory` kill is what ends the container first, so the cap in force is the smaller of the two. Nothing prunes it during a cycle, and `tehasbot clean` reclaims the host's scratch and nothing of a running container's, so what a long cycle leaves there is what it runs out of. A cycle that fills it is expected to end there. It stays `exec` because Docker's own default for a tmpfs is `noexec` and a product's gate is expected to write and run helpers there through npm, cargo and Playwright; no host of ours has been seen to need the bit, so that reason is an expectation, not an observation |

What is left off the list on purpose: `SETUID` and `SETGID`, because nothing a
cycle runs becomes another user (the image has no `sudo`, the container runs as
one uid, and tehas gives it no setuid program to reach another); `SYS_ADMIN`,
the one capability that would let a container mount anything, which Chromium's
own sandbox would want, and Playwright's Chromium is run with `--no-sandbox`
instead; and `NET_RAW`, which nothing needs: the agents reach GitHub and the
models over HTTP with `curl`, `git`, `npm` and `cargo`, none of which opens a
raw socket. `no-new-privileges:true` breaks `sudo` inside the image, and
nothing in `host/sandbox/Dockerfile` installs it or relies on it.

The root filesystem stays writable, unlike a preview's: a cycle's gate installs
into its own tree and its caches, and a read-only root would need one `--tmpfs`
for every directory a tool writes in. The one `--tmpfs` above is given all the
same, so that the directory a tool writes the most into is the bounded one;
the rest of the writable layer stays unbounded. The process table is bounded by
`--pids-limit`, in the table above.

`test/work_test.sh`, `test/plan_test.sh` and `test/dev_test.sh` pin each of
these words on the `docker run` line of their own container, and
`test/work_caps_test.sh` pins the function that prints them. Whether a whole
cycle still builds, tests and drives a headless browser with them is not yet
seen on a host: see the sandbox row of the [features page](/features).

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

The starts of a process that `--pids-limit` refused are counted the same way,
from the container's `pids.events`:

```
tehas work: this cycle's container reached its limit of processes, 41 starts were refused
```

A cycle whose container is at that limit can run no command, so its gate does
not run and its handoff may not be posted. The comment that reports the cycle
says the count, and the dispatcher's `done` line has it as `forks_refused`.

A container that ends with 137 while the cycle was still running is reported
as `killed`: `tehas work` exits 137, and the issue is parked with
`needs-human-attention` and status `killed` in the comment. That is a
container that ran out of memory, or one that was stopped from outside, for
example with `docker kill`. It is not `killed` when `tehas work` stopped the
container itself, at the time limit or on a signal, nor when the runner had
already written its result. The record for the improvement loop carries the
count as `oomKills`.

#### Time limits

`TEHAS_WORK_TIMEOUT` bounds the cycle itself, and `TEHAS_WORK_STALL` a cycle
that writes nothing. Before the limit comes the cycle's deadline,
`PI_ENSEMBLE_CYCLE_DEADLINE`: a quarter of `TEHAS_WORK_TIMEOUT` before the end,
or one run of the gate and the handoff allowance when that is longer. The
allowance is 8 minutes, and pi-rukas is given it as
`PI_ENSEMBLE_HANDOFF_TIMEOUT_MS`, so both sides count the same time for the
handoff. An image whose `--capabilities` has the feature
`cycleDeadlineHandoffAllowance` bounds every step by the deadline, so its
deadline is `TEHAS_WORK_TIMEOUT` less the allowance alone. After it pi-rukas
starts no new step and parks the cycle with the cap `deadline` and a handoff,
so a cycle that runs long ends at a step boundary with its branch and pull
request said, not in the middle of a step. A step that started before the
deadline may run past it; the margin is there for that step. The runner is
given `TEHAS_WORK_TIMEOUT` in whole minutes and ends the cycle itself, with
exit 30; `tehas work` stops a runner that is still there 300 seconds later.
Either way the cycle is `timeout` and goes on record with the cap
`wall-clock` and the step it was in. The steps around it are bounded too, so
a dockerd that stops answering or a network that swallows packets does not
hold a slot for ever:

| Step | Bound | After it |
| --- | --- | --- |
| The image's `--capabilities` answer before a cycle | 120 s | `tehas work` exits 78, with `gave no --capabilities answer` |
| The push check before a cycle | 300 s | `tehas work` exits 3, with `the push check did not end within 300s` |
| The clone of the cycle's workspace | 300 s | `tehas work` exits 78 |
| `docker stop` at the end of a cycle, and on a signal | 60 s | `docker kill`, bounded the same way, and the line `docker stop of tehas-work-N did not end within 60s, killing it` |
| The console at the end of a cycle | 3 s, then 30 s after it is asked to stop (`TEHAS_CONSOLE_WAIT`) | The console is killed, with `the console did not stop within 30s, killing it` |
| A call to GitHub in the report of a cycle that was stopped | 20 s, tried once | The call counts as refused. A label change that never happened leaves the issue `tehas:running` for the next poll |
| A call of `~/tehas/libexec/analytics` from a cycle or a poll | 5 s, then a kill 1 s later | `WARN analytics <subcommand>: exit N`, and the cycle or the poll goes on as without it |
| The stop of a cycle, its container and its report | 300 s (`TimeoutStopSec` of `tehas-cycle@.service`) | systemd kills the unit. The worst stop within the bounds above is about 277 s |
| The dispatcher's fetch of the product checkout | 120 s | `WARN: could not fast-forward`, and the poll goes on |
| A poll of the dispatcher | 1 hour (`TimeoutStartSec` of `tehas-dispatch.service`) | systemd stops it. A poll runs no planning turn and no index run, so it has none to cut off |
| A planning turn | `TEHAS_PLAN_TIMEOUT`, then a quarter of an hour (`RuntimeMaxSec` of `tehas-plan-<issue>.service`) | The next poll reports the turn as cut off |
| The code index | 30 minutes for the indexer, then 40 minutes for the unit (`RuntimeMaxSec` of `tehas-index.service`) | The index stays as it was. The next poll starts the cycles |

`tehas-cycle@.service` stops a cycle after 8 hours (`RuntimeMaxSec`), as the
last bound for a hang that none of these reaches. The stop is a TERM, and the
cycle is reported as one that stopped. A unit cannot read the instance's
settings, so `TEHAS_WORK_TIMEOUT` may be at most 25200, 7 hours: a longer one
is refused with exit 78. The hour between is for the stop and the report.
`TEHAS_PLAN_TIMEOUT` may be at most 7200, 2 hours: the unit of a turn runs to
the limit plus a quarter of an hour, and one turn runs at a time. A planning
turn and the index run each run as their own unit, not inside a poll, so they
hold no poll and not its lock. A poll that finds the dispatcher's lock held
for longer than half an hour says so once in the log, as
[troubleshooting](/troubleshooting) shows.

`TEHAS_TICKET_CONCURRENCY` sets the most issues worked on side by side, each
in a cycle, a container and a workspace of its own. It defaults to the host's
CPUs minus one, at least 1, and a value above the host's CPUs is accepted. It
is a ceiling: the dispatcher starts fewer while the slice has no room. A
container's CPU quota and pids limit follow it (see above); its memory does
not. There is no CPU or memory budget to set; an instance that still sets
`TEHAS_WORK_CPUS` or `TEHAS_WORK_MEMORY` is refused with 78 and one line naming
the slice file.

`TEHAS_CONCURRENCY` sets how many workstreams one cycle may run side by side,
inside that one container. It defaults to 1, and pi-rukas is always told it as
`PI_ENSEMBLE_MAX_WORKSTREAMS`.

A workstream is one developer agent and its review loop in their own worktree.
pi-rukas plans up to that many per issue and folds anything beyond the limit
into the last one, so no work is dropped at a low setting; it is done in
sequence by fewer agents. pi-rukas plans parts that touch the same files as
one workstream, but not parts that only share a symbol one of them adds: each
part starts from the same commit without the others' new types and exports,
so each fails its type check alone.

The factory has one axis of parallelism: issues side by side, one workstream
each. The two settings would otherwise multiply, and with them the agents
calling the model backends and the gates on the same CPUs: 4 issues of 4
workstreams are 16 developer agents and up to 16 full gates at once, in four
containers' quotas. So an instance may set `TEHAS_CONCURRENCY` above 1 only
with `TEHAS_TICKET_CONCURRENCY=1`, one issue at a time. With both above 1 every
command stops with exit 78 and one line that names both:

```
tehas: TEHAS_CONCURRENCY (4) and TEHAS_TICKET_CONCURRENCY (7) are both above 1: choose one, ...
```

The default ceiling is the host's CPUs minus one, so on most hosts an instance
that wants several workstreams also sets the ticket ceiling:

```
# in the instance's instance.env, then pull and ./install.sh on the host
TEHAS_TICKET_CONCURRENCY=1
TEHAS_CONCURRENCY=4
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
- Each cycle writes only in its own workspace and in `~/tehas/work/<name>-<issue>.state`,
  the directory made for it: its copy of the code index, its memory, Pi's
  sessions. What it caches under its home otherwise lives in its container's
  own layer and goes with the container. A cycle reads nothing another cycle
  wrote.
- The cost is a cold start. Every cycle copies the index again (a second or two
  for this repository: the databases, not the indexer's logs or the staging
  files of a run that was stopped), and a product whose gates install packages
  downloads them again: minutes once per cycle for a large dependency tree,
  nothing after the first install within the cycle.
- A host upgraded from the version that mounted two Docker cache volumes,
  `pi-ensemble-cache` and `pi-ensemble-bun-cache`, still has them on the disk
  with what those containers wrote. Nothing mounts them now, and `tehas index`
  writes the code index into `~/tehas/state/index` instead. Removing the two
  volumes is a person's call: `docker volume rm` names them.
- The same change stopped mounting `~/.vipune` and Pi's `~/.pi/agent/sessions`.
  What earlier cycles and planning turns wrote there is still on the host and
  nothing mounts it now; a cycle keeps its memory and sessions in
  `~/tehas/work/<name>-<issue>.state`. Removing those two directories is a
  person's call too, and nothing moves what is in them: a manual session starts
  a new history in `~/tehas/dev/<name>.state/sessions`, and so does a cycle.

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
equal terms. There is no CPU quota on the slice; each container has its own
(see above).

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
user remapped to the tehas user's id, shellcheck, Playwright's Chromium with
fonts, `file`, and a Python image library. The last two are in the container
only: `bootstrap.sh` installs neither and its check does not look for them.
cargo's registry belongs to that user, so a gate that builds Rust can fetch its
crates; they go with the container. The base is pinned by digest in
`sandbox/Dockerfile`, shellcheck by checksum, codebase-memory-mcp by the
checksum of its installer plus a version read back from the binary
(`CBM_INSTALLER_SHA256` and `CBM_VERSION`), and Playwright by the npm tree
beside it (`sandbox/npm/playwright`, whose `package-lock.json` fixes every
package in that tree by hash). `versions.env` holds the versions and the
checksums, and the build stops when a pin there does not match the file beside
the Dockerfile. A product whose gates need another toolchain adds it to the
instance's own `sandbox.Dockerfile`, which `build.sh` builds on top of this
image and gives the tag cycles run: see
[the sandbox image](/new-project#5-the-sandbox-image). The platform's file stays
as this checkout has it, because a changed one makes `tehas update` refuse.

Two things the base carries are changed on purpose.

- codebase-memory-mcp is put back at the version `versions.env` names, the one
  `bootstrap.sh` installs on the host. The base installs it from upstream's main,
  unpinned, so its tool schemas moved under a host between builds and a code
  search call that worked in one cycle was refused in the next. The index a cycle
  searches is built by the same program in the same image, by `tehas index`, so
  pinning it fixes both halves at once. What `tehas index` records beside the
  commit is the id of the image that ran, not the pin it was told to use, because
  a host that pulled a new pin has not necessarily rebuilt the image. A run that
  finds a different id there, or none, sets aside what the other image wrote and
  builds from scratch, and puts it back unless this run wrote an index of its
  own, so rebuilding the sandbox image is enough on its own and no indexer ever
  updates an index it did not build.
- wigolo comes off PATH. The base installs it with npm's scripts off, which skips
  the native build of its better-sqlite3, so every call died on a missing
  bindings file and cost a cycle a call each time an agent reached for it.
  pi-rukas's own recipe preflights `command -v wigolo` and says when it is not
  installed, so its absence is an answer and not a gap.

The image also carries the embedding model `vipune` searches with,
`BAAI/bge-small-en-v1.5`. vipune downloads it on its first search, and a cycle
cannot reach the publisher: the download failed and every search after it
answered an empty store. The build fetches the model's two files,
`onnx/model.onnx` and `tokenizer.json`, from huggingface.co at the revision
vipune asks for, and checks each against its sha256. `versions.env` holds the
model (`VIPUNE_MODEL`), the revision (`VIPUNE_MODEL_REVISION`) and the two
checksums. The files go where vipune reads them, `~/.cache/huggingface/hub` of
the user a container runs as. The build then checks that the model works: as
that user, with the network shut off, it adds a memory to a scratch store and
searches it back. It stops when either step errors or the search finds nothing,
and when the base's vipune is not `VIPUNE_VERSION`, because the version picks
the revision. When vipune is bumped, bump the model pins with it: a vipune that
cannot find its model names the revision it wants.

To check an image by hand, with an empty store mounted where a cycle mounts
its own:

```
mem=$(mktemp -d)
docker run --rm --network none --user "$(id -u):$(id -g)" -e HOME=/home/vscode \
  -v "$mem:/home/vscode/.vipune" tehas/pi-rukas:uid$(id -u) \
  sh -c 'vipune add "tehas dispatch reads label events" && vipune search "tehas dispatch" --json'
```

A step near the end of the build checks that every command a cycle's
instructions name, and the commands `libexec/` gives a container (the one after
`--entrypoint`, or the one directly after the image), answers in the image, and
it stops when one is missing. Those commands are `pi`, pi-rukas's headless
runner at `/opt/pi-rukas/bin/pi-rukas-headless`, `git`, and
codebase-memory-mcp: they are how a cycle, a planning turn, `tehas dev` and
`tehas index` start. A base image that moved or dropped one of them would fail
every cycle at container start, which is what this step is for.
`test/sandbox_test.sh` reads those commands out of the `docker run` lines in
`libexec/` and fails when the Dockerfile does not check one of them; a unit
that starts a container some other way is outside what it reads.

```
~/tehas/sandbox/build.sh
```

Each build is also tagged with its date, so
`docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid$(id -u)` rolls back (an
instance with its own `sandbox.Dockerfile` tags back `<date>-product`, the layer
the cycles run: see [Rolling back pi-rukas](#rolling-back-pi-rukas)).
Each image is several gigabytes, with Chromium and Rust in it. After a build
`build.sh` keeps the newest three dated tags, the new one among them (with the
instance's `<date>-product` counted as one of them), and
removes older ones. It keeps an older tag whose image a container, running or
stopped, was made from, or that a `uid` tag names. On a host that builds
often the images then take about three times the size of one, plus those
still in use. `tehasbot clean` is what takes the rest: the dated tags past
`TEHAS_CLEAN_AFTER` hours, keeping the newest, and then the old `sha-` tags of
the base repository, the repository of the image the build pulls from upstream,
each removed by its own name (see [tehasbot
clean](/tools#tehasbot-clean)). The base is upstream's image for the commit
`versions.env` follows, and the puller names it one way only, by that commit;
the bases of the commits before it are what the cleaner takes, and the base of
the commit followed today stays.

A build first asks the base image's runner what it supports,
`pi-rukas-headless --capabilities`. A base that does not speak the contract
versions `tehas work` reads (exit codes 1, result schema 5, stable NDJSON 1),
or whose `work` verb does not take `--timeout`, `--json` and `--restart`, is
not built on: the build exits 1 and names each gap. Whether the answer lists
`PI_ENSEMBLE_ISSUE_IMAGES` for manifest format 1, the manifest tehas writes
beside a cycle's images, is recorded as the label `tehas.rukas.images`. The
Dockerfile asks the base it stands on again and stops when the two disagree. `tehas work`, `tehas plan`, the
dispatcher and `install.sh` read the label; an image built without build.sh
carries `unknown`, which tehas reads as "not said" rather than as an answer.

An image that says its pi-rukas does not read them holds every issue whose
text links an attachment while `TEHAS_WORK_IMAGES=1` stands; other issues
start. A host with that setting that puts back an image from before upstream
merged the reader sets `TEHAS_WORK_IMAGES=0` first: putting it back is the
moment the hold becomes real. `install.sh` reports the answer it read from
the image, and the dispatch log names each issue it holds.

The build takes pi-rukas at the commit `PI_RUKAS_FOLLOW` names, as described
under [pi-rukas comes from upstream](#pi-rukas-comes-from-upstream).
`build.sh --if-moved` builds only when that is not the commit the current
image was built on, and `build.sh --check` only says whether it moved. The
contract test proves the product's gates run inside the image, as the unprivileged user,
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

## pi-rukas comes from upstream

tehas takes pi-rukas from [upstream](https://github.com/trail-openers/pi-rukas).
`PI_RUKAS_REPO` in `versions.env` names the repository and `PI_RUKAS_FOLLOW` the
commit a host follows:

- `release:<line>`, the default: the commit of the newest `vX.Y.Z` tag of that
  line, by version. `release:0` takes the newest `v0.Y.Z`, `release:2` the
  newest `v2.Y.Z`, `release:0.14` the newest `v0.14.Z`. `versions.env` names the
  major version this tehas works with, `release:0` while pi-rukas is before 1.0,
  so a new pi-rukas reaches a host at its next `tehas update`. A new major
  version waits for a tehas release that moves the line.
- `release`: the newest `vX.Y.Z` tag of any line.
  A tag with a suffix, such as `v1.0.0-rc1`, does not count.
  Upstream marks its releases as pre-releases; tehas reads the tags, not that mark.
- `branch:<name>`: the head of that branch, for example `branch:main`.
- `commit:<40 hex>`: that commit, asked of no one, for a host that must keep one
  pi-rukas. A commit that is not 40 lower-case hex digits is refused with 78.

For the sandbox build, any of these may go in the instance's `instance.env`, where
it wins over the line in `versions.env`, and so may `PI_RUKAS_IMAGE_DIGEST` below.
An instance that sets neither follows the line its tehas was shipped with. `lib/config.sh` checks `PI_RUKAS_FOLLOW` when the
settings load and stops the command with 78 naming it. `bootstrap.sh` reads only
`versions.env`.

What follows from it:

- `tehas update` moves the sandbox image to that commit whenever it is not
  the commit the image was built on, also when no file of tehas changed. The
  base is the image upstream publishes for each commit, named by its tag
  `sha-<first 12 characters>`, or by its digest when `PI_RUKAS_IMAGE_DIGEST`
  (`sha256:<64 hex>`) is set: a release then starts from the one image its packer
  saw rather than from whatever that tag points at now. A digest of any other
  shape stops the build with 78 before docker is asked anything. A commit whose
  image upstream has not published yet fails the update with a message that says
  so; the next update tries again. `tehas update --check` says whether the commit
  moved.
- `./bootstrap.sh user` moves the host's clone in `~/tehas/repos/pi-rukas` to
  that commit and runs its installer. That clone is what `tehas dev` and an
  interactive Pi session on the host load. Cycles run the pi-rukas inside the
  sandbox image.

The shell tests check that every argument the Dockerfile reads comes from
`versions.env`, except the ones `sandbox/build.sh` works out: the base, the
commit, the uid and whether the base reads images.

Nothing checks a new commit before a cycle runs on it, beyond the image's
`--capabilities` answer, and the dispatcher does not hold cycles back for one:
see [Limits that hold work back](/operating#limits-that-hold-work-back).

### Rolling back pi-rukas

Cycles run pi-rukas from the sandbox image, so putting the previous image back
is the rollback, and it takes effect at the next cycle's start:

```
docker image ls tehas/pi-rukas            # the builds, tagged by date
docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid$(id -u)
```

An instance with its own `sandbox.Dockerfile` has two tags per build, `<date>`
for the platform image and `<date>-product` for the layer on top of it, and
those are what cycles run. Tag back the `-product` one, or the rollback puts
back an image without the product's toolchain.

Each image carries the pi-rukas commit it was built with, as its label
`tehas.rukas`, and the dispatcher and the improvement loop read the commit
from there. The loop's record of a cycle has the commit and the image's id as
the cycle's container started.

That holds until the next `tehas update`, which builds the followed commit
again. Hold off updates until upstream tags a fix, or set `PI_RUKAS_FOLLOW` in
`instance.env`, where it wins over the release's line: `branch:<name>` for a
branch whose head is the commit you want, or `commit:<sha>` for one commit, with
`PI_RUKAS_IMAGE_DIGEST` beside it when the base image is to be pinned too.

Where the instance sets `TEHAS_IMPROVE_RUKAS_REPO`, the
[improvement loop](/operating#the-improvement-loop) files what it finds about
pi-rukas there, each issue naming the product's issues whose cycles showed it.
No cycle on the product fixes pi-rukas. A fix is made in pi-rukas and reaches
a host with the next `tehas update` after upstream tags a release with it. To check it, apply
`tehas:ready` again to a product issue the filed issue names. The loop's next
run comments there whether the problem came back, with the pi-rukas commit
that cycle ran.

## Services

| Service     | How it runs                                                | Check                                                  |
| ----------- | ---------------------------------------------------------- | ------------------------------------------------------ |
| This site   | `docker compose -f ~/tehas/edge/docs.compose.yaml up -d`   | `curl -i http://tehas-tehas:8080/health`               |
| Dispatcher  | systemd user timer `tehas-dispatch.timer`, every 2 minutes, and `tehas-dispatch.path`, a poll as soon as a cycle, a planning turn or an index run ends | `tail ~/tehas/logs/dispatch.log`                       |
| Planning turn | a unit the poll starts, `tehas-plan-<issue>.service`, one at a time. systemd stops it a quarter of an hour after `TEHAS_PLAN_TIMEOUT` | `tail ~/tehas/logs/dispatch-plan-<issue>.log` |
| Code index | a unit the poll starts, `tehas-index.service`, before cycles start on an idle host. systemd stops it after 40 minutes | `tail ~/tehas/logs/dispatch-index.log` |
| Energy use sampler, optional | systemd user service `tehas-energy.service`, every 15 s. It asks the endpoint the dispatcher settled on, the word in `~/tehas/live/dispatch.json`. Enable it only when the model backend is a [viiwork](https://github.com/janit/viiwork) mesh | `jq .generatedAt ~/tehas/live/energy.json` |
| Host snapshot publisher, optional | systemd user service `tehas-top.service`, every 5 s. It writes nothing unless `TEHAS_TOP=1` | `jq '.host.cpus, .host.load' ~/tehas/live/top.json` |
| Protolab pages | systemd user timer `tehas-protolab.timer`, every 5 minutes | `tail ~/tehas/logs/protolab.log` |
| Improvement loop, optional | systemd user timer `tehas-improve.timer`, every 3 hours. It does nothing unless `TEHAS_IMPROVE_REPO` is set | `tail ~/tehas/logs/improve.log` |
| Cleaner | systemd user timer `tehas-clean.timer`, every hour, the first time 15 minutes after boot. It removes build output, Docker's build cache, host scratch, the sandbox image's old dated tags and the old `sha-` tags of the base repository, each removed by its own name. `install.sh` copies the unit; a person enables it. The unit prints; `libexec/clean` writes `clean.log` | `tail ~/tehas/logs/clean.log`, and `journalctl --user -u tehas-clean` for a run that wrote nothing there |
| Edge router | `docker compose -f ~/tehas/edge/router.compose.yaml up -d` | `curl -i http://tehas-tehas:8081/` |
| Previews | systemd user timer `tehas-preview.timer`, every 5 minutes | `tail ~/tehas/logs/preview.log`, `jq . ~/tehas/live/previews.json` |
| Preview sweep | systemd user timer `tehas-preview-sweep.timer`, every 3 hours | `jq .lastSweep ~/tehas/live/previews.json` |
| One preview | a container `tehas-preview-<pr>` | `docker ps --filter label=tehas.preview` |

The docs server binds to `TEHAS_HOST_ADDR`, port 8080. Without the setting it
stays on `127.0.0.1`. `docker compose` reads the address from the environment,
so run it from a shell that has sourced `~/tehas/env.sh`.

Both compose files name Caddy by digest rather than by tag, with the version in
the comment above the image. `versions.env` records the version and the digest
together; `preview/Dockerfile` names the same digest, and `test/edge_test.sh`
checks the three agree.

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

### Themes and layouts

Every page, the factory floor, the business page, the manual and the Protolab,
takes its look from one theme: its colours, typefaces, sizes, outlines and
shadows, and the colours the canvases paint with. `TEHAS_THEME` in
`instance.env` names it:

| Theme       | Look                                                                    |
| ----------- | ----------------------------------------------------------------------- |
| `pixel`     | The default: the factory as a 16-bit game, in the Topaz bitmap typeface |
| `paper`     | Plain type on a light page, thin rules and light screens                |
| `beesknees` | A business application: near-black and muted green on white, thin rules, no motion. Dark where the viewer's system is |
| `beesknees26` | The 2026 redraw of beesknees, which it will replace: ink and muted green on a near-white ground, white panels with soft shadows, IBM Plex, no motion. Light; the business page has a button for a dark set, kept in the viewer's browser |

`TEHAS_LAYOUT` names what the front page is:

| Layout     | Front page                                                                          |
| ---------- | ----------------------------------------------------------------------------------- |
| `floor`    | The default: the factory floor, the game view                                       |
| `business` | The business page: live power and tokens, the production line from prototypes to shipped, the last 10 days and the issue board |

Both pages are always there, at `/floor` and `/business`, in whatever theme
the instance has; the setting only picks the one at `/`. The business page was
drawn for beesknees, and the floor for pixel. The business page reads the
same files as the floor: `live/status.json`, `live/protolab.json`,
`live/analytics.json` and `live/energy.json`. Its live strip shows only where
the host publishes power, and its 10 days only once there is a report. With no
queue file it shows the floor's sample tickets and says so. Its live charts
cover the last 30 minutes, which is what `energy.json` holds.

A page asks for `/fun/theme.css`, and the docs server and the edge router
answer with `~/tehas/fun/theme-<name>.css`. Both read the settings when their
container is created: run the two `docker compose ... up -d` lines again after
changing either. The sprites and scenes are pictures and stay the same in every
theme.

A theme is one file in `host/fun/`, `theme-<name>.css`, that sets every token
`theme-pixel.css` sets; its name is added to the list in `lib/config.sh`.
`test/theme_test.sh` fails when a theme leaves a token out or a page names a
colour or a typeface of its own.

## Secrets and trust

- `~/.config/tehas/env`: the bot's GitHub token (`GH_TOKEN`) and the model
  endpoints' tokens (`TEHAS_MODEL_KEY_*`, see
  [A backup model endpoint](#a-backup-model-endpoint)), mode 600. The
  energy switches `TEHAS_ENERGY` and `TEHAS_ENERGY_URL` go here too
  (see `tehasbot energy` in [Commands](/tools)); loading the instance keeps
  them.
- `~/.config/tehas/deploy_key`: the read-only deploy key the platform checkout
  is pulled with.

Both are outside the repository and the tehas tree.

| Question                               | Answer                                                                                                                                  |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Who can start a cycle                  | Only an account in `TEHAS_ALLOWED_LABELERS`. The dispatcher checks who most recently applied `tehas:ready`, not just that it is there   |
| What the label approves                | The title and body as they were when it was applied. An issue whose text was changed afterwards by any other account, or by one GitHub no longer names, is skipped until the label is applied again |
| What else starts a cycle               | The retry box of a parked issue, ticked by one of those accounts. It counts once, and only in the question whose comment id the host has on record for that issue. Like the label, the tick approves the text as it was at that moment |
| Who can start or continue planning     | The same accounts. A session starts when one of them applied `tehas:plan`. A turn runs on a tick or a reply from one of them to the question the session is waiting on |
| Who can put a ticket in the Protolab   | The same accounts. `tehasbot protolab` checks who most recently applied `tehas:prototype`, as the dispatcher checks `tehas:ready`. A label from anyone else counts as no label, and a page it had is removed. The pass stops with an error when `TEHAS_ALLOWED_LABELERS` names the bot's own login. The files beside a ticket are taken only from an owner, member or collaborator |
| What a planning container is given     | The same token, the same mounts and the same approve-everything mode as a work cycle's. The agent is asked to plan and file issues. Nothing in the container holds it to that |
| What an agent can do as the bot        | Post comments, edit the bot's comments, edit the description of any pull request and apply labels, with the token. So a comment is the factory's question only if the host has its id on record in `~/tehas/state`, which no cycle or planning container is given, and an answer counts only by who gave it |
| Where an agent's conversation is kept  | In the container's own sessions directory: `~/tehas/work/<name>-<issue>.state/sessions` for a cycle, `~/tehas/dev/<name>.state/sessions` for a manual session. No other container reads or writes it, and a cycle's goes with its workspace |
| Does the token enter the cycle         | Yes, as `GH_TOKEN`. pi-rukas pushes the branch and opens the pull request from inside. Agent code can do anything the token can. So do the model endpoints' tokens that `models.json` names, as `TEHAS_MODEL_KEY_*`: Pi sends them. Agent code can read them and call the endpoints with them |
| What else enters the container         | `PI_ENSEMBLE_*` settings, the bot's git name and email, the host aliases. Not the rest of the host environment                          |
| Host paths mounted                     | The cycle's own workspace, `~/tehas/work/<name>-<issue>`, at the same path in every container; its own transcripts, `~/.pi/agent/ensemble-runs/issue-<issue>`; its own state directory, `~/tehas/work/<name>-<issue>.state` (read-write); its own `models.json` from that directory, `mcp.json`, Pi's `settings.json` (read-only). Only the last two are the same host file in every container; nothing one container writes is mounted for another |
| Not reachable from the container       | The Docker socket, SSH keys, `~/.config/tehas`, the platform checkout, the product checkout, the instance directory                     |
| What the host runs from the workspace  | Nothing but the cleaner's one bounded `git rev-parse` of a worktree's gitdir, which reads no config of the tree and takes nothing from it. Every other git the host runs is in the product checkout, which no container has written. It reads nothing from the workspace: a cycle's outcome is the runner's result, from its standard output |
| The indexing container                 | `tehas index` runs the indexer, not an agent: a clone of the product read-only, and `~/tehas/state/index` to write, which no other container is given; no network, no token            |
| The improvement loop's container       | `tehasbot improve` runs plain Pi, not pi-rukas: the records of the cycles read-only, one directory to write its findings in, and its own `models.json`. No GitHub token, only the model endpoints' tokens; no repository, no tool that runs a command. The host makes every GitHub call and checks the findings against a fixed shape before it files one |
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
calls, the security options of [What a container may not
do](#what-a-container-may-not-do) among them.

### A session by hand

`tehas dev` starts the same container for a person, and the table above holds
for it with these differences:

| Question                    | Answer                                                                                                             |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Who can start one           | The tehas user, and the accounts in the group `tehas`, through one `sudo` rule. See below                           |
| The tree it works in        | `~/tehas/dev/<name>`, kept between sessions, where a cycle gets a fresh clone                                       |
| git identity                | The stored author of that clone, else the bot's. A label: it is not checked against anything                       |
| What else enters            | A terminal, and `TERM` and `COLORTERM` from the shell. Nothing else of the host environment, and not `~/.config/gh` |
| Where its build output goes | `~/tehas/dev/<name>.state/target`, mounted read-write and named by `CARGO_TARGET_DIR`. A session's build output is kept with its clone and the cleaner walks nothing under `<name>.state`; [tehas dev](/tools#tehas-dev) says what that costs and who clears it |
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
git clone https://github.com/tehas-app/tehas.git ~/tehas/repos/tehas
./install.sh
```
