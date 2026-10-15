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

The label counts when an account in `TEHAS_ALLOWED_LABELERS` applied it, which
is the same rule `tehas:ready` follows. A `tehas:prototype` applied by anyone
else, the bot included, counts as no label at all: the ticket gets no page, a
page it had is removed, and the Protolab's log names the issue and the account.
An account in a cycle holds the bot's token, and the bot can label, so the
account behind the label is what is checked, not the label itself. The check
only means something while the bot's own login is outside
`TEHAS_ALLOWED_LABELERS`, and the Protolab stops with an error when it is not.

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
- **Only a label from an allowed account** puts the ticket here, as said
  above: the most recent `tehas:prototype` labelling is by an account in
  `TEHAS_ALLOWED_LABELERS`.
- **Only files attached by an owner, member or collaborator** of the
  repository are taken. Links by anyone else are counted on the page and not
  fetched.
- An HTML prototype runs in the browser's sandbox, apart from the dashboard
  and from every other prototype. Scripts work; storage in the browser
  (`localStorage`, cookies) does not.

The page and its files go when the ticket is closed, loses the label, or
carries it from an account the instance does not allow. They are copies: GitHub
keeps what was attached.

A prototype goes to the factory when it is ready for implementation, which
means:

- the issue body has a good description, written as in step 1 below,
- reference material is attached where it helps: images, or a zip of a
  prototype built elsewhere,
- someone applies `tehas:ready`, as in step 2.

Attachments are reference for people today. The cycle reads the issue body and
nothing else, so anything it must know has to be in the text. For a picture
that means words: name the element by its selector and its visible title, and
say where in it the thing goes.

