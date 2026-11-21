# Spec: plugins, views on the web

**Status**: Draft 2026-11-21. Part of the plugins spec, `2026-11-21-plugins.md`, which holds the terms, the settings and the manifest. The sign-in, the browser's policy and the proxy, the views' widgets and actions. Nothing is built.

## How a view is shown

After a good run, tehas swaps `/out` into
`~/tehas/plugins/<name>/view/` in one rename. A failed run leaves the last
good view in place.

The edge router serves each view under a name one level deep, as the
previews and the Protolab do: `plugin-<name>.<domain>`, or
`plugin-<name>-<sub>.<domain>` with `TEHAS_EDGE_ONE_LEVEL=on`. It serves
files only: no plugin program answers a request in v1. The one route that is not a file, `/_actions/`, goes to tehas's own gate (see "Actions").

Each view becomes a menu item through `tehas panel set
plugin-<name>-<view id> --label <label> --url https://<view name><path>`,
written by `tehas update` and taken out when the plugin goes. A panel is on
another origin than the dashboard, as `tehas panel` requires. Panels are at
most 4 in all, a person's and the plugins' together; a plugin that would make
a fifth is refused with 78 at `tehas update`. A factory fronted by
tehas-industry may show the same item through the host bridge's `menu`
instead; that is industry's choice and needs nothing more from tehas.

**A view is never public.** Every address under a plugin's name is behind
the sign-in below.

## Sign-in: GitHub is the gate

Every request to a plugin needs a signed-in person who has access to the
factory's repository, `TEHAS_REPO`, on GitHub. There is no other way in.

- **One gate for every address.** The edge sends every request for a
  plugin's name to `tehas-plugin-gate`, which answers all of them: the
  view's files, `/_kit/`, `/_actions/`. No file server answers a plugin's
  name directly. A request without a good session gets the sign-in, never
  a file.
- **No stray address.** No signed or secret URL, no token in a query
  string, no share link, no export link, no address of the view's files
  on another name. A plugin's view has nothing that works when copied out
  of a signed-in browser.
- **GitHub's web flow, through the factory's GitHub App.** The App's client
  id and secret are kept as its key is, in `~/.config/tehas`, and never reach
  a plugin. A factory without `TEHAS_GITHUB_APP_ID` cannot grant `view` or an
  `action:`: `tehas update` refuses them with 78. There is no token fallback.
- **One place to sign in.** GitHub calls back to one name,
  `plugins.<domain>` (`plugins-<sub>.<domain>` with `TEHAS_EDGE_ONE_LEVEL=on`).
  It hands each plugin's name a one-use code, which that name trades for a
  session of its own, as industry's landing site hands sessions to its host
  sites.
- **Who gets in.** The person's permission on `TEHAS_REPO`, asked of GitHub
  with the installation token, must be at least `TEHAS_PLUGIN_ACCESS`:
  `read`, `triage`, `write`, `maintain` or `admin`. The setting has no
  default: with any plugin that has `view`, an `action:`, a `skill:` or a
  `tool:` granted and no `TEHAS_PLUGIN_ACCESS`, `tehas update` exits 78.
- **Checked again.** At sign-in, every 10 minutes, and on every action. A
  person who lost access is signed out on the next check.
- **The session.** The person's GitHub token stays in the gate's memory
  only, never on disk and never given to a plugin. The browser holds a
  random id in a `__Host-` cookie, `HttpOnly`, `Secure`, `SameSite=Lax`,
  one per plugin name. A session lasts at most 12 hours, and a restart of
  the gate signs everyone out.
- **In the dashboard's dock.** A view that is not signed in shows a sign-in
  button, which opens the sign-in in a pop-up, as industry's wizard does in
  its frame. GitHub's pages cannot be framed.
- **Actions.** A press needs the session, an `Origin` of the plugin's own
  name, and the permission checked on that press. The run records the login
  that pressed it.
- **Chats.** `tehas plugin skills` and `tehas plugin call` take
  `--login <login>`, the person the chat asks for. tehas checks that login's
  permission the same way before it lists or calls anything. Industry signed
  the person in; the factory still checks.
- **Hooks** run for tehas itself, at a stage, and no person reaches them.

## Talking to the outside: the factory is the proxy

A view's page reaches nothing but its own name. What it needs from
elsewhere, the factory fetches for it. So a plugin author cannot put a
script, a style, a font or a tracker of a third party into a person's
browser, and a page cannot send what it shows anywhere.

### What the browser is told

