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
| GitHub's REST paths, GraphQL, the API host | `github-rest` | the forge's GitHub adapter, for labels only (`console/github/`; its runner in `console/core/src/forge.rs` until T4.11) | 846, with GitHub's JSON fields and `--json`/`--jq`: the console 240, dispatch 225, ops 98, cycle 93, the front pages 69, mcp 38, store 37, core 21, analytics 8, xtask 6 | the GitHub adapter of the three ports (T4) |
| Running `gh` | `gh-argv` | the forge's `GhRunner` | 408: dispatch 207, cycle 67, the console 62, ops 34, core 13, store 11, mcp, xtask 4 each | the GitHub adapter; the security shims a written floor if they stay |
| State labels and markers (`tehas:...`) | `label-words` | none; `host/labels.tsv` is the list | 1878, with `needs-human-attention` and `epic`: dispatch 1010, cycle 228, core 189, the console 184, ops 137, store 70, the front pages 34, mcp 21 | one module in core, checked against `host/labels.tsv` (T2) |
| The `$TEHAS_HOME` layout: `state/`, `live/`, `run/` | `home-layout` | none | 1356: dispatch 341, cycle 278, ops 238, store 145, the console 124, analytics 98, proxy 31, contractor, core 25 each, `install.sh` 21, mcp 16, the units 5 | `Home` (T5); the bash and the units at a written floor (T8) |
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
| git | `git` | core, cycle, dispatch, ops, the console, plugin, xtask |
| Docker | `docker` | core (`sandbox.rs`), cycle, dispatch, ops, the console, plugin, mcp, xtask |
| systemd | `systemctl` and the units of `host/systemd/` | core, cycle, dispatch, ops, the console, plugin, `install.sh` |
| pi-rukas | the sandbox image, `console/core/src/rukas.rs` | core, ops |
| Caddy | the containers of `host/edge/` | ops (edge) |

## Ports

The traits that stand between the code and an outside system today.

| Trait | File | Abstracts | In our terms | Fake |
| --- | --- | --- | --- | --- |
| `Tickets`, `TicketWrites` | `console/ports/src/tickets.rs`, answered in part by the GitHub adapter in `console/github/` (T4.3) | a ticket's digest, its marks going on and off, the factory's labels made, a mark put on or taken off | yes | `tehas_ports::fake`; the adapter's own tests stub `GhRunner` |
| `GhRunner` | `console/core/src/forge.rs` | running `gh` for the forge | no: argv | stubbed `gh` runner in tests |
| `GitHub` | `console/core/src/access.rs` | a collaborator's permission | partly | in `access.rs` tests |
| `Net` | `console/attach/src/take.rs` | fetching an issue's files | yes | in attach's tests |
| `Hands` | `console/dispatch/src/finish.rs`, `auto_finish.rs` | the finish's effects | partly | in dispatch's tests |
| `Host`, `Catalog`, `Refresh`, `Day` | `console/plugin/src/` | the plugin runner | yes | in the crate's tests |
| `Exec` | `console/ops/src/plugin/exec.rs` | running a plugin | partly | in the crate's tests |
| `Outside`, `Shell` | `console/ops/src/rukas.rs`, `update/` | the update's outside world | partly | in the crate's tests |

v2 adds three ports, with GitHub as their one adapter (T4): **tickets**
(the work level), **blueprints** (the issues a cycle works from) and the
**code host** (changes, checks, merges, tags, releases). Each gets a fake.

