---
title: Bootstrap a host
description: "Preparing a fresh machine to run a tehas factory with bootstrap.sh."
---

# Bootstrap a host

`bootstrap.sh` prepares a fresh machine with everything tehas and pi-rukas
need, at the versions in `host/versions.env`. It is in the top of the platform
repository, next to `install.sh`. It installs tools only: credentials, the
checkouts and tehas itself come after it, on the [new project page](/new-project).

## What the machine needs

- Ubuntu 26.04 on x86_64. The script stops on anything else.
- One Unix user for tehas, and root once, for the first step.
- A private network to reach the dashboard on, and outbound HTTPS to GitHub,
  the package and release hosts.
- An OpenAI-compatible model endpoint it can reach.
  [viiwork](https://github.com/janit/viiwork) is one, and the one the example
  instance uses.
- Disk for the sandbox image, about 10 GB once built, plus the product's own
  needs. The example host has 8 CPUs, 15 GB of memory and 58 GB of disk.

## Run it

```
git clone <the platform repository> ~/tehas/platform
cd ~/tehas/platform
sudo ./bootstrap.sh root boss     # 1. as root, naming the tehas user; runs step 2 as boss too
./bootstrap.sh check              # 3. as the tehas user: one line per tool, and one for the slice
sudo ./bootstrap.sh slice         # only if step 1 could not install the slice, or it changed
sudo ./bootstrap.sh dev boss alice   # optional: let the account alice type "tehas dev"
```

If the platform repository is private, the clone needs the deploy key from the
credentials section first; create that key by hand and come back.

### 1. `root <user>`

Makes the user when it is missing (`useradd -m`, bash as its shell). Installs
from Ubuntu's packages: Docker with compose and buildx, git, jq, curl,
openssl, rsync, minisign (`tehas update` checks release signatures with it),
shellcheck, unzip and the build tools that compiling two of the tools needs. Adds the user to the `docker` group, enables lingering for it (so
the dispatcher timer runs without a login) and enables the Docker service.

System packages are not pinned. You get what the distribution offers.

When Docker runs on cgroup v2 with the cgroupfs driver, it adds
`"exec-opts": ["native.cgroupdriver=systemd"]` to `/etc/docker/daemon.json`,
keeping every other setting and any other `exec-opts`, saves the file before as
`daemon.json.prev` and restarts Docker. A `daemon.json` that is not JSON is
left as it is, and `root` stops with exit 1 and says so.

Then the `slice` step below. On cgroup v1 that step writes nothing, `root`
still finishes, and it says to run `sudo ./bootstrap.sh slice` once Docker uses
the systemd driver.

Last, it runs step 2, `user`, as the tehas user, with `runuser -l`. That is a
new login session, which reads the groups again, so it has the `docker` group
and there is no logout in the middle. A shell of that user that was open before
gets the group at its next login. The user must be able to read the directory
`bootstrap.sh` is in: a checkout or a release unpacked under the user's home
is. `--root-only` stops before step 2, for running it by hand later.

### `slice`

```
sudo ./bootstrap.sh slice
```

Installs one systemd unit, `/etc/systemd/system/tehas.slice`, from
`host/slice/tehas.slice` in the checkout, reloads systemd and starts the slice.
It takes no arguments and reads nothing from the instance, the user's home or
the network. The file is the same on every host:

```
[Slice]
MemoryMax=75%
CPUWeight=100
```

The slice is where every sandbox container runs, under one memory limit of
three quarters of the host's memory. Without it no container starts and the
dispatcher holds. [The host](/host#the-slice) says what it is for.

Root is needed for exactly this: writing one system unit and reloading the
system manager. Docker places containers under the system manager, so a slice
of the tehas user cannot hold them. The step adds no sudo rule, no package and
no Docker setting.

What it does, in order:

1. It stops with exit 64 if it is not root or was given an argument.
2. It asks Docker for its cgroup driver and version. Anything but the systemd
   driver on cgroup v2 is refused and nothing is written. The fix is
   `"exec-opts": ["native.cgroupdriver=systemd"]` in `/etc/docker/daemon.json`
   and a restart of Docker.
3. If the file under `/etc` equals the shipped one, systemd has the slice
   loaded from a file with a memory limit, and systemd does not report the
   file as changed since it read it (`NeedDaemonReload=no`), it prints
   `bootstrap: slice part already in place` and changes nothing. A reload
   that failed after an update leaves the old limit in force beside an equal
   file; that is not in place, and the step goes on to reload.
4. Otherwise it writes the file, runs `systemctl daemon-reload` and
   `systemctl start tehas.slice`, and stops with exit 1 if the slice has no
   memory limit after that.
5. It prints one line with the file systemd loaded, the limit in bytes, and
   whether `systemd-oomd` is active. That last part is information: tehas
   neither uses nor needs systemd-oomd, and the step does not touch it.

Run it again when an update of tehas changes the shipped file.
`./install.sh --check` says when: its `Slice:` line reads `DIFFERS`.
`tehas update` cannot do it, because it runs as the tehas user.

Another limit for one host goes in
`/etc/systemd/system/tehas.slice.d/override.conf`, written by root. The step
never writes or removes that file.

### Optional: `dev <user> [<login>...]`

Lets other accounts on the host start a session with `tehas dev`, each under
its own name. Run it after tehas is installed, and again to add an account or
after an update of the platform.

It makes the group `tehas` and puts the named accounts in it, writes one
`sudo` rule for that group, and installs `/usr/local/bin/tehas` for everyone.
The rule is checked with `visudo` before it is installed, and nothing is
changed if an account does not exist. It does not put anyone in the `docker`
group. What a member can and cannot do is on [The host](/host).

### 2. `user`

`root` runs this for you. Run it by hand as the tehas user after
`root --root-only`, or again later. Installs under the user's home, each tool in its own place:

| Tool                              | Where                                  | From                          |
| --------------------------------- | -------------------------------------- | ----------------------------- |
| Node and npm                      | `~/tehas/tools/node`                   | Release archive, checksum     |
| `gh`                              | `~/tehas/bin/gh`                       | Release archive, checksum     |
| Bun                               | `~/.bun/bin`                           | Release archive, checksum     |
| Pi, `ctx7`, `parallel-cli`        | `~/tehas/tools/npm/<name>`, commands linked into `~/.local/bin` | npm tree, pinned by lockfile  |
| Rust, then `vipune` and `oo`      | `~/.rustup`, `~/.cargo`                | rustup, then `cargo install`  |
| `codebase-memory-mcp`             | `~/.local/bin`                         | Its installer, pinned release |
| pi-rukas                          | `~/tehas/repos/pi-rukas`, its branch's head | Its repository, then its `install.sh` |

Every release archive is checked against a SHA-256 in `versions.env` before it
is unpacked. The directories it makes under the home are `bin/`, `tools/`,
`repos/` and `tmp/`, and `install.sh` counts them, beside the ones it makes
itself, as a home tehas owns: a home prepared by `bootstrap.sh user` and not yet
installed is accepted rather than refused. `tools/` is one of them for the same
reason `bin/` is: `env.sh` puts `tools/node/bin` first on the `PATH`. The three npm tools are pinned the same way, by a tree beside
`versions.env` rather than by a download: `host/npm/<name>/package.json` names
one version and `host/npm/<name>/package-lock.json` fixes every package in that
tree by hash. `bootstrap.sh` copies the two files to `~/tehas/tools/npm/<name>`
and installs them with `npm ci`, which refuses a package whose hash does not
match the lockfile, so a tool's dependencies are as fixed as the tool. Before it
installs anything it checks each tree against `versions.env`: the file's
SHA-256, and the version its `package.json` and its lockfile name. It stops
with nothing installed when they do not agree. A new tree is built beside the
installed one and moved into place only once `npm ci` succeeded, so an install
that fails leaves the tree that worked and the commands linked into it.
`parallel-cli` is pinned this way only as far as npm goes: its own install script downloads the binary it runs,
and the lockfile says nothing about that download.

For `user`, a pinned tool counts as present only when its tree is installed
too: the lockfile under `~/tehas/tools/npm/<name>` has the SHA-256 that
`versions.env` records. A host that already had one of these tools at the same
version, from a bootstrap that installed it into npm's global tree, therefore
reinstalls it from `host/npm` on the next `./bootstrap.sh user` and drops the
global copy. `check` reports such a tool as `DIFFERS`, with "not the pinned
tree" after the version it finds on the `PATH`, since `user` would replace it.

The last step runs pi-rukas's own installer, which links its
extension and skills into `~/.pi/agent/` and pulls its own `latest` image. It is
run without `PI_ENSEMBLE_IMAGE`, so it never touches the tehas sandbox image.

pi-rukas comes from the repository named in `versions.env`, at the head of the
branch named there. It is not pinned: each run moves the clone to the head,
and `check` reports a clone behind it as `DIFFERS`. A clone already on the
host is pointed at that repository before it is fetched.
[The host](/host#pi-rukas-comes-from-upstream) says how the sandbox image
follows the same branch.

Compiling `vipune` and `oo` takes a few minutes. The rest is quick. Rust is
also what `install.sh` builds the cycle console with.

Deno is no longer installed. A host set up before that still has `~/.deno`;
nothing uses it and `bootstrap.sh` does not remove it.

### 3. `check`

Changes nothing. Prints each tool with the version found and the one expected,
then the system tools, Docker's compose and buildx, Docker without sudo, and
lingering. Exit 1 if anything is missing, at another version, or an npm tool
not on its pinned tree. `./install.sh --check` runs it as its `Dependencies`
line.

The last line is the slice: `tehas.slice: in place`, `missing` or `differs`,
by comparing the file under `/etc/systemd/system` with the one in the checkout.
It does not change the exit status.

## Running it again

It is safe to run again. A tool already at its version is skipped. A tool it
owns that is at another version is replaced with the pinned one. An npm tool at
its version is skipped only once its pinned tree is in place. To change a
version, edit `host/versions.env` (the version and, for an archive, its
checksum; for an npm tool, the SHA-256 of its lockfile, regenerated with
`npm install --package-lock-only --ignore-scripts` in that directory under
`host/npm`), run `./bootstrap.sh user`, rebuild the sandbox image and rerun the
contract test before a cycle uses it.

An update that brings a changed `versions.env` to a host prints
`later: .../bootstrap.sh user`. Neither `install.sh` nor `tehas update` runs
it. Run `./bootstrap.sh user` yourself once the update has finished and no
cycle is running. [The host](/host#configuration-comes-from-git) has the
details.

A tool of the same name installed elsewhere by someone else is left alone. The
pinned one comes first on the `PATH` that `env.sh` sets.

## Two traps the script handles

Both were found installing the example host by hand. The script deals with
them; they are here so the error is recognisable if it comes back.

- **`codebase-memory-mcp` refuses a group-writable directory.** Ubuntu's default
  umask leaves `~/.local` and what npm creates in it group-writable, and the
  installer stops with `install_dir_group_or_world_writable`. The script removes
  group write from `~/.local`, `~/.local/bin` and `~/.local/lib` first.
- **`parallel-cli` needs its download script.** Installed with npm's scripts
  switched off, the package arrives without its binary and answers
  `binary not found`. The script installs that one tree with scripts on. The
  tree is pinned by its lockfile; the binary the script fetches is not, so that
  download is the one part of this tool the pin does not cover. The tool is a
  hosted search service and is not logged in.

## Credentials

Not created by the script. All of them go in `~/.config/tehas/`, mode 600,
never in git.

**The bot token.** A token of a GitHub machine account with write access to the
product repository, in `~/.config/tehas/env` as `GH_TOKEN=...`. What it is used
for: reading the queue and label events, relabelling and commenting, editing
the description of a pull request to say where its preview is, pushing the
cycle's branch, opening the pull request.

- Test it before placing it: `GH_TOKEN=... gh api repos/<owner>/<name>` must
  answer with `push: true`.
- A fine-grained token only reaches repositories owned by the account or
  organisation it names as resource owner. A bot that is a collaborator on a
  repository in another person's account gets 404 with a fine-grained token,
  however the collaborator access is set. There the choices are a classic token
  with the `repo` scope, or moving the repository into an organisation the bot
  belongs to, where a fine-grained token can be limited to the one repository
  with Contents, Issues and Pull requests read and write.
- The token enters the cycle's container. Use one bot account per instance so
  that it reaches one product only.
- Note its expiry. Cycles stop at the push check on that date.

**The deploy key.** For a private platform repository, a read-only deploy key:

```
ssh-keygen -q -t ed25519 -N "" -f ~/.config/tehas/deploy_key
git -C ~/tehas/platform config core.sshCommand \
  "ssh -i ~/.config/tehas/deploy_key -o IdentitiesOnly=yes"
```

Add `deploy_key.pub` to the repository's deploy keys without write access.

**The bot's git identity.** `git config --global user.name` and `user.email`
for the tehas user. `tehas work` refuses to start a cycle without them.

## What is proven

`bootstrap.sh` was written from a hand install of the example host. On that
host `check` passes and a second `user` run changes nothing. A run from scratch on a
fresh machine has not been done. Expect to fix something the first time.

The `slice` step is tested, not yet seen on a host.
