# Architecture

The knowledge map of tehas: each piece of knowledge, who owns it and
where else it is known; the ports; the crate graph; and what the 1.x
contract froze. It is the map the refactor to coding standard v2 works
from, and it is kept current as the steps land. The rules are in
`docs/architecture-rubric.md`, the questions for a pull request in
`docs/architecture-review.md`, and the checks in `architecture.toml`.

Counts are at the fork, v1.4.0 (a0aa16a5), from `python3 -I
scripts/architecture.py --force`. They are grep counts: approximate, and
they include `#[cfg(test)]` modules inside source files, which the test
cannot leave out. Tests in files of their own are left out.

## Knowledge map

| Knowledge | Check | Owner today | Known elsewhere (count) | Owner under v2 |
| --- | --- | --- | --- | --- |
| GitHub's REST paths, GraphQL, the API host | `github-rest` | the forge, for labels only (`console/core/src/forge.rs`, `forge/`) | 162: dispatch 57, the console 45, ops 22, cycle 14, core 13, the front pages 4, attach, mcp, plugin 2 each, plugin-gate 1 | the GitHub adapter of the three ports (T4) |
| Running `gh` | `gh-argv` | the forge's `GhRunner` | 153: dispatch 39, the console 34, ops 32, cycle 22, core 13, store, xtask 4 each, attach, mcp, the sandbox build, `lib/`, `scripts/` 1 each | the GitHub adapter; the security shims a written floor if they stay |
| State labels and markers (`tehas:...`) | `label-words` | none; `host/labels.tsv` is the list | 886: dispatch 405, core 164, ops 132, the console 81, cycle 48, the front pages 30, mcp 18, store 6 | one module in core, checked against `host/labels.tsv` (T2) |
| The `$TEHAS_HOME` layout: `state/`, `live/`, `run/` | `home-layout` | none | 512: ops 232, dispatch 91, cycle 60, the console 39, core 23, `install.sh` 21, store 20, plugin 8, proxy 7, analytics 6, contractor 5 | `Home` (T5); bash asks it (T8) |
| The store's verbs and answers | review | the store (`console/store/src/verbs.rs`) | callers pass verb strings to `call` and read `Value` | a typed store client (T5) |
| `serde_json::Value` in signatures | `untyped-signatures` | none | 1044: dispatch 270, mcp 170, the console 138, core 126, cycle 125, ops 78, attach 35, analytics 34, plugin 21, store 20, proxy 16, contractor 11 | parsed at the edge (T3, T4, T5, T7) |
| `Value` in public fields | `untyped-fields` | none | 56: dispatch 37, cycle 6, ops 4, core 3, contractor, the console 2 each, attach, proxy 1 each | types of ours (T3, T7) |
| Settings and their names | review | core's settings (`console/core/src/settings/`), `lib/config.sh` for bash | the contract's settings list | unchanged |
| The cycle's container: mounts, environment | `test/fixtures/argv/` | `console/core/src/sandbox.rs` | pinned by `test/work_test.sh`, `test/sandbox_argv_test.sh` | unchanged (AGENTS.md rule 4) |

## Outside systems

| System | How it is reached | From |
| --- | --- | --- |
| GitHub | `gh` (REST and GraphQL), and the REST API directly in `console/core/src/access.rs` | most crates; see `github-rest` and `gh-argv` |
| git | `git` | core, cycle, dispatch, ops, the console, plugin, proxy, contractor, xtask |
| Docker | `docker` | core (`sandbox.rs`), cycle, dispatch, ops, the console, plugin, mcp, proxy, contractor, xtask |
| systemd | `systemctl` and the units of `host/systemd/` | core, cycle, dispatch, ops, the console, plugin, `install.sh` |
| The models | HTTP through the model proxy | proxy, contractor |
| pi-rukas | the sandbox image, `console/core/src/rukas.rs` | core, ops |
| vipune | `host/sandbox/vipune-shim`, `console/cycle/src/memory.rs` | cycle |
| Caddy | the containers of `host/edge/` | ops (edge) |

## Ports

The traits that stand between the code and an outside system today.

