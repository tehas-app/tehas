# Handover: tehas to coding standard v2, for v2.0.0

Date: 2026-11-21. Status: forked from v1.4.0 on 2026-11-22, at T0. Start a
session on this work here.

The owner's direction (2026-11-21): every codebase of the tehas system
moves to **coding standard v2**. Once tehas reaches **v1.4.0**, the factory
is refactored to coding standard v2, and the result is **v2.0.0**.

- **v1** is whatever the code is now. There was no formal definition.
- **v2** is the rules R1 to R10 below, enforced by the architecture test
  and reviewed against `docs/architecture-review.md`. This file carries
  what the work needs.

The aim is a more resilient architecture, **not a new code style**. The
style in `AGENTS.md` stays as it is. Resilient means:

- a supplier changes, one module changes;
- a supplier fails (an outage, a rate limit, an odd answer), and the
  factory carries on and reports it once;
- a crash loses nothing;
- a contract changes without a flag day.

Keep it light: no rules on function length or naming, no rewrite.

v2.0.0 carries everything: coding standard v2, the adapters with the
adapter API, and the repository's new name, tehas-factory (the owner,
2026-11-21). See "Released as a trio" below.

## Released as a trio

The owner decided on 2026-11-21 that the three releases that complete
coding standard v2 in the tehas system are tagged together: tehas 2.0.0
and the 2.0.0 and 1.0.0 of the two repositories that read it. For tehas:

1. **2.0.0 carries everything.** Coding standard v2, the adapters and the
   adapter API (T4), events (T11), and the rename (T10).
2. **The branch stays unmerged until the trio is tagged.** Main stays the
   stable line and ships 1.4.x patches; each is ported into the branch.
   Nothing is tagged from the branch, release candidates included, until
   the owner says so (2026-11-22). The merge into main and the final tag
   wait for the trio and the owner.
3. **One set of ports, defined here.** The ports of T4 are the shared
   set: a consumer that runs many factories adopts the
   same ports instead of writing its own. Its needs (the operations, the
   shapes, the failures, the actors) are read before T4 drafts, and
   neither side merges its ports before the other has read them.
4. **The gate of the trio.** Every rule at conformant, graded by a fresh
   reviewer and confirmed by a person, with no departure or raised floor
   written to make a date. The contracts between the three are final,
   the factory API among them, and the end-to-end suite passes the three
   tagged versions together.
5. **Plugins stay off.** The plugin API is written into the 2.0 contract
   at T9 as it then stands. The feature stays behind the setting that
   refuses it today, and finishing it is 2.1 work. The refactor moves
   the plugin code with its behaviour kept, and builds no new plugin
   features.
6. **pi-rukas v0.14.100.** 2.0.0 pins pi-rukas v0.14.100 (the owner,
   2026-11-21). It is not released yet. Each release candidate's tag
   names the pi-rukas commit it pins (`scripts/tag-release.sh`); from the
   first candidate after v0.14.100 is out, that is its commit. If it is
   not out by the final, the owner decides.

### The adapters

- The ports of T4 are that abstraction, three of them (the owner,
  2026-11-22): tickets, blueprints and the code host. Tickets and
  blueprints are kept apart in the API: they are subtly different.
- **Allowed setups.** A code host (GitHub, GitLab or Bitbucket) always.
  Tickets come from that code host or from Jira. Never Jira alone, so
  blueprints always come from the code host.
- **The contract names the act, not the label.** Approve, block, plan,
  refine are tracker terms; labels are the GitHub adapter's answer.
- **The account behind a change.** Authorisation stays who applied the
  label (AGENTS.md rule 5). In tracker terms: every tracker adapter names
  the account behind a change and keeps its history.
- **The adapter API is defined here.** The ports, the named outcomes, the
  fakes and the adapters' contract tests, with a setting that chooses the
  adapter. GitHub stays the one adapter written in 2.0.0. An adapter
  delivered from outside waits for plugins (2.1).

## Before the fork

Done: the fork is at v1.4.0 (a0aa16a5), the owner approved the gate
changes and holds the factory that builds tehas to no new work.

