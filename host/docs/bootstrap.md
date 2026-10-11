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
sudo ./bootstrap.sh root boss     # 1. as root, naming the tehas user
# log out and in again, so the docker group applies
./bootstrap.sh user               # 2. as the tehas user
./bootstrap.sh check              # 3. one line per tool
sudo ./bootstrap.sh dev boss alice   # optional: let the account alice type "tehas dev"
sudo ./bootstrap.sh pool 14g         # optional: cycles share TEHAS_WORK_MEMORY, see Limits and concurrency in the host page
```

If the platform repository is private, the clone needs the deploy key from the
credentials section first; create that key by hand and come back.

### 1. `root <user>`

Installs from Ubuntu's packages: Docker with compose and buildx, git, jq, curl,
openssl, rsync, shellcheck, unzip and the build tools that compiling two of the
tools needs. Adds the user to the `docker` group, enables lingering for it (so
the dispatcher timer runs without a login) and enables the Docker service.

System packages are not pinned. You get what the distribution offers.

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

Installs under the user's home, each tool in its own place:

| Tool                              | Where                                  | From                          |
| --------------------------------- | -------------------------------------- | ----------------------------- |
| Node and npm                      | `~/tehas/tools/node`                   | Release archive, checksum     |
| `gh`                              | `~/tehas/bin/gh`                       | Release archive, checksum     |
| Bun                               | `~/.bun/bin`                           | Release archive, checksum     |
| Pi, `ctx7`, `parallel-cli`        | `~/.local` (npm prefix)                | npm, exact versions           |
| Rust, then `vipune` and `oo`      | `~/.rustup`, `~/.cargo`                | rustup, then `cargo install`  |
| `codebase-memory-mcp`             | `~/.local/bin`                         | Its installer, pinned release |
| pi-rukas                          | `~/tehas/repos/pi-rukas`, one commit   | Its repository, then its `install.sh` |

Every release archive is checked against a SHA-256 in `versions.env` before it
is unpacked. The last step runs pi-rukas's own installer, which links its
extension and skills into `~/.pi/agent/` and pulls its own `latest` image. It is
run without `PI_ENSEMBLE_IMAGE`, so it never touches the tehas sandbox image.

pi-rukas comes from the repository named in `versions.env`, which is a fork of
upstream. A clone already on the host is pointed at that repository before it
is fetched. [The host](/host#pi-rukas-comes-from-a-fork) says how a fix
moves through the fork.

Compiling `vipune` and `oo` takes a few minutes. The rest is quick. Rust is
also what `install.sh` builds the cycle console with.

Deno is no longer installed. A host set up before that still has `~/.deno`;
nothing uses it and `bootstrap.sh` does not remove it.

### 3. `check`

Changes nothing. Prints each tool with the version found and the one expected,
then the system tools, Docker without sudo, and lingering. Exit 1 if anything
is missing or at another version. `./install.sh --check` runs it as its
`Dependencies` line.

## Running it again

It is safe to run again. A tool already at its version is skipped. A tool it
owns that is at another version is replaced with the pinned one. To change a
version, edit `host/versions.env` (the version and, for an archive, its
checksum), run `./bootstrap.sh user`, rebuild the sandbox image and rerun the
contract test before a cycle uses it.

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
  `binary not found`. The script installs that one package with scripts on. The
  tool is a hosted search service and is not logged in.

## Credentials

Not created by the script. All of them go in `~/.config/tehas/`, mode 600,
never in git.

**The bot token.** A token of a GitHub machine account with write access to the
product repository, in `~/.config/tehas/env` as `GH_TOKEN=...`. What it is used
for: reading the queue and label events, relabelling and commenting, pushing
the cycle's branch, opening the pull request.

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
