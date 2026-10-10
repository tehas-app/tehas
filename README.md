```
       __________________________.____    _________    ______
       \___     ___/  __    /    |    \ _/    _    \ _/   __/__
__ __/\ _/       \_   \____/__   _     \__   _/     \\______   \_ /\___ ___ _
       \____     /____      /___|      //___\       /      ____/
 - --------l____/----l_____/----l_____/------\_____/_______/-----sQ!-- -- -
 
```

# Tehas

And if you're wondering... tehas is Estonian for factory :)

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

## How it works

1. You label an issue `tehas:ready`. The label counts only when an account on
   the instance's allow list applied it.
2. A dispatcher polls the queue every two minutes and starts one cycle at a
   time.
3. The cycle runs in a container with CPU and memory limits. It gets the bot's
   GitHub token and the product checkout. It does not get the Docker socket or
   any SSH key.
4. pi-rukas explores the code, plans, writes the change, runs the repository's
   own gates and reviews the result.
5. The bot opens a pull request and comments on the issue. A person merges it.

Tehas has no database. Its state is the labels on GitHub and plain files on the
host. A dashboard on port 8080 shows the queue and the console of the running
cycle.

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
| [Operating](host/docs/operating.md) | Writing an issue, the labels, pausing, retrying |
| [A new project](host/docs/new-project.md) | Installing tehas for your repository |
| [Bootstrap a host](host/docs/bootstrap.md) | Preparing a fresh machine |
| [The host](host/docs/host.md) | Files on the machine, settings, the sandbox image |
| [Commands](host/docs/tools.md) | `tehas`, `tehasbot` and what each of their commands does |
| [Troubleshooting](host/docs/troubleshooting.md) | What each kind of stop means |

Agents can start from [`host/docs/llms.txt`](host/docs/llms.txt).

## State of the project

Tehas is experimental. One instance runs it, the one that works on this repository, and
nothing has run for long. Its first unattended cycle took 61 minutes and about
8.5 million tokens for a 172-line change. The
[features page](host/docs/features.md) says for each part whether it was seen
working on a host, covered by tests only, or not built.

Two things it does not do: run a preview of a pull request, and serve more than
one repository from one host.

## Working on tehas

```
scripts/verify.sh          # lint, shellcheck, the shell tests
scripts/verify.sh --full   # plus deno lint
```

The tests need bash, git, jq and shellcheck. They use no network and no sandbox
image. The rules for changes are in [`AGENTS.md`](AGENTS.md).

| Path | What it is |
| --- | --- |
| `bin/` | The two commands: `tehas` for a person, `tehasbot` for the units. Installed to `~/tehas/bin` |
| `libexec/` | The scripts behind them, bash and one Deno script. Installed to `~/tehas/libexec` |
| `lib/config.sh` | Loads and checks the instance settings |
| `lib/sandbox.sh` | What a sandbox container is given, for a cycle and for a session by hand |
| `lib/update-steps.sh` | The follow-up steps of an install, which `install.sh` asks for and `tehas update` runs |
| `host/` | Everything else that is installed: units, the docs server, the sandbox image, the manual |
| `install.sh`, `bootstrap.sh` | Install tehas on a host, and prepare a host's tools |
| `instances/` | `example/` to copy, and `tehas-tehas/`, the settings of the example instance |
| `test/` | Shell tests, one file per unit |
| `scripts/` | The gate and the lint |
| `docs/adr/` | Decisions, with their reasons |

## Licence

Apache License 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE). The pixel
typeface in `host/fun/` is third-party work under its own terms and is credited
in the footer of every page.
