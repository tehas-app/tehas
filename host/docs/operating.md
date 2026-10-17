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
  `<script src="app.js">`,
- a zip, unpacked into a folder of its own; its files count against the same
  limits. It takes the types above and md files, kept in their folders, so
  `index.html` in a zip can refer to `data/sample.json` in it. Links, paths
  that leave the zip, paths more than six levels deep and locked files are
  left out. At most 200 entries of a zip are read, and once a file of it is
  over the 10 MB, nothing more of it is unpacked.

A prototype zip may carry instructions for language models on what it holds,
for example a `prototype.md`; the lab serves it like any other file and does
not define what it says.

The limits:

- **10 megabytes a ticket**, counted in the order the files were attached,
  and at most 40 files. What does not fit is left out, and the page lists it
  under "left out" with the reason.
- **Static files only.** Nothing is run on the host for a prototype.
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

## Attachments

What is attached to an issue reaches three parts of tehas, each by its own
rule. A cycle works from words first: anything it must know is safest in the
issue text.

| What                                        | A cycle | A planning session | The Protolab |
| ------------------------------------------- | ------- | ------------------ | ------------ |
| The issue's title and body                  | Read | Read, at the start | The title |
| Comments on the issue                       | Not given | Replies by an allowed account to its question, as answers | Not shown |
| A png, jpg, webp or gif linked in the body  | Fetched and mounted with `TEHAS_WORK_IMAGES=1`; shown to explore and plan when the sandbox image says its pi-rukas reads them | The same | Shown |
| The same image linked in a comment          | With `TEHAS_WORK_COMMENTS=1`, when an allowed account wrote it before the label or the retry tick and nobody edited it since | When a reply to its question links it | Shown |
| md, txt, csv, json, yaml, log, html, css or js linked in the body | Fetched and mounted with `TEHAS_WORK_FILES=1`; pi-rukas does not read them yet | The same | html, css, js, json and txt shown, html in a frame |
| svg, avif, a video                          | Not given | Not given | Shown |
| A zip                                       | Not given | Not given | Unpacked, its files shown as above |
| Any other file, such as a pdf               | Not given | Not given | Left out |

The rules behind each column:

