# The Protolab, parked

**Status**: parked in tehas 2.0.0 (step T14 of
`docs/plans/2026-11-21-coding-standard-v2.md`), 2026-11-22.

The Protolab is parked, not abandoned. It was envisioned and never pulled
through, no factory used it, and 2.0 takes it out of core. The owner means
to bring it back as a plugin: `docs/specs/2026-11-22-plugin-lab.md` says
how. This record lists every piece that went and where it lived, so that
the plugin, or anyone else, can find it again.

Every path and line below is at the last commit that had all of it:
`fe9d5b1367f2de956d6b5c6e78fa94dd1ae340f4`. Read a piece with
`git show fe9d5b13:<path>`.

## What it did

A timer pass, `tehasbot protolab`, read the open issues labelled
`tehas:prototype` by an account in `TEHAS_ALLOWED_LABELERS` (or of the
issue type `TEHAS_ISSUE_TYPES` mapped to `prototype`). For each it built a
static page with the issue's title, a link to the issue, and the files
allowed accounts attached to it: images, videos, HTML files in a sandboxed
frame, and the contents of zips. The edge router served the pages on port
8081, by path and, with `TEHAS_DOMAIN`, at `lab-<issue>.<domain>`. The
front pages listed the same tickets as the Protolab panel and as the
Prototypes stage of the business page's line.

## What was removed

### The pass

| Piece | Where it lived |
| --- | --- |
| The pass: which tickets, which files, the zip line, the pages, `live/protolab.json` | `console/src/protolab.rs:1` (1298 lines) |
| One pass's build: fetch, unpack, write, remove; the list written at `console/src/protolab/build.rs:306` | `console/src/protolab/build.rs:1` (1333 lines) |
| `console protolab run` | `console/src/main.rs:122`, `:215`, `:479` |
| `tehasbot protolab`, with the App's token | `console/src/run.rs:144`, `:329`; `console/src/commands.rs:435`; `console/src/front.rs:145`; `console/src/usage/tehasbot.txt:11` |
| Its umask | `bin/tehasbot:11` |
| Its tests | `test/protolab_test.sh` (1169 lines) and the zips of `test/fixtures/protolab/` with `make-zips.sh` |

### The units

| Piece | Where it lived |
| --- | --- |
| `tehas-protolab.service` | `host/systemd/tehas-protolab.service` (8 lines) |
| `tehas-protolab.timer`, every 5 minutes | `host/systemd/tehas-protolab.timer` (9 lines) |
| Installed and enabled by `install.sh` | `install.sh:690`, `:691` |
| Enabled by `tehas update` | `console/ops/src/update/host.rs:200`; `console/ops/src/update/steps.rs:48` |

`tehas update` now stops and removes the two units (`retired.list`,
`console/ops/src/update/steps.rs`).

### `live/protolab.json` and the `protolab/` directory

| Piece | Where it lived |
| --- | --- |
| `live/protolab.json`, the list the pages read | written at `console/src/protolab/build.rs:306`; its sample `test/fixtures/live/protolab.json` |
| `~/tehas/protolab/`, a page and the files of each ticket | `install.sh:417` (`home_dirs`), `:995` (made); `host/docs/host.md:41` |
| Its mount in the edge router | `host/edge/router.compose.yaml:35` (`../protolab:/srv/protolab:ro`) |
| The router's Protolab: `lab-<n>.` names, the list by path | `host/edge/router.Caddyfile:13`, `:37`, `:78` to `:117` |
| `TEHAS_PROTOLAB_URL` made from `TEHAS_DOMAIN` as `https://lab.<domain>` | `console/core/src/settings/load.rs:876` |

`tehas update` removes the `protolab/` directory of the home
(`RETIRED_DIRS`, `console/ops/src/update/leftovers.rs`).

### The pages

| Piece | Where it lived |
| --- | --- |
| The Protolab's own pages' style | `host/fun/protolab.css` (102 lines) |
| The lab scene | `host/fun/scene-lab.svg` |
| The factory floor's PROTOLAB menu link, door and panel | `host/fun/index.html:91`, `:171`, `:227` to `:239` |
| The panel's rows, the poll of `/live/protolab.json` | `host/fun/game-view.js:135` to `:233`, `:988` |
| The panel's rules | `host/fun/game-view.css:497` to `:528`, and its row in the grid templates |
| The Prototypes stage of the business page: `labBase`, `labItems`, the lab poll, `MORE.proto` | `host/fun/business-view.js:29`, `:41`, `:53` to `:68`, `:80`, `:540` |
| The business page's Protolab link | `host/fun/business.html:87` (`nav-lab`) |
| The token `--prototype-ground` | `host/fun/theme-pixel.css:102`; `host/fun/theme-beesknees26.css:88` |
| The design notes | `DESIGN.md:379` (Protolab), `:388` (Protolab Page) |

The stage stays, as **Idea**: an open ticket that carries none of the
queue's state labels (`host/fun/stages.js`). It is read from the labels
the queue uses, never from `tehas:prototype` or `live/protolab.json`. The
CSS class `.s-proto` became `.s-idea`; the token `--stage-proto` keeps its
name.

