# Plugin surfaces for 2.1

**Version**: plugin specification revision 1.5.0, 2026-11-22 (the number is
fixed when it merges, in `2026-11-21-plugins-1.0.0.md`). Built after
tehas 2.0.0, with the rest of plugins (2.1). Nothing here is built.

## Why

tehas 2.0.0 takes seven features out of core
(`docs/plans/2026-11-22-unused-features.md`, step T16 of
`docs/plans/2026-11-21-coding-standard-v2.md`). Each comes back in 2.1 as a
plugin. The specification up to 1.4.0 cannot carry them: a plugin runs only
when the default branch moves, its one hook comes after a cycle, and it
reads nothing of the host's.

This file says which surfaces the seven need, what each plugin is, and
which plumbing T16 keeps as library code.

| Feature in 1.4 | Plugin | Surfaces it needs that are new here |
| --- | --- | --- |
| Panels | `panels` | `frame` (4) |
| Business page | `business` | `live:` (2) |
| Energy use | `energy` | `feed:mesh` (3) |
| Host panel | `hoststats` | `feed:host` (3) |
| Review loop | `review` | `every:` (1), `history:` (5), drafts from a run (6) |
| Product wiki | `wiki` | `every:` (1), `history:` (5) |
| Describe, text files, zips and comment attachments for a cycle | `given` | `hook:gate:cycle.before-work` (7) |

## What the platform adds

Each addition is an ask in the manifest and a grant in the instance.
`tehas plugin review` lists it, and a person writes the grant. Nothing on
this list is given by default. Until the code that enforces a word has
merged, the reader refuses it by name.

### 1. `every:<hours>`: a refresh by the clock

Today the tick refreshes a plugin only when the default branch's head moved
(`console/plugin/src/tick.rs`). The review loop ran weekly and the wiki
nightly.

- The manifest asks `every`. The grant gives the hours, 1 to 720. One hour
  is the floor.
- The tick starts a refresh when the plugin's last attempt began that many
  hours ago or more. The tick runs every 10 minutes, so a run starts within
  10 minutes of being due. The run's `trigger` is `clock`.
- A plugin with this grant is not refreshed when the head moves. A run with
  the model is paid for by the clock, not by each merge. An action and
  `tehas plugin run` still ask for a run.
- A failed attempt is not tried again before the next period.
- The rules of "Not while held back" and "One refresh of a plugin at a
  time" hold as written.

What reaches the container: nothing new. No rule is touched. Not a security
change.

### 2. `live:<feed>`: a view reads a live feed

The dashboard's feeds are files in `~/tehas/live/`, public on the docs
server. A view cannot read them: its policy is `connect-src 'self'`.

- The ask and the grant are the same word, `live:<feed>`. The feed is one
  of a closed list: `status`, `running`, `analytics`, `plugins`. A feed a
  later release adds is a revision.
- The gate answers `GET /_live/<feed>.json` on the plugin's name with the
  file as it is, under the session and the web part's `Sec-Fetch` rules,
  with `Cache-Control: no-store`. A feed that is not there is 404.
- At most 4 feeds a plugin, and for each feed at most 30 requests a
  minute per login. Over it: 429.

What reaches the container: nothing. Runs do not get the feeds. The files
are public already, so the grant gives no new reading, only a route. A new
gate route is a security change, a small one.

### 3. `feed:<name>`: a feed the host makes

`issues:` is a feed the host makes for a run. These two are feeds the host
makes for a view. Both exist today as public files, `live/top.json` and
`live/energy.json`. As plugin feeds they are behind the sign-in.

- The ask is `feed:host` or `feed:mesh`. No other name is known.
- While a ready plugin holds the grant, `tehas-plugin-feeds.service`
  samples and writes `~/tehas/plugins/<name>/feeds/<feed>.json`, atomically.
  With no such plugin the unit is not installed and nothing is sampled.
