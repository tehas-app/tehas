# Synthetic answers

Answers GitHub cannot give on demand, so nobody can record them. A test
reads a file here in place of the recording of the same name under
`recorded/`.

## One fault in a live recording

Each is made by hand from a live recording of the same question, with
only the fault changed. Everything else (ids, logins, cursors, times,
shapes) is the recording's.

Source of every file in this section:
`console/github/tests/recorded/activity/live/events-whole-first.json`,
the live answer for pull request 15145 of a public repository. The node
numbers count from 0 in `data.repository.pullRequest.events.nodes`.

| File | Changed from the recording |
| --- | --- |
| `activity/null-node/events-whole-first.json` | Node 3 (dependabot's `LabeledEvent` "dependencies") is `null` |
| `activity/no-type/events-whole-first.json` | Node 3 has no `__typename` |
| `activity/no-time/events-whole-first.json` | Node 3 has no `createdAt` |
| `activity/no-login/events-whole-first.json` | Node 3's `actor` has no `login` |
| `activity/no-label/events-whole-first.json` | Node 3 has no `label` |
| `activity/no-commit-time/events-whole-first.json` | Node 0's (`PullRequestCommit`) `commit` is `{}`, no `committedDate` |
| `activity/no-review-state/events-whole-first.json` | Node 6 (the `COMMENTED` `PullRequestReview`) has no `state` |
| `activity/other-number/events-whole-first.json` | `pullRequest.number` is 15146, not the 15145 asked |
| `activity/no-page-info/events-whole-first.json` | `events` has no `pageInfo` |
| `activity/no-cursor/events-whole-first.json` | `pageInfo` is `{"hasNextPage": true, "endCursor": null}`: more follows, no cursor to it |

## One fault in a live aliased question

Each starts from a live answer of the same question and changes only what
makes the case. Their `locations` and `message` are the recording's.

Source of the `edits/` files: `console/github/tests/recorded/edits/live/`,
the live edit histories of `tehas-app/tehas-rec-edits-not-found` (kit run
#1, 2026-11-26). `bodies.answer.json` asks issues 1 (kept), 2 (deleted)
and 1001 (never made); `nodes.answer.json` asks comment
`IC_kwDOV9AT5c8AAAABrNMQ6A` (kept) and `IC_kwDOV9AT5c8AAAABrNMTAA`
(deleted), and REST reads comment 7194481408 (deleted). A not found at
an item is recorded there; one inside an item cannot be made on demand.

| File | Changed from the recording |
| --- | --- |
| `edits/inner-not-found/bodies.answer.json` | `bodies.answer.json`: the NOT_FOUND of `i2` has `path` `["repository","i2","userContentEdits"]`, inside the item |
| `edits/inner-not-found/bodies.fail` | `bodies.fail` as it is |
| `edits/inner-not-found/nodes.answer.json` | `nodes.answer.json`: node 0 is `null`, and a copy of the NOT_FOUND has `path` `["nodes",0,"userContentEdits"]`. Asked of the fixture project's two comments, whose REST reads give other node ids, so node 0's data is taken out |
| `edits/inner-not-found/nodes.fail` | `nodes.fail` as it is |
| `edits/missing-items/bodies.json` | `bodies.answer.json` with every alias and the errors taken out: an item missing from the answer |
| `edits/missing-items/nodes.json` | `nodes.answer.json` with `nodes` empty and the errors taken out |
| `edits/gone-repository/comment-7194480872.answer.json` | `comment-7194481408.answer.json` as it is, under the kept comment's id, asked of `tehas-app/tehas-rec-gone`, a repository that is not there: GitHub's 404 names neither, so it is the same answer |
| `edits/gone-repository/comment-7194480872.fail` | `comment-7194481408.fail` as it is, under the kept comment's id |

Source of the `what/` files: `console/github/tests/recorded/what/live/from-1.answer.json`
and `from-1.fail`.

| File | Changed from the recording |
| --- | --- |
| `what/limited/from-3.answer.json` | `data` as it is; `errors` one RATE_LIMITED error with no `path`. A spent budget cannot be made on demand and no real one is recorded, so the message is GitHub's documented text with no account id |
| `what/limited/from-3.fail` | `from-1.fail` as it is |

## The whole ticket read, cut to the depth that asks no history

The `Cut` ticket read asks no milestone and no history page, so its
question is a subset of the whole read's and the kit recorded only the
whole one: GitHub answers only what a question asks, and the cut
question was not asked of the live site. Each file is the issue's
recorded ticket answer with those two keys dropped from the issue node.

| File | Source and change |
| --- | --- |
| `issue-27/ticket-cut.json` | `recorded/issue-27/ticket.json`: `milestone` and `events` dropped from the issue node |
| `issue-28/ticket-cut.json` | `recorded/issue-28/ticket.json`: `milestone` and `events` dropped from the issue node |
| `issue-30/ticket-cut.json` | `recorded/issue-30/ticket.json`: `milestone` and `events` dropped from the issue node |

## A body that is not JSON

These are not GitHub's answers: they stand for a body gh hands back that
does not parse. Source: none, each is made by hand.

| File | Changed: a body that is not JSON |
| --- | --- |
| `activity/garbled/events-whole-first.json` | The text `not json` |
| `activity/branch/branch-garbled.json` | The text `not json` |
| `code-host/garbled/changes-OPEN.json` | A pull request list cut off inside `pageInfo` |
| `code-host/garbled/checks-e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1-1.json` | A check run list cut off inside the first run |
| `code-host/garbled/repo.json` | A repository cut off at `private` |
| `edits/garbled/bodies.json` | A GraphQL answer cut off after its first alias |
| `edits/garbled/nodes.json` | A GraphQL answer cut off inside `nodes` |
| `prepare-one/garbled/label.json` | The text `not json` |
| `vocabulary/garbled/labels-first.json` | The text `not json` |
| `vocabulary/garbled/types-first.json` | The text `not json` |
| `what/garbled/from-3.json` | A GraphQL answer cut off after its first alias |
| `what/failed-garbled/from-3.answer.json` | An HTML page, `<html>bad gateway</html>`, printed beside the failure in `from-3.fail` |
| `what/failed-garbled/from-3.fail` | The failure that HTML page came with, `: HTTP 502` |
