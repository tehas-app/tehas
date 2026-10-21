---
title: The MCP server
description: "A read-only MCP server for agents outside the host: what it shows, how to switch it on, and how an agent connects."
---

# The MCP server

Agents outside this host, such as Hermes or OpenClaw agents, can ask a factory
how it is doing over MCP: whether it is working, what is in line, what ended,
a running cycle's console, and the last ten days. The server is read only. No
tool changes anything; pause, limit, drain, labels and retries stay with a
person on the host, through [the factory API](/api).

It is off until you switch it on, and it answers only a request that carries
one of your keys.

## What it shows

A key holder sees the public dashboard's data, plus the sessions of the last
30 days and their logs, masked. A request without a key sees nothing.

The key is the boundary, not the dashboard. Its inputs are three directories,
each given to it read only:

- `~/tehas/live/`, the files the docs server publishes under `/live/` and at
  `/health`;
- `~/tehas/mcp/`, the sessions snapshot `sessions.jsonl` that `tehasbot
  status` writes on each poll. It is private to the tehas user and never
  published;
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

## How it runs

`tehasbot mcp serve`, run by `tehas-mcp.service`, checks the settings and runs
the server, `~/tehas/libexec/tehas-mcp`, in a container of its own. The
container runs as the tehas user, not root, with a read-only root, no
capabilities, `no-new-privileges`, 256 MB of memory and 64 processes. It is
given `~/tehas/live`, `~/tehas/mcp` and `~/tehas/logs` read only, the binary,
the keys as an environment variable and port 8082 on `TEHAS_HOST_ADDR` (the
loopback address without it), and nothing else: no Docker socket, no SSH key, no token, nothing from
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
| `factory_sessions` | all optional: `outcome` (`pr`, `merged`, `handoff`, `parked`, `failed`, `refused`, `stopped`), `failureKind`, `tehasVersion`, `rukasVersion` (also matches the commit), `kind` (`work`, `plan`), `how` (`label`, `retry-tick`, `outage-retry`, `fixer`, `answer`, `refine`), `issue`, `limit` from 1 to 200 (50 without it), `before` | The runs of the last 30 days, newest first, filters combined: id, kind, issue, attempt, how, started, ended, seconds, outcome, failedStep, failureKind, failureCause, parkReason, tehasVersion, rukasVersion, and links to the issue and pull request (`issueUrl`, `prUrl`). `next` is the last id when more remain: pass it as `before`. A version or failure kind is matched exactly |
| `factory_session` | `id`, like `work-12-1792485967` | One run of the last 30 days: every figure of the session, its `steps`, its `roles` (model, provider, tokens), its `logs` as a word and a size in bytes (never a file name), `filed` for a planning session, and `issueUrl` and `prUrl` |
| `factory_failures` | none | The work runs of the last 30 days per tehas and pi-rukas version pair, the most runs first: `runs`, `outcomes` and `failureKinds` as counts, and `causes`, the five most common as cause, kind, count and up to ten issue links, newest first. Planning sessions are not counted |

The links are built from the `repo` the dashboard publishes, and only when it
is `owner/name`. Without a valid repo a session has no links and a cause has no
issues. The session tools read `sessions.jsonl`, which `tehasbot status`
rewrites on each poll; `stale` is true when it is more than ten minutes old. A
session older than 30 days, or an id that is not in it, is a tool error.

A file that is missing or half written gives a tool error that names it, and
the server keeps answering. `factory_stats` with analytics off says that
`analytics.json` is not published.