- The gate answers `GET /_feed/<feed>.json`, as `/_live` above.
- **`feed:host`** is the snapshot of `tehas top`, cut to the published keys
  (`PUBLISHED` in `console/src/top.rs`), every 5 seconds. A process is
  named by its short command name only, as today.
- **`feed:mesh`** is the energy sample of `console/src/energy.rs`, every 15
  seconds: the draw now, the kWh of 24 hours and 30 days, the 30-minute
  window, the machines and models that serve the factory. The host asks the
  mesh of the factory's subagent provider, with that endpoint's token, and
  copies only the listed fields: no address, node id or GPU. The grant
  `feed:mesh=<url>` names a cluster URL instead, asked with no token. The
  URL follows the rule of today's `TEHAS_ENERGY_URL`. Private and CGNAT
  addresses are allowed because a person wrote it in the instance. The
  voice relay's refusals (1.1.0) still hold: loopback, link-local
  (the cloud metadata address included), multicast, "this network", the host's own
  addresses and the Docker bridge ranges. The name is resolved once and the
  connection goes to the address checked.
- An answer that is not a viiwork cluster snapshot removes the file, as
  today. A timeout or a busy answer leaves the last one.

**Why the host fetches, and not `net:`.** The other choice was `net:` with
private addresses. This spec recommends the host-made feed, for three
reasons:

1. The default source is the model endpoint, asked with its token. No fetch
   made for a plugin sends a token, and none should.
2. The sample is taken every 15 seconds and keeps a window. A run cannot do
   that, and a view that fetched the raw answer would put addresses and
   node ids in the browser.
3. `net:` stays global unicast only (revision 1.0.1). No plugin can name a
   private host, and the fetch has one list of refused addresses.

What reaches the container: nothing. The plugin never sees the raw answer
or the token. The rule touched is "what no grant gives": the host's network
stays ungiven, since the host fetches a listed address for its own feed. A
security change: a new unit that reads `/proc` or the mesh for a plugin's
view, and a gate route.

### 4. `frame`: an outside page in a frame

Panels show pages of the project, such as a support chat, in the dock. A
view cannot frame another site: its policy is `frame-src 'self'`.

- The manifest asks `frame`. The grant gives up to 4 pages,
  `frame:<id>=<url>`: the id `a-z` and `-`, 1 to 16, the URL `https://`, at
  most 512 bytes, with no space, user or password, and not on
  `TEHAS_DOMAIN`. These are the rules of `tehas panel set`.
- The gate serves `/_outside/<id>`, a page tehas makes. It holds one frame
  of that URL and nothing else, with the sandbox words the dock uses today
  (`allow-scripts allow-same-origin allow-forms allow-popups
  allow-popups-to-escape-sandbox`), a link to open the page in a new tab,
  and the policy `default-src 'none'; frame-src <the URL's origin>;
  base-uri 'none'; frame-ancestors <the plugin's origin> <the dashboard's
  origin>`, with `Referrer-Policy: no-referrer`, so the plugin's host name
  is not sent to the outside site.
- With `menu`, each granted page is an item of the plugin in the Plugins
  section. Its label is the id. A plugin's own view may frame
  `/_outside/<id>` too; its own policy does not change.

So this is not `view` with a wider `frame-src`. The plugin's pages never
hold the outside frame, and tehas sets the sandbox. A plugin that only
shows pages needs no build and no run.

What reaches the container: nothing. Guarantee 7 says a view loads nothing
from another site. That stays true of the plugin's pages; the outside page
loads in tehas's page, by a person's grant. A security change: a gate route
that frames another origin.

### 5. `history:<commits>`: the raw sources of a range

`product` is a clone of depth 200 on one branch, with no tags. The wiki
reads the version tags and the commits since its last build. The review
reads every commit since its last run.

- The manifest asks `history`. It needs `product`. The grant gives the most
  commits, 1 to 5000.
