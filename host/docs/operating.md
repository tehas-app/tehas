---
title: Operating
description: "Running a tehas factory day to day: writing an issue, labels, review, pause and retry."
---

# Operating tehas

Everything on this page is done in GitHub unless a command is shown. Commands
run on the host as the tehas user (in the example, `ssh boss@tehas-tehas`).

## Prototypes: before the factory

Optional. The factory needs no prototype stage: an issue can be written and
labelled `tehas:ready` directly, as in step 1 below. Skip this section if you
do not use the label.

A prototype is an ordinary issue with the label **`tehas:prototype`**. The
label puts the ticket at the prototype stage: an idea that is still being
worked out. That work can happen anywhere, with other tools and systems, for
as long as it takes. tehas does not act on `tehas:prototype`: the dispatcher
never starts a cycle because of it.

The **Protolab** is where tickets at the prototype stage are kept. It is a
small site of its own, on port 8081 of the host, with a page for every open
prototype: the title, a link to the issue, and what is attached to the issue.
On the [factory floor](/) the Protolab panel lists the same tickets and each
row opens its page.

To put something on a ticket's page, attach it to the issue on GitHub, in the
issue text or in a comment, the way any file is attached there. The page has
it within about five minutes. It takes:

- images (png, jpg, gif, webp, svg, avif) and videos (mp4, webm, mov), shown
  as they are,
- HTML files, shown in a frame with a link to open them full size,
- the style sheets, scripts and data an HTML file refers to (css, js, mjs,
  json, txt). Attach them to the same ticket and refer to them by name, as
  `<script src="app.js">`.

The limits:

- **10 megabytes a ticket**, counted in the order the files were attached,
  and at most 40 files. What does not fit is left out, and the page lists it
  under "left out" with the reason.
- **Static files only.** Nothing is run on the host for a prototype, and an
  archive is not unpacked.
- **Only files attached by an owner, member or collaborator** of the
  repository are taken. Links by anyone else are counted on the page and not
  fetched.
- An HTML prototype runs in the browser's sandbox, apart from the dashboard
  and from every other prototype. Scripts work; storage in the browser
  (`localStorage`, cookies) does not.

The page and its files go when the ticket is closed or loses the label. They
are copies: GitHub keeps what was attached.

A prototype goes to the factory when it is ready for implementation, which
means:

- the issue body has a good description, written as in step 1 below,
- reference material is attached where it helps: images, or a zip of a
  prototype built elsewhere,
- someone applies `tehas:ready`, as in step 2.

Attachments are reference for people today. The cycle reads the issue body and
nothing else, so anything it must know has to be in the text.

## 1. Write an issue tehas can build

The cycle reads the issue body and nothing else. Give it:

- one outcome, small enough for one pull request,
- a `## Acceptance criteria` section with checkable items,
- the files or area involved, if you know them.

Vague issues park at the first step with "underspecified". Text in the issue is
treated as untrusted input: it cannot change the gates, and the cycle halts if
its patch touches `.pi/`, `.github/` or `AGENTS.md`.

## 2. Authorize it

Apply the label **`tehas:ready`**. That label is the authorization to spend
compute on the issue.

- Only a label applied by an allowed account counts: the logins in
  `TEHAS_ALLOWED_LABELERS` (in the example, `janit`). A `tehas:ready` applied
  by the bot or anyone else is ignored and logged.
- Add **`tehas:urgent`** as well to move it ahead of other ready issues.

## 3. What happens next

Watch it on the [factory floor](/) or the [plain dashboard](/dashboard): the queue, the issue being worked on with its
console streaming live, and what is waiting for you.

