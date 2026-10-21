# Spec: sessions and their logs over MCP

**Status**: Draft, 2026-10-21. Nothing is built.

## Purpose

The pi-rukas developer wants to know why sessions fail on a factory. Today the
MCP server (see [the MCP spec](2026-10-20-mcp.md) and
[the manual page](../../host/docs/mcp-server.md)) shows which cycles failed and the
step they ended in, plus the ten-day totals of stop reasons. It cannot say
why one session failed, which versions it ran, or show its log. The answers
are on the host, in `~/tehas/state/analytics.db` and `~/tehas/logs/`, which
the server is built never to read.

This spec gives a key holder the factory's sessions of the last 30 days, with
their versions, failure category, cause and the rest of what the host records
about each, and lets them fetch any of a session's log files, gzipped if they
ask.

A session here is a run: one work cycle or one planning turn, one row of the
`runs` table.

## The rule changes

The 2026-10-20 spec rests on "the MCP server shows nothing the public site
does not already serve". **That rule is replaced** (the owner's call,
2026-10-21):

> A key holder sees the public dashboard's data, plus the sessions of the
> last 30 days and their logs, masked. A request without a key sees nothing.

The key is now the boundary, not the dashboard. What follows from it:

- On a factory that works on a private repository, a key gives its holder
  that repository's code, issue text and model output as the agents saw them.
  The manual says so where it says how to make a key, and says to give keys
  only to people who may read the repository.
- The manual's advice "put it on the internet only where the dashboard is
  already on the internet" no longer holds as the reason it is safe. It is
  replaced by the sentence above.
- Nothing new reaches the public dashboard or `/live/`.
- Still read only. No tool changes anything.

The other properties of the 2026-10-20 spec stay: every field a tool returns
is on an allowlist, no path comes from the caller, missing data gives a tool
error naming what is missing, and the factory does not depend on the server.

## Goals

1. List the sessions of the last 30 days, newest first, filtered by outcome,
   failure category, tehas version, pi-rukas version, kind and issue.
2. One session in full: what the host records about the run, its steps, its
   roles, which log files it has with their sizes, and links to the issue it
   worked on and its pull request.
3. One log file of one session, whole, plain or gzipped, masked.
4. Failure counts per version pair over the 30 days, so "did pi-rukas v0.14.0
   make things worse" is one call.

## Non-goals

- Logs older than 30 days, even where the file is still on disk.
- Pruning the logs on disk. They are kept forever today; that is a separate
  question.
- Search inside logs. The client fetches the file and searches it.
- Issue bodies, comments or anything from GitHub that the host does not
  already have in these files.
- Per-key scopes. Every key sees the same thing.
- Changing what the public dashboard shows. Its report stays ten days.

## Anti-goals

- A key, a token or a password from a log in a response. The mask runs over
  every byte of log content the server sends, before compression.
- A caller naming a file. The server opens a log only by a name it builds
  from a session id it found in its own session list.
- The MCP container gaining a write path, a network route out, or any of
  `~/.config/tehas` beyond the keys.
- A request that makes the server hold a whole 9 MB transcript in memory
  more than once, or run past the container's memory limit.

## What a session holds

From the `runs`, `run_steps` and `run_roles` tables, schema 1, as
[Analytics](../../host/docs/analytics.md) describes each column:

| Group | Fields |
| --- | --- |
| Identity | id, kind, issue, attempt, how, started, ended, seconds, queue wait |
| Versions | tehas version and commit, pi-rukas version and commit, image, result schema |
| Outcome | outcome, exit code, status, last step, failed step, cap, park reason, review held, failure kind, failure cause, provider outage, endpoint, pr |
| Use | input, output, cache read and write tokens, turns, dispatches, cost, runner ms |
| Container | OOM kills, refused forks, concurrent cycles |
| Steps | each step with its round, start, end and seconds |
| Roles | each role with its model, provider, dispatches, turns, tokens, cost, ms |
| Links | `issueUrl`, the issue the session worked on, and `prUrl`, its pull request when it has one |
| Logs | for each log file the session has: its name (one of the words below) and its size in bytes |

The links are built from the factory's repository, as `status.json` names it
in `repo` (the field `factory_status` already serves):
`https://github.com/<repo>/issues/<n>` and `https://github.com/<repo>/pull/<pr>`.
GitHub forwards an issue link that is really a pull request, so nothing is
looked up. GitHub decides who may open a link: on a private repository the
link works only for someone with access to it, and the server does nothing
about that. The repository name must be `owner/name` of `A-Za-z0-9._-`; any
other value leaves the links out rather than building a link from it. The
session's issue number and PR number come from the run row, never from the
caller.

