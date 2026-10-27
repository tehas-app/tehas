## How to write the plan: tehas

The plan you draft becomes an issue that an unattended `/work` cycle builds.
The operator may make it the text of this very issue, in place of the request
above, so it must stand on its own: keep everything the request says that
still holds, and leave nothing out that the cycle needs.

1. Ground it in the code first. Use the code index and read the files. Find
   the functions, scripts, tests and manual pages the request touches. Every
   claim you write must be one the next agent can confirm at file:line on the
   default branch today. Never say a file or symbol does not exist, moved or
   lives elsewhere unless you checked. If the work is already on the default
   branch, say so with the commit, and plan nothing.

2. Leave out of the plan: anything under `.github/` or `.pi/`, `CODEOWNERS`,
   `agents.json`, the project's verify command and gate scripts, and
   `AGENTS.md`. A cycle may not change them. Name them as work for a person.

3. The spec, in this order, under 4000 characters:
   - One sentence: the outcome.
   - "Deliverables": a numbered list. Each item says what changes, then
     `[paths: `a`, `b`]` with exact repository paths in backticks. A new file
     whose name is decided: its path, then ` (new)`. Never a pattern, never
     `n/a`, never prose in the path list.
   - "Acceptance criteria": one testable bullet each, naming the test file and
     what the case checks.
   - "Out of scope": the nearby files the cycle must not touch, by path.
   - "Evidence": up to five lines of `claim - path:line`.
   Nothing else: no inventory of the request, no notes on how you searched.

4. Write decisions, not options: "Use X, not Y, because Z". The cycle cannot
   ask. When a decision is the operator's, ask it as a question in this session
   before you draft.

5. Classify the plan in its title, as a conventional-commit type and a short
   summary: `feat: ...` for new behaviour, `fix: ...` for a bug, `chore: ...`
   for housekeeping, or `docs:`, `test:`, `refactor:` when that is all it is. A
   scope may follow the type, as in `fix(dispatch): ...`. Keep a type the
   request's title already has. The pull request takes its title from this one.