1. **Wait for v1.4.0.** The tag is the owner's. This plan is on main
   already (#2735, merged 2026-11-21), so v1.4.0 carries it.
2. **The owner opens the branch to the gate.** `verify.yml` runs on pull
   requests into `main` and `release/**` only, and the gate files are
   changed by people (AGENTS.md rule 7). Before the first pull request
   into the branch, the owner either adds `refactor/coding-standard-v2`
   to `verify.yml`'s branches, or names the branch under `release/`.
   Until then, run `scripts/verify.sh` yourself on every pull request
   into the branch, and say so in it.
3. **The owner stops new work reaching the factory that builds tehas.**
   Its queue gets no new tickets while the branch is open.

## How the work runs

1. **Fork.** Create `refactor/coding-standard-v2` from the commit tagged
   `v1.4.0`, and push it. Write the fork
   commit in the log at the end of this file.
2. **Work in pull requests into the branch.** Each step below is one or
   more pull requests whose base is `refactor/coding-standard-v2`, each
   small, behaviour-keeping and through the gate on its own. Squash
   merges are fine there.
3. **New work stays off tehas meanwhile** (the owner, 2026-11-21: the
   same process as the other repositories, a branch with a freeze).
   - From the tag v1.4.0 until the branch merges, nothing new starts on
     main.
   - 1.4.x patch releases continue from main, for fixes that must land:
     bugs that break an install, and security.
4. **Port what lands on main.** When a fix lands on main, merge
   `origin/main` into the branch in a pull request of its own, "Port
   main @ <sha>", as soon as it lands. Never rebase or force-push the
   branch: others' pull requests are based on it. Write each port in the
   log.
5. **Finish.**
   - The last pull request into the branch writes `v2` in
     `.tehas/coding-standard` (see "Done").
   - When the trio is tagged, one pull request from
     `refactor/coding-standard-v2` into `main`, merged with **a merge
     commit**, not squashed, so the steps stay in main's history.
   - Tags, release candidates included, wait for the owner's word. pi-rukas
     v0.14.100 is pinned before any tag.
6. **A merge train of two lanes.** At most two pull requests into the
   branch are in flight at once. Each is brought up to the branch's tip,
   passes the gate and the architecture test, and is squash-merged; the
   next takes its lane.

While the branch is open, `.tehas/coding-standard` says `v1`, so the
architecture test is skipped. Run it on every pull request into the
branch:

```
python3 -I scripts/architecture.py --force --base origin/refactor/coding-standard-v2
```

The pull request that lowers a count lowers its `max`. A `max` is never
raised without a new `why`.

## What must keep passing

**The gate.** Every pull request passes `scripts/verify.sh`, the whole
gate; `--changed` while a change is made, `--full` before each merge into
the branch.

**The tests decide what the behaviour is.** A step that changes what a
test asserts is wrong. The one exception is moving a test from stubbing
`gh` onto a port's fake, where the fake gives the same answers.

**The pins hold.** The sandbox's argument lists in `test/fixtures/argv/`
and the labels of `host/labels.tsv` hold unchanged. A change near the
cycle's container is a security change (AGENTS.md rule 4): name it in the
pull request.

**The hard rules of AGENTS.md bind every step:**

- platform files name no product;
- fail closed;
- loading settings has no side effects;
- do not widen a cycle's container;
- authorisation is who applied the label;
- tests first;
- people change the gates;
- leave the real instances alone;
- nothing merges on its own;
- no secrets.

**What may change in v2.0.0.** A major version may change the contract
(`host/docs/contract.md`): the commands, flags and exit statuses, the
settings, the labels, the factory API and the plugin API. This work
changes it only where the refactoring needs to, and each change:

- is named in the pull request;
- is added beside the old term first and removed only in T9 (expand,
  then contract);
- is listed with its readers: adopters, through `tehas update`, as
  `api/renames-1.0.tsv` did for 1.0; programs that use the factory API;
  plugins.

Several security boundaries each decide today whether a `gh` call writes.
They encode one rule, but they may stay separate on purpose: decide that
with the owner before merging them.

## The rules (coding standard v2)

| Rule | Must hold |
| --- | --- |
| R1 One owner per piece of knowledge | A supplier's paths, fields, documents; a file's layout; a set of state words: each written in one module |
| R2 Ports in our terms | Outside systems behind traits named for what the factory needs, not `gh` argument lists or raw JSON; the adapter alone knows the supplier; general enough for two suppliers, no further |
| R3 Pure core | Decisions are pure functions; effects at the edge, journalled where a crash would lose work (the dispatcher's decide then apply) |
| R4 Typed seams | No `serde_json::Value` across a seam; parse at the edge, every read field required, refuse what does not parse |
| R5 Deep modules (should) | Small public surface against what a module hides; group by knowledge, not by kind of code or by the order the old bash ran |
| R6 Failures in the adapter | Time limits, retries, paging and edge cases inside the adapter; a few named outcomes out of it |
| R7 Tests at the interface | Tests drive the ports' fakes, never `gh` argument lists; adapters have contract tests on recorded samples |
| R8 Inward dependencies | The core uses nothing of the shell or the adapters; the crate graph holds it |
| R9 Expand, then contract | A new form beside the old, callers moved, the old removed in v2.0.0 |
| R10 Small steps | Behaviour kept; each step its own pull request |

The rules apply to the bash that stays as much as to the Rust: a name
owned by one module is owned there for both.

## Where tehas starts

Measured at c4dff5f9 with the architecture test's draft checks. The
counts are from grep, include tests inside source files, and are
approximate. Re-measure at the fork.

**Already as v2 asks:**

- the store, mcp, attach, proxy, contractor and analytics crates are
  deep;
- the dispatcher decides, then applies (ADR 0003);
- core's rule "each rule written once";
- the `Forge` trait answers in a fixed word list, with a 404 on removing
  a label counted as done.

**Where it departs:**

- **GitHub is known directly in most crates.** The `Forge` trait covers
  labels only. Outside it there are about 280 places that run `gh` and
  about 100 REST paths, most in the front's gh shim, cycle's relay and
  dispatch's reads. `host/docs/forge.md` lists areas outside the trait.
- **The functional core reads GitHub's shapes.** dispatch's `Snapshot`
  holds `Value` fields as GitHub answers them. `decide`
  (`console/dispatch/src/decide.rs:370`) reads `body`, `labels`,
  `createdAt` and `headRefName`, and its actions are `Value`s ordered by
  a `_line` key (`:455`).
- **Split by time, not by knowledge.** Many steps in dispatch and cycle
  are placed "as the bash does it". The Rust port kept the old bash's
  order.
- **Labels have no single owner.** The state words appear in about 400
  places, and `host/labels.tsv` is read in several.
- **The `$TEHAS_HOME` layout has no owner.** `state/...` and `live/...`
  paths appear in about 570 places.
- **The store is deep inside and stringly outside.** Callers pass verb
  strings and read the answer as `Value`.
- **Several places each decide whether a `gh` call writes**: the
  dispatcher's read guard, the front's gh shim, the sandbox's bash shim
  and cycle's relay.
- **Tests pin GitHub's call shapes:** about 84 `t_stub gh` uses in the
  shell tests.
- **Untyped data:** about 1000 function signatures carry `Value`.

**Draft checks and their counts at c4dff5f9:**

| Check | Rule | Count | Aim |
| --- | --- | --- | --- |
| `github-rest` (REST paths, `graphql`, the API host outside the forge) | R1 | 105 | 0 |
| `gh-argv` (`gh` run outside the forge) | R2 | 277 | 0, or the security shims as a written floor |
| `label-words` (state labels outside one module) | R1 | 413 | 0 |
| `home-layout` (`state/`, `live/`, `run/` paths outside one module) | R1 | 570 (without `run/`) | 0 |
| `untyped-signatures` (`Value` in a signature) | R4 | 996 | 0, or a floor written in `docs/architecture.md` |
| `untyped-fields` (`Value` in a public field) | R4 | 54 | 0, or a floor written |

The checks also read `bin/`, `libexec/`, `lib/` and the host's scripts
where a name can appear in bash. A floor above 0 is allowed only where
it is the honest answer, and is written down with its reason. Examples:
a security shim that must read `gh` arguments, or the adapter's own
parsing.

## The steps

In this order, T11 before T9 and T10 last. T3 to T7 may overlap where a type and its port go
together. Each "Done when" is checked before the next step leans on it.

### T0. The test, the rules, the map

- The architecture test: `scripts/architecture.py` and its test, the same
  file as in the other repositories of the tehas system.
- `architecture.toml` with the checks above, tightened, at the fork's
  counts.
- `.tehas/coding-standard` with `v1`.
- `docs/architecture-review.md`.
- An architecture section in AGENTS.md. AGENTS.md is a gate file: a
  person makes that change, or approves it in the pull request.
- The test as a job of the gate, with the owner's approval.
- A **knowledge map** in a new `docs/architecture.md`:
  - each piece of knowledge, its owner, and where else it is known, with
    counts;
  - each port, its adapters and its fake;
  - the crate graph.
- A **contract** section there: what 1.x froze, and which of it this work
  expects to change.
- `docs/architecture-rubric.md` (the grading below), and a **grades**
  table with the baseline grade as its first row.

Done when the test holds at the fork's counts, the map lists every
outside system and every port, and the baseline is graded.

### T1. Fakes and tests at the ports

A fake tracker and a fake code host, written against the ports of T4.
The Rust tests use them in place of stubbing `gh`. The shell tests that
stub `gh` move to them where the point is not the bash itself. Done with
T4, in the same pull requests: a port and its fake change together, and
a crate's tests move onto the fake before its calls move.

Done when no test outside the forge's own pins a `gh` argument list for
a behaviour of Rust code.

### T2. One vocabulary for ticket states

A module in core alone names the labels and markers, checked against
`host/labels.tsv`. Everyone else says `State::Ready`, not
`"tehas:ready"`. This is what later lets a tracker answer with a native
field instead of a label.

Done when `label-words` is 0, and `host/labels.tsv` is read in one place.

### T3. A typed core

- dispatch's `Snapshot` is filled with tehas's own types.
- `Decision.actions` is an `enum Action`.
- The order is an explicit plan, not `_line`.
- `decide` stays pure and stops knowing GitHub.

This is the precondition for the ports.

Done when `decide` and `Snapshot` carry no `Value`, and the dispatcher's
tests pass unchanged.

### T4. Three ports in tehas's terms

- **A tickets port:** the work level. Epics, stories, approval, priority,
  release targets, the account behind a change and its history. It grows
  from `Forge`.
- **A blueprints port:** the issues a cycle works from. The technical
  design, the plan, the technical discussion, blockers.
- **A code host port:** changes, branches, checks, merges, tags, releases,
  tokens.

To settle in the spec:

- whether a cycle takes one blueprint, or a ticket directly when it has
  none;
- the links from a ticket to its blueprints to their changes, which
  tehas keeps, since no supplier does;
- identity mapped across suppliers;
- approval named as an act (on Jira, a transition), with authorisation
  still the account behind it (AGENTS.md rule 5).

GitHub is their one adapter, answering all three, and it owns every `gh` argument, path,
document and field, the paging, the retries and the time limits. An
outage or a rate limit is handled there, and reported once. The security
shims stay where they are unless the owner agrees to merge them.

They are the adapter API of "The adapters" above, and the shared set a
consumer that runs many factories adopts: read its needs before the
drafts, and answer its open questions in the spec.

Design each port twice: two independent drafts each, read by fresh
reviewers against R1, R2, R4, R6 and R7, and against how little of the
rest changes. Write the choice in a spec under `docs/specs/`, and update
`host/docs/forge.md`. Move one crate at a time.

Done when `github-rest` is 0 and `gh-argv` is at 0 or at its written
floor, and a fake code host that times out or rate-limits leaves the
dispatcher and a cycle running, with one report.

### T5. Deep homes for files

- A `Home` type owns the `state/`, `live/` and `run/` paths. `run/` holds
  the sockets, one directory per socket, each given only to the container
  that uses it; they stay out of `live/`, which is mounted more widely.
  Its mounts are a security change (AGENTS.md rule 4).
- A typed store client stands over `call`, so no caller passes a verb
  string or reads `Value`.

Done when `home-layout` is 0 outside `Home`.

### T6. Regroup by knowledge

core, ops, dispatch, cycle and the front are regrouped by what they know,
not by the order the old bash ran. Each piece gets a small public
surface, and its tests move to it. The cut is the step's to settle from
the map. Ask the owner before the first crate is split.

Done when the public-items report shows no module grouped by kind, and
the crate graph holds R8.

### T7. Typed seams

`Value` leaves the seams as the core, the ports and the store client are
typed (T3 to T5). What is left is moved, or written down as a floor.

Done when `untyped-signatures` and `untyped-fields` are 0 or at written
floors.

### T8. The bash that stays

What is still bash follows the same owners: it asks the Rust programs
for names it needs, rather than writing them again. `scripts/bash-lines.max`
holds the ceiling as before.

Done when the checks that read bash hold at 0 or at written floors.

### T11. Events

Events ship in 2.0.0, in the manner of Symfony's EventDispatcher (the
owner, 2026-11-22). Names use the ports' words (`ticket.approved`,
`change.merged`, `cycle.after-work`), never a supplier's.

