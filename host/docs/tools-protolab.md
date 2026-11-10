---
title: tehasbot protolab and tehasbot improve
description: "The Protolab's pages for prototypes, and the loop that files issues about its own cycles."
---

## tehasbot protolab

```
tehasbot protolab       # one pass; a line per ticket built or removed
systemctl --user enable --now tehas-protolab.timer
. ~/tehas/env.sh && docker compose -f ~/tehas/edge/router.compose.yaml up -d
```

Builds the Protolab in `~/tehas/protolab`: a page for every open issue
labelled `tehas:prototype` by an account in `TEHAS_ALLOWED_LABELERS`, with the
files attached to it, and the list
`~/tehas/live/protolab.json` that the dashboard and `prototypes` of
[`tehas status --json`](/api#tehas-status---json) read, each ticket with `builtAt`, the
time of its page. The timer runs it
every five minutes; the log is `~/tehas/logs/protolab.log`.

The pass is the console's (`~/tehas/libexec/console protolab run`,
`console/src/protolab.rs`): the calls to GitHub, the check of who applied the
label, the downloads, the zips through `libexec/attach`, the pages and the
dashboard's file. It escapes what came from GitHub, puts together every figure
of a page, and reads the attachment links of a ticket, by the one regular
expression of `console/attach/src/given.rs`. The command beside it keeps the settings and
the GitHub App's token, and refuses with exit 78 when no console stands beside
it. Each call of gh is held to 30 seconds. A console with other bytes builds
every page once more.

A host updated from the version whose command built the pages in bash: a pass
of the old command that is running when the new console lands logs that its
tickets could not be read, leaves `protolab/index.html` empty and exits 1. The
next pass of the timer writes the page again, within five minutes.