Optional, and only when the model backend is a [viiwork](https://github.com/janit/viiwork) mesh with
`tehas-energy.service` enabled: the factory floor also has a section
**Energy use**: what the machines serving the factory draw now, their kWh over
24 hours and 30 days, the last 30 minutes as a chart, and the factory's models
with their busy slots. The figures are the mesh's own and are good to about
15%. Only machines that are online and serve a model the factory uses are
counted; other machines of the mesh and other models are left out. A counted
machine's draw is all of it, including any other model it serves.
While the section is there, a live meter stands among the doors on the first
screen: the draw now, and the tokens the mesh produced for the factory's models
in the last minute. The tokens are the mesh's count for those models, so other
users of the same models are in it.
With any other backend the section and the meter are simply not there, and
nothing else changes. To turn them off on a viiwork mesh, see
[the energy sampler](/tools).

The dispatcher takes issues urgent first, then by the lowest issue number: one at a time unless the host is set to work on more (see [The host](/host)). A running cycle is never stopped to make room for an urgent one.

| Label on the issue      | Meaning                           | Your move                                |
| ----------------------- | --------------------------------- | ---------------------------------------- |
| `tehas:ready`           | Queued                            | Wait, or remove the label to withdraw it |
| `tehas:running`         | A cycle owns it                   | Leave the issue and its branch alone     |
| `tehas:awaiting-merge`  | Pull request is open and reviewed | Review and merge                         |
| `needs-human-attention` | The cycle stopped                 | Read the comment; see below              |
| `tehas:plan`            | A planning session is asked for   | Wait for the first comment               |
| `tehas:planning`        | A planning turn is running        | Wait                                     |
| `tehas:question`        | The factory asked something       | Tick a box; in a planning session you may reply instead |
| `tehas:improvement`     | Filed by the improvement loop     | Read it; apply `tehas:ready` if it should be built |

When the cycle starts and when it ends, the bot comments on the issue. The token
usage per agent role is posted on the pull request.

The console on the [factory floor](/) and on the [plain dashboard](/dashboard)
shows one cycle at a time. When several are running, or one is running and an
earlier one can still be read, a row of tabs above it names each by its issue
number.

A timer polls every two minutes. To see the factory's state, or to see or force
a poll by hand on the host:

```
tehas status                 # taking work? the queue, the current or last cycle
tehas dispatch --dry-run     # what would it do?
tehas dispatch               # do it (returns once the cycle is started)
systemctl --user status 'tehas-cycle@*'   # which cycles are running
systemctl --user stop tehas-cycle@12      # stop the cycle for issue 12
```

A stop ends the container. The issue gets `needs-human-attention`, and the
console shows the cycle as interrupted.

`tehas` is the one command to type on the host; `tehas help` lists what it
does. See [Commands](/tools).

## 4. Review and merge

The pull request comes from the bot account against the product's default
branch (in the example, `main`). It has passed the product's fast gate, its
full gate, its smoke command and pi-rukas's six-pass review, but none of those
is a human reading the diff. Nothing merges on its own.

After merging, the issue closes and leaves the queue. If you close a pull
request without merging, remove `tehas:awaiting-merge` from the issue by hand;
Tehas does not notice a rejected pull request yet.

## 5. When a cycle stops

`needs-human-attention` means the cycle parked. The bot's comment gives the
status, the last step and the log file on the host. pi-rukas usually adds its own
handoff comment with the reason and numbered options.

To try again after fixing the cause (often: make the issue clearer), tick
`Retry from a clean start` in the bot's comment. Tick `Leave it parked` to
take the question away and keep the issue as it is. The old way still works:
remove `needs-human-attention` and apply `tehas:ready` again. A cycle started
that way takes `tehas:question` off the issue, and the old boxes no longer do
anything.

When the bot cannot post the boxes, it posts a plain comment that gives the
old way. When the boxes are posted and the label `tehas:question` cannot be
added, a second comment says that the boxes cannot be used, tells you to run
`tehas labels` on the host, and gives the old way.

A tick counts only from an allowed labeller, and only while the issue is
parked: it carries `needs-human-attention` and neither `tehas:running` nor
`tehas:awaiting-merge`. It is used once. When the cycle starts, the bot unticks
the box, so the same tick cannot start a second cycle. A box ticked by anyone
else, or more than one box, is unticked and the bot says so in a comment.
A retry ticked on an issue that is not parked is unticked too, and the bot
says that nothing was started.
A paused factory, one with every slot taken, or one at the cap of
`TEHAS_MAX_AWAITING`, holds the retry back as it holds any cycle. The tick
stays and is used when there is room.

The host keeps the id of the question it posted when the cycle stopped. A
tick counts only in that comment; a box in any other comment does nothing.
On an issue for which the host has no question on record, a box ticked in a
comment of the bot that looks like a question is unticked, and the bot says
that the question is not one the factory is waiting on and gives the old way.

The next cycle starts in a fresh workspace, with `--restart`. Branches and
commits the earlier attempt pushed are kept on GitHub. What it left only on the
host, such as commits on worktrees that were never pushed, is moved to
`~/tehas/work/tehas-<issue>.before-retry` and stays there until a cycle of that
issue merges or leaves a pull request. Only the latest stopped attempt is kept:
a second retry replaces it. Recover what you need from a stopped cycle before
you retry twice.

A label or a tick approves the text of the issue as it was at that moment. If
someone other than an allowed labeller changes the title or the body
afterwards, the issue is skipped and the dispatcher's log says who changed it.
Read the new text. Then approve it again: for the label, remove `tehas:ready`
and apply it again; for a retry tick, untick the box and tick it again.

## Planning on an issue

A planning session is a conversation with a planning agent that ends in filed
issues. It runs on the issue, so you can steer it from a phone. The agent is
not asked to build code, and the awaiting-merge limit does not hold a planning
turn back. Its container has the same access as a work cycle's, token
included, so treat `tehas:plan` as you treat `tehas:ready`.

An issue is in one flow at a time. Applying `tehas:ready` to an issue with an
open planning session starts nothing until the session is ended. The
dispatcher's log says so.

1. Write an issue that says what you want planned.
2. Apply `tehas:plan`. As with `tehas:ready`, only a label applied by an
   allowed account counts, and it approves the text as it was when you
   applied it.
3. The bot comments that the session has started. Each turn of the agent ends
   in a comment with what it found. When it needs a decision, the comment
   ends in a question with one checkbox per choice, a last box `End this
   planning session`, and the label `tehas:question` is on the issue. When
   the agent offers more than 20 choices, the first 20 get a box and a line
   says how many were left out; reply to name another one. The
   last box and the label come after every turn, whether or not the agent
   asked something, and a reply or the last box works either way.
4. Answer by ticking one box, or by replying in a comment when none of the
   boxes fits. If you do both, the reply counts. The next poll that is free
   continues the session with your answer, and the issue carries
   `tehas:planning` while the turn runs. A poll waits for a planning turn,
   so while another planning turn runs, your answer waits until it ends,
   which can be an hour, and no new cycle starts either. A running work
   cycle does not hold an answer back. A poll also handles one answer or one
   planning start and then stops, and it does nothing while the model
   endpoint does not list the model.
5. The agent files the issues it plans with the bot's token. To finish, tick
   `End this planning session`. Filed issues are never started on their own:
   apply `tehas:ready` to the ones you want built.

Only answers from allowed labellers count. A box ticked by anyone else, or
more than one box, is unticked and the bot says so. A reply counts only if its
author is an allowed account and, when it was edited, its last editor is too.
Replies from others are ignored. Only the question the session is waiting on
counts, which is the one whose id the host has on record. Any other comment
with boxes, older or newer, does nothing.

What the agent writes in the bot's comments is posted on the issue. On a
public repository that is public. Secrets and private network addresses are
masked there, as on the dashboard, and the mask catches accidents only. The
issues the agent files are written directly and are not masked: they are
public as written. Do not put anything in the issue that should not be public.

If you tick a box in a planning question and nothing happens, the labels may
be missing from the repository. The bot finds its questions by
`tehas:question`, so without that label the boxes are posted and ticking them
does nothing. Run `tehas labels` on the host, as described in
[Commands](/tools).

While a question waits, the factory works on other issues. A session ends
when you tick the last box, when an answer arrives after
`TEHAS_PLAN_MAX_TURNS` turns (20 unless the instance says otherwise) have
run, or when the issue is closed or loses `tehas:question`. The first two
remove the factory's labels from the issue, `needs-human-attention` included,
and say so in one comment. To plan again, apply `tehas:plan` again.

If a turn does not finish, for example because the host restarted, the bot says
so in a comment that names the log, and the issue gets `needs-human-attention`
next to `tehas:question`. Reply to run the turn again with your reply as the
next message. That holds for a first turn too, when it failed after its
session had come into being, for example by running out of time. If a first
turn fails before that, or is cut off by a restart of the host, there is no
session to continue: the bot says so, the issue gets `needs-human-attention`,
and you apply `tehas:plan` again.

The bot account is never one of the allowed labellers. If it is listed in
`TEHAS_ALLOWED_LABELERS`, the dispatcher stops with an error rather than let
the bot approve its own questions. It checks this when it reads a question, so
the error shows once a question is open.

## Pause and resume

Apply **`tehas:pause`** to the control issue, the one `TEHAS_CONTROL_ISSUE`
names (in the example, issue 1). No new cycle starts while it is there. A cycle
already running finishes. The same label also stops planning turns, answers
to planning questions and retries by a tick, because the poll ends before it
reaches them. Remove the label to resume.

## Limits that hold work back

- `TEHAS_TICKET_CONCURRENCY` cycles at a time: 1 by default.
- Inside a cycle, `TEHAS_CONCURRENCY` workstreams side by side: 4 by default,
  fewer if the model backends cannot take it. See [The host](/host).
- No new cycle while 3 issues carry `tehas:awaiting-merge` or are being worked
  on (`TEHAS_MAX_AWAITING`). Unreviewed pull requests are the queue you control.
- A self-hosted model endpoint has few real slots per model, so a cycle takes
  a while; the six-pass review is the slowest part.

## The improvement loop

Optional. It is off unless the instance sets `TEHAS_IMPROVE_REPO` and the timer
`tehas-improve.timer` is enabled; see [Commands](/tools).

Every three hours a model reads the cycles that ended since the last run and
says what could be better: in the dispatcher, the cycle, pi-rukas, the
console, the gates or this manual. A finding that is not already in the issue
stack is filed as an issue of its own in that repository.

- **The label is `tehas:improvement` and nothing else.** tehas does not act on
  it, and the loop never applies `tehas:ready`. A filed issue is a proposal a
  model wrote: read it, correct it, and apply `tehas:ready` yourself if it
  should be built. Close it if not.
- **At most 3 issues a run, and none while 10 are open** with the label. Close
  some or build some and the loop files again.
- **The whole log is linked.** Each issue links the logs of the cycles it
  names, gzipped, as assets of the release `tehas-improve-logs` in that
  repository. In a public repository they are public. A log that holds
  something shaped like a token is not uploaded, and the issue says so.
- **One line per run** in `~/tehas/logs/improve.log`:

```
2026-10-11T09:00:12Z improve: cycles=4 findings=3 invalid=0 duplicates=1 of=#41 over=0 filed=2 issues=#57,#58
```

`cycles` is how many were read and `findings` what the model gave. `invalid`
were dropped for not being the shape asked for, `duplicates` were already in
the stack or given twice (`of` names the issues the model pointed at), and `over` were more
than the limits allow and are dropped. `filed` and `issues` are what was
opened.

To turn the loop off, remove `TEHAS_IMPROVE_REPO` from `instance.env`: cycles
stop leaving records and a run does nothing. Or stop the timer with
`systemctl --user disable --now tehas-improve.timer`.

## Run one issue by hand

Skips the queue, the labels and the comments:

```
tehas work 123            # exit 0: merged or parked, 1: aborted, 2: died or timed out
tehas work 123 --restart
```

It does not take the dispatcher's lock, so a poll can start a queue cycle while
yours runs. Apply `tehas:pause` first, or check that nothing is ready.

## Watching a running cycle

A cycle cannot be attached to or made interactive. It is pi-rukas run headless:
Pi in RPC mode with no terminal, given the one prompt `/work <issue>` and set to
approve its own steps. There is no prompt to type at and nothing waits for an
answer.

What you can do is watch it, on the [factory floor](/) or on the host:

```
tail -f ~/tehas/live/console-123.txt     # the console the dashboard shows
tail -f ~/tehas/logs/work-123-*.jsonl    # the raw RPC stream
docker exec -it tehas-work-123 bash      # a shell in the cycle's container
```

The shell is beside the session, not in it, and it is in the workspace the
agents are working in: what you change there changes the cycle. Do not use
`docker attach`. It shows the raw stream, not a Pi interface, and an interrupt
typed there is passed to Pi and ends the cycle.

To steer a cycle, let it park or stop it, make the issue clearer and apply
`tehas:ready` again. To work with pi-rukas interactively, start a session of
your own, as below.

## Work by hand on the host

The host that runs the factory is also a place to work yourself. Log in with
your own account and start a session:

```
ssh alice@tehas-tehas
tehas dev              # a session in your clone, under your name
tehas dev -- -r        # resume an earlier session
```

That is an interactive Pi with pi-rukas, in the sandbox, on the factory's
models. There is nothing to configure in your account: the image, the models,
the limits and the bot's token are the factory's own, and your name comes from
your own git configuration. An administrator lets an account in once, with
`sudo ./bootstrap.sh dev`; see [Bootstrap a host](/bootstrap).

- **Where you work.** A clone named after your login, made the first time
  from the product checkout, at the tip it last fetched. It is yours and it
  stays: tehas never resets or removes it. Pull inside the session if it is
  behind. It is in the tehas user's home, `~/tehas/dev/alice` there, so you
  reach it through a session and not from your own shell.
- **Whose name is on the commits.** `user.name` and `user.email` of your own
  account, read from your git configuration every time you start. Every
  commit of the session carries them, yours and the agents'. Nothing else of
  your configuration is used: no aliases, no signing key, no credentials.
- **Who runs it.** The session runs as the tehas user, through `sudo`. You get
  the session and nothing else of that account: no shell, no Docker, none of
  the other `tehas` commands.
- **Who pushes.** The bot. The push and the pull request go through its token,
  so GitHub shows the bot as the author of the pull request and your name on
  the commits. The author is a label, not a proof: it is whatever your git
  configuration says. A rule that needs a review from somebody other than the pull
  request's author looks at the bot, so read the commit authors before you
  approve.
- **The factory beside you.** Nothing is paused. A session and a cycle share
  the model endpoint, and the command tells you when a cycle is running. Apply
  `tehas:pause` first if you want the models to yourself. The energy section
  and the token meter count a session, because they measure the models; the
  factory floor does not show it.
- **Outside the queue.** No labels and no bot comments. The dispatcher does
  not know the session exists.

One session per clone at a time. What the session's container is given is on
[The host](/host).

Logged in as the tehas user itself, the same command takes a name for the
clone and an author, because that account is shared and has no name of its
own to offer:

```
tehas dev alice --author "Alice Example <alice@example.com>"   # the first time
tehas dev alice                                                # from then on
```

Without an author the commits carry the bot's name, and the command says so
when it starts.