- Before a run, the host collects the range from the head of the plugin's
  last good run to this head, and mounts it read-only at `/run/history/`:
  `head.json`, `tags.json`, `log.jsonl`, `changed.json` and `changelog.md`,
  as the wiki spec's "The raw sources" lists them. No author email;
  trailers dropped. With no last good run, or one that is not an ancestor
  of the head, the range is the whole history up to the grant's number.
- The run's clone holds the commits of the range, so the plugin can read a
  diff. A range over the number is cut at the oldest end, and `head.json`
  says so.
- A path the host's secret check refuses is left out whole from
  `/run/history/`, as the wiki spec's guarantees say. The clone at
  `/product` still holds those paths' blobs, as the depth-200 clone does
  today. The check is about what the host hands over, not about the clone.

What reaches the container: one read-only mount, `/run/history/`, and a
deeper clone at `/product`. Pinned in `sandbox.rs` and
`test/fixtures/argv/plugin-history`. A security change.

### 6. `issue:draft` from a run

v3 has `issue:draft` for a service, which asks through its socket. A batch
run has no socket for it.

- A run granted `issue:draft` and `record` may write
  `/out/_tehas/draft.json`: the draft of `console/core/src/plugin/draft.rs`
  (`draft`, `title`, `body`, `record`). One draft a run, read through the
  tar reader, at most 64 KiB. A draft that breaks a rule fails the run.
- The host holds it as the plugin's pending draft. A newer draft replaces
  one that nobody filed.