| Trait | File | Abstracts | In our terms |
| --- | --- | --- | --- |
| `Forge` | `console/core/src/forge.rs` | labels: what an issue carries, the newest event, who applied it, add, remove | yes |
| `GhRunner` | `console/core/src/forge.rs` | running `gh` for the forge | no: argv |
| `GitHub` | `console/core/src/access.rs` | a collaborator's permission | partly |
| `Net` | `console/attach/src/take.rs` | fetching an issue's files | yes |
| `Run` | `console/cycle/src/relay.rs` | the relay's `gh` calls | no: argv |
| `Vipune` | `console/cycle/src/memory.rs` | the memory store | yes |
| `Hands` | `console/dispatch/src/finish.rs`, `auto_finish.rs` | the finish's effects | partly |
| `Factory`, `Backing`, `Records`, `Reader`, `Sink`, `Downloads` | `console/proxy/src/` | the model proxy's channel, jobs and records | yes |
| `Work`, `World` | `console/contractor/src/job.rs` | the contractor's jobs | yes |
| `Host`, `Catalog`, `Refresh`, `Day` | `console/plugin/src/` | the plugin runner | yes |
| `Exec` | `console/ops/src/plugin/exec.rs` | running a plugin | partly |
| `Outside`, `Shell` | `console/ops/src/rukas.rs`, `update/` | the update's outside world | partly |

v2 adds three ports, with GitHub as their one adapter (T4): **tickets**
(the work level), **blueprints** (the issues a cycle works from) and the
**code host** (changes, checks, merges, tags, releases). Each gets a fake.

## Write guards

Each of these decides whether a GitHub call writes. They encode one rule
and may stay separate on purpose; the owner decides (T4).

- the dispatcher's read guard (`console/core/src/read_only.rs`);
- the console's gh shim (`console/src/gh_shim.rs`, `gh_writes.rs`);
- the sandbox's gh shim (`host/sandbox/gh-shim`);
- cycle's relay (`console/cycle/src/relay.rs`).

## Crate graph

| Crate | Depends on |
| --- | --- |
| tehas-core | nothing of the workspace |
| store, analytics, attach, tehas-front, tehas-mcp, tehas-xtask | tehas-core |
| tehas-cycle | attach, tehas-core |
| tehas-dispatch | attach, tehas-core |
| tehas-ops | store, tehas-core, tehas-cycle |
| tehas-proxy | store, tehas-core, tehas-cycle |
| tehas-contractor | tehas-core, tehas-cycle |
| tehas-plugin | tehas-core, tehas-ops |
| tehas-plugin-gate | tehas-core, tehas-plugin |
| console | attach, store, tehas-core, tehas-cycle, tehas-ops |

core depends on nothing, so the graph points inward at the root (R8).
The direction is not checked in CI yet. core itself mixes the pure
rules with effects (`sandbox.rs`, `access.rs`, `call.rs`, `retry.rs`):
T6 settles where the core ends.

## Grouping (R5)

The plan's reading, to be checked by the grades: the store, mcp, attach,
proxy, contractor and analytics are deep. core, ops, dispatch, cycle and
the console are grouped in places by the order the old bash ran, not by
what they know. The `public-items` report lists the widest files (3553
public items in all: core 1027, ops 411, cycle 385, store 299, the
console 255, dispatch 212).

## Tests that pin GitHub (R7)

The shell tests stub `gh` with `t_stub gh` (84 uses in `test/`), and several Rust tests assert
`gh` argument lists. T1 moves them onto the ports' fakes, keeping the
answers, crate by crate before the calls move.

## Contract

What 1.x froze is `host/docs/contract.md` with the `api/` files
(`surface-1.0.txt`, `commands.txt`, `features.txt`, the schemas and
samples) and `api/renames-1.0.tsv`. This work changes it only where the
refactoring needs to, each change named in its pull request, added beside
the old term and removed at T9, and agreed with the owner first.

| Part | Where frozen | Expected to change in 2.0 |
| --- | --- | --- |
| Commands and flags | `api/commands.txt`, `surface-1.0.txt` | `tehas github check` and `tehas github writes` name a supplier; to be named for the ports |
| Exit statuses | contract, "Exit statuses" | no |
| Settings | contract, "Settings and the knob rule"; `api/settings-rules.json` | settings that choose the adapters; the release repository at the rename (T10) |
| Labels | `host/labels.tsv` | the contract names the act (approve, block, plan, refine); labels become the GitHub adapter's answer |
| The factory API | `api/api.schema.json`, `status.schema.json` and the other schemas | fields that name GitHub's shapes, where the ports rename them |
| The plugin API | `api/plugin-*.schema.json`, the grants | written into the 2.0 contract as it stands; events (`hook:<when>:<event>`) added (T11) |
| Events | none yet | new: the event names and schemas (T11) |

## Grades

The grade is the ten bands, never a sum. Rows are appended, never
rewritten.

| Date | Commit | R1 | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | Pass line |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