- **A cycle** is given the issue's title and body. With
  `TEHAS_WORK_IMAGES=1` [tehas work](/tools#tehas-work) also fetches the
  images the body links, at most 8 and 10 MB, and mounts them read-only in
  the cycle's container with a manifest. They are taken when an allowed
  account labelled the issue, whoever opened it, or when an owner, member or
  collaborator opened it. The dispatcher records at the start what the label
  approved, and `tehas work` fetches only those links. Whether the explore
  and plan steps are shown the images is what the sandbox image's own label
  `tehas.rukas.images` answers. Upstream pi-rukas merged the reader on
  2026-10-14, and the host follows its main, so an image built since then
  says yes. One that says no holds every issue whose text links an
  attachment while the setting stands; the other issues start. The steps
  after plan read the plan, not the pictures. With `TEHAS_WORK_DESCRIBE=1`
  the model also describes each image in words, into `images.md` beside
  them; with `TEHAS_WORK_FILES=1` the text files come too. pi-rukas reads
  neither yet. `TEHAS_WORK_COMMENTS=1` adds the attachments of comments, by
  the rule in the table.
- **A planning session** is given the same images and files by the same
  rules, once at its start, and is shown the images when the same label says
  so. A reply to its question can carry more. See
  [Planning on an issue](#planning-on-an-issue).
- **The Protolab** shows what is attached to an issue labelled
  `tehas:prototype` by an allowed account, in the body or in a comment, from
  an owner, member or collaborator, as said under "Prototypes" above.

So for a picture, write the words too: name the element by its selector and
its visible title, and say where in it the thing goes.

## 1. Write an issue tehas can build

The cycle reads the issue's title and body; see [Attachments](#attachments)
for the rest. Give it:

- one outcome, small enough for one pull request,
- a `## Acceptance criteria` section with checkable items,
- the files or area involved, if you know them,
- the test file and the manual page the change will touch.

Name the tests and the pages even when they seem obvious: the plan fences the
work to what the issue names, and a cycle that changes a file outside the
fence parks with `verify-failed:develop` (see
[Troubleshooting](/troubleshooting)).

Vague issues park at the first step with "underspecified". Text in the issue is
treated as untrusted input: it cannot change the gates, and the cycle halts if
its patch touches `.pi/`, `.github/`, `CODEOWNERS` or `agents.json`. An edit to
`AGENTS.md` or `CLAUDE.md` is not halted: it ships, and you see it in the diff.
What the cycle is judged against is the doctrine at the commit it started from,
so an edit of its own changes nothing for that cycle.

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

The dispatcher takes issues `tehas:urgent` first, then `tehas:next`, then by the lowest issue number: as many at a time as the host's ceiling and the slice's room allow (see [Limits that hold work back](#limits-that-hold-work-back)). A running cycle is never stopped to make room for an urgent one. The dashboard lists the queue by issue number, not the order the dispatcher will work it in: whether a `tehas:urgent` or `tehas:next` moves the queue is decided by who applied the label, not by its being there. `tehas:urgent` counts from an allowed labeller. `tehas:next` counts from an allowed labeller or [the fixer](#the-fixers-grant), when the latest event of the label applies it; one taken off since, or applied by anyone else, is ignored and logged.

An issue with `tehas:blocked` is not started while an issue it names is open, so its slot goes to work that can move. The label counts on the same terms as `tehas:next`. The blockers are named in a comment by an allowed labeller or the fixer that holds the marker:

```
Blocked by #12 and #34.

<!-- tehas:blocked by=12,34 -->
```

Up to 20 numbers, separated by commas and no spaces. The newest such comment among the last 100 counts; one by anyone else is ignored. The issue also waits, and the log says why, when the label counts and no such comment is there, when the newest one does not hold a marker that reads, when it was last edited by another account, or when GitHub does not give the state of a number it names. Once every blocker is closed the issue may start. The dispatcher leaves the label on; take it off when it no longer applies. To start a blocked issue anyway, take the label off.

| Label on the issue      | Meaning                           | Your move                                |
| ----------------------- | --------------------------------- | ---------------------------------------- |
| `tehas:ready`           | Queued                            | Wait, or remove the label to withdraw it |
| `tehas:running`         | A cycle owns it                   | Leave the issue and its branch alone     |
| `tehas:awaiting-merge`  | Pull request open, only the merge left | Review and merge                    |
| `tehas:shipped`         | A merged pull request closed it   | Nothing; the bot gives it                |
| `needs-human-attention` | The cycle stopped                 | Read the comment; see below              |
| `tehas:plan`            | A planning session is asked for   | Wait for the first comment               |
| `tehas:planning`        | A planning turn is running        | Wait                                     |
| `tehas:question`        | The factory asked something       | Tick a box; in a planning session you may reply instead |
| `tehas:improvement`     | Filed by the improvement loop     | Read it; apply `tehas:ready` if it should be built |
| `tehas:fixer-escalated` | The fixer left it to a person     | Read the fixer's note; decide, or take the label off to hand it back |
| `tehas:next`            | Started after urgent issues, before the rest | Take it off to give the issue its place by number back |
| `tehas:blocked`         | Not started while a blocker it names is open | Close the blocker, or take the label off to start it anyway |

`tehas:ready` from an allowed labeller starts a fresh cycle whatever else the
issue carries. So does one from [the fixer](#the-fixers-grant), within its grant. tehas's labels of the earlier outcome go when the cycle starts.
`needs-human-attention` stays until the cycle runs: the cycle starts with
`--restart`, and pi-rukas takes the label off as it begins. The dashboard
counts an issue with `tehas:running` as running, whatever else it carries. On
an issue whose cycle is still running, the label waits: a fresh cycle starts
when that one has ended. Remove the label before then to withdraw it.

When the cycle starts and when it ends, the bot comments on the issue. The token
usage per agent role is posted on the pull request.

The console on the [factory floor](/) and on the [plain dashboard](/dashboard)
shows one cycle at a time. When several are running, or one is running and an
earlier one can still be read, a row of tabs above it names each by its issue
number.

A timer polls every two minutes, and a cycle that ends starts a poll at once. To see the factory's state, or to see or force
a poll by hand on the host:

```
tehas status                 # taking work? the queue, the current or last cycle, the slice's room
tehas status --json          # the same for a program, with the demand (see the factory API)
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

Each finding of the six-pass review has a severity: CRITICAL, HIGH, MEDIUM or
LOW. Which severity stops a cycle is the product's decision, not the reviewer's
and not pi-rukas's. pi-rukas reads it from the product repository's doctrine
(`AGENTS.md`, or `CLAUDE.md` when the product has one), at the commit the cycle
started from, so a cycle cannot lower its own bar mid-run. For this repository
the bar is MEDIUM and above, decided in the platform repository's
`docs/adr/0002-review-severity-policy.md`, a record in the source rather than a
file on a host. A finding at the bar or above sends the change back to a fix
step and the review runs again, up to three rounds. Only LOW findings may be
deferred: they are notes on the pull request and start no fix round. A CRITICAL
finding blocks whatever the bar says.

`AGENTS.md` of this repository does not state the bar yet, because rule 7 gives
that file to a person, so a cycle's log still records the verdict as `unstated`
and takes pi-rukas's default, which is the same MEDIUM. Once a person adds the
sentence the log names the bar it quoted and no cycle behaves differently. The
sentence has to be one pi-rukas can read: one sentence that names the bar with a
relation, `This repository blocks at MEDIUM severity and above.` pi-rukas takes
the severity out of the sentence the judge quotes and checks that sentence
against the file word for word, so a paraphrase that names the severities
without the relation is a silent no-op and the cycle takes the default again.

An instance can set the bar itself with `PI_ENSEMBLE_REVIEW_THRESHOLD`, which
wins over what the product says (see [Commands](/tools#instance-settings)).
tehas passes that line through without checking it, and pi-rukas honours it only
when it reads as one of the four severities: a value that is not one of them is
ignored, not refused with 78. With neither, MEDIUM.

Three rounds is the most a review takes. A CRITICAL finding parks the cycle. A
round cap reached with only MEDIUM and HIGH findings left, an approving
adversarial review, and those findings posted as a comment on the pull request,
carries on to the gate with the findings named on the pull request instead of
parking; a post that fails parks. A review that runs out of its time budget
parks whatever the findings are.

After merging, the issue closes and leaves the queue. If you close a pull
request without merging, remove `tehas:awaiting-merge` from the issue by hand;
Tehas does not notice a rejected pull request yet.

## 5. When a cycle stops

`needs-human-attention` means the cycle parked. The bot posts one comment: the
status, the last step, the cap pi-rukas stopped on, the log file on the host,
and then pi-rukas's own handoff, with the reason and what to do next. pi-rukas
posts no handoff of its own: tehas runs it with
`PI_ENSEMBLE_HANDOFF_CHANNEL=host` and posts the one it renders. The comment's
first line is pi-rukas's hidden marker, `<!-- pi-rukas:handoff issue=N cap=... -->`.

What a handoff with an open pull request leaves on the issue depends on why
pi-rukas handed off:

- Only the merge is left (cap `awaiting-human-merge`): `tehas:awaiting-merge`,
  a short comment on the issue, and pi-rukas's handoff with the token usage on
  the pull request.
- Any other cap, no cap, a review that ran out of its rounds with findings
  still open, or an issue that carries `needs-human-attention` at the end:
  `needs-human-attention` only. The comment names the cap and links the pull
  request. Read the pull request before you retry.

An issue never ends a cycle with both labels.

A cycle that reaches its deadline parks with the cap `deadline`. tehas gives
pi-rukas a deadline a quarter of `TEHAS_WORK_TIMEOUT` before the end (at least
one run of the gate and 8 minutes): after it, the cycle starts no new step and
hands off with its branch and pull request, instead of being stopped in the
middle of one.

To try again after fixing the cause (often: make the issue clearer), tick
`Retry from a clean start` in the bot's comment. Tick `Leave it parked` to
take the question away and keep the issue as it is. With
`TEHAS_WORK_COMMENTS=1`, what an allowed account wrote on the cycle's pull
request between the park and the tick goes with the retry: its text as
`retry-context.md` beside the images, and its images and files by the usual
rules. A label works as well: apply `tehas:ready`. `needs-human-attention` does not
have to be removed first. A cycle started
that way takes `tehas:question` off the issue, and the old boxes no longer do
anything.

An issue closed since its cycle stopped is not work. Neither the tick nor a new
`tehas:ready` starts anything on it, and the poll's log says so.

A cycle that a provider outage parked says so in its comment: `parked by a
provider outage`, with the cap. Nothing needs doing: once the model endpoint
answers, the dispatcher starts the issue again on its own and says so on the
issue. It does that once. If that cycle is parked by an outage too, the issue
waits for a tick or a label like any other.

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
`tehas:running` nor `tehas:awaiting-merge`. It is used once. The poll first
marks the issue `tehas:running`; if GitHub refuses that, the tick stays and
the next poll tries again. Once the issue is marked the question is no longer
on record, so the same tick cannot start a second cycle, and the bot unticks
the box to show it was taken. A box ticked by anyone
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

The next cycle starts in a fresh workspace, with `--restart` when the issue
carries `needs-human-attention`. That flag is the answer the label asks for:
pi-rukas lets the cycle past its check of the label and takes the label off.
tehas removes it nowhere. Branches and
commits the earlier attempt pushed are kept on GitHub. What it left only on the
host, such as commits on worktrees that were never pushed, is moved to
`~/tehas/work/tehas-<issue>.before-retry` and stays there until a cycle of that
issue ends with its own merge, or hands off waiting only for a human to merge.
A merge or a close someone else makes while the cycle runs keeps both the
workspace and that copy, for the poll's 7-day removal. Only the latest stopped
attempt is kept: a second retry replaces it, unless the attempt before cloned
no workspace, which leaves the older copy beside the new one. Recover what you
need from a stopped cycle before you retry twice. Nothing is kept for ever: the
dispatcher's poll removes an issue's kept workspaces once no cycle has run on
it for 7 days, three issues a poll at most, and logs each removal.

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
   asked something, and a reply or the last box works either way. When the
   agent asks more than one question in a turn, the comment lists them all,
   numbered, with their choices as text and no box for them: answer them in
   a reply.
4. Answer by ticking one box, or by replying in a comment when none of the
   boxes fits. If you do both, the reply counts, and the agent is told
   which box was ticked as well. The next poll that is free
   continues the session with your answer, and the issue carries
   `tehas:planning` while the turn runs. The turn runs as its own unit,
   `tehas-plan-<issue>.service`, and the polls go on meanwhile: cycles start
   as usual, and the turn's end starts the poll that posts its outcome. One
   planning turn runs at a time, so while another runs, your answer waits
   until it ends, which is up to two hours (`TEHAS_PLAN_TIMEOUT`). A
   running work cycle does not hold an answer back. A poll also handles one
   answer or one planning start and then stops, and it does nothing while the
   model endpoint does not list the model.
5. The agent shows you a plan before it files it, and files it when you
   confirm. Every turn of a session works on the commit the session started
   on. Whether what is filed is what you saw depends on pi-rukas. The release
   the host follows keeps no record of the plans it showed (see [the queue
   and dispatch](/features#queue-and-dispatch)), so read
   the issues it files. A later release keeps one, and a confirmed plan is
   then filed as it was shown when it is found there: with no time limit
   from a release that reads `PI_ENSEMBLE_PLAN_CACHE_TTL_MS`, within two
   hours from one that does not. A plan that is not found is drafted again.
   The agent files the issues it plans with the bot's token. To finish, tick
   `End this planning session`. Filed issues are never started on their own:
   apply `tehas:ready` to the ones you want built. The agent is told to add
   and remove no `tehas:*` label and no `needs-human-attention` on any issue.
   A `tehas:ready` it applies anyway is taken off, as one from any account
   outside the list is.

The conversation of a session is kept by Pi on the host, in
`~/.pi/agent/sessions`. If it is gone, for example after the host was rebuilt,
the session cannot go on: the bot ends it, says so, and your answer is not
sent. Apply `tehas:plan` again to start a new session.

Only answers from allowed labellers count, and the fixer's within its grant
(see [The fixer's grant](#the-fixers-grant)). A box ticked by anyone else, or
more than one box, is unticked and the bot says so. A reply counts only if its
author is an allowed account and, when it was edited, its last editor is too.
Replies from others are ignored. Only the question the session is waiting on
counts, which is the one whose id the host has on record. Any other comment
with boxes, older or newer, does nothing. An answer runs a turn only while
GitHub says the issue is open: on an issue closed since the turn that asked,
a box and a reply do nothing.

Where the instance gives a cycle the images an issue links
(`TEHAS_WORK_IMAGES=1`), a planning session is given them too, by the same
rules, once at its start, and with `TEHAS_WORK_FILES=1` its text files. The
bot says what it fetched, which it left out, and whether the session was shown
them. If one cannot be fetched, no session starts and the bot says so. A reply
can carry a picture or a file too: the next turn adds what the reply links to
the session's images, by the same rules and limits, and tells the agent their
names. What it leaves out the bot says. If one cannot be fetched, the turn
does not run, and the bot says so; reply again to run it. The planning agent
is shown the images when the sandbox image's label says its pi-rukas reads
them. See [Attachments](#attachments).

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

Two switches stop the factory. Either one is enough.

**`tehas pause [reason]`** sets the switch on this host: a flag,
`~/tehas/state/pause.json`, holding who set it, when and why, in a file only the
account that ran the command can read. It asks GitHub nothing, so a host can be
paused when GitHub cannot be reached. `tehas pause` again succeeds and keeps the
first flag. **`tehas resume`** removes it, and says so when the factory stays
paused for the other switch.

**`tehas:pause`** on the control issue, the one `TEHAS_CONTROL_ISSUE` names (in
the example, issue 1), is the switch that works from away from the host. It works
one way. Anyone who may label the repository may apply it, because stopping is
the safe direction. Only an account in `TEHAS_ALLOWED_LABELERS` lifts it: when
the label is gone and the `tehas:pause` event with the latest stamp on the
control issue is a removal by anyone else, names nobody, or is an application no
event took back, the poll starts nothing and says what it found. Deleting or
renaming the label itself is the last of those: no account was asked to lift the
pause, so it holds. To resume from away, an allowed account applies
`tehas:pause` and takes it off again. A poll that cannot read the control issue's
label events starts nothing.

While either switch is set the poll starts nothing: no cycle, no planning turn,
no answer to a planning question, no retry by a tick. A cycle already running
finishes, and the poll still gives up work that is finished: it stops a cycle
whose issue is closed or whose pull request is merged or closed, stops one nobody
has heard from, and clears an issue marked `tehas:running` with no cycle behind
it. A paused factory therefore keeps no slot and no model time for work it has
already done. `tehas status` prints a line for each switch that is set, with when the
flag was written and why; the dashboard and the front page say the factory is
paused and, while a cycle is still running, that it is finishing that issue.

The reason is written to the flag and shown on pages that may be public, so write
nothing in it that should not be read in public. The account that set the flag is
not published.

## All hands

**`tehas:all-hands`** on the control issue asks for all hands on this project.
It counts while it is on the issue and the account that applied it last is in
`TEHAS_ALLOWED_LABELERS`. A label anyone else applies is ignored and said once
in the dispatcher's log. On its own it changes nothing in this factory: no
cycle starts or waits because of it. `tehas status` prints an `all-hands` line,
its JSON says `"allHands": true` and the dashboard shows "all hands". What it
means for other factories is up to the program that runs them, such as
tehas-industry. Take the label off to end it.

## Limit and drain

A limit caps what this factory starts, until it expires or is cleared. It
keeps a host quiet for a weekend, or leaves the models to another project:

```
tehas limit set --cycles 1 --plan off --for 7200 --reason "the models are busy"
tehas limit                  # what is set
tehas limit clear            # back to no limit
```

`--cycles` is the most cycles at once; `--plan off` also holds new planning
sessions and improvement runs, though an answer to a planning question still
runs. Without `--for` or `--until` the limit stays until cleared. Running
cycles are never stopped. A limit that has expired, or a file that cannot be
read, holds every start until it is set again or cleared, and `tehas status`
says so. Pause is a separate switch: a paused factory stays paused under any
limit.

Before a reboot or a disk change:

```
tehas drain --wait 3600      # start nothing new, wait up to an hour for the cycles
tehas limit clear            # afterwards
```

A program that drives factories uses the same commands: see
[the factory API](/api).

## Limits that hold work back

- A limit set with `tehas limit`, when there is one: see
  [Limit and drain](#limit-and-drain).
- At most `TEHAS_TICKET_CONCURRENCY` cycles at a time. It is a ceiling, not
  a promise: the dispatcher starts fewer when the slice has no room.
- Room in `tehas.slice`, the systemd slice every sandbox container runs in.
  A cycle starts only while the slice's memory limit, less what every
  container in it is promised or uses, leaves one more `TEHAS_CYCLE_RESERVE`
  (2g by default). A container counts at its reserve or at what it uses,
  whichever is more, so a dev session, a planning turn and the indexer take
  room like a cycle does. A cycle that has started and has no container yet
  counts at one reserve, also when the slice is empty and systemd has
  removed its directory. File cache and other memory the kernel can take back
  do not count.
- No more containers in the slice than their ceilings fit in its limit: a
  container may use a quarter of the slice, so four. Every container counts,
  a dev session, a planning turn and the indexer as well, and a cycle that
  has no container yet. Room by use alone would let in cycles that are small
  in their first steps and may each grow to a quarter later; then the whole
  slice reclaims and the kernel kills in the biggest container. So on any
  host at most four cycles run at once, whatever `TEHAS_TICKET_CONCURRENCY`
  says; a host with 4 slots is unchanged, except that a dev session or a
  planning turn takes the fourth. The ceiling is not sized by
  `TEHAS_TICKET_CONCURRENCY` instead: containers that are not cycles would
  still oversubscribe the slice, and on a host with many CPUs the ceiling
  would fall below the reserve.
- No new cycle while the whole slice stalls on memory: `full avg60` of its
  memory pressure above 10 while more than half of the slice is promised.
  Below that the pressure is one container at its own ceiling, which a new
  container does not make worse, so it holds nothing.
- No cycle and no planning turn while the slice is missing, has no limit, or
  Docker cannot place containers in it. `tehas status` then says `warn`. See
  [Troubleshooting](/troubleshooting).
- No cycle, no planning turn and no retry while there are no resources: the
  model endpoint does not answer, or does not offer the configured model, and
  nor does its backup where there is one. The
  dispatcher asks it at every poll. `tehas status` says `warn: No resources`
  and the dashboard says `no resources` until it is back.
- With `TEHAS_WORK_IMAGES=1`, no cycle for an issue whose text links an
  attachment while the model does not read images. The dispatcher asks the
  model with a one-pixel image only when such an issue could start now, at
  most once a poll. The issue stays queued, the log says `hold #N: ...`, and
  issues without images start as usual. A planning turn is not held by it.
- No cycle while the code index is rebuilt. On a host where no cycle runs and
  the branch has moved, the poll that would start cycles starts the index run
  as `tehas-index.service` instead, and the log says `holding: the code index
  is refreshed first`. It takes seconds to minutes. Its end starts the poll
  that starts the cycles. Planning turns are not held by it.
- A slot is never kept by work that is done. Each poll stops a running
  cycle whose issue is closed, or whose pull request, opened by the bot since
  the cycle began, is merged or closed. It is stopped as stopping its unit
  does: the container goes, the lock is released and `tehas:running` comes
  off. The log says `stop #N: <reason>`. A merged one is reported as merged,
  one whose issue was closed only loses `tehas:running`, and one whose pull
  request was closed is parked without the retry boxes. What the cycle had not
  pushed is nowhere else, so a cycle stopped this way keeps its workspace, and
  its comment names the workspace it left (`~/tehas/work/tehas-<issue>`) when
  there is one, and says the cycle left none when there is not. tehas removes a
  workspace only when the cycle ends with its change merged, or hands off with
  its pull request open for a human to merge. A poll that cannot read the
  issue or the pull requests ends there: it stops nothing, starts no cycle and
  sweeps no kept workspace, so a cycle is never given up on a guess and holds
  its slot until a later poll can ask. The slot is filled on the same poll.
  And a cycle that ends starts a poll at once
  (`tehas-dispatch.service`), so its slot does not wait for the timer. If a
  poll is running then, systemd takes that start as the running poll, which
  may have counted the cycle as still running. So the cycle also leaves
  `~/tehas/state/dispatch.again`, and `tehas-dispatch.path` starts the next
  poll as soon as that one ends. A poll removes the file as it begins.
- Nothing else. A new pi-rukas commit runs at full concurrency from its first
  cycle, so a bad one can fail up to `TEHAS_TICKET_CONCURRENCY` cycles at
  once. Those issues end labelled `needs-human-attention`, as any failed cycle
  does, and the time limits bound the cost.
- When a poll starts fewer cycles than there are free slots and issues
  waiting, the log says why on every such poll, and `tehas status` shows the
  same line: `holding: <n> of <free> free slots start: ...` when the
  awaiting-merge limit or the slice's room cut it, else the hold that stopped
  all of them.
- Inside a cycle, `TEHAS_CONCURRENCY` workstreams side by side: 1 by default,
  and more only with one cycle at a time. See [The host](/host).
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
- **A finding at the boundary is two issues.** The category `boundary` is
  for a problem with a half in the factory and a half in pi-rukas, or a
  shape the two have to agree on. Each half is filed in its own repository,
  the two link each other, and both carry `cross-project` as well. Without
  a repository for pi-rukas it is one issue that names both halves.
- **The model reads what each cycle ran with**: the `PI_ENSEMBLE_*`
  variables its container was given, the `TEHAS_*` settings that shape a
  cycle, how many cycles were running as its container started, and the
  provider retry line of its preflight log. `tehas work` leaves the first
  three in `~/tehas/state/cycles/<issue>.settings`. What the model reads may
  reach a public issue, so only short plain values are written: numbers,
  booleans and words of letters, digits and `_.,:=-`. Any other value is
  `[withheld]`: every URL, host, path and address, anything that may be a
  token, and every value whose name says it may be a secret or a host.
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
  what the new cycle showed, its whole log, and the pi-rukas commit the cycle
  ran: the `buildCommit` of its runner's result, or for a record written
  before tehas kept it, the sandbox image's label. Of a closed issue in the
  repository for pi-rukas, the loop asks GitHub whether the commit that closed
  it is in that pi-rukas commit, the question `git merge-base --is-ancestor` answers. When it is
  not, the comment says `not in this build` instead of `yes`. When the
  record names no commit, or GitHub does not answer, it says `build unknown`.
  The record names no commit of the factory, so an issue there that a
  commit closed always says `build unknown`. This is how a fix in pi-rukas is verified: deploy it,
  apply `tehas:ready` to the product issue again, and read the comment. A
  model wrote it, so check it against the log. The loop closes and reopens
  nothing. At most 10 comments a run, and none in a run that waits because
  every repository is full.
- **The label is `tehas:improvement`**, with `cross-project` beside it on the
  two halves of a boundary finding. tehas does not act on
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
`not-commented=acme/rukas#12`. A run reads at most five cycles, the oldest
first; `left=2` at the end says two wait for the next run. `stopped=124
returned=2` says the analyst hit its bound of 60 minutes: what it had written
was filed, and the two cycles it named nowhere wait for a later run.

To turn the loop off, remove `TEHAS_IMPROVE_REPO` from `instance.env`: cycles
stop leaving records and a run does nothing. Or stop the timer with
`systemctl --user disable --now tehas-improve.timer`.

## The fixer's grant

Optional. The fixer is an account that decides on parked issues from outside
the factory: start one again, start it again working differently, or leave it
to a person. It may also answer some planning questions. What runs as that account is not part of tehas. The factory says
what the account may do, and this section is that.

- **It is an account of its own**, named by `TEHAS_FIXER_LOGIN`. It is never
  the bot, because every agent posts as the bot: a poll stops with 78 when
  the two are one. It is never one of `TEHAS_ALLOWED_LABELERS`: loading the
  settings stops with 78. Unset, there is no fixer, and the account's labels
  count as any outsider's.
- **Its `tehas:ready` starts a cycle only within its grant**: a person
  approved an earlier cycle of the issue, the body is still the body that
  person approved, nobody outside the list changed the title or text since,
  and the fixer has started fewer than `TEHAS_FIXER_MAX_RETRIES` cycles on it
  since that approval (2 unless the instance says otherwise). Otherwise the
  label comes off and the issue says why. So the fixer never starts work
  nobody asked for: it never readies a `tehas:improvement` proposal.
- **The approval record says so.** A fixer start has `how: "fixer"`,
  `rests_on`, the person's approval it rests on, and `fixer_starts`. A
  person's start counts again from none. Its other limits are the
  dispatcher's as for any start: the slots, the cap of waiting merges and
  the slice's room.
- **Its comment reaches the cycle.** The fixer's newest comment on the issue
  that starts with `<!-- tehas:fixer issue=<n> action=retry -->` or
  `action=change`, written in the ten minutes before its label and edited by
  nobody after, goes into the record as `retry_context`. Where the instance
  gives a cycle images or files, `tehas work` writes it to
  `retry-context.md` beside them, quoted, under a head that says a model
  wrote it. No mount and no variable more: elsewhere it reaches no cycle.
- **It may answer a planning question**, by a tick or a reply, only when
  no person has answered the same question: a person's tick or reply wins,
  whichever came first. And only in a session an allowed account's
  `tehas:plan` started, fewer than `TEHAS_FIXER_MAX_PLAN_ANSWERS` times in
  the session (3 unless the instance says otherwise), and never for two
  turns in a row, so a person sees every second question at most. It may
  not tick `End this planning session`. Its reply starts with the line
  `<!-- tehas:fixer issue=<n> action=answer -->`, and the agent is told
  the answer is the fixer's. Any other answer of the fixer's is an
  outsider's: a tick is unticked and the bot says why, a reply is ignored
  and the bot says why once. GitHub says who last saved a comment, not who
  ticked a box, so a tick the fixer saved after another account edited the
  question is held: it is neither used nor unticked, and the bot asks a
  person, once, to tick again or reply. To overrule it, reply or tick before the next
  poll, or reply to the next question. The planning record,
  `~/tehas/state/plan-<issue>.json`, keeps who started the session (`by`)
  and who answered each turn (`answers`). A session started before tehas
  kept these takes no answer from the fixer.
- **`tehas:fixer-escalated`** is the fixer's word that a person decides.
  The factory does nothing on it; a start takes it off.
- **It may order the queue** with `tehas:next` and `tehas:blocked`, and
  name the blockers in a marked comment, as an allowed labeller may (see
  [What happens next](#3-what-happens-next)). Neither starts anything: an
  issue still needs a `tehas:ready` that counts. When to apply them, and how
  many, is the fixer's to decide, not the factory's.
- **Each cycle's end leaves a line** in
  `~/tehas/state/cycles/<issue>.history.jsonl`, the last 50: when, its
  status, last step, cap, park reason, failure, whether a provider outage
  stopped it, whether it parked, its pull request and who started it.
  Nothing in tehas reads it yet; it is the record a later `tehas history`
  lists. A fixer reads GitHub, not this file.
- **The park comment carries the park's facts.** The bot's park comment,
  with boxes or without, has the line
  `<!-- tehas:park issue=<n> step=<step> kind=<failure kind> provider=<0|1> sig=<sha256> -->`.
  `step` and `kind` are the history line's, cut to `[a-z0-9-]`, and empty
  when the cycle had none. `provider` is 1 when a provider outage stopped
  it. `sig` is the sha256 of the history line's
  `step|cap|park_reason|failure_kind|failure_cause`: two parks with the same
  `sig` stopped the same way. The free text of a park stays out of the
  marker. Only the bot's comments count: a reader ignores the line in
  anyone else's.

## Run one issue by hand

Skips the queue, the labels and the comments:

```
tehas work 123            # exit 0: merged or parked, 1: aborted, 2: died or timed out
tehas work 123 --restart
```

It does not take the dispatcher's lock, so a poll can start a queue cycle while
yours runs. Apply `tehas:pause` first, or check that nothing is ready. It does
take the issue's own lock: for an issue whose cycle is running it refuses with
exit 5 and touches nothing, and while yours runs the dispatcher starts no cycle
for that issue.

## Watching a running cycle

A cycle cannot be attached to or made interactive. It is pi-rukas run headless:
its one-shot runner, `pi-rukas-headless work <issue>`, with no terminal and no
Pi session of its own: it starts a Pi process for each agent. There is no prompt to type at and nothing waits for an
answer.

What you can do is watch it, on the [factory floor](/) or on the host:

```
tail -f ~/tehas/live/console-123.txt     # the console the dashboard shows
tehas top                                # the same console, with the queue and the cycles
tail -f ~/tehas/logs/work-123-*.jsonl    # the runner's NDJSON progress
docker exec -it tehas-work-123 bash      # a shell in the cycle's container
```

The shell is beside the session, not in it, and it is in the workspace the
agents are working in: what you change there changes the cycle. Do not use
`docker attach`. It shows the runner's output, not a Pi interface, and an
interrupt typed there reaches the runner and ends the cycle as interrupted.

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
