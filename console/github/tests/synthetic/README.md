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

## A live answer moved into the fixture project

A recording of another project, with only what puts it in the fixture
project (`acme/widget`, repository id 7001) changed. The replay of
`tests/replay/` reads these as it reads `recorded/<item>/`.

Source of every file in this section:
`console/github/tests/recorded/live-parent/parent-2824.json`, kit run #1
(2026-11-26, step `parents` re-recorded at 18:50Z), GraphQL's answer
for the parent of issue 2824 of a public repository.

| File | Changed from the recording |
| --- | --- |
| `issue-21/parent.json` | The alias `n2824` is `issue`, the key of the adapter's question for one issue; `number` 2824 is 21 and the parent's `number` 2597 is 20; both `databaseId` 1411216280 are 7001. The parent's `nameWithOwner` is the recording's: the adapter tells the project by its id |

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
