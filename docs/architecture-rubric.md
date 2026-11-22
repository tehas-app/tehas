# Architecture rubric

What a grader reads. The grader sees the code at one commit and this
page, and for R10 the pull requests since the last grade. Nothing else:
not the plan, not the session that wrote the code.

Copied from tehas-docs, `wiki/decisions/architecture-method.md` at
commit 7a281f3: §3 (the rules) and §9.2 (grading). The bodies are
verbatim. Only the headings were renamed: "3. The rules" is "The rules
(§3)", and "9.2 Grading a repository" is "Grading (§9.2)", one level up.
The relative links in the copy resolve against tehas-docs
`wiki/decisions/`, not this repository. The checks that hold what a grep
can hold are in `architecture.toml`; run them with
`python3 -I scripts/architecture.py --force`.

How this repository applies R10: the grader gets, beside the code, the
diffs of every pull request merged into `refactor/coding-standard-v2`
since the last grade (the "Port main" merges included, the plan left out)
and the gate's result on each, and grades R10 from them. A person
confirms the grade.

## The rules (§3)

Each rule says what must hold, the known practice it comes from, and how
it is checked. "Must" is required for a conformant repository; "should"
is the default, departed from with a reason written in the repository's
`docs/architecture.md`. Where a check is mechanical, it runs in CI (§5).

**R1. One owner for each piece of knowledge.** Must. A supplier's
paths, field names and query strings, a file's layout, a set of state
words, a route list: each is written in one module. A change to one
piece of knowledge touches its owner and nothing else.
*From:* information hiding (Parnas, 1972); Ousterhout's information
leakage; tehas-industry's rule of 2026-11-09, "each job has one owner".
*Checked:* the outside names (a host name, a path prefix, a label
word) are listed per repository and may appear only in their owner.

**R2. Outside systems behind ports, in our terms.** Must. A program
names what it needs (`tickets_of(factory)`, `ask(chat)`), not how a
supplier serves it (`call(GET, "/repos/…")`). The adapter is the only
module that knows the supplier. A port is general enough for two
suppliers and no further.
*From:* ports and adapters (Cockburn); Ousterhout's somewhat
general-purpose modules, held back by Fowler's speculative generality.
*Checked:* no port method takes a method, a path, a URL or untyped JSON.

**R3. A pure core, effects at the edge.** Must. Decisions are pure
functions with their own tests. The shell reads, asks the core, then
writes. Where losing work to a crash matters, the shell writes what it
will do before doing it (a journal), so a restart picks up.
*From:* functional core, imperative shell (Bernhardt); tehas's decide
then apply (ADR 0003).
*Checked:* the core's modules (or crate) do not depend on the network,
clock, file or process libraries.

**R4. Typed seams, parsed at the edge.** Must. What crosses a seam is a
type of ours. Outside data is parsed into those types where it enters,
every field the program reads required, unknown fields ignored, and what
fails to parse is refused and shown as unreadable. No `serde_json::Value`,
JavaScript object of unknown shape or argv string travels past the
adapter. Make wrong states impossible to build where the type system
allows it.
*From:* parse, don't validate (King); fail closed (the AGENTS.md of
tehas-industry and tehas-obeya).
*Checked:* no `Value` in the signature of a port, a core function or a
public item outside the adapters; a count kept until it reaches zero.

**R5. Deep modules.** Should. A module's public surface is small
against what it hides. A module that only passes calls on is folded
into its caller or its callee. A module grouped by what kind of code it
is ("all the pure functions", "all the helpers") is regrouped by what it
knows.
*From:* Ousterhout (deep modules, pass-through methods, decomposition
by knowledge, not by time order).
*Checked:* the public items of each module are counted in the knowledge
map (§4); a rise is a question in review, not a failure.

**R6. Failures handled where they can be.** Must for adapters. An
adapter sets its own time limits, retries what is safe to retry, keeps a
supplier's budget, and answers its edge cases as normal results (a 404
on removing a label counts as done). A failure it cannot absorb becomes
one of a few named outcomes of the port (`Unreachable`, `SignIn`,
`Refused`), reported once, never a raw status passed up.
*From:* Ousterhout's define errors out of existence and pull complexity
down; time limits and bulkheads, common practice in services.
*Checked:* a fake of each port that times out, refuses and answers
oddly, and a test that the program carries on with one report (§3 R7).

**R7. Tests at the interface.** Must. A module is tested through its
public interface and fakes of its ports. Tests do not pin a supplier's
call shapes (argv, paths) except in the adapter's own tests, which
check it against recorded samples of the supplier's real answers (a
contract test). The tests come before the moves they protect.
*From:* Fowler (tests before refactoring); consumer-driven contract
tests; tehas's rule "stub the outside world".
*Checked:* fakes implement the port trait; only adapter tests name the
supplier's paths.

