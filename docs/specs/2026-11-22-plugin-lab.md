# The lab plugin

**Version**: plugin specification revision 1.4.0, 2026-11-22 (the number is
fixed when it merges, in `2026-11-21-plugins-1.0.0.md`). Built after
tehas 2.0.0, with the rest of plugins (2.1). Nothing here is built.

## Why

tehas 2.0.0 removes the Protolab from core (step T14 of
`docs/plans/2026-11-21-coding-standard-v2.md`). The owner's reasons: it was
envisioned and never pulled through, no factory uses it, and a view of this
kind belongs in a plugin. The owner asked to keep its plumbing for file
storage and serving, so that a plugin can do the same job.

This file says what that plugin does, what the platform adds for it, and
which plumbing T14 keeps for those additions. Other features that could
leave core are listed in `docs/plans/2026-11-22-unused-features.md`.

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
issue from a page. The lab makes a page from an issue. They share the gate
and the keep rule.

## What the platform adds

Each addition is an ask in the manifest and a grant in the instance.
`tehas plugin review` lists it, and a person writes the grant. Nothing on
this list is given by default.

### 1. Plugin labels

The manifest may declare labels:

```json
"labels": [{"name": "lab", "color": "5319e7", "description": "Show this issue's prototype in the lab"}]
```

- A label name is the plugin's name, or the plugin's name followed by `:`
  and `a-z` and `-`. A name starting `tehas:` is refused.
- `tehas labels` creates a declared label and pins it in `state/labels`, as
  it pins its own. If the product already has a label of that name that
  `tehas labels` did not make, it refuses with exit 78 and takes nothing
  over. A person renames or removes that label first.
- The dispatcher needs no change. It acts only on the labels it names
  (`label_pins::GRANTING`, `decide/flow.rs`), so a plugin label never
  starts or holds a cycle.

### 2. `issues:<label>`: an issue feed

tehas reads the issues and hands them to the run. The plugin never reads
GitHub.

- Before a run, the host reads the open issues that carry the label through
  the forge port. It keeps an issue only when an account in
  `TEHAS_ALLOWED_LABELERS` applied the label, and that label is the pinned
  one. Hard rule 5 is kept on the host, by the code that does it for the
  dispatcher.
- For each issue it writes the number, the title, the URL, who applied the
  label and when, and the texts that give files to `/run/issues.json`,
  checked against `api/plugin-issues.schema.json`. Those texts are the body
  and comments written by an allowed account that nobody off the list
  edited last, without quoted lines. A title changed since by someone off
  the list is held back, as the Protolab holds it.
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
plugin's list) comes later as an ask of its own. Its rule is kept as library
code in T14.

### 4. `pages`: attached files, served as sandboxed pages

The plugin writes `/out/pages/<issue>/`. The trees count against `keep:`.

**Why not `/_keep`.** `/_keep` serves kept files in the plugin's own
origin, for the plugin's own view. The lab's pages hold HTML and scripts
that a person attached and the plugin did not write. They must not run in
the plugin's origin, where they could read the plugin's view and act as the
person. So they run sandboxed, in an opaque origin.

**The page token.** A sandboxed page in an opaque origin sends no cookie
with the requests for its own files: the gate's cookie is `SameSite=Lax`
and those requests are cross-site. So the files are served under a token
instead of the cookie:

- A signed-in request for `/_page/<issue>/`, under the gate's usual rules,
  answers `303` to `/_page/<issue>/<token>/`, with a new token: 32 random
  bytes, base64url, for that issue's tree. Only a signed-in person gets a
  token.
- A token ends at the sign-in's 12 hours or at the run after the one that
  wrote the tree it was made for, whichever comes first. It is not bound to
  a person, so it must not outlive a sign-in, even when a quiet label means
  no run comes for weeks. Opening the page again makes a new token.
- A request under `/_page/<issue>/<token>/` needs no cookie, and the gate
  reads none. It must be a `GET` or `HEAD` of a file in that issue's tree
  with a token that is valid now. Anything else is 404, with no difference
  between a wrong token and a missing file.
- The web part's `Sec-Fetch` rules apply to `/_page/<issue>/` as written.
  Under a token, a page's own requests are `cross-site`, so any
  `Sec-Fetch-Site` is taken and the token is the proof. A missing
  `Sec-Fetch-Site` is refused. `Sec-Fetch-Dest: document` is refused, so a
  page opens only in a frame, and the frame's ancestor is fixed by the
  policy below.

**The policy** on every answer under `/_page`:

- `Content-Security-Policy: sandbox allow-scripts; default-src 'self';
  connect-src 'none'; form-action 'none'; frame-ancestors <the plugin's
  origin>`. No `allow-same-origin`, `allow-forms`, `allow-popups`,
  `allow-top-navigation` or `allow-modals`.
- `Referrer-Policy: no-referrer`, so the token does not leave in a
  `Referer`.
- The page the lab writes around the files has no script and nothing from
  elsewhere: `script-src 'none'` on top of the above.

The v3 frame (`/_frame`) and `/_keep` stay as they are. A host name for each
page (`lab-<n>.<domain>` today) is not part of this: an opaque origin keeps
pages apart already. The edge's matcher for such names is kept in T14 in
case a later page type needs real origins.

