# Spec: plugins, tools on the MCP server

**Version**: part of the plugin specification 1.3.0, `2026-11-21-plugins-1.0.0.md`.

**Status**: Draft 2026-11-21. Builds on the v2 chats part,
`2026-11-21-plugins-v2-chats.md`, and its tools. Nothing here is built, and
no code of it starts before v2's Tools work (v2 Work 4) has merged and a
tool has answered a chat on a host. Owner's decision: MCP is an extension
point of plugins. A plugin's tool can be offered to the agents outside the
host that reach the factory over MCP.

## Purpose

The factory's MCP server, `tehas-mcp`, answers outside agents with what the
factory knows. With this part, a plugin can add tools to it. Example: a docs
plugin's `find-doc` tool answers an outside agent's question, as it already
answers Obeya's chat.

Only tools. A skill, a view, a gate, a hook or a service is never reached
over MCP.

## The capability

| Capability | What it gives | What a person should weigh |
| --- | --- | --- |
| `mcp:<tool-id>` | The tool `<tool-id>` is listed and called on the MCP server, for full keys | Every holder of a full MCP key can call it, outside the sign-in and without a login. Its answer goes to an agent the factory does not run. A tool reading `/state` gives what the plugin keeps there to that agent |

`mcp:<tool-id>` needs `tool:<tool-id>`, granted for the same commit. A grant
of `mcp:` without its `tool:` is refused at load. The manifest asks for
`mcp:<tool-id>` in `asks`, and `<tool-id>` names one of its `tools`. A
manifest with `mcp:` for an id not in `tools` is refused. There is no new
manifest field and no new setting. With `TEHAS_MCP` off, or with
`TEHAS_PLUGIN` off, no plugin tool is served.

## On the server

- **Names.** A tool is listed as `plugin_<name>_<id>`, each `-` as `_`, as
  the chats name it. The server's own tools are `factory_*`, so the two
  never clash.
- **The list.** `tools/list` for a full key gives the server's tools and then
  each served plugin tool: `description` is `Plugin <label>, untrusted: `
  and the tool's `about`, and `inputSchema` is its `params`. The server asks
  the broker for the list on each `tools/list`, so a hold or a new grant
  takes effect without a restart.
- **Dashboard keys.** A dashboard key is never served a plugin tool. It is
  told about it the same way it is told about any tool it may not call.
- **A call.** The server checks the arguments against `params` and the 2 KiB
  cap, then sends the call to the broker. The answer comes back as one text
  content block holding
  `{"untrusted": true, "source": "plugin:<name>", "answer": <answer>}`, as
  the chats' hosts mark a tool result. A tool that failed or timed out is an
  MCP tool error, `isError: true`, with the reason the broker gave and no
  output of the tool.
- **The log.** The access line names the tool as `tool=plugin_<name>_<id>`.
  The arguments are never logged, as for every tool.

## The broker

`tehasbot plugin calls`, Rust in `console/ops/src/plugin/calls.rs`, run by
`tehas-plugin-calls.service`. It runs on the host, as the factory's user, and
serves `~/tehas/run/plugin-calls/calls.sock`. The directory is mode 700 and
made by the command, never by Docker.

It answers two requests and nothing else:

| Request | What it does |
| --- | --- |
| `GET /tools` | The served tools: every tool of a ready, unheld plugin granted `tool:<id>` and `mcp:<id>` at the current pin. Name, label, id, `about`, `params` |
| `POST /call` | `{"plugin", "tool", "args", "key"}`. Runs one call. `key` is the key's id, the first 8 hex characters of its sha256, as the access log names it. Never the key |

A call is the call of `tehas plugin call`: the same function, the same
`inspect` check, the same container (`test/fixtures/argv/plugin-call`, kind
`call`: no `product`, no `model`, no `net`, `/state` read only, `/out` a
tmpfs of 1 MiB), the same `timeout`, the same 16 KiB answer. The broker
starts nothing but that container. A tool not granted `mcp:` at the current
pin, a held plugin, or any other path is refused with 403 and one line on
stderr naming the plugin and the tool.

**Who called.** The run row holds `mcp:<key id>` where a chat's holds the
login, as `--by` names who asked for a hold. No login is checked: an MCP key
is not a person, and a full key already reads the logs of the repository.

**Rates.** At most 2 calls of a plugin at once, as in v2, and at most 1 per
key at once. At most 60 calls a minute per key across all plugins. Over a
rate the call is refused, `isError: true`, "busy". A call has no model, so it
spends no model budget. If a later revision gives a call `model`, the per-key
rate is where its budget is bounded.

**Trust.** The broker takes the scope on the MCP server's word: it does not
see keys. A process that can write to the socket can call any tool granted
`mcp:`, which is what a full key can do already. The socket is mounted into
`tehas-mcp` only, and nothing else is given it.

## Changes to the MCP server

