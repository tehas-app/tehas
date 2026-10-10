---
title: Overview
description: "Tehas is a software factory powered by the Pi coding agent. How a GitHub issue flows through it to a reviewed pull request."
---

# Tehas

Tehas is a software factory powered by the [Pi coding agent](https://pi.dev)
and [pi-rukas](https://github.com/trail-openers/pi-rukas). It turns a GitHub
issue into a reviewed pull request without a person driving an agent session. GitHub issues are the only control plane: you label an issue,
tehas works on it, and the result comes back as a pull request and comments on
that issue. A human merges. Tehas never deploys and never touches production.

All inference runs on self-hosted models, on an OpenAI-compatible endpoint the
operator names. This site is the operating manual of one instance.

## One instance, one product

An instance is one host serving one product repository. What differs between
instances is in an instance directory: two small files, `instance.env` and
`models.json`. The scripts are the same everywhere.

The examples on this site come from the instance that works on tehas itself:

| Thing            | Value in the example                                    |
| ---------------- | ------------------------------------------------------- |
| Host             | `tehas-tehas`, Unix user `boss` (`ssh boss@tehas-tehas`) |
| Product          | `janit/tehas`, default branch `main`                         |
| Product checkout | `~/tehas/repos/tehas`                                        |
| Platform checkout | `~/tehas/platform`, pulled with a read-only deploy key |
| Settings         | `instances/tehas-tehas/` in the platform checkout       |
| Dashboard        | <http://tehas-tehas:8080> on the private network, published as <https://tehas.app> |
| Control issue    | 1                                                       |

## How an issue flows

```
you: write an issue with acceptance criteria
you: apply the label tehas:ready            <- the authorization to spend compute
dispatcher: picks it, labels it tehas:running
pi-rukas /work cycle, in a sandbox on the host:
    explore -> plan -> worktrees -> develop -> verify gates
    -> adversarial loop -> commit + PR -> six-pass review
dispatcher: labels it tehas:awaiting-merge, comments the PR link,
            posts token usage on the PR
you: review, click through, merge
```

If the cycle cannot finish, the issue gets `needs-human-attention` and a comment
saying where it stopped.

## What runs where

| Where          | Role                                                                                                                  |
| -------------- | --------------------------------------------------------------------------------------------------------------------- |
| The host       | Tehas: dispatcher, sandboxed agents, the gates, this site. One Unix user, everything under `~/tehas`.                 |
| Model endpoint | All model inference. The host needs no GPU; it reaches the endpoint over a private network.                           |
| GitHub         | The queue, the state (labels) and the results (pull requests and comments).                                           |

## What is optional

tehas needs a host, a product repository, a bot account and any
OpenAI-compatible model endpoint. Everything in this table can be left out, and
the factory works the same without it.

| Part                 | When it applies                                                                                  | Without it                                             |
| -------------------- | ------------------------------------------------------------------------------------------------ | ------------------------------------------------------ |
| Energy use           | Only when the model endpoint is a [viiwork](https://github.com/janit/viiwork) mesh, and `tehas-energy.service` is enabled | The section and the meter are not on the factory floor |
| viiwork              | One possible model backend, the one the example instance uses. tehas does not require it         | Use any other OpenAI-compatible endpoint               |
| Protolab             | Only if you label issues `tehas:prototype`. The dispatcher never acts on that label              | The Protolab is empty                                  |
| Search indexing      | `TEHAS_INDEXING=1`, for an instance published on the internet                                    | Every page says noindex                                |
| Visitor analytics    | `TEHAS_ANALYTICS_SRC` and `TEHAS_ANALYTICS_SITE`, for an instance that wants to count visits     | No script is loaded                                    |
| Factory floor        | The game view on the front page. The [plain dashboard](/dashboard) shows the same queue          | Use `/dashboard`                                       |

## Start here

- [Factory floor](/): the front page. The factory as a 16-bit game view: the
  live queue, the selected work order, the running cycle's console.
- [Plain dashboard](/dashboard): what is in line, being worked on (with its live console),
  waiting for you and done.
- [Pi software factory](/pi-software-factory): what tehas is made of, and
  what it adds to running Pi by hand.
- [Features](/features): everything it does, and how far each part is proven.
- [Operating tehas](/operating): submit work, pause, retry, merge.
- [Commands](/tools): every script and what it prints.
- [The host](/host): layout, settings, models, the sandbox image, installing.
- [Bootstrap a host](/bootstrap): preparing a fresh machine.
- [A new project](/new-project): installing tehas for another repository.
- [Troubleshooting](/troubleshooting): when a cycle parks or a tool fails.
