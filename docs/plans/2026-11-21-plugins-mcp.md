# Plan: plugin tools on the MCP server

The spec is `docs/specs/2026-11-21-plugins-mcp.md`, revision 1.2.0 of the
plugin specification. This plan says how it is built and in what order.

## Before it starts

- The spec has merged (tehas-03 reviews and merges it).
- v2 Work 4, Tools, has merged: `tool:`, `tehas plugin call`, the call's
  container and `test/fixtures/argv/plugin-call`. pi-rukas-42 owns it. The
  broker reuses that call function, so nothing here starts before it.
- v2 Work 4 itself waits on the v1 container work, Work 3 (3b is #2674).
- A tool has answered a chat on a host.

## Pull requests

Each is a security change and says so.

### 1. The capability

- `console/core/src/plugin/manifest.rs`: `mcp:<id>` in `asks`, refused when
  `<id>` is not in `tools`. Schema `api/plugin-manifest.schema.json` and one
  sample.
- `console/core/src/settings/rules.rs`: a grant of `mcp:<id>` needs
  `tool:<id>` on the same line.
- `tehas plugin review`: the capability's row.
- Tests: a case per refusal, beside the code.

### 2. The broker

- `console/ops/src/plugin/calls.rs`: `tehasbot plugin calls`. A unix socket
  at `~/tehas/run/plugin-calls/calls.sock`, directory mode 700. `GET /tools`
  and `POST /call`, nothing else.
- What it serves is a pure function of the settings, the manifests at the
  pins and the holds: `served(&Settings, &[Manifest], &Holds) -> Vec<Tool>`.
  Tested without a socket.
- A call goes through the same function as `tehas plugin call`, with
  `who = Mcp(key_id)` in place of a login. The run row gains that form.
- Rates: 2 per plugin, 1 per key at once, 60 per key a minute. A clock is
  passed in so the tests do not sleep.
- `host/systemd/tehas-plugin-calls.service`; `install.sh` installs it;
  `tehas update` starts or stops it by the settings.
- Tests: Rust, a case per path, held, ungranted, old pin, each rate. One
  shell case for the unit's install, if `install.sh` needs one.

### 3. The server

- `console/ops/src/mcp.rs`: the mount of `~/tehas/run/plugin-calls` at
  `/run/tehas/calls`, only when the broker is to run.
  `test/fixtures/argv/mcp-plugins` pins it, and a test keeps the argument
  list without it unchanged.
- `console/mcp/src/plugins.rs`: the broker client, the list and the call,
  the answer's marking.
- `console/mcp/src/tools.rs`: `list_scoped` and `serves` take the plugin
  tools for `Scope::Full` only.
- `host/docs/mcp-server.md` and `host/docs/plugins.md` change with it. A
  `changelog.d/` fragment.
- Tests: a stub broker on a socket in a temp directory. List and call per
  scope, an argument over 2 KiB, a failed tool, a broker that is not there.

### 4. On a host

On a test instance with one plugin granted `tool:` and `mcp:`: an outside
agent calls it with a full key; a dashboard key and a held plugin are
refused. Then `host/docs/features.md` says so.

## Who does it

tehas-bd builds 1 to 3 after the spec and v2 Work 4 have merged, and asks
the owner for 4.