A security change. `host/docs/mcp-server.md` changes with the code.

| Where | Was | Is |
| --- | --- | --- |
| What the server is | Read only. It reads three directories, calls nothing and runs nothing | Read only for the factory. No tool changes the factory, its repository, the store or a plugin's files. A full key can run a granted plugin tool, which runs a plugin's code in a container on the host, with that call's limits |
| What it calls | Nothing: not GitHub, not Docker, not git, not a model | The broker's socket, for a plugin tool, and nothing else |
| The container's mounts | `~/tehas/live`, `~/tehas/mcp`, `~/tehas/logs`, read only, and the binary | The same, and `~/tehas/run/plugin-calls` at `/run/tehas/calls`, only when `TEHAS_PLUGIN=on` and at least one plugin is granted an `mcp:`. Its argument list is pinned beside the other plugin argument lists, `test/fixtures/argv/mcp-plugins` |
| Its network | `tehas-mcp`, no masquerade | Unchanged. The socket is a file, not a network |
| `tools/list` | The server's own tools | Those, then the served plugin tools, for a full key |

Unchanged: the keys and their two lists, the 401 without a key, the access
log's fields, the 64 KB request body, the image, the user, the limits of the
container. No route of the server reaches a skill, a view, a gate, a hook or
a service.

## Changes to the plugin parts

| Part | Was | Is |
| --- | --- | --- |
| v2, "What v2 ships" | Tools in chats, `tool:<id>` | And on the MCP server, `mcp:<id>` (this part) |
| v2, the manifest's `asks` | The v2 capabilities by name with their id | Also `mcp:<id>`, naming a tool |
| v2 inspection, crossings | A tool's arguments and answer: `tehas plugin call` | The same, also through the broker. One check, the same function |
| v2 inspection, what is kept | Never through MCP | Unchanged: crossing rows are never served over MCP. A row of a call through the broker names `mcp:<key id>`, never a login |
| v2 rule 5 | Nothing of a plugin reaches a cycle's agents | Unchanged. A cycle has no MCP key and no route to the server |

## Commands

| Command | What it does | Exits |
| --- | --- | --- |
| `tehasbot plugin calls` | Serves the broker's socket until stopped | 0 stopped; 78 off or a bad setting |
| `tehas plugin review <name>` | Also prints, for each `mcp:` asked, the text of the capability's row above | as v1 |

`install.sh` installs `tehas-plugin-calls.service` and never switches it on.
`tehas update` enables and starts it when `TEHAS_PLUGIN=on`, `TEHAS_MCP=1`
and a plugin is granted `mcp:`, stops it otherwise, and restarts `tehas-mcp`
when the mount changed.

## Enforced in one place

| Rule | Where | Test |
| --- | --- | --- |
| `mcp:` needs `tool:`, names a tool | `console/core/src/plugin/manifest.rs`, `settings/rules.rs` | a case per refusal |
| What the broker serves and refuses | `console/ops/src/plugin/calls.rs` | a case per path, held, ungranted, old pin, rates |
| The call's container | `sandbox.rs` | `test/fixtures/argv/plugin-call`, unchanged |
| The MCP container's mount | `console/ops/src/mcp.rs` | `test/fixtures/argv/mcp-plugins`, and the argument list without it unchanged |
| A dashboard key never sees a plugin tool | `console/mcp/src/tools.rs`, `serves` | list and call cases per scope |
| The answer's marking | `console/mcp/src/plugins.rs` | a snapshot |

## Later

Each stays refused by name until a plugin needs it:

- **Skills as MCP prompts.** A granted skill is instructions for tehas's own
  chats. Serving it to an agent tehas does not run is a different grant.
- **A view's files as MCP resources.**
- **A plugin reading the factory through MCP** (`factory-read`,
  `factory-sessions`), as the v2 handover lists.
- **Plugin tools for dashboard keys**, or a third list of keys for them.
- **A tool with `model`** over MCP, bounded by the per-key rate.

## Open points

1. Whether 60 calls a minute per key is right. The first plugin to ask for
   `mcp:` answers it.
2. Whether an outside agent should see which commit a tool runs, in its
   description. Off by default: it names the pin, which is not secret, but
   adds nothing an agent acts on.

## Work

After v2 Work 4 (Tools) has merged and run on a host. Each is a pull
request; each is a security change.

1. **The capability.** `mcp:` in the manifest reader, the settings rules and
   `tehas plugin review`.
2. **The broker.** `tehasbot plugin calls`, its unit, its socket, the rates,
   the run row's `mcp:<key id>`.
3. **The server.** The mount, its argument list, `tools/list` and
   `tools/call` for plugin tools, the scope cases, the manual.
4. **On a host.** A tool called by an outside agent with a full key, refused
   to a dashboard key and to a held plugin. Then `host/docs/features.md`
   says so, and not before.
