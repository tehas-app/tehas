# GitHub polls toward v1.2.0: cheaper, checked, woken

**Goal:** the dispatcher's poll asks GitHub only what changed, and asks it
cheaply. A host, or a push that comes later, can start a poll at once, so the
timer can run less often. A push is where this is going; the timer stays as
the fallback.

**Status:** planned for v1.2.0 (the owner, 2026-11-20). Item 2 shipped in
v1.0.1 (#2577). Each other item says whether it fits v1.2.0. Started
2026-11-19, at v1.0.0.

## Why

Every factory polls GitHub every two minutes, changed or not, and a cycle that
ends starts one more. The factories of one owner often share one GitHub App
installation, and so its 5,000 GraphQL points an hour. When the budget runs
out, every poll on that installation fails, and a factory whose polls fail
starts nothing.

Most polls find nothing new.

## What a poll costs, measured

GitHub's own figure for each query the poll sends, with
`rateLimit(dryRun: true) { cost }` against `tehas-app/tehas` on 2026-11-19.
The queries are as `console/dispatch/src/read/github.rs` and
`console/src/queue.rs` build them on main.

| Read | When | Points |
| --- | --- | --- |
| The queue's page (`PAGE`, 25 open issues), first page with the open pull requests and labels, with or without issue types | Every poll | 1 a page |
| The queue's later pages | Every poll, one per 25 open issues | 1 a page |
| The comments chunk (`comments_query`, 10 issues, each `comments(last: 100)` with `userContentEdits(last: 100)`) | Every poll, for the questions waiting, the blocked candidates and the parks that may lift | 10 a chunk, so about 1 an issue |
| The body edit histories (`edits_query`, 25 issues) | Candidates with an edited body | 1 a chunk |
| The blockers' states (`blocker_states`) | When a blocker comment names an issue | 1 |
| One pull request (`PULL`) | Each refresh request | 1 each |
| The bot's pull requests (`gh pr list --author`) | While cycles run | 1 |
| The status snapshot, `tehasbot status`: the issue list (200), the closed issues' closers (`shipped::QUERY`), the shipped count, the control issue's labels, the epics | After a poll that changed the queue, and at least every five minutes | about 9 together: 2, 3, 1, 1, 2 |
| The words of the breakroom (1,000 issues) | At most once an hour | about 10 |

The costly part is the comments chunk. Its price comes from
`userContentEdits(last: 100)` nested under `comments(last: 100)`: GitHub
prices up to 100 edits for each of 100 comments. Asked without the nested
edits, the comments of 10 issues cost 1 point, and of 25 issues also 1.

For the 44 open issues of `tehas-app/tehas`, with a handful of issues to read
comments of, that is about 10 to 15 points a poll and about 300 to 450 an
hour. A repository with 370 open issues pays about 15 points a poll for the
pages alone. Not measured on a host: the first item below makes it
measurable.

## Items

### 1. The poll records its points (v1.2.0)

**What.** Each GraphQL read of the poll also asks `rateLimit { cost }`, which
adds nothing to its cost, and the poll adds the costs up. The poll's row in
the state store keeps the sum beside `gh`. `tehas status` shows it on the
`poll` line, and `tehas api --json` gives it as `poll.last.points` and
`poll.hour.points`. The status snapshot logs its own points in its log line.

**Contract.** Two fields added to `tehas api --json`: the schema and its
samples in `api/api.schema.json`, the names on `api/surface-1.0.txt`. A new
word of `features`, `poll-points`, appended to `api/features.txt`, so a
program can tell an older factory from one that spent nothing. "What a poll
costs" in `host/docs/operating.md` says what the figure counts.

**Tests.** The sum over a poll's answers, an answer with no `rateLimit` that
counts as unknown, not as 0, the fields in the samples.

### 2. The comments chunk without the nested edits (shipped in v1.0.1, #2577)

**What.** The comments chunk asks each comment's `lastEditedAt` and
`editor`, as it does now, and no longer `userContentEdits`. A second query
reads the edit histories of only the comments that have a `lastEditedAt`, by
node id (`nodes(ids: [...])`, up to 100 a query, measured at 1 point). Most
comments are never edited, so most polls send no second query. The rules in
`console/dispatch/src/questions.rs` see the same `userContentEdits` as
before. A failed history read counts as an edit by nobody known, which holds,
as a failed read does today.

**Points.** A chunk of 10 issues from 10 points to 1, and the chunk can grow
to 25 issues. The largest saving of this plan, with no change in what the
rules decide.

**Contract.** None. The queries are internal.

**Tests.** The query asks no nested edits. A comment with a `lastEditedAt`
gets its history from the second query, one without gets none. A failed
second query holds as today. The answers the rules read are the same for an
issue read either way.

### 3. Comments only for issues that moved (v1.2.0 if the check below holds)

**What.** The queue's page asks each issue's `updatedAt`, which is measured
to cost nothing more. The poll keeps the last comments answer of each issue
with the `updatedAt` it was read at, and reads comments again only for an
issue whose `updatedAt` moved, or whose answer is older than a ceiling
(10 minutes suggested).

**The check first.** The rules hold on edits after an approval, so a reuse is
safe only if every change they read moves the issue's `updatedAt`: a new
comment, a comment edited, a comment deleted. Before this is built, try each
on a scratch repository and record the result here. If any one does not move
`updatedAt`, this item is dropped. Item 2 already takes most of the cost.

The body edit histories are not kept: they cost 1 point for 25 issues and
they are what an approval rests on.

**Contract.** None.

**Tests.** Reused when `updatedAt` is equal and the answer is younger than
the ceiling, read again when either is not, read again after a failed read.
Pure, with the times handed in.

### 4. A free change check before a timer poll (v1.2.0)

**What.** Before a poll started by the timer reads GitHub, it makes one REST
conditional request:

`GET /repos/{owner}/{repo}/issues?state=all&sort=updated&direction=desc&per_page=1`

with `If-None-Match` set to the last answer's `ETag`. Pull requests are
issues to this endpoint, so a label, a comment, a new issue or a push to a
pull request moves its first item. GitHub answers 304 when nothing changed,
and a 304 to an authorised conditional request is not counted. A 200 costs
one REST request and no GraphQL points.

On a 304 the poll ends at once as "unchanged", records that, and reads
nothing more. It never skips when:
- it was asked for: `tehas dispatch --soon` or a cycle that ended, both
  through `state/dispatch.again`. Item 4 has `tehas start`, `tehas refresh`,
  `tehas limit set` and `tehas order set` ask for a poll the same way, which
  they do not today, so a skip never delays what a person or a host asked;
- the check failed or was refused: a check it could not make counts as a
  change (rule 2);
- the last full poll is older than `TEHAS_POLL_FULL_SECS`.

A skipped poll starts nothing, so a skip can only delay work, never start
work that should not start. The ceiling bounds the delay for what the check
cannot see: a check run finishing, a label renamed, the clock passing a park
or a limit's expiry.

The status snapshot skips its GitHub reads on the same answer, as nothing it
reads has changed.

**Details.** `gh api` exits 1 on a 304, so the check reads the status line
(`gh api -i`), and a 304 is neither a failed call for health
(`tehas_core::github_fails`) nor a write for the shim's log. GitHub's answer
carries `Cache-Control: max-age=60`, so checking more often than once a
minute gains nothing.

**Contract.** A new tuning knob, `TEHAS_POLL_FULL_SECS`: whole seconds from
120 to 3600, default 120, so every poll is a full poll and an upgrade changes
nothing. Its row goes in `api/settings-rules.json` and on
`api/surface-1.0.txt`. As a knob, its default may change in a later minor
release, and the release notes say so. A new word of `features`,
`poll-check`, and an added field `poll.hour.unchanged` in `tehas api --json`
with its schema and samples. `tehas status` shows unchanged polls on the
`poll` line.

**Tests.** The ETag round trip: 200, then 304, then 200 with a new first
item. Whether a poll skips is a pure function of who started it, the check's
answer, the last full poll's age and the ceiling, with a case for each line
above. A 304 not counted as a failed call. A malformed `TEHAS_POLL_FULL_SECS`
stops every command with 78.

### 5. The wake in the factory API (v1.2.0, #2436)

**What.** `tehas dispatch --soon` exists on main: it writes
`state/dispatch.again`, and `tehas-dispatch.path` starts a poll now, or after
the one that runs. #2436 makes it part of the factory API:
- a section in `host/docs/api.md`, "The wake", beside the other asks: what it
  does, that it returns at once, that a poll already running is followed by
  one more, and that a paused or drained factory takes the ask and its poll
  starts nothing new;
- the word `wake` appended to `features`, so a caller can tell whether a
  factory has it;
- its exit statuses in the table of `tehas dispatch` in
  `host/docs/contract.md`: 0 asked, 1 the file could not be written, 64, 78;
- tests for the ask and the listing.

A woken poll is never skipped by item 4, so a host that knows the repository
changed gets a full poll at once.

**Contract.** The `--soon` flag is on `api/surface-1.0.txt` already. The new
part is the word `wake`. A `--json` form is not needed: the exit status is
the answer. If one is wanted, it is a flag added to `api/commands.txt` with a
schema in `api/`.

### 6. A longer interval where a wake is in place (after v1.2.0)

With items 4 and 5 in place, a host that wakes its factory can set
`TEHAS_POLL_SECS` to 1800 and `TEHAS_POLL_FULL_SECS` to 1800, and the timer
becomes a safety net. The defaults stay at 120 in v1.2.0. Raising them for
everyone waits for a release in which the wake has been seen working on
hosts, and the release notes say so.

### 7. A wake that names issues (later, not v1.2.0)

A push knows which issue changed. A later wake could carry the numbers, so the
poll reads comments only of those, on top of item 3's kept answers. It needs
item 3 and a new flag, and it is not needed for the savings above.

## How this lines up with a push later

tehas runs no server that takes requests, and that does not change. A push
reaches a factory through whatever fronts its host: a program there that
receives the push, verifies it, and runs `tehas dispatch --soon` as the
factory's user. That is item 5. tehas needs nothing more for a push:
- the push wakes a full poll at once;
- item 4's check, on the timer, catches what a lost delivery missed;
- items 2 and 3 keep each poll cheap, woken or not.

## Order of pull requests

Small and independent, each with its tests and its manual page:

1. Item 2, the comments chunk without the nested edits. No contract change.
2. Item 1, the poll's points, with `poll-points`.
3. Item 5, the wake in the factory API, with `wake` (#2436).
4. Item 4, the change check and `TEHAS_POLL_FULL_SECS`, with `poll-check`.
5. Item 3, after its check on a scratch repository.

## Out of scope

- The queue's page size. It is load-bearing for correctness (`ISSUE_PAGE`)
  and costs 1 point a page.
- A server in tehas for webhooks.
- The REST and search budgets, beyond the check's own request.
- What the rules decide. Every item reads the same facts, at less cost.
