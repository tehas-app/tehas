# Spec: plugins v2, reaching outside

**Version**: part of the plugin specification 1.0.0, `2026-11-21-plugins-1.0.0.md`.

**Status**: Draft 2026-11-21. Part of `2026-11-21-plugins-v2.md`. Nothing is
built.

A plugin still has no network. When it is granted an origin, tehas fetches
from that origin for it: for a view through the gate, for a run through a
socket. One check, one list of refused addresses.

## The capability

| Capability | What it gives | What a person should weigh |
| --- | --- | --- |
| `net:<id>=<origin>` | The view may ask `/_proxy/<id>/<path>`; with the entry's `run`, a run may ask the same through `/run/tehas/net.sock` | The origin learns the factory's address and what the plugin sends. With `product`, the code can leave through it |

The grant names the origin, not only the id, so a new pin whose manifest
moves `id` to another origin no longer matches its grant, and the plugin is
refused.

## The manifest

```json
"net": [{"id": "tiles", "origin": "https://tiles.example.org", "paths": ["/v1/"], "run": false}]
```

| Field | Rule |
| --- | --- |
| `id` | `a-z` and `0-9`, 1 to 12, unique in the plugin |
| `origin` | `https://` and a host name, no port, path, user or query. Not an IP literal |
| `paths` | 1 to 8 prefixes, each starting and ending with `/` |
| `run` | Optional, default `false`. `true` lets runs ask too. Needs `build` |

At most 4 origins.

## The fetch

`console/core/src/plugin/fetch.rs`, used by the gate and by `tehas-plugin`.

- **The address.** The host name is resolved once, every A and AAAA is
  checked, and the connection goes to the address that was checked, never a
  second lookup. Only global unicast is allowed: an address that IANA's
  special-purpose registries for IPv4 and IPv6 mark as not globally
  reachable is refused, and so are the host's own addresses and the Docker
  bridge ranges the host reports. One refused address refuses the name.
- **Port 443 only**, TLS with the system's roots.
- **No proxy from the environment.**
- **The path** is normalised (`..`, `%2e`, `//`, encoded slashes) before the
  prefix check, and the normalised path is what is sent.
- **Redirects** are not followed. A 3xx is passed back as 502.
- **What goes upstream.** Nothing the browser or the run sent but the path
  and, for a `POST`, the body. tehas sets `Host`, `User-Agent` and `Accept`
  itself, and a `Content-Type` for a body. No cookie, no `Authorization`,
  no `Referer`, no header of the person.
- **Methods** `GET` and `HEAD` from a view; `GET` and `POST` from a run, a
  body at most 1 MiB.
- **The answer.** Upstream headers are dropped but `Content-Type` and
  `Content-Length`. A compressed answer is decoded with a bound, and the cap
  of 8 MiB is checked after decoding.
- **Rate.** 60 requests a minute per plugin, and from a view 30 a minute per
  login too. Over it: 429, not sent.
- **Time.** 20 seconds a request.

## From a view

`GET /_proxy/<id>/<path>` on the plugin's own name, with the session and the
v1 web part's `Origin` and `Sec-Fetch-Site` rules. The answer never becomes a
document of the plugin's name: it is served only when `Sec-Fetch-Dest` is
`empty` or `image`, with `Content-Disposition: attachment`, the v1 headers
and `Content-Security-Policy: sandbox`. Any other `Sec-Fetch-Dest` is 403.

## From a run

With `run: true`, `tehas-plugin` serves `/run/tehas/net.sock` for the run,
beside the model's socket, as `tehas-mcp egress` serves the outsource
contractor (`console/mcp/src/egress.rs`). `POST /fetch` with `{"id",
"method", "path", "body"}`; the same fetch and caps. The run's argument list
gains the socket's directory only with the grant;
`test/fixtures/argv/plugin-net` pins it.

The socket answers `/fetch` only. It is no HTTP proxy and takes no
`CONNECT`: a tunnel would hide the path, the headers and the decoded size
that the rules above check. A plugin that runs a browser in its container
puts a proxy of its own beside it, which ends the browser's TLS with a key
made in the container and turns each request into a `/fetch`. Every request
then passes the same rules, and the rate counts each one.

## Enforced in one place

| Rule | Where | Test |
| --- | --- | --- |
| The address, path, redirect and answer rules | `fetch.rs` | a case per refused range, a stub upstream |
| The proxy route | `tehas-plugin-gate` | `Sec-Fetch-Dest` and header cases |
| The run's socket | `sandbox.rs` | `test/fixtures/argv/plugin-net` |