Their types are in a crate of their own, `console/ports/` (`tehas-ports`),
which a consumer vendors at the tag: std and serde alone, no supplier word,
no effects. T4.1 lands it in four parts. T4.1a: the data types (ids,
bodies, bounds, accounts, outcomes, marks and markers). T4.1b: the traits,
the board, the site, the links and the write, with the per-pass reports
the traits take (`Reports`, `Report`, `Caller`, `PortName`).
T4.1c: the ledger and health: `Ledger`, the budget and health state
shared across processes, keyed by `HostId` and `Whose` (the host, one
`Holder` on it, or one pool of a holder), and `health::next`, the pure step from one outcome to the
next state and the `Notice`, if the state changed. T4.1d: the fakes
(`tehas_ports::fake`: `World`, `FakeSite`, `Faulty`, the `NoticeLog` sink,
`MemoryLedger`, `MemoryTokens`), the laws (`tehas_ports::laws`, the
contract test of any adapter) and the `Writer`; with it the crate holds
what step T4.1 names. A handle takes `&mut self` and is `Send`; a `Site` and the
caller's stores (`Links`, `Keyring`, `TokenStore`) are `Send + Sync` and
take `&self`. `Join` is the board of a split setup: pure code over three
owned read handles and the links, each ticket answered alone.

## Checks of the core (R3, R8)

The core today is the dispatcher's `decide` and the modules it decides
from: `decide.rs`, `decide/`, `place.rs`, `snapshot.rs`, `text.rs`,
`settings.rs`, `questions.rs`, `starts.rs`. `core-effects` holds it at 0:
no file, network, process, environment or clock. `core-inward` counts 3
uses of the shell in `decide.rs` (the push fault's hold from
`read::host`, `call::trim`, `apply::start::change_note`), which T3 moves.
tehas-core is not the core: it holds the forge, the sandbox and the
process runner. T6 settles where the core ends.

## Checks of the ports (R1, R3, R8)

Three checks hold `console/ports/` at 0, as the ports spec's "The crate"
asks. `ports-deps`: its manifest holds its package keys and serde alone,
so no other dependency, feature or build script, and serde only on the
spec's exact line (version 1, default features off, `std` and `derive`);
any other serde line counts. `ports-words`: no supplier name, `repos/`,
`api.`, `graphql` or label word in its source or manifest, doc lines
included. `ports-effects`: no file, network, process, environment, thread
or clock in its source, whether reached by a path, a grouped import
(`use std::{fs, ...}`, also spread over lines) or an alias
(`use std::{env as e}`, `std as`); no standard stream (`stdin`,
`stdout`, `stderr`); and no `print!`, `println!`, `eprint!`, `eprintln!`
or `dbg!`; no `thread_local!`, state kept per thread being hidden global
state. In-process sharing (`std::sync`), which the fakes use for
`Send + Sync`, is no effect and is not counted. The crate's tests round-trip
its types through a small serde format of their own, since the crate takes
no serde_json; `untyped-signatures` allows that one file by name, as its
matches are serde's own `Visitor::Value`.

`event-names` (R1, T11.1) holds an event name's string to the ports'
events module: a string literal equal to a name, quotes included, counts
anywhere else in `console/`, comments too. Code names `Name::CycleEnded`;
a test that needs a name in JSON takes it from `Name::as_str`. A new name
joins the pattern in the pull request that adds it.

`adapter-wiring` (R8, T4.2) holds the GitHub adapter, `tehas-github`, to
the wiring of the programs: a line that names `tehas_github` counts
anywhere in `console/` but the adapter and a program's `src/main.rs`. Its
count was the callers of `GitHub::new` that had not moved behind the
ports yet. T4.3 moved the last of them, so it is 0: the console's wiring
is a module of `console/src/main.rs`, and its commands take their handles
from there.

## Write guards

Each of these decides whether a GitHub call writes. They are kept
separate on purpose; whether any merge is the owner's (T4). Where each
sits against the ports is in `docs/specs/2026-11-22-ports.md`, "The write
guards".

- the dispatcher's read guard, `gh_reads` in
  `console/dispatch/src/read/run.rs`, inside the poll;
- the read-only host, `TEHAS_READ_ONLY` (`console/core/src/read_only.rs`),
  asked by the console's shim and by git pushes;
- the console's gh shim (`host/shim/gh`, with `console/src/gh_shim.rs` and
  `gh_writes.rs`), under every host `gh` call;
