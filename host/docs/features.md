---
title: Features
description: "What tehas does today, and how far each part is proven."
---

# What tehas does

Tehas turns GitHub issues into reviewed pull requests, using the Pi coding
agent and [pi-rukas](https://github.com/trail-openers/pi-rukas) on self-hosted
models. A person writes the issue, applies one label, and later reviews and
merges. Everything in between is tehas.

It is a set of shell scripts, one small Rust program and a few containers on
one host. There is no server process of its own and no database of its own:
state lives in GitHub labels, in pi-rukas's state files and in plain files under
`~/tehas`.

Each feature below says how far it has been proven. "Verified" means it was run
on a tehas host and the result checked. Rows with a date were checked on the
example instance. Rows without one were checked on another instance, before
settings moved into an instance directory. Nothing here has run for longer than
a day.

## Queue and dispatch

| Feature                                 | Detail                                                                                                                                      | Status                                                                                  |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| GitHub issues as the only control plane | Labels are the state machine: `tehas:ready`, `tehas:running`, `tehas:awaiting-merge`, `needs-human-attention`                               | Verified                                                                                |
| Authorization by label                  | A cycle starts only if `tehas:ready` was applied by an allowlisted account, and the text has not been changed by anyone else since. A label applied by the bot or anyone else is ignored and logged | The labeller check is verified. The rule about changed text is tested, not yet seen on a host |
| Polling                                 | A systemd user timer runs one poll two minutes after the previous one ended. A poll is a few GitHub calls and no model call                 | Verified                                                                                |
| Two commands                            | `tehas` is what a person types (`dispatch`, `work`, `index`, `labels`, `status`); `tehasbot` is what the units run. The scripts behind them are in `~/tehas/libexec`. `install.sh` removes the one-file-per-command layout that came before | Covered by the shell tests; not yet installed on a host |
| One cycle at a time                     | Urgent first (`tehas:urgent`), then lowest issue number                                                                                     | Verified                                                                                |
| Pause switch                            | `tehas:pause` on the control issue stops new cycles; a running one finishes                                                                 | Verified                                                                                |
| Review backpressure                     | No new cycle while 3 issues await merge                                                                                                     | Built, not yet reached                                                                  |
| Model endpoint check                    | The queue is held, not burned, while the model endpoint does not list the configured model                                                  | Built after a real failure; the hold path itself is untested                            |
| Retry                                   | Removing `needs-human-attention` and re-applying `tehas:ready` starts a fresh cycle                                                         | Verified                                                                                |
| Reporting                               | Start and outcome comments on the issue; token usage per agent role on the pull request                                                     | Issue comments verified; the pull request comment awaits the first cycle that opens one |

## The work cycle

| Feature                 | Detail                                                                                                                                   | Status                                                                             |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Headless pi-rukas       | `/work <issue>` is sent to `pi --mode rpc` inside the sandbox container; no terminal needed                                              | Verified                                                                           |
| Sandbox                 | Agents run in a container with CPU and memory limits, a fresh clone to work in and the bot token: no Docker socket, no SSH keys, no host environment. The network is not restricted | Verified with the product checkout mounted. The fresh clone per cycle is tested, not yet seen on a host |
| Project gates           | The cycle runs the project's own `.pi/verify-cmd`, `.pi/verify-cmd-full` and `.pi/smoke-cmd`                                             | Gates verified in the sandbox; first full cycle in progress at the time of writing |
| Protected gates         | pi-rukas halts a cycle whose patch touches `.pi/`, `.github/` or `AGENTS.md`                                                             | pi-rukas feature, not exercised here                                               |
| Human merge             | Nothing merges on its own                                                                                                                | By configuration (no merge grant in `AGENTS.md`)                                   |
| Self-hosted models only | One model for every role, set in the instance's `instance.env`                                                                           | Verified                                                                           |
| Work by hand            | `tehas dev` gives a person an interactive pi-rukas session on the host: the cycle's sandbox with a terminal, a clone that is kept, and the person's name on the commits. The push and the pull request are still the bot's | Covered by the shell tests, not yet seen on a host |
| Concurrency knob        | `TEHAS_CONCURRENCY` sets how many workstreams a cycle runs side by side; 4 by default, lower it when the model backends have few slots    | Setting reaches the sandbox; its effect on a cycle is not yet observed             |
| Code index              | `tehas index` builds the index behind the agents' code search, in the sandbox image; `install.sh` runs it once and the dispatcher refreshes it before a cycle when the branch has moved | Covered by the shell tests; run against the upstream image on a development machine, not yet on a tehas host |

## Visibility

| Feature                    | Detail                                                                                                                                 | Status                   |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ |
| Plain dashboard            | At `/dashboard`: in line, being worked on, waiting for review, stopped, completed (10 each, then a link to GitHub)                     | Verified                 |
| Live console               | The running cycle's agents, the commands they run and the start of each result, streamed to the dashboard; tokens and passwords masked | Verified on a real cycle as a Deno script. The Rust program that replaced it is tested, not yet seen on a host |
| Factory floor (game view)  | The front page: the stages as a 16-bit factory floor with the live queue, a work order monitor, an issue board and a breakroom         | Tested with fake files   |
| Production console         | The running cycle's console in the game view, read from the same `/live/` files as the dashboard                                       | Tested with fake files   |
| Energy use                 | Optional. In the game view, only when the model backend is a [viiwork](https://github.com/janit/viiwork) mesh and `tehas-energy.service` is enabled: draw now, kWh for 24 hours and 30 days, a 30-minute chart, the machines serving the factory and its models, and a live meter among the doors with the draw now and tokens a minute | Tested with fake files   |
| Protolab                   | A page for every open issue labelled `tehas:prototype`, on port 8081: title, link to the issue, and the attached images, videos and static HTML files, 10 MB a ticket. Served by a container of its own; HTML runs sandboxed. Listed in the game view. With `TEHAS_DOMAIN`, each ticket also at `lab-<issue>.<domain>` | Covered by the shell tests; run by hand against real attachments, not yet on a host |
| Previews                   | A running copy of the product for every open pull request whose branch is in the repository and that an owner, member or collaborator opened, at `uat-<pr>.<domain>`: built from the commit's `preview/Dockerfile`, locked down, removed when the pull request is merged or closed, with a second check every three hours. No build on a full disk; the pull request, the dashboard and `/health` say so. See [Previews](/edge) | Covered by the shell tests. The router's routes were run in Caddy 2.8.4 with a container on an internal network, and the preview image of this repository was built and run with a preview's options, both on a development machine. The domain, the certificate and the proxy were set up and checked from outside, 2026-10-10. `tehasbot preview` has not run on a host, against GitHub or Docker |
| Health endpoint            | `/health` and `/healthz` on port 8080: pass, warn (paused, or low on disk for previews) or fail (last cycle failed, answered with 503) | Tested with fake files   |
| Operating manual           | This site, Markdown rendered by the docs server with no build step                                                                     | Verified                 |

## Host management

| Feature                  | Detail                                                                                                           | Status                                               |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| Configuration from git   | `./install.sh` copies this repository's files into place and keeps the previous version; `--check` reports drift | Verified                                             |
| Host update in one step  | `tehas update` fast-forwards the platform checkout, runs `install.sh`, then its follow-ups; nothing new is added, and a step that did not run stays owed until it has | Covered by the shell tests; not yet run on a host    |
| Instance settings        | Everything that differs between instances is in one directory; no script names a product or a host               | Covered by the shell tests                           |
| Host bootstrap           | `bootstrap.sh` installs every tool at a pinned version and checks the result                                     | Check and a second run verified; a run from scratch is not |
| Read-only source         | The host clones this repository with a read-only deploy key; the bot has no access to it                         | Verified                                             |
| Pinned sandbox image     | Base by digest, tools by version; each build is also tagged by date for rollback                                 | Verified                                             |

## The example instance

The instance on the host `tehas-tehas` works on this repository, product
`janit/tehas`, with the settings in `instances/tehas-tehas/`. Each row
is what was seen there or in the tests, with its date.

| Check                                  | Result                                                                                                                                              | Status                   |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ |
| No product or host names in platform files | `scripts/check-generic.sh` passes. It is a lint against regressions, not proof                                                                  | Passing, 2026-10-08      |
| No script edits                        | Installed from `instances/tehas-tehas/` with no file changed outside `instances/`. One local line (`TEHAS_BRANCH`) was added on the host, see below  | Verified, 2026-10-08     |
| Fresh install                          | 39 files on that date; the docs server and the dispatcher timer run, and nothing else was put on the host                                           | Verified, 2026-10-08     |
| Invalid settings change nothing        | `install.sh` and `tehas dispatch` exit 78 with an empty or incomplete instance; `~/tehas` identical before and after                                | Verified, 2026-10-08     |
| Gates in the sandbox image             | The repository's own `.pi/verify-cmd-full` and `.pi/smoke-cmd` pass inside the image as the unprivileged user                                       | Verified, 2026-10-08     |
| What a cycle's container can reach     | The bot token and the listed mounts; no Docker socket, no SSH keys, no `~/.config/tehas`, not the platform checkout                                  | Verified, 2026-10-08     |
| Dispatch                               | The timer picked up a labelled issue unattended, checked who labelled it, relabelled it, commented, passed the push check and started the cycle      | Verified, 2026-10-08     |
| A cycle to a pull request              | Issue 6 went from a label to pull request 13 in 61 minutes, unattended: explore, plan, develop, adversarial review, commit, lens review. The change (3 files, 172 lines, 13 new test cases) passes the full gate. It took five attempts; see below | Verified, 2026-10-09     |
| Host stays responsive during a cycle   | Load 0.2 to 0.5 on 8 CPUs through a whole 61-minute cycle; the cycle's container at 5% of a CPU and about 200 MB                                    | Verified, 2026-10-09     |
| Bootstrap                              | `bootstrap.sh check` passes and a second `bootstrap.sh user` changes nothing. The host itself was installed by hand before the script existed       | A run from scratch is not proven |

What the five attempts showed, none of it a fault of the instance's setup:

- **A cycle needs two model conversations served at once.** pi-rukas's
  driving session takes turns of its own while a step's agent works. With one
  free slot, or with other agents using the model, calls are refused
  (`no free slot`) and the cycle stops after three retries. Attempts 1 to 3
  ended that way. The dispatcher only checks that the model is listed.
- **A renamed repository empties the queue without an error.** Fixed: the
  dispatcher now checks the name at every poll.
- **pi-rukas can lose a file an agent left uncommitted** (reported upstream as
  pi-rukas issue 1049). Attempt 4 did the work and failed its gate for that
  reason. `AGENTS.md` now tells agents to commit their own work, which avoids
  the faulty code; attempt 5 passed.
- **The pull request was opened against `main`**, not the branch the cycle
  started from, so its review read the whole difference between the two. The
  lens review ended incomplete and the issue carries `needs-human-attention`
  next to `tehas:awaiting-merge`. On an instance whose work branch is the
  default branch this does not arise.

One cycle is one cycle. It used about 8.5 million tokens for a 172-line change.

The local line: a cycle starts from the product's default branch, and this
repository's contract files were on a work branch when the instance was set up.
`TEHAS_BRANCH` named that branch on the host until it was merged.

## Not built

- Monitoring: availability probes, incident issues, weekly report.
- Electricity price scheduling. `tehas:urgent` only changes ordering today.
- Model evaluation; every role uses the same model by decision, not by
  measurement.
- Watching tehas itself: a stuck cycle, a failed refresh. A full disk is seen
  where previews are on, and stops previews only.
- More than one project per host. See [A new project](/new-project).
- A private database copy per cycle or per preview.

## Known limits

- One host, one project, one cycle at a time.
- The cycle's container holds the bot's token, so agent code can do what that
  token can. Give each instance its own bot account. See [The host](/host).
- A pull request closed without merging leaves `tehas:awaiting-merge` on its
  issue until someone removes it.
- A preview is public, and it is built from a pull request nobody has reviewed
  yet. Only a branch in the product repository gets one, and only when an
  owner, member or collaborator opened the pull request. See
  [Previews](/edge).