- **In Rust:** a small in-process dispatcher of our own: typed events,
  ordered listeners, an event that can be stopped.
- **For plugins:** events as JSON, with schemas in `api/`, of two kinds.
  A notify event comes after the fact, as `hook:after:work` does today.
  A gate event waits for an answer, with a time limit and a policy a
  person picks. Each event a plugin listens to is a grant,
  `hook:<when>:<event>`, written for the plugin's commit, listed by
  `tehas plugin review`, and checked again on every install, enable and
  pin move. Plugins stay off in 2.0.0: the contract specifies this, and
  the in-process side ships.
- **In the front end:** the kit's pages use `EventTarget` and
  `CustomEvent` named `tehas:<event>`, with server events over SSE and
  `postMessage` to plugin frames from an allowed list. The kit owns these
  names; the other repositories of the trio follow them.

T9 lists the event names and schemas in the 2.0 contract.

Done when the dispatcher and its events are in, each event has its
schema in `api/`, and the contract lists them.

### T9. The contract for 2.0

- `host/docs/contract.md` for 2.x, with a section "Changed in 2.0" that
  lists every change and its readers.
- Each rename as a row, in the way `api/renames-1.0.tsv` held 1.0's, so
  `tehas update` can list what an instance still uses.
- The `api/` files for 2.0.
- A changelog fragment with an adopter action.

