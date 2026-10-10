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

Optional, and only when the model backend is a viiwork mesh with
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
[tehas-energy](/tools).

The dispatcher takes one issue at a time, urgent first, then the lowest issue
number.

| Label on the issue      | Meaning                           | Your move                                |
| ----------------------- | --------------------------------- | ---------------------------------------- |
| `tehas:ready`           | Queued                            | Wait, or remove the label to withdraw it |
| `tehas:running`         | A cycle owns it                   | Leave the issue and its branch alone     |
| `tehas:awaiting-merge`  | Pull request is open and reviewed | Review and merge                         |
| `needs-human-attention` | The cycle stopped                 | Read the comment; see below              |
| `tehas:plan`            | A planning session is asked for   | Wait for the first comment               |
| `tehas:planning`        | A planning turn is running        | Wait                                     |
| `tehas:question`        | The factory asked something       | Tick a box; in a planning session you may reply instead |

When the cycle starts and when it ends, the bot comments on the issue. The token
usage per agent role is posted on the pull request.

A timer polls every two minutes. To see or force a poll by hand on the host:

```
tehas-dispatch --dry-run     # what would it do?
tehas-dispatch               # do it (returns when the cycle ends)
```

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
remove `needs-human-attention` and apply `tehas:ready` again. When the bot
could not post the boxes, its comment says so and gives the old way.

A tick counts only from an allowed labeller, and only while the issue is
parked: it carries `needs-human-attention` and neither `tehas:running` nor
`tehas:awaiting-merge`. It is used once. When the cycle starts, the bot unticks
the box, so the same tick cannot start a second cycle. A box ticked by anyone
else, or more than one box, is unticked and the bot says so in a comment.
A paused factory, or one with `TEHAS_MAX_AWAITING` pull requests waiting,
holds the retry back as it holds any cycle.

The next cycle starts in a fresh workspace, with `--restart`. Branches and
commits the earlier attempt pushed are kept on GitHub; what it left only on the
host is gone.

A label or a tick approves the text of the issue as it was at that moment. If
someone other than an allowed labeller changes the title or the body
afterwards, the issue is skipped and the dispatcher's log says who changed it.
Read the new text. Then approve it again: for the label, remove `tehas:ready`
and apply it again; for a retry tick, untick the box and tick it again.

## Planning on an issue

A planning session is a conversation with a planning agent that ends in filed
issues. It runs on the issue, so you can steer it from a phone. Nothing in it
builds code: a planning turn opens no pull request, and the awaiting-merge
limit does not hold it back.

1. Write an issue that says what you want planned.
2. Apply `tehas:plan`. As with `tehas:ready`, only a label applied by an
   allowed account counts, and it approves the text as it was when you
   applied it.
3. The bot comments that the session has started. Each turn of the agent ends
   in a comment with what it found. When it needs a decision, the comment
   ends in a question with one checkbox per choice, a last box `End this
   planning session`, and the label `tehas:question` is on the issue. The
   last box and the label come after every turn, whether or not the agent
   asked something, and a reply or the last box works either way.
4. Answer by ticking one box, or by replying in a comment when none of the
   boxes fits. If you do both, the reply counts. The next poll that is free
   continues the session with your answer, and the issue carries
   `tehas:planning` while the turn runs. A poll takes the dispatch lock, so
   while a work cycle or another planning turn runs, your answer waits until
   it ends, which can be hours. A poll also handles one answer or one start
   and then stops, and it does nothing while the model endpoint does not list
   the model. A running planning turn holds the queue the same way a cycle
   does.
5. The agent files the issues it plans with the bot's token. To finish, tick
   `End this planning session`. Filed issues are never started on their own:
   apply `tehas:ready` to the ones you want built.

Only answers from allowed labellers count. A box ticked by anyone else, or
more than one box, is unticked and the bot says so. A reply counts only if its
author is an allowed account and, when it was edited, its last editor is too.
Replies from others are ignored. Only the newest question the bot posted counts;
an older comment with boxes does nothing.

What the agent writes in the bot's comments is posted on the issue. On a
public repository that is public. Secrets and private network addresses are
masked there, as on the dashboard, and the mask catches accidents only. The
issues the agent files are written directly and are not masked: they are
public as written. Do not put anything in the issue that should not be public.

If you tick a box and nothing happens, the labels may be missing from the
repository. The bot finds its questions by `tehas:question`, so without that
label the boxes are posted and ticking them does nothing. Run
`tehas-dispatch --init-labels` on the host, as described in
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
next message. If the first turn does not finish, there is no session to
continue: the bot says so, the issue gets `needs-human-attention`, and you
apply `tehas:plan` again.

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

- One cycle at a time.
- Inside a cycle, `TEHAS_CONCURRENCY` workstreams side by side: 4 by default,
  fewer if the model backends cannot take it. See [The host](/host).
- No new cycle while 3 issues carry `tehas:awaiting-merge`
  (`TEHAS_MAX_AWAITING`). Unreviewed pull requests are the queue you control.
- A self-hosted model endpoint has few real slots per model, so a cycle takes
  a while; the six-pass review is the slowest part.

## Run one issue by hand

Skips the queue, the labels and the comments:

```
tehas-work 123            # exit 0: merged or parked, 1: aborted, 2: died or timed out
tehas-work 123 --restart
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
`tehas:ready` again. To work with pi-rukas interactively, apply `tehas:pause`
and run it through its own wrapper in a clone of your own. That is outside the
queue: no labels, no bot comments.