**R8. Dependencies point inward.** Must. The core depends on nothing of
the shell or the adapters. Adapters depend on the port types, never the
other way. Where a repository has more than one crate, the crate graph
enforces it; in one crate, the module rule is checked by a test.
*From:* the dependency rule of ports and adapters.
*Checked:* `cargo metadata` (crates) or a grep of `use` lines (modules).

**R9. Contracts change by expand, then contract.** Must. A new term goes
in beside the old one in a minor release, callers move to it, and the
old term goes in the next major release. A major release that mostly
deletes is the safe shape. Inside a repository the same holds for a
move: the new module goes in beside the old (branch by abstraction), and
the old is removed once nothing calls it.
*From:* Fowler's parallel change and branch by abstraction; semantic
versioning.
*Checked:* each repository's contract file lists what is frozen and
since which release.

**R10. Small, behaviour-keeping steps.** Must. A move keeps behaviour,
lands as an ordinary ticket and pull request, and is green on its own.
A move earns its place by making a named change easier ("make the change
easy, then make the easy change", Beck in Fowler). No big rewrite.
*From:* Fowler; [remanufacturing](../components/remanufacturing.md),
which is this kind of work.
*Checked:* in review.

The ideas each rule draws on are known practice; their wording here is
the session's, from its knowledge of the sources, not re-read
[UNVERIFIED for exact wording].

## Grading (§9.2)

Each rule is graded on four bands:

| Band | Score | What it means |
| --- | --- | --- |
| `absent` | 0 | The code breaks the rule widely, and nothing measures it |
| `partial` | 0.25 | The rule holds in some modules only, or its breaks are not all listed |
| `mostly` | 0.75 | The rule holds in most modules, and every break is listed, each with its count in the architecture test or a line in `docs/architecture.md`. Widespread breaks are `partial` even when listed |
| `met` | 1 | The rule holds, or holds to a written floor, and CI checks it where it can be checked |

What each band asks, rule by rule:

| Rule | Measured by | `met` when |
| --- | --- | --- |
| R1 One owner | Owned-names check (§5); the knowledge map | Every listed piece of knowledge has one owner and the check is at zero |
| R2 Ports in our terms | No method, path, URL or untyped JSON in a port's signature | Every outside system is behind a port; adapters alone name the supplier |
| R3 Pure core | Dependency check on the core's modules | The core uses no network, clock, file or process library |
| R4 Typed seams | Untyped-seam count (§5) | The count is zero; parse failures are refused and shown as unreadable |
| R5 Deep modules | Public items per module in the knowledge map; review | No module grouped by kind of code; no pass-through left; departures written |
| R6 Failures in the adapter | The failure fakes: time out, refuse, answer oddly | Each port's failure fakes leave the program running with one report |
| R7 Tests at the interface | Fakes implement the ports; supplier paths only in adapter tests; contract tests | Every port has a fake and every adapter a contract test from recorded samples |
| R8 Inward dependencies | `cargo metadata` or the module check | CI holds the direction |
| R9 Expand, then contract | The contract file | Every frozen term is listed with its release; nothing removed outside a major |
| R10 Small steps | Review of the pull requests since the last grade | The moves since the last grade were behaviour-keeping and reviewable alone |

How far the rule holds and whether its breaks are listed both count: listing alone does not lift a band [FROM owner, 2026-11-22, after two sets of graders read `mostly` differently at tehas-industry's I0].

**The grade is the ten bands, not a sum** [SPEC]. A total would let a
`met` hide an `absent`. Two pass lines:

- **Adopting:** every rule at `partial` or better, the architecture
  test in CI, the knowledge map written.
- **Conformant:** every must rule (R1 to R4 and R6 to R10) at `met`, R5
  at `mostly` or better.

**When and by whom.** A repository is graded at adoption (its
baseline), before each release, and whenever its owner asks. The
mechanical bands come from the architecture test. The judged bands (R5,
R6, R10, and the parts of the others the test cannot see) are scored
by a reviewer who sees only the code at that commit and this rubric,
not the plan or the session that wrote the code, so the grade is not
the author's own reading [INFERENCE: the fresh reader of
[EGM](../components/egm.md), applied to grading]. A person confirms the
grade. Each grade names the band and the evidence for it: a count, a
file and line, a test.

**Where it is kept.** A table in the repository's `docs/architecture.md`,
one row per grade: the date, the commit, the ten bands, the pass line
reached. Rows are appended, never rewritten, so the trend shows. The
rubric itself is copied into each repository as
`docs/architecture-rubric.md`, from this page at a dated version, so an
agent there needs nothing from this wiki.

**Release gates.** A part's 1.0.0 and a major release are not tagged
below **conformant** unless the owner says otherwise and the
departure is written. A minor release may be tagged at any grade, but
no band may fall below the last grade's [SPEC].

**Cycle scoring.** The rubric is written in cycle scoring's format so
the industrial engineer could later grade merged cycles against R1 to
R10 as well, with the review questions of §9.1 as what the reason must
name [IDEA].
