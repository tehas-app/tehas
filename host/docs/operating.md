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
- no line of it says a file goes there. A body left with `SCREENSHOT GOES
  HERE`, or `IMAGE_GOES_HERE`, where the picture should be, is a handover that
  stopped short: the dispatcher holds such an issue and says so in one line of
  `logs/dispatch.log` rather than spend a cycle on something no container was
  given. Attach the file, or write out what it shows, and then label.
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
| A zip                                       | Unpacked with `TEHAS_WORK_ZIPS=1`, its files given as above | The same | Unpacked, its files shown as above |
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
- **A zip** reaches a cycle with `TEHAS_WORK_ZIPS=1`, and needs
  `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`: what comes out of an archive
  goes into the directory of one of those. It is taken by its first bytes, not
  by a name ending in `.zip`: a link named `.zip` that holds something else is
  left out and said. At most 2 archives per issue, each downloaded at most
  4 MB. Every file inside one then goes through the same checks as a file the
  issue links, an image by its first bytes and a text file by its extension
  and its encoding, and counts against the same limits. An svg, a nested
  archive or any other binary is left out with its path in the archive and the
  reason. The files are named on from the linked ones, and the archive is not
  kept. See [tehas work](/tools#tehas-work).
- **A planning session** is given the same images and files by the same
  rules, once at its start, and is shown the images when the same label says
  so. A reply to its question can carry more. See
  [Planning on an issue](#planning-on-an-issue).
- **The Protolab** shows what is attached to an issue labelled
  `tehas:prototype` by an allowed account, in the body or in a comment, from
  an owner, member or collaborator, as said under "Prototypes" above.

So for a picture, write the words too: name the element by its selector and
its visible title, and say where in it the thing goes.

And when the picture is not there yet, do not leave its place marked. A body
that says `SCREENSHOT GOES HERE`, or `IMAGE_GOES_HERE`, while its text links no
attachment at all is held by the dispatcher: the same line of
`logs/dispatch.log` says the words it found. Nothing on the issue changes.

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

Vague issues park at the first step with "underspecified", and the factory
then refines them once on its own (see [Refined on its own](#refined-on-its-own)). Text in the issue is
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

The control issue, the one `TEHAS_CONTROL_ISSUE` names and the one that carries
`tehas:pause`, is never worked on. It holds the pause switch and no outcome, so
a cycle on it would spend a slot and model time on the switch itself. Whoever
applies `tehas:ready` or `tehas:plan` to it, and whoever ticks a retry box on
its question, starts nothing: no cycle, no planning turn, no retry. The poll
changes no label of it, and says so once, in one line of `logs/dispatch.log`
as it says a label from an account that is not allowed: `skip #N: the control
issue is never worked on`. The dashboard and `tehas status` do not count it in
line.

## 3. What happens next

Watch it on the [factory floor](/) or the [plain dashboard](/dashboard): the queue, the issue being worked on with its
console streaming live, and what is waiting for you.

Unless `TEHAS_ANALYTICS=off`, the plain dashboard also has a section **Last 10
days**, from `live/analytics.json` (see [tehasbot status](/tools#tehasbot-status-and-the-console)):
a row of figures (shipped, with a bar a day; planned, with its trend; runs by
how they ended; first-pass yield; lead time; cycle time; machine time; tokens;
availability; and energy per shipped issue where the mesh reports it), a chart
of shipped issues and failed runs per day with the cycle-minutes as a line, the
five most common stop reasons, the share of runs that got through each step,
and one row per tehas and pi-rukas version pair. Every figure says its count as
`n=`: ten days make small numbers. A pair with fewer than three runs shows its
count only. The section is hidden where there is no file, and the page reads it
at most once a minute. What the file means is worked out by one module,
`host/fun/analytics-data.js`.

Optional, and only when the model backend is a [viiwork](https://github.com/janit/viiwork) mesh with
`tehas-energy.service` enabled: the factory floor also has a section
**Energy use**: what the machines serving the factory draw now, their kWh over
24 hours and 30 days, the last 30 minutes as a chart, and the factory's models
with their busy slots. The figures are the mesh's own and are good to about
15%. Only machines that are online and serve a model the factory uses are
counted; other machines of the mesh and other models are left out. A counted
machine's draw is all of it, including any other model it serves.
While the section is there, the panel that opens the first screen carries a
live meter: the draw now, and the tokens the mesh produced for the factory's
models in the last minute. The tokens are the mesh's count for those models, so
other users of the same models are in it. On a wide screen, 1440px and up, it
is a card of its own beside that panel, over the column the work order monitor
stands in; below 1440px it stands at the right end of the panel, over its title
and the lines under it, and below 900px it lies across the foot of that panel
instead. It leads to Energy use.
With any other backend the section and the meter are simply not there, and
nothing else changes. To turn them off on a viiwork mesh, see
[the energy sampler](/tools).

The dispatcher takes issues `tehas:urgent` first, then [refreshes](#refreshes) of its own pull requests, then `tehas:next`, then by the lowest issue number: as many at a time as the host's ceiling and the slice's room allow (see [Limits that hold work back](#limits-that-hold-work-back)). A running cycle is never stopped to make room for an urgent one. The dashboard lists the queue by issue number, not the order the dispatcher will work it in: whether a `tehas:urgent` or `tehas:next` moves the queue is decided by who applied the label, not by its being there. `tehas:urgent` counts from an allowed labeller. `tehas:next` counts from an allowed labeller or [the millwright](#the-millwrights-grant), when the latest event of the label applies it; one taken off since, or applied by anyone else, is ignored and logged.

An issue the poll puts on neither the urgent list nor the `tehas:next` list is the lowest kind, so one whose `tehas:next` came from an account the poll does not count is of that kind too. With `TEHAS_LOW_GATE` on the instance (see [tehas dispatch](/tools#tehas-dispatch)) the poll runs one command of yours before it starts any of them, once a poll at most, and only when such an issue would otherwise start. Only its exit status is read: 0 lets them start, and anything else holds them for that poll, a command that is missing, one that is not executable or one that outlives `TEHAS_LOW_GATE_WAIT` included, while urgent and `tehas:next` issues go ahead. What the command prints is kept in `~/tehas/logs/low-gate.log`, rewritten each time it is asked, so a gate that closes for a reason says that reason there. The log says `low issues wait: TEHAS_LOW_GATE answered S` once when the hold begins and `low issues may start: TEHAS_LOW_GATE answered 0` once when it ends, whatever the command answers while the hold lasts; after that each held issue says it on its own line. No cycle that already runs is stopped by it. A closed gate is not one of the holds [tehas status](/tools#tehas-status) shows, so a factory that starts nothing for it reads healthy elsewhere: look in `~/tehas/logs/dispatch.log`. With no setting there is no gate, and every ready issue is started as before.

An issue with `tehas:blocked` is not started while an issue it names is open, so its slot goes to work that can move. Once every issue it names is closed, it starts. Apply `tehas:blocked` on its own and leave `tehas:ready` off: the dispatcher starts the issue when its blockers close, on your approval, as if you had applied `tehas:ready` then. The label counts on the same terms as `tehas:next`, with one difference: on its own it starts work, so it counts only from an allowed labeller, and only while the text is the one it approved, as `tehas:ready` does. A `tehas:blocked` the millwright applies holds an issue but does not start one. On an issue tehas is already working on, one with `tehas:running`, `tehas:awaiting-merge`, `tehas:question`, `tehas:planning`, `tehas:refreshing`, `tehas:plan` or `tehas:refine`, it starts nothing. The blockers are named in a comment by an allowed labeller or the millwright that holds the marker:

```
Blocked by #12 and #34.

<!-- tehas:blocked by=12,34 -->
```

Up to 20 numbers, separated by commas and no spaces. The newest such comment among the last 100 counts; one by anyone else is ignored. The issue also waits, and the log says why, when the label counts and no such comment is there, when the newest one does not hold a marker that reads, when it was last edited by another account, or when GitHub does not give the state of a number it names. Once every blocker is closed the issue starts, and the start takes `tehas:blocked` off with `tehas:ready`. A blocker that is closed already holds nothing: name one that is still open, or reopen it. To start a blocked issue anyway, swap `tehas:blocked` for `tehas:ready`. To stop it starting, take the label off.

An issue labelled `tehas:improvement` and `cross-project` together is not started: the finding says the change belongs in another repository of the stack as well as this one, so one cycle here cannot make it, and a cycle on it spends a slot and its tokens before it parks. The dispatcher starts nothing and the log says why; it takes no label off and writes no comment, because the issue is a proposal the loop filed and a person decides what becomes of it. Read the `Names` line, which says the other repositories the loop read, and either work on both sides yourself or close it. Either label alone starts a cycle as any other issue would: `cross-project` on an issue the loop did not file is a person's label and means nothing to the dispatcher, and a finding that names no other repository is worked on here. The hold is on `tehas:ready` and on a ticked retry alike; a `tehas:plan` session on such an issue is allowed, since planning where the change goes on each side is what it is for.

An epic is never started: an issue with open sub-issues is a plan broken into issues, not a change one cycle can make, and a cycle on it spends a slot and its tokens before it parks as too large. When `tehas:ready` is applied to such an issue, the dispatcher takes the label off, says so in one comment and starts nothing; a retry ticked on one starts nothing and the log says why. The count comes from the issue GitHub already answers for, so a GitHub without sub-issues is unaffected. Read the plan and apply `tehas:ready` to one of its sub-issues instead; a sub-issue with no open sub-issues of its own is started as any other issue. A `tehas:plan` session on an epic is allowed: planning an epic is what it is for.

With `TEHAS_SIZE_MAX_FILES` set on the instance, an issue whose own body names more files than that number is not started either. The dispatcher reads the count off the approved text itself: a path is what stands between two ticks and holds a `/` or ends in `.sh`, `.rs`, `.md`, `.json`, `.toml` or `.yml`, and one path counts once however often it is written. Such an issue is a stack of changes wearing the shape of one issue, and a cycle on it spends its explore and its plan before it parks. The dispatcher takes `tehas:ready` off, puts `needs-human-attention` on, and says the count, the setting and the way out in one comment: narrow the issue to the part one cycle can make, or split it into issues of that size, then apply `tehas:ready` to one of them. The hold belongs to `tehas:ready` alone, so it holds nothing else back: a ticked retry, a refresh of the bot's own pull request and a `tehas:plan` session start as they did. Unset, the dispatcher never asks, and a wide issue starts as any other.

| Label on the issue      | Meaning                           | Your move                                |
| ----------------------- | --------------------------------- | ---------------------------------------- |
| `tehas:ready`           | Queued                            | Wait, or remove the label to withdraw it |
| `tehas:running`         | A cycle owns it                   | Leave the issue and its branch alone     |
| `tehas:awaiting-merge`  | Pull request open, only the merge left | Review and merge                    |
| `tehas:shipped`         | A merged pull request closed it   | Nothing; `tehas tidy` and `tehasbot status` give it |
| `needs-human-attention` | The cycle stopped                 | Read the comment; see below              |
| `tehas:plan`            | A planning session is asked for   | Wait for the first comment               |
| `tehas:refine`          | A refining session is asked for: planning with the refine brief | Wait for the first comment |
| `tehas:planning`        | A planning turn is running        | Wait                                     |
| `tehas:replaced`        | Closed: a refine filed the issue that replaces it | Work on the issue named in `Replaced by` |
| `tehas:refined`         | A refine filed its first part; later parts remain | Apply `tehas:refine` for the next part, or close it |
| `tehas:question`        | The factory asked something       | Tick a box; in a planning session you may reply instead |
| `tehas:improvement`     | Filed by the improvement loop     | Read it; apply `tehas:ready` if it should be built |
| `cross-project`         | On a finding of the loop: the change belongs in another repository of the stack too | Not started; read the `Names` line, then work both sides or close it |
| `tehas:millwright-escalated` | The millwright left it to a person | Read the millwright's note; decide, or take the label off to hand it back |
| `tehas:next`            | Started after urgent issues, before the rest | Take it off to give the issue its place by number back |
| `tehas:blocked`         | Started once every blocker it names is closed | Close the blockers; swap it for `tehas:ready` to start it now |
| `tehas:refreshing`      | On a pull request: a [refresh](#refreshes) works on it | Do not merge or push to it now; it comes off when the cycle ends |

`tehas:ready` from an allowed labeller starts a fresh cycle whatever else the
issue carries. So does one from [the millwright](#the-millwrights-grant), within its grant. tehas's labels of the earlier outcome go when the cycle starts.
`needs-human-attention` stays until the cycle runs: the cycle starts with
`--restart`, and pi-rukas takes the label off as it begins. The dashboard
counts an issue with `tehas:running` as running, whatever else it carries. On
an issue whose cycle is still running, the label waits: a fresh cycle starts
when that one has ended. Remove the label before then to withdraw it.

Once an issue is closed, the next poll takes tehas's state labels off it:
`tehas:running`, `tehas:awaiting-merge`, `tehas:question`, `tehas:planning`,
`tehas:refreshing`, `tehas:blocked`, `tehas:ready` and `tehas:plan`.
`needs-human-attention` and the labels a person chooses, such as `tehas:urgent`
and `tehas:next`, stay.

The same edit puts `tehas:shipped` on an issue that a merged pull request
closed: `tehas tidy` reads what closed the issues it tidies and adds the label
in the call that strips their state labels, so the label says one thing, that a
merge closed the issue, whatever the queue's list happened to hold. Two commands
put the label from a merged closer, and both read the one answer from
`lib/shipped.sh`: `tehas tidy` in the edit that takes the state labels off, and
`tehasbot status` from its own read of the list for any issue missing it.
`tehas tidy` is the one that says what it did in
`~/tehas/logs/dispatch.log`. Neither gives it to the control issue. An issue
that already carries the label is not
labelled again, and one that the read could not answer is never labelled on a
guess: its state labels still come off and the tidy line says no merged pull
request was read as the closer, or, when the whole read failed, that the read
failed once for the poll.

The pages' **Shipped** figure counts the closed issues that carry
`tehas:shipped`, and not the hundred most recent items of the queue's list: an
issue a person closes as completed without a merged pull request does not carry
the label and is not counted. The count is the `completedCount` of the queue
snapshot; where the snapshot has none, the figure is the length of that list.

When the cycle starts and when it ends, the bot comments on the issue. The token
usage per agent role is posted on the pull request.

The console on the [factory floor](/) and on the [plain dashboard](/dashboard)
shows one cycle at a time. When several are running, or one is running and an
earlier one can still be read, a row of tabs above it names each by its issue
number.

A timer polls every two minutes, and a cycle that ends starts a poll at once. A
poll that fails does not stop the polling: the timer counts from the last start
as well as the last end, so the next poll comes two minutes later whatever this
one exited with. To see the factory's state, or to see or force
a poll by hand on the host:

```
tehas status                 # taking work? the queue, the current or last cycle, the slice's room
tehas status --json          # the same for a program, with the demand (see the factory API)
tehas top                    # the same overview, on one screen that redraws itself
tehas dispatch --dry-run     # what would it do?
tehas dispatch               # do it (returns once the cycle is started)
systemctl --user status 'tehas-cycle@*'   # which cycles are running
tehas stop 12                             # end the cycle for issue 12, clear what it left
systemctl --user stop tehas-cycle@12      # stop the cycle for issue 12
```

A stop ends the container. The issue gets `needs-human-attention`, and the
console shows the cycle as interrupted.

`tehas stop 12` asks that unit to stop and takes away what the cycle left on the
host: its container, its workspace, its transcripts, and the files in
`~/tehas/state` that make this host think the cycle still runs, is owed, or is
parked. It keeps the cycle's record and its logs. Its last step is the issue's
own: when the host part went clean, `tehas:running` comes off it, with no other
label added and nothing written there. See
[`tehas stop`](/tools#tehas-stop).

`tehas` is the one command to type on the host; `tehas help` lists what it
does. See [Commands](/tools).

### What a poll costs

Under its `factory` line, `tehas status` shows what the polls cost:

```
poll      last 24 s wall, 3.1 s CPU, 12 gh (0 writes); last hour 14 polls, 41 s CPU, 2 writes
```

Each poll that holds the dispatcher's lock records these figures in the state
store as it ends, however it ends. A dry run records nothing.

- **wall**: the time from when the poll took the lock to its end.
- **CPU**: user and system time of the poll and of the commands it waited for,
  from bash's `times`, `libexec/tidy` among them.
- **gh**: the calls the dispatcher made to `gh` itself. Calls of the commands it
  runs, such as `libexec/status` and `libexec/tidy`, and a call under
  `timeout`, are not counted.
- **writes**: the lines of the host shim's write log,
  `~/tehas/state/github-writes.log` (see [GitHub App identity](/github-app)),
  dated from the second the poll started to its end, by kind: `label`,
  `comment`, `edit` and other. They are the writes of the whole host in that
  time, a cycle's included. What the shim counts as a write is what a call asks
  GitHub to do, not how it asks it: a `gh api graphql` call sends POST whether
  it reads or writes, and the shim logs it only when the query names a
  `mutation`. An idle poll reads and writes nothing, and its figure is 0.
  Without that log, on a host with no GitHub App, the writes are not counted
  and the line leaves them out.
- **starts**: the cycles the poll started, 0 when it started none. A poll of an
  older tehas keeps no answer for that, which is not the same as 0.
- **last hour**: the polls that ended in the last hour, their CPU time and
  their writes added up.

`~/tehas/libexec/store poll-show` gives the same figures as JSON, with the
writes of the last poll by kind. `store poll-show --api --now <unix>` shapes
them in the names [`tehas api`](/api#tehas-api) prints them under, with the
last poll's age in minutes counted from `<unix>`; `tehas api --json` carries
that as its `poll` field, and its plain output prints the line of words below
the millwright's. `store prune` drops the records with the
other old rows of the store. With no poll on record there is no `poll` line.

`~/tehas/libexec/store poll-idle --since <seconds>` answers one object over the
polls of the last `<seconds>` that started no cycle: `{"polls": n, "cpu_ms": m,
"wall_ms": m, "writes": m}`, the three `m` being the median of those polls. The
`writes` figure is left out when none of them had a write log, and `{"polls":
0}` is the answer when no idle poll is on record. A poll with no `starts` on its
row is not one of them. The health check uses it for the budget of idle polls.

### The dispatcher's shadow

`tehas-dispatch`, the dispatcher's decisions written in Rust, runs beside
each poll of `tehas-dispatch.service` while `~/tehas/state/dispatch-shadow.on`
is there, and acts on nothing on GitHub or in the state store: it reads what the poll reads and decides, and the bash poll
still does the work. It asks what the poll asks to read, the model endpoints'
`/models` among them, and, only when an issue it would start needs one, the
model's answer to a one-pixel image and the sandbox image's
`--capabilities` (a container that ends at once). It has 120 seconds before
the poll and 60 after it. Just before
the poll it reads GitHub, the state store and the host and keeps its decision
in `~/tehas/state/dispatch-shadow.json`. The poll's own lines are kept in
`~/tehas/state/dispatch.last` as they go to `logs/dispatch.log`. After the
poll it sets its decision beside them and adds one line to
`~/tehas/logs/dispatch-shadow.log`:

```json
{"time":"2026-10-26T10:00:00Z","same":false,"bash_only":["start #7 (labelled by alice)"],"rust_only":[],"exit":{"bash":0,"rust":0},"needs":[],"picked":[7],"gh_requests":6,"seconds":2.4}
```

- **same**: the two logged the same lines and ended with the same status.
- **bash_only**: lines only the poll logged. Some are what only acting says:
  the code index's hold before a start (`holding: the code index is
  refreshed first ...`), tidy's lines, the analytics' warnings. The shadow
  decides, it does not act. A decision line here, a hold or a skip, is a
  difference to look into.
- **rust_only**: lines only the shadow said. The world can move in the
  seconds between the two (a cycle ends, a label is applied), so a single
  line here is not yet a fault; the same line poll after poll is.
- **needs**: facts the shadow was given no answer to. It never runs
  `TEHAS_LOW_GATE`: on an instance that sets one, a poll that would ask it
  names `gate` here.
- **gh_requests** and **seconds**: what its snapshot cost.

`jq -c 'select(.same | not)' ~/tehas/logs/dispatch-shadow.log` lists the polls
that differed. It is off unless a person asks for it: `touch
~/tehas/state/dispatch-shadow.on` starts it, and removing the file stops it.
On this repository's queue it costs about 9 s of wall time before each poll,
3 s of CPU and 20 GitHub reads, so it runs on the host where it is being
proven and not on every host. A host where `libexec/tehas-dispatch` is not
installed, or the file is not there, polls as before; the shadow failing never
fails the poll.

### The cycle's shadow

`tehas-cycle`, a cycle's report written in Rust, decides beside each report
of `tehasbot cycle` while `~/tehas/state/cycle-shadow.on` is there, and acts
on nothing on GitHub or in the state store. Just before the cycle reads what
tehas work left, the shadow reads the same files, the cycle's record and its
approval, asks GitHub once for the bot's pull requests on the branch (and,
for a handoff, the issue's labels), decides, and keeps its report in
`~/tehas/state/cycles/<issue>.shadow.json`. It has 60 seconds. The bash
report then keeps what it did beside it: each call of `gh`, the park and the
end it put on record. After the report the shadow sets the two side by side
and adds one line to `~/tehas/logs/cycle-shadow.log`:

```json
{"time":"2026-10-26T18:00:00Z","issue":7,"attempt":1,"same":true,"differs":[],"rust_log":[],"done":"done #7 rc=20 status=aborted step=develop minutes=12 pr=none oom_kills=0 forks_refused=0","seconds":1.4}
```

- **same**: the two made the same writes to GitHub in the same order, and put
  the same park and the same end on record. A comment counts as the same
  whether or not it carries the words of a label that could not be changed,
  since only the bash met that.
- **differs**: which of `end`, `park` and `writes` differed, each with both
  sides on the line.
- **rust_log**: the warnings the shadow's report would log.

A report that differed is kept whole, with what the bash did, in
`~/tehas/state/cycle-shadow/`, the newest 50. GitHub can answer the shadow
and the bash differently in the seconds between their asks, so one
difference is not yet a fault; the same one cycle after cycle is.
`jq -c 'select(.same | not)' ~/tehas/logs/cycle-shadow.log` lists them. `touch
~/tehas/state/cycle-shadow.on` starts it, and removing the file stops it. A
host where `libexec/tehas-cycle` is not installed, or the file is not there,
reports as before; the shadow failing never fails the report.

## 4. Review and merge

The pull request comes from the bot (the factory's app or bot account) against the product's default
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

A pull request that waits while other work merges may start to conflict with
the branch. Where the instance turns [refreshes](#refreshes) on, the factory
merges the branch into it on its own.

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

A runner that died before it wrote its result leaves no verdict of its own, so
tehas reads the cycle's own run log for the step it reached and asks GitHub for
the bot's pull request of the branch `commit-pr` pushed. With one there the
comment is `Tehas cycle handed off with its pull request open and its review
unfinished`: `needs-human-attention`, the pull request named, and no boxes,
because a clean retry redoes what the open pull request already holds. With
none, the issue is parked as any other that stopped without a pull request. The
comment names the runner's own exit, not the exit of `tehas work`, and says the
step was read from the log. See [the runner died and wrote no
result](/troubleshooting).

An issue never ends a cycle with both labels.

A cycle that reaches its deadline parks with the cap `deadline`. tehas gives
pi-rukas a deadline a quarter of `TEHAS_WORK_TIMEOUT` before the end (at least
one run of the gate and the handoff's 8 minutes): after it, the cycle starts no new step and
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

A cycle that parks at `child-timeout:develop`, its developer stopped at its
time bound, first writes a note into the issue's body, because a cycle reads
the body and none of the comments. The note sits at the end, between
`<!-- tehas:last-attempt base=... -->` and `<!-- /tehas:last-attempt -->`,
under the heading "The last attempt ran out of time". It says how long each
step took, whether the gate last passed, what was left in the workspace, and
the files the plan named, and asks the next cycle for a smaller change. All of
it comes from what pi-rukas's driver recorded, none from an agent's words. A
later develop timeout replaces the note. Edit or delete it as you like. The
note is written as the bot, so the issue shows an edit by the bot, and a retry
on it approves the text with the note. A note that cannot be written is a
warning in the log, and the park is posted as before.

When the note is there and the rest of the body has not changed since it was
written, a second develop timeout does not write it again, and its park
comment has no `Retry from a clean start` box, only `Leave it parked`: the
same text would run into the same bound. The comment says to split the issue
with `tehas:plan`, or make the body smaller and apply `tehas:ready`. A
`tehas:ready` on the unchanged text is held as below, the millwright's too.
The note and the check need `libexec/tehas-cycle`; without it the park is
posted as before.

A new `tehas:ready` on an issue whose last cycle parked asks for nothing while
the issue still carries the body that cycle ran on. The dispatcher compares the
hash of the body that cycle's approval bound, as the state store keeps it for
the park, with the issue's text now, and when they are equal and the park was
not a provider outage it starts no cycle, leaves the park and the labels as
they are, and says so in the poll's log. Once per park it also posts one
comment: edit the body, or tick `Retry from a clean start`. The tick is the
park's own ask and is never held, so a retry on the same text starts as it
did, and so does a start after any change to the body. A park of a provider
outage is held by its own rule instead, and a park of a cycle started by hand,
which no approval bound to a body, holds nothing. A park that cannot be read
holds the issue for that poll.

An issue closed since its cycle stopped is not work. Neither the tick nor a new
`tehas:ready` starts anything on it, and the poll's log says so.

A cycle that a provider outage parked says so in its comment: `parked by a
provider outage`, with the cap. Nothing needs doing: once the model endpoint
answers, the dispatcher starts the issue again on its own and says so on the
issue, on the word of whoever started the cycle that parked. It does that
once. If that cycle is parked by an outage too, the issue waits for a tick or a
label like any other, and so does a cycle started by hand. `tehas stop` on the
issue gives up that one retry.

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
it for 7 days, three issues a poll at most, and logs each removal. With them go
what the cycle left outside its workspace: the agents' transcripts in
`~/.pi/agent/ensemble-runs/issue-<issue>`, the console
`~/tehas/live/console-<issue>.txt`, and the transcripts of the issue's planning
session, `~/.pi/agent/ensemble-runs/plan-<issue>`. An issue whose workspace is
already gone loses those three on their own, on the same rule: the issue's lock
free and none of them touched for 7 days. The lock has to be a lock file: as
with `tehas stop`, one that is a link or a directory says nothing about the
issue, and the poll removes nothing of it and says so. An issue the poll cannot
date is kept too, as `tehasbot clean` keeps what it cannot date, and is tried
again on a later poll. Only the names of that one issue go, and the dated
directories of `~/.pi/agent/ensemble-runs` are the runner's, not tehas's, so the
sweep leaves them for a person.

A label or a tick approves the text of the issue as it was at that moment. If
someone other than an allowed labeller changes the title or the body
afterwards, the issue is skipped and the dispatcher's log says who changed it.
Read the new text. Then approve it again: for the label, remove `tehas:ready`
and apply it again; for a retry tick, untick the box and tick it again.

## Planning on an issue

A planning session is a conversation with pi-rukas's planner that ends in a
filed issue. It runs on the issue, so you can steer it from a phone. The agent is
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
3. The bot comments that the session has started. Each turn ends in a comment
   with what the planner found. A turn is one run of pi-rukas's headless
   planner and can take many minutes. When it needs a decision, the comment
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
   as usual, and the turn's end starts the poll that posts its outcome. The
   unit holds `~/tehas/state/plan-<issue>.json.lock` while the turn runs, so
   no poll starts a second turn on the issue, and `tehas status` shows the
   turn on a `plan` line. One
   planning turn runs at a time, so while another runs, your answer waits
   until it ends, which is up to two hours (`TEHAS_PLAN_TIMEOUT`). A
   running work cycle does not hold an answer back. A poll also handles one
   answer or one planning start and then stops, and it does nothing while the
   model endpoint does not list the model.
5. The planner shows you a drafted plan before anything is filed: the comment
   holds its title and spec, lists its open points with the planner's
   proposals, and offers one box, **File this plan**. Tick it to file the
   plan exactly as shown, proposals included, as one issue; reply to change
   it, and the next turn drafts it again. A session files once. Every turn of
   a session works on the commit the session started on. The plan is filed
   with the bot's token. To finish, tick
   `End this planning session`. Filed issues are never started on their own:
   apply `tehas:ready` to the ones you want built. A `tehas:ready` from any
   account outside the list, the bot's included, is taken off.

pi-rukas's record of a session is kept on the host, in
`~/tehas/work/<name>-plan-<issue>.keep/plan-sessions`, and carried into each
turn. If it is gone, for example after the host was rebuilt, the session cannot
go on: the bot ends it and says so. Apply `tehas:plan` again to start a new session.

Only answers from allowed labellers count, and the millwright's within its grant
(see [The millwright's grant](#the-millwrights-grant)). A box ticked by anyone else, or
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

The bot (`TEHAS_BOT_LOGIN` with an app, otherwise the token's account) is never one of the allowed labellers. If it is listed in
`TEHAS_ALLOWED_LABELERS`, the dispatcher stops with an error rather than let
the bot approve its own work: every poll reads its own login first, before it
reads a label or changes one. The Protolab refuses the same list, with an error
before it reads, builds or removes a page. A poll or a pass that cannot read
its own login stops there and does nothing else.


### The dispatcher in Rust

`TEHAS_DISPATCHER=rust` in `instance.env` hands every poll to
`~/tehas/libexec/tehas-dispatch poll`, once `libexec/dispatch` has checked
the settings, the model keys and the checkout. It is the same poll, in the
same order and with the same lines in `logs/dispatch.log`: the lock, the
snapshot of the queue (one GraphQL query in pages of 100 where the bash poll
makes a call for each list), the decision, the actions, and the end every
poll that held the lock has. Each write to GitHub goes through the state
store's action journal first, and a poll begins by settling the writes a poll
before it left open there: each is read again on GitHub and recorded as done
or as not done, and never made again. `bash`, or leaving the setting out, is
the bash poll. A value of neither is exit 78.

Where it differs from the bash poll:

- A poll systemd stops with TERM ends without its cost record and tidy; the
  next poll's tidy covers it.
- The low gate is run while the poll decides, so a poll that an earlier step
  ends has run it; it writes `logs/low-gate.log` and nothing else.
- The open planning sessions are read once, with the queue; the bash poll
  reads them again after its own planning writes.
- A closed issue is not in the query, so the poll says nothing of it; the
  bash poll, which reads each issue again, logs `skip #N: it is closed`.

It is not the default, and not proven on a host yet (see
[features.md](features.md)). To go back, take the line out of
`instance.env`: the next poll is the bash poll.

### Refining a request

A parked issue is not held back: `tehas:refine` or `tehas:plan` from an allowed
account starts the session there too, and the start takes `tehas:question` off,
as a `tehas:ready` cycle start does.

`tehas:refine` starts the same planning session with one difference: after the
issue's approved text, the planner is given the refine brief,
`~/tehas/lib/refine-brief.md`. It tells the planner to read the code first and
to plan the request into one issue that a single `/work` cycle finishes: one
behaviour change, its files by exact path, a named test file for each
acceptance criterion, what is out of scope, and nothing under `.github/` or
`.pi/`. The part's title starts with its classification as a conventional-commit
type, `feat:`, `fix:`, `chore:`, `docs:`, `test:` or `refactor:`, keeping one
the request already has; pi-rukas gives the cycle's pull request the same
type, where a title without one becomes `chore(work):`. A session files one issue, so for a larger request the planner files
the first part and lists the rest under "Later parts". Apply `tehas:refine`
again to the original issue to plan the next part. The label is checked like
`tehas:plan`: only an allowed account's counts. An issue with both labels is
planned, not refined. The filed part is never started on its own: read it and
apply `tehas:ready`. Run `tehas labels` once after an update that brings this
label.

When a refine files, the session is over: pi-rukas files once per session.
The filed issue gets `Replaces #<n>` as its first line and is linked as a
sub-issue of the original. If the plan lists no later parts, the original gets
`tehas:replaced` and is closed as not planned. If it does list them, or its
body cannot be read, the original stays open with `tehas:refined`: apply
`tehas:refine` to it for the next part, and close it when nothing is left.
Either way the original then gets a comment `Replaced by #<m>` with the
planner's words. A sub-issue link that cannot be made is a warning in the log;
the first line of the filed issue says the same, and a poll that cannot write
that line tries again on the next. A `tehas:plan` session that files is
posted as before.

#### Refined on its own

A cycle that parks at explore as `intent-park` with the reason
`underspecified` or `too-large` is not yet one cycle's work. The next poll
refines it once without a label, on the strength of the `tehas:ready` (or the
retry) that started the cycle, and says so in the session's first comment. It
does not wait for a free cycle slot. A box of the park ticked first wins, even
one the factory holds for a free slot. It does not refine when:

- the factory is paused (it does once the factory is resumed),
- the issue is closed, or no longer carries `needs-human-attention` and
  `tehas:question`, or the park's question was answered,
- a person added `tehas:ready`, `tehas:blocked`, `tehas:plan` or
  `tehas:refine`, or the issue carries `tehas:running`, `tehas:refined` or
  `tehas:replaced`,
- the issue has had a planning session before, or a planning session filed it
  (its first line is `Replaces #<n>`),
- the cycle was run by hand or by the millwright, or the account that started it
  may no longer label,
- the text was changed since the cycle began, by anyone: a person at work on
  it decides.

Then the issue stays parked for a person, as before, and the log says why. A
park is refined at most once: the state store spends it when the session
starts, or when the poll finds it cannot. An automatic session that reaches
its turn limit, or that pi-rukas lost, ends with `needs-human-attention`, so
the issue reads as parked again. One you end yourself ends as any other. Its
lines in `tehas history` carry `auto`.

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

## Reading what ended

The records of a finished cycle stay on the host. Two commands read them; both
take a cursor, so a program that keeps what it was last given reads each line
once.

```
tehas history                    # what ended, oldest first
tehas history --json --after 1760700000-12-41
```

One line for each cycle and each planning turn, `kind` telling them apart: when
it ran, how it ended, where it stopped, who asked for it. The same lines in
JSON with `--json`. It reads only what the units wrote, takes no lock a running
cycle holds, and writes nothing, so it is safe to run while the factory works.

```
tehas export --after 1760700000-12-41 ~/archive
```

Copies the result, the logs, the agents' transcripts and the Protolab page of
every cycle since that cursor into `~/archive`, one directory each. Every text
file goes through the console's mask, so a token or a private address that
reached a log does not reach the copy. An export is never written under
`~/tehas`, not even through a link, and the mask catches accidents rather than
everything: an export is still a copy of what a cycle wrote, so where it goes is
still a decision.

## The last 365 days

The factory records every run it makes, with the tehas and pi-rukas versions
it ran, and keeps 365 days of them. Two commands read them:

```
tehas analytics report                    # the ten-day report in the terminal
tehas analytics export copy.db --json     # a copy of the database, to fetch
```

The report is the one on the plain dashboard. The copy is for deeper analysis
elsewhere; it holds no token, host or address. What is in it, column by
column, is on [Analytics](/analytics). `TEHAS_ANALYTICS=off` stops the
recording, and both commands then say so and exit 69.

## Limits that hold work back

- A limit set with `tehas limit`, when there is one: see
  [Limit and drain](#limit-and-drain).
- At most `TEHAS_TICKET_CONCURRENCY` cycles at a time. It is a ceiling, not
  a promise: the dispatcher starts fewer when the slice has no room. With
  refreshes on, refreshes have a lane of their own (see
  [refreshes](#refreshes)) and do not count here.
- No new cycle while the host runs short of disk: less than
  `TEHAS_CYCLE_MIN_FREE` (10g by default) free on the file system of `~/tehas`
  or on the one Docker keeps its images on, whichever has less. The poll reads
  both with `df`; space it cannot read is held on as well, and the log says
  `holding: <n> GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=<value>: no
  cycle starts` or `holding: the free disk space could not be read: no cycle
  starts`. Neither figure is ever taken for the other: when `df` answers
  nothing for one of the two file systems, the poll holds rather than guess.
  Running cycles are not touched, and a planning turn is not held, so
  what the host already works on finishes; only new cycles wait. On 2026-10-20
  a host's disk filled and every running cycle died, which is why the check is
  there. Previews have their own, lower, bound in `TEHAS_PREVIEW_MIN_FREE`.
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
- No cycle for an issue whose text says a file goes there (`SCREENSHOT GOES
  HERE`, `IMAGE_GOES_HERE`, `the image goes here`) while its text links no
  attachment at all. The instance giving a cycle images or not makes no
  difference: either way the cycle would be given no file and no word of the
  gap, and would spend its explore on a premise it cannot see. The log says
  `hold #N: its text says <the words> and links no attachment, so the cycle
  would be given none`; the issue keeps every label it has and gets no comment,
  because what is left to do is the owner's: attach the file, or write out what
  it shows. A planning turn is not held by it, and neither is a retry tick or a
  refresh held by anything else about it.
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
  poll as soon as that one ends. `tehas-dispatch.service` removes the file
  before the poll runs, so a poll that stops on bad settings does not leave it
  to start the next one at once. If systemd still fails the path unit, for
  example at the service's start limit, `tehasbot health` warns
  (`checks.units`), and the next `install.sh` resets it and starts it again.
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

- No new cycle while the branch is red, when the instance sets
  `TEHAS_BRANCH_CHECK`: each poll asks GitHub, in one call that may take 30
  seconds, for that check on the head commit of the branch work lands on.
  When it failed, timed out or could not start, the poll starts no cycle, no
  retry and no refresh, and says `holding: main is red at <sha>`, with the
  branch's name and the first 12 characters of the commit. A cycle started
  then would find the gate red on its base and park hours later. Running
  cycles go on. When the check is pending, missing or cannot be read, cycles
  start as before, so an outage at GitHub does not stop the factory; the log
  says why it could not tell, once each time that changes.

The review cap, the new pi-rukas, the room, the pressure and a red branch hold back cycles
only. Planning
turns still run. `tehas status` shows the room in its `slice` line, as the
dispatcher's last poll read it, and `tehas dispatch --dry-run` reads it
afresh. A hold is written to `~/tehas/logs/dispatch.log` once, when its reason
changes, and `released: cycles may start again` once when it ends. The gate of
the lowest issues is not one of these holds: it reaches neither `tehas status`
nor `/health`, and is seen in `logs/dispatch.log` and `logs/low-gate.log`.

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
- **Or none at all.** With `TEHAS_IMPROVE_RUKAS_SKIP=1` a finding of the
  category `pi-rukas` is not filed anywhere, and the run line counts it as
  `skipped`. This is for a host whose pi-rukas developers read its cycles
  themselves, through the [MCP server](/mcp-server). It cannot be set with
  `TEHAS_IMPROVE_RUKAS_REPO`, and no issue there is checked again.
- **A finding at the boundary is two issues.** The category `boundary` is
  for a problem with a half in the factory and a half in pi-rukas, or a
  shape the two have to agree on. Each half is filed in its own repository,
  the two link each other, and both carry `cross-project` as well. Without
  a repository for pi-rukas it is one issue that names both halves.
- **The model reads what each cycle ran with**: the `PI_ENSEMBLE_*`
  variables its container was given, the `TEHAS_*` settings that shape a
  cycle, how many cycles were running as its container started, and the
  provider retry line of its preflight log. `tehas work` puts the first
  three on the cycle's record in the state store as the container starts.
  What the model reads may
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
  two halves of a boundary finding, and on a finding of any category that
  speaks of a repository of the stack beside the one its issue is filed in -
  the text names it as `owner/name:`, or an open or recently closed issue
  there cites one of the cycles the finding rests on. It is the dispatcher's
  tell that one cycle here cannot make the change on its own, and it holds such
  an issue back from starting; see
  [what happens next](#3-what-happens-next). The issue says those
  repositories in its `Names` line, so a person reads where the other half of
  the work is before labelling. The names are read from the stack the host
  gave the loop, never from a name a model wrote, and none is used that the
  stack does not hold. tehas does not act on
  `tehas:improvement`, and the loop never applies `tehas:ready`. A filed issue is a proposal a
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
  they are public. A log goes up masked as the console masks it: tokens,
  passwords, private addresses. A log that still holds something shaped like a
  token, or that could not be masked, is not uploaded, and the issue says so.
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
`not-commented=acme/rukas#12`. `skipped=2` says two findings about pi-rukas
were not filed. A run reads at most five cycles, the oldest
first; `left=2` at the end says two wait for the next run. `stopped=124
returned=2` says the analyst hit its bound of 60 minutes: what it had written
was filed, and the two cycles it named nowhere wait for a later run.

To turn the loop off, remove `TEHAS_IMPROVE_REPO` from `instance.env`: cycles
stop leaving records and a run does nothing. Or stop the timer with
`systemctl --user disable --now tehas-improve.timer`.

## The cleaner

A full disk stops previews and fails clones, and most of what fills it is
something the next build makes again. `tehasbot clean`, from
`tehas-clean.timer` once an hour (the first time 15 minutes after boot),
removes what has not been used for `TEHAS_CLEAN_AFTER` hours, 24 by default:

- the build output of a kept workspace: every directory anywhere under
  `~/tehas/work/<name>-<issue>` whose name is one of `TEHAS_CLEAN_DIRS`, the
  toolchain names listed in [Commands](/tools), and `<name>-<issue>.state/cache`,
  the cycle's copy of the code index. The trees beside the workspace are walked
  as roots of their own: `<name>-<issue>.before-retry`, the workspace a retry
  moved aside, and the worktrees of a cycle under either. The runner of this host
  makes them in a directory of its own inside the workspace, two levels under its
  root, and that depth is the reach of the listing: a runner that nested them a
  level deeper would leave their build output where it stands, with the log still
  reporting the issue walked. A cycle
  makes its worktrees in the workspace it was given, under a dot-name, and
  leaves them with a clone of their own and that clone's build output under them,
  so the walk of the workspace, which enters no dot-name, reaches none of them.
  An issue kept only by the tree a retry moved aside - the workspace of the cycle
  that merged is gone - is walked too. A worktree that is a symlink is not walked:
  a link there stands outside the workspace and is left while the roots beside it
  are taken. A kept workspace is never built in again -
  every cycle clones fresh - and the dispatcher's 7-day sweep takes the whole of
  it later. The issue's lock is held while they go, so no cycle starts in it,
  and a workspace touched within the age is left alone, the tree of a retry and
  the worktrees beside it being part of that age rule. Nothing is removed
  under a lock that is not there: an issue whose lock file is gone is left
  alone, because the cleaner cannot tell a finished cycle from a removed lock.
  `.git`, the sources, uncommitted changes, `memory` and `sessions` stay for a
  person to read, and no dot-name is entered.
- Docker's build cache of that age, with `docker builder prune`. This is the
  build cache of the whole host's Docker daemon: tehas is one project on one
  host, and the cache of anything else on it is made again too. No image and no
  volume is removed by this step.
- the host's scratch: old files under `~/tehas/tmp/dl` and
  `~/tehas/tmp/node-compile-cache`, old `~/tehas/tmp/analytics-facts.*` files
  an analytics call left when its caller was killed, and old
  `~/tehas/tmp/tehas-test.*` directories. Nothing elsewhere under `~/tehas`, and nothing outside it: not
  `~/tehas/dev`, so the build output a manual session keeps in
  `<name>.state/target` is walked for no build-output name. It is the one build
  output on the host with no automated reclaim, and a person's call: `du -sh
  ~/tehas/dev/*.state/target` says what the sessions hold, and `rm -rf` of that
  path clears one clone's build (see [tehas dev](/tools#tehas-dev)).
- the sandbox image's dated tags whose image is of that age, each with its own
  `docker rmi`. The newest dated tag stays, and so does any tag a container or
  a `uid` tag was made from: a cycle or a preview still running from an image
  keeps it whatever its age. The build's own sweep keeps the newest three dated
  tags; this step is what takes those past the age besides the newest.
- the base image the sandbox build pulls from upstream, of that age and made no
  container, removed by its own name with `docker rmi`. Nothing else is offered
  to docker: the step never asks for the images that have lost their tag, which
  is how a preview image of `tehasbot preview` and the rollback tag of an update
  stay untouched. Volumes are a person's call. A docker that answers no image
  list, or nothing
  about the host's containers, removes no image: an unknown is never a reason
  to take one away.
- the logs of cycles, planning turns, index runs and the improvement loop's
  analyst in `~/tehas/logs` older than 7 days, whether or not the improvement
  loop is on. Long-lived logs such as `dispatch.log` and `clean.log` stay.
- the state store's upkeep: a copy a day (see below), and rows past 90 days
  removed with `store prune --days 90`: cycle endings, settled actions of the
  journal, and old heartbeats of cycles that have ended. A cycle's own record
  stays.

One line per removal and a closing line with the bytes freed go to
`~/tehas/logs/clean.log`, and the same lines are printed, so a run by hand
shows them in the terminal. When the age rule passes, an issue is left alone for
one of two reasons, and the line says which: `kept the build output of #<issue>:
no lock to hold, so
nothing of it was removed` when something was there to remove and no lock was
there to hold it, and `nothing to remove in
#<issue>` when none of its trees held build output at all. Two other lines each
name a reason of their own, one before the age is asked and one because it could
not be answered: `kept the build output of #<issue>: its lock is held`, for an
issue a cycle is running, and `kept the build output of #<issue>: git could not
name the gitdir of one of its worktrees`, for a worktree whose record git no
longer answers for - a stale worktree, a moved clone, or a `.git` file that names
nothing. The second is why a worktree record that went stale keeps an issue's
build output on the disk: the cleaner takes what it dated, and a tree it could
not date is not taken. [Commands](/tools#tehasbot-clean) carries the wording of
all four. A timer run prints to
the journal: what it refuses
to do before it can open the log - settings it will not accept, another run
holding its lock, a state directory it could not make - is in
`journalctl --user -u tehas-clean` and in no log file. The command starts no
work, so the pause label does not hold it. A step that fails is logged as a
warning, the other steps still run, and the exit status is non-zero.

**To turn it on**, enable the timer, which `install.sh` and `tehas update`
install but do not start:

```
systemctl --user enable --now tehas-clean.timer
tail ~/tehas/logs/clean.log
journalctl --user -u tehas-clean
```

Run it once by hand with `tehasbot clean`, or with `tehasbot clean --dry-run`
to see what it would remove. **To turn it off**,
`systemctl --user disable --now tehas-clean.timer`; a toolchain the cleaner does
not know about is added to `TEHAS_CLEAN_DIRS` in `instance.env` rather than left
unmanaged.

The poll's sweep also removes the lock file of an issue nothing has used for 30
days, three a poll: one no cycle holds, whose last cycle on record ended 30
days ago, or that has no cycle on record and is that old.

### Backups and restore

A plain copy of `~/tehas/state/tehas.db` is not a backup: the store runs in WAL
mode, and the newest rows can be in `tehas.db-wal`. The cleaner makes a
consistent copy once a day with `store backup`, as
`~/tehas/state/backup/tehas-<YYYYMMDD>.db` (mode 0600), and keeps the newest
seven. `tehasbot health` warns (`checks.backup`) when there is none or the
newest is more than two days old. The copies `install.sh` makes before a
migration, `tehas-<schema>-<time>.db`, are kept beside them and never removed.

**To restore** a copy:

```
tehas drain --wait 3600         # start nothing, wait for the running cycles
systemctl --user stop 'tehas-*.timer' 'tehas-*.path' tehas-energy.service tehas-mcp.service
systemctl --user list-units 'tehas-*' --state=active   # wait until no service is listed
cd ~/tehas/state
mv tehas.db tehas.db.broken; rm -f tehas.db-wal tehas.db-shm
cp backup/tehas-<YYYYMMDD>.db tehas.db && chmod 600 tehas.db
TEHAS_HOME=~/tehas ~/tehas/libexec/store check
systemctl --user start tehas-energy.service tehas-mcp.service
systemctl --user start $(systemctl --user list-unit-files 'tehas-*.timer' 'tehas-*.path' --state=enabled --no-legend | cut -d' ' -f1)
tehas limit clear
```

Every unit that reads or writes the store is stopped first: one left running
keeps the old file open and goes on writing to it. `store check` must say
`"integrity":"ok"`. The next poll reconciles the labels
on GitHub against what the copy holds; a cycle that ran after the copy was
made is not in it.

## The millwright's grant

Optional. The millwright is an account that decides on parked issues from outside
the factory: start one again, start it again working differently, or leave it
to a person. It may also answer some planning questions. What runs as that account is not part of tehas. The factory says
what the account may do, and this section is that.

The millwright was called the fixer. Its old names are no longer read: a
`TEHAS_FIXER_*` setting stops every command with 78 and names the new one,
and the label `tehas:fixer-escalated` and the marker
`<!-- tehas:fixer issue=<n> action=... -->` count for nothing. A record
written as `how: "fixer"` still counts as the millwright's, and
[`tehas api`](/api#tehas-api) still lists `fixer-grant` and `fixer` beside
the new names.

- **It is an account of its own**, named by `TEHAS_MILLWRIGHT_LOGIN`. It is never
  the bot, because every agent posts as the bot: a poll stops with 78 when
  the two are one. It is never one of `TEHAS_ALLOWED_LABELERS`: loading the
  settings stops with 78. Unset, there is no millwright, and the account's labels
  count as any outsider's.
- **Its `tehas:ready` starts a cycle only within its grant**: a person
  approved an earlier cycle of the issue, the body is still the body that
  person approved, nobody outside the list changed the title or text since,
  and the millwright has started fewer than `TEHAS_MILLWRIGHT_MAX_RETRIES` cycles on it
  since that approval (2 unless the instance says otherwise). Otherwise the
  label comes off and the issue says why. So the millwright never starts work
  nobody asked for: it never readies a `tehas:improvement` proposal.
- **The approval record says so.** A millwright start has `how: "millwright"`,
  `rests_on`, the person's approval it rests on, and `millwright_starts`. A
  person's start counts again from none. Its other limits are the
  dispatcher's as for any start: the slots, the cap of waiting merges and
  the slice's room.
- **Its comment reaches the cycle.** The millwright's newest comment on the issue
  that starts with `<!-- tehas:millwright issue=<n> action=retry -->` or
  `action=change`, written in the ten minutes before its label and edited by
  nobody after, goes into the record as `retry_context`. Where the instance
  gives a cycle images or files, `tehas work` writes it to
  `retry-context.md` beside them, quoted, under a head that says a model
  wrote it. No mount and no variable more: elsewhere it reaches no cycle.
- **It may answer a planning question**, by a tick or a reply, only when
  no person has answered the same question: a person's tick or reply wins,
  whichever came first. And only in a session an allowed account's
  `tehas:plan` started, fewer than `TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS` times in
  the session (3 unless the instance says otherwise), and never for two
  turns in a row, so a person sees every second question at most. It may
  not tick `End this planning session`. Its reply starts with the line
  `<!-- tehas:millwright issue=<n> action=answer -->`, and the agent is told
  the answer is the millwright's. Any other answer of the millwright's is an
  outsider's: a tick is unticked and the bot says why, a reply is ignored
  and the bot says why once. GitHub says who last saved a comment, not who
  ticked a box, so a tick the millwright saved after another account edited the
  question is held: it is neither used nor unticked, and the bot asks a
  person, once, to tick again or reply. To overrule it, reply or tick before the next
  poll, or reply to the next question. The session on the state store
  (`plan-show`) keeps who started it and who answered each turn, and
  whether the millwright did.
- **`tehas:millwright-escalated`** is the millwright's word that a person decides.
  The factory does nothing on it; a start takes it off.
- **It may order the queue** with `tehas:next` and `tehas:blocked`, and
  name the blockers in a marked comment, as an allowed labeller may (see
  [What happens next](#3-what-happens-next)). Neither starts anything: an
  issue still needs a `tehas:ready` that counts. When to apply them, and how
  many, is the millwright's to decide, not the factory's.
- **Each cycle's end leaves a line** of history in the state store
  (`~/tehas/state/tehas.db`), the last 50 of each issue: when, its status,
  last step, cap, park reason, failure, whether a provider outage stopped
  it, whether it parked, its pull request, who started it and how. A
  planning turn leaves one of its own in the store too, with `kind` `plan`.
  `tehas history` lists both, oldest first, and `tehas export` copies what a
  cycle left behind. A millwright reads GitHub, not these records.
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

## Refreshes

Optional, off unless `TEHAS_REFRESH_CONFLICTS=1`. A pull request the factory
opened waits in `tehas:awaiting-merge` for a person. While it waits, other work
merges, and it may start to conflict with the branch. A refresh is a cycle that
merges the branch into that pull request, resolves the conflicts and pushes to
the same branch: the same pull request, its review kept. Never a rebase, a
force-push or a second pull request. It needs a sandbox image whose pi-rukas
takes `--continue-pr` and says so with `features.continuePr` in its
`--capabilities` (pi-rukas#1468, in v0.14.0); until then a refresh starts
nothing and the dispatcher says so.

**What it reads.** `tehasbot refresh`, every `TEHAS_REFRESH_EVERY` minutes (10
unless the instance says otherwise), lists the open pull requests of the repository. It keeps the bot's that close
exactly one open issue carrying `tehas:awaiting-merge`, or `needs-human-attention`
after a handoff that left the pull request open, without `tehas:running` and not
the control issue, that are the only open pull request that closes it, and come
from a branch of the repository. A draft is read as any other pull request.
Those GitHub says are `CONFLICTING` get a request,
`~/tehas/state/refresh/<issue>.json`. `UNKNOWN` is read again on the next run. A
request whose pull request no longer qualifies goes. `tehas-refresh.timer`
starts it every minute, and it goes ahead only when the last run began that
many minutes ago, by `~/tehas/state/refresh/last-run`. A run that failed
counts. Each run writes one line to `~/tehas/logs/refresh.log`:

```
2026-10-18T09:00:00Z read 6 pull requests, 1 conflict, wrote 1 requests
```

**When it starts.** The dispatcher checks each request again at the start,
and starts it on the word of the person whose approval the pull request's cycle
rests on (the `by` of the approval of the issue's last start, or the person's
approval it rests on after the millwright):

- the issue is open, carries `tehas:awaiting-merge` or `needs-human-attention`
  and none of `tehas:running`, `tehas:millwright-escalated`,
  `tehas:plan`, and no cycle or planning session is open on it. A handed-off
  issue that carries `tehas:ready` gets the fresh cycle a person asked for,
  not a refresh; a `tehas:question` the handoff left behind is no bar, and the
  start takes it off;
- its body is the body that person approved, and nobody outside
  `TEHAS_ALLOWED_LABELERS` renamed or edited it since;
- the pull request is still the bot's, the one open pull request that closes
  the issue, at the head the request named, and GitHub still says
  `CONFLICTING`. A person who pushed to it since is handling it;
- fewer than `TEHAS_REFRESH_MAX` refreshes of it since that approval (2 unless
  the instance says otherwise), and none that parked. A start that failed
  counts as none: where the mark of a refresh could not be put on the pull
  request, or the cycle never ran, the dispatcher writes the record back as it
  found it and asks again on the next run. The count is what the bot itself saw
  start.

A request that fails a check goes, and the dispatcher's log says why. At the
cap or after a parked refresh the bot says so on the issue, once: a person
resolves the conflict or applies `tehas:ready`. A pull request that carries
`tehas:refreshing` waits.

**The refresh lane.** Refreshes run in a lane of their own, so a conflicting
pull request is refreshed while every cycle slot is busy. At most
`TEHAS_REFRESH_SLOTS` refreshes run at once (1 unless the instance says
otherwise), and they do not count against `TEHAS_TICKET_CONCURRENCY`. The slice
holds four containers whatever the slots say, so new cycles leave room in it
for the lane: while the lane has a slot free, a cycle does not take the last
room of the slice. On a host with `TEHAS_TICKET_CONCURRENCY=4` and one lane
slot that is three cycles and one refresh. The poll says
`holding: tehas.slice has room for 1 more and the refresh lane keeps 1`, or
`holding: 3 of 4 free slots start: ... the refresh lane keeps 1`. With every
cycle slot busy the hold line ends `the refresh lane has 1 of 1 free`, and a
request that finds the lane full waits: `wait #<issue>: the refresh lane is
full (1 of 1)`. `tehas status` shows the lane's free slots on the slice line.
`TEHAS_REFRESH_SLOTS=0` turns the lane off: refreshes then take the cycles'
slots, and no room is kept. With `TEHAS_REFRESH_CONFLICTS` off no room is kept
either. With it on but an image that cannot refresh, the room is still kept:
turn refreshes off on such a host.

Pause, the limit, the slots of the lane and the slice's room hold a refresh as
they hold any start; the limit of `tehas limit` counts refreshes and cycles
together. `TEHAS_MAX_AWAITING` does not: the pull request is open already,
and the start takes `tehas:awaiting-merge` off. A refresh goes after `tehas:urgent` and before
`tehas:next`. A refresh that waits for room counts in the demand of
`tehas status`.

**After a handoff.** The refresh takes a handed-off pull request, draft or not:
a handoff leaves `needs-human-attention` and `tehas:question` on the issue, and
neither keeps the refresh away. The refresh of a handed-off issue is a restart,
as a person's `tehas:ready` would be: the cycle takes `needs-human-attention`
off as it starts, the refresh's start takes `tehas:question` off, and how the
refresh ends labels the issue, `tehas:awaiting-merge` or a new handoff. The
handoff comment stays on the issue, and the pull request keeps the draft state
it had.

**While it runs** the pull request carries `tehas:refreshing`: do not merge or
push to it. The dispatcher applies it before it marks the issue, and a start
whose label change fails starts nothing. `tehasbot cycle` takes it off when the
cycle ends, whatever the outcome. The dispatcher takes it off a pull request
whose issue has no cycle running, and one applied by any account but the bot.
Both directions go through GitHub's REST issues endpoint, not `gh pr edit`,
which asks GitHub a GraphQL question the bot's token may not answer
(see [issue #120](https://github.com/tehas-ai/tehas/issues/120)); a 404 is
read as a label that is not there, so a mark already gone needs no retry.

**The record and the cycle.** The approval record has `how: "refresh"`, the
person's `by` and `at`, `rests_on`, `refreshes` (the count since that
approval), `refresh_of` (`pr`, `head_sha`, `base_sha`, `branch`) and one
`retry_context` entry, source `refresh`, that tehas writes itself:

> This pull request conflicts with `main`. Merge `main` into its branch,
> resolve the conflicts so the change still does what the issue asks, run the
> gate, and push to the same branch. Do no other work.

No comment text reaches the cycle this way. As with the millwright's note, it
reaches `retry-context.md` only where the instance gives a cycle images or
files. `tehas work` passes `--continue-pr` for a refresh, and refuses a refresh
with 78 on an image that does not take it. Nothing else about the container
changes. The outcome is read as any cycle's: a refresh that pushed goes back to
`tehas:awaiting-merge` with the same pull request, one that parks parks the
issue, and no refresh of it starts again on its own.

**What it does not do.** Failing checks and requested changes, pull requests
the bot did not open, from forks or closing several issues, and conflicts
between two open pull requests of the factory: each is resolved against the
branch when the other merges.

**To turn it on**, set `TEHAS_REFRESH_CONFLICTS=1` in `instance.env` and
enable the timer, which `install.sh` and `tehas update` install but do not
start:

```
systemctl --user enable --now tehas-refresh.timer
```

The first run is at once. `~/tehas/logs/refresh.log` says what it found.

**To turn it off**, set `TEHAS_REFRESH_CONFLICTS=0` (or remove it) in
`instance.env`: `tehasbot refresh` then exits at once and the dispatcher starts
no refresh. `systemctl --user disable --now tehas-refresh.timer` stops the
hourly run as well.

## Updating tehas

An update is a person's choice: nothing updates tehas on its own. Run
`tehas update --check` to see the version installed, the newest release and
the `Adopter action` of each release between, do what they ask, then run
`tehas update`. `tehas update --to <version>` goes back (see
[Rolling back tehas](/host#rolling-back-tehas)). An update that changes a unit
restarts the units that run, and that can end a running cycle: pause and let
the cycles finish first (see [Pause and resume](#pause-and-resume)). An update
that moves records into the state store refuses while a cycle or a planning turn
runs and records of the older tehas are there, and installs nothing: drain first
with `tehas drain --wait 3600`, let planning turns end, update, then
`tehas limit clear` (see [tehas drain](/tools#tehas-drain), and `install.sh` in
[the tools](/tools)). The update moves each old record file it imported to
`~/tehas/state/imported-<YYYYMMDD>/`; remove that directory once satisfied.
While a record file of the older tehas is still in `state/`, every poll stops
with exit 78 and logs `ERROR: records of an older tehas at <path>; run
install.sh again`: run `install.sh` again, and it imports what is left. How it works is in
[Updating tehas](/host#updating-tehas). A host installed from a checkout keeps
pulling it until it is moved: see
[Move a checkout host to releases](/host#move-a-checkout-host-to-releases).

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
