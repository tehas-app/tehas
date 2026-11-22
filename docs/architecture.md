# Architecture

The knowledge map of tehas: each piece of knowledge, who owns it and
where else it is known; the ports; the crate graph; and what the 1.x
contract froze. It is the map the refactor to coding standard v2 works
from, and it is kept current as the steps land. The rules are in
`docs/architecture-rubric.md`, the questions for a pull request in
`docs/architecture-review.md`, and the checks in `architecture.toml`.

Counts are at the fork, v1.4.0 (a0aa16a5), from `python3 -I
scripts/architecture.py --force`. They are grep counts and approximate.
Tests count, in files of their own and in `#[cfg(test)]` modules alike,
so a test that moves does not lower a count it did not fix.

## Knowledge map

| Knowledge | Check | Owner today | Known elsewhere (count) | Owner under v2 |
| --- | --- | --- | --- | --- |
| GitHub's REST paths, GraphQL, the API host | `github-rest` | the forge, for labels only (`console/core/src/forge.rs`, `forge/`) | 846, with GitHub's JSON fields and `--json`/`--jq`: the console 240, dispatch 225, ops 98, cycle 93, the front pages 69, mcp 38, store 37, core 21, analytics 8, xtask 6 | the GitHub adapter of the three ports (T4) |
| Running `gh` | `gh-argv` | the forge's `GhRunner` | 408: dispatch 207, cycle 67, the console 62, ops 34, core 13, store 11, mcp, xtask 4 each | the GitHub adapter; the security shims a written floor if they stay |
| State labels and markers (`tehas:...`) | `label-words` | none; `host/labels.tsv` is the list | 1878, with `needs-human-attention` and `epic`: dispatch 1010, cycle 228, core 189, the console 184, ops 137, store 70, the front pages 34, mcp 21 | one module in core, checked against `host/labels.tsv` (T2) |
| The `$TEHAS_HOME` layout: `state/`, `live/`, `run/` | `home-layout` | none | 1356: dispatch 341, cycle 278, ops 238, store 145, the console 124, analytics 98, proxy 31, contractor, core 25 each, `install.sh` 21, mcp 16, the units 5 | `Home` (T5); bash asks it (T8) |
| The store's verbs and answers | review | the store (`console/store/src/verbs.rs`) | callers pass verb strings to `call` and read `Value` | a typed store client (T5) |
| `serde_json::Value` in signatures | `untyped-signatures` | none | 1421: dispatch 369, mcp 251, cycle 181, the console 162, core 128, ops 89, store 77, analytics 45, attach 42, plugin 33, proxy 28, contractor 16 | parsed at the edge (T3, T4, T5, T7) |
| `Value` in public fields, enum variants, tuple fields | `untyped-fields` | none | 66: dispatch 38, cycle 7, ops 5, attach, contractor, core, the console 3 each | types of ours (T3, T7) |
| Settings and their names | review | core's settings (`console/core/src/settings/`), `lib/config.sh` for bash | the contract's settings list | unchanged |
| GitHub's HTTP statuses | `raw-statuses` | none outside the forge | 259: core 92, dispatch 51, the console 46, ops 31, cycle 26, plugin 6 | the adapter, as the ports' named outcomes (T4) |
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

| Trait | File | Abstracts | In our terms | Fake |
| --- | --- | --- | --- | --- |
| `Forge` | `console/core/src/forge.rs` | labels: what an issue carries, the newest event, who applied it, add, remove | yes | none: its tests stub `GhRunner` |
| `GhRunner` | `console/core/src/forge.rs` | running `gh` for the forge | no: argv | stubbed `gh` runner in tests |
| `GitHub` | `console/core/src/access.rs` | a collaborator's permission | partly | in `access.rs` tests |
| `Net` | `console/attach/src/take.rs` | fetching an issue's files | yes | in attach's tests |
| `Run` | `console/cycle/src/relay.rs` | the relay's `gh` calls | no: argv | in relay tests |
| `Vipune` | `console/cycle/src/memory.rs` | the memory store | yes | in cycle's tests |
| `Hands` | `console/dispatch/src/finish.rs`, `auto_finish.rs` | the finish's effects | partly | in dispatch's tests |
| `Factory`, `Backing`, `Records`, `Reader`, `Sink`, `Downloads` | `console/proxy/src/` | the model proxy's channel, jobs and records | yes | in proxy's tests, except `Downloads` (none) |
| `Work`, `World` | `console/contractor/src/job.rs` | the contractor's jobs | yes | in the crate's tests |
| `Host`, `Catalog`, `Refresh`, `Day` | `console/plugin/src/` | the plugin runner | yes | in the crate's tests |
| `Exec` | `console/ops/src/plugin/exec.rs` | running a plugin | partly | in the crate's tests |
| `Outside`, `Shell` | `console/ops/src/rukas.rs`, `update/` | the update's outside world | partly | in the crate's tests |

v2 adds three ports, with GitHub as their one adapter (T4): **tickets**
(the work level), **blueprints** (the issues a cycle works from) and the
**code host** (changes, checks, merges, tags, releases). Each gets a fake.

## Checks of the core (R3, R8)

The core today is the dispatcher's `decide` and the modules it decides
from: `decide.rs`, `decide/`, `place.rs`, `snapshot.rs`, `text.rs`,
`settings.rs`, `questions.rs`, `starts.rs`. `core-effects` holds it at 0:
no file, network, process, environment or clock. `core-inward` counts 3
uses of the shell in `decide.rs` (the push fault's hold from
`read::host`, `call::trim`, `apply::start::change_note`), which T3 moves.
tehas-core is not the core: it holds the forge, the sandbox and the
process runner. T6 settles where the core ends.

