# Plugin specification: a view's network

**Status**: DRAFT, 2026-11-25, split out of the plugin specification
2.0.0 (`2026-11-25-plugins-2.0.0-operator.md`) at its revision 7. It waits
for the browser spike (H8, that spec's part 9). Nothing here is built, and
nothing here is part of 2.1. Until this spec lands, every view's frame is
`connect-src 'none'`.

## 1. A view's network

### 6.4 A view's network: normal HTTPS, always through the host's proxy

The owner: "can we limit client side https access to be via the proxy
too? via the web component or some other encapsulation", then "requests
should be normal https, but be proxied" (decision 60).

**Built only after H8's browser spike** (part 9) shows the frame, the CSP
and the proxy work as written. Until then 6.4 is specified and floats.

- **Normal HTTPS, to the proxy only.** The browser makes real HTTPS
  requests, but only to the host's proxy:
  `https://<dashboard>/_proxy/<load-token>/<origin-id>/<path>`. The frame's
  CSP sets `connect-src`, and `img-src`, `media-src` and `font-src` beyond
  the per-load file path, to exactly that prefix and nothing else, with
  `base-uri 'none'`. The browser blocks any other destination.
- **The kit's `fetch` shim.** Authors write
  `fetch("https://api.example.com/x")`. The shim maps the URL to an origin
  id through the plugin's destinations, and a same-origin or relative URL
  to `parent` (5.1), rewrites it to the proxy path, and calls the
  browser's own `fetch`. An unmapped URL throws a named error. Code that
  goes around the shim meets the CSP. There's no `XMLHttpRequest` or
  WebSocket shim in 2.x, and the CSP blocks both.
- **What a view holds can leave.** The CSP stops requests, not every
  channel: a frame can still navigate itself, use a meta refresh or a
  link, or try WebRTC, STUN or DNS prefetch. So anything a view holds may
  leave the browser. Hence:
  - the sandbox has no `allow-top-navigation`, `allow-popups` or
    `allow-forms` (H8);
  - the host element destroys the frame on its second `load` event, so a
    navigation away ends the view;
  - a view credential (`github-read`, below) is granted only to our own
    signed plugins, never to a third-party view.
- **The load token.**
  - It's minted for each load of the frame, scoped to the instance, the
    plugin, that load and its allowed origins.
  - It lives 5 minutes. The host element renews it with a heartbeat over
    the dashboard's own session.
  - It's bound to that session's id, and the proxy checks
    `Sec-Fetch-Site`, `Sec-Fetch-Mode` and `Sec-Fetch-Dest` on each
    request.
  - It's revoked on reload, or when the load's port is revoked (6.3), and
    it's never in a cookie. The frame sends `Referrer-Policy:
    no-referrer`, and its opaque origin means no cookie travels.
  - No `Access-Control-Allow-Origin` answer helps another origin. None is
    needed, since the frame is the only client.
- **No cookies, no CSRF.** The proxy refuses any request that carries a
  cookie. The dashboard's cookies are `SameSite=Strict`, its state changes
  need a CSRF token and never use `GET`, and its `frame-ancestors` names
  only the hosting apps.
- **The proxy calls the request gate** (5.1), on the plugin's one list of
  destinations. The proxy adds: no `Cookie` or `Authorization` from the
  view and no `Set-Cookie` back; never a 3xx or a `Location` to the frame,
  so a redirect is followed on the server side within the routes, or
  refused; records that never keep the path's load-token segment; and the
  view's budgets (below).

  The plugin gate relays the request over its socket to the view egress
  process, `tehas-vx-<instance>` (10.5), which runs the request gate and
  forwards through the upstream when one is set, and directly otherwise.
  The plugin gate never connects out, the adapter host never carries a
  view's request, and the operator has no network.
- **Budgets per plugin and person-session,** not per load, in a rate pool
  apart from the core's, so a view that reloads can't drain the
  installation's limit.
- **No credentials for a view,** with one exception below. A view reaches
  only origins whose `credential` is false. Attaching any other credential
  for a view is out of 2.x without a new decision.
- **Answers are streamed,** with the answer-header allow-list and the caps
  enforced while streaming.
- **The grant is `net:`.** This re-scopes tehas-factory#2761 to this
  design. It's specified now and built when a plugin needs it: it floats.
  The GitHub adapter doesn't depend on it.

#### GitHub GraphQL reads from a view

The owner: "the client might want to do some direct graphql requests to
github and that should be fine" (decision 61). This is the one exception
to "no credentials for a view", and only for our own signed plugins.

- **The view credential.** The vetted entry marks `api.github.com` with
  `view_credential: "github-read"`. No other origin may carry a view
  credential in 2.x. The proxy attaches the factory's installation token,
  minted read-only (`contents`, `issues`, `pull_requests` and `metadata`
  read) and narrowed to the instance's one repository, so GitHub itself
  lets only that repository, and public data, be read. The token never
  reaches the browser, and no record keeps it.
  - On the direct path, `github-read` needs core's narrowed mint
    (tehas-factory#3338): the mint's test is a precondition of building it.
  - On the upstream path, free-form view GraphQL is refused until the
    upstream's own spec pins a read-only, one-repository credential for
    the class `github-read`.
- **Free-form reads only, through one gate.** A view may send its own
  GraphQL to `https://api.github.com/graphql` through the shim, by `POST`
  only. The proxy parses each document and requires exactly one operation,
  of type `query`. Mutations, subscriptions, batches and `GET` are refused.
  - It re-serialises the document canonically, and costs and sends that
    same text.
  - A client's own `rateLimit` field, and aliases that collide, are
    refused.
  - Limits: depth 10, 5,000 nodes, a cost of 500 for each request, and a
    budget for each person-session. The proxy first asks GitHub for the
    canonical query's cost with `rateLimit(dryRun: true) { cost }`, and
    refuses one over the cap without running it.
- **REST under this credential** is limited to `GET` on the routes the
  vetted entry lists for views, each with its allowed query keys (10.10).
- **A view writes nothing to GitHub in 2.1.** A view's writes are only
  6.3's persisted actions, each carried out by a fixed call of the
  console's closed list (10.5). In 2.1 that list holds no write to GitHub
  for a view, and adding one is a change to this spec.

#### Laws

- a direct fetch from the frame is blocked;
- an unmapped URL is refused;
- an origin that takes the credential is refused, `api.github.com`'s
  view credential for our own signed plugins aside;
- a third-party view never gets `github-read`;
- a private address is refused;
- an expired load token, another load's, and one replayed from another
  session or with the wrong `Sec-Fetch-*` values are refused;
- a request that carries a cookie is refused;
- a `Set-Cookie`, a 3xx or a `Location` never reaches the frame;
- the frame is destroyed on its second `load`;
- a load token in a record is redacted;
- a free-form mutation from a view, a batch and a `GET` to `/graphql` are
  refused;
- a client `rateLimit` field and colliding aliases are refused;
- a query over the cost, depth or node limit is refused, and the text
  costed is the text sent;
- reading another private repository fails, because GitHub refuses it
  under the narrowed token;
- the token never appears in the frame or in a record;
- a view that reloads draws on the same person-session budget.


## 2. Destinations for `net:`: globs and the Public Suffix List

- **Globs only in host configuration.** The one form is a leftmost `*.`
  label, as in `https://*.example.com`. Never a bare `*`, never a glob in
  the scheme or the port, and never a glob whose rest is a public suffix
  (`https://*.com` and `https://*.co.uk` are refused).
  - Public suffixes are read from a copy of the Public Suffix List pinned in
    the release by its upstream commit and sha256, both sections, ICANN
    and private. It changes only by a pull request that moves the pin.
  - The settings load checks the value's form and stops with 78 on a bad
    one (hard rule 3: it reads no list and makes no call). The programs
    check globs against the pinned list before any request, and stop with
    78 on a public-suffix glob.
- **Matching** is on the IDNA-normalised host, compared whole, and a glob
  matches exactly one or more labels to the left of its rest. The address
  checks of 10.3 still run on every connection.

## 3. The parent

- **The parent is always allowed** (decision 63: "the hosting parent
  should always be allowed"). The reserved origin id `parent` is the app
  the plugin runs in: the factory's dashboard, a managing system, Obeya or
  an annex.
  - It needs no manifest entry, and the person's host configuration can't
    narrow it away. A manifest may not declare an origin named `parent`.
  - The parent decides what's exposed. Each hosting app publishes its
    plugin-facing route list: for the factory, the factory API's read
    routes and the surfaces; for any other, the hosting app's
    plugin-facing routes. A route not on the list is refused. "Always
    allowed" is the destination, not every endpoint.
  - On the client side, the kit's `fetch` shim maps a same-origin or
    relative URL (`fetch("/api/...")`) to `parent`, through the same proxy
    and the same request gate (6.4).
  - On the server side, a driver or an adapter reaches the parent through
    the core's sockets (its host functions and surfaces), never by
    network. That's the same `parent` destination.
  - The plugin gate serves `parent`, from a canonicalised route list: a
    path is decoded exactly once, and `..`, `%2e`, `%2f` and `%5c` are
    refused before matching. It never goes through the adapter host or the
    view egress process. An adapter's `http.request` to `parent` is
    refused.
  - The person's session cookie never travels. The gate authorises a read
    on an exposed route as the signed-in person's standing, with no cookie
    forwarded. A write needs a single-use confirm token that the host
    element mints after its confirm (6.3), bound to the method, the route
    and the body's hash.
  - View and `parent` traffic has its own load scope. 10.3's "no port call
    in flight" rule binds an adapter's requests only.

## 4. The view egress user

- **Views leave through an egress user of their own.** A view's proxied
  requests (6.4) are relayed to a separate egress process run as
  `tehas-vx-<instance>`, never to the adapter host. It holds no write
  token and no adapter upstream key. Its own client certificate is scoped
  by the upstream to the classes `none` and `github-read`. It runs the same
  request gate (5.1), with 5.3's fences.

## 5. Threats

| Threat | Rule (section) | Law or test |
| --- | --- | --- |
| A view sends data out directly | The frame's CSP holds it to the proxy prefix (6.4) | 6.4's law: a direct fetch from the frame is blocked |
| A load token is stolen or replayed from elsewhere | Scope, 5 minutes, bound to the session, `Sec-Fetch-*`, `no-referrer`, revocation (6.4) | 6.4's law: an expired, other-load, other-session or wrong `Sec-Fetch-*` token is refused |
| A view leaks what it holds by navigation, a link, WebRTC or DNS prefetch | No top navigation, popups or forms; the frame destroyed on its second load; view credentials for our own plugins only (6.4) | 6.4's laws: the second load destroys the frame; a third-party view never gets `github-read`; **OPEN**: a browser test of WebRTC and DNS prefetch under H8's spike |
| A CSRF-style request rides the person's cookies | The proxy refuses cookies; `SameSite=Strict`, a CSRF token, no `GET` for state changes, `frame-ancestors` (6.4) | 6.4's law: a request with a cookie is refused; **OPEN**: a test of the dashboard's cookie and CSRF rules |
| The plugin gate's relay is used as a deputy | It relays only to the view egress process, which holds no write token or adapter key, with a certificate scoped to `none` and `github-read` (6.4, 10.5) | **OPEN**: a law that a view's request can't reach the adapter host or take the class `adapter` |
| The plugin gate is used to reach the network | The egress hosts; the gate only relays (5.1, 6.4) | **OPEN**: a test that the plugin gate's unit can't connect out |
| A view reaches an intranet or metadata address | The proxy's address checks (6.4, 10.3) | 6.4's law: a private address is refused |
| A view uses the person's session | The opaque origin, no cookies, no credentials (6.4) | 6.4's laws: a credential origin is refused, and `Set-Cookie` never reaches the frame |
| A token lands in a log | Records never keep the load-token segment or the view credential (6.4) | 6.4's laws: a load token in a record is redacted; the token never appears in the frame or a record |
| A view writes to GitHub free-form | Reads only, one `query`; writes only through persisted actions with a confirm (6.4) | 6.4's laws: a free-form mutation is refused; a persisted write without a confirm is refused |
| A view's query exhausts the rate limit | Depth limit, cost cap by dry run, budget per load (6.4) | 6.4's law: a query over the cost cap is refused |
| A plugin calls a parent route that isn't exposed | The parent's plugin-facing route list (5.1) | 5.1's law: an unexposed parent route is refused |
| A CSRF-style write to the parent as the person | No session cookie; writes only after a host-drawn confirm (5.1, 6.3) | 5.1's laws: a parent write without a confirm is refused; a cookie never travels to the parent |
| An annex parent's API is abused | The annex's own plugin-facing route list and rules (5.1) | **OPEN**: a law run against each annex's published list |
| A view reads another private repository | The token read-only and narrowed to one repository (6.4) | 6.4's law: GitHub refuses the read under the narrowed token |
