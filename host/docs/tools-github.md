---
title: The GitHub commands
description: "Asking for refreshes, the App's tokens, and the writes of the last day."
---

# The GitHub commands

## tehas refresh

```
tehas refresh --pr 50       # ask for the refresh of pull request #50 now
tehas refresh --pr 50 --json  # the same, and say what it asked as one object
tail ~/tehas/logs/refresh.log
```

`tehasbot refresh` is the same command under its old name. It stays through
1.x, so a program that calls it keeps working, and prints the same words and
the same object.

It finds the factory's own `gh` whatever `PATH` the caller gives: it puts
`~/tehas/shim` and `~/tehas/bin` first on `PATH` before it reads GitHub, so a
caller without a login shell reads through the shim as a login shell would.
When no `gh` can be run at all, the reason says `gh not found` rather than
that GitHub did not answer.

Asks for the refresh of one of the factory's own pull requests that conflicts
with the branch: it writes a request, `~/tehas/state/refresh/<issue>.json`, for
the dispatcher to start (see [refreshes](/operating#refreshes)). Nothing runs it
on a timer: what schedules the factory's refreshes runs it for each pull
request. The poll reads a request of up to 4096 bytes; a larger file is not
read, counts as an unreadable request, and the poll's log names it with a
`WARN:` line. Deterministic, no model, no container.

It reads pull request N now and applies the refresh rule. A `CONFLICTING` or
`UNKNOWN` one gets its request, with a last field `"asked": "pr"`:

```
{
  "at": "2026-11-13T09:00:00Z",
  "pr": 50,
  "head_sha": "<40 hex>",
  "base_sha": "<40 hex>",
  "asked": "pr"
}
```

A request for the same pull request at the same head keeps its `at`. It
removes no other request. Its line in `~/tehas/logs/refresh.log` is
`asked for pull request #50: wrote the request of #7` or `asked for pull
request #50: not refreshed: <why>`. Exit 0 the request is written; 3 it is not
a pull request the factory refreshes, or it does not conflict, with the reason
on standard error and nothing written; 1 the read of GitHub failed or took
more than 20 seconds, or named no pull request, nothing written; 64 no `--pr`,
or N is not a whole number above 0, before any setting is read; 78 a setting
is missing or malformed.

With `--json` it also prints one object on standard output on exit 0, 3 and
1, so a caller does not read standard error for the reason:

```
{"api": 1, "asked": true, "pr": 50, "issue": 7, "headSha": "<40 hex>", "why": null}
{"api": 1, "asked": false, "pr": 50, "issue": 7, "headSha": "<40 hex>", "why": "it does not conflict: GitHub says MERGEABLE"}
```

`api` is the version of the object, 1, as in every `--json` object of the
[factory API](/api). `asked` is `true` when the request was written, and `why`
is then `null`; otherwise `why` holds the same words as standard error.
`issue` and `headSha`, the head commit of the pull request, are `null` when
GitHub did not name them, as when its read failed. Exit 64 and 78 print no
object. The old name `tehasbot refresh --pr N --json` prints the same object.

## tehas github check

```
tehas github check              # when the app's token runs out, and whether it reaches TEHAS_REPO
```

Makes one token of the app and prints the app, the installation, the bot's
login, when the token runs out and whether the installation reaches
`TEHAS_REPO`, and never the token. It reads the installation's repositories a
hundred at a time, page after page, until it finds `TEHAS_REPO` or a page is
not full. It is `tehasbot github-token --check` under its public name; the old form stays through 1.x. Exit 0 the check
passed; 1 no token could be had, or it does not reach `TEHAS_REPO`; 64 an
argument it does not know; 78 settings that are missing, malformed or name no
app.

## tehasbot github-token

```
tehasbot github-token           # print the app's token
tehasbot github-token --cycle   # print the cycle's token, narrowed to TEHAS_REPO
tehasbot github-token --session # the same body, for a planning turn or a manual session
```

What the gh shim, the poll and the cycles run for a token. A person checks
the token with [tehas github check](#tehas-github-check).

Makes the installation token of the GitHub App named by `TEHAS_GITHUB_APP_ID`.
The app's installation on `TEHAS_REPO` and its login are looked up at GitHub
with the app's key and kept in `~/tehas/state/github/installation` and
`~/tehas/state/github/login`. Each new token asks GitHub for the login again
and rewrites the file when the app was renamed, with a line on standard error,
and so does `TEHAS_BOT_LOGIN` for the rest of that command. A renamed app also
needs `console ids record` for its new login: until then `state/account-ids`
holds the old one, and the millwright's labels do not count. If the app is not
installed on `TEHAS_REPO`, the command names the repository and the install
link. It signs a JSON Web
Token with the app's key, `~/.config/tehas/github-app.pem` (mode 600, never
given to a container), and exchanges it at GitHub for a token that lasts an
hour. The token is kept in `~/tehas/state/github/token` (mode 600) and made
again when fewer than 10 minutes are left; one command makes it at a time.
The web token and the token go to curl on its standard input, never on its
command line. Without `--check` it prints the token and nothing else. `--check`
is [tehas github check](#tehas-github-check).

Exit 0 a token or a check that passed; 1 no token could be had, or it does not
reach `TEHAS_REPO`; 64 an argument it does not know; 78 settings that are
missing, malformed or name no app.

`--cycle` makes the cycle's token instead (#1814): GitHub is asked for a token
of the same installation for the repository of `TEHAS_REPO` alone, with
Contents and Pull requests write, Issues read and nothing else. It is kept
apart, in `~/tehas/state/github-cycle`, and made again the same way. A planning
turn and a manual session get a token of the same body, made when the command
starts and kept in `~/tehas/state/github-session` (`--session`). The command
asks for it again every 5 minutes while its container runs, so a session
longer than an hour keeps a good token.

With an app named, the host acts as the app. `env.sh` puts `~/tehas/shim`
first on `PATH`: its `gh` takes a token from `tehasbot github-token` at every
call and runs `~/tehas/bin/gh`, so a command that runs for hours never holds an
expired token. One exception: a call given `TEHAS_GH_TOKEN` in its environment
uses that as `GH_TOKEN` and runs no `github-token` at all. The dispatcher's
poll takes the token once at its start and gives it to every `gh` it runs that
way, and to `tidy`, `status` and `health` when it runs them, so twenty calls of
a poll ask for one token and not twenty (#938, #965). The name of the token is
not how the status knows the poll ran it: the poll gives the status
`TEHAS_FROM_POLL=1` as well, and nothing else takes that name (see
[tehasbot health](/tools-health#tehasbot-health)). The
poll holds that token in memory: it is never logged, never written to a file,
and never put in the poll's record. git's github.com credential helper is
pointed at the same shim, as environment-level config (no file is written), and
a `GH_TOKEN` in `~/.config/tehas/env` is refused (exit 78), so nothing acts as
the user it belongs to. The dispatcher, the refresh and the Protolab take the
bot's login from `~/tehas/state/github/login`: an installation token cannot ask
GitHub who it is.

A cycle's container is never given the app's own token, its directory or the
key. `tehas work` makes the cycle's token with
`tehasbot github-token --cycle` and gives the container
`~/tehas/state/github-cycle` read-only at `/run/tehas/github`, and the token as
its first `GH_TOKEN` (#1814). What a cycle pushes, opens and comments carries
the app's `<slug>[bot]` login, the same as the host's. `tehas work` renews the
cycle's token every 5 minutes while the cycle runs. If it cannot make the
token, no cycle starts. `GH_CYCLE_TOKEN` and `TEHAS_CYCLE_LOGIN` are refused
with exit 78.

The cycle's token cannot write issues. A cycle's `gh issue create`,
`gh issue edit` and `gh issue comment` are run by the host instead, with the
app's token, through the relay directory of the cycle (`/run/tehas/relay` in
the container). A comment goes only on the cycle's own issue or one the cycle
created, an edit only on an issue the cycle created during that cycle, and a
cycle has at most 50 such writes. Labels, assignees, milestones, projects,
state changes, another repository and any text with `<!-- tehas:` in it are
refused with the reason (see [the relay](/github-app#the-relay-is-a-security-channel)).
What it posts is masked as the console masks a log, and its `@` mentions
notify nobody.

A planning turn's and a manual session's container is given
`~/tehas/state/github-session` read-only at `/run/tehas/github`, never the key
or the app's own token, and that token as its `GH_TOKEN`. The
image's `gh` (`host/sandbox/gh-shim`, before `/usr/bin/gh` on its `PATH`) reads
the token there at every call, and git's helper asks that `gh`; without the
directory it uses `GH_TOKEN`. A cycle's container reads the cycle's token
directory the same way. A call GitHub refuses for a rate
limit is tried once more after 60 seconds, and only that call; a call that
reads standard input is not retried.

GitHub names an app `<slug>[bot]` in its REST answers and `<slug>` in GraphQL:
where tehas compares a GraphQL author with the bot it drops the `[bot]`.

What the shim of `~/tehas/shim/gh` counts as a write is what a call asks GitHub
to do, and not how it asks it. `gh api graphql` sends POST whatever it asks
for, a query among them, so the method says nothing there and the shim reads
the first word of the document instead: a query, a fragment, or a document that
names no operation reads and is not logged, and only a `mutation` adds a line
of kind `api POST`. An idle poll of the dispatcher asks nothing but queries, so
it writes nothing. See [GitHub App identity](/github-app).

A write that GitHub refuses for a rate limit (`HTTP 429`, or `HTTP 403` with
`rate limit`) is sent once more after 60 seconds (`TEHAS_GH_RETRY_WAIT`), as in
the container, and only that call: gh shows no `Retry-After`, and GitHub asks
for a minute when it names none. Both attempts are lines of the log. A write
that reads standard input is sent once. So is a write typed at a terminal:
gh's output stays the terminal, so `gh pr create` and the other commands that
ask still ask, and a rate limit shows at once. Nothing is shared between calls, so a
call that waits holds up no other. A caller with a time limit of its own, such
as the poll's 30 seconds a call, ends the call at its limit; the poll's action
journal settles that write on a later poll. A write that ended at its limit, or
that GitHub answered with a 5xx, is kept as `unknown`, not as failed: it may
have reached GitHub, so it is read again (a comment by its body, a label by
name) before it is ever sent again. Found there, it is done and not sent; not
found, it is sent once; a read that failed sends nothing until a later read.

## tehas github writes

```
tehas github writes             # the writes of the 24 hours to now, by hour
```

Reads `~/tehas/state/github-writes.log` and prints the writes and the refused
writes (403 or 429) of the last 24 hours, the busiest minute and the busiest
hour, beside GitHub's limits for writes (about 80 a minute and 500 an hour),
and then one row per clock hour with its writes, its refused writes and its
busiest minute. It needs no settings and changes nothing. `tehasbot
github-writes` is the same command under its old name, kept through 1.x.

Exit 0 the writes were printed; 1 neither `TEHAS_HOME` nor `HOME` is set; 64
an argument it does not know.