- the sandbox's gh shim (`host/sandbox/gh-shim`), in a cycle's container;
- cycle's relay (`console/cycle/src/relay.rs`), between the container and
  the host.

## Crate graph

From `cargo metadata`. Tests' own dependencies are in the last column.

| Crate | Depends on | Its tests also use |
| --- | --- | --- |
| tehas-ports | nothing of the workspace, serde alone | |
| tehas-core | nothing of the workspace | attach |
| attach, tehas-front, tehas-mcp, tehas-xtask | tehas-core | |
| store | tehas-core, tehas-ports | |
| analytics | tehas-core | tehas-mcp |
| tehas-cycle | attach, tehas-core | |
| tehas-items | attach, tehas-core | |
| tehas-dispatch | attach, tehas-core | |
| tehas-ops | store, tehas-core, tehas-cycle | tehas-dispatch |
| tehas-plugin | tehas-core, tehas-ops | |
| tehas-plugin-gate | tehas-core, tehas-plugin | |
| console | attach, store, tehas-core, tehas-cycle, tehas-items, tehas-ops | tehas-dispatch |

core depends on nothing, so the graph points inward at the root (R8).
Its tests read links by attach's rule, which the issue feed takes as a
parameter.
The direction is not checked in CI yet. core itself mixes the pure
rules with effects (`sandbox.rs`, `access.rs`, `call.rs`, `retry.rs`):
T6 settles where the core ends.

## Kept for the 2.1 plugins

Some crates have no caller in 2.0 on purpose. They hold the plumbing of the
features 2.0 took out of core (T14, T16), kept for the 2.1 plugins that bring
those features back
(`docs/specs/2026-11-22-plugin-lab.md`,
`docs/specs/2026-11-22-plugin-surfaces-2.1.md`). A dead-code pass of 2.0.x
leaves them alone.

| Crate | Kept for |
| --- | --- |
| `console/items` | the lab plugin's issue feed and page writer |
| `console/attach` | the `files:` grant and the plugin mode of attachments; 2.0's cycles use it too |
| `console/describe` | the `given` plugin's image descriptions |
| `console/review-render` | the `review` plugin's issue body |
| `console/wiki-collect` | the `history:` grant's raw sources |
| `console/wiki-render` | the `wiki` plugin's check, render and keep rules |
| `console/mesh-feed` | the host-made `feed:mesh` |
| `console/host-feed` | the host-made `feed:host` |

`console/xtask/tests/kept_for_plugins.rs` fails when one of them leaves the
workspace or loses its tests, and when this table does not name it. A crate
leaves the list only with the plugin specification change that drops its
use. core's `outside.rs`, the rules of an outside page for the `frame`
surface, is kept too, but it is live: `tehas host-message` calls it.

## Grouping (R5)

The plan's reading, to be checked by the grades: the store, mcp, attach
and analytics are deep. core, ops, dispatch, cycle and
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
| gh-argv, tests-stub-gh | 3 and 2 | gh-argv: `console/ops/src/pins/tools.rs` 1 and `console/ops/src/pins/tests.rs` 2; tests-stub-gh: `console/ops/src/pins/tests.rs` 2 | the pinned tool named "gh" in the release's list of pinned tools (`host/versions.env`) and its tests (ports spec, Decided 71). Nothing runs gh there; the name is the tool's |
| home-layout | 16, in the bash and the units | `install.sh` 12, `host/systemd/` 4 | T8's freeze, below |
| gh-argv | 3, in the bash | `lib/github-app.sh` 1, `host/sandbox/gh-shim` 1, `scripts/tag-release.sh` 1 | T8's freeze, below |
| github-rest | 4, in the bash | `host/sandbox/gh-shim` 4 | T8's freeze, below |
| tests-stub-gh | 83, in the shell tests that stay shell | 18 files of `test/` | T8's freeze, below |

### The bash that stays (T8)

