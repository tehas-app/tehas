---
title: The contract
description: "What of tehas is public and stays compatible through every 2.x release, what is internal and may change, how a program starts work for a person, what changed in 2.0 and for whom, and the exit statuses of every public command."
---

# The contract

This page says what a person or a program may rely on in tehas 2.x. What it
names public stays compatible through every 2.x release: within 2.x a public
name is only added, never renamed, removed or given a new meaning. Everything
it does not name public is internal and may change in any release.

2.0 is a major release. What it changed is listed once, in
[Changed in 2.0](#changed-in-20), each change with its readers and what an
adopter does. What is still to land before 2.0 is tagged is in
[Pending for 2.0](#pending-for-20), and moves to the first list as it lands.

## Public and internal

Public:

- the `tehas` commands, their flags and their [exit statuses](#exit-statuses);
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
- [the upgrade to 2.0](#upgrading-to-20) by `tehas update`, with the state
  store and analytics migrated;
- [the cycle inputs rule](#cycle-inputs);
- the host namespace `/host/` and [the host bridge](/api#the-host-bridge);
- [analytics.db at schema 6](#analyticsdb);
- [the commands of other parts](#the-commands-of-other-parts): the rule by
  which `tehas <part>` runs `tehas-<part>`, the words kept for the parts,
  and the front asset of a release.

Internal:

- `tehasbot`, the units' command, every verb of it;
- the `console` program and its subcommands;
- `store`, `attach`, `analytics`, `tehas-dispatch`, `tehas-cycle` and
  `tehas-mcp` as programs, and their verbs and statuses;
- everything under `~/tehas/live/`, `~/tehas/state/` and `~/tehas/logs/`. The
  dashboard reads `live/`; a program asks a command;
- the environment names the settings load keeps for the units and the tests
  other than the public ones above, and every `TEHAS_*_CMD` and `TEHAS_*_WAIT`
  test hook;
- `install.sh` and `bootstrap.sh` and their statuses: `tehas update` and
  `tehas setup` are the commands;
- the `Forge` trait of [the forge page](/forge): tehas 2.0 promises no forge
  interface;
- `tehas top --json`, a snapshot for the screen;
- the flags of `tehas plan`, which the dispatcher writes for a planning turn,
  `tehas index --before-cycle`, the dispatcher's, and `tehas disk --publish`,
  the cleaner's. `api/commands.txt` marks them and `tehas top --json`
  `internal`;
- every [feature still being built](#features-still-being-built).

## Starting work for a person

A program on the host starts work for a person with `tehas start <issue>`,
called for a person who is signed in to that program. In 2.0 it is the one
way; until #2894 lands, the
millwright's grant below is a second. The program
answers for who the person is and that they may approve work; tehas records
the approval as the host's (`how: host`), binds the issue's title and text,
and the next poll starts it under the rules every start passes. [tehas
start](/api#tehas-start) has the command and its object.

A person who is not working through such a program approves on the issue,
with `tehas:ready` from an account of `TEHAS_ALLOWED_LABELERS`. Authorisation
is who applied the label: the dispatcher reads the label events and compares
the account with the list.

A program answers a question an issue waits on for a person with
[tehas answer](/api#tehas-answer) `--by <login>`, which must be an account of
`TEHAS_ALLOWED_LABELERS`, and reads the question and its choices with
[tehas question](/api#tehas-question).

Today the millwright's own `tehas:ready` also starts work, by its grant: it
retries a cycle a person approved before (`TEHAS_MILLWRIGHT_MAX_RETRIES`)
and feeds an issue a person wrote (`TEHAS_MILLWRIGHT_FEED`). The grant is
coming out in 2.0 with #2894 (Pending). After it, the millwright's
`tehas:ready` starts only an epic's sub-issues under
`TEHAS_MILLWRIGHT_EPICS=on`.

## What is frozen and how

| Surface | Frozen | What holds it |
| --- | --- | --- |
| Settings | Each row of `api/settings-rules.json`: its name, its meaning, and what a missing or malformed value does (exit 78) | `test/settings_rules_test.sh` |
| Switch grammar | Every switch takes `on`, `off`, `1` and `0`. The manual writes `on` and `off` | The Rust tests of the settings rules |
| Commands | The `tehas` commands, their subcommands and the flags of each, but the flags marked `internal` | `api/commands.txt`, held to the commands tehas knows and to the synopsis lines of each command's text by the tests of `console/src/front.rs` |
| Exit statuses | One meaning per status, and [one table per command](#exit-statuses) | `console/core/src/exit.rs`, the tests of each command, and `console/tests/surface.rs`, which fails when a public command of `api/commands.txt` has no table below |
| Labels | The rows of `host/labels.tsv`: the name and what the dispatcher does with it | The dispatcher's and the tidy's tests |
| Store | `tehas update` brings a store of 1.4.x (schema 23) to 2.0's schema, 29 | The store's migration tests |
| analytics.db | `user_version` 6, tables and columns only added | `console/analytics/src/schema.pinned` |
| Factory API | `tehas api --json`: `api`, `features` (words only added), `flags`, `millwright`, `poll` | `api/api.schema.json` and its samples, and `api/features.txt`, the words of `features` in order, held by `test/api_test.sh` |
| `--json` objects | Each object with a schema and samples in `api/`. Fields are only added | `test/json_contract_test.sh`, which lists every schema in `api/` and the test that holds each one's samples to what its command prints, and the tests of `console/src/front.rs`, which fail when a public `--json` row of `api/commands.txt` has no schema |
| MCP | The tools, their input and output schemas | The MCP server's tests: every answer they read is checked against its tool's output schema, and the cycle status enum against the core's vocabulary |
| `tehas update` | From 1.4.x to 2.0 ([Upgrading to 2.0](#upgrading-to-20)). The release repository and the signing key are built into tehas: the `TEHAS_RELEASE_REPO` line of `host/versions.env` and `host/release/minisign.pub` when the release was built. An instance may name another repository with `TEHAS_RELEASE_REPO` in `instance.env`; the key stays the built-in one | The update's tests, and the tests of `console/core/src/release.rs` |
| Cycle inputs | [The rule below](#cycle-inputs), with the names as pi-rukas has them while it is 0.x; a renamed name is taken under both names through 2.x | `console/cycle/tests/work_container.rs` |
| Host namespace | `/host/` on the factory's origin belongs to the host that fronts the factory's domain: tehas never serves a path under it, and its docs server answers 404 there, which the dashboard reads as no host. The host bridge, `/host/bridge.json`, is `bridge: 1`, by `api/host-bridge.schema.json` and [its rules](/api#the-host-bridge) | `console/tests/host_bridge.rs` (the 404 under `/host/`, the schema and its samples), `test/json_contract_test.sh`, `test/host_bridge_test.mjs` (what the dashboard keeps of an answer) and `test/panels_dock_test.mjs` (a menu item opens in the dock, and a page item in a new tab with no dock) |

The surface list in `api/` names the frozen surface in one place: each
public command, subcommand and flag, setting, role, label and `--json`
field, read from the files in the table above. Within 2.x lines are only
added. `console/tests/surface.rs` fails when one of its names goes, and when
the surface gains a name the list does not have yet. What a building feature
adds is not in it.

In `tehas status --json` the fields of `running[].progress` describe one
driver's steps and may change.

`testFeatures` in `tehas status --json`, with `read`, `features` and `lab`,
reports a lab build, and that a release reads no `TEHAS_TEST_FEATURES`, so a
person can tell a lab host from a release host. A release prints it only
while the variable is set.

The keys of `rows` in `tehas state check --json` are the store's table names.
They are internal: a later release may add, rename or drop a table, and with it
a key. `rows` itself is public.

## The rules for 2.x

- **Readers.** A program that reads a `--json` object, `tehas api --json`,
  `api/settings-rules.json` or an MCP answer ignores a field it does not know.
  A schema in `api/` describes the release that ships it: a later 2.x release
  may add to it. Every object still carries `"api": 1`; what 2.0 took out of
  an object is listed in [Changed in 2.0](#changed-in-20).
- **What 2.x may add.** Within 2.x a field, an enum value or an exit status
  may be added. An existing field, value or exit status keeps its meaning and
  is never removed. A reader treats an enum value it does not know as
  unknown, not as an error.
- **Building features.** A feature still being built is outside the contract:
  its rows in `api/settings-rules.json` (each marked `"building": true`), its
  key in the `flags` object of `tehas api --json`, its `--json` fields, its
  commands and its MCP tools may change or go until the release that ships
  it. Only a shipped feature's key is guaranteed in `flags`, and
  `api/api.schema.json` requires only those.
- **MCP protocol revisions.** The revisions [the MCP server](/mcp-server)
  accepts at 2.0, 2026-07-28 and 2025-11-25, stay accepted through 2.x. Newer
  ones may be added.
- **pi-rukas names.** When pi-rukas renames a `PI_ENSEMBLE_*` input, tehas
  takes both the old and the new name through 2.x.
- **Driver labels and settings.** The labels `tehas:lean` and `tehas:classic`
  and the words `lean` and `classic` of `TEHAS_DRIVER` are kept, not
  renamed, through 2.x. Today `TEHAS_DRIVER=lean` stops with 78 and
  `tehas:lean` starts nothing, while lean is building.
- **No levers for older APIs.** 2.x keeps no alias, renamed command,
  deprecated flag or setting that brings back a behaviour of 1.x.
- **Release repository and key.** The update reads releases from the
  repository built into tehas and checks them with the key built into tehas,
  as [tehas update](/tools-update#from-a-release) says.

## Upgrading to 2.0

A host upgrades its factories and the programs that run them together.

A factory moves from 1.4.x to 2.0 with `tehas update`:

1. **Check the settings first.** 2.0 rewrites no setting. Each line of
   `instance.env` is kept, dropped with one warning, or refused with 78, as
   [what 2.0 does with a setting](#what-20-does-with-a-setting) lists.
   `cargo xtask config-check --settings <instance dir>`, run in a checkout
   of the 2.0 release, says which before anyone updates the host. Fix every
   refused line before the update. Every setting 2.0 refuses was already
   refused by 1.4.0: `RETIRED` holds the same twelve names, and
   `api/renames-1.0.tsv` is unchanged. Every setting that left
   `api/settings-rules.json` since 1.4.0 is dropped with a warning, not
   refused: twenty-two of its instance settings and two of its host settings,
   `TEHAS_ENERGY` and `TEHAS_ENERGY_URL`, which `~/.config/tehas/env`
   held. `api/settings-1.4.0.txt` lists every setting 1.4.0 accepted, 101
   instance and 10 host settings, and the test
   `every_setting_v1_4_0_accepted_is_kept_or_dropped_with_a_warning_never_refused`
   in `console/core/src/settings/load.rs` holds each to today's load: in
   its table or dropped with a warning, never refused. The names it drops
   are exactly those `IGNORED` holds.
2. **Run `tehas update`.** It stops and removes the units and files of the
   features 2.0 took out of core, and migrates the state store and analytics.
3. **The state store** (`state/tehas.db`) goes from schema 23, 1.4.x's, to
   29 in one run, step by step: 24 adds the answers of `tehas answer`, 25
   keeps a stop's question with its choices, 26 lets a cycle's `how` say
   `reply`, 27 rewrites every `retry-tick` to `reply` and marks a question
   open at the upgrade to be asked again, 28 rewrites `fixer` to
   `millwright` and drops `comment_links` from an approval's input, 29 drops
   the tables of outsource and keeps every other row.
   `tehas state check --json` prints the schema.
4. **Analytics** (`state/analytics.db`) goes from schema 4 to 6 the first
   time it is opened after the update: it drops `samples.watts` and
   `runs.contractor_id`, keeps every other column and row, and rewrites `retry-tick` to `reply` and `fixer` to
   `millwright` in `runs.how`.
5. **pi-rukas.** A checkout follows pi-rukas's 0.15 line
   (`PI_RUKAS_FOLLOW=release:0.15`). 2.0 needs pi-rukas v0.15.6 or later.

The update to 2.0 is supported from 1.4.x only. A host on an older release
updates to the newest 1.4.x first.

## Settings and the knob rule

A ruled setting keeps its name and its meaning through 2.x, and a missing or
malformed value always stops the command with exit 78 before it calls GitHub,
Docker or git. A setting is only added in 2.x.

A tuning knob is a setting that sizes or times something: seconds, bytes,
counts, retries, such as `TEHAS_WORK_TIMEOUT` or `TEHAS_MAX_AWAITING`. Its
default may change in a minor release, and the release notes say so. Its name,
its meaning and the values it takes do not change. A setting that turns
something on or off, names a repository, an account or a path, is no knob:
its default stays.

A `TEHAS_*` name the rules do not hold is internal, and an instance that sets
one is not covered by this page.

### What 2.0 does with a setting

2.0 rewrites no setting, in `instance.env` or in `~/.config/tehas/env`. Each
line is one of three:

- **Kept.** Every row of `api/settings-rules.json` not marked
  `"building": true` loads as in 1.4.x.
- **Dropped with a warning.** A setting of a feature 2.0 took out of core
  loads, every command warns of it once on standard error, and nothing reads
  it (`IGNORED` in `console/core/src/settings/load.rs`). A 1.4.x instance
  never fails to load for one. Take the line out when it suits you:
  - `TEHAS_REVIEW`, `TEHAS_REVIEW_REPO` (the review loop);
  - `TEHAS_ENERGY`, `TEHAS_ENERGY_URL`, `TEHAS_ENERGY_SOURCE_URL` (energy
    use);
  - `TEHAS_WIKI` (the product wiki);
  - `TEHAS_PROTOLAB_URL`, `TEHAS_PROTOLAB_ZIP_URL`, `TEHAS_PROTOLAB_ZIP_HOSTS`,
    `TEHAS_PROTOTYPE_FILE` (the Protolab);
  - `TEHAS_WORK_DESCRIBE`, `TEHAS_WORK_FILES`, `TEHAS_WORK_ZIPS`,
    `TEHAS_WORK_COMMENTS` (a cycle's describe, text files, zips and
    comments);
  - `TEHAS_OUTSOURCE`, `TEHAS_OUTSOURCE_TIMEOUT`, `TEHAS_OUTSOURCE_GRACE`,
    `TEHAS_OUTSOURCE_RETRIES`, `TEHAS_OUTSOURCE_REST`,
    `TEHAS_OUTSOURCE_REPOS`, `TEHAS_OUTSOURCE_CONTRACTORS`,
    `TEHAS_OUTSOURCE_JOIN_TTL`, `TEHAS_OUTSOURCE_PROXY_ADDR`,
    `TEHAS_OUTSOURCE_PROXY_IDLE` (outsource).
- **Refused.** The line stops every command with 78 and says what to do:
  - a name an earlier release retired (`RETIRED` in the same file):
    `TEHAS_FIXER_LOGIN`, `TEHAS_FIXER_MAX_RETRIES`,
    `TEHAS_FIXER_MAX_PLAN_ANSWERS`, `TEHAS_CLONES`, `TEHAS_WORK_CPUS`,
    `TEHAS_WORK_MEMORY`, `TEHAS_DISPATCHER`, `TEHAS_THEME`, `TEHAS_LAYOUT`,
    `TEHAS_LOW_GATE`, `TEHAS_LOW_GATE_WAIT`, `TEHAS_PRIORITY_FIELD`;
  - a `setting` row of `api/renames-1.0.tsv`, which names the key that
    replaced it or why it went;
  - a value for a setting of a feature still being built, which names the
    feature.

Pending: #2894 drops `TEHAS_MILLWRIGHT_MAX_RETRIES`,
`TEHAS_MILLWRIGHT_FEED` and `TEHAS_MILLWRIGHT_MAX_FEEDS` with a warning, and
#2892 `TEHAS_SPEC_PIPE`. Until #2892 lands, a value for `TEHAS_SPEC_PIPE` is
refused as a building feature's.

## The commands of other parts

`tehas <part> [words]` runs `tehas-<part>` from `PATH`, by
[the rule of the commands page](/tools#the-commands-of-other-parts). Through
2.x:

- a command of tehas wins over a part of the same word;
- `industry`, `kaizen`, `kata`, `monozukuri` and `yokoten` are kept for the
  parts and never become a command of tehas; the list is only added to;
- a part's word is a lowercase letter, then lowercase letters and digits in
  groups joined by one hyphen, at most 32 bytes;
- the part runs in place of tehas with the remaining words as given, no
  settings loaded; its output and its exit status are its own, and are that
  part's contract, not tehas's;
- a word that is no command and no part is exit 64; a part that is found but
  cannot be run is exit 1;
- each release carries the front, `tehas-front-vX.Y.Z-linux-x86_64`, with
  `.sha256` and `.minisig` beside it, signed with the release key. It exits
  with a part's status, 0 for its usage, 64 for a word no part has, 1 for a
  part that cannot be run and 141 when standard output closes.

A command tehas adds in 2.x may take a word that is not kept for the parts,
and then wins over a part of that word. A part that wants its word for good
asks for it to be kept.

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
that follows it, the old name stays accepted through 2.x, and the release
notes say so. The rule itself, which names pass and what
is mounted, is frozen. `console/cycle/tests/work_container.rs` pins the mounts and the variables,
and a change to them is a security change.

## analytics.db

`~/tehas/state/analytics.db` is at `user_version` 6. Within it tables and
columns are only added, never renamed, retyped or removed. A file of schema 4,
from 1.4.x, is migrated to 6 in place as it is opened: it loses
`samples.watts` and `runs.contractor_id`, and keeps every row. A new version
comes only with a major release. [Analytics](/analytics) has the schema;
[`tehas analytics copy`](/api#tehas-analytics-copy) is how a program takes a
consistent copy.

## Changed in 2.0

Every change 2.0 makes to what a program, an operator or a plugin sees, once.
Each names its pull request, what changed, its readers, and what an adopter
does after `tehas update`. No old name is kept beside a new one. The readers
are named by role: adopters (who run a factory), people on an issue, the
consumer that runs many factories, the consumer that reads
the analytics copies, Obeya, MCP clients, the end-to-end suite, and plugins.

- **`tehas answer`, a question answered for a person.**
  #2794, #2808, #2813.
  `tehas answer <issue> <choice> --by <login> [--json]`, or
  `tehas answer <issue> --text <file> --by <login> [--json]`, never both,
  answers the question a stopped ticket or a planning session waits on:
  `retry`, `retry-text` or `park` for a stop, `option:<k>`, `update`, `end`
  or `--text` for a planning question. `--by` must be an account of
  `TEHAS_ALLOWED_LABELERS`, or the command stops with 78. `--text -` reads
  the words from standard input, at most 1 MiB. The object is
  `api/answer.schema.json`, the feature word `answer`.
  Readers: the consumer that runs many factories, Obeya, the end-to-end
  suite.
  Adopter action: a program that answered for a person by ticking a box
  calls `tehas answer`.
- **`tehas question`, the question an issue waits on.**
  #2830.
  `tehas question <issue> [--json]` prints the question and its choices, each
  in the words `tehas answer` takes. The object is
  `api/question.schema.json`, the feature word `question`.
  Readers: the consumer that runs many factories, Obeya, the end-to-end
  suite.
  Adopter action: none.
- **Questions are answered in words; no tick changes anything.**
  #2843.
  A stop's comment and a planning question list their choices as plain
  lines and ask a reply: a stop's by the word of a choice it offers, a
  planning question's by an option's number, `end`, or the person's own
  words. A ticked box does nothing, and tehas says nothing of it.
  Readers: people on an issue, the consumer that runs many factories.
  Adopter action: answer by a reply from an allowed account or through
  `tehas answer`. A program that ticked boxes moves to `tehas answer` and
  reads the choices with `tehas question`.
- **A stop that offers no retry asks nothing.**
  #2843.
  Its comment says to split the issue or edit the body, and its hold's
  `held.clear` says "edit the body, or split the issue with `tehas:plan`".
  A park's hold says "edit the body, or reply to the park comment with the
  word of one of its retries".
  Readers: people on an issue, readers of `tehas status` and
  `tehas blockers`, the consumer that runs many factories, which shows
  `held.clear` and does not parse it.
  Adopter action: none.
- **A retry a reply asked for is `how: reply`; `retry-tick` is gone.**
  #2843, #2867.
  `reply` is the `how` of a retry a person's reply asked for, and `answer`
  is only for `tehas answer`. Every stored `retry-tick` becomes `reply`: in
  the state store at schema 27, in `runs.how` of analytics at each open.
  `tehas history`, `tehas export`, the MCP tool `factory_sessions` (its
  `how` filter) and the approval record take `retry-tick` no more.
  Readers: the consumer that runs many factories, the consumer that reads
  the analytics copies, MCP clients, the end-to-end suite.
  Adopter action: a program that filters or counts runs by `retry-tick`
  reads `reply`.
- **A question open at the upgrade is asked again.**
  #2867.
  The first poll after the update posts it once more in 2.0's words, adds a
  line to the old comment that links the new one, and records the new
  one's choices. A stop of before 2.0 that offered no retry is not asked
  again.
  Readers: people on an issue, the consumer that runs many factories
  (`tehas question` lists the new choices).
  Adopter action: none.
- **The millwright's old name, the fixer, is gone.**
  #2889.
  A cycle's `how` never says `fixer`, and a planning note never
  `fixer-refused`: the state store at schema 28 writes `millwright` and
  `millwright-refused` in their place. The approval record given to a cycle
  says `how: "millwright"`, and a cycle refuses one that says `fixer`.
  Analytics rewrites `fixer` to `millwright` in `runs.how` at each open, and
  `factory_sessions` answers `millwright`. The dispatcher's log and its
  comments name the millwright. `millwright` of `tehas api --json` is
  unchanged.
  Readers: the consumer that runs many factories, where it reads a cycle's
  record, the consumer that reads the analytics copies, MCP clients, the
  end-to-end suite.
  Adopter action: a program that reads `fixer` reads `millwright`.
- **The Protolab is removed.**
  #2877.
  Gone: the verb `protolab` of tehasbot, `tehas-protolab.timer` and
  `tehas-protolab.service`, `live/protolab.json`, `~/tehas/protolab`,
  the command `prototype file` of tehas, the page `/tools-protolab`, the keys
  `protolab-zip-url` and `prototype-file` of `flags` in `tehas api --json`,
  and `prototypes` in `tehas status --json`. Its settings are dropped with a
  warning. What stays for the lab plugin is listed under
  [plugins](#plugins).
  Readers: adopters, the consumer that runs many factories, the end-to-end
  suite, plugins, and parts whose skills tell people to attach
  `prototype.zip` and apply `tehas:prototype`.
  Adopter action: `tehas update` removes the units, the directory,
  `live/protolab.json` and `logs/protolab.log`. Take
  `TEHAS_PROTOLAB_URL`, `TEHAS_PROTOLAB_ZIP_URL`, `TEHAS_PROTOLAB_ZIP_HOSTS`
  and `TEHAS_PROTOTYPE_FILE` out of `instance.env`.
- **An item is served at `item-<n>.<domain>`.**
  #2877.
  The kept store of an item's files is served by the edge router from
  `~/tehas/items` at `item-<n>.<domain>`, where the Protolab used
  `lab-<n>.<domain>`.
  Readers: adopters with an edge domain.
  Adopter action: a DNS name `lab-<n>.<domain>` becomes
  `item-<n>.<domain>`; a proxy in front of the host can drop `lab` from the
  names it sends to the edge router.
- **The first stage is Idea.**
  #2877.
  A ticket's line on the factory's pages reads Idea, In line, Planning,
  Building, Review, Stopped, Shipped. Idea is an open ticket that is not in line:
  it carries none of the state labels of `host/labels.tsv` that put it in
  the queue or further on, and is not closed. Nothing on the pages is called
  Prototype.
  Readers: people on the dashboard, the consumer that runs many factories
  where it shows a ticket's stage.
  Adopter action: none.
- **The review loop leaves core.**
  #2857.
  Gone: the verb `review` of tehasbot, `tehas-review.service` and
  `tehas-review.timer`, the key `review` of `flags` in `tehas api --json`,
  and the review's sandbox kind with its argument list, so a container can
  be started in one way fewer. `TEHAS_REVIEW` and `TEHAS_REVIEW_REPO` are
  dropped with a warning. It returns as a plugin in 2.1.
  Readers: adopters, the consumer that runs many factories (`flags`), the
  end-to-end suite, plugins.
  Adopter action: `tehas update` removes the units. Take the two settings
  out of `instance.env`.
- **The product wiki leaves core.**
  #2862.
  It was still being built. Gone: the command `wiki` of tehas and the verb
  `wiki` of tehasbot, the `/wiki` page and the dashboard's link,
  `live/wiki.json`, `tehas-wiki.service`, `tehas-wiki.timer`, `tehas-wiki.path`, `tehas-wiki-status.service` and
  `tehas-wiki-status.timer`, the sandbox kind `wiki` and the key `wiki` of
  `flags`. `TEHAS_WIKI` is dropped with a warning. It returns as a plugin in
  2.1.
  Readers: adopters, the consumer that runs many factories (`flags`), the
  end-to-end suite, plugins.
  Adopter action: `tehas update` removes the units. Take `TEHAS_WIKI` out of
  `instance.env`. `~/tehas/wiki` is left in place and read by nothing.
- **Energy use leaves core.**
  #2864.
  Gone: the verb `energy` of tehasbot, `tehas-energy.service`, `live/energy.json`, the
  energy section and live meter of the factory floor, the business page's
  live strip, `energy` in `tehas top` and `live/top.json`, and the `energy`
  object of the MCP tool `factory_stats`. `TEHAS_ENERGY`,
  `TEHAS_ENERGY_URL` and `TEHAS_ENERGY_SOURCE_URL` are dropped with a
  warning. It returns in 2.1 as a plugin fed by the host's mesh feed.
  Readers: adopters, the consumer that runs many factories, MCP clients,
  plugins.
  Adopter action: `tehas update` removes the unit and `live/energy.json`.
  Take the three settings out of `instance.env` and `~/.config/tehas/env`.
  A program that stops `tehas-energy.service` when it adopts a factory, or
  starts it when it creates or restores one, drops the unit from both
  lists.
- **analytics.db is at schema 6.**
  #2869, #2886.
  The migration drops `samples.watts`, which nothing writes since energy
  use left core, and `runs.contractor_id`, which no release wrote a value
  in, and keeps every other column and row. A file of schema 4 or 5 is
  migrated in place. `tehas analytics copy --json` reports schema 6.
  Readers: the consumer that reads the analytics copies.
  Adopter action: a program that reads the copies pins `user_version` 6 and
  `console/analytics/src/schema.pinned` of this release.
- **A cycle is given the issue's images alone.**
  #2875.
  Describe, the text files an issue links, unpacked zips and the
  attachments of comments leave core. `TEHAS_WORK_DESCRIBE`,
  `TEHAS_WORK_FILES`, `TEHAS_WORK_ZIPS` and `TEHAS_WORK_COMMENTS` are dropped
  with a warning. The approval record no longer lists `comment_links`.
  `tehas dev --issue` needs `TEHAS_WORK_IMAGES=on`. The four return in 2.1 as
  the `given` plugin.
  Readers: adopters, plugins, the consumer that runs many factories where it
  reads an approval record.
  Adopter action: take the four settings out of the instance. An instance
  that set `TEHAS_WORK_FILES=on` without `TEHAS_WORK_IMAGES=on` now gives a
  cycle no directory unless a retry has comments.
- **The settings of a removed feature load, warn once and are dropped.**
  #2846.
  A 1.4.x instance never fails to load for a setting of a feature 2.0 took
  out of core; the list is under
  [what 2.0 does with a setting](#what-20-does-with-a-setting).
  Readers: adopters, the consumer that runs many factories where it writes
  an instance's settings.
  Adopter action: take the lines out when it suits you.
- **`cargo xtask config-check`, the settings checked before the update.**
  #2885.
  `cargo xtask config-check --settings <instance dir>`, in a checkout of the
  release, runs on an instance directory the checks `tehas update` runs on
  the settings before it installs. It prints `dropped <NAME>: <reason>` for
  each setting 2.0 drops and `fail <NAME>: <why>` for each finding, then
  `pass` or `fail`, and exits 0 or 78. It reads only the instance directory:
  not `~/.config/tehas/env`, no GitHub, Docker or git, and it writes
  nothing.
  Readers: adopters, the consumer that runs many factories where it keeps
  an instance's settings.
  Adopter action: run it before the update, and again after each fix,
  until it passes.
- **Outsource is removed, with the model proxy and the contractor.**
  #2886.
  It was still being built: no release could turn it on. Gone: the command
  `outsource` of tehas and the verb `outsource-proxy` of tehasbot, the
  programs `tehas-proxy` and `tehas-contractor`,
  `tehas-outsource-proxy.service`, the key `outsource` of `flags` in
  `tehas api --json`, `contractors` in `tehas status --json` with the sample
  `api/status.outsourced.json`, the MCP tool `factory_contractors`,
  `live/contractors.json`, and the vipune stand-in of the sandbox image. Its
  ten settings are dropped with a warning. The state store's step to 29
  drops its tables, and analytics 6 drops `runs.contractor_id`.
  Readers: adopters, the consumer that runs many factories, the consumer
  that reads the analytics copies, MCP clients, the end-to-end suite.
  Adopter action: run `tehas update`, which stops and removes
  `tehas-outsource-proxy.service` and removes `libexec/tehas-proxy`,
  `libexec/tehas-contractor`, `live/contractors.json` and
  `sandbox/vipune-shim`. Then take any `TEHAS_OUTSOURCE` and
  `TEHAS_OUTSOURCE_*` lines out of `instance.env`.
- **The All hands label is removed.**
  #2881.
  `tehas:all-hands` on the control issue is no longer read, and
  `tehas labels` no longer creates it. `tehas status` prints no `all-hands`
  line; `tehas status --json`, `live/status.json` and `checks` of the health
  file have no `allHands`; the dashboard shows no "all hands" mark.
  `features` in `tehas api --json` no longer lists `all-hands`.
  Readers: the consumer that runs many factories, Obeya, the end-to-end
  suite, plugins.
  Adopter action: none. A `tehas:all-hands` label left on a control issue
  does nothing; delete it on GitHub if you like, and remove
  `~/tehas/state/all-hands.ignored` where an earlier version wrote it. A
  program that read `allHands` or checked `features` for `all-hands` finds
  them gone.
- **The state store is at schema 29.**
  #2794, #2830, #2843, #2867, #2889, #2886.
  The steps 24 to 29 are listed under [Upgrading to 2.0](#upgrading-to-20).
  `tehas state check --json` prints `schema` 29, and `rows` counts the new
  table `answers` and no longer the tables of outsource.
  Readers: `tehas update`, readers of `tehas state check` and of a state
  copy, the end-to-end suite.
  Adopter action: `tehas update` migrates the store.
- **2.0 is supported from 1.4.x.** This page. The update to 2.0 is
  supported from 1.4.x only, and a host upgrades its factories and the
  programs that run them together.
  Readers: adopters, the consumer that runs many factories.
  Adopter action: update a host on an older release to the newest 1.4.x
  first.
- **`tehasbot refresh --pr N` is not public.** This page. `tehasbot` is
  internal, every verb of it.
  Readers: adopters, the consumer that runs many factories.
  Adopter action: call `tehas refresh --pr N`.

## Pending for 2.0

Planned or in review, not merged. Each moves to
[Changed in 2.0](#changed-in-20) when it lands, with its readers and
adopter action as the pull request states them.

- **The millwright's grant goes, and `tehas start` is the one way a
  program starts work for a person.**
  #2894. The bot's
  `tehas:ready` no longer retries a cycle a person approved before, and the
  feed no longer starts an issue a person wrote; the poll takes the label
  off as an outsider's. `TEHAS_MILLWRIGHT_MAX_RETRIES`,
  `TEHAS_MILLWRIGHT_FEED` and `TEHAS_MILLWRIGHT_MAX_FEEDS` are dropped with
  a warning; `features` drops `millwright-grant`. The millwright's
  `action=retry` and `action=change` comments do nothing; `action=answer`
  and `action=finish` work as before.
  Readers: the consumer that runs many factories, Obeya, the end-to-end
  suite, plugins.
  Adopter action: a program that applied `tehas:ready` as the millwright to
  start or retry work for a person runs `tehas start` for a signed-in
  person instead.
- **The spec mark goes.**
  #2892. A `Spec:` line is
  plain text, no pull request is checked for a spec block, the key
  `spec-pipe` leaves `flags`, and `TEHAS_SPEC_PIPE` is dropped with a
  warning. Lean's `pi-spec v1` block stays.
  Readers: adopters, the consumer that runs many factories (`flags`).
- **The approval record refuses `comment_links`.**
  #2888. A new approval that
  names the key is refused; a stored one loads with it ignored.
  Readers: readers of the approval record.
- **Events.** T11, spec `docs/specs/2026-11-22-events.md`
  (#2878, merged); the build
  is to come. Notify events, each with a schema in `api/` and a witness in
  the factory API:

  | Event | Raised when | Key |
  | --- | --- | --- |
  | `cycle.started` | a start is on record | cycle id |
  | `cycle.parked` | a cycle's history line says it parked | cycle id |
  | `question.asked` | a question is recorded | the question's id |
  | `cycle.ended` | a cycle's history line is written | cycle id |
  | `change.offered` | a cycle left a change for review | cycle id |
  | `factory.limit-changed` | `tehas limit` or `tehas drain` writes another limit | the limit's `set` |
  | `ticket.acted` | an allowed account's act is recorded | `<ticket>@<act>@<at>` |
  | `ticket.closed` | a ticket tehas follows is recorded closed | `<ticket>@<at>` |
  | `change.merged` | a change linked to a blueprint is recorded merged | the change's number |

  With them: the end word `pr` becomes `offered`, the `how` word `label`
  becomes `approved`, `tehas question` prints `askedAt` for `asked_at`, a
  question's `id` becomes the store's own number, every start gets a
  history line, `tehas api --json` prints `store`, and `features` gains
  `cycle-ids`, `questions` and `events`. A further store step and an
  analytics step carry the words.
  Readers: the consumer that runs many factories, the consumer that reads
  the analytics copies, MCP clients, the end-to-end suite, plugins.
- **The plugin hook is named by its event.** T11.8. The grant
  `hook:after:work` becomes `hook:after:change.offered`, with no alias.
  Readers: plugins.
- **Lean is the default driver.** T13. An issue with a `pi-spec v1` block
  runs the lean driver; an issue without one runs classic. When lean is
  only the default and refuses the repository's setup, the same cycle runs
  classic, with a host message and one comment on the issue. A lean start
  on a sandbox image with pi-rukas older than v0.15.6 is refused.
  Readers: adopters, the consumer that runs many factories.
  Adopter action: set `verify.roster`, or `TEHAS_DRIVER=classic`.
- **The rest of clean house.** T16. Panels, the Business page and the Host
  panel (`TEHAS_TOP`) leave core for plugins (`tehas top` stays); the
  `tehas:prototype` label and the `prototype` kind of `TEHAS_ISSUE_TYPES`
  go; the store keeps only the migration from 1.4.x.
  Readers: the consumer that runs many factories, which holds
  `tehas:prototype` in its queue and starts `tehas-top.service`, and parts
  that read the label.
- **The ports.** T4, spec `docs/specs/2026-11-22-ports.md`.
  `host/labels.tsv` gains a fourth column, `word`, and `host/markers.tsv`
  is new. Two settings choose the adapters: `TEHAS_CODE_HOST` and
  `TEHAS_TICKETS`. In 2.0 each accepts only `github`; a missing setting
  means `github`, so a 1.4.x instance still loads, and any other value is
  refused with 78.
  Readers: adopters, the consumer that runs many factories where it keeps
  an instance's settings.
- **The rename.** T10. tehas becomes tehas-factory. The `$id` of every
  schema in `api/`, the release repository and the manual move with it.
- **The surface list for 2.x.** T9's last update: `api/surface-1.0.txt`
  becomes `api/surface-2.0.txt`, and `console/tests/surface.rs` reads it,
  once the removals above have landed.

## Features still being built

A feature is `building` or `shipped` in a release (`console/core/src/features.rs`).
Only a shipped feature can be on. A building feature leaves nothing on the
frozen surface ([the rules for 2.x](#the-rules-for-2x)): its switch set to
`on`, or a line that gives one of its own settings a value, stops every
command with exit 78 and names the feature; it adds no command, no label, no
`--json` field and no MCP tool; and its settings, its key in `flags`, its
commands, fields and MCP tools are not contract until the release that ships
it. `tehas api --json` lists every feature in `flags`, `false` while it is
building, and a shipped feature's word in `features`.
`api/settings-rules.json` marks the rows of a building feature
`"building": true`. `api/commands.txt` marks a command of a building feature
`building=<feature>` at the end of its row, and a flag of one that a public
command gains `--flag:building=<feature>`: neither is on the surface until
the feature ships. Promoting a feature to shipped only adds. A lab host
proves a building feature on a build that reads `TEHAS_TEST_FEATURES`, which
no release does: see [proving a building feature on a lab
host](/tools-update#proving-a-building-feature-on-a-lab-host).

What each registered feature leaves visible while its flag is off:

### spec-pipe

Building. Coming out in 2.0 with #2892
(Pending). Off by default; `TEHAS_SPEC_PIPE=on` stops every command with exit
78 and says `spec-pipe` is still being built.

- `tehas api --json` has `"spec-pipe": false` in `flags`, and no word
  `spec-pipe` in `features`.
- The poll asks an issue's body nothing more: a `Spec:` line is text, and a
  body with one starts as any other.
- A start fetches no spec and its approval keeps no pin.
- A cycle is given no `spec` and no `approved.json`, and its manifest names
  neither.
- No pull request is checked for [the pull request spec
  block](#the-pull-request-spec-block): a handoff waits for a merge as
  before.
- No command, no label, no `--json` field, no MCP tool.


#### The pull request spec block

Part of `spec-pipe`, so it is not part of the frozen surface. A pull request
built from an approved spec answers it with this block in its body, version
1:

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

### lean

Building. Planned to ship in 2.0 as the default driver (T13, Pending). Off by
default; `TEHAS_LEAN=on` stops every command with exit 78 and says `lean` is
still being built, and so does `TEHAS_DRIVER=lean`, which names the feature
it needs.

- `tehas api --json` has `"lean": false` in `flags`, and no word `lean` in
  `features`.
- `TEHAS_DRIVER` takes `classic`; every cycle runs the classic driver.
- `tehas work --driver lean` exits 78 and starts no cycle. `--driver classic`
  works as before.
- The labels `tehas:lean` and `tehas:classic` stay in the label set, so
  `tehas labels` does not remove them. `tehas:lean` is inert: a start that
  would run lean starts nothing, the label that asked comes off and the issue
  says once that lean is still being built. Classic never runs in its place.
  `tehas:classic` and an issue with no driver label run classic.
- An approval record that names driver `lean` is refused with 78 by
  `tehas work`; it does not run as classic.
- No command, no `--json` field, no MCP tool.

[Choosing the driver](/operating#choosing-the-driver) says what the feature
does once it ships.

### plugins

Building since v1.2.0, and off in 2.0: finishing it is 2.1's work. The 2.0
contract holds the plugin API as it stands, outside the frozen surface.
Off by default; `TEHAS_PLUGIN=on` stops every command with exit 78 and says
`plugins` is still being built.

- `tehas api --json` has `"plugins": false` in `flags`, and no word `plugins`
  in `features`.
- The settings `TEHAS_PLUGINS`, `TEHAS_PLUGIN_GRANTS_<NAME>`,
  `TEHAS_PLUGIN_ACCESS`, `TEHAS_PLUGIN_LIMITS`, `TEHAS_PLUGIN_MODEL_DAY` and
  `TEHAS_PLUGIN_GATE_ADDR`
  are in `api/settings-rules.json`, but they are not contract: a line that
  gives one a value stops every command with exit 78 and names `plugins`.
- `tehas plugin` and its subcommands exit 78 and read nothing more,
  and `tehas update` fetches and builds no plugin. `tehas plugin check
  <dir>` runs: it reads a local folder and no settings. `tehas help plugin` prints
  its text, and the list of `tehas help` does not name it.
- `api/plugin.schema.json`, `api/plugin-review.schema.json`,
  `api/plugin-manifest.schema.json`, `api/plugin-context.schema.json` and
  `api/plugin-verdict.schema.json` and
  their samples are not contract until the release that ships the feature.
- The state store has the table `plugin_runs` from schema 19, empty.
- `live/plugins.json`, the plugins menu, is written only while the feature
  is on and a ready plugin is granted `menu`, so a release writes none. Its
  shape is not contract until the release that ships the feature.
- `tehas-plugin` and its units are not installed, `tehasbot plugin` runs
  nothing, and no plugin container starts.
- The gate's program `tehas-plugin-gate` and its unit
  `tehas-plugin-gate.service` are installed only while the feature is on, so
  a release installs neither. The edge router sends the names `plugin-*` and
  `plugins*` to the gate in every release; with no gate they answer 503.
- A panel id starting `p-` is a plugin's view, which `tehas panel set`
  refuses with 64 whether the feature is on or not. The flags
  `--app-client-id` and `--app-secret-file` of `tehas setup` belong to the
  feature and are not contract until it ships.
- No label, no `--json` field of another command, no MCP tool, no file under
  `live/`.

The plugin API as 2.0 holds it, for the work of 2.1:

- **The grants.** A plugin asks in its manifest and a person grants for the
  plugin's commit; `tehas plugin review` lists each grant, and every
  install, enable and pin move checks them again. The one hook grant the
  code takes today is `hook:after:work`. With T11.8 (Pending) it becomes
  `hook:after:change.offered`, with no alias.
- **The events a plugin can see**, under the names 2.1 grants: each event
  of the [list under Pending](#pending-for-20) as `hook:after:<event>`, and
  a gate as `hook:before:<event>=<policy>`, where the policy is `hold` or
  `pass` and a gate grant with none is refused with 78. After T11.8, 2.0
  takes one, `hook:after:change.offered`, and refuses every other `hook:`
  word as not in this release. The first gate, `hook:before:cycle.started`, comes in 2.1.
- **Revision 1.6.0** of the plugin specification
  (`docs/specs/2026-11-22-plugins-1.6.md`) and the plugin surfaces for 2.1
  (`docs/specs/2026-11-22-plugin-surfaces-2.1.md`) are built after 2.0.

What 2.0 keeps for the plugins that take over the features it removed.
Some of it is live in core: the edge routes `item-<n>.<domain>` to
`~/tehas/items`, and `console/attach` fetches the images a cycle is given.
The rest is library code that nothing in core calls yet:

| Kept | Where | For |
| --- | --- | --- |
| The store of an item's files and the page writer | `console/items` | the lab plugin |
| Which open issues carry a mark from an allowed account | `console/core/src/issue_feed.rs` | the lab plugin |
| The edge's static serving of an item at `item-<n>.<domain>`, from `~/tehas/items` | `host/edge/router.Caddyfile` | the lab plugin |
| Fetch, limits, sniff, zip unpack and manifest; zip by address | `console/attach` | the lab and `given` plugins |
| The describe prompt and its quoting | `console/describe` | the `given` plugin |
| The review's renderer | `console/review-render` | the `review` plugin |
| The wiki's collector, check and render | `console/wiki-collect`, `console/wiki-render` | the `wiki` plugin |
| The mesh read | `console/mesh-feed` | the `energy` plugin |

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

### tehas blockers

| Status | When |
| --- | --- |
| 0 | Shown |
| 1 | The record of the last poll is there and cannot be read |
| 3 | No poll has recorded its holds yet |
| 64 | An argument other than `--json` |
| 78 | The settings are missing or malformed |

### tehas start

| Status | When |
| --- | --- |
| 0 | Approved, withdrawn, or nothing to withdraw |
| 1 | GitHub did not answer, or the state store refused or is not installed |
| 3 | GitHub has no open issue of that number: none, a pull request or a closed issue. Nothing is recorded |
| 64 | A bad argument; nothing is written |
| 78 | The settings are missing or malformed; nothing is written |

### tehas answer

| Status | When |
| --- | --- |
| 0 | The answer is recorded |
| 1 | The state store refused or is not installed |
| 3 | No question of that kind waits on the issue, or the question does not offer that option. Nothing is recorded |
| 64 | A bad argument; nothing is written |
| 78 | `--by` may not authorise work, or the settings are missing or malformed; nothing is written |

### tehas question

| Status | When |
| --- | --- |
| 0 | A question waits, and it is printed |
| 1 | The state store refused or is not installed |
| 3 | No question waits on the issue |
| 64 | A bad argument |
| 78 | The settings are missing or malformed |

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
| 78 | The settings are missing or malformed (a setting 2.0 refuses among them), a login cannot be resolved or its id changed, or there is no console or no platform to update from |

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
| 78 | The settings are missing or malformed, or with `--sandbox` the slice cannot be used |

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
