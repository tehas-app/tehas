---
title: The contract
description: "What of tehas is public and stays compatible through every 1.x release, what is internal and may change, the rules for settings and a cycle's inputs, and the exit statuses of every public command."
---

# The contract

This page says what a person or a program may rely on. What it names public
stays compatible through every 1.x release: within 1.x a public name is only
added, never renamed, removed or given a new meaning. Everything it does not
name public is internal and may change in any release.

The freeze started at v1.0.0-beta.1. From then on nothing public is renamed,
reshaped or removed; a beta or a release candidate only fixes, adds tests and
docs, or promotes a feature to shipped. The renames 1.0 made came in the
alphas. Each is a row of `api/renames-1.0.tsv`, and
[tehas update](/tools-update#tehas-update) lists every one an instance still
uses.

The rules 1.x keeps are in [the rules for 1.x](#the-rules-for-1x).

## Public and internal

Public:

- the `tehas` commands, their flags and their [exit statuses](#exit-statuses).
  `tehasbot refresh --pr N` is kept as the old name of `tehas refresh --pr N`
  through 1.x;
- the settings of `api/settings-rules.json`, by the [knob rule](#settings-and-the-knob-rule);
- the labels of `host/labels.tsv`, and what the dispatcher does with each;
- the `--json` objects that have a schema in `api/` (`api/<name>.schema.json`),
  and `tehas api --json`;
- the tools of [the MCP server](/mcp-server#the-tools), their arguments and
  their answers;
- the files `~/.config/tehas/env` and the instance directory's layout;
  `TEHAS_MANAGER_VERSION` in `~/.config/tehas/env` is the version of the
  program that manages the factory, written by that program and shown by
  health;
- the update from any 0.30.x, 0.31.x or 0.32.x release by `tehas update`,
  with the state store and analytics migrated;
- [the cycle inputs rule](#cycle-inputs);
- the host namespace `/host/` and [the host bridge](/api#the-host-bridge);
- [analytics.db at schema 4](#analyticsdb).

[The pull request spec block](#the-pull-request-spec-block) is defined
below as part of `spec-pipe`, a feature still being built: it becomes public
with the release that ships that feature.

Internal:

- `tehasbot`, the units' command, but for the old name above;
- the `console` program and its subcommands;
- `store`, `attach`, `analytics`, `tehas-dispatch`, `tehas-cycle`, `tehas-mcp`
  and `tehas-proxy` as programs, and their verbs and statuses;
- everything under `~/tehas/live/`, `~/tehas/state/` and `~/tehas/logs/`. The
  dashboard reads `live/`; a program asks a command;
- the environment names the settings load keeps for the units and the tests
  other than the public ones above, and every `TEHAS_*_CMD` and `TEHAS_*_WAIT`
  test hook;
- `install.sh` and `bootstrap.sh` and their statuses: `tehas update` and
  `tehas setup` are the commands;
- the `Forge` trait of [the forge page](/forge): tehas 1.0 promises no forge
  interface;
- `tehas top --json`, a snapshot for the screen;
- the flags of `tehas plan`, which the dispatcher writes for a planning turn,
  `tehas index --before-cycle`, the dispatcher's, and `tehas disk --publish`,
  the cleaner's. `api/commands.txt` marks them and `tehas top --json`
  `internal`;
- every feature still being built, below.

## What is frozen and how

| Surface | Frozen | What holds it |
| --- | --- | --- |
| Settings | Each row of `api/settings-rules.json`: its name, its meaning, and what a missing or malformed value does (exit 78) | `test/settings_rules_test.sh` |
| Switch grammar | Every switch takes `on`, `off`, `1` and `0`. The manual writes `on` and `off` | The Rust tests of the settings rules |
| Commands | The `tehas` commands, their subcommands and the flags of each, but the flags marked `internal` | `api/commands.txt`, held to the commands tehas knows and to the synopsis lines of each command's text by the tests of `console/src/front.rs` |
| Exit statuses | One meaning per status, and [one table per command](#exit-statuses) | `console/core/src/exit.rs`, the shell tests of each command, and `console/tests/surface.rs`, which fails when a public command of `api/commands.txt` has no table below |
| Labels | The rows of `host/labels.tsv`: the name and what the dispatcher does with it | The dispatcher's and the tidy's tests |
| Store | The store opens any store from schema 12 (v0.30.0) and brings it to its own schema. An older store is refused with a message that names v0.30 | The store's migration tests |
| analytics.db | `user_version` 4, tables and columns only added | `console/analytics/src/schema.pinned` |
| Factory API | `tehas api --json`: `api`, `features` (words only added), `flags`, `millwright`, `poll` | `api/api.schema.json` and its samples, and `api/features.txt`, the words of `features` in order, held by `test/api_test.sh` |
| `--json` objects | Each object with a schema and samples in `api/`. Fields are only added | `test/json_contract_test.sh`, which lists every schema in `api/` and the test that holds each one's samples to what its command prints, and the tests of `console/src/front.rs`, which fail when a public `--json` row of `api/commands.txt` has no schema |
| MCP | The tools, their input and output schemas | The MCP server's tests: every answer they read is checked against its tool's output schema, and the cycle status enum against the core's vocabulary |
| `tehas update` | From any 0.30.x, 0.31.x or 0.32.x release. A 0.30.x host can name an rc or the release, not an alpha or a beta. From 1.0 on, the release repository and the signing key are built into tehas: the `TEHAS_RELEASE_REPO` line of `host/versions.env` and `host/release/minisign.pub` when the release was built. An instance may name another repository with `TEHAS_RELEASE_REPO` in `instance.env`; the key stays the built-in one | The update's shell tests, and the tests of `console/core/src/release.rs` |
| Cycle inputs | [The rule below](#cycle-inputs), with the names as pi-rukas has them while it is 0.x; a renamed name is taken under both names through 1.x | `test/work_test.sh` |
| Host namespace | `/host/` on the factory's origin belongs to the host that fronts the factory's domain: tehas never serves a path under it, and its docs server answers 404 there, which the dashboard reads as no host. The host bridge, `/host/bridge.json`, is `bridge: 1`, by `api/host-bridge.schema.json` and [its rules](/api#the-host-bridge) | `test/host_bridge_test.sh` (the 404 under `/host/`, the schema and its samples), `test/json_contract_test.sh`, `test/host_bridge_test.mjs` (what the dashboard keeps of an answer) and `test/panels_dock_test.mjs` (a menu item opens in the dock, and a page item in a new tab with no dock) |

`api/surface-1.0.txt` lists the names of the frozen surface in one place:
each public command, subcommand and flag, setting, role, label and `--json`
field, read from the files in the table above. Lines are only added.
`console/tests/surface.rs` fails when one of its names goes, and when the
surface gains a name the list does not have yet. What a building feature
adds is not in it. A name that was on the list before its feature became
building stays on it: the list is only added to. Such a name is refused
while the feature is building, as every building feature's name is
([review](#review)).

In `tehas status --json` the fields of `running[].progress` and `prototypes`
describe one driver's steps and the Protolab, and may change. The fields of
`contractors` may change while outsource is still being built, and so may
`api/status.outsourced.json`, the sample that shows them.

`testFeatures` in `tehas status --json` stays, with `read`, `features` and
`lab`, through 1.x. It has been on the surface since 1.0.0-beta.7. It reports
a lab build, and that a release reads no `TEHAS_TEST_FEATURES`, so a person
can tell a lab host from a release host. A release prints it only while the
variable is set. If 1.x replaces the lab mechanism, the field and its
subfields keep their meaning.

The keys of `rows` in `tehas state check --json` are the store's table names.
They are internal: a later release may add, rename or drop a table, and with it
a key. `rows` itself is public.

## The rules for 1.x

Decided for 1.0 on 2026-11-18.

- **Readers.** A program that reads a `--json` object, `tehas api --json`,
  `api/settings-rules.json` or an MCP answer ignores a field it does not know.
  A schema in `api/` describes the release that ships it: a later 1.x release
  may add to it.
- **What 1.x may add.** Within 1.x a field, an enum value or an exit status
  may be added. An existing field, value or exit status keeps its meaning and
  is never removed. A reader treats an enum value it does not know as
  unknown, not as an error.
- **Building features.** A feature still being built is outside the contract:
  its rows in `api/settings-rules.json` (each marked `"building": true`), its
  key in the `flags` object of `tehas api --json`, its `--json` fields (such
  as `contractors` in `tehas status --json` and `api/status.outsourced.json`),
  its commands and its MCP tools may change or go until the release that ships
  it. Only a shipped feature's key is guaranteed in `flags`, and
  `api/api.schema.json` requires only those. A name that was on `api/surface-1.0.txt`
  before its feature became building stays on it and keeps its meaning,
  though its row is marked `"building": true` and the release refuses it:
  `TEHAS_REVIEW` and `TEHAS_REVIEW_REPO` of [review](#review).
- **The building features of 1.0 come in 1.1.** outsource, spec-pipe,
  protolab-zip-url, prototype-file, lean and review are planned for 1.1
  (decided 2026-11-19). The names [listed for each below](#names-reserved-for-11) are
  the ones 1.1 adds to the frozen surface: no other feature takes them, and
  1.1 ships each under the same name. Their shapes may still be settled
  before 1.1; once shipped, they are only added to.
- **MCP protocol revisions.** The revisions [the MCP server](/mcp-server)
  accepts at 1.0, 2026-07-28 and 2025-11-25, stay accepted through 1.x. Newer
  ones may be added.
- **pi-rukas names.** When pi-rukas renames a `PI_ENSEMBLE_*` input, tehas
  takes both the old and the new name through 1.x.
- **Driver labels and settings.** The labels `tehas:lean` and `tehas:classic`
  and `TEHAS_DRIVER=lean` stay accepted through 1.x, whatever lean becomes.
- **Update sources.** `tehas update` reaches 1.0 from 0.30.x, 0.31.x and
  0.32.x. A 0.30.x host can name only an rc or the release with `--to`, not
  an alpha or a beta: that is what v0.30.6 accepts as a tag.
- **Release repository and key.** From 1.0 the update reads releases from the
  repository built into tehas and checks them with the key built into tehas,
  as [tehas update](/tools-update#from-a-release) says.

## Settings and the knob rule

A ruled setting keeps its name and its meaning through 1.x, and a missing or
malformed value always stops the command with exit 78 before it calls GitHub,
Docker or git. A setting is only added in 1.x.

A tuning knob is a setting that sizes or times something: seconds, bytes,
counts, retries, such as `TEHAS_WORK_TIMEOUT` or `TEHAS_MAX_AWAITING`. Its
default may change in a minor release, and the release notes say so. Its name,
its meaning and the values it takes do not change. A setting that turns
something on or off, names a repository, an account or a path, is no knob:
its default stays.

A `TEHAS_*` name the rules do not hold is internal, and an instance that sets
one is not covered by this page.

## Cycle inputs

What a cycle's container is given, as `tehas work` gives it:

- **Variables.** Every `PI_ENSEMBLE_*` variable the host's settings set reaches
  the cycle, but for the settings of pi-rukas's own docker wrapper
  (`PI_ENSEMBLE_IMAGE`, `PI_ENSEMBLE_HOST_ALIASES`,
  `PI_ENSEMBLE_NO_DOCKER_SOCKET`, `PI_ENSEMBLE_NO_SSH`) and the names a cycle
  gives itself whatever the instance says: `PI_ENSEMBLE_EXTRA_IMAGE_DIRS`,
  `PI_ENSEMBLE_HANDOFF_CHANNEL`, `PI_ENSEMBLE_HANDOFF_TIMEOUT_MS`,
  `PI_ENSEMBLE_CYCLE_DEADLINE`, `PI_ENSEMBLE_CYCLE_BUDGET_MS`,
  `PI_ENSEMBLE_VERIFY_CONCURRENCY` and `PI_ENSEMBLE_SPAWN_TIMEOUT_MS`
  (`CYCLE_LEAVES_OUT` in `console/core/src/sandbox.rs`). Nothing else of the
  host's environment enters.
- **Capabilities.** An input the image does not list in its `--capabilities`
  answer refuses the cycle with exit 78 and says why in
  `state/cycles/<issue>.refused`.
- **Mounts.** Exactly these ten, and no Docker socket, no SSH key and nothing
  of `~/.config/tehas`:

| Host | In the container |
| --- | --- |
| The cycle's workspace, `~/tehas/work/<name>-<issue>` | Its path inside, read-write |
| The cycle's transcripts, `~/.pi/agent/ensemble-runs/issue-<issue>` | `/home/vscode/.pi/agent/ensemble-runs` |
| `<workspace>.state/cache` | `/home/vscode/.tehas-cache` |
| `<workspace>.state/memory` | `/home/vscode/.vipune` |
| `<workspace>.state/sessions` | `/home/vscode/.pi/agent/sessions` |
| `~/.pi/agent/mcp.json` | `/home/vscode/.pi/agent/mcp.json`, read-only |
| `<workspace>.state/models.json` | `/home/vscode/.pi/agent/models.json`, read-only |
| `~/.pi/agent/settings.json` | `/home/vscode/.pi/agent/settings.json`, read-only |
| `~/tehas/state/github-cycle` | `/run/tehas/github`, read-only |
| `<workspace>.state/relay` | `/run/tehas/relay` |

The `PI_ENSEMBLE_*` names, and what each means, follow pi-rukas while pi-rukas
is 0.x: a name pi-rukas renames is taken under the new name in the release
that follows it, the old name stays accepted through 1.x, and the release
notes say so. The rule itself, which names pass and what
is mounted, is frozen. `test/work_test.sh` pins the mounts and the variables,
and a change to them is a security change.

## analytics.db

`~/tehas/state/analytics.db` is at `user_version` 4. Within it tables and
columns are only added, never renamed, retyped or removed, and a file of
schema 4 that a 0.30.x release wrote opens and gains what was added. A new
version comes only with a major release. [Analytics](/analytics) has the
schema; [`tehas analytics copy`](/api#tehas-analytics-copy) is how a program
takes a consistent copy.

## The pull request spec block

Part of `spec-pipe`, a feature still being built, so it is not part of the
frozen surface until the release that ships it. A pull request built from an
approved spec answers it with this block in its body, version 1:

```
<!-- tehas:spec v1 -->
Spec: <link> sha256:<64 lower-case hex>
- <id> met: <where>
- <id> not met: <why>
<!-- /tehas:spec -->
```

For example:

```
<!-- tehas:spec v1 -->
Spec: https://github.com/user-attachments/files/123/spec.md sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
- R1 met: src/search.rs, search() with tests in tests/search.rs
- R2 not met: the export needs an API the backend does not have yet
<!-- /tehas:spec -->
```

The rules (`spec_block` in `console/core/src/grammar.rs`):

- The open line is exactly `<!-- tehas:spec v1 -->` and the close line exactly
  `<!-- /tehas:spec -->`, each on a line of its own. Spaces around a line do
  not count, and LF and CRLF line ends are both read.
- Only lines outside fenced code blocks count, as for the issue's `Spec:` line.
  A block quoted inside a fence is text.
- The first line inside that is not blank is the `Spec:` line, in the form of
  the issue's: the link of the spec, not its file name, and the sha256 of the
  manifest's `spec` entry.
- One or more requirement lines follow, blank lines allowed between them:
  `- <id> met: <text>` or `- <id> not met: <text>`. A line splits at its first
  `:`. What comes before must end in ` not met` or ` met`, and what precedes
  that is the id. The id is not empty and holds no `:` and no backtick. The
  text is not empty.
- The id is the spec's own requirement id, such as `R1` or `FR-3`. A spec
  with no ids uses the text of each top heading instead, without the `#`s and
  any `:`.

A body with no block has none. A body tehas cannot read: no close line, a
second block, an open line of another version (named) or one that starts
`<!-- tehas:spec v` and is not exactly the open line, a missing or malformed
`Spec:` line, a line inside of neither form, no requirement line, or an id
named twice. The manifest's `spec` entry says in `block` which version the
cycle writes.

What tehas checks (`spec_answered`, and the cycle's report): with the feature
on, a start not run by hand whose approval pins a spec, a pull request waits
only for a merge when its body holds one block tehas can read and the block's
`Spec:` line names the sha256 of that pin. The link is not compared. A body
with no block, a block of another sha256 or one tehas cannot read parks the
issue with `needs-human-attention` and the park reason `spec-block`; so does an
approval that cannot be read. The pull request stays open and gets no
comment.

The lines are for the person who reviews. This block is not lean's `pi-spec v1`
block, which goes in an issue body.

## Features still being built

A feature is `building` or `shipped` in a release (`console/core/src/features.rs`).
Only a shipped feature can be on. A building feature leaves nothing in the
frozen surface ([the rules for 1.x](#the-rules-for-1x)): its switch set to `on`, or a line that gives one of its own
settings a value, stops every command with exit 78 and names the feature, it
adds no command, no label, no `--json` field and no MCP
tool, and its settings, its key in `flags`, its commands, fields and MCP tools
are not contract until the release that ships it, except a name that was
public before the feature became building, which stays on
`api/surface-1.0.txt` ([review](#review)). `tehas api --json` lists
every feature in `flags`, `false` while it is building, and a shipped
feature's word in `features`. `api/settings-rules.json` marks the rows of a
building feature `"building": true`. Promoting a
feature to shipped only adds. A lab host proves a building feature on a build
that reads `TEHAS_TEST_FEATURES`, which no release does: see [proving a
building feature on a lab host](/tools-update#proving-a-building-feature-on-a-lab-host).

### Names reserved for 1.1

Each building feature of 1.0 is planned for 1.1, and these are the names it
adds when it ships. The names marked frozen are in `api/surface-1.0.txt`
already and keep their meaning in 1.0.

| Feature | Settings | Commands | Fields, tools and text | Labels | Programs and units |
| --- | --- | --- | --- | --- | --- |
| outsource | `TEHAS_OUTSOURCE` and `TEHAS_OUTSOURCE_TIMEOUT`, `_GRACE`, `_RETRIES`, `_REST`, `_REPOS`, `_CONTRACTORS`, `_JOIN_TTL`, `_PROXY_ADDR`, `_PROXY_IDLE` | `tehas outsource invite [--name NAME]`, `list`, `status`, `revoke <contractor>`, `drain <contractor>`, each with `--json` | `contractors` in `tehas status --json` (`api/status.outsourced.json`), the MCP tool `factory_contractors`, the job channel `/outsource/v1/<call>`, the column `runs.contractor_id` of `analytics.db` (frozen) | none | `tehas-proxy`, `tehas-contractor`, `tehas-outsource-proxy.service` |
| spec-pipe | `TEHAS_SPEC_PIPE` | none | the issue line `Spec: <link> sha256:<hex>`, [the pull request spec block](#the-pull-request-spec-block) version 1, the cycle's `spec` and `approved.json` and their manifest keys `spec` and `approvedText`, the runner capability `issueSpec` | none | none |
| protolab-zip-url | `TEHAS_PROTOLAB_ZIP_URL`, `TEHAS_PROTOLAB_ZIP_HOSTS` | none | the ticket line `Zip: <address> sha256:<hex>` | `tehas:prototype` (frozen) | none |
| prototype-file | `TEHAS_PROTOTYPE_FILE` | `tehas prototype file` with `--title`, `--body-file`, `--spec`, `--spec-sha256`, `--zip`, `--zip-sha256` | none | `tehas:prototype` (frozen) | none |
| lean | `TEHAS_LEAN`, and the word `lean` of `TEHAS_DRIVER` (frozen) | `tehas work --driver lean` (the flag is frozen) | the issue's `pi-spec v1` block, the `driver` columns of `analytics.db` (frozen) | `tehas:lean`, `tehas:classic` (frozen) | none |
| review | `TEHAS_REVIEW`, `TEHAS_REVIEW_REPO` (both frozen) | none | none | `tehas:improvement` (frozen) | `tehas-review.service`, `tehas-review.timer` |

Each feature's name is its key in `flags` and the word 1.1 appends to
`features`. None of them adds a `/health` field.

What each registered feature leaves visible while its flag is off:

### outsource

Building, planned for 1.1. Off by default; `TEHAS_OUTSOURCE=on` stops every command with exit
78 and says `outsource` is still being built.

- `tehas api --json` has `"outsource": false` in `flags`, and no word
  `outsource` in `features`.
- The settings `TEHAS_OUTSOURCE_*` are in `api/settings-rules.json`, but they
  are not contract: a line that gives one a value stops every command with
  exit 78 and names `outsource`.
- `tehas-proxy` ends at once with exit 0 and opens no port.
- No command, no label, no `contractors` in `tehas status --json`, no MCP
  tool, nothing in `live/`.
- The state store has its tables for contractors and jobs, which are internal.
  `tehas state check --json` counts them under `rows`, whose keys are the
  store's table names and internal.
- `analytics.db` has the column `contractor_id` of `runs`, the name of the
  contractor that ran the cycle, NULL on every run while outsource is off. It
  is part of schema 4, which is frozen: 1.1 keeps its name, its type and its
  meaning.

[Outsource](/outsource) says what the feature does once it ships.

### spec-pipe

Building, planned for 1.1. Off by default; `TEHAS_SPEC_PIPE=on` stops every command with exit
78 and says `spec-pipe` is still being built.

- `tehas api --json` has `"spec-pipe": false` in `flags`, and no word
  `spec-pipe` in `features`.
- The poll asks an issue's body nothing more: a `Spec:` line is text, and a
  body with one starts as any other.
- A start fetches no spec and its approval keeps no pin. The approval record
  in the state store has room for one, which is internal.
- A cycle is given no `spec` and no `approved.json`, and its manifest names
  neither.
- No pull request is checked for a spec block: a handoff waits for a merge as
  before.
- No command, no label, no `--json` field, no MCP tool.

[tehas work](/tools-work) says what the feature gives a cycle once it ships.

### protolab-zip-url

Building, planned for 1.1. Off by default; `TEHAS_PROTOLAB_ZIP_URL=on` stops every command with
exit 78 and says `protolab-zip-url` is still being built.

- `tehas api --json` has `"protolab-zip-url": false` in `flags`, and no word
  `protolab-zip-url` in `features`.
- The setting `TEHAS_PROTOLAB_ZIP_HOSTS` is in `api/settings-rules.json`, but
  it is not contract: a line that gives it a value stops every command with
  exit 78 and names `protolab-zip-url`.
- A `Zip:` line in a ticket is text on the Protolab page, as it always was.
- No command, no label, no `--json` field, no MCP tool.

[A zip by address](/tools-protolab#a-zip-by-address) says what the feature
does once it ships.

### prototype-file

Building, planned for 1.1. Off by default; `TEHAS_PROTOTYPE_FILE=on` stops every command with
exit 78 and says `prototype-file` is still being built.

- `tehas api --json` has `"prototype-file": false` in `flags`, and no word
  `prototype-file` in `features`.
- `tehas prototype file` exits 78 and files nothing. `tehas help prototype`
  prints its text, and the list of `tehas help` does not name it.
- No label, no `--json` field, no MCP tool.

[`tehas prototype file`](/tools-protolab#tehas-prototype-file) says what the
feature does once it ships.

### lean

Building, planned for 1.1. Off by default; `TEHAS_LEAN=on` stops every command with exit 78
and says `lean` is still being built, and so does `TEHAS_DRIVER=lean`, which
names the feature it needs.

- `tehas api --json` has `"lean": false` in `flags`, and no word `lean` in
  `features`.
- `TEHAS_DRIVER` takes `classic`; every cycle runs the classic driver.
- `tehas work --driver lean` exits 78 and starts no cycle. `--driver classic`
  works as before.
- The labels `tehas:lean` and `tehas:classic` stay in the label set, so
  `tehas labels` does not remove them, and they and `TEHAS_DRIVER=lean` stay
  accepted through 1.x whatever lean becomes. `tehas:lean` is inert: a start that
  would run lean starts nothing, the label that asked comes off and the issue
  says once that lean is still being built. Classic never runs in its place.
  `tehas:classic` and an issue with no driver label run classic.
- An approval record that names driver `lean`, kept from before, is refused
  with 78 by `tehas work`; it does not run as classic.
- No command, no `--json` field, no MCP tool.

[Choosing the driver](/operating#choosing-the-driver) says what the feature
does once it ships.

### review

Building since v1.0.0-rc.1, planned for 1.1. It shipped as a setting before the registry, but
no run was seen on a host, so it became building (decided 2026-11-19). Off by
default; `TEHAS_REVIEW=on` stops every command with exit 78 and says `review`
is still being built.

- `tehas api --json` has `"review": false` in `flags`, and no word `review`
  in `features`.
- The settings `TEHAS_REVIEW` and `TEHAS_REVIEW_REPO` were public before
  review became building, so they stay on `api/surface-1.0.txt` and keep
  their meaning. While review is building they are refused: a line that
  gives `TEHAS_REVIEW_REPO` a value stops every command with exit 78 and
  names `review`.
- `tehasbot review` exits 0 at once and calls nothing.
- `install.sh` does not install `tehas-review.service` or
  `tehas-review.timer`, does not enable the timer and an update does not
  restart it. A host that had them from an earlier release keeps the files,
  and `install.sh` stops and disables the timer if it is enabled.
- No command, no label, no `--json` field, no MCP tool.

[The review loop](/operating#the-review-loop) says what the feature does once
it ships.

## Exit statuses

A status means the same in every public command, but for the exceptions a
command's table below names:

| Status | Meaning |
| --- | --- |
| 0 | done |
| 1 | failed: a call, a read or a write did not succeed |
| 3 | the command worked and the answer is no, or not yet |
| 64 | a bad argument |
| 78 | settings missing or malformed, or a feature that is off |
| 141 | standard output closed before the command finished printing |

The exceptions: a command whose work is a session or a cycle (`tehas work`,
`tehas plan`, `tehas dev`) passes on that run's own status and has statuses of
its own, listed with the command, and `tehas mcp` exits 0 when `TEHAS_MCP` is
off, so that the unit that runs it stays stopped rather than failed. Each command's tables below say
which of these it uses and when. The constants are in
`console/core/src/exit.rs`. Every command exits 141, with nothing on standard
error, when its reader goes away, as in `tehas status | head -1`.

### tehas status

| Status | When |
| --- | --- |
| 0 | The factory is taking work or is paused (`pass`, `warn`) |
| 3 | It is not (`fail`), the health file is older than ten minutes, or there is no health file yet |
| 64 | An argument other than `--json` |

### tehas api

| Status | When |
| --- | --- |
| 0 | Printed |
| 64 | An argument it does not know |
| 78 | The settings are missing or malformed |

### tehas disk

| Status | When |
| --- | --- |
| 0 | Printed, and with `--publish` written |
| 1 | The published file could not be written |
| 64 | A bad argument |
| 78 | The settings are missing or malformed |

### tehas top

| Status | When |
| --- | --- |
| 0 | Ended by `q` or a signal, or one frame or one snapshot printed |
| 1 | There is no live directory at all |
| 64 | An unknown argument, or a `--every` that is not a number from 1 to 64 |

### tehas history

| Status | When |
| --- | --- |
| 0 | Printed, also with nothing to list |
| 1 | The state store did not answer (busy); nothing is printed |
| 64 | An argument it does not know |
| 78 | The settings are missing or malformed, or the state store is not installed or cannot be used |

### tehas export

| Status | When |
| --- | --- |
| 0 | Copied, also with nothing to copy |
| 1 | The state store did not answer, which copies nothing, or a cycle could not be written, which the others are copied without |
| 64 | A bad argument, an export into the factory's own home, or a directory that cannot be made |
| 78 | The settings are missing or malformed, the file of models cannot be read, or the state store is not installed or cannot be used |

### tehas state

| Status | When |
| --- | --- |
| 0 | Done |
| 1 | Refused or failed: a store that does not check, a running factory for `pack` or `load`, a copy whose name is taken |
| 64 | A bad argument |
| 78 | The settings, the release or the state store are missing or malformed |

### tehas analytics

| Status | When |
| --- | --- |
| 0 | Printed or written |
| 1 | The analytics program is not installed, the database cannot be read, or `<file>` exists or cannot be made |
| 64 | A bad argument |
| 78 | The settings are missing or malformed, or `TEHAS_ANALYTICS=off`; nothing is written |

### tehas limit

| Status | When |
| --- | --- |
| 0 | Shown, set or cleared |
| 1 | The limit file could not be written or removed; the limit is as it was |
| 64 | A bad argument; nothing is written |
| 78 | The settings are missing or malformed; nothing is written |

### tehas order

| Status | When |
| --- | --- |
| 0 | Shown, set or cleared |
| 1 | The state store refused the change or did not answer |
| 64 | A bad argument; nothing is written |
| 78 | The settings are missing or malformed, or the state store is not installed or cannot be used; nothing is written |

### tehas start

| Status | When |
| --- | --- |
| 0 | Approved, withdrawn, or nothing to withdraw |
| 1 | GitHub did not answer, or the state store refused or is not installed |
| 3 | GitHub has no open issue of that number: none, a pull request or a closed issue. Nothing is recorded |
| 64 | A bad argument; nothing is written |
| 78 | The settings are missing or malformed; nothing is written |

### tehas refresh

| Status | When |
| --- | --- |
| 0 | The request is written |
| 1 | The read of GitHub failed, took more than 20 seconds or named no pull request, which writes nothing, or the request could not be written |
| 3 | It is not a pull request the factory refreshes, or it does not conflict; nothing is written |
| 64 | No `--pr`, or N is not a whole number above 0 |
| 78 | The settings are missing or malformed, or `tehas-dispatch` is not installed or cannot run |

### tehas drain

| Status | When |
| --- | --- |
| 0 | No cycle is running |
| 1 | The limit could not be written; nothing is printed on standard output and nothing is waited for |
| 3 | Some still are at the end of the wait, or a poll held its lock for the whole of it. The answer is printed |
| 64 | A bad argument; nothing is written |
| 78 | The settings are missing or malformed |

### tehas freeze and tehas thaw

| Status | When |
| --- | --- |
| 0 | The freeze or the thaw is asked, or there is nothing to do |
| 1 | The request could not be written or removed |
| 64 | A bad argument |
| 78 | The settings are missing or malformed |

### tehas pause

| Status | When |
| --- | --- |
| 0 | The host is paused |
| 1 | The flag could not be written; the host is not paused by this command |
| 64 | A bad argument, a reason without `--reason` among them |
| 78 | The settings are missing or malformed, or there is no console to run |

### tehas resume

| Status | When |
| --- | --- |
| 0 | The flag is gone, removed or never there |
| 1 | A name of the flag could not be removed, or is not this account's to remove |
| 64 | Any argument |
| 78 | The settings are missing or malformed, or there is no console to run |

### tehas stop

| Status | When |
| --- | --- |
| 0 | The cycle is stopped and its leftovers are gone, or there was nothing to do |
| 1 | The cycle still holds its lock, a stop or a removal failed, the label could not be read or taken off, or the state store could not be read or written |
| 64 | No issue number, one that is not a positive number, or an argument it does not know |
| 78 | The settings are missing or malformed, or the state store is not installed or cannot be used |

### tehas panel and tehas host-message

| Status | When |
| --- | --- |
| 0 | Done |
| 1 | The list could not be read or written, or another run held the lock |
| 64 | A bad argument, one item over the limit, or for a panel a url on `TEHAS_DOMAIN`; nothing is written |
| 78 | The settings are missing or malformed; nothing is written |

### tehas dispatch

| Status | When |
| --- | --- |
| 0 | The poll ran, or there was nothing to do, or `--soon` wrote its file |
| 1 | A call to GitHub failed or could not be read, the state store could not be read, systemd cannot load the cycle unit, a cycle could not be started, or `--soon` could not write its file |
| 64 | An argument it does not know; no poll |
| 78 | The settings are missing or malformed, or the dispatcher or the state store is not installed or cannot be used |

### tehas wake

| Status | When |
| --- | --- |
| 0 | The poll is asked for |
| 1 | `state/dispatch.again` could not be written |
| 64 | An argument other than `--json`; nothing is written |
| 78 | The settings are missing or malformed; nothing is written |

### tehas work

`tehas work` passes on the runner's own status when the runner ran and wrote
its result, and has statuses of its own beside them. Here 3 is tehas's own
"the sandbox cannot push", not the common "no".

| Status | When |
| --- | --- |
| 0, 10, 11, 20, 21, 30, 40 | The runner's: merged, handoff, handoff for clarification, aborted, interrupted, crash or its own time limit, refused to start |
| 2 | The cycle was stopped as stalled or at the time limit, or the runner ended without a result |
| 3 | The sandbox cannot push to the repository; no cycle was started |
| 4 | No cycle was started: the approval, the issue's text or an attachment did not hold |
| 5 | A cycle of the issue is running already; nothing was touched |
| 64 | Not an issue number, a bad option, no bot token or no git identity |
| 78 | The settings are invalid, or the host or the image cannot run the cycle; no cycle was started |
| 137 | The container was killed while the cycle ran |

### tehas plan

| Status | When |
| --- | --- |
| 0 | The turn ended with something to say or ask, or it filed |
| 2 | The runner failed, ran out of time, refused the turn, said and asked nothing, or its outcome could not be written down |
| 4 | An image or file the issue links could not be fetched; nothing ran |
| 64 | A bad argument, no bot token or no git identity; nothing ran |
| 78 | The settings are invalid, or the host or the image cannot run a planning turn; nothing ran |

### tehas dev

| Status | When |
| --- | --- |
| any | The session's own, as Docker reports it |
| 4 | With `--issue`: an image or file the issue links could not be fetched; no container |
| 64 | A bad argument, no bot token or no git identity; no container |
| 78 | The settings are invalid or the host cannot run the session; no container |

### tehas index

| Status | When |
| --- | --- |
| 0 | The index was built, or was already current |
| 1 | The sandbox image is missing, or the indexer failed or did not finish in time |
| 3 | Not yet: a cycle is running, another index run holds its lock, or a poll held the dispatcher's lock for the whole wait; nothing was done |
| 64 | An argument it does not know |
| 78 | The settings are invalid, the checkout is no clone of `TEHAS_REPO`, or the host cannot run it; nothing was done |

### tehas setup

| Status | When |
| --- | --- |
| 0 | Done |
| 1 | A check or a step failed, a product check among them |
| 64 | An unknown option, or app options that do not go together |
| 78 | A setting is missing, malformed or differs without `--force`, a step stopped on its settings (78 from `install.sh`), or there is no console and no checkout to build one |

### tehas update

| Status | When |
| --- | --- |
| 0 | Updated, or already up to date |
| 1 | The update failed: another update holds the lock, the checkout or the release cannot be moved, a step or `install.sh` failed or timed out, systemd did not reload the units, the image could not be moved; or `--check` found something an update does not fix (the host's tools, the slice, root's copy of the command, the tests), or the sandbox build's own check failed |
| 3 | `--check` found something to update and nothing an update does not fix: a checkout behind, a release to switch to, files that differ, steps left over, or a pi-rukas commit the image is not on |
| 64 | A bad argument or a combination it refuses |
| 78 | The settings are missing or malformed (a setting tehas 1.0 renamed or removed among them), a login cannot be resolved or its id changed, or there is no console or no platform to update from |

### tehas labels

| Status | When |
| --- | --- |
| 0 | Every label is there and every granting label is pinned |
| 1 | A label could not be made, or a granting label could not be pinned or is not the pinned one |
| 64 | Any argument |
| 78 | The settings are missing or malformed |

### tehas check-product

| Status | When |
| --- | --- |
| 0 | Every check passed |
| 1 | The sandbox could not be run |
| 3 | A check failed; each failure is a `FAIL` line |
| 64 | A bad argument |
| 78 | The settings are missing or malformed |

### tehas init-product

| Status | When |
| --- | --- |
| 0 | The pull request is open, or nothing is missing |
| 1 | git or gh failed, or the branch is already on origin |
| 64 | A bad argument |
| 78 | The settings are missing or malformed, or the checkout is not a clone of `TEHAS_REPO` |

### tehas mcp

| Status | When |
| --- | --- |
| 0 | The server ended, or it is off, or a key was printed |
| 1 | The binary or the image is missing, a network named `tehas-mcp` lets the container out, or `~/tehas/mcp` cannot be made |
| 64 | An unknown argument |
| 78 | The settings, `TEHAS_MCP` or the lists of keys are malformed |

### tehas github check and tehas github writes

| Status | When |
| --- | --- |
| 0 | The check passed, or the writes were printed |
| 1 | No token could be had, it does not reach `TEHAS_REPO`, or no home is set |
| 64 | An argument it does not know |
| 78 | Settings that are missing, malformed or name no app |

### tehas help

| Status | When |
| --- | --- |
| 0 | Printed |
| 64 | A command it does not know |