## Write guards

Each of these decides whether a GitHub call writes. They encode one rule
and may stay separate on purpose; the owner decides (T4).

- the dispatcher's read guard (`console/core/src/read_only.rs`);
- the console's gh shim (`console/src/gh_shim.rs`, `gh_writes.rs`);
- the sandbox's gh shim (`host/sandbox/gh-shim`);
- cycle's relay (`console/cycle/src/relay.rs`).

## Crate graph

From `cargo metadata`. Tests' own dependencies are in the last column.

| Crate | Depends on | Its tests also use |
| --- | --- | --- |
| tehas-core | nothing of the workspace | |
| store, attach, tehas-front, tehas-mcp, tehas-xtask | tehas-core | |
| analytics | tehas-core | tehas-mcp |
| tehas-cycle | attach, tehas-core | |
| tehas-dispatch | attach, tehas-core | |
| tehas-ops | store, tehas-core, tehas-cycle | tehas-dispatch |
| tehas-proxy | store, tehas-core, tehas-cycle | |
| tehas-contractor | tehas-core, tehas-cycle | store, tehas-proxy |
| tehas-plugin | tehas-core, tehas-ops | |
| tehas-plugin-gate | tehas-core, tehas-plugin | |
| console | attach, store, tehas-core, tehas-cycle, tehas-ops | tehas-dispatch |

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
| Commands and flags | `api/commands.txt`, `surface-1.0.txt`; frozen since v1.0.0-beta.1 | `tehas github check` and `tehas github writes` name a supplier; to be named for the ports |
| Exit statuses | contract, "Exit statuses"; since v1.0.0-beta.1 | no |
| Settings | contract, "Settings and the knob rule"; `api/settings-rules.json`; since v1.0.0-beta.1 | settings that choose the adapters; the release repository at the rename (T10) |
| Labels | `host/labels.tsv`; since v1.0.0-beta.1 | the contract names the act (approve, block, plan, refine); labels become the GitHub adapter's answer |
| The factory API | `api/api.schema.json`, `status.schema.json` and the other schemas; since v1.0.0-beta.1, each later name added in the release `tehas api` lists it under | fields that name GitHub's shapes, where the ports rename them |
| The plugin API | `api/plugin-*.schema.json`, the grants; not frozen until the feature ships | written into the 2.0 contract as it stands; events (`hook:<when>:<event>`) added (T11) |
| Events | none yet | new: the event names and schemas (T11) |

## Departures and floors

A count that stays above 0 at the end of the work is written here with
its reason, as a floor; a departure from a should rule (R5) likewise.

| Check | Floor | Where | Reason |
| --- | --- | --- | --- |
| gh-argv, tests-stub-gh | 1 each | `console/tests/top_snapshot.rs` | the short name "gh" of a process in a fixture of /proc (`"name":"gh"`, #2859), which tehas top shows. Nothing runs gh. A pattern that skipped it would be fragile and allowing the file would hide a later gh call there, so it stays counted as a floor |

### Temporary rises

Empty since #2890. A T15 pull request that raises a count narrows the check in the same pull request: a test file whose every match is the check's data or tehas's own words goes in the check's `allowed`, by name and with what it covers, or a list copied in many tests gets one owner. A real finding goes to the step that fixes it in the plan.

#2890 cleared the rows of #2859, #2860, #2866 and #2879 that way. Allowed by name: console/tests/store_paths.rs for label-words and home-layout (the store lint's patterns and sample lines), console/tests/host_bridge.rs for raw-statuses (the docs server's 404), and console/cycle/tests/headless_caps.rs, top_snapshot.rs, top_view.rs and top_frame.rs for github-rest (the runner's and tehas top's --json, tehas's running.json). The list of tools every test stubs to fail has one owner, console/core/src/testing.rs, allowed in gh-argv, in place of eight copies of the deny list and three lists of tools stubbed to fail; that took out 10 in gh-argv and tests-stub-gh, 5 of them older than T15. Its findings moved: two github-rest lines to T4's Done, one untyped-signatures line to T7's, and the /proc fixture to the floor above. Counts at #2890: github-rest 825, gh-argv 395, tests-stub-gh 403, home-layout 491, label-words 1866, raw-statuses 259.

### Fork values

Each check's count at the fork (v1.4.0, a0aa16a5), as `architecture.toml` measured it once its patterns were final (#2780). The final review compares each count with this.

| Check | Fork value |
| --- | --- |
| github-rest | 846 |
| gh-argv | 408 |
| label-words | 1878 |
| home-layout | 1356 |
| untyped-signatures | 1421 |
| untyped-fields | 66 |
| core-effects | 0 |
| core-inward | 3 |
| raw-statuses | 259 |
| tests-stub-gh | 419 |

Above its fork value today: none. Below it: github-rest (825, 21 below), gh-argv (395, 13 below), tests-stub-gh (403, 16 below), label-words, home-layout and untyped-signatures. raw-statuses is at it (259).

## Grades

The grade is the ten bands, never a sum. Rows are appended, never
rewritten.

| Date | Commit | R1 | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 | R10 | Pass line |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-11-22 | 0538004d (baseline) | partial | partial | partial | partial | partial | partial | partial | mostly | mostly | baseline: no pull requests since a grade | below adopting: the test is not in CI yet |

The baseline was graded by the overseer's three fresh graders on a
one-commit export of 0538004d, with `docs/plans/` and this section cut.
All three agree on R1 to R8. On R9 two read `mostly` and one `met`; the
lower band stands. A person confirms the grade.
