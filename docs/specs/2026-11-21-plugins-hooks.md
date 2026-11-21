# Spec: plugins v1, the hook after work

**Version**: part of the plugin specification 1.0.0, `2026-11-21-plugins-1.0.0.md`.

**Status**: Draft 2026-11-21. Part of `2026-11-21-plugins.md`. Nothing is
built.

v1 has one hook point, `after:work`, and it reports only. It never holds an
issue, a slot, a lock or a label. A factory that wants a plugin to stop a
merge makes the plugin's status a required check in branch protection; that
is GitHub's gate, and the person's choice.

## When it runs

At the end of every cycle that leaves an open pull request of the bot's,
refreshes included, and on `tehas plugin hook <name> <pr>`.

The seam is in `console/cycle/src/supervise.rs`, after `apply::run` has done
the report: the label is set, the park comment posted, the issue's lock
released by the supervisor's normal end. The supervisor then writes one
request file per plugin with `hook:after:work` granted,
`~/tehas/plugins/<name>/hooks/<pr>.json` (the pull request, its head commit
as GitHub gave it, the issue, the base), and starts
`tehas-plugin-hook@<name>-<pr>.service` with `systemctl --user start --no-block`.
It waits for nothing.

No hook is asked for when:

- the cycle was stopped by a signal, refused at start (`tehas work` refused
  it before the runner ran), or parked with no pull request;
- the pull request is merged or closed;
- the plugin is refused, off, or not granted the hook.

A second request for the same plugin and pull request replaces the first. A
hook already running finishes, then the newer request runs.

## The run

A hook is a run of the plugin's image as the security part says, with kind
`hook`, `/state` read only, `/product` at the pull request's head, its
`timeout` from the manifest (60 to 1800 seconds), no `model` in v1, and
`/run/context.json` holding the pull request's number, base and head.

At most one hook of a plugin at a time, and at most two hooks in all at once.
They run in `tehas.slice` and count in admission by room. A hook waits for a
refresh of the same plugin to end, so it reads a whole `/state`.

When it ends, tehas asks GitHub for the pull request's head. A head that moved
while the hook ran makes the verdict stale: it is posted for the commit it
read, and a new request for the new head follows. If reading the head fails,
the verdict is posted for the head the hook read and no new request is made.

## The verdict

`/out/_tehas/verdict.json`, read through the tar reader:

```json
{
  "verdict": "note",
  "summary": "The change adds an endpoint with no sign-in check.",
  "findings": [
    {"file": "server/routes/refund.js", "line": 42, "text": "POST /refund answers without a session"}
  ]
}
```

| Field | Rule |
| --- | --- |
| `verdict` | `pass` or `note`. `fail` is v2 and is read as `note` |
| `summary` | One line, at most 140 characters |
| `findings` | At most 20. `file` a path in the product or absent, `line` 1 or more or absent, `text` at most 300 characters |

`api/plugin-verdict.schema.json` holds it, with samples. A verdict that breaks
the schema, a hook that fails or runs out of time: tehas posts the status
`error` with "the plugin's hook did not finish".

The verdict read is kept as `~/tehas/plugins/<name>/hooks/<pr>.verdict.json`
for the gate's findings page. A new hook on the pull request deletes the old
one when it starts. `tehasbot plugin tick` asks GitHub nothing, so it removes a
verdict when its head commit is an ancestor of the default branch in the
product's mirror (merged), or when the file is older than 30 days. After a
squash merge the head is not an ancestor, so that verdict stays up to 30 days,
behind the sign-in.

## What tehas posts

One commit status, and nothing else on GitHub. Not through the action
journal: the store has no status kind, and a status posted twice is harmless,
since GitHub keeps the newest per context. A failed post is retried once and
logged.

- Context `tehas/plugin/<name>`, on the head commit the hook read.
- State `success` for `pass` and for `note`, `error` when the hook did not
  finish. v1 never fails a status, so a required check fails only on `error`.
- Description: the summary through `q_clean`
  (`console/core/src/question.rs`), which disarms check boxes, `<!-- tehas:`
  markers and `@` mentions and masks secret-looking text, cut at 140
  characters.
- Target URL: `https://plugin-<name>.<domain>/_hooks/<pr>`, where the gate
  shows the findings behind the sign-in, as plain text in a page tehas makes.
  With no `view` granted, no target URL.
- It needs the App's "Commit statuses: Read and write", an adopter action.
  `tehas setup --new-app` asks for it; a cycle's narrowed token is unchanged.
  Without it, GitHub answers 403, the status is not written, and
  `tehas plugin --json` sets `status_unwritten`; the findings are still at the
  target address.

No comment, on purpose. A cycle's agents read the pull request with their own
token inside the container, where tehas cannot filter what they read, so a
plugin's text in a comment could steer an agent that can push. A status's
description is short and cleaned, and the findings stay behind the sign-in,
which also keeps them off a public repository.

## The reference hook

Floorplan's: the default branch's map from `/state`, the pull request's head
from `/product`, a rebuild with no model call, and the Health check's new
findings as a `note`: their kind, file and line, never a value. Nothing new:
`pass`.

## What v1 does not have

`gate:`, `before:` points, `plan` and `refresh` points, a `fail` verdict,
`model` in a hook, and hooks for a cycle with no pull request. See the v2
handover.
