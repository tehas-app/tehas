---
title: For agents
description: "Where a coding agent starts with this tehas factory: llms.txt, the manual as Markdown and the live data."
---

# For agents

A coding agent can pick tehas up from one address: [`/llms.txt`](/llms.txt) on
this host. It says what tehas is, how to get work built by this factory, how to
set tehas up for another repository, and where the live data is.

## Hand it to your agent

Paste one of these, with this host's address in place of `<this host>`:

```
Read http://<this host>/llms.txt, then write an issue this factory can build for: <what you want>
```

```
Read http://<this host>/llms.txt, then set tehas up for this repository. Ask me before deciding anything about secrets, accounts or who may authorize work.
```

Away from this host, start from the README of the
[public repository](https://github.com/tehas-org/tehas) instead. A running factory
to look at is the public dashboard at <https://tehas.app/>.

## What an agent can read here

| Address                                 | What it is                                                     |
| --------------------------------------- | -------------------------------------------------------------- |
| `/llms.txt`                             | The entry point: a summary and links, as plain text            |
| `/<page>.md`                            | Any manual page as Markdown, for example `/new-project.md`     |
| `/health`                               | Factory health as JSON; 503 when unhealthy                     |
| `/live/status.json`                     | The queue snapshot: repository, issues by state, paused or not, and the most used words of all its issues |
| `/live/running.json`                    | The running cycles and the one that finished last              |
| `/live/endings.json`                    | The last ten cycles that ended, oldest first                   |
| `/live/console-<issue>.txt`             | That cycle's console, with tokens and passwords masked         |

All of it is read-only. Work is submitted and authorized in GitHub, as
[Operating](/operating) describes.

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
