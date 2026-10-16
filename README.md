```
       __________________________.____    _________    ______
       \___     ___/  __    /    |    \ _/    _    \ _/   __/__
__ __/\ _/       \_   \____/__   _     \__   _/     \\______   \_ /\___ ___ _
       \____     /____      /___|      //___\       /      ____/
 - --------l____/----l_____/----l_____/------\_____/_______/-----sQ!-- -- -
 
```

# Tehas

Tehas is a software factory powered by the [Pi coding agent](https://pi.dev).
You write a GitHub issue and label it. Agents on your own model server work on
it in a sandbox, and a pull request comes back for you to review and merge.

Pi does the coding, with the
[pi-rukas](https://github.com/trail-openers/pi-rukas) extension running the
work cycle. Tehas is the part around them: the queue, the sandbox, the
dashboard. [Pi software factory](host/docs/pi-software-factory.md) says who
does what. Tehas is its own project and is not part of Pi.

You can watch one work at [tehas.app](https://tehas.app). That instance works
on this repository.

And if you're wondering... tehas is Estonian for factory :)

## How it works

1. You label an issue `tehas:ready`. The label counts only when an account on
   the instance's allow list applied it.
2. A dispatcher polls the queue every two minutes and starts a cycle for each
   issue there is room for: at most the host's CPUs minus one by default,
   fewer while `tehas.slice` has no room.
3. The cycle runs in a container with CPU and memory limits. It gets the bot's
   GitHub token and the product checkout. It does not get the Docker socket or
   any SSH key.
4. pi-rukas explores the code, plans, writes the change, runs the repository's
   own gates and reviews the result.
5. The bot opens a pull request and comments on the issue. A person merges it.

An instance can also read its own work. With the improvement loop switched
on, a model reads the cycles that ended, every three hours, and files what
could be better as issues for a person to judge.

Tehas has no database. Its state is the labels on GitHub and plain files on the
host. A dashboard on port 8080 shows the queue and the console of the running
cycle.

## Work by hand

The same host takes work by hand. A person logs in with their own account and
types one command:

```
ssh alice@tehas-tehas
tehas dev
```

That is an interactive Pi session with pi-rukas in the cycle's sandbox, on the
factory's models, in a clone that is kept between sessions. Commits carry the
name and email of the person's own git configuration. The push and the pull
request are the bot's. One step as root lets an account in:
`sudo ./bootstrap.sh dev <tehas user> <login>`.
[Operating](host/docs/operating.md) has the details.

## What you need

- A Linux host with Docker. `bootstrap.sh` supports Ubuntu 26.04 on x86_64.
- An OpenAI-compatible model endpoint the host can reach. It has to serve two
  requests at once. [viiwork](https://github.com/janit/viiwork) is one that fits:
  a flexible meshed inference platform for both home and enterprise grade hardware.
  The example instance uses it. Any other OpenAI-compatible endpoint works too.
- A GitHub bot account that can push to the product repository.
- In the product repository: `AGENTS.md`, `.pi/verify-cmd` and
  `.pi/settings.json`. The ones in this repository are a working example.

One install serves one repository.

## Install

The settings of an install are an instance directory. Copy
[`instances/example/`](instances/example/), fill in `instance.env` and
`models.json`, and point the host at it:

```
echo /path/to/your/instance >~/.config/tehas/instance-dir
```

Then, on the host, from a checkout of this repository:

```
./bootstrap.sh check     # are the tools there, at the pinned versions?
./install.sh --check     # report what would change
./install.sh             # copy the files into place
```

`install.sh` checks the settings first and installs nothing if one is missing
or malformed. The bot account, its token, the sandbox image and the first
issue are covered step by step in
[A new project](host/docs/new-project.md).

## Documentation

The manual is in [`host/docs/`](host/docs/). Every instance serves it on port
8080, and the example instance serves it at [tehas.app](https://tehas.app).

| Page | What it covers |
| --- | --- |
| [Overview](host/docs/overview.md) | How an issue moves through the factory |
| [Features](host/docs/features.md) | Every part, and how far each has been tested |
| [Operating](host/docs/operating.md) | Writing an issue, the labels, pausing, retrying, the improvement loop |
| [A new project](host/docs/new-project.md) | Installing tehas for your repository |
| [Bootstrap a host](host/docs/bootstrap.md) | Preparing a fresh machine |
| [The host](host/docs/host.md) | Files on the machine, settings, the sandbox image |
| [Previews](host/docs/edge.md) | A preview of every pull request, and how the names were published |
| [Commands](host/docs/tools.md) | `tehas`, `tehasbot` and what each of their commands does |
| [Troubleshooting](host/docs/troubleshooting.md) | What each kind of stop means |

Agents can start from [`host/docs/llms.txt`](host/docs/llms.txt).

## State of the project

Tehas is experimental. One instance runs it, the one that works on this repository, and
nothing has run for long. Its first unattended cycle took 61 minutes and about
8.5 million tokens for a 172-line change. The
[features page](host/docs/features.md) says for each part whether it was seen
working on a host, covered by tests only, or not built.

A running preview of every pull request has been seen working on the example
instance, but not every part of it yet. One thing tehas does not do: serve
more than one repository from one host.

## Working on tehas

```
scripts/verify.sh          # lint, shellcheck, the shell tests, the console's tests where cargo is
scripts/verify.sh --full   # plus the console's format and lints (rustfmt, clippy) and the front page's tests (node)
```

The tests need bash, git, jq and shellcheck. They use no network and no sandbox
image. The full gate also needs cargo and node, and cargo fetches the console's
crates the first time. The rules for changes are in [`AGENTS.md`](AGENTS.md).

To release and deploy:

```
scripts/tag-release.sh              # tag the head of main as the next patch and push the tag
scripts/tag-deploy.sh tehas-tehas   # run tehas update on that host, check it lands on the tag
```

Both run unattended. `tag-release.sh` runs the gate on the head of main before
it tags, does nothing when the head is already tagged, and prints the tag on
stdout; `--minor`, `--major`, `--message`, `--no-verify` and `--dry-run` are
there too. `tag-deploy.sh` refuses when main has
moved past the newest tag, because `tehas update` installs the head of the
branch, not a tag. `--check` reports without changing the host.

`tag-deploy.sh` logs in with plain `ssh <host>`. Put the user and key for the
host in `~/.ssh/config`, or set the command in `TEHAS_SSH`. A deploy can end
with a `later:` line, such as `bootstrap.sh user` when `host/versions.env`
changed. `tehas update` does not run those steps; a person runs them on the
host.

| Path | What it is |
| --- | --- |
| `bin/` | The two commands: `tehas` for a person, `tehasbot` for the units. Installed to `~/tehas/bin` |
| `libexec/` | The scripts behind them, all bash. Installed to `~/tehas/libexec` |
| `console/` | The console of a running cycle, a Rust program. `install.sh` builds it and installs it as `~/tehas/libexec/console` |
| `lib/config.sh` | Loads and checks the instance settings |
| `lib/attachments.sh` | Files attached to an issue: which links count and how one is fetched |
| `lib/cycles.sh` | Which cycles are running, from the lock each one holds, and where a parked cycle's question is on record |
| `lib/headless.sh` | What tehas reads of pi-rukas's headless runner: its `--capabilities` answer, its NDJSON progress and its `--json` result |
| `lib/edge.sh` | What `tehasbot protolab` and `tehasbot preview` share: numbered sets under `~/tehas` and the previews' network |
| `lib/improve.sh`, `lib/improve-prompt.md` | The improvement loop's shared parts, and the prompt its analyst reads |
| `lib/labels.sh` | Who applied a label, read from the label events |
| `lib/live.sh` | The live view of the cycles, put together from a file per cycle |
| `lib/planning.sh`, `lib/questions.sh` | The steps of a planning session, and the questions the factory asks on an issue |
| `lib/sandbox.sh` | What a sandbox container is given, for a cycle and for a session by hand |
| `lib/slice.sh` | Reads `tehas.slice`: whether it is usable, the limits of a container, the room left |
| `lib/status.sh`, `lib/top.sh` | The factory's state for `tehas status`, and the frame of `tehas top` |
| `lib/update-steps.sh` | The follow-up steps of an install, which `install.sh` asks for and `tehas update` runs |
| `host/` | Everything else that is installed: units, the docs server, the sandbox image, the manual |
| `preview/` | What `tehasbot preview` builds for a pull request of this repository |
| `install.sh`, `bootstrap.sh` | Install tehas on a host, and prepare a host's tools |
| `instances/` | `example/` to copy, and `tehas-tehas/`, the settings of the example instance |
| `test/` | Shell tests, one file per unit, and the front page's node tests |
| `scripts/` | The gate, the lint, the test runner, the pi-rukas pin, and the release and deploy scripts |
| `docs/adr/` | Decisions, with their reasons |

## Licence

Apache License 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE). The pixel
typeface in `host/fun/` is third-party work under its own terms and is credited
in the footer of every page.
