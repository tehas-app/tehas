---
title: Pi software factory
description: "Tehas is a software factory powered by the Pi coding agent and pi-rukas: a GitHub issue goes in, a reviewed pull request comes out, on self-hosted models."
---

# A software factory powered by the Pi coding agent

Tehas is a Pi software factory. It takes a GitHub issue, runs the
[Pi coding agent](https://pi.dev) on it inside a sandbox, and hands back a
reviewed pull request. Nobody sits in the agent session. A person writes the
issue, applies one label, and later reviews and merges.

The [factory floor](/) on this host is one such factory at work. This page
explains what it is made of.

Tehas is its own project. It is built on Pi and is not part of Pi or endorsed
by its maintainers.

## What a Pi software factory is

Pi is a minimal agent harness. You can use it in a terminal, run it in print or
JSON mode, control it over RPC or build on its SDK. It does the work of one
session: it reads code, runs commands, edits files and talks to a model.

A software factory is what stands around that session so that work can pass
through without someone driving it: a queue, a rule for who may start work, a
closed place for the agent to run, checks the result must pass, a review, and a
record of what happened. A Pi software factory is one where the agent doing the
work is Pi.

## Who does what

| Part           | What it does in tehas                                                                                                                                                              |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pi             | The coding agent. Tehas starts it as `pi --mode rpc` in a container, so it needs no terminal, and sends it one command: `/work <issue>`.                                           |
| pi-rukas       | A Pi extension that provides `/work`. It runs the cycle, starts child Pi processes by role (explore, developer, adversarial developer, code review) and keeps them in git worktrees. |
| Tehas          | The factory: the dispatcher that reads the queue, the check of who applied the label, the sandbox with its limits, the settings, the factory floor and the live console.            |
| GitHub         | The queue, the state and the results. Issues are the work orders, labels are the state machine, and the outcome is a pull request with comments.                                   |
| Model endpoint | All inference. A self-hosted, OpenAI-compatible endpoint the operator names, for example a [viiwork](https://github.com/janit/viiwork) mesh. The factory host needs no GPU. |
| A person       | Writes the issue, authorizes it with a label, reviews the pull request and merges.                                                                                                  |

[pi-rukas](https://github.com/trail-openers/pi-rukas) and
[Pi](https://github.com/earendil-works/pi) are separate projects with their own
repositories.

## How an issue becomes a pull request

1. Someone writes an issue with one outcome and a list of acceptance criteria.
2. An allowed account applies the label `tehas:ready`. That label is the
   authorization to spend compute. Applied by anyone else, it is ignored and
   logged.
3. The dispatcher polls the queue every two minutes, picks the issue and labels
   it `tehas:running`.
4. Pi starts in the sandbox and pi-rukas runs the cycle: explore, plan, a
   worktree for each workstream, develop.
5. The project's own gates run: the commands in `.pi/verify-cmd`,
   `.pi/verify-cmd-full` and `.pi/smoke-cmd`.
6. A second agent attacks the patch before anything is committed.
7. The cycle commits, opens a pull request and reviews it in six passes:
   security, error handling, type safety, performance, architecture and
   simplicity.
8. The dispatcher labels the issue `tehas:awaiting-merge`, comments the link
   and posts token usage for each agent role on the pull request.
9. A person reviews and merges. Tehas never merges, deploys or touches
   production.

A cycle that cannot finish leaves the label `needs-human-attention` and a
comment saying where it stopped. [Overview](/overview) has the same flow in
short, and [Operating](/operating) covers the day-to-day.

## What tehas adds to running Pi by hand

| Running Pi yourself                          | With tehas                                                                                                    |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| You start a session and stay with it         | Issues wait in GitHub and are picked up as slots come free, urgent ones first                                 |
| Whoever has the terminal decides what runs   | Only a label from an account on the allow list starts work                                                    |
| The agent runs with your files and your keys | It runs in a container with CPU and memory limits and one bot token: no Docker socket, no SSH keys            |
| You decide when it is done                   | The repository's own gates decide, and the cycle halts if its patch touches `.pi/`, `.github/` or `AGENTS.md` |
| Output scrolls past in your terminal         | The console streams to the factory floor with tokens and passwords masked, and usage is posted on the pull request |
| A stalled session waits for you to notice    | A stopped cycle labels the issue and says why; a pause label on the control issue holds the queue             |

Running Pi by hand is still there, on the same host. A person with an account
on it types `tehas dev` and gets an interactive session with pi-rukas in the
cycle's sandbox, on the factory's models, with their own name on the commits.
[Operating](/operating) says how.

## See it run

The [factory floor](/) shows the stages as stations: in line, building, review
and shipped. Beside them are the selected work order with its tokens, time and
estimated energy, the issue board and the console of the running cycle. When
the live queue cannot be read the page loads sample tickets and says so.

The [plain dashboard](/dashboard) has the same queue as lists.

## Questions

### Is this an official Pi project?

No. Tehas uses Pi as it is published, pinned by version. It takes pi-rukas from [upstream](https://github.com/trail-openers/pi-rukas), pinned to the newest release when tehas is released.

### Does tehas write code without anyone checking it?

It writes and reviews without anyone watching. It does not merge. Every pull
request waits for a person.

### Which models does it use?

Self-hosted ones, through an OpenAI-compatible endpoint named in the instance
settings. The example instance serves them with
[viiwork](https://github.com/janit/viiwork), a flexible meshed inference platform for both home and enterprise grade hardware;
any other OpenAI-compatible endpoint works too. One model serves every agent role today. A cycle needs a model that
can hold at least two conversations at once and a context of around 80,000
tokens; [Features](/features) explains what happens without that.

### How much of it is proven?

[Features](/features) lists every part with how far it has been checked, down
to what has only been tested with fake files. Read it before relying on
anything here.

### Can one factory serve several repositories?

Not yet. An instance is one host, one product repository and one cycle at a
time by default.

### Where does the name come from?

"Tehas" is Estonian for factory. "Pirukas", behind pi-rukas, is Estonian for
pie.

## Set one up

- [A new project](/new-project): installing tehas for another repository.
- [Bootstrap a host](/bootstrap): preparing a fresh machine.
- [For agents](/agents): handing the setup to a coding agent, starting from
  [`/llms.txt`](/llms.txt).
- Source: [github.com/janit/tehas](https://github.com/janit/tehas).
