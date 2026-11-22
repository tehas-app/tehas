---
title: tehas plan
description: "One turn of a planning session."
---

# tehas plan

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
What the dispatcher writes of a plan goes through the console's mask
(`tehas_core::mask`) first: the issue's new title and body, the comment that
ends the session and the comment on an issue a refine replaced. Tokens,
passwords, private addresses and the host's secret values are replaced, and
the title is cut at 256 characters, the most GitHub takes. The sum a start is
bound to is taken of the masked text, which is the text the issue holds.
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
clone's git directory (plain files inside the clone only, none through a
link, at most 1 MB each and 1024 files and 32 MB together); `exit`, the
runner's exit status of the last turn; and with `TEHAS_WORK_IMAGES=on` the
issue's images. A turn that fails and leaves no record, its container killed
or never started, leaves the record an earlier turn kept, so a retry goes on
from it. Every later turn's workspace is at that commit and gets the
record back. Its clone, of `TEHAS_CLONE_DEPTH` commits, is deepened to that
commit when the branch has moved past the depth since. A later turn whose
commit is missing or gone starts at the tip of the branch and says so. The answer reaches the runner as
`.tehas/answer.md` in the workspace, which git ignores. The dispatcher keeps
the directory at `~/tehas/work/<name>-plan-<issue>.keep`, beside the turn's
workspace and out of the state directory, writes a later turn's answer there
as `answer.md` from the answer on record, and removes it when the session
ends. A first turn's `--brief` is a file in `~/tehas/tmp` until the turn ends;
the text is on the state store.

The issue a confirm files is in the runner's answer. The turn's comment names
it in one line, `Filed in this turn: <address>`, and the session on the state
store keeps it. pi-rukas files a session once: a second confirm files nothing more.

With `TEHAS_WORK_IMAGES=on` the first turn fetches the images the issue links,
by the rules of `tehas work`, with `tehas:plan` in place of `tehas:ready`,
and says on the issue what it fetched and whether the session was shown them.
Each turn's container gets a copy of them, read-only, at the path a cycle has
them, with `PI_ENSEMBLE_EXTRA_IMAGE_DIRS` and `PI_ENSEMBLE_PLAN_ISSUE`, the
issue's number. The copy is
`~/tehas/work/<name>-plan-<issue>.images`, removed after the turn. An image
that cannot be fetched starts no session: exit 4, said on the issue. With
`TEHAS_WORK_DESCRIBE=on` the first turn also writes `images.md`, as
`tehas work` does, with the issue's number in `X-Tehas-Issue` on each request,
and every turn's copy holds it. With `TEHAS_WORK_FILES=on`
the first turn fetches the text files too, by the rules of `tehas work`, into
the same directory.

With `TEHAS_WORK_IMAGES=off` and `TEHAS_WORK_FILES=off` a turn is given no
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
| 4    | With `TEHAS_WORK_IMAGES=on` or `TEHAS_WORK_FILES=on`: a first turn where an image or file the issue links could not be fetched, or the issue could not be read for them, or a later turn where one its reply links could not be fetched; nothing ran |
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
