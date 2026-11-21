# Spec: plugins v2, the menu, views and services

**Version**: part of the plugin specification 1.0.0, `2026-11-21-plugins-1.0.0.md`.

**Status**: Draft 2026-11-21. Part of `2026-11-21-plugins-v2.md`. Nothing is
built.

## The menu

v1 takes panels from the person's four, ids `p-<name>`. v2 gives plugins a
list of their own.

- `tehas update` writes `live/plugins.json`: each ready plugin with `menu`
  granted, its views, labels and addresses. `live/` is public, so names and
  labels are, as in v1. The views are not.
- Both front pages read it and show a **Plugins** section in the menu, below
  the person's panels. A view with `open: dock` opens in the dock; with
  `open: page` it opens in a new tab.
- A plugin with `menu` granted takes no `p-` panel. A plugin without it keeps
  v1's panel, so a v1 grant line means what it meant.
- `tehas panel` and the host bridge's `menu` stay as they are.

## More than one view

```json
"views": [
  {"id": "map", "label": "Floorplan", "path": "/", "open": "dock"},
  {"id": "health", "label": "Health", "path": "/health/", "open": "page"}
]
```

| Field | Rule |
| --- | --- |
| `id` | `a-z` and `-`, 1 to 16, unique |
| `label` | 1 to 24 characters |
| `path` | A directory of the view, starting and ending with `/`, each view its own; a path may be inside another view's |
| `open` | `dock` or `page` |

At most 4. Each needs `view:<id>`; v1's `view` grant is a single view at
`/`. A request belongs to the view with the longest path that holds it. If
that view is not granted, the request is 404: it never falls back to a view
with a shorter path, so a grant left out of `/health/` is not served through
`/`. A path no view holds is 404 at the gate.

## Widgets

A widget inside the dashboard's own pages would run a plugin's code on the
dashboard's origin. v2 does not have them. 1.3.0 changes this rule, a security change:
see "Changes to v2" in `2026-11-21-plugins-design.md`.

## `kind: "service"`

A plugin with a server of its own, behind the gate, as Obeya runs behind
industry.

| Capability | What it gives | What a person should weigh |
| --- | --- | --- |
| `serve` | The plugin's container runs while it is ready, and the gate sends its views' requests to it | The plugin's code runs all the time, not only in a run, and answers every signed-in person. It is not told who they are |

- The manifest gives `port` (1024 to 65535) and `health` (a path). `serve`
  needs `build`.
- The container runs as `tehas-plugin-service@<name>.service`, with the
  arguments of a v1 run, `--network none` as always, and one change:
  systemd restarts it, not Docker. The service listens on a unix socket,
  `/run/tehas/serve/http.sock`, in a directory bind-mounted from
  `~/tehas/plugins/<name>/serve/`, as the model's socket is, and the gate
  connects to that socket. The directory is setgid, mode 2770, with the host
  user's group, and the container gets `--group-add` of that group, so the
  service can make the socket and the gate can reach it. `test/fixtures/argv/plugin-service` pins it.
- The gate proxies the service's view paths to it, and sends no login and
  nothing else of the person: it strips every `X-Tehas-*`, `Authorization`,
  `Cookie`, `Proxy-*`, `Forwarded` and `X-Forwarded-*` header the browser
  sent and adds none. Who pressed is known to the gate's own session, as for
  a filed record (`record-file --by-id`). A later `identity` grant may give a
  service the login. Every rule of the v1 web part holds: the session,
  the headers, the request rules and the policy. A server's pages cannot be
  hashed ahead, so a service's documents get `script-src 'self'` with no
  hashes, which the browser already holds to no inline script.
- In v2 a service gets no `product`, `model` or `net`: tehas makes those for
  each run, and a service has no run. A later version adds each behind a
  grant of its own and says how it is counted.
- It has memory and pids limits of its own, smaller than a run's, set in
  `sandbox.rs` and pinned by the fixture. They count against
  `TEHAS_PLUGIN_LIMITS` all the time, and in admission by room, so a factory
  sees what a service costs.
- `/state` is read and write. `/out` is not used.
- `tehas plugin` shows the service's state, its restarts and its health.
