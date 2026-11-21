# Spec: the product's needs, what a project asks of a factory's host

**Status**: Draft 2026-11-21, with the owner's decisions the same day (see
"Decided"). Nothing is built.

## Purpose

Today a project learns what it needs from a factory by installing one: the
sandbox image fails on a missing tool, a gate runs out of memory, an install
cannot reach its registry. `tehas init-product` guesses the gate from the
stack, and `tehas check-product --sandbox` proves it, but only on a host that
is already there.

This spec moves that knowledge in front of the install. A product says once,
in its own repository, what it needs: its toolchain, the services its gates
use, the room a cycle takes, the hosts it reaches, the kind of preview it has.
Then:

1. A developer's agent can write the file from the repository alone, on the
   developer's laptop, before any host exists. The agent is the wizard.
2. The person knows what machine to get before they get it.
3. tehas builds the sandbox image and checks the host from the file, and
   says exactly which need it cannot meet, before a cycle runs.
4. tehas-industry reads the same file to place a factory and size its
   host.

## The guarantees

1. **Optional.** A product without the file works as today: the lockfiles,
   `.pi/install-cmd` and `sandbox.Dockerfile` stand.
2. **No second copy.** The gates stay in `.pi/verify-cmd` and its siblings,
   the preview in `preview/Dockerfile`. The file points at them and never
   repeats a command.
3. **Facts, not secrets.** The file names secrets a gate needs by name only.
   A value never goes in it.
4. **Checked, never guessed at run time.** An unknown field or a malformed
   value stops the command with 78. A need the host cannot meet is a `FAIL`
   line naming the need, not a cycle that parks later.
5. **Written by people or their agents, proposed by tehas.** tehas may
   propose the file in a pull request; it never commits it to the default
   branch.

## The file

`.tehas/needs.toml` at the root of the product repository, on the default
branch. TOML, as tehas-industry's settings are.

```toml
needs = 1                       # the format's version

[platform]
os = "linux"
arch = ["x86_64"]               # what the gates and the build run on

[toolchain]                     # what the sandbox image must have
node = "22"
pnpm = "10"
rust = "1.91"
apt = ["libssl-dev", "pkg-config"]

[[service]]                     # what the gates reach beside the code
name = "postgres"
version = "18"
for = ["verify-full", "smoke"]  # which gates use it

[room]                          # one cycle's share of the host
memory = "6g"
disk = "20g"
gate-minutes = 20

[network]                       # hosts the install and the gates reach
egress = ["registry.npmjs.org", "static.rust-lang.org"]

[preview]
kind = "web"                    # web, mobile, data, cli, none
dockerfile = "preview/Dockerfile"

[secrets]                       # names only, set on the host by a person
names = ["STRIPE_TEST_KEY"]
for = ["verify-full"]
```

| Table | Read by | Today's equivalent |
| --- | --- | --- |
| `platform` | `tehas check-product`, industry placing a factory | `bootstrap.sh`'s check, stated nowhere in the product |
| `toolchain` | the sandbox build: a generated layer above the platform's image | `sandbox.Dockerfile` by hand |
| `service` | `check-product --sandbox`; later, a service container per cycle | Not supported ("a private database copy per cycle" is not there yet) |
| `room` | the dispatcher's limits, `TEHAS_VERIFY_TIMEOUT`, industry sizing a host | The instance's settings |
| `network` | the sandbox's egress list | The instance's settings |
| `preview` | the edge, the Protolab | `preview/Dockerfile` alone |
| `secrets` | setup asking for them by name | By hand |

A service, a toolchain or a preview kind tehas does not support yet is not
an error in the file: `tehas check-product` says `FAIL service postgres (not
supported on this tehas)`, so the file can describe the product as it is,
ahead of the factory.

## Commands

```
tehas scope [--json] [<path>]    # read a checkout, print the needs it finds
tehas scope --write              # write .tehas/needs.toml from what it found
tehas check-product              # gains a `needs` line per table
```

`tehas scope` runs anywhere tehas's release unpacks, a laptop included, and
needs no settings, no host and no network. It reads the lockfiles, the
manifests (`package.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`,
`Gemfile`, `composer.json`), `.tool-versions` and `mise.toml`, `.nvmrc`,
`rust-toolchain.toml`, `docker-compose*.yml` and `.devcontainer/` for
services, and the `run:` steps and `services:` of `.github/workflows`. Its
JSON says for each field what it found, where it found it, and how sure the
rule is (`found` from a pinned file, `guessed` from a workflow or a
convention, `unknown`). An `unknown` field is a question for a person.

An agent that has no tehas can do the same from this spec and the JSON
Schema at `api/needs.schema.json`; `tehas scope --check <file>` validates
what it wrote.

## How it fits

- **`tehas init-product`** proposes `.tehas/needs.toml` with the other
  contract files, in the same pull request.
- **`tehas setup`** reads it, if it is there, to build the image, set the
  room and the egress list, and ask for the named secrets.
- **tehas-industry** reads it before it creates a factory, to pick a host
  with room.
- **Larger setups**: on the industry's `kubernetes` backend (specified in
  tehas-industry, not built), `room` is what a factory's pod asks for, so
  the cluster can place it and its autoscaler can add a node when the
  queue needs one.
- **Other code hosts**: nothing here names GitHub. A code host or ticket
  tracker is a factory's setting, not a product's need.

## Later: a web wizard

`tehas setup` asks its questions in a terminal. A person who would rather
use a browser could run `tehas setup --web`, which serves a local page,
built on the kit, that walks through the same steps: the machine check, the
endpoint, the GitHub App's link, the labelers, the needs this spec
describes and the onboarding pull request. The page and `tehas setup
--json` call the same steps, so a person in the browser and an agent on the
command line finish the same install, and either can pick up where the
other stopped. `tehas setup --new-app` already writes a local page for the
app, `new-app.html`. A simple plugin builder could sit on the same page
later: pick what the plugin adds, see the grants it will ask for, get a
repository with its manifest (see `2026-11-21-plugins.md`).

## Decided

1. **Name and place**: `.tehas/needs.toml`, a file of its own. `.tehas/`
   is already where tools beside the factory keep a product's files, and a
   section of `AGENTS.md` or `.pi/` would mix what people write for agents
   with what tehas parses.
2. **Services are declared now and refused** until tehas can run one, so
   the file describes the product as it is and `tehas check-product` says
   which need is not met.
3. **`room` is the product's ask; the instance caps it.** A product that
   asks for more than the cap is a `FAIL room` line naming both numbers.
4. **Placement reads `platform` and `room` only** in the first version.
   More fields come when a backend needs them.
5. **`tehas scope` is a tehas command**, in the release, running with no
   settings, so it works on a laptop before any host exists.
6. **The JSON Schema is `api/needs.schema.json`**, under the same licence
   as the rest of `api/`.

## Open points

1. Mobile previews (`kind = "mobile"`): what a factory would need to build
   and show one. The value is accepted; nothing reads it yet.

## Work

1. The schema and its reader in Rust, with tests: unknown fields and
   malformed values exit 78.
2. `tehas scope`: the readers of each file listed above, each a pure
   function of a checkout's files, with fixtures; `--json`, `--write`,
   `--check`.
3. `tehas check-product`: a `needs` line per table.
4. `tehas init-product`: propose the file with the other contract files.
5. `tehas setup`: the toolchain layer of the sandbox image, the room, the
   egress list and the secret names, from the file.
6. The manual: the contract table on the new project page, Start.