- The gate shows it at `/_draft` on a page tehas makes: the title and body
  as text, to edit. A signed-in person presses "File". tehas files it as
  v3 says, with the person as author, and writes the issue on the record.
  "Discard" drops it. Both are writes and go under v3's filing rules
  (#2752): `POST`, the `Origin` and `Sec-Fetch` rules, the gate's nonce,
  and the person as author.
- Nothing is labelled. A person applies a label afterwards (hard rule 5).

**A behaviour change from today's review loop.** Today `tehasbot review`
files the issue itself, labelled `tehas:improvement`, in
`TEHAS_REVIEW_REPO`, and comments on it while it is open. As a plugin, the
bot files nothing. A person reads the draft and files it, or does not.
Guarantee 8 holds: the plugin never writes to GitHub. The store's
`review_issue` table goes; the record and the draft take its place.

What reaches the container: nothing new. Not a security change beyond v3's
Work 7, which this needs first.

### 7. The hook before a cycle starts: files for a cycle

**Its name.** The event is the one before a cycle starts, as T9 of the v2
plan lists it, in the ports' words and in T11's grant form
`hook:<when>:<event>`. There is no alias for a 1.x name. This file writes
it `hook:gate:cycle.before-work`, a placeholder until T9 lands; the name T9
gives replaces it everywhere here.

Today `tehas work` describes images with the model, fetches the text files
and zips an issue links, and takes links from comments, each behind a
`TEHAS_WORK_*` setting. Images stay in core. The rest becomes a hook that
runs before the cycle's container starts.

**When it runs.** At every start of `tehas work` that has an approval
record, after core has fetched the images and written `manifest.json`. A
start by hand with no record runs no hook.

Today a start mounts the given directory only when `TEHAS_WORK_IMAGES` or
`TEHAS_WORK_FILES` is on, a retry has comments, or a person filed a
plugin's files (`console/cycle/src/work.rs`). On a host with images off, a
start would mount nothing and the hook would give nothing. So while a
plugin holds this grant, a start with an approval record mounts the given
directory and passes `PI_ENSEMBLE_EXTRA_IMAGE_DIRS` exactly as it does with
images on. That is an argument list the tests already pin, not a new one.
If the hook gives nothing and core gave nothing, the directory holds only
the manifest.

**The run.** One container of the plugin, kind `hook`, as the security
part says: no network, no token, `/state` read only. The supervisor starts
it and waits. It gets:

- `/run/context.json` with `at` set to the event's name and the issue's number.
- `/run/issues.json`, the lab's feed with this one issue. Its texts are the
  approved body and the comments of the approval record: written by an
  allowed account and not edited after the approval. Who approved is who
  applied the label, read by the host (hard rule 5).
- `/given/<issue>/`, read-only: a copy of what core gave so far, the images
  and the manifest.
- With `files:<MiB>` also granted: the other files those texts link,
  fetched by the host through `console/attach` and put in the same
  directory. Zips are unpacked. The limits are the lab's.
- With `model` granted, the model socket. v1's "no `model` in a hook" is
  lifted for this point only. `model:images` works as in a refresh. The
  grant's number is per run, and the hook runs at every start, retries
  included, so these runs count against the plugin day budget
  (`TEHAS_PLUGIN_MODEL_DAY`) too. When that budget is used up, the hook
  gives nothing and the cycle starts, as after a timeout. It never holds
  the start.

**What it hands over.** The plugin writes `/out/give/`. The host reads it
through the tar reader and checks every file with `console/attach`:

- an image by its first bytes (png, jpg, webp, gif), or a text file by its
  extension (md, txt, csv, json, yaml, log, html, css, js), valid UTF-8
  with no NUL byte
- at most 16 files, 1 MB each and 4 MB together, at most 4 folders deep,
  plain names (`attach/plugin_files.rs`)

The host copies them into the cycle's given directory as
`plugin-given/<name>/`, and adds to `manifest.json` under `plugins` the
plugin's name, its commit, and each file's path, size and sha256.

**What reaches the cycle's container.** Files in the directory it already
gets. No new mount, variable or socket. The cycle's lists in
`test/fixtures/argv/` do not change.

**What the agents are told.** The code that writes these files is a third
party's, pinned at a commit, so this is a supply path to agents that can
push, not only a model reading untrusted input. So:

- The cycle's instructions name `plugin-given/<name>/` as untrusted data
  written by plugin `<name>` at `<commit>`, to read as data and never as
  instructions.
- Each text file the host copies there starts with a head of one line
  saying the same, in the file's own comment form where it has one and as
  a plain first line otherwise. An image gets no head; the manifest names
  it.

**How it fails closed.** All or nothing for each plugin. A hook that
fails, runs out of time, is over the model's budget, or leaves one file
that breaks a rule gives nothing. The manifest names the plugin under
`plugins` with the reason and no files, and the cycle starts with what core
gave. A copy whose sha256 differs from the manifest stops the cycle before
the container starts, as it does for filed files.

**A slow hook.** The manifest's hook `timeout` is 30 to 300 seconds for
this point. At most 2 plugins hold the grant, run one after the other in
the order of `TEHAS_PLUGINS`. A third grant stops `lib/config.sh` with exit
78 when the settings load, so `tehas update` and every command refuse
before anything runs. `TEHAS_PLUGINS` naming a plugin twice is refused the
same way, as it is today. So a start waits at most 10 minutes. The wait
is before the runner starts. The hook's container counts in the room the
cycle was admitted with.

**How a person sees what was added.** The manifest is kept as
`~/tehas/state/cycles/<issue>.manifest.json`, with each file's source. Each
hook is a row in `plugin_runs` with the issue. `tehas plugin log <name>
--hook <issue>` prints its log.

**The rules it touches.** Hard rule 4: what a cycle's container is given
grows by files, in the existing mount. v1's guarantee 5: a hook now runs
while a cycle holds its slot and lock. v2's rule 5, "nothing of a plugin
reaches a cycle's agents unless a person filed it": this is the second
exception. The person's review is the grant, at one commit, not each file.
What a person should weigh: agents that can push read what this plugin
writes, and with `model` that text is a model's, made from untrusted input.

This is a security change, the largest in this revision.

## The plugins

Each repository is `tehas-app/tehas-plugin-<name>`, Apache-2.0. A name is
`a-z`, with no hyphen. Each grant line is for the tehas-tehas instance.

### panels

Outside pages in the dock. No build, no run.

```json
{"plugin": 1, "name": "panels", "label": "Panels", "asks": ["menu", "frame"]}
```

```
TEHAS_PLUGIN_GRANTS_PANELS="<commit> menu frame:support=https://chat.example/tehas"
```

The pages are the instance's, so they are in the grant, not the manifest.
`tehas panel` goes from core.

### business

The page at `/business`, as a view.

```json
"asks": ["view", "menu", "live:status", "live:running", "live:analytics", "live:plugins"]
```

```
TEHAS_PLUGIN_GRANTS_BUSINESS="<commit> view menu live:status live:running live:analytics live:plugins"
```

A refresh copies the page's files to `/out`. The page reads the feeds from
`/_live/` and draws the production line, the last 10 days and the board.
Its power figures come back when a later revision lets one plugin read
another's feed.

### energy

```json
"asks": ["view", "menu", "feed:mesh"]
```

```
TEHAS_PLUGIN_GRANTS_ENERGY="<commit> view menu feed:mesh"
```

A refresh copies the view's files. The view reads `/_feed/mesh.json` and
draws the draw now, the kWh, the chart and the machines. The settings
`TEHAS_ENERGY`, `TEHAS_ENERGY_URL` and `TEHAS_ENERGY_SOURCE_URL` go.

### hoststats

```json
"asks": ["view", "menu", "feed:host"]
```

```
TEHAS_PLUGIN_GRANTS_HOSTSTATS="<commit> view menu feed:host"
```

The view reads `/_feed/host.json` and draws the readings, the processes and
the units, and calls a snapshot older than 30 seconds stale. `TEHAS_TOP`
goes. `tehas top` stays in core.

### review

```json
"asks": ["view", "menu", "product", "history", "model", "every", "action:run", "record", "keep", "issue:draft"]
```

```
TEHAS_PLUGIN_GRANTS_REVIEW="<commit> view menu product history:2000 model:4000000 every:168 action:run record keep:50 issue:draft"
```

A run reads the range from `/run/history/` and the code, asks the model for
findings through six lenses, writes them to the view and keeps the whole
document as the record's file. It writes one draft: the critical, high and
medium findings one to a line and the rest counted, as
`console/core/src/review.rs` renders them today. A person files it.

### wiki

```json
"asks": ["view", "menu", "product", "history", "model", "every", "action:build", "keep"]
```

```
TEHAS_PLUGIN_GRANTS_WIKI="<commit> view menu product history:5000 model:15000000 every:24 action:build keep:200"
```

A run reads the raw sources and the code, has the model write or update the
pages under the schema it carries, checks them, renders them to HTML with
any raw HTML shown as text, and writes the view. It keeps the last ten
builds and the last of each release as kept files. A failed run leaves the
last view. The page is behind the sign-in, which meets decision 9 of the
wiki spec on every host.

### given

```json
"asks": ["hook:gate:cycle.before-work", "files", "model", "model:images"]
```

```
TEHAS_PLUGIN_GRANTS_GIVEN="<commit> hook:gate:cycle.before-work files:10 model:200000 model:images"
```

The hook describes each image, at most 8, 1500 characters each, into
`images.md`, every line quoted under a head that says a model wrote it from
untrusted images. It copies the text files, the files of zips, and the
files that comments linked into `/out/give/`. The settings
`TEHAS_WORK_DESCRIBE`, `TEHAS_WORK_FILES`, `TEHAS_WORK_ZIPS` and
`TEHAS_WORK_COMMENTS` go. `TEHAS_WORK_IMAGES` stays.

## What 2.0 keeps, and where it goes

Agreed with the v2 session for T16.

Kept code that nothing calls in 2.0 lives in a small crate of its own, with
public items and its tests, so it builds clean with no `dead_code` allow. It
is not left as a module inside `console/src` that nothing calls.

| Kept in T16, as library code | Used by |
| --- | --- |
| The panel's URL and id rules (`console/ops/src/panel.rs`) and the dock's sandbox words (`host/fun/panels.js`) | `frame` (4) |
| The top snapshot's cut to the published keys and the publisher's loop (`console/src/top.rs`), as a crate. The reading and the frame stay in core for `tehas top` | `feed:host` (3) |
| The mesh read, the field list and the window (`console/src/energy.rs`), as a crate | `feed:mesh` (3) |
| The wiki's collector of raw sources (`console/src/wiki/collect.rs`), as a crate | `history:` (5) |
| The draft's rules (`console/core/src/plugin/draft.rs`) and the store's record verbs | Drafts from a run (6) |
| `console/attach`, all of it, as T14 keeps it; the copy and limits of `attach/plugin_files.rs`; the approval record's texts and comment links (`attach/comments.rs`, the dispatcher's part) | `hook:gate:cycle.before-work` (7) |
| The review's renderer (`console/core/src/review.rs`), as a small crate | The `review` plugin vendors it |
| The wiki's check, render and keep rules (`console/src/wiki/check.rs`, `render.rs`, `publish.rs`), `host/wiki/` | The `wiki` plugin vendors them |
| The describe prompt, head and quoting (`console/cycle/src/describe.rs`), as a crate | The `given` plugin vendors them |
| The pages and their models: `host/fun/business-view.js`, `energy-view.js`, `host-view.js`, `host-data.js` and their styles | Moved to the plugins' repositories, not kept in core |