### 5. `list`: a list on the dashboard

The dashboard carries no plugin text. A plugin with this ask writes
`/out/_tehas/list.json`, checked against `api/plugin-list.schema.json`: up
to 200 items, each an issue number and a page path. The path must be
`/_page/<issue>/` for that issue and nothing else.

The host builds `live/plugin-<name>.json` itself. It takes each item's
title and URL from its own feed (the held title), and drops an item whose
issue is not in the feed. `tehas status --json` gets `plugins.<name>.list`,
in place of the `prototypes` that 2.0 removes. The game view and the
business view read the lists of the plugins in `live/plugins.json`, not a
fixed file.

## The lab plugin

Repository `tehas-app/tehas-plugin-lab`, Apache-2.0.

The manifest asks, with no values:

```json
{
  "plugin": 1,
  "name": "lab",
  "labels": [{"name": "lab", "color": "5319e7", "description": "Show this issue's prototype in the lab"}],
  "asks": ["view", "menu", "issues", "files", "pages", "list", "keep"]
}
```

The instance grants, with the values:

```
TEHAS_PLUGIN_GRANTS_LAB="<commit> view menu issues:lab files:10 pages list keep:200"
```

On a run it:

1. reads `/run/issues.json` and `/given/<issue>/manifest.json`
2. for each issue, writes `/out/pages/<issue>/` with the files and an
   `index.html` that holds the title, a link to the issue, the images, the
   videos, and links to the HTML files and the unpacked folders
3. removes the tree of an issue that is no longer in the feed
4. writes the view (the list of pages) and `/out/_tehas/list.json`

It needs no model and no `product`. Its tests cover the page writer and the
list against fixed feeds.

## What T14 keeps, and where it goes

Agreed with the v2 session.

| Kept in T14 | Used by |
| --- | --- |
| `console/attach`, all of it | `files:` (3) |
| The host-side reading of labelled issues, the applier check, the title hold, and which texts give files, as "the issue feed" | `issues:` (2) |
| The page writer and its escaping, as a small crate of its own | The lab's writer (step 2 of the run). It vendors the crate |
| The edge's sandboxed static serving and its policy, under neutral names | `pages` (4), moved into the gate |
| The edge's per-item host matcher | Nothing yet; kept for later |
| The zip-by-address rule (hosts, redirects, sha256) | A later ask for zips by address |

## Changes to 1.0.0

| Where | Change | Why |
| --- | --- | --- |
| v1, guarantee 8 (writes only through tehas) | Kept, and its pair for reads: under `issues:`, tehas reads issues for a plugin and hands them over. The plugin still never reaches GitHub | The applier check stays on the host |
| v1, guarantee 9 (removable) | Page trees are kept files, and go with the plugin under the v3 keep rule. So do its labels' pins; the labels stay on GitHub for a person to remove | Nothing outlives the plugin on the host |
| v1, the manifest | `labels`, and the asks `issues`, `files`, `pages`, `list` | |
| Security part, the container | Two read-only mounts: `/run/issues.json` under `issues:`, `/given/` under `files:`. Pinned in `sandbox.rs` and the `test/fixtures/argv/plugin-*` lists | A security change |
| Web part, the gate | `/_page/<issue>/` and `/_page/<issue>/<token>/` beside `/_frame` and `/_keep`, with their policy and `Sec-Fetch` rules above | Attached pages run in an opaque origin |
| v2 menu part | The dashboard shows a plugin's list from the host's own feed; the plugin gives only numbers and paths | No plugin text on the dashboard's origin |
| `tehas labels` | Creates and pins declared labels; exit 78 on a name it did not make | A plugin takes over no existing label |
| `tehas status --json` | `plugins.<name>.list` | It replaces `prototypes` |
| Settings | None new. A grant is a `TEHAS_PLUGIN_GRANTS_<NAME>` line | |

## Work

After plugins are on in a release (2.1). Each is a pull request; a security
change says so.

1. **Manifest and labels.** `labels`, the four asks refused by name until
   each is enforced, `tehas labels` for declared labels and its refusal.
2. **The issue feed.** `issues:`, the host read, `/run/issues.json` and its
   schema, the tick's refresh on a changed feed. Security change: a new
   mount, `sandbox.rs` and the argument lists.
3. **The attachments.** `files:`, the fetch through `console/attach`, the
   cache, `/given/`. Security change: a new mount.
4. **Pages.** `pages`, `/_page` and its token, its policy and `Sec-Fetch`
   rules in the gate. Security change.
5. **The list.** `list`, its schema, `live/plugin-<name>.json`, status, the
   game and business views.
6. **The plugin.** `tehas-app/tehas-plugin-lab`: the writer, the view, its
   tests.
7. **The manual.** `host/docs/plugins.md`, `api.md`, `features.md` as
   building, the changelog fragment.

## Open points

- Whether the lab's pages may be public, as the Protolab's were. This spec
  says no: every plugin page is behind the sign-in. A public page would be
  an ask of its own, reviewed on its own.
- The lab could hand a ticket's files to a cycle, as the page ticket does.
  This spec leaves that out, because images and files already reach a cycle
  from the issue text.