The owner, 2026-11-23: T8 does not rewrite the bash that is left. It is
frozen at its count of that day, as a written floor. A count here may
only fall. A pull request that adds a match in bash takes out as many,
and the one that takes a match out lowers the number here and the
check's `max`. The numbers are ceilings measured on 5c112a145 with #2981,
by `scripts/architecture.py`'s count over the files named. #2981 was the
one small change that could lower a count: `install.sh` now writes each
path under `state/` once.

What is still bash, and why it stays bash in 2.0:

| Piece | What it still owns | Why it stays bash |
| --- | --- | --- |
| `install.sh` | the copies into the home, the build of the programs, the update lock and the dispatcher's lock, the first `state/` | it is what builds and installs the programs, so it runs where none is installed yet; it builds a console first and asks it for the settings, the waits and the steps, but no console verb prints a path of the home |
| `bootstrap.sh` | the tools of a fresh host at the versions of `host/versions.env` | a fresh host has no Rust toolchain and no program of ours until it has run |
| the units, `host/systemd/` | when and as whom the programs run | systemd reads them; a unit is given a path, it cannot ask a program for one |
| `lib/config.sh`, `lib/paths.sh`, `host/env.sh` | loading the instance's settings and the tool PATH into a login shell and a unit | a shell's environment is set by sourcing; the settings' names and rules are core's, which `lib/config.sh` asks through the console |
| `lib/github-app.sh`, `host/shim/gh` | a login shell acting as the GitHub App | environment and PATH of the shell, and a `gh` on PATH; the token is made in Rust |
| `bin/`, `libexec/` | finding the console and running it | 24 and 55 lines that exec the console; `libexec/setup` builds one where a new host has none |
| `host/sandbox/gh-shim`, `host/sandbox/build.sh` | the `gh` of a cycle's container, the image build | the shim is the sandbox's security shim and runs in an image with no program of ours; one of the write guards above |
| `scripts/` | the gate and the release | they run in a checkout and in CI, before anything is built |

The lines of `bin/`, `lib/` and `libexec/` have their own ceiling,
`scripts/bash-lines.max`: 186, and 186 measured. The shell of `test/` has
`scripts/test-lines.max`: 34514, and 34514 measured. The gate checks
both.

The checks that read bash, and what each still counts there:

| Check | Frozen | Fork value of the whole check | What the matches are |
| --- | --- | --- | --- |
| label-words | 0 | 1878 | none in bash or the units |
| home-layout | 16 | 1356 | `install.sh` 12: ten paths under the home it writes once each (the state directory, `state/tehas.db`, `state/backup`, `state/dispatch.lock`, `state/update.lock`, `state/sandbox.Dockerfile.sha256`, `state/first-install`, `state/cycles` in a note, `state/index.json`, and `live` on the line that makes the home's directories) and two comments (lines 97 and 140). The units 4: `tehas-dispatch.path` and `tehas-dispatch.service` name `state/dispatch.again`, `tehas-mcp-fetch.socket` names `run/mcp-fetch/fetch.sock`, and `tehas-cycle@.service` line 9 is a comment |
| gh-argv | 3 | 408 | `lib/github-app.sh` line 19, the credential helper string git is given, which names the console's shim. Two comments: `host/sandbox/gh-shim` line 33 and `scripts/tag-release.sh` line 68 |
| github-rest | 4 | 846 | `host/sandbox/gh-shim`: lines 43, 50 and 57 read whether a `gh api graphql` call is a mutation, to refuse a write on a read-only host. Line 33 is a comment |
| tests-stub-gh | 83 | 419 | the shell tests that ports spec T4.12 and the tests-stub-gh row of its Done keep as written floors: `github_app_test.sh` 22, `gh_fail_test.sh` 21 (the test of `t_gh_fail` in `test/lib.sh`), the six `shim_writes_*` files 16, `init_product_test.sh` 7, `tag_release_test.sh` 6, `install_first_test.sh` 4, and 1 each in `sandbox_argv_test.sh` (rule 4), `gh_shim_test.sh`, `bootstrap_lib.sh`, `bootstrap_test.sh`, `install_lib.sh`, `setup_test.sh` and `config_defaults_test.sh`. The spec names `install_fresh_test.sh` and `config_test.sh`; their cases are in `install_first_test.sh` and `config_defaults_test.sh` now, and `bootstrap_test.sh`'s 2 are split with `bootstrap_lib.sh` |

Six of the 23 matches outside the tests are comments, on five lines
(`host/sandbox/gh-shim` line 33 counts in two checks). They count
because those checks skip lines that start `//`, the Rust comment, and
not `#`. They are left as they are: rewording a comment to miss a pattern
fixes nothing.

Why the rest stays. `install.sh` has a console by the time it writes to
the home, and could ask `Home` for these ten paths if a console verb
printed them. None does. Adding one is new Rust and a new command in the
contract (`api/commands.txt`), not a small change to the bash, and the
owner's freeze leaves it out of 2.0. The units need real paths: systemd
cannot ask. The credential helper is a string git reads from the environment of a
login shell. The gh shim of the sandbox must read `gh`'s arguments to
decide whether a call writes. The 83 shell tests are those whose point
is the bash, as the ports spec keeps them: the shims, the container's
argument lists (rule 4), the App's shell side, the harness's own gh
failure, install, bootstrap, init-product and setup, the settings library
and the release script.

Not in this floor. The other 30 matches of tests-stub-gh in 20 files of
`test/`, `update_test.sh` among them, stub `gh` for a behaviour of Rust
code: ports spec T4.12 moves them into Rust over the fake, with the
crate they test, and the plan's T15 ports the shell tests to Rust. Its 36
matches in data files of Rust tests are not bash. Dispatch's JSON
fixtures 21 give way to builders at T4.6. The store's 1.x SQL fixtures
10, core's text samples 4 and mcp's log sample 1 are written floors of T4
(ports spec T4.4, T4.8 and T4.11, and its Done). home-layout's matches in
the Rust test files of cycle (70) and of the console (8, and 1 in
`console/src/wake.rs`) are T5's, whose Done is home-layout at 0 outside
`Home`: cycle's are 64 shell stubs of the runner writing its notes under
`state/cycles`, 3 expected values and 3 files a test checks are not made;
the console's are 5 shell stubs in its update tests (`tests/update.rs` 2,
`update/mod.rs` 2, `update_checkout.rs` 1) and 4 paths a test holds to
check against, `wake.rs` line 89 an expected path.

### Temporary rises

Empty since #2890. A T15 pull request that raises a count narrows the check in the same pull request: a test file whose every match is the check's data or tehas's own words goes in the check's `allowed`, by name and with what it covers, or a list copied in many tests gets one owner. A real finding goes to the step that fixes it in the plan.

#2890 cleared the rows of #2859, #2860, #2866 and #2879 that way. Allowed by name: console/tests/store_paths.rs for label-words and home-layout (the store lint's patterns and sample lines), console/tests/host_bridge.rs for raw-statuses (the docs server's 404), and console/cycle/tests/headless_caps.rs, top_snapshot.rs, top_view.rs and top_frame.rs for github-rest (the runner's and tehas top's --json, tehas's running.json). The list of tools every test stubs to fail has one owner, console/core/src/testing.rs, allowed in gh-argv, in place of eight copies of the deny list and three lists of tools stubbed to fail; that took out 10 in gh-argv and tests-stub-gh, 5 of them older than T15. Its findings moved: two github-rest lines to T4's Done, one untyped-signatures line to T7's, and the /proc fixture to the floor above. Counts at #2890: github-rest 819, gh-argv 386, tests-stub-gh 401, home-layout 491, label-words 1855, raw-statuses 259.

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

Above its fork value today: none. Below it: github-rest (804, 42 below), gh-argv (378, 30 below), tests-stub-gh (393, 26 below), raw-statuses (255, 4 below), label-words, home-layout and untyped-signatures.

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