Not kept:

- the `review_issue` table and `state/review.json`
- the units and timers of energy, top, review and wiki. They go through
  `RETIRED_UNITS` in `console/ops/src/update/steps.rs` and the old units
  loop of `install.sh`, as T14 does for the Protolab, so `tehas update`
  stops and removes them on a host
- the settings named above. An instance that still sets one loads as it
  is: the setting is ignored with one warning line, or `tehas update`
  rewrites it. `cargo xtask config-check` covers each one

## Changes to 1.0.0

| Where | Change | Why |
| --- | --- | --- |
| v1, "Runs" | A refresh by the clock under `every:`; such a plugin is not refreshed by a moved head. `trigger` gains `clock` | Weekly and nightly runs |
| v1, guarantee 5 (outside the cycle) | Kept for the container. `hook:gate:cycle.before-work` runs while the cycle holds its slot, and the start waits for it, at most 10 minutes | The files must be there before the agents start |
| v1, guarantee 7 (a view loads nothing from another site) | Kept for a plugin's pages. `/_outside/<id>` is tehas's page and frames a granted URL | Panels |
| v1, guarantee 8 (writes only through tehas) | Kept. A run may leave a draft; a person files it | The review loop no longer files by itself |
| v1, the manifest | The asks `every`, `live:<feed>`, `feed:<name>`, `frame`, `history`, `hook:gate:cycle.before-work`. A hook's `at` may be the event before a cycle starts, with `timeout` 30 to 300 | |
| Hooks part | `cycle.before-work`, with `model` allowed in it. The event after a cycle (`after:work` in 1.x) is renamed as T9 lists it too, with no alias | |
| Security part, the container | One new read-only mount, `/run/history/`. In a `cycle.before-work` hook, the lab's `/run/issues.json` and `/given/`. Pinned in `sandbox.rs` and `test/fixtures/argv/plugin-*` | A security change |
| Security part, the product | With `history:`, the clone holds the range, up to the grant's number | The review and the wiki read diffs |
| Web part, the gate | `/_live/<feed>.json`, `/_feed/<feed>.json`, `/_outside/<id>`, `/_draft` | |
| v2 rules, rule 5 | A second exception: files a granted `cycle.before-work` hook gave, checked and listed by the host | Hard rule 4; a security change |
| v2 outside part | Unchanged. `net:` stays global unicast only | The host makes the mesh feed |
| v3, drafted issues | A batch run may propose one draft through `/out` | The review plugin has no service |
| `manifest.json` of a cycle | `plugins`: each hook's name, commit, files or reason. Added within version 1 | A person sees what was added |
| Settings | None new. Removed with their features in 2.0: `TEHAS_ENERGY*`, `TEHAS_TOP`, `TEHAS_REVIEW*`, `TEHAS_WIKI*`, and four of `TEHAS_WORK_*` | |

