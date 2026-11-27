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

## A live parent answer for the fixture project

| File | Source and change |
| --- | --- |
| `issue-21/parent.json` | Byte for byte the kit run #1 (beta2b, 2026-11-26) recording `recorded/issue-28/parent.json` of the parents' question, manifest `beta2b-manifest.sha256` sha256 `d7b1c28605aba6491a50c27d40e91cc0fdb76e7c4d1df4b2274895e97b62239b`: nothing changed. tehas-a4's ruling of 2026-11-26: the stub of 21's parent is a real recorded answer, never a hand-renumbered one. The hand-made `recorded/issue-21/parent.json` it replaces is removed |

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

## anonymous/

Read by `tests/anonymous.rs` through `Stub::synthetic(<case>)`, before
`recorded/anonymous/`, and `json-asset/` by `tests/anonymous_download.rs`. Each is made from a live answer to an anonymous
reader (kit run #1, 2026-11-26) under
`console/github/tests/recorded/anonymous/`, named in the second column.

| File | Source | Changed from the recording |
| --- | --- | --- |
| `anonymous/no-location/repos_tehas_app_tehas_rec_releases_releases_latest` | `repos_tehas_app_tehas_rec_releases_releases_latest` | A redirect with no place to go, which GitHub cannot be made to send: the status `200` is `302`, no `location` header, the release body kept |
| `anonymous/chain/repos_tehas_app_tehas_rec_releases_releases_latest` | `repos_tehas_app_tehas_rec_releases_releases_latest` | The first of two redirects, which GitHub cannot be made to send: the status is `302`, and a `location: https://api.github.com/repositories/1473248788/releases/latest` header follows it |
| `anonymous/chain/repositories_1473248788_releases_latest` | `repos_tehas_app_tehas_rec_releases_releases_latest` | The second redirect: the status is `302`, and a `location: https://api.github.com/repos/tehas-app/tehas-rec-releases/releases/latest` header follows it |
| `anonymous/pre/repos_tehas_app_tehas_rec_releases_releases_latest` | `repos_tehas_app_tehas_rec_releases_releases_latest` | GitHub never answers `latest` with a prerelease: `"prerelease":false` is `"prerelease":true` |
| `anonymous/spent/repos_tehas_app_tehas_rec_releases_releases_per_page_100` | `repos_tehas_app_tehas_rec_releases_releases_per_page_100` | A spent budget, which cannot be made on demand: the status `200` is `403`, `x-ratelimit-remaining` `37` is `0`, `x-ratelimit-used` `23` is `60`, the body is GitHub's rate-limit message and `content-length` its 275 bytes |
| `anonymous/busy/repos_tehas_app_tehas_rec_releases_releases_per_page_100` | `repos_tehas_app_tehas_rec_releases_releases_per_page_100` | A 429 with no reset time and no `retry-after`, which cannot be made on demand: as `spent/`, with the status `429` and the `x-ratelimit-reset` line taken out |
| `anonymous/json-asset/repos_tehas_app_tehas_rec_releases_releases_assets_796197977` | `repos_tehas_app_tehas_rec_releases_releases_tags_v1_1_0` | The API's JSON for the asset in place of its bytes, which GitHub cannot be made to send: the release's headers kept, the body only its first asset's object as the release gives it, `content-length` its 1469 bytes |
| `anonymous/no-reset/repos_tehas_app_tehas_rec_releases_releases_per_page_100` | `repos_tehas_app_tehas_rec_releases_releases_per_page_100` | A spent budget with no reset time, which cannot be made on demand: as `spent/`, with the `x-ratelimit-reset` line taken out |
| `anonymous/bad-reset/repos_tehas_app_tehas_rec_releases_releases_per_page_100` | `repos_tehas_app_tehas_rec_releases_releases_per_page_100` | A spent budget whose reset is not a number: as `spent/`, with `x-ratelimit-reset: 1795704168` changed to `x-ratelimit-reset: soon` |
| `anonymous/secondary/repos_tehas_app_tehas_rec_releases_releases_per_page_100` | `repos_tehas_app_tehas_rec_releases_releases_per_page_100` | A secondary limit with its `retry-after` as a date, which cannot be made on demand: the status `200` is `403`, a `retry-after: Thu, 26 Nov 2026 14:15:16 GMT` header follows it, the body is GitHub's secondary-limit message and `content-length` its 235 bytes; `x-ratelimit-remaining` stays `37` |
