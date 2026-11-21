# Spec: plugins v1, views on the web

**Version**: part of the plugin specification 1.0.0, `2026-11-21-plugins-1.0.0.md`.

**Status**: Draft 2026-11-21. Part of `2026-11-21-plugins.md`. Nothing is
built.

A plugin's view is served at a name of its own, only to a person GitHub says
has access to the factory's repository. One program, `tehas-plugin-gate`,
answers every request for every plugin name. No other program answers them.

## Names

| Name | What answers |
| --- | --- |
| `plugin-<name>.<domain>` | A plugin's view, its kit and its actions |
| `plugins.<domain>` | The sign-in: GitHub's callback, the hand-off, sign-out |

With `TEHAS_EDGE_ONE_LEVEL=on` they are `plugin-<name>-<sub>.<domain>` and
`plugins-<sub>.<domain>`, as for previews (`host/docs/edge.md`). Plugin names
have no `-`, so two plugins never share a name under that rule.

- `view` and `action:` need `TEHAS_DOMAIN` and https in front: the session's
  `__Host-` cookie cannot exist on plain http. Without `TEHAS_DOMAIN`, a
  grant of either is refused at load with 78.
- **The router.** In `host/edge/router.Caddyfile`, `plugin-*` and `plugins*`
  are matched first in `route`, before `@look`, the previews and the
  Protolab, and go only to the gate with `reverse_proxy`. A name of a plugin
  that is not set is 404, "No plugin is set at this address.", as `@odd` is;
  the gate answers a well-formed name with no plugin set the same way, since
  the router does not read the settings. The gate down is a page of its own
  text, never the Protolab's or a preview's. `test/edge_test.sh` pins all
  three.
- The proxy in front, `host/docs/edge.md`'s matcher, lists names by hand: the
  two names are an adopter action, and so is a Cloudflare cache rule that
  bypasses the cache for both. `edge.md`'s one-level `Host` rewrite does not
  apply to plugin names; they have a matcher of their own.
- The gate checks the whole `Host` against the names its settings make, not
  the first label. Any other `Host`: 404.

## The gate

