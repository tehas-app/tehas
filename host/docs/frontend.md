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
kit in `host/fun/kit/` holds Solid 2 and the island base. The plain dashboard
is on it: its sections are the islands of `host/fun/factory/`
([The plain dashboard](#the-plain-dashboard)). The business page and the
factory floor still draw with plain scripts and move in 1.2. The byte ratchet
in [Budgets](#budgets) holds every page at its size today.

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
  kit's feed store (`kit/feed.js`) and never by a `fetch` of its own; a stream
  of events through `kit/stream.js` ([Streams](#streams)). A request that
  changes something is a press (`kit/press.js`, [Presses](#presses)). A console that grows is read by a plain element
  instead ([The plain dashboard](#the-plain-dashboard)).
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
   directory, in any file but a `.render.js` and the kit files that draw. It
   also fails on an element or model file whose relative imports reach the
   library or a kit file that draws (`feed.js`, `parts.js`, `state.js`,
   `stream.js`), other than through a `.render.js` file: an element would load
   the library before its hydrate trigger fires. `stream.js` holds Solid state
   and so counts as a kit file that draws; `press.js` and `request.js` hold no
   library, and an element or a model may import them.
2. The model is plain data. No Solid type crosses into it.
3. The render module reads server data only through `kit/feed.js` and
   `kit/stream.js`, sends it only through `kit/press.js`, and reads its
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

Since `kit 1` the rule is one source per address and request options (below).
A reader that gives other options gets a source and a poller of its own for
the same address. This is a change to the rule, not only an addition to it,
and it is kept only so a port can send another program's session header:
islands that share an address give the same options. Objects count by their
content, whatever the order of their keys. A function counts by its identity,
so a `headers` function is defined once per page, at the top of a module
(see [Request headers](#request-headers)); an arrow written inside an island
makes a source per island and per mount.

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
- A template given as a value, such as a `fallback=${html`…`}`, is drawn
  when the outer template is, outside the `<Loading>` and `<Errored>` around
  it. It must not read a feed: an error there is caught by nothing and stops
  every feed of the page. Pass a function, or take its text from props.
- Data that only part of an island needs is read under a boundary of its
  own, so a file that fails does not hide what another file gave.
- The feed stops, and its fetch is aborted, when the last root that asked for
  it is disposed.

A feed of another program's API takes these options, each added in `kit 1`
with no change to a feed that gives none:

| Option | Does |
| --- | --- |
| `headers` | Request headers: an object, or a function read on each poll, so a session header can change |
| `credentials` | The `fetch` credentials mode, such as `"same-origin"` or `"include"` |
| `parse` | `"json"`, the default, or `"text"`: `data()` is then the body as text |
| `accept` | A status that is data, not an error: `{ 404: { releases: [] } }` makes a deliberate 404 an empty value. The value may be a function of the body's text. A status not listed and not 2xx is an error, as before |
| `expose` | The answer's headers that `meta()` gives, by name |

```js
const plan = feed("/api/plan", {
  every: 30000,
  headers: sessionHeaders,          // defined once per page, see below
  accept: { 404: { releases: [] } },
  expose: ["X-Session"],
});
plan.meta(); // null before the first answer, then
             // { status: 404, headers: { "x-session": "…" }, date: <ms>, at: <ms> }
```

- `meta()` is the last answer: its `status`, the `expose`d headers by
  lower-case name (null where absent), `date`, the server's `Date` header in
  ms (null without one), and `at`, the viewer's clock when it came. It is a
  signal of its own: it never suspends, it moves on every answer, a failed
  one too, and a new `Date` with the same bytes leaves `data()`'s readers be.
  It lands a moment before `data()` does, while the poll is still pending.
- The server's clock is `meta().date` moved on by what the viewer's clock has
  run since `meta().at`. The host island judges a snapshot's age so.
- An error of a status that is not accepted carries it as `error.status`.

Two more, added in `kit 1` with no change for a caller that does not use
them:

- `refresh()` returns a promise of the answer of the poll it asked for. It
  never rejects: it resolves `{ ok: true, status, data }` when the answer is
  data, a 2xx or a status in `accept`, the same bytes as before too, and
  `{ ok: false, status, error }` when the poll failed, timed out or was
  aborted, with `status` 0 when no answer came. When it resolves, the answer
  is in `data()` and the readers have drawn it. A `refresh()` while a poll is
  out resolves with the poll that follows it. A caller that ignores the
  promise sees what it saw before. A refresh of a feed that is gone resolves
  at once with `ok: false`.
- `put(value)` makes `value` the data of every reader of that feed at once,
  with no fetch, as if an answer had brought it: the answer of a press or an
  event a stream pushed. The source stays one per address and options. The
  poll goes on at its period, and a poll that then brings the same data
  leaves the readers be, as does a put of the data they already have. A put
  while a poll is out takes the place of that poll's answer, which was asked
  before the value was known, and `isPending` no longer counts that poll. A
  poll asked in the same step as a put, by `refresh()` or by the period, goes
  out after it: the readers have the value at once, then that poll's answer.

```js
// a press's answer, or a live event, into the feed: no second request
const view = feed(`/api/t/${n}/view`, ASK);
const pressed = await press(`/api/t/${n}/act`, { body });
view.put(pressed.data);

// wait for the answer a refresh asked for, the same as before or not
const r = await tickets.refresh();
if (!r.ok) say(r.status ? `The server answered ${r.status}.` : "No answer.");
```

### Request headers

A header that changes, such as a session, lives in a page store, and one
module of the page exports the function that reads it. Every feed, stream
and press of that address passes that same function, so they share one
source. The function is read untracked on each poll or connect: a change of
the session is sent with the next one and starts no request of its own.

```js
// session.state.js
import { pageStore } from "../kit/state.js";
export const useSession = pageStore("session", {
  state: () => ({ id: "" }),
  actions: (set) => ({ set(id) { set((d) => { d.id = id; }); } }),
});
export const sessionHeaders = () => ({ "X-Session": useSession().state.id });
```

## Streams

A stream of events comes through `stream(src, options)` of `kit/stream.js`:
`text/event-stream` or NDJSON read over `fetch`, so it can send headers,
which an `EventSource` cannot. There is one connection per address and
options per page.

```js
// in a render module
import { stream } from "../kit/stream.js";
import { useTicket } from "./ticket.state.js";

const ticket = useTicket();
const live = stream("/api/tickets/41/events", {
  format: "sse",                                   // or "ndjson"
  headers: sessionHeaders,                         // one function per page
  on: ticket.actions.apply,                        // each event, { type, data, id }
});
live.state();      // "connecting", "open", "error" or "closed"
live.error();      // the last error, or null
live.status();     // the last answer's status, or 0
live.reconnect();  // drop the connection and connect now
```

- `on` is called with each event, `{ type, data, id }`. An SSE event's
  `type` is its `event:` field or `"message"`, `id` the last `id:`; an NDJSON
  line is `{ type: "message", data, id: null }`. `data` is parsed as JSON;
  `parse: "text"` leaves it a string. An event that does not parse is
  skipped and said in `error()`, and the stream stays open.
- Wire `on` to a page store's action. The events then land in the store and
  every island reads them there. Two readers that give the same function,
  such as the same store's action, get each event once.
- `state()`, `error()` and `status()` are signals. A dropped connection, a
  status that is not 2xx and an answer that does not come in 30 s are
  errors: the state is `"error"` and it connects again after a wait that
  starts at `retry` (1000 ms) and doubles up to `max` (30000 ms). An SSE
  `retry:` field sets the first wait, no shorter than `retry` and no longer
  than `max`. An answer that opens resets the wait. A stream the server ends
  is asked for again. No wait is longer than `max`, after a failure, an end
  or a break, with a server's `retry:` field or `ended` set. The one
  exception is an `ended` over 30000 with no `max` given: that wait is kept,
  and caps the waits after an end or a break in place of `max`. An SSE
  stream sends `Last-Event-ID` when it connects again: the `id:` of the last
  event it dispatched, so an event cut off before its blank line is asked
  for again. An event the stream ends in the middle of is dropped, as an
  `EventSource` drops it.
- A 204 answer stops it, as it stops an `EventSource`: the state is
  `"closed"` until `reconnect()`.
- The error of an answer the stream did not read is a `StreamError` with
  `status` and the body as `data`: parsed when it is JSON, else text, cut at
  64 KiB. The body is read for at most 2 s; a body that stalls gives what
  came by then. The wait before asking again counts from the answer, not
  from the end of the body. A `PressError` carries a body the same way.
- While the page is hidden an open connection stays, and a reconnect that
  falls due waits until the page shows. With `hidden: "close"` the connection
  closes while the page is hidden and opens again when it shows. With
  `hidden: "live"` it connects again while hidden too, as an `EventSource`
  does.
- `headers` is an object or a function read on each connect, so a session
  that changes is sent on the next one. Define the function once per page
  ([Request headers](#request-headers)): it is part of the connection's key.
  `credentials` is passed to `fetch`.
- The connection closes when the last root that asked for it is disposed.

A page moved from a client of its own can keep that client's ends and
waits. These options were added in `kit 1`; without them a stream does what
is said above. Each is part of the connection's key.

| Option | Does |
| --- | --- |
| `end` | Ends the stream for good on an answer it names: a list of statuses, or a function `(status, type)` of the status and the content type, without its parameters, that returns `true`. The state is `"closed"`, `status()` and `error()` stay, with the body on the error, until `reconnect()` |
| `grow` | `"double"`, the default, or `"fixed"`: every wait after a failure is `retry`, up to `max` |
| `ended` | The wait in ms after the server ends a stream cleanly, 0 or more. By default it is the first wait, `retry` or the server's `retry:` field. A `max` given caps it; with no `max`, an `ended` over 30000 is kept. A server's `retry:` field replaces it, as an `EventSource` waits that field after an end too. A failure after it waits the backoff's next step. 0 asks again at once, as some clients did. A second clean end in a row that brought no event waits `retry`, then the backoff grows as after a failure, so a server that ends every stream at once is not asked in a loop |
| `broken` | `"fail"`, the default, or `"end"`: a stream that breaks after it opened waits as a clean end does, `ended` or the first wait, and an end or a break starts the backoff again, so only failed connects grow it. A server's `retry:` field replaces the wait after a break as after an end, and a break counts with the clean ends: a second end or break in a row that brought no event backs off as `ended` says. A client that read the stream with `fetch` often did both, and often waited the same after every end or break, with an event or not |
| `hidden` | `"live"`, above |

```js
// What an EventSource did: give up on any answer it cannot read, wait 3 s
// after a failure or an end, and keep asking while the tab is hidden.
const eventSource = (status, type) => status !== 200 || type !== "text/event-stream";
stream(src, { end: eventSource, retry: 3000, grow: "fixed", ended: 3000, hidden: "live", on });

// Give up on a refusal only, and show the server's sentence.
const live = stream(src, { end: [401, 403], on });
live.error()?.data?.error;   // "Sign in to see this ticket."

// 2 s after a failure, then 4 s, and so on; 1 s after a clean end or a
// break, and the next failure waits 2 s again.
stream(src, { retry: 2000, ended: 1000, broken: "end", on });

// Ask again at once after a clean end.
stream(src, { format: "ndjson", ended: 0, on });
```

Define an `end` function once per page, as a `headers` function is: it
counts by its identity.

## Presses

A request that changes something is a press: `press(src, options)` of
`kit/press.js`, a `POST` by default. It has no library, so it runs inside
Solid's own `action()`; the kit does not wrap `action`, `createOptimistic` or
`createOptimisticStore`.

```js
const answer = await press("/api/tickets", {
  headers: sessionHeaders,
  body: { title },               // a plain object: sent as JSON, with its content type
  accept: { 409: null },         // a status that resolves instead of rejecting
  expose: ["X-Session"],
});
// { status, ok, data, headers, date }
```

- `method`, `headers` (an object or a function), `credentials` and `signal`
  go to `fetch`.
- A `body` that is a string, `FormData`, `URLSearchParams`, a `Blob` or bytes
  goes as it is, so a native form's `new FormData(form)` can be pressed. A
  plain object, an array, a number, a boolean or `null` goes as JSON with
  `Content-Type: application/json` unless one is given. Any other body is
  refused with a `TypeError` before anything is sent.
- `parse` reads the answer, `"json"` by default or `"text"`. An empty body is
  `null`.
- A status that is neither 2xx nor in `accept` rejects with a `PressError`,
  which carries `status` and the body as `data` (parsed when it is JSON).

The pattern: an optimistic value shows at once, a failed press rolls it
back, and a press that works holds it until the truth comes.

```js
import { action, createOptimistic } from "../kit/vendor/solid.js";
import { feed } from "../kit/feed.js";
import { press } from "../kit/press.js";

const votes = feed("/api/votes");
const [shown, setShown] = createOptimistic(() => votes.data().count);
const vote = action(function* () {
  setShown((n) => n + 1);                 // shows at once
  yield press("/api/vote", { body: {} }); // a rejection ends the action: the guess reverts
  votes.refresh();                        // held with the action: no step back
});
// onClick=${(e) => vote().catch((err) => setFailed(message(err)))}
```

- Every write inside the action is held until it ends, and an optimistic
  write reverts when it ends. So the truth must come in the action: write it
  after the `yield`, or refresh the feed it comes from. A feed's `refresh()`
  inside the action is held with it, unless a poll of that feed was out:
  then the guess reverts and the truth shows when that poll lands.
- Put a bare `yield` before any write that follows an `await`: code after an
  `await` runs outside the action.
- For a list, `createOptimisticStore(() => truth, seed)` with a draft write:
  push a guess before the press, and write what the press answered into the
  page store after the `yield`.

```js
const [shown, setShown] = createOptimisticStore(() => ({ items: list.state.items }), { items: [] });
const add = action(function* (title) {
  setShown((d) => {
    d.items.push({ title, pending: true });
  });
  const saved = yield press("/api/items", { body: { title } });
  list.actions.add(saved.data);           // the page store: the truth
});
```

- The action's promise rejects with the press's error; catch it where the
  island says what went wrong.

## Moving a hand-written client onto the kit

For a page whose reads, streams and presses are a client of its own:

| The client did | On the kit |
| --- | --- |
| A `fetch` on a timer, or a poll loop | `feed(src, { every })` |
| A request header, such as a session | `headers: sessionHeaders`, one function per page that reads a page store ([Request headers](#request-headers)), on a feed, a stream or a press |
| A 404 or another status that means "nothing" | `accept: { 404: <empty value> }`: data, not an error, and not "down" |
| Reading the answer's status, a header or the `Date` | `meta()`, with `expose` for the headers |
| A text file, not JSON | `parse: "text"` |
| `EventSource`, or a reader of SSE or NDJSON over `fetch` | `stream(src, { format, headers, on })` |
| Its own reconnect and backoff | `stream` connects again; `state()` says where it is |
| An event handler that writes state | `on: store.actions.<verb>`, a page store's action |
| A post with its own pending and rollback | `action()` with `press()` and `createOptimistic` or `createOptimisticStore` |
| An `EventSource` that gave up on a refusal or a wrong content type | `end: (status, type) => status !== 200 \|\| type !== "text/event-stream"`, or a list such as `end: [401]` |
| The server's sentence of a refused stream | `error().data`, the refused answer's body |
| Its own waits before asking again | `retry`, `grow: "fixed"`, `max`, `ended` for the wait after a clean end, and `broken: "end"` when a break after the open waited as an end and the backoff grew only on failed connects |
| Asking again while the tab is hidden | `hidden: "live"` |
| A wait for the answer of a read it asked again | `await f.refresh()`: `{ ok, status, data }` or `{ ok: false, status, error }` |
| Taking the answer of a press or a pushed event as the data, with no second read | `f.put(value)` |

- Keep the client's model pure: the shapes of its answers become the
  model's input, the render module wires the feed, the stream and the store.
- One address and one set of options is one source per page. Two islands
  that need the same feed or stream give the same options, the same
  `headers` function and the same `on`.
- A byte range read of a file that grows stays a plain element: see the
  console's tail in [The plain dashboard](#the-plain-dashboard).
- Where the kit's default differs from what the page did, keep the old
  behaviour with an option rather than code of the page's own:

```js
// The page read the view again after a press: take the press's answer.
const done = action(function* () {
  const r = yield press(api(`/t/${n}/act`), { ...ASK, body });
  view(n).put(r.data);
});

// The page waited for a read it asked again, the same answer or not.
const r = await tickets().refresh();

// The page's EventSource gave up for good on a 401 or a 5xx, waited 3 s,
// and asked again while hidden.
const live = stream(src, { end: eventSource, retry: 3000, grow: "fixed", ended: 3000, hidden: "live", on });

// The page read the stream with fetch: 1 s after an end or a break, and
// after a failed connect 2 s, then 4 s, up to 30 s, from 2 s again after
// the next stream that opened.
stream(src, { retry: 2000, ended: 1000, broken: "end", on });

// The page asked again for the server's sentence after a refused stream.
const said = live.error()?.data?.error;
```

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
- The kit and the pages use no inline style, so they work under a CSP with
  `style-src 'self'`: no `style="…"` in a template (the ratchet refuses it)
  and no `setAttribute("style")`. A value that must vary goes through a
  class, or `element.style.setProperty("--tehas-…", value)`, which such a
  CSP allows. The edge sends no CSP for the pages today; the browser checks
  run them under one.

## Light and dark

The mode is a toggle a person sets, not the system setting. A page is light
unless the viewer picks dark, and the choice is kept in that browser where
storage allows (`fun/mode.js`, the key `tehas-mode`). Only a theme whose
`--modes` token names `dark` offers it: today `beesknees26`, on the business
page's mode button. The kit has no mode element: a page that offers the
toggle loads `fun/mode.js`.

## The kit

The shared kit lives in `host/fun/kit/` and ships with tehas, versioned by the
release that ships it:

| File | Holds |
| --- | --- |
| `vendor/solid.js` | Solid 2, one minified ES module made by `tools/vendor-solid`. Do not edit it |
| `vendor/LICENSES` | The licences of the bundled packages |
| `vendor/solid.d.ts` | Types for `vendor/solid.js`, written by hand from the packages' own `.d.ts` files, for each name it exports and no other, an async `createMemo` among them, whose accessor gives what its promise resolves to. A checker such as `tsc --checkJs` reads it in place of the bundle. An install never stages it or copies it into a home, and it counts in no page. A pull request's preview copies `host/fun` whole, so the preview serves it |
| `island.js` | The island base: attributes, the hydrate trigger, the mount and the disposal, the events. No library |
| `feed.js` | The feed store ([Feeds](#feeds)) |
| `stream.js` | The stream source ([Streams](#streams)) |
| `press.js` | Presses ([Presses](#presses)). No library |
| `request.js` | What the kit's modules that make requests share about a request and its answer. No library |
| `state.js` | Page stores ([Page state](#page-state)), and `shared(key, make)`, one value per key per page under the page's owner |
| `parts.js` | `view`, and shared pieces drawn in Solid: `TimeAgo`, `Empty`, `Busy`, `Failed` (an error with a retry) and `Guard` (`<Loading>` inside `<Errored>` with a retry) |
| `kit.css` | The kit's rules in `@layer kit`: `[data-tehas-island]` is `display: block`, `[data-tehas-island="contents"]` is `display: contents` |
| `MANIFEST` | `kit <n>`, the contract number, then `file <sha256> <bytes> <name>` for every other file |

The kit is `kit 1`, first shipped with tehas v1.1.0. A module imports the
library by a relative address, `./kit/vendor/solid.js` from a file in `fun/`:
the pages use no inline script, so they work under a CSP without
`'unsafe-inline'`, and there is no import map. Only a
`.render.js` file and the kit files that draw (`feed.js`, `state.js`,
`parts.js`, `stream.js`) import it.

Within one `kit <n>`, element names, attributes, events, the class names of the
kit's patterns and the exports of its modules are only added, never renamed or
removed. A break raises `n`, and the changelog says so under "Adopter action".
A program that builds on the kit copies `host/fun/kit/` whole at a pinned tehas
tag, checks each file against the manifest, and does not edit it.

A `.d.ts` file under `host/fun` is for a type checker. An install does not
copy it into the home, and the sweep removes one there. An address of one in a
JSDoc comment, such as `import("./vendor/solid.d.ts")`, is no import: the asset
checks and `pages.max` do not read it. An address of one in code is an import
of a file no home has, and the checks refuse it.

The kit's modules carry their types as JSDoc comments: what each export takes
and returns, and the types a caller names. `request.js` has `Headers`, `Accept`
and `Parse`; `island.js` has `Ctx`, `Handle`, `RenderModule` and `IslandSpec`;
`feed.js` has `FeedOptions`, `FeedAnswer` and `Feed`; `press.js` has
`PressOptions` and `PressAnswer`; `stream.js` has `StreamOptions`,
`StreamEvent` and `Stream`. A file names one with
`/** @import { Feed } from "../kit/feed.js" */`. The comments are served and
counted like any other bytes; they change no code. `tsc --checkJs --strict`
reads the kit with no error. A program that copies the kit and checks its own
code with `tsc --checkJs` gets the kit's types from the modules and
`vendor/solid.d.ts`, and needs no copy of its own.

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
(a nested `html` template is fine) and on a `style=` attribute in one. Space
or a comment between `html` and its backtick does not hide a template, and
`html` is imported under its own name: `import { html as h }` fails, since the
checks read the name `html`. `export … from` the library takes only the names
it exports, as an import does.

A complete island, alone on its page. For two islands that share a feed and a
page store, copy the tested demo in `test/browser/`: `index.html` (the page),
`board.state.js` (the store both import), and `kit-demo-queue.*` and
`kit-demo-picked.*` (each island's element, model and render module).
`kit-check.mjs` checks it in a browser ([Tests](#tests)).

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

## The plain dashboard

`host/docs/dashboard.frag` is the first page on the kit. Each section is an
island of `host/fun/factory/`, laid out as its children
(`data-tehas-island="contents"`), so the page's grid and rules are as before.
The fragment holds the server HTML each one replaces on mount: the state row
reads "loading", the lists are empty and the optional sections are hidden.

| Element | Reads | Draws |
| --- | --- | --- |
| `tehas-factory-state` | `status.json`, `running.json` | The state, the all-hands and read-only marks, how old the queue is, the link to all issues |
| `tehas-factory-console` | `running.json`, `status.json` | The tabs of the cycles, the shown cycle's title and step, and the console |
| `tehas-factory-issues` | `status.json` | One list, named by `list`: `queued`, `awaiting`, `stopped` or `completed` |
| `tehas-factory-readonly` | `status.json` | What a read-only factory's starts left |
| `tehas-factory-days` | `analytics.json` | Last 10 days |
| `tehas-factory-previews` | `previews.json` | The previews |
| `tehas-factory-host` | `top.json` | The host and its units, the snapshot's age by the server's clock |

- Seven islands read `status.json` through one feed, polled every ten seconds;
  `running.json` the same. `analytics.json` is read once a minute. `top.json`
  is read every ten seconds while the last answer was good and once a minute
  while there is none: the island reads it at a minute, and a second reader
  that asks for ten seconds lives only while the file is there.
- A file that is not there leaves its island's server HTML: a host with no
  previews, no `top.json` or no report has those sections hidden, as before.
  The section comes when the file does.
- The host bridge's answer (`fun/host-bridge.js`) reaches the islands that
  draw tickets through the page store of `factory/bridge.state.js`.
- What more than one island draws, the space between two elements and the
  host's notes on a ticket, is in `factory/parts.render.js`. It is a render
  module with no element, so `pages.max` gives it an island line of its own.
- The host island judges the snapshot's age by the server's clock: the
  `Date` of the feed's last answer (`meta()`), moved on by what the viewer's
  clock has run since. A viewer whose clock is off from the host's sees the
  same age as one whose clock is right.
- The console is a plain element inside the console island,
  `<tehas-factory-tail file live>` (`factory/tehas-factory-tail.js`). A
  console is text that grows and is read by byte range, so the element reads
  it itself, every two seconds, and follows the output while its checkbox is
  checked. It is the one fetch of the page outside a feed. A text feed with a
  `Range` header would not do: the tail appends each answer where a feed
  replaces its data, two equal answers in a row are one to a feed, the
  offset counts bytes where a text feed gives characters, and a 206, a 200
  and a 416 each mean something else to it.
- A bar's length in "Last 10 days" is the custom property
  `--tehas-factory-share`, which `factory/factory.css` reads.

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
| `dashboard` | `docs/_layout.html` and `docs/dashboard.frag`, with the islands its tags load |
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
the kit's `MANIFEST` does not match its files, when `vendor/solid.d.ts` is
missing, does not type exactly the names the library exports or exports in a
form the check does not read, on a `.d.ts` address in code, and on the Solid
rules of
[Solid 2 in tehas](#solid-2-in-tehas). A change may lower a line. A change
that grows a page raises its line in the same change, and that raise is a
person's call, said in the pull request, as with the bash ceiling.

The owner approved these lines on 2026-11-20: `library solid 123689`, the
kit's Solid as `tools/vendor-solid` bundles it, `page dashboard 83939 53988`,
the dashboard's script at 83939 bytes on the kit, then
`page dashboard 88935 53988`, the dashboard's script with the kit's JSDoc
types, and under that approval of the types' growth
`page dashboard 88967 53988`, with a page store's comparison untracked.

The budgets for new work:

| Thing | Budget | Held by |
| --- | --- | --- |
| An island's three files | 12 KB (12288 bytes) | The test |
| The library | Its `library solid` line | The test |
| An existing page | Its line in `pages.max` | The test |
| A plain element | 2 KB | Review |
| The kit's core modules | 30 KB (30720 bytes) for `island.js`, `feed.js`, `request.js`, `state.js` and `parts.js`, which a page that polls loads: a review figure, not a gate. The owner set 24 KB on 2026-11-20 and then approved the growth of the kit's JSDoc types, but named no new figure: 30 KB is proposed by the change that typed the kit and waits for the owner's word. At `kit 1` with the types they are 29616 bytes, under it. `stream.js` and `press.js`, 19843 more, load only on a page that imports them | Review |
| A new page | 40 KB of script, 30 KB of CSS beside the theme and the kit | Review, and its line in `pages.max` |
| A page with no island | No script beyond `fun/mode.js` | Review |

## Tests

- Pure parts, a model or a view's data function, are tested with `node --test`
  in `test/*_test.mjs`, with no document. The gate runs them where node is
  there.
- The kit's page stores, feeds, streams and presses are tested in node too
  (`test/kit_state_test.mjs`, `test/kit_feed_test.mjs`,
  `test/kit_stream_test.mjs`, `test/kit_press_test.mjs`). The library needs two
  methods of a document when it loads, which `test/kit_stub.mjs` gives; with
  it, signals, stores, async memos and `isPending` run in node. Drawing does
  not.
- What Solid draws is checked in a browser, outside the gate, which has no
  browser: `node test/browser/kit-check.mjs` serves `test/browser/` with the
  kit and `test/fixtures/live/` under a CSP with no inline script or style,
  loads it in headless Chromium and checks two islands that share one feed and
  one store, and a third, `kit-demo-wire`, against a small API: a feed with a
  request header, a chosen answer header and the server's `Date`, a 404 that
  is accepted as an empty value, an SSE stream that resumes from
  `Last-Event-ID` and an NDJSON stream feeding a page store, both closed by a
  204, and a press whose optimistic value holds when it works and rolls back
  when it fails. It prints `skip` where no Chromium is found.
  `node test/browser/dashboard-check.mjs` does the same for the plain
  dashboard: it renders the manual's layout around the fragment, serves the
  samples and a console that grows, and checks each section, the tabs, the
  console's tail and the polling. `--shot <png>` saves a screenshot.
  `--staged` first stages `host/` with `console update assets`, as
  `install.sh` does, and serves the stage, so the page is checked with the
  hashed names a home gets; it also fails when an address under `/fun/` has
  no file.
- The kit's types are checked by the workspace test `update::types` in
  `console/ops/src/update/types.rs`. It runs TypeScript's native `tsc` 7.0.2
  with `--checkJs --strict --noEmit` over the kit's seven modules and
  `test/fixtures/kit-types/uses.js`, which uses the kit as this page says,
  and fails on any error. Each `misuse-*.js` beside it makes one mistake, on
  the line marked `// rejected: TS<code>`: an option spelt wrong, the data of
  a refresh's answer read without asking whether it is ok, a method a feed
  does not have, `load` spelt wrong, and a `mount` that returns no handle.
  `tsc` must reject each there with that code and nowhere else.
  `TEHAS_TSC` names the `tsc`; otherwise one on `PATH` of that version is
  used. With neither the test says `SKIPPED` on stderr and passes, except
  when `CI` or `TEHAS_TEST_STRICT` is set to anything, `false` too, where it
  fails, as a missing shellcheck fails the shell tests. The gate's workflows install npm's
  `@typescript/typescript-linux-x64` 7.0.2, checked by its sha256, and name
  it. By hand, in `console/`:
  `TEHAS_TSC=/path/to/tsc cargo test --locked -p tehas-ops update::types`.
- The markup and the CSS of today's pages are pinned as text by
  `test/front_page_test.sh` and `test/theme_test.sh`. A change to a page
  changes them in the same commit.
- `test/live_fixtures_test.mjs` runs the page code against the samples in
  `test/fixtures/live/`: each sample through the views and the island models
  that read it. It fails when a page fetches or names a file that has no
  sample.
- The byte ratchet runs with the workspace's tests and needs neither node nor
  the network.
