---
title: The Protolab
description: "The page of a prototype."
---

# The Protolab

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
[`tehas status --json`](/api#tehas-status--json) read, each ticket with `builtAt`, the
time of its page. The timer runs it
every five minutes; the log is `~/tehas/logs/protolab.log`.

The pass is the console's (`~/tehas/libexec/console protolab run`,
`console/src/protolab.rs`): the calls to GitHub, the check of who applied the
label, the downloads, the zips through `libexec/attach`, the pages and the
dashboard's file. It escapes what came from GitHub, puts together every figure
of a page, and reads the attachment links of a ticket, by the one regular
expression of `console/attach/src/link.rs`, the one the attachment checker and the
dispatcher read links by. The command beside it keeps the settings and
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

The label is judged by its GitHub id too. `tehas labels` pins
`tehas:prototype` in `~/tehas/state/labels` beside the granting labels (see
[label pins](/github-app#label-pins)), and each pass that finds tickets reads
the id of the label that now carries the name. When it is not the pinned one,
or the file has no pin for it, no ticket is at the stage: the pass logs
`refused: the label tehas:prototype ...` and removes the pages. When the label
cannot be read, nothing is built or removed. An account is its login and its
user id, as for the dispatcher: an application, or with `TEHAS_ISSUE_TYPES` a
type, from a listed login with another id counts as nobody's.

A ticket whose title was changed after the approval by an account outside the
list is not built again: the log says `its title was changed by '<account>'
since <approver> approved it, left as it was`, and a page it had keeps the
title it was built with. An allowed account applying the label (or the type)
again approves the title as it then is.

It fetches only links to GitHub's attachment storage, in text written by an
owner, member or collaborator of the repository, and only the types listed
under [Operating](/operating). A ticket gets 10 megabytes and 40 files. An
issue that has not changed since the last pass is not built again, so a quiet
pass fetches no files: one GitHub call for its own account, one for the list,
one for the label's id when the list is not empty, and one for every ticket at
the stage, its label events. With `TEHAS_ISSUE_TYPES` a ticket that changed
costs one more call, its label events, for the renames. When GitHub cannot be
reached, what is there stays. A pass that finds no console to write the pages
with says so and exits 78 before it calls GitHub.

A prototype is somebody's HTML and script. That is why the Protolab is a
server of its own on another port, and why every attached file is answered
with a sandbox policy: the browser gives it an origin with nothing in it. To
publish the Protolab outside the private network, give it a host name of its
own, never a path under the dashboard's, and set `TEHAS_PROTOLAB_URL`.

Without the timer and the container there is no Protolab, and the panel on the
factory floor says so.