`tehas-plugin-gate`, a Rust program of the workspace, its own unit
`tehas-plugin-gate.service`, listening on `TEHAS_PLUGIN_GATE_ADDR`: a
private or loopback address literal with a port, never 8080 or 8081 (the
dashboard's), by default `TEHAS_HOST_ADDR:8095`, where the docs server
binds and the router already reaches it. Never `0.0.0.0` or a public
address. An IPv6 address is written in brackets, `[<address>]:<port>`. It is installed only
with the feature on.

Every answer, a file, an error, the sign-in page and a redirect, carries:

```
Cache-Control: private, no-store
Vary: Cookie
Cross-Origin-Resource-Policy: same-origin
X-Content-Type-Options: nosniff
Referrer-Policy: no-referrer
Permissions-Policy: camera=(), microphone=(), geolocation=(), usb=(), payment=(), clipboard-read=()
```

and never an `Access-Control-*` header. A request to upgrade the connection
(`Upgrade:`) is 404 until a revision adds the one WebSocket path (1.1.0's
relay). `OPTIONS` is 403 with no session
lookup. A test asserts no answer lacks these.

### Requests across sites

A preview, `uat-<pr>.<domain>`, is the same site as a plugin's name, so
`SameSite` alone does not keep a preview's script out.

- A request that is not a top-level navigation (`Sec-Fetch-Mode` other than
  `navigate`, or missing) needs `Sec-Fetch-Site: same-origin`. So does every
  `POST`.
- Every request but `GET` and `HEAD` needs `Origin` equal to the plugin's own
  name, and a missing `Sec-Fetch-Site` is refused.
- A navigation from another site gets the page, never runs an action.

### Paths

| Path | What it is |
| --- | --- |
| `/_kit/<file>` | A file of the installed tehas's `host/fun/kit/MANIFEST`, when the manifest has `ui.kit` |
| `/_actions/<id>` | A press, see "Actions" |
| `/_hooks/<pr>` | The findings of the plugin's last hook on that pull request, in a page the gate makes, every text escaped: a finding may quote names the model wrote |
| `/_session` | `GET`: 200 `{"signed": true}` or 401, for a frame to poll |
| `/_signout` | `POST`, see "The session" |
| `/_auth/ask`, `/_auth/take` | The plugin name's two steps of the hand-off, see "The sign-in" |
| anything else | A file of `view/`, `index.html` for a directory, served only when `view` is granted at the pin, the manifest asks for `view`, and the plugin's state is ready at the pin |

Paths starting `/_` are the gate's, whatever `/out` holds. A path is
normalised (`..`, `%2e`, `//`, encoded slashes) before it is matched, and a
path that leaves `view/` is 404.

A request without a good session: a navigation gets the sign-in page; any
other request 401 with no body. Never a file.

## The sign-in

GitHub's web flow, through the factory's GitHub App.

- **The App.** It needs a callback URL, `https://plugins.<domain>/_auth/callback`,
  and a client secret. `tehas setup --new-app` sets the callback in its
  manifest and keeps the client id and secret beside the key, in
  `~/.config/tehas/github-app.client-id` and `github-app.client-secret`, mode
  600; the gate exits 78 without them or with a wider mode. For an App made before, an adopter action:
  add the callback and generate a secret, then `tehas setup --app-client-id
  <id> --app-secret-file <file>`: the two go together, are refused with
  `--new-app`, and replace a different value only with `--force`. The App
  manifest names the callback whenever `TEHAS_DOMAIN` is set.
  A factory without the App, or without the secret, cannot grant `view` or
  `action:`: refused at load with 78. No token fallback.
- **One callback.** GitHub calls back to `plugins.<domain>`. That name hands
  each plugin name its session by a one-use code, as tehas-industry's landing
  site does (`crates/workshop/src/auth.rs`), with these rules:
  - The plugin name that asks is one of the settings' names, taken from the
    settings, never from `Host` or the address.
  - The plugin name's `/_auth/ask` sets a nonce in a `__Host-` cookie and
    sends the browser to `plugins.<domain>/_auth/pass?state=<nonce hash>&back=<path>`,
    which returns it with the code to the plugin name's `/_auth/take`.
  - `back` is a path only, normalised, under the plugin name.
  - The code lives 60 seconds, works once, and only for the name and nonce it
    was made for. It is the one value in an address; it opens nothing alone.
- **Who gets in.** After GitHub's callback the gate asks `GET /user` with the
  person's token, keeps the user id and login, and drops the token. It then
  asks `GET /repos/{TEHAS_REPO}/collaborators/{login}/permission` with the
  installation token and reads `user.permissions` (`pull`, `triage`, `push`,
  `maintain`, `admin`); `role_name` is shown in logs only, since a custom role
  has its own name. The person gets in when the permission is at least
  `TEHAS_PLUGIN_ACCESS`. A 404 is no access. A GitHub error at sign-in is a
  refusal, said as such on the page. The sign-in page answers 200.
- **A public repository.** GitHub gives `pull` on a public repository to every
  account. So `tehas update` reads the repository's visibility and refuses
  `TEHAS_PLUGIN_ACCESS=read` or `triage` while it is public, with 78 and the
  reason. The gate checks the same at every access check, and refuses access
  while it holds.
- **Checked again** every 10 minutes for a session, and on every press. A
  person who lost access is signed out at the next check. A GitHub error on
  the periodic check answers 503 and keeps the session; on a press it answers
  403. People are compared
  by user id, since logins are renamed and reused.

### The session

- A random id in a `__Host-tehas-plugin` cookie, `HttpOnly`, `Secure`,
  `SameSite=Lax`, `Path=/`, one per name. The gate holds the session in
  memory: user id, login, the time of the last check. No token.
- It ends after 2 hours unused or 12 hours in all. A restart of the gate
  signs everyone out.
- `POST /_signout`, with the `Origin` rule, ends that person's sessions on
  every plugin name and `plugins.<domain>`, clears the cookie, and answers 200
  `{"signed": false}`.

### In the dashboard's dock

A view is a panel: the dock's frame of `host/fun/panels.js`, which has
`allow-popups`. A frame that is not signed in shows a page with a plain link,
`<a href="https://plugins.<domain>/_auth/start?for=<name>" target="_blank">`,
and polls `/_session` every 3 seconds. The person signs in in that tab; the
frame sees the session and reloads. GitHub's pages cannot be framed, so the
sign-in never runs in the frame. The frame is the same site as the dashboard,
so its `SameSite=Lax` cookie is sent; a domain that is a public suffix would
make it cross-site, and then the dock's "Open in a new tab" link is the way.

Panels have no `open` in v1: a plugin's view always docks, and the dock's
link opens it in a tab.

## The browser's policy

Every page of a plugin's name has a `Content-Security-Policy` that tehas makes
for that document:

```
default-src 'none'; script-src 'self' <script hashes>;
style-src 'self' <style hashes>; style-src-attr 'unsafe-inline';
img-src 'self' data:; font-src 'self' data:; connect-src 'self';
frame-src 'self'; frame-ancestors 'self' <dashboard origin>;
form-action 'self'; base-uri 'none'; object-src 'none';
worker-src 'none'; manifest-src 'none'
```

with `Permissions-Policy: camera=(), microphone=(), geolocation=(), usb=(),
payment=(), clipboard-read=()` and `Cross-Origin-Opener-Policy:
same-origin-allow-popups`.

- **Hashes.** When a good refresh is swapped in, tehas reads each HTML file's
  inline `<script>` and `<style>` blocks and works out their sha256. Each
  document's policy names its own. The plugin cannot name one. A hash says
  tehas saw the block, not that it is safe.
- **Style attributes** are allowed (`style-src-attr 'unsafe-inline'`):
  Floorplan colours its cards with them, with computed values that cannot be
  hashed. A style cannot run code; what it could load, `img-src` and
  `font-src` bound to the view's own files and `data:`.
- **`frame-ancestors 'self'`** lets the view frame its own pages, as the
  reference plugin frames its map.
- **The export's own policy.** A page that also carries a `<meta>` policy
  gets both; the browser applies the stricter.
- **Styles added at run time.** A `<style>` element a script makes is not
  hashed and does not apply; a view sets styles as attributes or in its
  files.
- **A page that names another site** in a `<script src>`, `<link>` or
  `@import` loads nothing from it. The swap logs each such address.
  `<meta http-equiv="refresh">` and inline event handlers (`onclick=`) are
  refused at the swap, failing the run, and again when served, with 500.

What the policy does not stop: a page can navigate the frame or open a tab
with data in the address. So `view` with `product` means a plugin's page can
send what it shows to a site of its author's choosing when a signed-in person
opens it. `tehas plugin review` says so for that pair.

## The kit

With `"ui": {"kit": 1}`, the gate answers `/_kit/<file>` for each file the
installed tehas's `host/fun/kit/MANIFEST` lists and install puts there, read
from `~/tehas/fun/kit/`, behind the same sign-in and with the same headers.
The `.d.ts` files the MANIFEST lists are for a plugin's author and are not
installed, so they are 404 at the gate.
The view follows the island contract of `host/docs/frontend.md`. A tehas whose
kit is not `kit 1` refuses the plugin at update.

## Actions

A press is `POST /_actions/<id>` from the view, with the kit's `press()`.

- It needs a session, the `Origin` and `Sec-Fetch-Site` rules, the person's
  access checked on that press, and `action:<id>` granted.
- It asks for a refresh, as `tehas plugin run` does, and records the login.
- 202 asked; 409 a refresh is running; 429 within 10 minutes of the plugin's
  last press, a window the gate keeps in memory, so a restart clears it; 403
  not granted, no access, or a GitHub error on the check; 503 no runner; 401
  no session.
- `GET /_actions` answers `{"api": 1, "actions": ["refresh"]}`, the ids
  granted, so a view shows only those buttons.

## The panel

`tehas update` writes, for each plugin with `view` granted and ready,
`tehas panel set p-<name> --label <view.label> --url https://plugin-<name>.<domain>/`,
and clears it when the plugin goes or is refused. Ids starting `p-` are
reserved: a person's `tehas panel set p-...` is refused with 64, and
`tehas panel clear p-...` stays allowed. With the feature off, `p-` panels are
cleared. A view left out as a fifth panel prints a line; the update does not
fail.

`live/panels.json` is public, so a plugin's name and label are too. Its view
is not.