Done when the 2.0 contract is written, and each change has its row and
its note to adopters.

### T10. The rename

tehas becomes tehas-factory in 2.0.0, and `tehas-app/tehas` is later
reused for a get-started repository, which ends GitHub's redirect. Every
reference here moves before that. After T9:

- Grep the whole tree for `tehas-app/tehas` (not a list): the release
  repository of `host/versions.env`, the installer, `tehas update`, the
  ids of the schemas in `api/`, the manual, AGENTS.md and the tests.
- A setting that names the old repository is a contract change (T9 lists
  it): an instance that names it keeps working while the redirect holds,
  and `tehas update` says so.
- The repository's rename on GitHub is the owner's.

Done when nothing outside the renames list and the history in `docs/`
and `CHANGELOG.md` names `tehas-app/tehas` as the factory's repository.

## Grading

Each rule gets one band. The grade is the ten bands, never a sum.

| Band | Score | Means |
| --- | --- | --- |
| `absent` | 0 | The code breaks the rule widely, and nothing measures it |
| `partial` | 0.25 | Holds in some modules; the breaks are not listed |
| `mostly` | 0.75 | Holds except listed breaks, each counted in the test or written in `docs/architecture.md` |
| `met` | 1 | Holds, or holds to a written floor, and CI checks it where it can |

