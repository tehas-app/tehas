---
title: The front end
description: "How the dashboard and the front pages are built: the platform they use, the names, the island contract, the styling rules, the kit and the byte budgets."
---

# The front end

This page is the contract for the pages a factory serves: the business page,
the factory floor, the dashboard, the manual and the Protolab's pages. It says
what a page may use, how its parts are named and split, and how big it may
grow. Existing pages follow the rules as they are touched; a page moved to them
looks the same before and after.

What is true today: the pages are static HTML and Caddy templates in
`host/fun/` and `host/docs/`, with plain scripts and ES modules, no build step
and no dependency. The Protolab's pages are written by `tehasbot protolab` and
hold no script. No page has a custom element yet. The byte ratchet in
[Budgets](#budgets) holds every page at its size today.

## The platform

- No library and no framework. No npm, at run time, at install or in the gate.
- No build step. Native ES modules, loaded as the files are in `host/fun/`.
  `console update assets` gives each file a page names by tag a hashed name
  ([How the dashboard's files are named and cached](/host#how-the-dashboards-files-are-named-and-cached)).
- Native platform first. A behaviour the browser has is not written again in
  script.
- The baseline is the last two major versions of Chrome, Edge, Firefox and
  Safari, desktop and iOS, at the time of the release. A feature may be used
  when all four ship it in a stable release. No polyfill, no fallback and no
  transpiling: an older browser may look or work worse.

| Need | Use | Not |
| --- | --- | --- |
| Modal | `<dialog>` with `showModal()`, `<form method="dialog">`, `::backdrop` | A div overlay, a focus-trap script |
| Menu, tooltip, pop-over | The `popover` attribute with `popovertarget` | A positioning script |
| Disclosure | `<details>` and `<summary>`; `<details name>` for a group with one open | A toggle script |
| Actions | `<form>`, `button[name][value]`, constraint validation | A click handler that calls `fetch` |
| Filter chips | Links with the choice in the URL and `aria-current`; with no server, radio inputs and `:has(:checked)` | A tab script |
| Layout by a panel's width | Container queries | A resize observer |
| Styling by state | `:has()`, `[open]`, `:popover-open`, `:user-invalid` | Classes set by script |
| Modules | Native ES modules | A bundler |
| Time and numbers | `Intl.RelativeTimeFormat`, `Intl.NumberFormat` where the wording allows | A hand-written formatter |
| Charts | SVG | A chart library |

Not used, because one of the four engines lacks it: customized built-ins
(`is=`), CSS module scripts, `interestfor` and `::scroll-button`. A review
checks each new platform feature against the baseline.

## Three tiers

A part of a page takes the cheapest tier that does the job, and moves up only
when the tier below cannot:

1. **Native markup** with shared CSS. No element and no script.
2. **A plain element.** A custom element that adds one behaviour to the HTML
   inside it: a clock, a follow toggle, a pager. No adapter and no feed.
3. **An island.** A custom element that owns a region of the page, takes data
   and draws it through an adapter.

## Names

| Prefix | For |
| --- | --- |
| `tehas-<name>` | The shared kit: its elements, classes, data attributes and custom properties |
| `tehas-factory-<name>` | The factory's own dashboard parts, which know the shape of the files under `/live/` |

A component's CSS classes, data attributes (`data-tehas-…`) and custom
properties (`--tehas-…`) take the same prefix as its element. The theme's
tokens keep their names. A name, once
in a release, is not used for something else. Another program that builds
pages on the kit uses a prefix of its own and never these two.

A page names no product and no host: an integration is named by its role, such
as "a program that reads the factory API".

## The island contract

An island is a custom element in the light DOM.

- **Server HTML first.** The element's children are the first view: real
  content where the server wrote it, otherwise the skeleton and the empty
  state. A page with script off shows that.
- **Data in**, in this order of preference: attributes for scalars
  (`limit="10"`); a child `<script type="application/json">`, read once when
  the element upgrades; `src="/live/<file>.json"` for a feed, read through the
  kit's feed store and never by a `fetch` of its own.
- **Data out** only as DOM events: a `CustomEvent` named `<element>:<verb>`,
  for example `tehas-factory-board:filter`, with `bubbles: true` and plain JSON
  in `detail`. An element never calls another element's methods and never
  reads another element's DOM.
- **Hydration** by the attribute `hydrate`: `load` (the default), `visible`,
  `idle` or `interaction`. Until it fires, the server HTML stands.
- **Shared state** only through the kit's feed store: one poller per address
  per page, shared by every island that names it, paused while the page is
  hidden, keeping the last good answer.

An island kind is three files:

| File | Holds |
| --- | --- |
| `<name>.js` | The element: attributes, the hydrate trigger, the feed, the events. No drawing |
| `<name>.model.js` | Pure: `(data, props, nowMs) -> model`. No DOM and no library. Node tests call it |
| `<name>.render.js` | The adapter: the only file that draws, and the only one that may import a library |

The adapter's interface, whole:

```js
export function mount(host, ctx) // -> { update(model), unmount() }
// host  the custom element; the adapter owns its children and nothing outside it
// ctx   { model, emit(verb, detail), signal }  signal aborts when the element disconnects
```

Rules that keep the drawing swappable:

1. The element and the model never import a library. The byte ratchet's test
   fails on a library import, a bare address or one through a `vendor/`
   directory, in any file but a `.render.js`.
2. The model is plain data. No library type crosses `update`.
3. The adapter never fetches and never reads attributes. It gets a model and
   emits verbs.
4. Replacing how an island draws changes its `.render.js` file only. Element
   names, attributes, events and feeds stay.

No library is admitted in this release. If one ever is, it is one vendored file
under the kit, with its version, source, licence and sha256 in the kit's
manifest, loaded only by the islands that use it. Never from a CDN.

## Styling

- No shadow DOM. The themes are global custom properties and global rules, and
  server HTML must be styled before any script runs.
- Tokens live only in the theme files, `theme-<name>.css`
  ([Designs](/host#designs)), with the dark set under `:root[data-mode="dark"]`.
  No component rule goes in a theme file.
- New CSS goes in cascade layers, `@layer theme, base, kit, page`: kit rules in
  `kit`, a page's own in `page`. A rule of today's pages stays where it is until
  moving it is shown not to change what it paints.
- A component's rules are scoped by its class or element name. No rule reaches
  into another component.
- A custom element gets `display: block` or `display: contents` in the kit's
  CSS, so an element that has not upgraded lays out as its children do.
- An adapter sets classes and `--tehas-…` properties, never colours. Canvas
  code reads the tokens through `fun/theme.js`.

## Light and dark

The mode is a toggle a person sets, not the system setting. A page is light
unless the viewer picks dark, and the choice is kept in that browser where
storage allows (`fun/mode.js`, the key `tehas-mode`). Only a theme whose
`--modes` token names `dark` offers it: today `beesknees26`, on the business
page's mode button. The kit's mode element keeps this behaviour.

## The kit

The shared kit lives in `host/fun/kit/` and ships with tehas, versioned by the
release that ships it. Its `MANIFEST` starts with `kit <n>`, the contract
number, then one line per file: `file <sha256> <bytes> <name>`. The kit starts
at `kit 1` with tehas v1.0.0. No existing page loads it in this release.

Within one `kit <n>`, element names, attributes, events, the class names of the
kit's patterns and the exports of its modules are only added, never renamed or
removed. A break raises `n`, and the changelog says so under "Adopter action".
A program that builds on the kit copies `host/fun/kit/` whole at a pinned tehas
tag, checks it against the manifest, and does not edit it.

The kit holds generic parts only. The `tehas-factory-*` islands that read the
files under `/live/` are not in it.

## The live files and the pages

The files under `/live/` are for the dashboard, not for a program
([Files under live/](/api#files-under-live)); a program reads
`tehas status --json` and the host bridge. For the pages, within one `kit <n>`
a field of a file that a page reads is only added, never renamed or removed.

## Budgets

Sizes are counted in raw bytes, as served before compression, so a figure does
not move with the compressor.

`host/fun/pages.max` holds a ceiling for each page and each island:

```
page <name> <script bytes> <markup and style bytes>
island <name> <bytes>
```

| Page | Made of |
| --- | --- |
| `business` | `fun/business.html` |
| `floor` | `fun/index.html` |
| `dashboard` | `docs/_layout.html` and `docs/dashboard.frag` |
| `manual` | `docs/_layout.html` |

A page's script bytes are the files its `<script src="/fun/…">` tags name and
the modules those import by a relative address, at any depth, each file once.
Its markup and style bytes are its own files and the stylesheets its
`<link href="/fun/…">` tags name. The theme file is left out: its name comes
from a template. An island's bytes are its three files together, the island
named by the path of its `.render.js` under `fun/` without the ending.

The workspace test `update::pages` in `console/ops/src/update/pages.rs` checks
the pages against the file with the rest of the workspace's tests. It fails
when a page or an island is over its line, when one has no line, when a line
names nothing, when an island's line is over the island budget, and on a
library import outside a `.render.js` file. A change may lower a line. A change
that grows a page raises its line in the same change, and that raise is a
person's call, said in the pull request, as with the bash ceiling.

The budgets for new work:

| Thing | Budget | Held by |
| --- | --- | --- |
| An island's three files | 12 KB (12288 bytes) | The test |
| An existing page | Its line in `pages.max` | The test |
| A plain element | 2 KB | Review |
| The kit's core modules | 8 KB together | Review |
| A new page | 40 KB of script, 30 KB of CSS beside the theme and the kit | Review, and its line in `pages.max` |
| A page with no island | No script beyond the mode element | Review |

## Tests

- Pure parts, a model or a view's data function, are tested with `node --test`
  in `test/*_test.mjs`, with no document. The gate runs them where node is
  there.
- The markup and the CSS of today's pages are pinned as text by
  `test/front_page_test.sh` and `test/theme_test.sh`. A change to a page
  changes them in the same commit.
- The byte ratchet runs with the workspace's tests and needs neither node nor
  the network.
