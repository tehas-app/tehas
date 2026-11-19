---
title: The Lean Software Factory
faq: Questions
description: "Tehas is The Lean Software Factory: work is pulled from GitHub issues, one issue makes one reviewed pull request, the line stops on a defect, and it all runs on one host with self-hosted models."
---

# Tehas, The Lean Software Factory

Tehas is The Lean Software Factory. A GitHub issue goes in and a reviewed pull
request comes out, built by coding agents in a sandbox on self-hosted models. A
person decides two things: what to build and what to merge. The factory does
the rest, and it does it the way a lean factory runs a line.

The [factory floor](/floor) on this host is one such factory at work.

## What makes it lean

Lean manufacturing makes only what is asked for, one piece at a time, stops the
line when something is wrong, and keeps the work in sight. Tehas does the same
with code.

| Lean idea                | What it is in tehas                                                                                                                                                             |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pull, not push           | Nothing runs until a person on the allow list labels an issue `tehas:ready`. The dispatcher pulls labelled issues from the queue as slots come free.                            |
| Limit work in progress   | At most `TEHAS_MAX_CYCLES` cycles run at once, the host's CPUs minus one by default, and fewer while the host has no room.                                                       |
| One-piece flow           | One issue is one outcome and one pull request. A cycle explores, plans, develops, checks and reviews that one piece before it hands it on.                                     |
| Build quality in         | The repository's own gates decide when a change is done. A second agent attacks the patch before it is committed, and the pull request is reviewed in six passes.               |
| Stop the line            | A cycle that cannot finish stops, labels the issue `needs-human-attention` and says where it stopped. A cycle that touches `.pi/`, `.github/` or `CODEOWNERS` halts.            |
| The andon cord           | A pause label on the control issue holds the whole queue: nothing new starts until it comes off.                                                                                |
| Go and see               | The [factory floor](/floor) and the [plain dashboard](/dashboard) show the queue and the console of every running cycle, live.                                                  |
| Improve the line         | Every run is recorded. [Analytics](/analytics) and the [MCP server](/mcp-server) say where runs failed, per version, so the next release can fix the cause.                    |
| No waste in the plant    | One host, one product repository, no GPU on the factory host. Inference runs on a model endpoint you already own. Tehas is shell scripts, Rust programs and a few containers. |

## What tehas leaves out

Lean is also what a factory does not do. Tehas never merges, deploys or
touches production. It has no control plane of its own: GitHub issues and
labels are the queue, the state and the record. One instance serves one
repository, so its settings are two small files.

## Built on Pi

The agent on the line is the [Pi coding agent](https://pi.dev), with the
[pi-rukas](https://github.com/trail-openers/pi-rukas) extension running the
work cycle. That also makes tehas a Pi software factory.
[Pi software factory](/pi-software-factory) says who does what, and what tehas
adds to running Pi by hand. Tehas is its own project and is not part of Pi.

## Questions

### What is a lean software factory?

A system that turns requests for change into finished, reviewed changes the way
a lean plant turns orders into goods: work is pulled when it is asked for, it
flows one piece at a time, quality is checked at every step, and the line stops
on a defect instead of passing it on. Tehas is one, with coding agents doing the
work and people deciding what to build and what to merge.

### How is it different from running a coding agent by hand?

Nobody sits in the session. Issues wait in GitHub and are picked up as slots
come free, the agent runs in a sandbox with limits and one narrow token, and the
repository's gates decide when it is done. [Pi software
factory](/pi-software-factory) has the full comparison.

### Does it need a large setup?

No. One Linux host runs the factory and needs no GPU. The models run on an
OpenAI-compatible endpoint you name, on your own hardware.
[A new project](/new-project) is the install guide.

### Does it merge or deploy anything?

No. Every pull request waits for a person to review and merge it.

### Is this the lean driver?

No. The lean driver, `tehas:lean`, is one way to run a single cycle and is
still being built; see [Choosing the driver](/operating#choosing-the-driver).
The whole factory is lean whichever driver runs a cycle.

### How much of it is proven?

[Features](/features) lists every part with how far it has been checked. Read
it before relying on anything here.

## Set one up

- [A new project](/new-project): installing tehas for another repository.
- [Overview](/overview): how an issue flows through the factory.
- Source: [github.com/tehas-app/tehas](https://github.com/tehas-app/tehas).
