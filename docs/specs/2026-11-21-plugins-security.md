# Spec: plugins, the security model

**Status**: Draft 2026-11-21. Part of the plugins spec, `2026-11-21-plugins.md`, which holds the terms, the settings and the manifest. The sandbox, the grants, the host's model and the inspection of what crosses. Nothing is built.

## The security model

A plugin is someone else's code on the factory's host. It starts with
nothing, and a person grants each thing it may reach, by name, in the
instance's settings. A grant holds for one plugin; a new pin that asks for
more stops until a person grants it.

### The base, for every plugin

What every run has, with no grant, and nothing more:

- One container, started by `tehas-plugin@<name>.service`, as a user that is
  not root, with `--read-only`, `--cap-drop ALL`,
  `--security-opt no-new-privileges`, a `/tmp` on tmpfs, and limits on
  memory, CPU and processes from the manifest's `limits`, capped by the
  instance's.
- `--network none`.
- `/out`, which it writes, and `/state`, its own files between runs under
  `~/tehas/plugins/<name>/state`. No other plugin's.
- The run's time limit, `timeout`.

Its output stays on the host until `view` is granted.

### Capabilities

A manifest's `asks` lists what the plugin wants. Each is off until granted.

| Capability | What it gives | Why a person should think |
| --- | --- | --- |
| `view` | `/out` is served at `plugin-<name>.<domain>` and its views become menu items | What the plugin writes is shown to everyone the sign-in lets in |
| `product` | `/product`, read only: a clean checkout of the default branch's head, made for the run and removed after it, with no remote and no credentials | The plugin reads all of the product's code. With `view`, what it writes about it is shown to the people the sign-in lets in |
| `model:<budget>` | The host's model, through `tehas-proxy` only, with a key made for the run and a token budget. See "The host's model" | Model time the cycles also need, and with `product` the code goes to the model |
| `hook:<point>` | The plugin runs at a stage of an issue and leaves notes. See "Hooks around the stages" | It reads the cycle's branch, and its notes reach the issue |
| `gate:<point>` | The same, and a `fail` holds the issue | A plugin can stop the line at that point |
| `action:<id>` | The view's button `<id>` works: `POST /_actions/<id>` asks for a run | Anyone signed in to the view can press it |
| `skill:<id>` | Its instructions are offered to the factory's chats | Text someone else wrote steers a chat that reads the person's tickets and memory |
| `tool:<id>` | The factory's chats may call the tool; each call is a short run of the plugin | The call's arguments can carry what the person said in the chat |
| `net:<host id>` | The view may fetch from that host through the factory's proxy | What the page sends that host, and what comes back into the page |
| `factory-read` | `TEHAS_PLUGIN_MCP_URL` and a key of the MCP server that gives the dashboard's tools only, as `TEHAS_MCP_DASHBOARD_KEYS` does, made for the plugin | What the dashboard shows |
| `factory-sessions` | The same with a full key: sessions and their logs, masked | Agent logs can quote code and issues the dashboard does not show |

`kit` is no capability: the kit is tehas's own files, served to any view,
behind the view's sign-in.

What no grant gives, ever: a GitHub token or the App's key, the Docker
socket, an SSH key or agent, anything from `~/.config/tehas` or `~/tehas`
beyond the above, the host's network, a write to the product, GitHub, the
state store or another plugin's files, `--privileged`, a device or a host
path. A manifest that asks for something not in the table is refused with
78.

### Granting

In the instance's `optional.env`, one line per plugin, its name in capitals
with `-` as `_`:

```
TEHAS_PLUGIN_GRANTS_FLOORPLAN="view product model:2m action:refresh hook:after:work skill:map tool:find-part"
```

- A capability the manifest asks for and the line does not grant: the
  plugin is refused, 78, unless the manifest marks it `optional`, and then
  the run goes without it.
- A grant the manifest does not ask for gives nothing, and `tehas update`
  says so.
- No line, no grant. There is no default and no "all".

`tehas plugin review <name> [<commit>]` fetches the pin, reads its manifest
and prints, for a person, what it asks, what is granted, what is new since
the last pin, and the line to put in the settings. It changes nothing.
`tehas update` runs the same check, and on a refusal prints the same text
and exits 78 before it builds anything. So a new pin that asks for more
never runs on the old grants.

Taking a grant away and running `tehas update` applies to the next run, and
removes the view and menu items with `view`, the network and the run's key with `model`, the
MCP key with `factory-read` or `factory-sessions`.

### The image

The build runs at `tehas update`, which a person starts. It gets the
network, since an image needs its packages, but no build secret, no SSH
forwarding and no host path. Every `FROM` names an image by digest
(`@sha256:`); a `FROM` by tag is refused with 78. The image is tagged
`tehas-plugin-<name>:<commit>` and only that tag runs.

