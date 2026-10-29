---
title: For agents
description: "Where a coding agent starts with this tehas factory: llms.txt, the manual as Markdown and the live data."
---

# For agents

A coding agent can pick tehas up from one address: [`/llms.txt`](/llms.txt) on
this host. It says what tehas is, how to get work built by this factory, how to
set tehas up for another repository, where the live data is, and how to ask a factory over MCP.

## Hand it to your agent

Paste one of these, with this host's address in place of `<this host>`:

```
Read http://<this host>/llms.txt, then write an issue this factory can build for: <what you want>
```

```
Read http://<this host>/llms.txt, then set tehas up for this repository. Ask me before deciding anything about secrets, accounts or who may authorize work.
```

Away from this host, start from the README of the
[public repository](https://github.com/tehas-app/tehas) instead. A running factory
to look at is the public dashboard at <https://tehas.app/>.

## What an agent can read here

| Address                                 | What it is                                                     |
| --------------------------------------- | -------------------------------------------------------------- |
| `/llms.txt`                             | The entry point: a summary and links, as plain text            |
| `/<page>.md`                            | Any manual page as Markdown, for example `/new-project.md`     |
| `/health`                               | Factory health as JSON; 503 when unhealthy                     |
| `/live/status.json`                     | The queue snapshot: repository, issues by state, how many are in planning, how many closed issues carry `tehas:shipped`, paused or not, and the most used words of all its issues |
| `/live/epics.json`                      | The open epics, issues with sub-issues, newest first: each with its count of sub-issues and how many are closed, and its sub-issues in this repository with their state |
| `/live/running.json`                    | The running cycles and the one that finished last              |
| `/live/endings.json`                    | The last ten cycles that ended, oldest first                   |
| `/live/console-<issue>.txt`             | That cycle's console, with tokens, keys, passwords and the host's secret settings masked         |

All of it is read-only. Work is submitted and authorized in GitHub, as
[Operating](/operating) describes.

## Ask this factory over MCP

A factory can also answer questions about itself over MCP, when its owner has
switched that on. Every tool is read only. The server is at
`https://<domain>/mcp`, over Streamable HTTP, and every request needs the header
`Authorization: Bearer <key>`. A key comes from the factory's owner, who makes
one with `tehas mcp key` on the host. Ask your person for it and never guess
one. A request without a key gets 401.

- `factory_status`: whether the factory works, the queue, the running cycles, whether it is paused
- `factory_issues`: the issues in one stage, such as `queued`, `running` or `stopped`
- `factory_endings`: the cycles that ended last, and the failed ones among them
- `factory_console`: the tail of one running cycle's console
- `factory_stats`: the ten-day report the dashboard shows
- `factory_issue`: one issue's story: its stage, its title, the running cycle and every run of the last 30 days
- `factory_sessions`: the runs of the last 30 days, newest first, with filters such as `outcome`
- `factory_session`: one run in full: steps, roles, logs and links
- `factory_log`: one log of a run, masked, plain in pieces or whole as gzip
- `factory_failures`: the work runs of the last 30 days per tehas and pi-rukas version pair, with their most common causes; `by: image` splits a pair by the sandbox image build
- `factory_outcomes`: how the factory has done further back than 30 days, as counts per tehas and pi-rukas version pair or per UTC week
- `factory_park`: why an issue's last stopped run stopped: the step, the failure kind, the cap, the park signature, the runner's error and the failing check lines, masked

To see what went wrong, call `factory_failures`, then `factory_sessions` with
`outcome` set to `failed`, then `factory_session` for one of them, then
`factory_log` for one of its logs. For a parked issue, `factory_park` with the
`issue` gives the step and the failing checks in one call. Sessions cover the last 30 days and logs are
masked. Ask for `gzip: true` to get a transcript whole; read a plain log in
pieces with `offset`. To ask how the factory has done over a year rather than a
month, or whether a release changed the park or failure rates, call
`factory_outcomes` with `by` and a window: it counts, and says nothing of any
issue.

A client config to paste, as an example. Your client's file and field names may
differ:

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

The tools, their arguments and the limits are in [The MCP server](/mcp-server).

## What an agent may not decide

- **Authorizing work.** `tehas:ready` spends compute and only counts from an
  account on the allow list. An agent writes the issue; a person labels it.
- **What the factory proposes for itself.** An issue labelled
  `tehas:improvement` was filed by the improvement loop, from a model's reading
  of finished cycles. It is a proposal like any agent's issue: a person
  decides, and a person applies `tehas:ready`.
- **Merging.** Nothing merges on its own.
- **Secrets and accounts** when installing: the bot's token, the allow list and
  the model endpoint are the owner's to choose.
