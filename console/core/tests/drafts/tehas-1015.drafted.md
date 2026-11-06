EPIC: EPIC

_Planned in pi-rukas plan session 5ceedea1-96e5-4530-a6db-0af2032a372c_

## Context & motivation

Descriptor: EPIC: Issue titles and bodies tehas writes must summarise the issue
Type: epic

## Prior context inventory

- # EPIC: Issue titles and bodies tehas writes must summarise the issue [context param]
- ## Problem [context param]
- When tehas writes an issue's title or body, it does not check that the text describes the issue. A bad draft from the planner replaces good text. [context param]
- Example: trail-openers/nessie#1184. A person titled it "EPIC: Slack channel turns explain their read-only tools instead of reporting an outage". At 14:31Z the factory bot ran "Update this issue" on a plan and: [context param]
- - renamed it to `EPIC: EPIC` [context param]
- - replaced the body with the brief echoed back: a `Descriptor:` line and a "Prior context inventory" that repeats every line of the old body with `[context param]` after it. [context param]
- The person had to restore the title by hand. [context param]
- Cause: `console/src/plan.rs` `update` takes the draft's `title` and `spec` as they are. The only fallback is for an empty title. `console/dispatch/src/apply/plan.rs` `plan_update` then runs `gh issue edit --title --body-file`. Nothing checks that the title is a summary of the body, or that the body is a plan and not the brief echoed back. [context param]
- ## Goal [context param]
- Every place where tehas creates or rewrites an issue's title or body writes a title that is a short, specific summary of the body. If the new text fails the check, the old text stays, and the issue gets a comment that says why. [context param]
- ## Writers to cover [context param]
- | Path | What it writes | [context param]
- | --- | --- | [context param]
- | `console/src/plan.rs` `update` -> `console/dispatch/src/apply/plan.rs` `plan_update` | title + body of the issue being planned ("Update this issue") | [context param]
- | `plan --confirm` ("File as a new issue") | title + body of a new issue (filed by pi-rukas) | [context param]
- | `console/dispatch/src/apply/plan.rs` (other `edit` around L779), `console/dispatch/src/apply.rs` L474 | check whether title or body is written | [context param]
- | `libexec/improve` `gh issue create` | title + body of loop findings | [context param]
- | `console/store/src/review_issue.rs` | title + body of review issues | [context param]
- | refresh / auto-refine (#329, #679) | rewritten bodies | [context param]
- A body is refused when it echoes the brief (`[context param]` markers, a `Descriptor:` line, a "Prior context inventory"). [context param]
- 2. Use the check in plan update. If the draft fails, keep the old title and body, post the draft as a comment, and leave the session open. [context param]
