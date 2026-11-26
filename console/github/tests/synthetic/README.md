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

## Releases: one fault in a live recording

Source: `console/github/tests/recorded/releases/fixture/`, the live
answers about `tehas-app/tehas-rec-releases` (kit run #1, 2026-11-26).

| File | Changed from the recording |
| --- | --- |
| `releases/fixture/ref-v7.json` | Nothing: `ref-v1.0.0.json` as it is, the answer about `v1.0.0` given when `v7` is asked |
| `releases/fixture/release-v6.json` | Nothing: `release-v1.2.0.json` as it is, the answer about `v1.2.0` given when `v6` is asked |
| `releases/bad/latest.json` | From `latest.json`: `prerelease` is `true`, a prerelease named latest |
| `releases/bad/releases-1.json` | From `releases-1.json`: the draft `v1.3.0` has `published_at` `2026-11-26T14:10:01Z` (its `created_at`), not `null` |
| `releases/down/latest.fail` | From `releases/none/latest.fail`: `HTTP 502`, not `HTTP 404` |
| `releases/lying/release-v1.1.0.json` | Nothing: `release-v1.1.0.json` as it is, the release that lists the file at 99 bytes |
| `releases/lying/download-tehas-v1.1.0-linux-x86_64.tar.gz.sha256.bin` | From `download-tehas-v1.1.0-linux-x86_64.tar.gz.sha256.bin`: its last byte (the newline) cut, 98 bytes of a file listed at 99 |
| `releases/refused/release-v1.1.0.json` | Nothing: `release-v1.1.0.json` as it is, the release the refused download is of |
| `releases/refused/download-tehas-v1.1.0-linux-x86_64.tar.gz.sha256.fail` | From `release-v9.fail`: `HTTP 403`, not `HTTP 404` |
| `releases/short/tags.json` | From `tags.json`: the node of `v1.2.0` taken out, so 3 nodes where `totalCount` says 4 |
| `releases/tree/tags.json` | From `tags.json`: `v1.0.0`'s target has `__typename` `Tree`, not `Commit` |

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