## Work

After plugins are on in a release (2.1), and after the lab's Work 2 and 3
(`issues:`, `files:`) and v3's Work 4 and 7 (`keep:`, records and drafts).
Each is a pull request; a security change says so.

1. **The clock.** `every:`, the tick's rule, the `clock` trigger.
2. **Live feeds.** `live:`, `/_live`. Security change: a gate route.
3. **Host feeds.** `feed:host` and `feed:mesh`, the unit, `/_feed`.
   Security change: a host unit for a plugin, a fetch of a listed address.
4. **Outside pages.** `frame`, `/_outside`, the menu items. Security
   change.
5. **History.** `history:`, the collector, the deeper clone. Security
   change: a new mount, `sandbox.rs` and the argument lists.
6. **Drafts from a run.** `/out/_tehas/draft.json`, the pending draft,
   `/_draft`.
7. **The hook before work.** The seam in `tehas-cycle`, the hook's mounts,
   the check and copy, the heads and the instructions, the manifest's
   `plugins`. Security change to what a cycle is given (hard rule 4) and a
   second exception to v2's rule 5. It starts only after the owner's
   explicit OK, recorded in its pull request; this spec proposes it and
   does not grant it.
8. **The plugins.** Seven repositories, each with its tests. `panels`,
   `business`, `energy` and `hoststats` first: they need no model.