Field names follow the factory API's camelCase, as `tehas history --json`
does (`failureKind`, `failureCause`). Times are ISO 8601 UTC, as there.

## Log files

Each run leaves files in `~/tehas/logs/` (as seen on tehas-tehas,
2026-10-21). The tool names them by word, never by path:

| Word | File | What it is |
| --- | --- | --- |
| `transcript` | `work-<n>-<stamp>.jsonl` | The headless runner's event stream, up to about 9 MB |
| `err` | `<transcript>.err` | The runner's standard error |
| `preflight` | `<transcript>.preflight` | The preflight check's output |
| `console` | `<transcript>.console` | The console as written during the run |
| `result` | `<transcript>.result.json` | The runner's result |
| `dispatch` | `dispatch-work-<n>.log` | The per-issue log that every run of that issue appends to |

A planning turn leaves fewer files (`libexec/plan`, `lib/planning.sh`), so
its words differ (my call):

| Word | File | What it is |
| --- | --- | --- |
| `progress` | `plan-<n>-<stamp>.jsonl` | The runner's standard error and NDJSON progress |
| `result` | `plan-<n>-<stamp>.json` | The runner's one result document |
| `dispatch` | `dispatch-plan-<n>.log` | The per-issue planning log |

A word that the session's kind does not have is a tool error naming the words
it does have. `factory_session` lists only the files that exist.

`dispatch` is per issue, not per run. The server serves it whole and says so
in the tool description; cutting it to one run is not attempted.

The `<stamp>` is `date -u +%y%m%d-%H%M%S` at the start of `libexec/work`,
close to but not equal to the run's `started`. The run row does not name its
transcript, and no column is added for it: the snapshot finds a run's files by
the stamp in their names, the latest in `[started - 300, ended]`. That works
for rows written before this change too (decided in the plan).

## Tools

New tools, added after the five that exist, in this order. Every result
carries `"api": 1`. The existing tools do not change.

| Tool | Arguments | Returns |
| --- | --- | --- |
| `factory_sessions` | all optional: `outcome`, `failureKind`, `tehasVersion`, `rukasVersion`, `kind`, `issue`, `limit` (1 to 200, default 50), `before` (a cursor) | Sessions of the last 30 days, newest first: id, kind, issue, attempt, started, ended, seconds, outcome, failed step, failure kind and cause, park reason, tehas and pi-rukas version, issue and PR links, and `next`, a cursor for the next page when there is one |
| `factory_session` | `id` | One session: every field in "What a session holds", with its steps, roles and log files |
| `factory_log` | `id`, `file` (a word from the table above), optional `gzip` (default false), `offset` and `bytes` (plain only) | The file's size, and its content masked. Plain: text from `offset`, at most `bytes`, at most 1 MB. Gzip: the whole masked file compressed, as base64 in an embedded resource blob with `mimeType` `application/gzip` |
| `factory_failures` | none | Per tehas and pi-rukas version pair over the 30 days: runs, and counts by outcome and by failure kind, with the top causes, each cause with the issue links of the sessions it ended (at most ten per cause) |

Argument checks, each a tool error, not a protocol error:

- `id` must be the shape the analytics program writes (`work-<n>-<unix>` or
  `plan-<n>-<unix>`) and be a session the server lists in the 30 days.
- `file` must be one of the words for the session's kind.
- Version filters are matched exactly against stored values and are at most
  64 characters of `A-Za-z0-9._+-`.
- A filter that matches nothing returns an empty list, not an error.

Size limits, my calls:

- Plain: 1 MB per call. A file larger than that is read in pieces with
  `offset` and `bytes`. The result says the file's total size and whether it
  was cut.
- Gzip: the masked file, compressed, up to 16 MiB of input. A file over
  16 MiB is not served in either mode, because the mask runs on whole lines.
  The largest seen is 9.1 MB.
- The 64 KB request body limit stays; only responses grow.
- The server's 10 second limit (`IO_TIMEOUT` in `console/mcp/src/http.rs`)
  bounds each write and the reading of the request. A multi-MB answer through
  Cloudflare and the Caddy proxy must still fit: the builder checks that a
  16 MB gzip answer and a 1 MB plain one get through `https://tehas.app/mcp`
  on tehas-tehas, and changes the write limit if not, without loosening the
  request deadline.