Pass lines:

- **adopting:** every rule `partial` or better, the test in CI and the
  map written;
- **conformant:** R1 to R4 and R6 to R10 `met`, and R5 `mostly` or
  better.

The baseline below is a reading at c4dff5f9, not yet graded by a fresh
reviewer.

| Rule | Band | Why |
| --- | --- | --- |
| R1 | partial | core writes each rule once; labels, layout and GitHub have no owner |
| R2 | partial | the proxy, contractor and attach traits are ports; `Forge` covers labels only |
| R3 | mostly | decide then apply holds; decide reads GitHub's shapes |
| R4 | partial | some crates are typed; `Value` is in the core and at the store's edge |
| R5 | partial | six deep crates; core, ops, dispatch, cycle and the front mixed or shallow |
| R6 | partial | `Forge` answers from a fixed word list; most GitHub failures reach callers |
| R7 | partial | many shell tests pin `gh` arguments |
| R8 | partial | crates exist; the direction is not checked |
| R9 | mostly | the contract and `api/renames-1.0.tsv` hold 1.x; no 2.0 list yet |
| R10 | not graded | |

Grade at the end of T0, at the end of T4, and before the final pull
request.

- The grader sees only the code at that commit and the rubric, not this
  plan or the session that wrote the code.
- Each band names its evidence: a count, a file and line, a test.
- A person confirms the grade.

## Done

All of these, in the last pull request into the branch:

- every count in `architecture.toml` at 0, or at a floor written in
  `docs/architecture.md`;
- a fresh grade at **conformant**, confirmed by the owner;
- `.tehas/coding-standard` says `v2`, and the architecture test passes
  without `--force`, against the base;
- `scripts/verify.sh --full` passes, and the 2.0 contract is written;
- T10 and T11 are done, and the trio's gate holds.

Then, when the trio is tagged, the pull request into main (with a merge
commit), and the owner's release of v2.0.0.

## Open, for the owner

- The AGENTS.md section of T0, which a person makes.
- Whether the security shims that decide a `gh` write stay separate (T4).
- The regrouping of T6, once the map shows the crates.
- Each contract change T9 lists, before it is made.

## Log

Append a line per event: the fork, each step's pull requests, each port
of main, each grade.

| Date | What | Commit or PR |
| --- | --- | --- |
| 2026-11-21 | Plan written and merged ahead of v1.4.0; the work waits for the tag | #2735 |
| 2026-11-21 | The owner approves the branch in `verify.yml`, the architecture test as a gate job, and no new work for the factory that builds tehas | |
| 2026-11-22 | Forked `refactor/coding-standard-v2` from v1.4.0 | a0aa16a5 |
| 2026-11-22 | The gate runs on the branch | #2775 |
| 2026-11-22 | The trio, the adapters, three ports, events (T11), the rename (T10), the pi-rukas pin, no tags from the branch and the merge train added to this plan | this pull request |
