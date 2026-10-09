# Tehas

A software factory powered by the [Pi coding agent](https://pi.dev) and
[pi-rukas](https://github.com/trail-openers/pi-rukas): GitHub issues go in,
reviewed pull requests come out, on self-hosted models. "Tehas" is Estonian for
factory. A running one is at [tehas.app](https://tehas.app), and
[Pi software factory](host/docs/pi-software-factory.md) explains what it is
made of.

tehas is a configurable, single-project automation appliance: configure an
instance, install it, run it. An instance is one host serving one product
repository. What it does is issue to pull request, and that needs settings
only. Running a preview of each pull request, with its own database copy, is
not part of this version: no shared interface has been designed for it yet.

**For agents:** a running instance serves `/llms.txt` on its dashboard: what
tehas is, how to get work built and how to set it up for another repository.
Without an instance to ask, read [`host/docs/llms.txt`](host/docs/llms.txt) and
then [`host/docs/new-project.md`](host/docs/new-project.md).

What stays in the product repository is the contract tehas reads there:
`AGENTS.md`, `.pi/` (the verify, smoke and worktree hooks) and, optionally,
`.agents/skills/`. This repository is a product too, worked on by its own
instance, so its `AGENTS.md` and `.pi/` are a working example.

**The operating manual is `host/docs/`, served by each instance on port 8080.**
Start there for how to submit work, pause, retry and troubleshoot. This file
only maps the repository.

- Feature set and how far each part is proven:
  [`host/docs/features.md`](host/docs/features.md)
- Preparing a host: [`host/docs/bootstrap.md`](host/docs/bootstrap.md)
- Installing for another project: [`host/docs/new-project.md`](host/docs/new-project.md)
- Day-to-day use: [`host/docs/operating.md`](host/docs/operating.md)

The examples in the manual come from the instance that works on tehas itself:
host `tehas-tehas`, Unix user `boss`, product `janit/tehas`, everything under
`~/tehas`, settings in [`instances/tehas-tehas/`](instances/tehas-tehas/). Its
dashboard is public at <https://tehas.app/>.

Files here are installed on a host; edit them here and install, not the other
way round.

## Layout

The commands are in `bin/`, the settings library in `lib/`. Everything else
that is installed lives under `host/`, including the operating manual in
`host/docs/`. `instances/` holds instance directories. The top-level `docs/` is
documentation about this project itself: its decisions. `install.sh` and `bootstrap.sh` stay at the top.

| Path here                                               | Installed as                              | Purpose                                                                    |
| ------------------------------------------------------- | ----------------------------------------- | -------------------------------------------------------------------------- |
| `lib/config.sh`                                         | `~/tehas/lib/config.sh`                   | Finds the instance, loads and checks its settings; sourced by every command |
| `host/env.sh`                                           | `~/tehas/env.sh`                          | Environment for every tehas process: paths, the token, then the settings   |
| `bin/tehas-dispatch`, `host/systemd/tehas-dispatch.*`   | `~/tehas/bin/`, `~/.config/systemd/user/` | Queue poller, every two minutes                                            |
| `bin/tehas-work`                                        | `~/tehas/bin/tehas-work`                  | One headless pi-rukas `/work` cycle                                        |
| `bin/tehas-status`, `bin/tehas-console.ts`              | `~/tehas/bin/`                            | Dashboard data and live console                                            |
| `bin/tehas-health`                                      | `~/tehas/bin/tehas-health`                | The health file behind `/health` and `/healthz` on port 8080               |
| `bin/tehas-energy`, `host/systemd/tehas-energy.service` | `~/tehas/bin/`, `~/.config/systemd/user/` | Optional: the energy file behind the Energy use section, viiwork backends only |
| `bin/tehas-protolab`, `host/systemd/tehas-protolab.*`, `host/edge/protolab.*` | `~/tehas/bin/`, `~/.config/systemd/user/`, `~/tehas/edge/` | The Protolab on port 8081: a page and the attached files for every prototype ticket |
| `bin/tehas-test`                                        | `~/tehas/bin/tehas-test`                  | The checks of the platform itself                                          |
| `host/sandbox/Dockerfile`, `host/sandbox/build.sh`      | `~/tehas/sandbox/`                        | The sandbox image agents run in                                            |
| `host/versions.env`                                     | `~/tehas/versions.env`                    | Pinned versions of every tool, for `bootstrap.sh` and the sandbox image    |
| `host/docs/`, `host/edge/docs.Caddyfile`, `docs.compose.yaml` | `~/tehas/docs/`, `~/tehas/edge/`    | The operating manual and plain dashboard (`/dashboard`) on port 8080       |
| `host/fun/`                                             | `~/tehas/fun/`                            | The front page on port 8080, a game view: live queue and cycle console     |
| `host/pi/mcp.json`, `host/pi/settings.json`             | `~/.pi/agent/`                            | Pi user configuration that is the same everywhere                          |
| `instances/<name>/models.json`, `settings.json`         | `~/.pi/agent/`                            | The instance's model provider and models; optionally its own Pi settings   |
| `instances/<name>/instance.env`                         | read in place                             | The instance's settings                                                    |

Not installed:

| Path                         | Purpose                                                                        |
| ---------------------------- | ------------------------------------------------------------------------------ |
| `install.sh`                 | Installs the above for the host's instance; `--check` reports without changing |
| `bootstrap.sh`               | Prepares a fresh host: every tool at the version in `host/versions.env`        |
| `instances/example/`         | A commented instance to copy                                                   |
| `instances/tehas-tehas/`     | The worked example                                                             |
| `test/`                      | Shell tests, one file per unit, run by `bin/tehas-test`                        |
| `scripts/check-generic.sh`   | Lint: platform files must not name a product or a host                         |
| `scripts/verify.sh`          | The gate entry point this repository's own `.pi/` names                        |
| `LICENSE`, `NOTICE`          | Apache License 2.0 and the attribution notice                                  |

## Instance settings

Everything that differs between instances is in an instance directory:
`instance.env` (shell `KEY=value` lines, no secrets) and `models.json`. The host
finds it through `TEHAS_INSTANCE_DIR`, then the one-line file
`~/.config/tehas/instance-dir`, then `~/.config/tehas` itself.

`instance.env` beats the environment. A missing or malformed setting stops a
command with exit 78 and one line naming it, before anything is touched.
[`instances/example/instance.env`](instances/example/instance.env) lists every
setting with a comment.

## Install

On the host, as the tehas user, from a checkout of this repository:

```
./bootstrap.sh check     # are the tools there, at the pinned versions?
./install.sh --check     # change nothing; Instance, Dependencies, Installed, Tests
./install.sh             # copy changed files into place, keep <name>.prev
```

`install.sh` validates the instance first and installs nothing if it is
invalid. It copies files and prints the follow-up step when the sandbox image,
the docs server or the systemd units changed.

Not in git, by design: `~/.config/tehas/env` (the bot's GitHub token) and, if
the platform checkout is private, the deploy key it is pulled with.

A fresh host is prepared by `bootstrap.sh`; see
[`host/docs/bootstrap.md`](host/docs/bootstrap.md).

## Checks

```
scripts/verify.sh          # the lint, shellcheck, the shell tests
scripts/verify.sh --full   # plus deno lint over the TypeScript
```

The shell tests need bash, git, jq and shellcheck only: no sandbox image, no
product checkout and no network.

## Why not pi-durable?

tehas drives a complete agent harness (`pi --mode rpc`) in a sandbox image and keeps its state on the host (labels + atomic files) so the dispatcher and dashboard can read it without a live agent. `@earendil-works/pi-durable` — the durable agent harness in the pi monorepo — would solve the crash-recovery gap (a cycle that dies mid-run leaves a stale `running` state that needs a manual restart), but adopting it means **replacing the inner agent layer** rather than adding a dependency, and its storage lives inside the sandbox process, which conflicts with host-side observable state. It is also experimental (API changes without notice), which does not compose with this repo's pin-by-digest standard.

**Decision: deferred, not rejected.** Revisit if pi-rukas ships on pi-durable, if cycles routinely outgrow the 6-hour work timeout, or if the pluggable issue-backend work needs a backend-agnostic durable store. See [docs/adr/0001-pi-durable.md](docs/adr/0001-pi-durable.md).

## Sandbox image

`tehas/pi-rukas:uid<uid>` is the upstream pi-rukas image plus:

- the in-container user remapped to the tehas user's id,
- deno and shellcheck (the upstream image has neither, so gates that use them
  could not run),
- Playwright's Chromium with system libraries and fonts.

The base is pinned by digest in the Dockerfile and the tools by version in
`host/versions.env`. A product whose gates need another toolchain adds it to
the Dockerfile.

```
# on the host, as the tehas user
~/tehas/sandbox/build.sh
```

`build.sh` tags the result with the build date as well, so an older image stays
available: `docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid$(id -u)` rolls
back.

To upgrade pi-rukas: pull the new upstream image, put its digest in the `FROM`
line, rebuild, then rerun the contract test below before any cycle uses it.

## Contract test

Proves the product's gates run inside the sandbox image, as the unprivileged
user, with a CPU and memory limit and nothing mounted but a clone. With the
example product:

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

## Headless work cycle

pi-rukas has no headless entry and its wrapper needs a terminal. Pi's RPC mode
does not, and it resolves extension commands, so `tehas-work` starts
`pi --mode rpc --approve` in the sandbox image, sends `/work <issue>` as one RPC
prompt and watches `.pi/work-state/<issue>.json` until the cycle leaves
`running`.

```
tehas-work 123            # exit 0: merged or parked for a human, 1: aborted, 2: died or timed out
tehas-work 123 --restart
```

The RPC stream is kept in `~/tehas/logs/work-<issue>-<time>.jsonl`. The
container gets the bot token and the `PI_ENSEMBLE_*` settings only, a CPU and
memory limit from the instance settings, and no Docker socket or SSH keys.
`test/work_test.sh` pins exactly what it is given.

`--approve` is required: without it Pi skips the project's `.pi/settings.json`
in a non-interactive mode. Child agents do not inherit the project's default
model; each role's model comes from `PI_ENSEMBLE_SUBAGENT_MODEL` and
`PI_ENSEMBLE_SUBAGENT_PROVIDER` in the instance's `instance.env`, or per role
from `PI_ENSEMBLE_MODEL_<ROLE>` and `PI_ENSEMBLE_PROVIDER_<ROLE>`. With none
set, a child falls back to the first model in `models.json`.

## Licence

Apache License 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE). The pixel
typeface in `host/fun/` is third-party work under its own terms, credited in
the footer of every page, and is not covered by that licence.
