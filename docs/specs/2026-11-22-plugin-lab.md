# The lab plugin, and moving unused features out of core

**Status**: draft, 2026-11-22. A plugin specification revision; its number
is given when it merges, in `2026-11-21-plugins-1.0.0.md`. Built after
tehas 2.0.0, with the rest of plugins (2.1). Nothing here is built.

## Why

tehas 2.0.0 removes the Protolab from core (step T14 of
`docs/plans/2026-11-21-coding-standard-v2.md`). The owner's reasons: it was
envisioned and never pulled through, no factory uses it, and a view of this
kind belongs in a plugin. The owner asked to keep its plumbing for file
storage and serving, so that a plugin can do the same job.

This file says what that plugin does, what the platform must add for it, and
which plumbing T14 keeps for those additions. The last part lists other
features that could leave core in the same way.

## What the lab does

The lab shows the prototypes a team attaches to issues. For each open issue
that carries the lab's label, applied by an allowed account, it builds a
page with the issue's title, a link to the issue, and the files attached to
it: images, videos, HTML files and the contents of zips. It also builds a
list of those pages.

It is the Protolab's job, with three differences:

| | The Protolab | The lab |
| --- | --- | --- |
| Where it runs | A timer pass on the host, with a GitHub token | A plugin run: no network, no token |
| Who can see it | Anyone who reaches port 8081 or `lab.<domain>` | People signed in through the gate, like every plugin view |
| Its label | `tehas:prototype`, a core label | `lab`, declared by the plugin |

The lab is the opposite flow from the page ticket. The page ticket makes an
issue from a page. The lab makes a page from an issue. They share the
frame, the records and the kept file trees.

## What the platform adds

Each addition is a grant. `tehas plugin review` lists it, and a person
approves it. Nothing on this list is given by default.

### 1. Plugin labels

The manifest may declare labels:

```json
"labels": [{"name": "lab", "color": "5319e7", "description": "Show this issue's prototype in the lab"}]
```

`tehas labels` creates them and pins them as it pins its own. A label name
is the plugin's name, or starts with it followed by `:`. A plugin cannot
declare a `tehas:` label. The dispatcher lists plugin labels as not in the
flow, so a plugin label never starts or holds a cycle.

### 2. `issues:<label>`: an issue feed

tehas reads the issues and hands them to the run. The plugin never reads
GitHub.

- Before a run, the host reads the open issues that carry the label through
  the forge port. It keeps an issue only when an account in
  `TEHAS_ALLOWED_LABELERS` applied the label, and that label is the pinned
  one. Hard rule 5 is kept on the host, by the code that does it for the
  dispatcher.
- For each issue it writes the number, the title, the URL, who applied the
  label and when, and the texts that give files to `/run/issues.json`. Those
  texts are the body and comments written by an allowed account that nobody
  off the list edited last, without quoted lines. A title changed since by
  someone off the list is held back, as the Protolab holds it.
- The tick refreshes a plugin with this grant when the feed's hash changed
  since its last run, as well as when the default branch moved. That is one
  GitHub read a tick for each such plugin, through the forge port's runner
  and its budget.

### 3. `files:<MiB>`: the attachments of the feed

This needs `issues:`. The host fetches the files that the texts link, using
`console/attach`, and mounts them read-only at `/given/<issue>/` with its
manifest:

- the link rule (`attach/link.rs`)
- the fetch (`attach/fetch.rs`)
- the sniff of the first bytes (`attach/sniff.rs`)
- zips unpacked into folders of their own (`attach/zip.rs`, `attach/unpack.rs`)

The limits are the Protolab's: 10 MB and 40 files an issue. The grant's
number may lower that, never raise it. A file whose link did not change
since the last run is taken from the host's cache, not fetched again. The
run gets no token and no network.

