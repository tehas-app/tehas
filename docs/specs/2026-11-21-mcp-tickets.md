# The MCP server gives tickets

Issue #2718, step 3 of epic #2689. An agent that asks why reads the ticket
behind a change: the issue's body and comments, its epic, and the pull request
that closed it.

## What it is

- `console tickets <owner/name> <dir>` (`console/src/tickets.rs`), run by
  `tehasbot status` after the epics while `TEHAS_MCP=1`, under the 60 s of the
  epics' read. It searches `repo:<repo> sort:updated-asc updated:>=<since>`,
  50 a page, at most 4 pages a poll, and writes `<dir>/<number>.json` for each
  issue and pull request. `<dir>` is `~/tehas/mcp/tickets`, mode 700.
- `.cursor.json` keeps `since` (the last change read) and, while a search has
  more pages, its page cursor and query. GitHub's search ends at 1000 results;
  the next search starts again from the last change read.
- A file keeps the body to 64 KiB and each of the first 50 comments to 16 KiB.
- `factory_ticket` and `factory_tickets` read the files (`console/mcp/src/tickets.rs`),
  listed beside the plugin tools for a full key.

Measured once by hand, off a host: four pages of tehas-app/tehas, 200 items,
in 7 s. A search of 200 files through a debug server took 0.4 s.

## Changes to the MCP server

| Where | Was | Is |
| --- | --- | --- |
| What it reads | `~/tehas/mcp`: the snapshot and the outcomes | Also `~/tehas/mcp/tickets/` |
| Its mounts, its arguments, its network | | Unchanged |
| What it calls | Nothing | Unchanged: GitHub is read by the status run on the host |
| A full key | Sessions, logs, the dashboard's numbers | And the repository's issues and pull requests, bodies and comments |
| A dashboard key | The four live-file tools | Unchanged |

## What the host does more

One to four GitHub GraphQL searches a poll while the MCP server is on. None
with it off.

## Not in this step

Review comments on pull requests. The code index and learnings are steps 4
and 5.
