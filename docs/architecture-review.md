# Architecture review

The questions every pull request is reviewed against, once the
repository's `.tehas/coding-standard` says `v2`, and on every pull
request into `refactor/coding-standard-v2` before then: one per rule of
`docs/architecture-rubric.md` (from tehas-docs,
wiki/decisions/architecture-method.md, section 9.1). The same in every
repository; this repository's own departures are in `docs/architecture.md`.

The architecture test (`scripts/architecture.py`, CI job `architecture`)
answers what is mechanical, and its summary on the pull request shows
each count against its limit. Do not check by hand what it already holds.
The questions below are for what it cannot see.

A "yes" to a question is either fixed in the pull request, or answered in
it with a reason. A reason that holds becomes a raised `max` with its
`why` in `architecture.toml`, or a line in `docs/architecture.md`.

| Rule | The question |
| --- | --- |
| R1 One owner | Does a piece of knowledge this change touches now live in more than one module? |
| R2 Ports in our terms | Does code outside an adapter now name a supplier's path, field, header or command? |
| R3 Pure core | Does a decision now read the clock, the network, a file or the environment? |
| R4 Typed seams | Does untyped data (`Value`, an object of unknown shape, argv) now cross a seam? |
| R5 Deep modules | Did a module's public surface grow more than what it hides? Is there a new pass-through? |
| R6 Failures in the adapter | Does a supplier's failure now reach a caller as a raw status, or an error the caller must interpret? |
| R7 Tests at the interface | Does a new test pin a supplier's call shape outside the adapter's own tests? Does a move come with its tests first? |
| R8 Inward dependencies | Does the core now depend on the shell or an adapter? |
| R9 Expand, then contract | Does a contract term change or go without its parallel step and the release that allows it? |
| R10 Small steps | Does the change alter behaviour where it says it keeps it, or is it too large to review on its own? |

The code style is not reviewed here; it stays as `AGENTS.md` says.