9. **The manual.** `host/docs/plugins.md`, `tools-work.md`, `features.md`
   as building, the changelog fragment with its adopter action for each
   removed setting.

## Decided

Settled for this revision (tehas-4c, under the owner's word to decide).

- **The lenses and the wiki verb.** The `review` and `wiki` plugins carry
  their own driver and prompts, as Floorplan does. They need not match
  pi-rukas's lenses or its `wiki` verb. A plugin is not built on a tehas
  image.
- **Where a draft is filed.** In the product repository, as v3 files every
  draft. Today's loop used a second repository so that its findings did not
  reach the factory by themselves. A draft carries no label, so nothing
  starts until a person labels it (hard rule 5). `TEHAS_REVIEW_REPO` goes.
- **A time of day.** None in 2.1. `every:24` runs a day after the last
  attempt.
- **Planning.** No `before:plan`. Describe and text files at the first turn
  of `tehas plan` go in 2.0 and do not come back.
- **What "comments" covers.** `TEHAS_WORK_COMMENTS`, the attachments that
  comments link. The comments of a retry (`comments.json`) stay in core.
- **The wait and the cycle's clock.** The hook's wait does not count
  against `TEHAS_WORK_TIMEOUT`. It is bounded by its own 10 minutes.
- **Who is told.** The comment on the issue that says what a cycle was
  given names each plugin that gave files, and how many. A reviewer sees it
  without the host.
- **Labels for outside pages.** The id is the label. Ids are short words.
- **The floor's chips.** The host chip and the energy meter leave the game
  view in 2.0 with their features. `list` does not grow a second shape in
  this revision.

## Open points

- **Feeds across plugins.** The business page showed live power. One
  plugin reading another's feed is left out, and so is the console of the
  running cycles (`TEHAS_SITE_CONSOLE`), which is a stream and not a feed.
- **The host's private settings.** The decision list says to check them
  before energy moves. They are not in this repository, so the rule for
  `feed:mesh=<url>` is taken from `TEHAS_ENERGY_URL` alone. To check on the
  host before Work 3.
