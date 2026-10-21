## How to plan this: tehas refine

This request was labelled `tehas:refine`. Plan it into one issue that a single
unattended `/work` cycle finishes. The cycle has these limits: explore 30
minutes, develop 90 minutes, and a review loop of at most a few hours (six
review lenses, one fix round, an adversarial re-check). A cycle that cannot
finish in that leaves a half-reviewed draft pull request. The request above may
be high level; your job is to turn it into precise work grounded in the code.

1. Ground it in the code first. Use the code index and read the files. Find
   the functions, scripts, tests and manual pages the request touches. Every
   claim you write must be one the next agent can confirm at file:line on the
   default branch today. Never say a file or symbol does not exist, moved or
   lives elsewhere unless you checked. If the work is already on the default
   branch, say so with the commit, and plan nothing.

2. Cut it into parts of one behaviour change each: about one or two code
   files, one test file, at most one manual page. A type or format change and
   all its readers are one part. A session files one issue, so plan the first
   part only, the one the others build on, and list the remaining parts, in
   order, under "Later parts". If the request is one part already, plan it
   whole.

3. Leave out of the part: anything under `.github/` or `.pi/`, `CODEOWNERS`,
   `agents.json`, the project's verify command and gate scripts, and
   `AGENTS.md`. A cycle may not change them. Name them under "Later parts" as
   work for a person.

4. The issue's spec, in this order, under 4000 characters:
   - One sentence: the outcome.
   - "Deliverables": a numbered list. Each item says what changes, then
     `[paths: `a`, `b`]` with exact repository paths in backticks. A new file
     whose name is decided: its path, then ` (new)`. Never a pattern, never
     `n/a`, never prose in the path list.
   - "Acceptance criteria": one testable bullet each, naming the test file and
     what the case checks.
   - "Out of scope": the nearby files the cycle must not touch, by path.
   - "Later parts": the remaining parts, one line each, in order.
   - "Evidence": up to five lines of `claim - path:line`.

5. Write decisions, not options: "Use X, not Y, because Z". The cycle cannot
   ask. When a decision is the operator's, ask it as a question in this session
   before you draft.
