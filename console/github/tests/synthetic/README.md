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
`recorded/anonymous/`. Each is made from a live answer to an anonymous
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
