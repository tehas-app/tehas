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

## Stubs of an aliased question

Hand-made stubs, not recordings. Each starts from the live recording
`recorded/what/live/from-1.answer.json` (the one live answer of an aliased
question here; no edit-history question is recorded live yet) and changes
only what makes the case. Their `locations` and `message` are that
recording's, copied as they are.

- `edits/inner-not-found/bodies.json`: to be replaced by a live recording,
  see the kit list (`d`, edits-not-found). Live answer's error object, `path`
  set to `["repository","i7","userContentEdits"]` (a NOT_FOUND inside issue 7)
  and `["repository","i8"]`; data set to `i7` and `i8` null.
- `edits/inner-not-found/nodes.json`: to be replaced by a live recording,
  see the kit list (`d`, edits-not-found). As above, by node: `path`
  `["nodes",0,"userContentEdits"]` and `["nodes",1]`; data `nodes` two nulls
  and `repository` with no `id`, which the reader does not read and no live
  recording gives for the fixture repository.
- `edits/missing-items/bodies.json`: an item missing from the answer, which
  GitHub cannot be made to give. Live answer with every alias and the errors
  taken out.
- `edits/missing-items/nodes.json`: the same by node: `nodes` empty, beside
  `repository` with no `id` as above.
- `what/limited/from-3.answer.json`: a whole-question RATE_LIMITED, which
  cannot be made on demand without spending a real budget. Live answer's
  `data` as it is; its `errors` replaced by one RATE_LIMITED error with no
  `path`. No real rate-limit answer is recorded to copy from, so the message
  is GitHub's documented text with no account id.
- `what/limited/from-3.fail`: `recorded/what/live/from-1.fail` as it is.

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
