# Handover: tehas to coding standard v2, for v2.0.0

Date: 2026-11-21. Status: waiting for v1.4.0 ("Before the fork"). Start a
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

v2.0.0 carries other work too: putting GitHub behind adapters, and the
repository's new name. Those are planned on their own. This work builds
what the adapters need (the ports, with GitHub as their one adapter) and
does neither.

## Before the fork

1. **Wait for v1.4.0.** The tag is the owner's. This plan stays out of
   main until then, as a pull request that waits for the tag.
2. **Land this plan.** Once the tag `v1.4.0` exists:
   - bring the plan's pull request up to date with main, and let the gate
     pass;
   - a person merges it (AGENTS.md rule 9);
   - nothing else lands between the tag and that merge.
3. **The owner opens the branch to the gate.** `verify.yml` runs on pull
   requests into `main` and `release/**` only, and the gate files are
   changed by people (AGENTS.md rule 7). Before the first pull request
   into the branch, the owner either adds `refactor/coding-standard-v2`
   to `verify.yml`'s branches, or names the branch under `release/`.
   Until then, run `scripts/verify.sh` yourself on every pull request
   into the branch, and say so in it.
4. **The owner stops new work reaching the factory that builds tehas.**
   Its queue gets no new tickets while the branch is open.

## How the work runs

1. **Fork.** Create `refactor/coding-standard-v2` from main at the merge
   of this plan (v1.4.0 and this file), and push it. Write the fork
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
   - One pull request from `refactor/coding-standard-v2` into `main`,
     merged with **a merge commit**, not squashed, so the steps stay in
     main's history.
   - The release of v2.0.0, the tag and the release notes follow the
     usual release process. Tags and releases are the owner's.

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
| `home-layout` (`state/`, `live/` paths outside one module) | R1 | 570 | 0 |
| `untyped-signatures` (`Value` in a signature) | R4 | 996 | 0, or a floor written in `docs/architecture.md` |
| `untyped-fields` (`Value` in a public field) | R4 | 54 | 0, or a floor written |

The checks also read `bin/`, `libexec/`, `lib/` and the host's scripts
where a name can appear in bash. A floor above 0 is allowed only where
it is the honest answer, and is written down with its reason. Examples:
a security shim that must read `gh` arguments, or the adapter's own
parsing.

## The steps

In this order. T3 to T7 may overlap where a type and its port go
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

### T4. Two ports in tehas's terms

- **A ticket tracker port:** tickets, states, the account behind a change
  and its history, comments, blockers, milestones. It grows from `Forge`.
- **A code host port:** pull requests, branches, checks, releases, tokens.

GitHub is their one adapter, and it owns every `gh` argument, path,
document and field, the paging, the retries and the time limits. An
outage or a rate limit is handled there, and reported once. The security
shims stay where they are unless the owner agrees to merge them.

Design both ports twice: two independent drafts each, read by fresh
reviewers against R1, R2, R4, R6 and R7, and against how little of the
rest changes. Write the choice in a spec under `docs/specs/`, and update
`host/docs/forge.md`. Move one crate at a time.

Done when `github-rest` is 0 and `gh-argv` is at 0 or at its written
floor, and a fake code host that times out or rate-limits leaves the
dispatcher and a cycle running, with one report.

### T5. Deep homes for files

- A `Home` type owns the `state/` and `live/` paths.
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

### T9. The contract for 2.0

- `host/docs/contract.md` for 2.x, with a section "Changed in 2.0" that
  lists every change and its readers.
- Each rename as a row, in the way `api/renames-1.0.tsv` held 1.0's, so
  `tehas update` can list what an instance still uses.
- The `api/` files for 2.0.
- A changelog fragment with an adopter action.

Done when the 2.0 contract is written, and each change has its row and
its note to adopters.

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
- `scripts/verify.sh --full` passes, and the 2.0 contract is written.

Then the pull request into main (with a merge commit), and the owner's
release of v2.0.0.

## Open, for the owner

- The branch in `verify.yml`, or a `release/` name for it; and the
  architecture test as a gate job (before T0's first pull request).
- The AGENTS.md section of T0, which a person makes.
- Holding the factory that builds tehas to no new work while the branch
  is open.
- Whether the security shims that decide a `gh` write stay separate (T4).
- The regrouping of T6, once the map shows the crates.
- Each contract change T9 lists, before it is made.

## Log

Append a line per event: the fork, each step's pull requests, each port
of main, each grade.

| Date | What | Commit or PR |
| --- | --- | --- |
| 2026-11-21 | Plan written; the plan waits for v1.4.0 | |
