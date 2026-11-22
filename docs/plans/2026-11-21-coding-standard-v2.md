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
   adapter API (T4), events (T11), answers in words (T12), and the rename
   (T10).
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
6. **pi-rukas v0.15.5 or later.** 2.0.0 needs pi-rukas v0.15.5 at the
   least (the trio's program manager, 2026-11-22). T13's proof found two
   faults: on 0.15.1 a roster that exits 0 reports no exit, so lean hands
   back on every pass (fixed in 0.15.2), and lean verify's base probes
   checked out beside the repository, which no cycle container can write
   (fixed in 0.15.4, which checks them out under the repository's
   `.worktrees/`). 0.15.5 adds `.worktrees/` to each clone's
   `.git/info/exclude`, so nothing that stages at the repository's root
   sees lean's trees. tehas refuses a lean start on a
   sandbox image whose pi-rukas is older, saying why. At the trio's
   readiness review the newest pi-rukas release on the 0.15 line is the one
   pinned (`PI_RUKAS_FOLLOW=release:0.15`): pi-rukas 0.16, which moves it
   to npm, is not for this trio. A tag
   names the pi-rukas commit it pins (`scripts/tag-release.sh`).

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
   - When the trio is tagged, the last "Port main" brings main into the
     branch, and one pull request from `refactor/coding-standard-v2` into
     `main` lands the branch so the steps stay in main's history. The
     repository allows squash merges only (checked 2026-11-22), and main
     has no active ruleset. Two ways, the owner's to choose then: the
     owner allows merge commits for that one pull request, or main is
     fast-forwarded to the branch's tip with a plain push (no force), which
     marks the pull request merged. The session doing this work does not
     push to main itself.
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

In this order, T11, T12, T13, T14 and T16 before T9 and T10 last. The owner,
2026-11-22: the migration of GitHub and its peers behind the ports (the
moves of T4, with T1's fakes and the tests of T15 that stub `gh`) is
designed now, in `docs/specs/2026-11-22-ports.md`, and lands after the
rest of the refactor, still before v2.0.0. T3 to T7 may overlap where a type and its port go
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

This is the precondition for the ports. It runs as fifteen small pull
requests, field by field, from the host's scalars to `enum Action` (T3.1
to T3.15). Each parses at `Snapshot::parse`, keeps every case the old
reading had (null, missing, failed, a wrong type), and lowers each `max`
it moves. The helpers that still take `Value` move out of `decide/` to
the parsing edge, and `decide` calls the typed rule.

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

T4's Done also clears every row of "Temporary rises" in
`docs/architecture.md` that is still there when it ends, whatever step
the row names. At #2866 they are: `github-rest` (#2859 and #2860),
`gh-argv` (#2859 and #2866), `raw-statuses` (#2859), `tests-stub-gh`
(#2866) and `untyped-signatures` (#2866). A row added later joins the
list in the same pull request. A T15 port is kept rise-neutral where it
can be: what it adds to a count it takes out elsewhere in the same pull
request.

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

T9 lists the event names and schemas in the 2.0 contract. Its plugin
API section lists every event a plugin can see under exactly the name
2.1 grants: the ports' word for the event (`cycle.after-work`), granted
as `hook:<when>:<event>`. The 1.x hook names, `hook:after:work` among
them, are dropped with no alias (the plugins lead, through the trio's
program manager, 2026-11-22).

Done when the dispatcher and its events are in, each event has its
schema in `api/`, and the contract lists them. The spec is
`docs/specs/2026-11-22-events.md`.

### T12. Answers in words

The tick boxes are retired (#2782, the owner, 2026-11-22): a stop or a
planning question is asked in words, and no tick changes anything. In
the issue's order, expand then contract:

1. **A verb for programs.** `tehas answer <issue> <choice> [--text FILE]
   --by <login> --json`, with its schema in `api/`. For a stopped ticket:
   `retry`, `retry-text`, `park`. For a planning question: `option:<k>`,
   `update`, `end`, or `--text`. `--by` is trusted as the host's caller,
   as `tehas start` is; it must be an account allowed to authorise work,
   or the command stops with exit 78. The history records who, when and
   how (`answer`).
2. **The consumers move to it.** Then the comments lose their boxes:
   options are plain lines that say what each leads to, and a stop with
   nothing to decide asks nothing.
3. **Nothing reads a tick.** A tick on an old comment does nothing;
   `retry-tick` stays readable in history. A person answers by a reply
   from an allowed account or by approving again.

T9 lists it in "Changed in 2.0" with its readers.

Done when #2782's "Done when" holds.

### T13. Lean is the default driver

The owner, 2026-11-22: 2.0.0 runs the lean driver by default. This is a
behaviour change, made in its own pull requests and not inside a
refactor step. The program manager is the session tehas-2d.

What "lean by default" covers (work-a7, 2026-11-22): lean runs an issue
that carries a `pi-spec v1` block. An issue without one runs classic, as
it does today. Sending spec-less issues to lean too, where the product
lets lean draft the spec, is #2839 for 2.1. Classic stays a supported
driver in 2.0 (the owner).

1. **The proof (#1105), on main.** `.pi/work.json` is on main (1d429809).
   About 20 issues of the same size, with
   `pi-spec v1` blocks, split evenly between the drivers, run on a lab
   build (`TEHAS_LAB_BUILD=on`, `TEHAS_TEST_FEATURES=lean`) at the
   pinned pi-rukas, on tehas-tehas. Its hold is lifted for the proof's
   issues only (the owner, 2026-11-22). Read the analytics report by driver and the pull
   requests.
2. **pi-rukas.** The pin carries the fixes of trail-openers/pi-rukas
   #2493: #2494 (several test directories), #2495 (no project-test check
   without a JUnit report), #2480 (a killed lean job keeps running),
   #2403 (the lens review is not recorded), #2501 (the rest of #2410)
   and #2511 (a typed setup refusal, for part 4). All are merged and
   ship in 0.15.0, and the roster fix of 0.15.2, the base-probe fix of
   0.15.4 and the exclude of 0.15.5 join them, so the pin is 0.15.5 or later
   on the 0.15 line
   (`PI_RUKAS_FOLLOW=release:0.15`). Each release
   candidate checks this as it checks the pin.
3. **Ship it.** `lean` in `console/core/src/features.rs` moves to
   shipped, and `host/docs/features.md` marks it proven with the dates
   and issues of the proof.
4. **The default.** With no `TEHAS_DRIVER`, a start runs lean. An issue
   with no `pi-spec v1` block runs classic, as under `TEHAS_DRIVER=lean`
   today. `TEHAS_DRIVER=classic` and `tehas:classic` keep classic.
   When lean is only the default and its result says `failure.kind` is
   `setup` (trail-openers/pi-rukas#2511: a refusal of the repository's
   setup at `ready`, before any model runs, never for a cause in the
   issue, with nothing posted), the same cycle runs classic. An exit
   status is not the key: lean's exit 11 also stops an issue with no
   spec. Nothing stops, and tehas says so (the owner, 2026-11-22): a
   host message names lean's reason and how to set `verify.roster`, with
   the recipes in `host/docs/new-project.md`, the issue gets one comment
   with the reason, and `approval.driver` records classic with the
   reason. A `tehas:lean` label or an explicit
   `TEHAS_DRIVER=lean` parks as today, and any other lean failure falls
   back to nothing. A transient git failure reading the setup is
   `crashed`, not `setup`, so it does not fall back. Lean's setup record
   is final, so the fallback removes `.pi/lean-state/<issue>.json` in
   the checkout, and a later lean start, after the product sets its
   roster, starts fresh. tehas reads no `.pi/work.json` itself: lean's refusal
   is the one rule. #2511 blocks this part only. If it is not in by the
   trio's readiness review, 2.0.0 ships lean available but not the
   default, and the owner decides.
5. **The contract.** T9 lists the new default in "Changed in 2.0", with
   its readers, in these words: "An issue with a `pi-spec v1` block runs
   the lean driver; an issue without one runs classic." The changelog's
   adopter action: "set `verify.roster`, or `TEHAS_DRIVER=classic`".

Done when the proof is in `features.md`, the pinned pi-rukas has the
fixes of trail-openers/pi-rukas#2493, and the dispatcher's and the
cycle's tests show a start with no driver setting running lean, and
falling back to classic on lean's setup refusal.

### T14. The Protolab removed, its plumbing kept

The owner, 2026-11-22: 2.0.0 removes the Protolab. It was envisioned
and never pulled through, so it is deleted as dead code, not migrated.
It is parked, not abandoned: `docs/specs/parked-protolab.md` records
every piece, where it lived and the last commit that had it, so it can
return.
No factory uses it and no issue carries its label. Its plumbing for
storing and serving files stays, made generic, for a plugin to use in
2.1.

Removed:

- the Protolab pass and its pages (`console/src/protolab.rs`, `tehasbot
  protolab`), its timer units, `live/protolab.json` and the `protolab/`
  directory of the home; `tehas update` stops and removes the old units
  (`retired.list`) and the directory (`RETIRED_DIRS`, a list other
  retired folders join);
- `prototype-file` (`tehas prototype file`, `console/src/prototype.rs`)
  and the four settings `TEHAS_PROTOLAB_URL`, `TEHAS_PROTOLAB_ZIP_URL`,
  `TEHAS_PROTOLAB_ZIP_HOSTS` and `TEHAS_PROTOTYPE_FILE`, which load, warn
  once and are dropped (`IGNORED`);
- the `tehas:prototype` label (`host/labels.tsv`, the label pins, the
  dispatcher's flow) and the `prototype` kind (`TEHAS_ISSUE_TYPES`
  keeps `improvement`), with the `prototypes` field of `tehas status
  --json` and its schemas. A separate T16 commit removes these;
- `host/docs/tools-protolab.md` and the rows of `features.md`;
- on the front pages, the Protolab's door, panel and link. The stage
  stays, as Idea (below).

Kept, under neutral names:

- `console/attach`: fetch, limits, sniff, zip unpack and manifest, which
  cycles already use;
- the safe store of a ticket's files and the page writer of
  `console/src/protolab/build.rs`, as the crate `console/items`, and the
  reading of marked issues as the issue feed,
  `console/core/src/issue_feed.rs`;
- the edge's static serving: a directory per item, sandboxed HTML, the
  no-script policy on the generated page, the per-item host name and
  its port, with their tests;
- zip by address (the host allowlist, the redirect walk, the sha256
  pin), as library code, off behind its setting.

Idea stays. The owner, 2026-11-22: the first stage of a ticket's
progress is renamed Idea, and nothing is called Prototype. A ticket can
be at Idea, in development, before it goes in line. The line reads Idea,
In line, Blueprint, Production, Review, Shipped. Idea is an open ticket
that is not in line yet: it carries none of the state labels of
`host/labels.tsv` that put it in the queue or further on (`tehas:ready`,
`tehas:running`, `tehas:awaiting-merge`, `tehas:shipped`, the plan and
refine labels), and it is not closed. A reader tells it from those, not
from a label of its own, so removing `tehas:prototype` and the
`prototype` kind does not remove it. T2's state words name it `Idea`.

T9 lists what goes in "Changed in 2.0", "Idea (was Prototype)", and
what stays and under which names, for the plugin work in 2.1.

Done when nothing names the Protolab, the kept plumbing has its tests
under its new names, and `tehas update` removes the old units.

### T15. Shell tests to Rust

The shell tests that test a Rust program from outside move to Rust
(#2816 to #2822, under #2597), keeping each case's behaviour. Each
issue names the files it needs and when they are free: now, after T3, or
after T4 when its cases stub `gh` (those move onto the ports' fakes with
T1). `test/sandbox_argv_test.sh` and `test/fixtures/argv/` stay shell,
as T4's floor. #2823 shortens the shell run, at any time; #2824 trims
the shell that stays, last. Each port lowers `scripts/test-lines.max`
to its new count.

Done when no shell test tests a Rust program's behaviour that a Rust
test can, and `test-lines.max` is at its new count.

### T16. Clean house

The owner, 2026-11-22: "no adapters or legacy levers for previous APIs:
this is our opportunity to clean house from the experimentation from
v0.x.x leading to the v1.x.x series." Only the current majors of the
trio are supported, so 2.0 carries no backwards compatibility.

- **No levers for older APIs.** No alias, renamed-command shim,
  deprecated flag, or setting or environment toggle that brings back an
  older behaviour. The ports and adapters of T4 are not such levers.
- **The experiments go.** Settings and feature flags no one uses, dead
  subcommands, half-finished paths, and stale docs and specs that never
  became core. Plugins stay as planned: off, their code moved and kept.
  What was envisioned and may return is parked with a record, as T14
  parks the Protolab.
- **The store keeps one migration.** The state store migrates from the
  schema of 1.4.x to 2.0's, and the owner confirmed that one is needed.
  Migrations from older schemas, the imports of v0.30 files, and their
  fixtures and tests go.
- **Every existing configuration works or migrates.** The owner,
  2026-11-22: "make sure existing configurations either work as is or
  can be migrated." Each setting, environment variable, key or file of
  an instance's layout that 2.0 removes or renames is either still read
  as it is, or rewritten by `tehas update` to its 2.0 form, beside the
  store's migration, with one line saying what changed. A 1.4.x
  configuration never fails to load. What the update rewrites, 2.0 no
  longer reads, so the rewrite is no lever. A test runs instance
  directories of every shape 1.4.x accepts, `instances/example/`
  included, through the migration and then through `lib/config.sh` and
  the settings loader. The real instances, which live outside this
  repository, are run through it by a person before the trio is tagged.
- **T9 has no renames table.** The changes of 2.0 are listed once, in
  "Changed in 2.0", with no old name kept beside a new one.
- **Classic stays.** `TEHAS_DRIVER=classic` and T13's fallback to it are
  a supported 2.0 driver, not a lever (the owner, 2026-11-22).
- **The unused features** of `docs/plans/2026-11-22-unused-features.md`,
  every row approved by the owner, 2026-11-22, one step per kind:
  - dropped in 2.0: the All hands label, the Spec marks, Outsource with
    the model proxy and the contractor, and the millwright's grant;
  - out of core in 2.0, as plugins in 2.1 with their plumbing kept:
    Panels, the Business page, Energy use, the Host panel (`tehas top`
    stays), the Review loop, the Product wiki, and a cycle's describe,
    text files, zips and comments (images stay).
  What the consumer's production pages use stays in core, since the
  owner dropped only what was never on a host (the trio's program
  manager, 2026-11-22):
  - the dock. Panels and the host bridge's menu share one mechanism,
    the dock of `host/fun/panels.js`. The dock and the bridge's
    `open: dock` items stay in core, moved to `host/fun/dock.js`; the
    Panels plugin takes only `tehas panel` and `live/panels.json`;
  - the business page's 17 rules that other pages share move to
    `host/fun/pages.css`, the one owner of them; the Business page takes
    only its own rules to the plugin.
  Outsource leaving 2.0 supersedes any plan of the manual for it in 2.1:
  T16 corrects the manual's words where they say so.
  Each has its line in T9's "Changed in 2.0", its settings handled by the
  configuration migration, and `cargo xtask config-check` covers them.
  A store table, column or stored key that exists only for a removed
  feature goes through a state-store step with a migration test, not left
  unread. The store's steps run one after another with no gap, so they
  land in number order: 28 the fixer-to-millwright rename, which also
  removes the `comment_links` key #2875 leaves in an approval's input, and
  29 Outsource's tables.

An inventory comes first, then one pull request per kind of thing
removed, each green alone, each lowering the counts and ceilings it
lowers. A pull request that drops or renames a setting, an environment
variable or a key carries its rewrite in `tehas update` and a test of
that rewrite.

Done when the inventory's items are gone or parked, and a final grep for
the 1.x names, the v0.30 paths and the retired settings finds only the
parked records and the history in `docs/` and `CHANGELOG.md`, and every
real instance's configuration loads on 2.0 after `tehas update`.

### T9. The contract for 2.0

- `host/docs/contract.md` for 2.x, with a section "Changed in 2.0" that
  lists every change and its readers.
- The `api/` files for 2.0.
- A changelog fragment with an adopter action.
- With the millwright's grant gone, the one way a program starts work for
  a person is `tehas start`, called for a signed-in person; the contract
  names it as that entry.

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
- T10 to T16 are done, and the trio's gate holds.

Then, when the trio is tagged, the pull request into main (a merge
commit or a fast-forward, as "Finish" says), and the owner's release of
v2.0.0.

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
| 2026-11-22 | The trio, the adapters, three ports, events (T11), the rename (T10), the pi-rukas pin, no tags from the branch and the merge train added to this plan | #2776 |
| 2026-11-22 | T0: the shared architecture test (sha256 211f1167 and b66f64af), `architecture.toml` at the fork's counts, the rubric, the review questions and the knowledge map; the baseline grade (the overseer's three fresh graders: R1 to R7 partial, R8 and R9 mostly, below adopting until the test runs in CI) | #2778 |
| 2026-11-22 | T0: checks for R3, R6, R7 and R8; tests counted; the checks as wide as their rules | #2780 |
| 2026-11-22 | T3 begins: the closing pull requests of an issue typed (`ClosingPr`) | #2781 |
| 2026-11-22 | The architecture test runs in CI on the branch; the AGENTS.md section merged by the owner | #2777, #2779 |
| 2026-11-22 | T12 (answers in words, #2782) and the pi-rukas pin (v0.14.101 or later) added to this plan; T3 planned as fifteen slices | #2783 |
| 2026-11-22 | T13: lean is the default driver in 2.0.0 (the owner) | #2784 |
| 2026-11-22 | The ending: squash merges only, so the branch lands by a merge commit the owner allows or a fast-forward | #2785 |
| 2026-11-22 | T13: the fallback to classic on lean's setup refusal; the pi-rukas blockers as #2493 lists them | #2798 |
| 2026-11-22 | T13: the fallback leaves a host message; the adopter action is "set `verify.roster`, or `TEHAS_DRIVER=classic`" (the owner) | #2801 |
| 2026-11-22 | T3.1 to T3.5: host scalars, branch check and push fault, issue lists, park rows, refresh inputs typed | #2786, #2788, #2793, #2796, #2805 |
| 2026-11-22 | T5 part 1: `Home` owns the `state/`, `live/` and `run/` layout; core, plugin, mcp, contractor, proxy, analytics, the console, store, ops and cycle read from it | #2787, #2789, #2795, #2802, #2803, #2804, #2806, #2807 |
| 2026-11-22 | Port main @ c4b1b8ed (url 2.5.9 for RUSTSEC-2026-0592, #2791) and @ ea6fa29d (lean runs the workspace's Rust tests, #2797, its `.pi/` change confirmed by the owner) | #2792, #2800 |
| 2026-11-22 | A port of main lands by a fast-forward of the branch to the port's head, never a squash, so main stays in the branch's history | #2792 |
| 2026-11-22 | T12.1: `tehas answer`, and `--text -` from standard input | #2794, #2808 |
| 2026-11-22 | T13 part 4: lean's fallback to classic keys on pi-rukas's typed setup refusal (pi-rukas#2511), with a host message | #2798, #2801 |
| 2026-11-22 | A flaky test fixed: the two writes reports compared without their minute | #2799 |
| 2026-11-22 | T14: the Protolab removed as dead code, its file storage and serving kept for plugins (the owner) | #2826 |
| 2026-11-22 | T15: shell tests to Rust (#2816 to #2824); the migration behind the ports lands after the rest of the refactor, before v2.0.0 (the owner) | #2826 |
| 2026-11-22 | T13: the pin is pi-rukas 0.15.0; a crash is no setup refusal, and the fallback clears lean's state of the issue | #2832 |
| 2026-11-22 | T14: the stage stays as Idea, an open ticket not in line; the Protolab parked, with a record (the owner) | #2835 |
| 2026-11-22 | T16: clean house, no backwards compatibility, the store keeps only the 1.4.x migration, no renames table in T9 (the owner) | #2837 |
| 2026-11-22 | T3.6 to T3.12: issue view, park and question records, the work record, the edit record, comments, the answer and park note, the approval record, the slice read, typed and ported onto `tehas answer` | #2810, #2811, #2812, #2814, #2815, #2825, #2831, #2833 |
| 2026-11-22 | T12.1c and T12.1d: the feature word `answer`, and `tehas question` | #2813, #2830 |
| 2026-11-22 | T4: the ports spec, read and accepted in writing by the consumer that runs many factories | #2828 |
| 2026-11-22 | Port main @ abc4afca (a checkout follows pi-rukas's 0.15 line, #2834) | #2836 |
| 2026-11-22 | T16: every row of the unused features approved; classic stays a driver; T10's rename to tehas-factory confirmed (the owner) | #2838 |
| 2026-11-22 | T13: lean by default covers issues with a spec block; spec-less issues stay classic in 2.0, the rest is #2839 for 2.1 (work-a7) | #2840 |
| 2026-11-22 | The pin: the newest pi-rukas on the 0.15 line at readiness, not 0.16 (the trio's program manager) | #2844 |
| 2026-11-22 | T3.13 to T3.15: the demand typed, an explicit plan in place of `_line`, the actions an `enum Action`. T3 is done: `decide` and `Snapshot` carry no `Value` in a type or signature | #2847, #2851, #2852 |
| 2026-11-22 | T16: removed features' settings load, warn once and are dropped (`IGNORED`) | #2846 |
| 2026-11-22 | Port main @ 40c97b24 (`xtask lean-test` answers lean's runner probe, #2845) | #2848 |
| 2026-11-22 | T15: the clean shell tests are Rust tests | #2842 |
| 2026-11-22 | The minimum pi-rukas is v0.15.2, for the roster's exit (found by T13's proof); lean refuses an older image | #2861 |
| 2026-11-22 | T15: the index and mcp, top, and headless shell tests are Rust tests; the raises they need are recorded in `docs/architecture.md` under "Temporary rises" | #2858, #2859, #2860 |
| 2026-11-22 | T16: the product wiki leaves core; its renderer and collector stay as crates | #2862 |
| 2026-11-22 | T12.2 and T12.3: answers in words, no boxes, and nothing reads a tick | #2843 |
| 2026-11-22 | T16: energy use leaves core | #2864 |
| 2026-11-22 | `docs/architecture.md` records each check's fork value | #2865 |
| 2026-11-22 | T15: the analytics command and preview choose shell tests are Rust tests | #2866 |
| 2026-11-22 | T16: the review loop leaves core; the review's container kind and its argv fixtures go (a narrowing) | #2857 |
| 2026-11-22 | Analytics schema 5 drops `samples.watts`, pinned by the consumer that reads the analytics copies before the merge | #2869 |
| 2026-11-22 | T4's Done clears every row of "Temporary rises" | #2870 |
| 2026-11-22 | T12.4: `retry-tick` is `reply` in both stores, and a question open at the upgrade is asked again once in 2.0's words | #2867 |
| 2026-11-22 | T15: the install and bootstrap shell tests split so the quick check runs fewer of them | #2868 |
| 2026-11-22 | The minimum pi-rukas is v0.15.4, for lean's base probes in a cycle container (found by T13's proof) | #2873 |
| 2026-11-22 | T16: describe, text files, zips and comment links leave core | #2875 |
| 2026-11-22 | The minimum pi-rukas is v0.15.5, so lean's trees stay out of a product's commits | #2882 |