An instance can give a cycle the images of its issue, with
`TEHAS_WORK_IMAGES=1`. [tehas work](/tools#tehas-work) then fetches the images
the issue text links, and the first two steps of the cycle, explore and plan,
are shown them beside the text. The plan puts the placement into words, and
the later steps read the plan. Images in comments are not read, and no cycle
has been seen to use a picture yet, so words in the text stay the safe way.

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
  by the bot or anyone else is no one's request for a cycle: the dispatcher
  takes it off the issue, says in a comment on the issue who applied it, and
  logs it. An account in `TEHAS_ALLOWED_LABELERS` applies it again to ask for
  a cycle. GitHub has no permission per label: any account that can write to
  the repository can apply any label, so the list is enforced here, at the
  next poll, and not by GitHub.
- Add **`tehas:urgent`** as well to move it ahead of other ready issues. It is
  checked like `tehas:ready`: only one applied by an allowed account moves the
  issue; one applied by the bot or anyone else is ignored and logged.

The factory works on open issues only. A closed issue is never taken, and the
number of a pull request is never taken, open, merged or closed. A label does
not change that. Before it marks an issue `tehas:running` the dispatcher asks
GitHub what the number is, and `tehasbot cycle` asks the same before it changes
a label or starts a container, whether the poll started it or a person did. An
issue closed between the poll's list and its start is skipped, with one line in
the log: `skip #N: it is closed, and a closed issue is never taken`. A cycle
that came up and then found its issue closed says so in its own log and does
nothing on the issue; it leaves `tehas:running` behind, and the next poll takes
that mark off without a word. The queue
and the dashboard list open issues, so a closed one that still carries a
factory label is not counted in line or as being worked on.

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
While the section is there, a live meter stands at the right end of the panel
that opens the first screen, over its title and the lines under it: the draw
now, and the tokens the mesh produced for the factory's models in the last
minute. The tokens are the mesh's count for those models, so other users of the
same models are in it. Below 900px the meter lies across the foot of that panel
instead. It leads to Energy use.
With any other backend the section and the meter are simply not there, and
nothing else changes. To turn them off on a viiwork mesh, see
[the energy sampler](/tools).

The dispatcher takes issues urgent first, then by the lowest issue number: as many at a time as the host's ceiling and the slice's room allow (see [Limits that hold work back](#limits-that-hold-work-back)). A running cycle is never stopped to make room for an urgent one. The dashboard lists the queue by issue number, not the order the dispatcher will work it in: whether a `tehas:urgent` moves the queue is decided by who applied the label, not by its being there.

| Label on the issue      | Meaning                           | Your move                                |
| ----------------------- | --------------------------------- | ---------------------------------------- |
| `tehas:ready`           | Queued                            | Wait, or remove the label to withdraw it |
| `tehas:running`         | A cycle owns it                   | Leave the issue and its branch alone     |
| `tehas:awaiting-merge`  | Pull request is open and reviewed | Review and merge                         |
| `tehas:shipped`         | A merged pull request closed it   | Nothing; the bot gives it                |
| `needs-human-attention` | The cycle stopped                 | Read the comment; see below              |

`tehas:ready` from an allowed labeller starts a fresh cycle whatever else the
issue carries. The labels of the earlier outcome go when the cycle starts. On
an issue whose cycle is still running, the label waits: a fresh cycle starts
when that one has ended. Remove the label before then to withdraw it.

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
tehas status                 # taking work? the queue, the current or last cycle, the slice's room
tehas top                    # the same overview, on one screen that redraws itself
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
take the question away and keep the issue as it is. A label works as well: apply `tehas:ready`. `needs-human-attention` does not
have to be removed first. A cycle started
that way takes `tehas:question` off the issue, and the old boxes no longer do
anything.

An issue closed since its cycle stopped is not work. Neither the tick nor a new
`tehas:ready` starts anything on it, and the poll's log says so.

When the bot cannot post the boxes, it posts a plain comment that gives the
label to apply. When the boxes are posted and the label `tehas:question` cannot be
added, a second comment says that the boxes cannot be used, tells you to run
`tehas labels` on the host, and names the label.

A cycle that was killed outright cannot report, for example when the host
lost power. The next poll finds its issue: open, marked `tehas:running`, and
no cycle on the host behind it. It takes `tehas:running` off, adds
`needs-human-attention` and says so in a plain comment without boxes. Look for
a pull request the cycle may have opened, then apply `tehas:ready` to retry.
The poll starts nothing for it by itself. A closed issue in that state only
loses `tehas:running`.

A tick counts only from an allowed labeller, and only while the issue is
parked and open: it carries `needs-human-attention` and neither
`tehas:running` nor `tehas:awaiting-merge`. It is used once. When the cycle starts, the bot unticks
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
that the question is not one the factory is waiting on and names the label.

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
   applied it. From any other account it is taken off the issue with a
   comment, as `tehas:ready` is.
3. The bot comments that the session has started. Each turn of the agent ends
   in a comment with what it found. A turn is over when the agent has stopped
   and every subagent it sent out has reported back, so one turn can take
   many minutes. When it needs a decision, the comment
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
5. The agent shows you a plan before it files it, and files it when you
   confirm. Every turn of a session works on the commit the session started
   on, so the plan you confirm is the plan that is filed, word for word, if
   you confirm within two hours of seeing it. After that the agent reviews it
   again before it files. The agent files the issues it plans with the bot's
   token. To finish, tick
   `End this planning session`. Filed issues are never started on their own:
   apply `tehas:ready` to the ones you want built.

Only answers from allowed labellers count. A box ticked by anyone else, or
more than one box, is unticked and the bot says so. A reply counts only if its
author is an allowed account and, when it was edited, its last editor is too.
Replies from others are ignored. Only the question the session is waiting on
counts, which is the one whose id the host has on record. Any other comment
with boxes, older or newer, does nothing. An answer runs a turn only while
GitHub says the issue is open: on an issue closed since the turn that asked,
a box and a reply do nothing.

Where the instance gives a cycle the images an issue links
(`TEHAS_WORK_IMAGES=1`), a planning session is given them too, by the same
rules: the issue's own at its start, and the images your replies link on the
turn that answers, which keep the numbering and the limits of what the session
already has. A link in a reply from an account that may not authorise work is
left out, and the bot says so. The bot says how many it was given and which it
left out. If one cannot be fetched, the turn does not run and the bot says so.
The pi-rukas that tehas installs does not show them to the planning agent yet,
so describe in words what a picture shows.

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
the bot approve its own work: every poll reads its own login first, before it
reads a label or changes one. The Protolab refuses the same list, with an error
before it reads, builds or removes a page. A poll or a pass that cannot read
its own login stops there and does nothing else.

## Pause and resume

Apply **`tehas:pause`** to the control issue, the one `TEHAS_CONTROL_ISSUE`
names (in the example, issue 1). No new cycle starts while it is there. A cycle
already running finishes. The same label also stops planning turns, answers
to planning questions and retries by a tick, because the poll ends before it
reaches them. Remove the label to resume.

## Limits that hold work back

- At most `TEHAS_TICKET_CONCURRENCY` cycles at a time. It is a ceiling, not
  a promise: the dispatcher starts fewer when the slice has no room.
- Room in `tehas.slice`, the systemd slice every sandbox container runs in.
  A cycle starts only while the slice's memory limit, less what every
  container in it is promised or uses, leaves one more `TEHAS_CYCLE_RESERVE`
  (2g by default). A container counts at its reserve or at what it uses,
  whichever is more, so a dev session, a planning turn and the indexer take
  room like a cycle does. A cycle that has started and has no container yet
  counts at one reserve. File cache and other memory the kernel can take back
  do not count.
- No new cycle while the whole slice stalls on memory: `full avg60` of its
  memory pressure above 10.
- No cycle and no planning turn while the slice is missing, has no limit, or
  Docker cannot place containers in it. `tehas status` then says `warn`. See
  [Troubleshooting](/troubleshooting).
- No cycle, no planning turn and no retry while there are no resources: the
  model endpoint does not answer, or does not offer the configured model. The
  dispatcher asks it at every poll. `tehas status` says `warn: No resources`
  and the dashboard says `no resources` until it is back. With
  `TEHAS_WORK_IMAGES=1` a model that does not read an image counts the same.
- One cycle at a time on a pi-rukas that is new on the host: one that no
  cycle here has ended as `merged`, or as a handoff with a pull request, on.
  While a cycle runs on it, `tehas status` says `holding: pi-rukas <commit> is
  new on this host`, and cycles still running on the pi-rukas before it count
  for the slots only. The first cycle that ends well on it lifts the hold. The
  commit is the sandbox image's, its label `tehas.rukas`, so an image put back
  for a [rollback](/host#rolling-back-pi-rukas) is known by its own commit;
  an image built before the label counts as the commit in
  `~/tehas/versions.env`. The commits proven are listed in
  `~/tehas/state/rukas-proven`. A new host, which has proven none, starts the
  same way.
- Inside a cycle, `TEHAS_CONCURRENCY` workstreams side by side: 4 by default,
  fewer if the model backends cannot take it. See [The host](/host).
- No new cycle while `TEHAS_MAX_AWAITING` issues carry `tehas:awaiting-merge`
  or are being worked on. The default is `TEHAS_TICKET_CONCURRENCY` plus 2, so
  every slot can be used while two pull requests wait. Unreviewed pull
  requests are the queue you control.
- A self-hosted model endpoint has few real slots per model, so a cycle takes
  a while; the six-pass review is the slowest part.

The review cap, the new pi-rukas, the room and the pressure hold back cycles
only. Planning
turns still run. `tehas status` shows the room in its `slice` line, as the
dispatcher's last poll read it, and `tehas dispatch --dry-run` reads it
afresh. A hold is written to `~/tehas/logs/dispatch.log` once, when its reason
changes, and `released: cycles may start again` once when it ends.

## The improvement loop

Optional. It is off unless the instance sets `TEHAS_IMPROVE_REPO` and the timer
`tehas-improve.timer` is enabled; see [Commands](/tools).

Every three hours a model reads the cycles that ended since the last run and
says what could be better: in the dispatcher, the cycle, pi-rukas, the
console, the gates or this manual. A finding that is not already in the issue
stack is filed as an issue of its own in that repository.

- **A finding about pi-rukas has a repository of its own** where the instance
  sets `TEHAS_IMPROVE_RUKAS_REPO`. The model gives each finding a category,
  and one of the category `pi-rukas` is filed there, because a cycle on the
  product cannot fix it. Both repositories are read for duplicates. Each
  has the limit of 10 open issues to itself, so one that is full does not
  hold the other back. How a fix then reaches the host is
  under [pi-rukas comes from upstream](/host#pi-rukas-comes-from-upstream).
- **Each issue names the product's issues** whose cycles it rests on, as
  `owner/name#7`, so GitHub links the two both ways.
- **Each issue says what working on it means**, in the line `Working on it`,
  because the text of an issue is the task a cycle is given. A finding about
  the factory is built in the repository it is filed in, once a person applies
  `tehas:ready`. A finding about pi-rukas is fixed in pi-rukas; no cycle on the
  product works around it. Where the
  instance has no repository for pi-rukas, such a finding is filed with the
  rest and tells a cycle to change no code and hand off.
- **An issue worked on again is checked against the filed issue.** When a
  later cycle of a product issue is read and an issue the loop filed rests on
  an earlier cycle of it, the model says whether the problem came back. That
  is a comment on the filed issue, open or closed: `Seen again: yes` or `no`,
  what the new cycle showed, its whole log, and the pi-rukas commit the host
  was installed with. This is how a fix in pi-rukas is verified: deploy it,
  apply `tehas:ready` to the product issue again, and read the comment. A
  model wrote it, so check it against the log. The loop closes and reopens
  nothing. At most 10 comments a run, and none in a run that waits because
  every repository is full.
- **The label is `tehas:improvement` and nothing else.** tehas does not act on
  it, and the loop never applies `tehas:ready`. A filed issue is a proposal a
  model wrote: read it, correct it, and apply `tehas:ready` yourself if it
  should be built. Close it if not.
- **At most 3 issues a run, and none in a repository where 10 are open** with
  the label, or the number `TEHAS_IMPROVE_MAX_OPEN` gives. The loop reads text
  anyone may have written into an issue or a log, so the limit is what keeps
  a steered or a mistaken model from filling the stack while nobody reads. Close some or build some and the loop files there again. A run
  waits, and reads nothing, only when every repository is full.
- **The whole log is linked.** Each issue links the logs of the cycles it
  names, gzipped, as assets of the release `tehas-improve-logs` in
  `TEHAS_IMPROVE_REPO`, wherever the issue is filed. In a public repository
  they are public. A log that holds
  something shaped like a token is not uploaded, and the issue says so.
- **One line per run** in `~/tehas/logs/improve.log`:

```
2026-10-11T09:00:12Z improve: cycles=4 findings=3 invalid=0 duplicates=1 of=#41 over=0 filed=2 issues=#57,#58 rechecked=1
```

`cycles` is how many were read and `findings` what the model gave. `invalid`
were dropped for not being the shape asked for, `duplicates` were already in
the stack or given twice (`of` names the issues the model pointed at), and `over` were more
than the limits allow and are dropped. `filed` and `issues` are what was
opened. An issue in the repository for pi-rukas is written with its
repository, as `acme/rukas#12`. `rechecked` is how many comments were posted
on issues that were worked on again; one GitHub refused follows as
`not-commented=acme/rukas#12`.

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
tehas top                                # the same console, with the queue and the cycles
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