### `prototype-file` and `tehas prototype file`

| Piece | Where it lived |
| --- | --- |
| `tehas prototype file`: open a ticket for the Protolab with your own login | `console/src/prototype.rs:1` (693 lines); `console/src/usage/prototype.txt` |
| Its routing | `console/src/front.rs:103`, `:829`; `console/src/commands.rs:29`, `:435` |
| In the API's list of commands | `api/commands.txt:64`, `:65` |
| Its tests | `test/prototype_test.sh` (183 lines) |

### The four settings

| Setting | Rule | Documented |
| --- | --- | --- |
| `TEHAS_PROTOLAB_URL` | `console/core/src/settings/rules.rs:448` | `host/docs/tools.md:154`; `api/settings-rules.json:705`; `api/surface-1.0.txt:199` |
| `TEHAS_PROTOLAB_ZIP_URL` | `console/core/src/settings/rules.rs:449` | `host/docs/tools.md:155`; `api/settings-rules.json:712` |
| `TEHAS_PROTOLAB_ZIP_HOSTS` | `console/core/src/settings/rules.rs:450` | `host/docs/tools.md:156`; `api/settings-rules.json:726` |
| `TEHAS_PROTOTYPE_FILE` | `console/core/src/settings/rules.rs:451` | `host/docs/tools.md:157`; `api/settings-rules.json:734` |

In 2.0 a line of any of the four still loads: it is on the settings
loader's `IGNORED` list, is not read, and every command warns of it once.

### The two feature words

| Word | Where it lived |
| --- | --- |
| `protolab-zip-url` (`TEHAS_PROTOLAB_ZIP_URL`, `TEHAS_PROTOLAB_ZIP_HOSTS`) | `console/core/src/features.rs:70`; `api/api.schema.json:22`; the `flags` of `api/api.*.json:44`; `host/docs/contract.md:349`, `:401`; `host/docs/features.md:200` |
| `prototype-file` (`TEHAS_PROTOTYPE_FILE`) | `console/core/src/features.rs:79`; `api/api.schema.json:23`; the `flags` of `api/api.*.json:45`; `host/docs/contract.md:428`; `host/docs/features.md:201` |

### The status field `prototypes`

`tehas status --json` carried `prototypes`, read from `live/protolab.json`
(`console/src/status.rs:239`, `:1060`; `api/status.schema.json:144`;
`host/docs/api.md:270`). With nothing writing that file in 2.0 the field
would always be left out, so its reader, its schema and its name on the
surface are removed here, and `tehas update` removes the file an earlier
version left.

### The export copy

`tehas export` copied a cycle's Protolab page into the export's
`protolab/` folder (`console/src/export.rs:501`;
`console/src/usage/export.txt:17`; `host/docs/api.md:992`, `:1011`).

### The manual

| Piece | Where it lived |
| --- | --- |
| The page `tools-protolab.md`: `tehasbot protolab`, `tehas prototype file`, a zip by address | `host/docs/tools-protolab.md` (181 lines) |
| Its row in the manual's list of pages | `host/docs/tools.md:708` |
| The Protolab's rows of the features page | `host/docs/features.md:126`, `:200`, `:201` |
| `live/protolab.json` in the list of live files | `host/docs/api.md:1629` |
| The unit and its log | `host/docs/host.md:2002`; `host/docs/edge.md:258`, `:263` |
| Prototypes and the Protolab, for people who file issues | `host/docs/operating.md:11` to `:100` |

## What was kept, and under which names

The plumbing for storing and serving files stays, under neutral names, for
the lab plugin to use:

| Kept | Was | Now |
| --- | --- | --- |
| The safe store of an item's files, and the page writer with its escaping | `console/src/protolab/build.rs`, `console/src/protolab.rs` | the `items` crate: `console/items/src/store.rs`, `console/items/src/page.rs` |
| Which open issues carry a mark from an allowed account, the applier check, the title hold, which texts give files | inside the pass | the issue feed, `console/core/src/issue_feed.rs` |
| The edge's static serving: a directory per item, sandboxed HTML, the no-script policy on the generated page, the per-item host name | `lab-<n>.`, `/srv/protolab`, `~/tehas/protolab` | `item-<n>.` in `host/edge/router.Caddyfile`, `/srv/items`, `$TEHAS_HOME/items` |
| Zip by address: the host allowlist, the redirect walk, the sha256 pin | in the pass | `console/attach/src/link.rs` (`is_external`), used by the item store |
| Fetch, limits, sniff, zip unpack and manifest | `console/attach` | `console/attach`, unchanged |

## Not removed here

The `tehas:prototype` label (`host/labels.tsv`, the label pins, the
dispatcher's flow), the `prototype` kind (`console/core/src/kinds.rs`,
`TEHAS_ISSUE_TYPES`) are removed by a separate T16 commit.

## Bringing it back

`docs/specs/2026-11-22-plugin-lab.md` is the plan: the lab plugin, with the
platform's additions (plugin labels, an issue feed, attachments, pages
behind the gate, a list) built over what is kept above.
