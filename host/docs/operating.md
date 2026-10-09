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

To try again after fixing the cause (often: make the issue clearer):

1. remove `needs-human-attention`,
2. apply `tehas:ready` again.

The next cycle starts from the state the last one saved in
`.pi/work-state/<issue>.json`. If it halts at once complaining about that state,
start clean on the host with `tehas-work <issue> --restart`; branches and commits
from the earlier attempt are kept.

## Pause and resume

Apply **`tehas:pause`** to the control issue, the one `TEHAS_CONTROL_ISSUE`
names (in the example, issue 5). No new cycle starts while it is there. A cycle
already running finishes. Remove the label to resume.

## Limits that hold work back

- One cycle at a time.
- Inside a cycle, `TEHAS_CONCURRENCY` workstreams side by side: 1 by default,
  more if the model backends can take it. See [The host](/host).
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
