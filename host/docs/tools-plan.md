---
title: tehas plan and tehasbot review
description: "One turn of a planning session, and one review of a branch."
---

# tehas plan and tehasbot review

## tehas plan

Runs one turn of a planning session, headless, in the sandbox image. The
dispatcher calls it. By hand it is useful to see what a turn does.

Run by hand it removes and remakes the workspace that a planning turn of the
same issue uses, and it does not take the dispatcher's lock, so do not run it
for an issue whose session the factory is working on.

`tehas plan` has no script: the console runs the turn
(`console/src/plan/start.rs`), after `console run` has loaded the settings. A
call that is no turn is refused with 64 before the settings are read.

```
tehas plan 12 --out /tmp/turn.json --keep /tmp/turn.keep --brief brief.md
tehas plan 12 --out /tmp/turn.json --keep /tmp/turn.keep --session <id> --answer answer.md [--comment <id>]
```

The dispatcher adds `--kind refine` to every turn of a `tehas:refine`
session.

The first form starts a session from a file with the issue's title and body.
The second goes on with the session and gives the file's content as the
operator's answer. Each turn is one run of pi-rukas's headless runner,
`pi-rukas-headless plan`, as the container's command. It exits when the
planner has questions (11), has drafted a plan to confirm (12), or thinks the
plan duplicates open work (10). After a drafted plan in a refine session, an
answer that ticked **File as a new issue** runs `plan --resume <id> --confirm`, which files the
plan as it was shown and nothing else. One that ticked **Update this issue**
runs no container: the turn reads the issue's title and body and hands the
drafted plan, kept in the `--keep` directory as `draft.json`, to the
dispatcher, which writes it onto the issue and ends the session. A plan with no
open points may need no such answer: the dispatcher then reads the issue and
`draft.json` itself and no turn runs, see
[A plan with no open points](/operating#a-plan-with-no-open-points).
The one gate on the draft's text is `made` (`console/core/src/plan.rs`), which
asks the text check of #1210 (`console/core/src/textcheck.rs`) of the draft's
own title and text: a title of nothing but a prefix (`EPIC: EPIC`), a word
said twice, a title that only names the issue it plans (`feat: Plan issue
#608`), the planner's brief as the plan, or a text with no line of the issue's
own in it is refused, and a refusal writes nothing: the issue keeps the title
and body a person gave it, and the comment of the turn says which rule the
draft failed. Both writers of a plan go through it, and
the dispatcher pre-checks the same function before it writes an `update` or
credits a start, so a draft that says nothing of the issue asks instead: the
console's `update` answer comes back with the question and no `update` key, and
a draft the check refuses is never marked `clear`.
Any other answer runs `--answer`. The first
turn runs `plan "<descriptor>" --start --context <brief>`, with
`--issue <n>` when the image lists that flag for `plan`, so the planner does not
take the issue for a duplicate of itself (pi-rukas v0.14.28 and later). A later
turn never passes it: the session keeps it. The descriptor is the issue's title,
and the console makes it (`descriptor` in `console/src/plan.rs`): the text after the `# ` of the brief's first line,
trimmed. A brief whose first line is `Title: <text>`, as a drafted plan writes
one, is named by `<text>` instead, so a half of a split is named for its own
work and not for the whole issue it was split from; a `Title:` lower in the body
is the plan's text and names nothing. With no title to read at all the
descriptor is `the work in .tehas/brief.md`. The issue's number is added, as
` (issue #<n>)`, only when the title has fewer than six words - pi-rukas asks
questions instead of planning below that, unless the descriptor carries a code
name - and only while the whole still fits 62 characters, the 72 pi-rukas keeps
of a title less the 10 of its longest prefix, `research: `. A long title is
therefore never cut, and never given the number. The descriptor is also the
title an **Update this issue** answer gives the issue, so a body that opens with
`Title:` names the issue it is the text of. The image must say `features.planTurns` in its `--capabilities`, or no turn
starts: exit 78. pi-rukas v0.14.0 is the first release that does.

`--keep` names a directory the session carries from one turn to the next. The
first form makes it anew. It holds the approved title and body, which every
turn's workspace gets as `.tehas/brief.md`, so a later turn never plans from
the issue as it is now; the commit the first turn's workspace was at;
`plan-sessions/`, pi-rukas's record of the session, which it keeps in the
clone's git directory (plain files only, at most 1 MB each); `exit`, the
runner's exit status of the last turn; and with `TEHAS_WORK_IMAGES=1` the
issue's images. Every later turn's workspace is at that commit and gets the
record back. A later turn whose commit is missing or gone starts at the tip of
the branch and says so. The answer reaches the runner as
`.tehas/answer.md` in the workspace, which git ignores. The dispatcher keeps
the directory at `~/tehas/work/<name>-plan-<issue>.keep`, beside the turn's
workspace and out of the state directory, writes a later turn's answer there
as `answer.md` from the answer on record, and removes it when the session
ends. A first turn's `--brief` is a file in `~/tehas/tmp` until the turn ends;
the text is on the state store.

The issue a confirm files is in the runner's answer. The turn's comment names
it in one line, `Filed in this turn: <address>`, and the session on the state
store keeps it. pi-rukas files a session once: a second confirm files nothing more.

With `TEHAS_WORK_IMAGES=1` the first turn fetches the images the issue links,
by the rules of `tehas work`, with `tehas:plan` in place of `tehas:ready`,
and says on the issue what it fetched and whether the session was shown them.
Each turn's container gets a copy of them, read-only, at the path a cycle has
them, with `PI_ENSEMBLE_EXTRA_IMAGE_DIRS` and `PI_ENSEMBLE_PLAN_ISSUE`, the
issue's number. The copy is
`~/tehas/work/<name>-plan-<issue>.images`, removed after the turn. An image
that cannot be fetched starts no session: exit 4, said on the issue. With
`TEHAS_WORK_DESCRIBE=1` the first turn also writes `images.md`, as
`tehas work` does, with the issue's number in `X-Tehas-Issue` on each request,
and every turn's copy holds it. With `TEHAS_WORK_FILES=1`
the first turn fetches the text files too, by the rules of `tehas work`, into
the same directory.

With `TEHAS_WORK_IMAGES=0` and `TEHAS_WORK_FILES=0` a turn is given no
attachments, and an instance line for `PI_ENSEMBLE_EXTRA_IMAGE_DIRS` is dropped
there too: the container is given no variable naming a directory it does not
have. A turn given nothing says nothing about what the issue links; that is
`tehas work`'s report of a cycle (see [tehas work](/tools-work#tehas-work)).

A later turn given `--comment <id>`, the id of the reply its answer came
from, fetches the images and files the answer links into that directory
before it starts: by the same rules and limits, counted with what is there,
numbered on from it, and with `source` `answer:<id>` in the manifest. A link
already there is not fetched again, and the new images are not described. The
dispatcher passes the id when the answer is a reply; the reply counts only
from an allowed account, so its links are approved as the label approves the
body's. The answer sent to the agent ends with the names of the new files. A
link left out is said on the issue. A link that cannot be fetched runs no
turn: exit 4, said on the issue, and the directory stays as it was; a reply
runs the turn again. On exit 0 the `--out` file holds the session id, what the
turn says and what it asks. For a drafted plan the text holds the plan's title
and spec, cut at 40000 characters, and the question lists its open points with
the planner's proposals and offers **Update this issue**. A refine session
also offers **File as a new issue**, since it exists to file one. Without
`--kind refine`, a plan the runner lists an empty list of open points for and
that was not cut adds `"clear": true`. That says only that no point is open; the
dispatcher decides whether it starts, and posts the question otherwise. The `console` beside the command shapes it, and
without one no turn starts: exit 78. Two or more
questions are put together as one numbered question with no options, answered
by a reply. On exit 2 it holds the session id alone when that is known, so the
dispatcher can run the turn again in the same session, and it is not written
when no session is known. When the runner says the session is gone (its exit
41), it holds the session id and `"lost": true`; the dispatcher then ends the
session and says on the issue to apply `tehas:plan` again.

| Exit | Meaning                                                              |
| ---- | -------------------------------------------------------------------- |
| 0    | The turn asked something, drafted a plan, or filed it                |
| 2    | The runner failed, refused the turn or ran out of time, the turn said and asked nothing, its session is gone, or its outcome could not be written down |
| 4    | With `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`: a first turn where an image or file the issue links could not be fetched, or the issue could not be read for them, or a later turn where one its reply links could not be fetched; nothing ran |
| 64   | A usage error, no bot token or no git identity; nothing ran          |
| 78   | The instance settings are invalid, `tehas.slice` cannot be used, or the image runs no planning turns; nothing ran |

The container is the one `tehas work` starts, under the name
`tehas-plan-<issue>`: the same image, the same limits in `tehas.slice`, the
same mounts, token and settings, from `console/core/src/sandbox.rs`, and no Docker socket,
SSH keys or `~/.config/tehas`. It works in a workspace of its own,
`~/tehas/work/<name>-plan-<issue>`, made fresh for the turn and removed after
it, and its cache and memory go in `<name>-plan-<issue>.state` beside it and
are removed with it. Its transcripts go to
`~/.pi/agent/ensemble-runs/plan-<issue>`. There is no dry-run push before a
turn. A confirm files the issue with the token. The runner's progress is kept
in `~/tehas/logs/plan-<issue>-<time>.jsonl` and its answer beside it as
`.json`. The limit is `TEHAS_PLAN_TIMEOUT`: past it the container is stopped
with `docker stop` and the turn exits 2. The session is on the state store
(`plan-show`). A container left under the turn's name, by
a turn whose client died, is removed before the turn starts.

## tehasbot review

Reviews what the default branch gained since the last reviewed commit, with
pi-rukas's lens reviewer running headless in the sandbox image. It is the
review step of the review loop: `tehas-review.timer` starts it once a week
where a person has enabled it, and it files its findings only where
`TEHAS_REVIEW_REPO` names a repository. With `TEHAS_REVIEW` other than `1` --
the default -- it exits 0 at once and calls nothing, so an enabled timer costs
such a host nothing.

```
tehasbot review
```

One run is one turn of `pi-rukas-headless lens-review --since <commit>
--json`, over a clone made fresh for the run and removed when it is over. The
commit to start from is read from `~/tehas/state/review.json`: the commit the
last review read. A record is trusted only when it holds a whole commit and
that commit is one the branch still reaches; with no record there, or one that
is unreadable, names no commit, or names a commit the branch no longer has, the
run starts at the commit 7 days back, or at the root when the branch is younger
than that, and says which of the two happened. A branch whose newest commit is
older than 7 days has nothing new in the window: the run says there is nothing
new and starts no container. The same answer comes when the record already
names the tip. A git that cannot walk back to the commit 7 days back
is said with what git said, and the run exits 2 and reviews nothing rather
than reading from the root.

What a review is for is the whole range, so a run that failed records nothing
and the next run reads the same commits again. Only the exits that mean the
reviewer answered move the record on.

| Runner exit | Meaning | Recorded |
| ----------- | ------- | -------- |
| 0  | APPROVED, or nothing to review | the commit |
| 10 | ISSUES_FOUND | the commit |
| 20 | CRITICAL | the commit |
| 30 | REVIEW_INCOMPLETE | nothing |
| 40 | ROUND_CAP_REACHED | nothing |
| any other | the runner failed or refused | nothing, and the command exits 2 |

On exit 0, 10 or 20 the record is
`{"commit": "<the head that was reviewed>", "at": "<UTC time>", "exit": <code>}`
and it replaces what was there. With any other status the file stays exactly as
it was, and is not made when the host had none. Before anything is recorded the
runner's document is read, as a cycle's result and a planning turn's answer are
read: its `exitCode` and the exit status of the container must say the same
thing about the range. Two answers that disagree record nothing, and both are
named in the line that says so.

| Exit | Meaning |
| ---- | ------- |
| 0 | the review answered with no findings |
| 10, 20 | findings, or serious findings: the status the runner gave |
| 2 | the runner failed, ran out of time, or its record could not be written down |
| 64 | an argument this command does not know, or no git identity for the clone; nothing ran |
| 78 | the instance settings are invalid, `tehas.slice` cannot be used, the console is not installed beside the command, or the image's runner has no `lens-review` verb in its `--capabilities`; nothing was cloned and nothing ran |

### What a review files

A run that exited 10 or 20 files its findings as one issue in
`TEHAS_REVIEW_REPO` for a person to triage; loading refuses a
`TEHAS_REVIEW_REPO` equal to `TEHAS_REPO`, the product. The filing is the
store's: `store review-file --repo <owner/name> --doc <document> --branch <b>
--commit <sha> [--ask <line>] [--private <name>]...`
(`console/store/src/review_issue.rs`) reads the document of the run, decides
what to file, and makes the calls of GitHub. The command asks for the filing
and says what the store answered; it puts no part of an issue together itself.
The rule of what the issue says is
`tehas_core::review` (`console/core/src/review.rs`): the findings of the six
lenses, `CRITICAL`, `HIGH` and `MEDIUM` one to a line and the rest counted, the
title naming the count, the branch and the commit the range starts at.

What comes out of a model is made safe on the way out by the two functions the
console's log of the same run goes through (`tehas_core::mask` and
`tehas_core::question::q_clean`): tokens, passwords, private addresses and the
host's own names (the same list the console is given, with `--private`) are
masked or hidden, a comment marker is escaped, a task box such as `- [ ]`
becomes the glyph `- ☐` (`☑` when ticked), and the body is cut at 60 000
characters. The line that asks for `tehas:refine`, the label that plans an
issue into parts of one cycle each
([Planning](/operating#planning)), is the command's own text and is put on
after the masking, so no finding can become it. The issue is labelled
`tehas:improvement` and nothing else -- never `tehas:ready`, since a finding
starts no cycle until a person says so.

While one of those issues is still open, the findings of a later run go into it
as a comment rather than beside it, so a range nobody has read yet stays one
issue. GitHub is asked for the open issue of the review, and a GitHub that will
not answer is asked nothing: the findings are then filed as new, because a
duplicate is read by a person and a lost finding is not.

A run that exited 0 files nothing, and a run whose container wrote no document
is a run that left no findings: the store renders its empty document as one and
asks GitHub nothing. A filing that GitHub refused is an answered filing: the
line says what did not happen, the record of the range is already written, so
the next run reads the next commits whatever GitHub did, and the command still
exits with the status the runner gave. A host with no `TEHAS_REVIEW_REPO` files
nothing and says so. The body GitHub is given is written to
`state/review.body`, mode 0600, while the calls are made, and taken away as the
verb ends.

The container is the one `tehas plan` starts a planning turn in: the same
image, the same limits in `tehas.slice`, the same mounts and variables from
`console/core/src/sandbox.rs`, and no Docker socket, SSH keys or `~/.config/tehas`. It
differs in its name, in what it runs, and in having no token: a review reads
code, and the container writes nothing to the tracker, so no `-e` carries
`GH_TOKEN`, and the directory of the app's token is not mounted. The names of a planning turn, which this run has none of, are
dropped the same way. It has no cid file, so a review left by a client that
died is not named for a later run to remove. The one call the run makes to
GitHub is the host's own, after the container ended, as the app: the issue, or the comment on the issue that is still
open.

The workspace is `~/tehas/work/<name>-review`, a clone made for every run and
removed when the run is over; inside the container it is at the path every
cycle has its own, `~/tehas/work/<name>`. Beside it is `<name>-review.state`,
the run's own copy of the code index and its own memory, removed with the
workspace. A run that fails after making the two removes them too. Its
transcripts go to `~/.pi/agent/ensemble-runs/review`. The
runner's one JSON document goes to `~/tehas/logs/review-<time>.json` and its
NDJSON progress beside it as `.jsonl`. Both are masked on the way there, as
every other log an agent can see is masked before a reader gets it: no other
reader reaches a file named `review-<time>`.

The shell command prepares the sandbox. The console beside it
(`~/tehas/libexec/console review`) reads the range, checks the variables
against the image's `--capabilities`, starts the container, bounds it, masks the
two files and writes the record. A host without the console reviews nothing:
78, and `run install.sh`.

One review at a time: the run holds `~/tehas/state/review.lock` for as long as
it goes, and a second one started beside it says so and starts nothing. A
container left under the run's name by a client that died is removed before the
run starts; the `docker rm`, `docker stop` and `docker kill` of these are each
bounded by `TEHAS_STOP_WAIT` (60 seconds), so a dockerd that stops answering is
reported rather than being the last thing the run does. So is the wait on the
`docker run` client: one that outlives its container is sent `TERM` after that
grace and `KILL` two seconds later, and the run says the docker client of the
review would not end.

The limit is `TEHAS_PLAN_TIMEOUT`, the bound of one headless turn: past it the
container is stopped with `docker stop` and the run exits 2. An image whose
runner has no `lens-review` verb reviews nothing, and that is said before
anything is cloned. A `PI_ENSEMBLE_*` name that the environment would carry
into the container is given only when the image's `--capabilities` says a host
may set it; one it does not name stops the run before anything is cloned, as it
does for a cycle.

`tehas-review.timer` starts `tehas-review.service` on Monday morning once a
week, `Persistent=true` so a host that was down when it was due runs it when it
comes back. `install.sh` and `tehas update` copy both units and restart the
timer where it was already running; enabling it is a person's action:

```
systemctl --user enable --now tehas-review.timer
```

The unit keeps its own lines in `~/tehas/logs/review-timer.log`, beside the
`review-<time>` files of the runs; `journalctl --user -u tehas-review` holds
what a run printed before it could open that file. `TimeoutStartSec` of the
service is 3 hours, above the 2 hours `TEHAS_PLAN_TIMEOUT` gives the container,
so systemd is the last bound and not the first. With `TEHAS_REVIEW` other than
`1` a run exits at once and all of this costs the host one short process a week.
