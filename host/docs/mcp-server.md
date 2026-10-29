---
title: The MCP server
description: "A read-only MCP server for agents outside the host: what it shows, how to switch it on, and how an agent connects."
---

# The MCP server

Agents outside this host, such as Hermes or OpenClaw agents, can ask a factory
how it is doing over MCP: whether it is working, what is in line, what ended,
a running cycle's console and the last ten days, as the dashboard shows them,
one issue's story, and the sessions of the last 30 days and their logs, masked,
the lines of those logs that hold a string you give, and why an issue's last
stopped run stopped.
See
[what it shows](#what-it-shows). The server is read only. No tool changes
anything; pause, limit, drain, labels and retries stay with a person on the
host, through [the factory API](/api).

It is off until you switch it on, and it answers only a request that carries
one of your keys.

## What it shows

A key holder sees the public dashboard's data, plus the sessions of the last
30 days and their logs, masked. A request without a key sees nothing.

The key is the boundary, not the dashboard. Its inputs are three directories,
each given to it read only:

- `~/tehas/live/`, the files the docs server publishes under `/live/` and at
  `/health`;
- `~/tehas/mcp/`, the two files `tehasbot status` writes on each poll: the
  sessions snapshot `sessions.jsonl` and the outcomes counts
  `outcomes.jsonl`. Both are private to the tehas user and never published;
- `~/tehas/logs/`, where each cycle and planning turn leaves its logs.

It reads nothing else on the host and calls nothing: not GitHub, not Docker,
not git, not a model. Each tool copies only the keys it names from those
files. A key the files gain later is not served until a release names it.

On a factory that works on a private repository, a key gives its holder that
repository's code, issue text and model output as the agents saw them, in the
logs. Give a key only to someone who may read the repository. The mask takes
out tokens, keys, passwords and private addresses it knows, but it is a net
for accidents, not a wall.

## Switching it on

1. Make a key for each agent, on the host:

   ```
   tehas mcp key
   ```

   It prints a new key and writes nothing. A key reads the factory's logs: on
   a private repository, give one only to someone who may read it.

2. Put the switch and the keys in `~/.config/tehas/env`, beside the host's
   other secrets, never in the instance directory, which is in a repository:

   ```
   TEHAS_MCP=1
   TEHAS_MCP_KEYS="<key> <key>"
   ```

   Keys are separated by spaces, each at least 32 characters of
   `A-Za-z0-9_-`. With `TEHAS_MCP=1`, an empty list or a key that is not like
   that stops the server with exit 78 and one line that names the setting.
   A `TEHAS_MCP_KEYS` line in `instance.env` is refused the same way.

3. Start it, and have it start with the host:

   ```
   systemctl --user enable --now tehas-mcp.service
   ```

   `install.sh` installs the unit and never switches it on. `tehas update`
   restarts it when it is running.

4. On the machine that publishes the dashboard, send `/mcp` to the host's port
   8082, next to the dashboard's 8080. For Caddy:

   ```
   handle /mcp {
       request_body {
           max_size 64KB
       }
       reverse_proxy <host address>:8082
   }
   ```

   Rate limiting by address belongs there too: the server has none of its own.
   It serves `/mcp` only and answers any other path with 404. This page is at
   `/mcp-server`, so the route takes no page of the manual with it.

To remove a key, take it out of the list and run
`systemctl --user restart tehas-mcp.service`. The journal names each request's
key by the first 8 hex characters of its sha256, never by the key, so you can
tell which agent is calling.

One line goes to the journal per request: the method, the status, the time it
took and the key, and for a `tools/call` the tool as well, as
`POST 200 4ms key=3511bfc5 tool=factory_log`. The line goes to the unit's
journal on the host, not to the client: read it with
`journalctl --user -u tehas-mcp`. The name of the tool only: the arguments are
never logged, as one can carry an issue number or text from a private
repository. `tool=?` is a name the server does not have, which is what a probe
of a tool it has not looks like. `tool=-` is a name that is not text, and only
the 2025-11-25 handshake can carry one, as a modern request must mirror its
name in `Mcp-Name` and no text equals a number: so a dash tells a client that
is broken from one that is probing, except for a probe whose name never reached
the server as text, which is a dash too. A request that is not a `tools/call`
logs no `tool=`. The fields are the server's own words, never the client's
text, so the line the unit writes stays one field per space-separated part: do
not add a field to it without naming where its bytes come from.

## How it runs

`tehasbot mcp serve`, run by `tehas-mcp.service`, checks the settings and runs
the server, `~/tehas/libexec/tehas-mcp`, in a container of its own. The
container runs as the tehas user, not root, with a read-only root, no
capabilities, `no-new-privileges`, 256 MB of memory and 64 processes. It is
given `~/tehas/live`, `~/tehas/mcp` and `~/tehas/logs` read only, the binary,
the keys as an environment variable, the host's names to mask as arguments
(names, not secrets) and port 8082 on `TEHAS_HOST_ADDR` (the loopback address
without it), and nothing else: no Docker socket, no SSH key, no token, nothing from
`~/.config/tehas` but the keys. It is on a network of its own, `tehas-mcp`,
made without masquerade, so it cannot reach the internet; the host itself is
still one hop away. The command makes the network, and refuses to start on a
network of that name that lets the container out. Its image is the Caddy
image the docs server runs, pinned by the same digest in `versions.env`; the
binary is static and needs nothing from it. The command makes `~/tehas/mcp`
with mode 700 before it starts the container, so Docker never makes it as
root.

With `TEHAS_MCP` unset or `0` the command removes a container left running and
exits 0, and the unit stays stopped.

## How an agent connects

| | |
| --- | --- |
| URL | `https://<domain>/mcp` |
| Transport | Streamable HTTP, POST only |
| Header | `Authorization: Bearer <key>` |
| Versions | `2026-07-28`, and `2025-11-25` for clients that start with `initialize` |

A client config for a Streamable HTTP server looks like this. It is an
example: your client's file and field names may differ.

```json
{
  "mcpServers": {
    "tehas": {
      "url": "https://<domain>/mcp",
      "headers": { "Authorization": "Bearer <key>" }
    }
  }
}
```

Every request needs a key, whatever its method, path or tool. A request with
no key, a wrong key or another scheme gets 401 and an empty body, before any
of its body is read. A request with a key that is not a POST gets 405. A request with an `Origin` header gets
403: no browser is a client of this server. A body needs a `Content-Length`
(411 without one, or when chunked), and over 64 KB gets 413. A head over 8 KB
gets 431. A request must arrive whole within 10 seconds, however slowly it
trickles in, every answer closes the connection, and past 32 connections at
once the next one gets 503. A method that is not a word of capitals gets 400,
so nothing a client sends there reaches the log as a line of its own. No
session is kept.

The binary takes the 10 seconds as `--io-timeout <seconds>`, a whole number
from 1 to 3600, and keeps the default without it. `tehasbot mcp serve` never
passes it, so the server on a host always waits the 10 seconds. The flag is
there for the tests, which would otherwise wait out the deadline itself.

The `2025-11-25` handshake is there because neither Hermes nor OpenClaw says
yet which version it speaks. It goes once both speak `2026-07-28`.

## The tools

Every result carries `"api": 1`. Within version 1 fields are only added; a
field without a value is left out. Results are JSON in `structuredContent` and
the same JSON as text.

| Tool | Arguments | Returns |
| --- | --- | --- |
| `factory_status` | none | `status` (pass, warn, fail), `output`, `time`, `stale` (no report for ten minutes), `generatedAt`, `repo`, `paused`, `planning`, `limit.state`, `queue` (`inLine`, `building`, `review`, `stopped`, `time`), `cycles` (issue, running, status, step, startedAt, time), `demand` (waiting, oldest, time), `platform` (commit, behind, upstream, fetchedAt) |
| `factory_issues` | `stage`, optional: `queued`, `planning`, `running`, `awaiting`, `stopped` or `completed` | Per stage, the issues the dashboard lists: number, title, url, updatedAt, and closedAt when closed. `planning` is a count and issue numbers, without titles |
| `factory_endings` | none | `endings`, the cycles that ended last (issue, status, step, startedAt, updatedAt), and `failed`, the failed ones among them (issue, status, step, startedAt, time) |
| `factory_console` | `issue`, and `bytes` from 1 to 65536 | One running cycle's issue, step, status, statusText, startedAt, updatedAt, and the last `bytes` of its console, 64 KB without it. Only those bytes are read, however large the console |
| `factory_stats` | none | The ten-day report the dashboard shows, with `refine`: refining sessions, the issues they filed, how many started and shipped, the lead time of the shipped ones and, to compare, the same counts for the other issues made ready. Counts only. See [Analytics](/analytics) for what each figure means |
| `factory_sessions` | all optional: `outcome` (`pr`, `merged`, `handoff`, `parked`, `failed`, `refused`, `stopped`), `failureKind`, `tehasVersion`, `rukasVersion` (also matches the commit), `kind` (`work`, `plan`), `how` (`label`, `retry-tick`, `outage-retry`, `millwright`, `answer`, `refine`; the old `fixer` is accepted and matches the same runs), `image` (an image id `sha256:<hex>`, or its first 12 hex or more, with or without `sha256:`), `issue`, `limit` from 1 to 200 (50 without it), `before` | The runs of the last 30 days, newest first, filters combined: id, kind, issue, attempt, how, started, ended, seconds, outcome, failedStep, failureKind, failureCause, parkReason, tehasVersion, rukasVersion, image and imageBuilt, and links to the issue and pull request (`issueUrl`, `prUrl`). `next` is the last id when more remain: pass it as `before`. A version or failure kind is matched exactly. `image` is the sandbox image the run's container started from, by its id, which names one build; `imageBuilt` is when that image was built, UTC. A run recorded before images were has neither, and a run of an image the host no longer had when the run ended has no `imageBuilt` |
| `factory_session` | `id`, like `work-12-1792485967` | One run of the last 30 days: every figure of the session, its `steps`, its `roles` (model, provider, tokens), its `logs` as a word and a size in bytes (never a file name), `filed` for a planning session, and `issueUrl` and `prUrl` |
| `factory_log` | `id`, `file` (a word below), optional `gzip` (false without it), and for a plain read `offset` (0 without it) and `bytes` from 1 to 1048576 (1 MiB without it) | One log file of the session, masked. Plain: `id`, `file`, `size`, `offset`, `text`, and `next`, the offset of the rest, when more remains. With `gzip: true`: the whole masked file compressed, as an embedded resource of type `application/gzip` at `tehas://sessions/<id>/<file>.gz`, and `id`, `file`, `size` and `gzipBytes` as the structured content |
| `factory_search` | `text` (3 to 200 characters), one of `issue` or `ids` (at most 50 session ids), optional `files` (log words, all the session has without it) and `limit` from 1 to 200 matches (50 without it) | The masked lines of the last 30 days that hold `text`, a fixed string and not a pattern, of one issue's sessions or of the sessions named: `matches`, each with `id`, `file` (a word below), `offset` in masked bytes (one to give `factory_log`) and `line` cut to 400 characters, and `sessions`, how many were read. `days` and `stale` as the session tools. When the reading stops early it gives `next`: the `id`, the `file` and the `offset` of the rest, which is where that session's log is read on from. See [the logs](#logs) |
| `factory_issue` | `issue`, a positive integer | One issue's story in one call: `issue`, `issueUrl`, `stage` (the first of `queued`, `planning`, `running`, `awaiting`, `stopped` or `completed` that lists it, left out when it is listed nowhere), `title` when the dashboard has it, `running` (`step`, `status`) while a cycle owns it — only a cycle that is running owns one, so a cycle the store has marked `lost`, or one that ended between publishes, puts nothing there and does not make the stage `running` — and `attempts`: every run of it of the last 30 days, oldest first, each with its `id` (the one to give `factory_session` or `factory_log`), `kind`, `attempt`, `how`, `started`, `ended`, `seconds`, `outcome`, `failedStep`, `failureKind`, `failureCause`, `parkReason` and `prUrl`. `firstSeen` and `lastSeen` are the started of the oldest and newest run, and `outcome` is the newest run's. An issue with no run in the window and no stage is a tool error that names it |
| `factory_failures` | all optional: `by` (`version` or `image`), `rukasVersion` (also matches the commit) | The work runs of the last 30 days per tehas and pi-rukas version pair, the most runs first: `runs`, `outcomes` and `failureKinds` as counts, and `causes`, the five most common as cause, kind, count and up to ten issue links, newest first. With `by: image` each pair is split by the sandbox image its runs used, and an entry also has `image` and `imageBuilt`, so two builds of one pi-rukas version can be compared; runs with no image on record are an entry without them. Planning sessions are not counted |
| `factory_outcomes` | all optional: `tehasVersion`, `rukasVersion` (both matched exactly), `since` and `until` (a day `YYYY-MM-DD`, or a full UTC time `YYYY-MM-DDTHH:MM:SSZ`), `by` (`version` or `week`) | How the factory has done further back than 30 days, as far back as the analytics reaches: 365 days. `by` (`version` without it), `days`, `generatedAt` and `stale`, and `entries`: grouped by version, one per tehas and pi-rukas pair with `tehasVersion`, `rukasVersion`, `runs`, `outcomes`, `failureKinds` and `parkReasons` as counts, the most runs first; grouped by week, the same counts per `week` (the Monday that opens the UTC week), newest first. A run is counted on its end, and one that never ended on its start. Counts only: no issue number, title, failure cause or log content |
| `factory_park` | `issue`, an integer from 1 to 999999999 | Why the issue's last stopped work run of the last 30 days stopped: a run whose outcome is `handoff`, `parked`, `failed` or `refused`. `id`, `started`, `ended`, `outcome`, `status`, `exitCode`, `latest` (no later work run of the issue in the window), and as the park comment's `tehas:park` marker has them `step`, `kind` (the failure kind), `cap`, `reason` (the park reason), `cause`, `provider` and `sig`. From the run's `result` log: `result` (`read`, `none`, `gone` or `unreadable`), `message`, `causeClass`, `causeReason`, `userAction` and `checks`, the failing check lines, masked, at most 20 of 400 characters each, with `checksCut` when there were more. `days` and `stale` as the session tools. An issue with no stopped run in the window is a tool error that names it |

The links are built from the `repo` the dashboard publishes, and only when it
is `owner/name`. Without a valid repo a session has no links and a cause has no
issues. The session tools read `sessions.jsonl`, which `tehasbot status`
rewrites on each poll; `stale` is true when it is more than ten minutes old. A
session older than 30 days, or an id that is not in it, is a tool error.

`factory_outcomes` reads `outcomes.jsonl` from the same directory, which
`tehasbot status` rewrites on each poll from `analytics outcomes`; `stale` is
true when it is more than a day old, the writer being a daily one. The file is
the whole of the analytics retention, and `days` is what the file itself says
that reach is, so the tool answers where the 30-day tools
cannot: whether a release changed the park or failure rates, and how often a
failure kind comes back. It holds counts only, and the tool serves only the keys
it names: of a group, `runs` and the three count maps, and the words that name
its grouping. A count that is not a whole number, or a map of them that is not,
is left out rather than served. The file is written grouped one way, and `by` says which: an argument
that asks for the other is a tool error that names the one it holds, rather than
an answer of nothing. A week's entry stands for seven days and is kept when any
of them falls in the window, because a count of a week cannot be split between
its days.

`factory_park` reads the same snapshot, then one file: the `result` log of
the run it answers, by the name the snapshot gives and only when that name is
the one pattern of a work run's result of that issue. A result over 1 MiB, a
link, or one that is not JSON is not read, and the answer says `unreadable`.
`sig` is the hash the cycle signs its park with, of the step, cap, park reason,
failure kind and cause the run recorded, so it matches the marker on the
issue's park comment without a call to GitHub. The check lines are the
runner's own extract of the evidence, then the lines of its evidence that say
`FAIL`, `FAILED`, `not ok` or `failed checks`, each once. Every text of the
result is masked, then made one line, then cut. Nothing else of the result is
served: not its handoff body, its paths of the work state or its usage.

`factory_issue` reads the same snapshot for its attempts and the dashboard's own
files for the rest: `status.json` for the stage and the title, `running.json`
for the cycle that owns the issue, and `health.json` for the issues being
planned, whose list `status.json` gives only as a count. It reads nothing else,
and it never gives an issue's body or its labels.

### Logs

`factory_log` names a file by a word, never by a path. The words a session
has depend on its kind:

| Kind | Word | File |
| --- | --- | --- |
| work | `transcript` | `work-<n>-<stamp>.jsonl`, the runner's event stream |
| work | `err` | `<transcript>.err`, the runner's standard error |
| work | `preflight` | `<transcript>.preflight`, the preflight check's output |
| work | `console` | `<transcript>.console`, the console as written during the run |
| work | `result` | `<transcript>.result.json`, the runner's result |
| work | `dispatch` | `dispatch-work-<n>.log` |
| plan | `progress` | `plan-<n>-<stamp>.jsonl`, the runner's standard error and progress |
| plan | `result` | `plan-<n>-<stamp>.json`, the runner's result |
| plan | `dispatch` | `dispatch-plan-<n>.log` |

`dispatch` is per issue: every run of the issue appends to it, and it is
served whole. Where the improvement loop is on, it removes the `work-*` and
`dispatch-work-*` logs after seven days (see [the host](/host)), so a work
session older than that is listed with no logs. A word the session does not have is a tool error that lists the
ones it has; `factory_session` lists them with their sizes.

The server opens only a file name the sessions snapshot gives for that
session, and only when the name is the one pattern its word allows for the
session's kind and issue. The logs directory also holds the host's own logs,
and nothing else in it can be named. A file that is now a link, or is gone,
is a tool error that names the word.

Limits:

- A file over 16 MiB is not served, plain or gzipped, and `factory_search` reads
  none of them.
- A plain read returns at most 1 MiB. Read a larger file in pieces: pass the
  `next` of one answer as the `offset` of the next. A piece starts and ends
  on a whole character.
- `factory_search` answers at most 200 matches, 50 without `limit`, and reads at
  most 8 MiB of raw bytes in a call. The cap is the call's, not one log's: what
  one log costs is off what the rest of the call has left, and where the reading
  stops the answer says so with `next`. As with `factory_log`, whose `next` is
  past the last byte it answered, `next` names a place the answer has read to:
  a log the cap spent before it kept a line of its own has nothing in the answer
  to read on past, so the reading moves on to the next log and the rest of that
  one waits for a call of its own.
- The way on of a search that stopped is the same call with the `next` put on the
  session's own record as its `searchResume`, which is what a caller of the tools
  holds from `factory_session`. Of that session then only the log `next` names is
  read, from its offset on; every other session keeps its own way on, so the rest
  of one log never hides another session's lines.
- Two logs are sent at a time, by `factory_log` and `factory_search` together. A
  log holds its place until its answer has been sent, however slowly the client
  reads. A third call meanwhile gets a tool error that asks to try again in a few
  seconds.

Every line is masked before it is sent, so `size`, `offset` and `next` count
bytes of the masked text, not of the file on the host. The size
`factory_session` gives is the file's. Two gzip answers at once of a
generated 9 MB file, not a real transcript, kept the server under 40 MB of
memory.

The mask takes out the tokens, keys, passwords and private addresses it
knows, and the host's names the dashboard's console hides: the host aliases,
the host of every model endpoint and the bot's email. It is a net for
accidents, not a wall: a log can hold a secret in a form the mask does not
know.

A file that is missing or half written gives a tool error that names it, and
the server keeps answering. `factory_stats` with analytics off says that
`analytics.json` is not published.