The gate sends every answer of a plugin's name with this policy, made by
tehas, never by the plugin:

```
Content-Security-Policy: default-src 'none'; script-src 'self' <hashes>;
  style-src 'self' <hashes>; img-src 'self' data:; font-src 'self';
  connect-src 'self'; frame-src 'self'; form-action 'self';
  base-uri 'none'; object-src 'none'; frame-ancestors <dashboard origin>
```

with `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer` and
`Cross-Origin-Opener-Policy: same-origin`. Nothing but `'self'`: no CDN, no
`unsafe-inline`, no `unsafe-eval`, no `blob:` script.

- **Inline code by hash.** A page that holds its scripts and styles inline,
  as Floorplan's one-file export does, gets each block's sha256 in the
  policy. tehas reads the blocks when it swaps a good run's `/out` into the
  view and works out the hashes itself; the plugin cannot name one. An
  inline event handler (`onclick=`) does not run.
- **A page that names another site** in a `<script src>`, a `<link>` or an
  `@import` still loads nothing from it, and the swap logs each such address
  so the author sees why.

### The proxy

A view that needs outside data asks its own name:
`GET` or `POST /_proxy/<host id>/<path>`. The gate makes the request
itself, server side, after the sign-in.

- **Only granted hosts.** The manifest names each in `net`, with an id, an
  `https://` origin and the path prefixes it may use. A person grants each
  one, `net:<host id>`. Any other host, any other path: 403.
- **Nothing of the person goes out.** The gate sends no cookie, no
  `Authorization`, no `Referer` and no login. It adds only `User-Agent:
  tehas-plugin/<name>`. A host that needs a key gets it from the factory's
  secrets, `TEHAS_PLUGIN_NET_KEY_<NAME>_<HOST ID>`, which the gate adds, and
  which the page and the plugin never see.
- **Nothing that runs comes back.** An answer is passed on only as JSON,
  plain text, or an image of PNG, JPEG, GIF or WebP, by its content type
  and its first bytes. HTML, script, CSS, SVG and anything else is
  refused with 502. Every answer goes back with
  `Content-Security-Policy: sandbox; default-src 'none'`, no `Set-Cookie`,
  and the type the gate checked.
- **No way into the host's network.** The host's name is resolved by the
  gate, and an address that is loopback, private, link-local or the
  factory's own is refused, after each redirect too. At most 3 redirects,
  each to a granted origin.
- **Bounded.** 10 seconds, 2 MiB an answer, 64 KiB a request body,
  `TEHAS_PLUGIN_NET_MINUTE` requests a minute for a plugin. Each request is
  a crossing (see "Crossings"): logged with the login, the host id and the
  status, and its body only when the instance keeps them.

### Runs

A run's container keeps `--network none`, with only the model proxy when
`model` is granted. A run that reaches another host through the same proxy
is reserved for a later release, under the same `net:` grants.

## Widgets in Solid

The dashboard draws its widgets in Solid 2 on the tehas kit, `kit 1` in
`host/fun/kit/` (the island contract is in `host/docs/frontend.md`). A
plugin's view may use the same framework, so its widgets behave as the
dashboard's do. With `"ui": {"kit": 1}`:

- The edge serves the installed tehas's kit, read only, at `/_kit/` under
  the plugin's view name: the files the kit's `MANIFEST` lists, nothing
  else. The view imports `/_kit/vendor/solid.js`, `/_kit/island.js` and the
  rest from there.
- The view keeps the island contract: `.model.js` pure, `.render.js` the
  only file that imports Solid, Solid 2 APIs only.
- `tehas update` refuses a plugin, with 78, whose `ui.kit` the installed
  tehas does not serve.

The plugin ships no copy of the kit or of Solid. For `checkJs` types, its
build reads `vendor/solid.d.ts` from a tehas release tag.

Widgets inside the dashboard's own pages are not in v1: a plugin's code
would run on the dashboard's origin.

## Actions

A view may show a button for each of its manifest's `actions`, pressed with
the kit's `press()`: `POST /_actions/<id>` on the view's name. The edge
passes it to `tehas-plugin-gate`, a small Rust program on the host whose
only work, past the sign-in, is to ask for a run, as `tehas plugin run`
does. An action whose
`action:<id>` is not granted answers 403, and the view hides its button. It answers 202
when it asked, 409 while a run is going, and 429 within 10 minutes of the
last ask. It takes no body and runs nothing itself.

Only a signed-in person with access can press, and a press costs at most
one run, one at a time.