The container is the edge router, on port 8081. It answers for the Protolab
and for the [previews](/tools-clean#tehasbot-preview), and picks by the name it is asked
for: `lab-<issue>.` is that ticket's page, `uat-<pr>.` is the preview of that
pull request, and any other name is the Protolab by path, the list at `/` and
a ticket at `/<issue>/`. It reads its mounts, has a read-only root and no way
to write to the host. With `TEHAS_DOMAIN` set, the list links each ticket at
`lab-<issue>.<domain>`, so every ticket is an origin of its own to a browser.
[The host](/host) has the table of what it answers.

A ticket is at the prototype stage only while its most recent `tehas:prototype`
labelling is by an account in `TEHAS_ALLOWED_LABELERS`, read the same way the
dispatcher reads `tehas:ready`. A label applied by anyone else, the bot
included, counts as no label at all: the ticket gets no page, a page it had is
removed, and the log names the issue and the account. Where a ticket's events
cannot be read the pass builds nothing for that ticket and removes nothing: a
page it had stays as it was. The pass reads its own login before it reads,
builds or removes anything of a ticket, and stops with exit 78 when
`TEHAS_ALLOWED_LABELERS` names it, as the dispatcher does. When GitHub does not
answer that question, the pass ends there with the same nothing built and
nothing removed.

It fetches only links to GitHub's attachment storage, in text written by an
owner, member or collaborator of the repository, and only the types listed
under [Operating](/operating). A ticket gets 10 megabytes and 40 files. An
issue that has not changed since the last pass is not built again, so a quiet
pass fetches no files: one GitHub call for its own account, one for the list,
and one for every ticket at the stage, its label events. When GitHub cannot be
reached, what is there stays. A pass that finds no console to write the pages
with says so and exits 78 before it calls GitHub.

A prototype is somebody's HTML and script. That is why the Protolab is a
server of its own on another port, and why every attached file is answered
with a sandbox policy: the browser gives it an origin with nothing in it. To
publish the Protolab outside the private network, give it a host name of its
own, never a path under the dashboard's, and set `TEHAS_PROTOLAB_URL`.

Without the timer and the container there is no Protolab, and the panel on the
factory floor says so.

## tehasbot improve

```
tehasbot improve        # one run; one line on standard output
systemctl --user enable --now tehas-improve.timer
```

One run of the improvement loop. It is off, and prints that it is, unless
`TEHAS_IMPROVE_REPO` is set. [Operating](/operating) says what it files and how
to read its line; the timer runs it every three hours, on the hour, and the log is
`~/tehas/logs/improve.log`. A [limit](/tools-control#tehas-limit) with planning off, or one
that expired or cannot be read, holds a run: it says `held by the limit`, exits
0, and the records wait for a later run.

When a cycle ends, `tehasbot cycle` leaves a record of it in
`~/tehas/tmp/improve/window/<issue>-<time>/`: how it ended, the stream without
its word by word updates, the console and the error output. The driver's slow
notices are kept beside them as `slow.jsonl`, only when the cycle wrote any:
one line per child that crossed the bound pi-rukas watches, with its step, its
role, its elapsed time, turns and tokens, and, when the runner reports them,
the command that repeated, how many times and with how many identical results.
Those lines are what a stuck cycle is read by when the console is gone: it is
written inside the container and swept with the workspace. The record keeps the
last 20 of them and never over 64 KiB. Its `cycle.json`
names the image the cycle's container started from (`image`, its id) and the
pi-rukas commit of that image's label (`rukas`), which `tehas work` reads as
the container starts and puts on the cycle's record in the state store. The
container is started from that id, not the tag, so a rebuild between the two
cannot start an image the record does not name. A rebuild while the cycle runs
does not change them. Without an image on record
the commit is the one the cycle noted at its start. A comment on an issue the loop
filed, when the issue is worked on again, names both. When the cycle was given
the issue's images or files, or given none though its issue linked some, the
record also holds what `tehas work` kept of them in
`~/tehas/state/cycles/`: `manifest.json`,
and `images.md` and `comments.json` when the cycle had them. Only this
cycle's, only plain files, at most 256 KB each, and never an image or a file
itself. A run fetches the open
issues and the ones closed in the last 30 days, takes the five oldest records
that wait, and starts the analyst. The rest stay for the next run. Then it
checks the one file the analyst leaves, files what passes and removes the
records it took.

The analyst has 60 minutes, and the prompt tells it so and asks it to write
`findings.json` early and again as it reads more. When it is stopped there
and the file passes the check, what the file holds is filed: the records that
a finding or a followup names are done, the others go back to the window. With
no file, or one that does not pass, every record goes back and the run exits 1.
Either way the next run takes half of what this one took, never below one; a
run that finishes in time doubles it again, up to five. The number is in
`~/tehas/state/improve.cap`.

The findings are about tehas and about pi-rukas, so the analyst is plain Pi
with one prompt (`~/tehas/lib/improve-prompt.md`) and no extension. What it
reads holds issue text and model output, which can be written to steer it, so:

| The analyst's container gets | It does not get |
| --- | --- |
| The records and the issue list, read-only | The bot's token, or any other variable of the host |
| One empty directory to write `findings.json` in | The product checkout, a workspace, Pi's sessions or transcripts |
| `models.json`, read-only, and the host aliases | pi-rukas, skills, context files, MCP servers |
| The tools read, grep, find, ls and write | A tool that runs a command or opens a connection |
| 1 CPU, 2 GiB, no swap, 1024 processes, 60 minutes, in `tehas.slice` | A cycle's limits: it is a batch job with a quota of its own, counted in the slice's room like any other container |

The host makes every GitHub call. A finding is filed only if it is the shape
asked for: a title of at most 100 characters, one of six categories, a body,
an excerpt as evidence, and cycles that were in this run. A filed issue's body
ends with a `## Done when` section naming the files the change touches, the
test file that proves it, and what can be observed afterwards; the section has
to be the last thing in the body and its heading and items have to be real
markdown, not text inside a code fence or an HTML comment. A finding whose body
has no such heading, or nothing under it, or anything a person can read after
it, is counted in `invalid=` and not filed. A title that matches an open issue is a duplicate. In the issue the
model's text has every mention broken and its evidence inside a fence it
cannot close. The label is `tehas:improvement` and no other.

Limits: 3 issues a run; no run files in a repository while
`TEHAS_IMPROVE_MAX_OPEN` (10 by default) open issues there carry the label. The
whole log of each cycle a finding names is gzipped and uploaded to the release
`tehas-improve-logs`, in pieces of at most 5 MB (`ticket-<issue>-<time>.log.gz`,
or `.log.1.gz`, `.log.2.gz`), masked first as the console masks a line. A log
that still holds something shaped like a token, or whose masked copy could not
be written, is not uploaded.

Exit 0 when the run finished, also with an analyst stopped at its bound that
left a good file, there was nothing to read, the open issues are at their
limit already, or the loop is off.
Exit 1 when the analyst or GitHub failed: the records are kept and the next run
reads them again. What a failed analyst printed is in
`~/tehas/logs/improve-analyst-<stamp>.log`, one file per failed run, named by
the time the run took the records. The run's line names it. Exit 64 on any
argument, 78 on invalid settings, without a console in `~/tehas/libexec`, or
when there are records to read and `tehas.slice` cannot be used: the records
are kept. A second run while one is going leaves it to that one. The unit gives
a run 90 minutes: the analyst's 60, one to kill it, and the uploads.

Each run also removes the per-cycle files and the kept analyst output in
`~/tehas/logs/`, and the records under `~/tehas/tmp/improve/`, older than seven
days, read or not.
