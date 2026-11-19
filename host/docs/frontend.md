---
title: The front end
description: "How the dashboard and the front pages are built: the platform and Solid 2, the names, the island contract, page state and feeds, the styling rules, the kit and the byte budgets."
---

# The front end

This page is the contract for the pages a factory serves: the business page,
the factory floor, the dashboard, the manual and the Protolab's pages. It says
what a page may use, how its parts are named and split, and how big it may
grow. Existing pages follow the rules as they are touched; a page moved to them
looks the same before and after.

What is true today: the pages are static HTML and Caddy templates in
`host/fun/` and `host/docs/`, with plain scripts and ES modules, no build step.
The Protolab's pages are written by `tehasbot protolab` and hold no script. The
kit in `host/fun/kit/` holds Solid 2 and the island base, but no page loads it
yet: the dashboard is the first page to move onto it. The byte ratchet in
[Budgets](#budgets) holds every page at its size today.

## The split

| Owned by | What |
| --- | --- |
| The platform | The page's structure as custom elements in the light DOM; `<dialog>`, `popover`, `<details>`, forms, `:has()`, container queries, `Intl`, SVG; the themes as global CSS |
| Solid 2 | Drawing each island, async data (feeds read with `isPending`, `<Loading>` and `<Errored>`), interface state (signals and stores, and the page stores the islands of a page share), actions |
| Pure modules | Each island's model, `(data, props, nowMs) -> model`: no DOM, no Solid, tested with `node --test` |

A custom element is the place a Solid root lives. It mounts the root when its
hydrate trigger fires and disposes it when it leaves the page. Solid draws
inside it and nowhere else.

## The platform

- One library, Solid 2, vendored in the kit as one file. No other library and
  no framework. No npm, at run time, at install or in the gate.
- No build step. Native ES modules, loaded as the files are in `host/fun/`.
  Templates are Solid's `html` tagged template, not JSX, so nothing is
  compiled.
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
| Drawing an island, its async data and state | Solid 2 through the kit | Hand-written DOM diffing, an observer or a pub-sub of its own |
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
   and draws it in Solid. A page with no island loads no Solid.

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

An island is a custom element in the light DOM, made with the kit's
`island()` (`kit/island.js`).

- **Server HTML first.** The element's children are the first view: real
  content where the server wrote it, otherwise the skeleton and the empty
  state. A page with script off shows that. Solid's `hydrate` only takes over
  HTML that Solid's own server rendering wrote, so the mount uses `render`
  and replaces the children. Until then the server HTML stands; it should have
  the shape of the drawn view, so the page does not jump.
- **Data in**, in this order of preference: attributes for scalars
  (`limit="10"`); a child `<script type="application/json">`, read once when
  the element upgrades; `src="/live/<file>.json"` for a feed, read through the
  kit's feed store (`kit/feed.js`) and never by a `fetch` of its own.
- **State between islands** only through page stores (`kit/state.js`), see
  [Page state](#page-state). An island never calls another island's methods
  and never reads another island's DOM.
- **Data out** as DOM events: a `CustomEvent` named `<element>:<verb>`, for
  example `tehas-factory-board:filter`, with `bubbles: true` and plain JSON in
  `detail`. Events are the public contract to whatever is outside the islands:
  the page's controller, a host page, a test. They are not how two islands of
  one page talk to each other.
- **Hydration** by the attribute `hydrate`: `load` (the default), `visible`,
  `idle` or `interaction`. Until it fires, the server HTML stands. An element
  laid out as its children (`data-tehas-island="contents"`) has no box, so
  `visible` watches its children.
- **One root per island.** Each island is a Solid root of its own, disposed
  when the element disconnects. Page stores and feeds live under the page's
  owner, so one island leaving disposes nothing another reads.

An island kind is three files:

| File | Holds |
| --- | --- |
| `<name>.js` | The element: `island(name, { props, load })`, its attributes and the import of its render module. No drawing and no library |
| `<name>.model.js` | Pure: `(data, props, nowMs) -> model`. No DOM and no Solid. Node tests call it |
| `<name>.render.js` | The Solid component: the only island file that draws and the only one that imports Solid |

The render module's interface, whole:

```js
export function mount(host, ctx) // -> { update(props), unmount() }
// host  the custom element; the render module owns its children and nothing outside it
// ctx   { props, data, first, emit(verb, detail), signal }
//       props  the attributes, typed by the element's `props`
//       data   the child JSON, or null
//       first  the server HTML's nodes, as they stood before the mount
//       emit   sends <element>:<verb>
//       signal an AbortSignal that aborts when the element disconnects
```

`view(Component)` from `kit/parts.js` makes that `mount` from a component: it
keeps the attributes in a store, so `update` is a store write, and `unmount` is
the disposer `render` returns.

Rules that keep the drawing swappable:

1. The element and the model never import a library. The byte ratchet's test
   fails on a library import, a bare address or one through a `vendor/`
   directory, in any file but a `.render.js` and the kit files that draw.
2. The model is plain data. No Solid type crosses into it.
3. The render module reads server data only through `kit/feed.js` and its
   attributes only through `props`.
4. Replacing how an island draws changes its `.render.js` file only. Element
   names, attributes, events and feeds stay.

## Page state

Signals are the state model. An island holds its interface state in Solid
signals and stores: no module-level variable as state, and no observer or
pub-sub written by hand.

Islands of one page share state through named page stores. A page store has a
fixed shape and named actions. The first island that asks for it makes it, and
every later one gets the same object. Islands read it reactively and write it
only through its actions: the setter is not handed out. One module defines
each store and every island that uses it imports that module.

```js
// board.state.js
import { pageStore } from "../kit/state.js";

export const useBoard = pageStore("board", {
  state: () => ({ picked: null }),
  actions: (set) => ({
    pick(number) {
      set((d) => {
        d.picked = number;
      });
    },
  }),
});
```

```js
// in a component of either island
const board = useBoard();
board.state.picked;        // a reactive read
board.actions.pick(42);    // a write; it lands on the next microtask
```

The stores live under one owner per page, made when `kit/state.js` is first
evaluated, so they outlive the island that made them.

## Feeds

Server data comes from `/live/` through `feed(src, { every })` of
`kit/feed.js`. There is one source per address per page: every island that
names the address reads the same async memo, and one poller asks for it.

- The shortest `every` any reader asks for is the poll period while that
  reader lives; none means no poll, only `refresh()`. A period is at least
  1000 ms.
- One fetch is out at a time. The next poll falls due a period after the last
  one lands, and a `refresh()` while one is out polls once more when it lands.
  A fetch with no answer in 30 s is aborted and counts as a failed poll.
- No poll while `document.hidden`. A poll that fell due meanwhile runs when the
  page shows again.
- A failed poll keeps the last good data, and `error()` says why. A first
  answer that fails is an error, which `<Errored>` catches.
- The same bytes again change nothing downstream.
- `data()` is read inside `<Loading>` until the first answer;
  `isPending(() => f.data())` is true while any poll is out, also one that
  brings the same bytes. A `<Busy>` bound to it shows during every poll.
- The feed stops, and its fetch is aborted, when the last root that asked for
  it is disposed.

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
- A render module sets classes and `--tehas-…` properties, never colours.
  Canvas code reads the tokens through `fun/theme.js`.
- The pages run under a CSP with `style-src 'self'` and no inline style: no
  `style="…"` in a template (the ratchet refuses it) and no
  `setAttribute("style")`. A value that must vary goes through a class, or
  `element.style.setProperty("--tehas-…", value)`, which the CSP allows.

## Light and dark

The mode is a toggle a person sets, not the system setting. A page is light
unless the viewer picks dark, and the choice is kept in that browser where
storage allows (`fun/mode.js`, the key `tehas-mode`). Only a theme whose
`--modes` token names `dark` offers it: today `beesknees26`, on the business
page's mode button. The kit's mode element keeps this behaviour.

## The kit

The shared kit lives in `host/fun/kit/` and ships with tehas, versioned by the
release that ships it:

| File | Holds |
| --- | --- |
| `vendor/solid.js` | Solid 2, one minified ES module made by `tools/vendor-solid`. Do not edit it |
| `vendor/LICENSES` | The licences of the bundled packages |
| `island.js` | The island base: attributes, the hydrate trigger, the mount and the disposal, the events. No library |
| `feed.js` | The feed store ([Feeds](#feeds)) |
| `state.js` | Page stores ([Page state](#page-state)), and `shared(key, make)`, one value per key per page under the page's owner |
| `parts.js` | `view`, and shared pieces drawn in Solid: `TimeAgo`, `Empty`, `Busy`, `Failed` (an error with a retry) and `Guard` (`<Loading>` inside `<Errored>` with a retry) |
| `kit.css` | The kit's rules in `@layer kit`: `[data-tehas-island]` is `display: block`, `[data-tehas-island="contents"]` is `display: contents` |
| `MANIFEST` | `kit <n>`, the contract number, then `file <sha256> <bytes> <name>` for every other file |

The kit is `kit 1`, first shipped with tehas v1.1.0. A module imports the
library by a relative address, `./kit/vendor/solid.js` from a file in `fun/`:
the pages allow no inline script, so there is no import map. Only a
`.render.js` file and the kit files that draw (`feed.js`, `state.js`,
`parts.js`) import it.

Within one `kit <n>`, element names, attributes, events, the class names of the
kit's patterns and the exports of its modules are only added, never renamed or
removed. A break raises `n`, and the changelog says so under "Adopter action".
A program that builds on the kit copies `host/fun/kit/` whole at a pinned tehas
tag, checks each file against the manifest, and does not edit it.

The kit holds generic parts only. The `tehas-factory-*` islands that read the
files under `/live/` are not in it.

## Solid 2 in tehas

The kit carries `solid-js` 2.0.3, `@solidjs/signals` 2.0.3, `@solidjs/web` 2.0.3
and `@solidjs/html` 2.0.1. The documentation is at
[docs.solidjs.com](https://docs.solidjs.com/), which is 2.0; the 1.x pages at
v1.solidjs.com do not apply. Most examples a reader or a model has seen are
1.x.

The library exports exactly these names, and the ratchet fails on an import of
any other:

`createSignal`, `createMemo`, `createStore`, `createRoot`, `createEffect`,
`createRenderEffect`, `onCleanup`, `onSettled`, `untrack`, `flush`,
`isPending`, `latest`, `refresh`, `action`, `createOptimistic`,
`createOptimisticStore`, `snapshot`, `getOwner`, `runWithOwner`, `For`, `Show`,
`Switch`, `Match`, `Loading`, `Errored`, `render` and `html`.

What differs from 1.x:

- Writes are batched and land on the next microtask; `flush()` lands them now,
  which a test needs. There is no `batch`.
- `createEffect(compute, effect)` takes two functions: what to track, then what
  to do with it. `onMount` is `onSettled`.
- Async data is a `createMemo` that returns a promise. `isPending(fn)` replaces
  `resource.loading`, `refresh` replaces `refetch`, and `<Loading>` and
  `<Errored>` (with a `reset`) replace `Suspense` and `ErrorBoundary`.
- A store's setter takes a draft: `set((d) => { d.x = 1; })`. There is no
  `produce` and no `createMutable`. `snapshot` replaces `unwrap`.
- Props are read where they are used, never destructured. In `html`, a value
  that changes is a function: `${() => m.title}`. A function with no arguments
  passed to a component becomes a getter, so a handler passed as a prop takes
  an argument: `retry=${(e) => live.refresh()}`.
- `<For each>` passes the item and an index accessor, and keys rows by
  reference. A model makes new row objects on every feed change, so that would
  draw every row again: focus is lost and a click can land on a node just
  removed. Key rows by what names them, `keyed=${(r) => r.number}`; the item
  then comes as an accessor, `row().title`. `keyed={false}` replaces `Index`.

The ratchet's test fails on these 1.x names in any file of `host/fun`:
`createResource`, `startTransition`, `useTransition`, `batch(`, `onMount`,
`Suspense`, `ErrorBoundary`, `createMutable`, `produce(`, `mergeProps`,
`splitProps`, `solid-js/web`, `solid-js/store` and `solid-js/html`.

Templates are constant text. Text from a data file is bound as an expression,
so it lands as text, and is never pasted into the template. In a file that
imports the library the test fails on an `html` template joined with `+`, on a
call of `html(…)`, on an untagged template literal inside an `html` template
(a nested `html` template is fine) and on a `style=` attribute in one.

A complete island:

```js
// fun/tehas-factory-queue.js
import { island } from "./kit/island.js";

island("tehas-factory-queue", {
  props: { src: String, limit: Number },
  load: () => import("./tehas-factory-queue.render.js"),
});
```

```js
// fun/tehas-factory-queue.model.js
export function queueModel(status, props) {
  const rows = (status?.queued ?? []).map((i) => ({ number: i.number, title: i.title }));
  return { rows: rows.slice(0, props.limit ?? rows.length) };
}
```

```js
// fun/tehas-factory-queue.render.js
import { For, html } from "./kit/vendor/solid.js";
import { feed } from "./kit/feed.js";
import { Empty, Guard, view } from "./kit/parts.js";
import { queueModel } from "./tehas-factory-queue.model.js";

export const mount = view((props, ctx) => {
  const live = feed(props.src, { every: 10000 });
  const model = () => queueModel(live.data(), props);
  return html`<${Guard} retry=${(e) => live.refresh()}>
    <ul>
      <${For} each=${() => model().rows} keyed=${(r) => r.number} fallback=${html`<${Empty} text="Nothing in line" />`}>${(row) =>
        html`<li><button type="button" onClick=${() => ctx.emit("pick", { number: row().number })}>
          ${() => row().title}
        </button></li>`}<//>
    </ul>
  <//>`;
});
```

```html
<tehas-factory-queue data-tehas-island src="/live/status.json" limit="10">
  <p class="tehas-empty">Nothing in line</p>
</tehas-factory-queue>
<script type="module" src="/fun/tehas-factory-queue.js"></script>
```

## The live files and the pages

The files under `/live/` are for the dashboard, not for a program
([Files under live/](/api#files-under-live)); a program reads
`tehas status --json` and the host bridge. For the pages, within one `kit <n>`
a field of a file that a page reads is only added, never renamed or removed.

A page reads these files add-only: it reads the fields it knows and leaves the
rest, and a field it reads may be missing, so a writer can add a field without a
page change. `test/fixtures/live/` holds one sample of each file a page in
`host/fun/` fetches, with the fields the pages read. A page that starts to read
a new file adds its sample in the same change. A writer that would drop or
rename a field of a sample breaks a page, which this rule does not allow.

## Budgets

Sizes are counted in raw bytes, as served before compression, so a figure does
not move with the compressor.

`host/fun/pages.max` holds a ceiling for each page and each island:

```
page <name> <script bytes> <markup and style bytes>
island <name> <bytes>
library solid <bytes>
```

| Page | Made of |
| --- | --- |
| `business` | `fun/business.html` |
| `floor` | `fun/index.html` |
| `dashboard` | `docs/_layout.html` and `docs/dashboard.frag` |
| `manual` | `docs/_layout.html` |

A page's script bytes are the files its `<script src="/fun/…">` tags name and
the modules those import by a relative address, at any depth, each file once,
but the library. `kit/vendor/solid.js` is counted once, on the `library solid`
line, and in no page: a page that loads an island pays for it once.
Its markup and style bytes are its own files and the stylesheets its
`<link href="/fun/…">` tags name. The theme file is left out: its name comes
from a template. An island's bytes are its three files together, the island
named by the path of its `.render.js` under `fun/` without the ending.

The workspace test `update::pages` in `console/ops/src/update/pages.rs` checks
the pages against the file with the rest of the workspace's tests. It fails
when a page, an island or the library is over its line, when one has no line,
when a line names nothing, when an island's line is over the island budget, on
a library import outside a `.render.js` file and the kit files that draw, when
the kit's `MANIFEST` does not match its files, and on the Solid rules of
[Solid 2 in tehas](#solid-2-in-tehas). A change may lower a line. A change
that grows a page raises its line in the same change, and that raise is a
person's call, said in the pull request, as with the bash ceiling.

The budgets for new work:

| Thing | Budget | Held by |
| --- | --- | --- |
| An island's three files | 12 KB (12288 bytes) | The test |
| The library | Its `library solid` line | The test |
| An existing page | Its line in `pages.max` | The test |
| A plain element | 2 KB | Review |
| The kit's core modules | 8 KB together | Review |
| A new page | 40 KB of script, 30 KB of CSS beside the theme and the kit | Review, and its line in `pages.max` |
| A page with no island | No script beyond the mode element | Review |

## Tests

- Pure parts, a model or a view's data function, are tested with `node --test`
  in `test/*_test.mjs`, with no document. The gate runs them where node is
  there.
- The kit's page stores and feeds are tested in node too
  (`test/kit_state_test.mjs`, `test/kit_feed_test.mjs`). The library needs two
  methods of a document when it loads, which `test/kit_stub.mjs` gives; with
  it, signals, stores, async memos and `isPending` run in node. Drawing does
  not.
- What Solid draws is checked in a browser, outside the gate, which has no
  browser: `node test/browser/kit-check.mjs` serves `test/browser/` with the
  kit and `test/fixtures/live/` under a CSP with no inline script or style,
  loads it in headless Chromium and checks two islands that share one feed and
  one store. It prints `skip` where no Chromium is found. The islands of a
  page get render tests in tehas-e2e.
- The markup and the CSS of today's pages are pinned as text by
  `test/front_page_test.sh` and `test/theme_test.sh`. A change to a page
  changes them in the same commit.
- `test/live_fixtures_test.mjs` runs the page code against the samples in
  `test/fixtures/live/`: each sample through the views that read it, and the
  plain dashboard drawn from all of them. It fails when a page fetches a file
  that has no sample.
- The byte ratchet runs with the workspace's tests and needs neither node nor
  the network.