### Enforced in one place

The container's arguments are a pure function in
`console/core/src/sandbox.rs` of the manifest and the grants, beside the
cycle's, and nothing else adds an argument. `test/fixtures/argv/` pins its
output for no grant, for each capability alone and for Floorplan's set, as
the cycle's lists are pinned. A change there is a security change, as hard
rule 4 says for the cycle.

### What is recorded

Each run is a row in the state store: plugin, commit, the grants it ran
with, start, end, result. `tehas plugin --json` shows each plugin's asks,
grants and last run, and `tehas history` lists the runs.

## The host's model

A plugin never holds the factory's model key and never talks to the model
endpoint itself. With `model` granted, a run is given
`TEHAS_PLUGIN_MODEL_URL`, an address of `tehas-proxy` on the run's internal
network, and `TEHAS_PLUGIN_MODEL_KEY`, a key the proxy made for that run and
forgets when it ends, as it does for an outsourced job. The run's network
reaches the proxy and nothing else.

The proxy holds the run to:

- **One model**, the one the grant names or the product model. Any other
  model id is refused.
- **Two routes**, chat completions and the model list. Every other path,
  among them any admin or embedding route of the endpoint, is refused.
- **A budget.** The grant gives tokens per run, `model:2m`; the
  instance gives a day's total for all plugins, `TEHAS_PLUGIN_MODEL_DAY`.
  A request past either is refused with 429 and the run ends as
  `budget`. No grant without a number: `model` alone is refused with 78.
- **The line first.** One request at a time per run, and none while a cycle
  waits for a slot or the endpoint check holds the queue: a plugin's request
  then waits, within its run's time limit. A plugin never takes model time a
  cycle needs.
- **A size**: the request's body and the answer's `max_tokens` are capped by
  the instance's `TEHAS_PLUGIN_MODEL_MAX`.

Each run's requests and tokens are counted and kept with the run's row, and
`tehas plugin --json` and analytics show them.

What the model writes is untrusted. It read the product's code and the
issue, and either may hold text written to steer it. So nothing a model
wrote is a command to tehas: it can go into the plugin's own view, or into a
hook's notes as quoted text, and a hook's verdict can hold an issue but never
let one through (see below).

## Crossings: where a plugin's data can be inspected

Everything a plugin sends or receives passes through tehas at a few known
places. v1 makes each of them one call to one interface, so that checks can
be added there later, an LLM prompt detector among them, without a new
contract and without the plugin's help.

| Crossing | Through | Direction |
| --- | --- | --- |
| A skill's text into a chat | `tehas plugin skills` | in |
| A tool's arguments and answer | `tehas plugin call` | out, in |
| A model request and its answer | `tehas-proxy` | out, in |
| A view's fetch and its answer | the gate's `/_proxy/` | out, in |
| A hook's verdict to GitHub | the report | in |
| A view's pages | the swap after a run | in |

### One interface

In `console/core`, a crossing is handed to `inspect(crossing) -> Verdict`
before it goes on: the plugin and its pin, the crossing's kind and
direction, the login if a person is behind it, and the payload. The verdict
is `allow`, `block` with a reason, or `hold`, which blocks and quarantines
the plugin. v1's checks are the size limits, the shapes and the secret mask
this spec already names. A later check is one more step in that function,
with its tests, at every crossing at once.

Inspectors are tehas's or industry's, never a plugin's: a plugin never sees
or checks another plugin's traffic.

### Kept for review

Every crossing is a row: time, plugin, pin, kind, direction, login, size,
the payload's sha256 and the verdict. That is always on, and holds no
content.

With `TEHAS_PLUGIN_KEEP=<days>`, 1 to 30, the payloads are kept too, masked
as anything tehas posts is, under `~/tehas/plugins/<name>/crossings/`, and
removed when the days are up. `tehas plugin crossings <name> --json` reads
them. That is what a mass review runs over: a detector reads a day of
crossings, scores each, and marks what it would block.

### What a block does

- **One crossing**: the request is refused, the chat gets "blocked by the
  factory", the hook's notes are not posted. The reason is logged.
- **Quarantine**: `tehas plugin hold <name> --reason <text>`, by a person,
  an inspector or industry. The plugin's runs, calls, hooks, views and menu
  items stop at once; a gate it held for becomes a `note`. It stays held
  until a person runs `tehas plugin release <name>`, and its grants stay as
  they were.
- **A pin**: a held pin stays held when industry rolls it out again; a new
  pin starts unheld, and its review shows why the last was held.

Industry can hold a plugin in every factory at once, through the same
command, and a mass review at industry can cover all its factories'
crossings.
