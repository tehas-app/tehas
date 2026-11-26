# Plugin specification: a view's network

**Status**: DRAFT, 2026-11-25. Split out of the plugin specification
2.0.0 (`2026-11-25-plugins-2.0.0-operator.md`, "the operator spec") at its
revision 7 (decision 65). It waits for the browser spike (H8, the operator
spec's part 9). Nothing here is built, and nothing here is part of 2.1.
Until this spec lands, every view's frame is `connect-src 'none'` and no
view has `net:`.

**The other files.** "The operator spec" is the normative file for 2.1,
`2026-11-25-plugins-2.0.0-operator.md`. Its section numbers are kept in
`2026-11-25-plugins-operator-floating.md`, which holds parts 1.2, 6 to
6.3 and the other floating parts this spec cites, and the decisions are in
`2026-11-25-plugins-2.0.0-history.md`.

**Normative**: tehas-standard v3, `v3/plugins.md` at main 8257b66: a
view's network only through the host, over its port, with `connect-src
'none'` always (§14); `parent`, with reach apart from authority (§6);
destination patterns, which a host may leave out until it has `net:`
(§6). Where the two differ, the standard holds and this spec is fixed.

**Why it's apart.** Three reviews of revision 6 found that its view proxy
couldn't work as written: an opaque-origin frame can't read a
cross-origin answer without a CORS answer, a load token can't be bound to
a session when no cookie travels, and `Sec-Fetch-*` headers describe a
request, not a frame. The owner chose to carry a view's requests over
the port instead (decision 68), and to build none of it in 2.1.

## 1. Requests over the port

The owner: "can we limit client side https access to be via the proxy
too? via the web component or some other encapsulation", then "requests
should be normal https, but be proxied" (decision 60).

- **The frame never connects anywhere.** Its CSP keeps `connect-src
  'none'`, and `img-src`, `media-src` and `font-src` stay at the per-load
  file path. There's no proxy address in the frame's policy, no load
  token and no CORS answer.
- **Authors still write `fetch("https://...")`.** The kit's `fetch` shim
  maps the URL to an origin id through the plugin's destinations, and a
  relative URL to `parent` (3). An unmapped URL throws a named error. The
  shim sends the request over the load's `MessagePort` as a typed
  `request` message (its origin id, method, path, query, allowed headers
  and body), and turns the host's `response` messages back into a
  `Response`. Code that goes around the shim meets `connect-src 'none'`.
  There's no `XMLHttpRequest` or WebSocket shim.
- **The host element makes the request.** `<tehas-plugin-view>` is core's
  code in the dashboard's own origin. It sends each request to the
  dashboard's request gate endpoint with the dashboard's own session and
  its CSRF token, as any dashboard request. The gate checks the session's
  current standing and the plugin's grants, then relays the request to
  the view egress process (4), which runs the request gate of the
  operator spec's 5.1 and makes it.
- **What comes back.** The answer's status, the answer headers the
  allow-list keeps, and the body, streamed back over the port in chunks
  under the operator spec's 10.2 caps. No `Set-Cookie`, `WWW-Authenticate`
  or `Location` reaches the frame, and a redirect is followed on the
  server side within the routes, or refused.
- **No cookie and no `Authorization` from the view.** The view can't name
  a header outside the allow-list. The dashboard's own cookie is
  `SameSite=Strict`, its state changes need the CSRF token and never use
  `GET`, and its `frame-ancestors` names only the hosting apps.
- **What a view holds can leave.** `connect-src 'none'` stops requests,
  not every channel: a frame can still navigate itself, use a meta
  refresh or a link, or try WebRTC, STUN or DNS prefetch. Hence:
  - the sandbox has no `allow-top-navigation`, `allow-popups` or
    `allow-forms`;
  - the host element destroys the frame on its second `load` event, so a
    navigation away ends the view;
  - a view credential (5) is granted only to our own signed plugins, never
    to a third-party view;
  - the spike tests WebRTC and DNS prefetch under the frame's policy, and
    this spec lists what each browser lets through.
- **Budgets per plugin and person-session,** in a rate pool apart from the
  core's, so a view that reloads can't drain the installation's limit.
- **The grant is `net:`.** This re-scopes tehas-factory#2761 to this
  design. It floats: it's built when a plugin needs it.

## 2. Destinations for `net:`: patterns and the Public Suffix List

The operator spec's 5.1 allows exact origins only in 2.1. With `net:`:

- **Patterns only in host configuration,** in
  `TEHAS_PLUGIN_ORIGINS_<NAME>`. The one form is a leftmost `*.` label, as
  in `https://*.example.com`. Never a bare `*`, never a pattern in the
  scheme or the port, and never one whose rest is a public suffix
  (`https://*.com` and `https://*.co.uk` are refused).
  - Public suffixes are read from a copy of the Public Suffix List pinned
    in the release by its upstream commit and sha256, both sections, ICANN
    and private. It changes only by a pull request that moves the pin.
  - The settings load checks the value's form and stops with 78 on a bad
    one (hard rule 3: it reads no list and makes no call). The programs
    check patterns against the pinned list before any request, and stop
    with 78 on a public-suffix pattern.
- **Matching** is on the IDNA-normalised host, compared whole, and a
  pattern matches exactly one or more labels to the left of its rest. The
  address checks of the operator spec's 10.3 still run on every
  connection.
- **Widening.** For an ordinary plugin under `net:` and an annex plant,
  the person may add origins beyond the vetted ones, as their recorded
  choice, shown in `tehas status`. An added origin takes only `GET` and
  `HEAD` with no body, and never a credential.
- **One list for both sides.** A plugin's destinations govern its
  driver's requests under `net:` and its view's alike, and narrowing
  closes an origin for both.

## 3. The parent

The owner: "the hosting parent should always be allowed (e.g. calling
internal apis from the obeya/industry/factory or other annexes)"
(decision 63).