## Where the data comes from

Two choices for how the container reads it. The builder picks one and says
why in the pull request.

**A. Mount them read-only.** The container is given `~/tehas/state/analytics.db`
and `~/tehas/logs/` read-only, beside `~/tehas/live`. The server reads the
database with SQLite. Problems to solve: the database is in WAL mode, and a
reader on a read-only mount cannot create the `-shm` file, so it must open
with `immutable=1` or read a copy, and immutable reads may miss the newest
rows. The MCP binary gains a SQLite dependency.

**B. The host writes a private snapshot.** On each poll, or when a run ends,
the host writes `~/tehas/mcp/sessions.json`: the 30 days of sessions, steps,
roles and log names, produced by the analytics program, which already holds
all the SQL. The container is given that directory and `~/tehas/logs/`
read-only. The server needs no SQLite, and the snapshot is never published.

My lean is B. It keeps the SQL in one program and the MCP binary small, and
it follows how `live/` already works. Either way:

- Every new mount is read-only and pinned by the container test, as the
  existing mounts are. This is a security change and the pull request says
  so, as `AGENTS.md` rule 4 asks of cycle containers.
- No write path, no new network route, no new environment variable but what
  the server needs to find its files.
- The container's memory limit, 64 MB today, may need raising for gzip of a
  9 MB file. The builder measures it and sets it, and the manual gives the
  number.

## Thirty days

- The analytics database already keeps 365 days (`RETENTION_DAYS` in
  `console/analytics/src/db.rs`, since 97b9d21). Retention does not change.
- The 30 days are the MCP window only: a constant in one place, applied to
  every new tool, so a session older than 30 days is neither listed nor
  opened, and its logs are not served, even though its row and files remain.
- The dashboard's report stays ten UTC days.

## Masking

The console's mask (`console/src/mask.rs`) runs over every log byte the
server sends, the same mask the dashboard's console uses. It is a net for
accidents, not a guarantee, and the manual says so.

- The mask runs on whole lines. Files over 16 MiB are not served, so no line
  is cut and no token falls across a cut.
- Plain `offset` and `bytes` count bytes of the masked text, so a token is
  masked before the caller's offset can cut it.
- The mask moves into a small shared crate, or the MCP crate depends on the
  console crate's mask. Builder's call; the console and the server must use
  the same code, not a copy.

## Tests

Rust tests beside the source, shell tests in `test/`, no network, no image:

- Sessions: fixtures with runs inside and outside 30 days; only those inside
  are listed; filters; newest first; cursor paging; `limit` bounds.
- Allowlist: fixture rows with an extra column; no tool returns it. A test
  lists every key each new tool can return.
- Ids: a malformed id, an id with `/` or `..`, an id older than 30 days, an
  id the list does not hold: each a tool error, and no file is opened.
- Files: an unknown word is refused; each word opens only the name it maps to.
- Mask: a log fixture with a GitHub token, a password line and a private
  address, served plain, in pieces across the token, and gzipped; none of
  them comes out. A token that crosses an `offset` is still masked.
- Gzip: the blob decodes to the masked text; a file over the gzip limit gives
  the tool error.
- Window: rows older than 30 days are kept in the database but no new tool
  lists, opens or serves logs for them.
- Links: built from `repo` and the row's numbers; a `repo` with a `/` too
  many, a space or a `..` leaves them out; a session without a PR has no
  `prUrl`.
- Container: the mounts and environment are pinned, the new ones read-only.

## Manual

- `host/docs/mcp-server.md`: the new rule and what a key gives, the four tools and
  their fields, the size limits, the memory limit, the log words, and which
  older sessions have no logs listed.
- `host/docs/analytics.md`: the new column.
- `host/docs/features.md`: the new tools tested, not proven.

## Proof

Proven when the pi-rukas developer, from outside the tailnet with a key on
`https://tehas.app/mcp`, lists the failed sessions of the last 30 days on
tehas-tehas, opens one, follows its issue link, and fetches its transcript gzipped and
its `err` plain. The route `/mcp` on tehas.app is in place (answering 401
without a key, 2026-10-21).

## Left to the builder

- Choice A or B for where the data comes from.
- The container's memory limit.
- Where the shared mask lives.
- The top-causes count in `factory_failures`, and the tool descriptions.