Zip by address (a `Zip: <address> sha256:<hex>` line on a host of the
plugin's list) comes later as `files:<MiB>:<host>...`. Its rule is kept as
library code in T14.

### 4. `pages`: kept trees, served as sandboxed pages

The plugin writes `/out/pages/<issue>/`. The gate serves it at
`plugin-<name>.<domain>/_page/<issue>/...` with the policy the Protolab's
pages have today:

- `sandbox allow-scripts allow-forms`, without `allow-same-origin`, so a
  page runs in an opaque origin and cannot read the gate's cookie or another
  page
- `default-src 'self'` within that page's tree, with `connect-src 'none'`
- for the page that the lab generates around the files, no script and
  nothing from elsewhere

The trees count against `keep:`. The v3 frame (`/_frame`) stays as it is,
for a plugin's own view. `/_page` is for files a person attached, shown
whole.

A host name for each page (`lab-<n>.<domain>` today) is not part of this.
An opaque origin already keeps pages apart. The edge's matcher for such
names is kept in T14 in case a later page type needs real origins.

### 5. `list`: a list on the dashboard

A plugin with this grant writes `/out/_tehas/list.json`, checked against
`api/plugin-list.schema.json`: up to 200 items, each an issue number, a
title, a page path and a time. The host copies it to
`live/plugin-<name>.json`.

`tehas status --json` gets `plugins.<name>.list`, in place of today's
`prototypes`. The game view and the business view read the lists of the
plugins listed in `live/plugins.json`, not a fixed file.

## The lab plugin

Repository `tehas-app/tehas-plugin-lab`, Apache-2.0.

```json
{
  "plugin": 1,
  "name": "lab",
  "labels": [{"name": "lab", "color": "5319e7", "description": "Show this issue's prototype in the lab"}],
  "grants": ["view", "menu", "issues:lab", "files:10", "pages", "list", "keep:200"]
}
```

On a run it:

1. reads `/run/issues.json` and `/given/<issue>/manifest.json`
2. for each issue, writes `/out/pages/<issue>/` with the files and an
   `index.html` that holds the title, a link to the issue, the images, the
   videos, and links to the HTML files and the unpacked folders
3. removes the tree of an issue that is no longer in the feed
4. writes the view (the list of pages) and `/out/_tehas/list.json`

It needs no model and no `product`. Its tests cover the page writer and
the list against fixed feeds.

## What T14 keeps, and where it goes

Sent to the v2 session so that T14 names these for the additions above.

| Kept in T14 | Used by |
| --- | --- |
| `console/attach`, all of it | `files:` (3) |
| The host-side reading of labelled issues, the applier check, the title hold, and which texts give files (from `console/src/protolab.rs`) | `issues:` (2). Name it for the feed, not the Protolab |
| The page writer and its escaping (`console/src/protolab/build.rs`) | The lab plugin's own writer (step 2 of the run). Copy it into the plugin at 2.1, or keep it as a small crate the plugin vendors |
| The edge's sandboxed static serving and its CSP | `pages` (4), moved into the gate |
| The edge's per-item host matcher | Nothing yet; kept for later |
| The zip-by-address rule (hosts, redirects, sha256) | `files:` with hosts, later |

## Other features that could leave core

The same test: built, off on every factory, never or barely seen on a host.
The owner decides each row. "Plugin" means it moves to a plugin after 2.0,
with its plumbing kept. "Drop" means it goes in 2.0.

| Feature | Evidence | Proposal |
| --- | --- | --- |
| All hands label | Never on a host. It only shows in status | Drop |
| Spec mark (`Spec:` lines) | Tests only. Its one filer was `tehas prototype file`; the page ticket hands files through records instead | Drop |
| Panels (outside pages in the dock) | A development machine only | Plugin: it is a view in a frame |
| Business page | A development machine only | Plugin, over the live feeds |
| Energy use | Tested with fake files | Plugin: a panel and a timed fetch. Check the host's private settings first |
| Host panel (`TEHAS_TOP`) | Tests only | Plugin. `tehas top` stays as an operator command |
| Review loop (`tehasbot review`) | Never on a host | Plugin: a timed run with the model that files an issue. Needs `issue:draft` |
| Product wiki | Being built, step 2 in review | Plugin: a timed run with the model over the checkout, and a served page |
| Describe, text files, zips and comments for a cycle | No host has run them. Images stay in core | Plugin: a hook before a cycle starts that adds files. Needs a `before:work` hook |
| Outsource, with the model proxy and the contractor | Being built. A release refuses it | Owner's call. It is about 20k lines across 11 crates, too deep for a plugin |
| Millwright's grant | Never on a host | Owner's call. It is an authorisation path, so it stays in core or goes |

Kept in core, though not proven on a host: refreshes, read-only instances,
the egress limit, the lean driver, analytics and the operator commands.
They are part of the loop, the sandbox or how a person runs a factory.

## Open

- Whether the lab's pages may be public, as the Protolab's were. This spec
  says no: every plugin page is behind the sign-in. A public page would be
  a grant of its own, reviewed on its own.
- The lab could move a ticket's files to a cycle, as the page ticket does.
  This spec leaves that out, because images and files already reach a cycle
  from the issue text.
