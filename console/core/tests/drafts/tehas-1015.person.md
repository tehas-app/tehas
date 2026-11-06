EPIC: Issue titles and bodies tehas writes must summarise the issue

## Problem

When tehas writes an issue's title or body, it does not check that the text describes the issue. A bad draft from the planner replaces good text.

Example: trail-openers/nessie#1184. A person titled it "EPIC: Slack channel turns explain their read-only tools instead of reporting an outage". At 14:31Z the factory bot ran "Update this issue" on a plan and:

- renamed it to `EPIC: EPIC`
- replaced the body with the brief echoed back: a `Descriptor:` line and a "Prior context inventory" that repeats every line of the old body with `[context param]` after it.

The person had to restore the title by hand.

Cause: `console/src/plan.rs` `update` takes the draft's `title` and `spec` as they are. The only fallback is for an empty title. `console/dispatch/src/apply/plan.rs` `plan_update` then runs `gh issue edit --title --body-file`. Nothing checks that the title is a summary of the body, or that the body is a plan and not the brief echoed back.

## Goal

Every place where tehas creates or rewrites an issue's title or body writes a title that is a short, specific summary of the body. If the new text fails the check, the old text stays, and the issue gets a comment that says why.

## Writers to cover

| Path | What it writes |
| --- | --- |
| `console/src/plan.rs` `update` -> `console/dispatch/src/apply/plan.rs` `plan_update` | title + body of the issue being planned ("Update this issue") |
| `plan --confirm` ("File as a new issue") | title + body of a new issue (filed by pi-rukas) |
| `console/dispatch/src/apply/plan.rs` (other `edit` around L779), `console/dispatch/src/apply.rs` L474 | check whether title or body is written |
| `libexec/improve` `gh issue create` | title + body of loop findings |
| `console/store/src/review_issue.rs` | title + body of review issues |
| refresh / auto-refine (#329, #679) | rewritten bodies |

## Sub-issues (to cut)

1. One check in Rust that every writer calls before it writes. A title is refused when it:
   - is only a type prefix (`EPIC:`, `fix:`, `feat:`) or a repeated word, or has fewer than N words after the prefix
   - shares no words with the body's first heading or its opening section.

   A body is refused when it echoes the brief (`[context param]` markers, a `Descriptor:` line, a "Prior context inventory").
2. Use the check in plan update. If the draft fails, keep the old title and body, post the draft as a comment, and leave the session open.
3. Use the check in plan confirm, improve and review-issue filing.
4. A test for each writer, using the nessie#1184 draft as a fixture.
5. pi-rukas side: the planner should never produce a title like `EPIC: EPIC`, or a spec that is the context inventory. File this upstream once the tehas half is cut.

## Done when

- The nessie#1184 draft, replayed through "Update this issue", leaves the issue's title and body unchanged and posts a comment that says why.
- Every writer in the table either calls the check or the PR says why it does not need it.
- `host/docs/` describes what a person sees when a draft is refused.