- The reserved origin id `parent` is the app the plugin runs in: the
  factory's dashboard, a managing system, Obeya or an annex. It needs no
  manifest entry, a manifest may not declare it, and the host's
  configuration can't narrow it away.
- **Reach is not authority** (the standard's §6). `parent` is always
  reachable, but each route the hosting app publishes for plugins names
  the grant it needs, and a route not on the list is refused. A request
  runs at the lesser of the person's standing and the plugin's grants.
- **No writes by request in 2.x.** A view writes to the parent only
  through a persisted `action` with the person's confirm (the operator
  spec's 6.3). A `parent` request by `fetch` is a read.
- The plugin gate serves `parent` from a canonicalised route list: a path
  is decoded exactly once, and `..`, `%2e`, `%2f` and `%5c` are refused
  before matching. It never goes through an egress host. An adapter's
  `http.request` to `parent` is refused.
- On the server side, a driver reaches the parent through the core's
  sockets (its host functions and surfaces), never by network.

## 4. The view egress user

- A view's requests leave through a process of their own, run as
  `tehas-vx-<id>` (named as the operator spec's 10.5 says), never through
  the adapter host. It holds no write token and no adapter credential. Its
  own client certificate is scoped by the upstream to the classes `none`
  and `github-read`. It runs the same request gate, with the operator
  spec's 5.3 fences.
- The plugin gate only relays to its socket, and never connects out
  itself.

## 5. GitHub GraphQL reads from a view

The owner: "the client might want to do some direct graphql requests to
github and that should be fine" (decision 61). This is the one exception
to "no credentials for a view", and only for our own signed plugins.

- **The view credential.** The vetted entry marks `api.github.com` with
  `view_credential: "github-read"`. No other origin may carry a view
  credential in 2.x. The view egress process attaches the factory's
  installation token, minted read-only (`contents`, `issues`,
  `pull_requests` and `metadata` read) and narrowed to the instance's one
  repository. The token never reaches the browser, and no record keeps it.
  - On the direct path, `github-read` needs core's narrowed mint
    (tehas-factory#3338).
  - On the upstream path, `github-read` is a class the upstream must
    define in a later protocol version; until then free-form view GraphQL
    is refused there.
- **Free-form reads only, through one gate.** A view may send its own
  GraphQL to `https://api.github.com/graphql`, by `POST` only. The gate
  parses each document and requires exactly one operation, of type
  `query`. Mutations, subscriptions, batches and `GET` are refused. It
  re-serialises the document canonically and sends that text. A client's
  own `rateLimit` field, and aliases that collide, are refused. Limits:
  depth 10, 5,000 nodes, a cost of 500 for each request by a
  `rateLimit(dryRun: true)` check first, and a budget for each
  person-session.
- **REST under this credential** is limited to `GET` on the routes the
  vetted entry lists for views, each with its allowed query keys.
- **A view writes nothing to GitHub in 2.x** except through the persisted
  actions of the operator spec's 6.3.

## 6. WebAssembly workers

An opaque-origin frame can't start a module worker from the gate's URL
the way revision 6 said; only a `blob:` worker starts, and it inherits the
frame's CSP (R2 H1). The spike decides the worker's box. Until then a
worker runs under the frame's own CSP, `connect-src 'none'` included, and
gets a module's bytes by `postMessage`.

## 7. Laws

- a direct fetch from the frame is blocked by `connect-src 'none'`;
- an unmapped URL is refused by the shim;
- a request with no dashboard session, or without its CSRF token, is
  refused by the gate, and so is one after the person signs out or loses
  standing;
- a sibling load's port can't send a request for another load;
- an origin that takes a credential is refused, `github-read` for our own
  signed plugins aside, and a third-party view never gets it;
- a private address is refused;
- a `Set-Cookie`, a 3xx or a `Location` never reaches the frame;
- the frame is destroyed on its second `load`;
- a free-form mutation, a batch and a `GET` to `/graphql` are refused;
- a client `rateLimit` field and colliding aliases are refused;
- a query over the cost, depth or node limit is refused;
- the mint for `github-read` names exactly the one repository and the
  read permissions (a law of the mint, not of GitHub);
- the token never appears in the frame or in a record;
- a view that reloads draws on the same person-session budget;
- a pattern in a manifest is refused; `*` and `https://*.com` in the
  setting are refused; a widened origin shows in `tehas status` with no
  credential;
- an unexposed parent route, a route whose grant the plugin lacks, and a
  path with `..`, `%2e`, `%2f` or `%5c` are refused; a `parent` write by
  request is refused;
- a view's request never reaches the adapter host or takes the class
  `adapter`.

## 8. Threats

| Threat | Rule (section) | Law or test | Evidence |
| --- | --- | --- | --- |
| A view sends data out directly | `connect-src 'none'`, requests over the port (1) | 7's direct-fetch law | proposed |
| A view uses the dashboard's session beyond its grants | The host element makes requests; the gate checks standing and grants (1, 3) | 7's session and grant laws | proposed |
| A sibling frame speaks for another load | Requests only on the load's own port (1) | 7's sibling law | proposed |
| A view leaks what it holds by navigation, a link, WebRTC or DNS prefetch | No top navigation, popups or forms; the frame destroyed on its second load; view credentials for our own plugins only (1) | 7's laws; the spike's WebRTC and DNS prefetch test | proposed |
| A CSRF-style request rides the person's cookies | The frame has no cookie; the gate needs the CSRF token; `SameSite=Strict` (1) | 7's CSRF law | proposed |
| The plugin gate's relay is used as a deputy | It relays only to the view egress process, which holds no write token or adapter credential (4) | 7's adapter-host law | proposed |
| The plugin gate reaches the network | It only relays (4) | **OPEN**: a test that its unit can't connect out | proposed |
| A view reaches an intranet or metadata address | The request gate's address checks (1) | 7's private-address law | proposed |
| A token lands in the frame or a record | The token stays in the egress process (5) | 7's token law | proposed |
| A view writes to GitHub free-form | Reads only, one `query` (5) | 7's GraphQL laws | proposed |
| A view's query exhausts the rate limit | Depth, nodes, cost by dry run, a budget (5) | 7's cost law | proposed |
| A plugin calls a parent route it has no grant for | Each route names its grant; the lesser of standing and grants (3) | 7's parent laws | proposed |
| A write to the parent as the person | No writes by request; actions with a confirm only (3) | 7's parent-write law | proposed |
| An annex parent's API is abused | The annex's own route list and grants (3) | **OPEN**: a law run against each annex's published list | proposed |
| A view reads another private repository | The token narrowed to one repository (5) | 7's mint law | proposed |
| A pattern opens a public suffix | Patterns only in host configuration, checked against the pinned list (2) | 7's pattern laws | proposed |

## 9. For the spike to settle

- that a request over the port and its streamed answer work in Chromium,
  Firefox and Safari, with the frame at `connect-src 'none'`;
- what a `blob:` worker inherits, and whether a worker can have a policy
  of its own;
- which leaks (WebRTC, STUN, DNS prefetch) each browser allows from the
  frame;
- whether a browser delegates a device to an opaque-origin frame (H10).
