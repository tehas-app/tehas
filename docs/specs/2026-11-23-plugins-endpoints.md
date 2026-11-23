# Plugin revision 1.7: model endpoints

**Version**: plugin specification revision 1.7.0, 2026-11-23 (the number is
fixed when it merges, in `2026-11-21-plugins-1.0.0.md`). Built after
tehas 2.0.0, with the rest of plugins (2.1). Nothing here is built.

The other side of this revision is industry's: tehas-industry
`docs/specs/2026-11-23-plugin-endpoints.md`, which says how industry
registers an endpoint plugin in the `models.json` it writes for its
factories.

## Why

A plugin granted `model:<tokens>` gets one socket per run,
`/run/tehas/model.sock`. It always reaches the main endpoint and key of the
provider the cycle's main role uses. A plugin cannot choose another, and no
one can bring one. Two needs follow from the plugins built so far:

- A plugin whose work suits another model than the cycles' (a large one for
  a first map, a small one for a hook on every pull request) should be able
  to use it, when a person says so.
- An operator should be able to add an endpoint, a hosted API or a model a
  plugin runs itself, the way a plugin adds a view: from a pinned
  repository, reviewed and approved, with the key kept by the operator.

## The owner's decisions

1. A plugin can select a specific endpoint. The grant names a provider of
   `models.json`, as `model:2000000@fleet`. A manifest may say which
   provider it prefers; only the grant decides. Without `@`, a grant means
   what it means today. Keys stay out of the plugin. The socket still
   rewrites the model id, counts tokens and keeps every limit it has.
2. A plugin can add an endpoint to the industry that hosts its factories. A
   plugin may be only an endpoint plugin: it adds a provider and nothing
   else. Industry registers it in the `models.json` it writes for its
   factories, after the owner approves. Plugins and factories then select it
   by name, as in 1. The key lives in industry's secrets, never in the
   plugin's repository or files, never in logs, command lines or pages.
3. Both API shapes count: OpenAI chat completions and Anthropic Messages.

## What the platform adds

Each addition is an ask, a grant or a setting. `tehas plugin review` lists
it, and a person writes it. Nothing here is given by default. Until the
code that enforces a word has merged, the reader refuses it by name.

### 1. `model:<tokens>@<provider>`: a provider named in the grant

- The grant may name a provider: `model:<tokens>@<provider>`. The provider
  is a key of `providers` in the factory's `models.json`. Its name is 1 to
  32 characters of `a-z`, `0-9` and `-`, a letter first, not ending in `-`.
  A provider with another name cannot be granted, and `review` says so.
- Without `@`, the grant is today's: the main endpoint of the product
  model's provider, the one the cycle's main role uses.
- With `@`, the socket sends to the named provider's main endpoint
  (`baseUrl` and `apiKey`), never its backup. The model is the main role's
  model when the provider is the main role's, and otherwise the first model
  the provider lists, as an agent that loses its model setting takes the
  first entry.
- The ask stays `model`. A manifest never names a provider in `asks`.
- `model:images` (v3) is unchanged and applies to whichever provider the
  grant names. A provider whose model's `input` lacks `image` makes a
  request with images 400, "the granted model takes no images".
- The socket keeps every rule of the security part: the fields, the roles,
  the body's bytes, `max_tokens`, the reserve, one request at a time, 16
  connections, the bounds and the 4 MiB answer. It rewrites `model` to the
  provider's own id, as today.
- The context's `model` gains `provider`: the provider's name, with or
  without `@` in the grant. So `{"id", "tokens", "provider"}`.
  `api/plugin-context.schema.json` gains the field. It names a provider,
  never an address or a key.

**A preference in the manifest.** A manifest that asks `model` may say
which providers it prefers:

```json
"model": {"prefer": ["claude", "fleet"]}
```

| Field | Rule |
| --- | --- |
| `model` | Optional; needs the ask `model`. An object with `prefer` only |
| `model.prefer` | 1 to 4 provider names, each by the rule above, unique, in order |

`prefer` gives nothing. `review` shows it, and writes the grants line with
the first preferred provider that `models.json` has and the day budgets
below name. A grant to a provider not in `prefer`, or with no `@`, is as
valid as one that follows it.

**A day budget per provider.** One provider may be paid by the token and
another not. So a grant that names a provider needs a day budget for it:

```
TEHAS_PLUGIN_MODEL_DAY_PROVIDERS="claude=2000000 fleet=20000000"
```

| Setting | Rule |
| --- | --- |
| `TEHAS_PLUGIN_MODEL_DAY_PROVIDERS` | Optional. Space-separated `<provider>=<tokens>`, the provider by the rule above, unique, at most 8, tokens 1 to 999999999: what all plugins together may spend with that provider in a UTC day. A grant `model:<n>@<provider>` whose provider has no entry here is 78 at load |

- A request counts against the run's `<tokens>`, against
  `TEHAS_PLUGIN_MODEL_DAY`, and with `@` against its provider's day. A
  request that would pass any of them is 429 and is not sent.
- `Tehas-Tokens-Left` (1.6.0) is the least of the three.
- `~/tehas/plugins/model-day` keeps the day's total and one total per
  provider. `plugin_runs` gains `provider`, the provider the run's socket
  sent to.
- The cycles' own use of a provider is not counted here. A provider shared
  with the cycles shares its capacity with them, as today.

**Fail closed.**

| What | When | What happens |
| --- | --- | --- |
| A grant `model:` with an `@` and no name, a name that breaks the rule, or two `@` | Load | 78, as any malformed grant |
| A grant naming a provider with no `TEHAS_PLUGIN_MODEL_DAY_PROVIDERS` entry | Load | 78 |
| A grant naming a provider `models.json` does not have, or one with no models | `tehas update` | The plugin is refused: "the grant names the provider fleet, which models.json does not have" |
| A provider whose `api` is not `openai-completions` or `anthropic-messages` | `tehas update` | The plugin is refused: "the provider's API is not one the model socket speaks" |
| The provider or its key gone by a run's start | The run's start | The run fails before its container starts, with that reason in its log. The runner reads `models.json` at each start, as the cycle's start does |

The load reads only the lines, as for every setting (hard rule 3). Whether
the provider is in `models.json` is a check of `tehas update`, after the
load, beside the manifest's.

What a person should weigh: a grant may send the product's code to another
endpoint than the cycles'. `review` names the provider, its API, whether its
endpoint is on this host, and the day budget it has.

What reaches the container: nothing new. The same socket, one more field in
the context. Not a security change in the container; a change in where the
code may go, which the grant names.

### 2. A provider whose API is Anthropic Messages

A provider's `api` in `models.json` is Pi's word for its shape. The socket
speaks to two: `openai-completions` and `anthropic-messages`.

**Decided: the socket speaks OpenAI chat completions to the plugin, either
way.** The checks of the security part (the fields, the roles, text and
image parts, the reserve) are written once, for one shape. A plugin then
works with any provider a person grants, and moving a plugin to another
provider is a change to its grant, not to its code. `POST /v1/messages` on
the socket stays 404.

For a provider whose `api` is `anthropic-messages`, the socket checks the
request as today, then translates it. A request that cannot be carried is
400, naming the field, and is not sent.

| Chat completions | Messages |
| --- | --- |
| `model` | The provider's model id |
| `system` messages | `system`: their texts, in order, joined with a blank line. A `system` message after the first message of another role is 400 |
| `user` and `assistant` text | Turns of the same role, with `text` blocks. Two turns of one role in a row are merged into one, in order |
| An assistant turn's `tool_calls` | `tool_use` blocks: `id`, `function.name` as `name`, `function.arguments` parsed as `input`. Arguments that are not a JSON object: 400 |
| `tool` messages | `tool_result` blocks in a `user` turn, `tool_call_id` as `tool_use_id`, the text as content. Tool messages in a row go into one turn |
| Image parts, with `model:images` | `image` blocks, `source.type: base64`, the media type the first bytes gave |
| `max_tokens` | `max_tokens` |
| `temperature` | `temperature`. Above 1: 400 |
| `top_p` | `top_p` |
| `stop` | `stop_sequences`, a string made a list |
| `tools` | `name`, `description` and `parameters` as `input_schema`. A tool whose `type` is not `function`: 400 |
| `tool_choice` | `auto` as `{"type": "auto"}`, `none` as `{"type": "none"}`, `required` as `{"type": "any"}`, a named function as `{"type": "tool", "name": ...}` |
| `response_format` | `{"type": "text"}` is dropped. Any other: 400 |
| `seed` | 400 |

- The request goes to `<baseUrl>/v1/messages`, as Anthropic's clients and
  Pi form it: such a provider's `baseUrl` has no `/v1`. The key goes in
  `x-api-key`, with `anthropic-version: 2023-06-01`. A provider whose key
  is a word with no `$`, such as `unused`, is sent no key.
- The answer comes back as a chat completion: `id`, `object:
  "chat.completion"`, `model` as the id the context names, one choice. The
  message's `content` is the `text` blocks joined, `null` when there are
  none; each `tool_use` block becomes a `tool_calls` entry with its `input`
  as JSON text. Other blocks are dropped.
- `finish_reason`: `end_turn` and `stop_sequence` are `stop`, `max_tokens`
  is `length`, `tool_use` is `tool_calls`, `refusal` is `content_filter`.
  Any other `stop_reason` is 502, "the answer could not be read".
- `usage`: `prompt_tokens` is `input_tokens` plus
  `cache_creation_input_tokens` and `cache_read_input_tokens`;
  `completion_tokens` is `output_tokens`. It replaces the reserve, as
  today.
- An error from the endpoint is passed back with its status when it is
  JSON, as today, in Anthropic's own shape. The socket does not rewrite it.

The translation is a pure function of the checked request and of the
answer, in `console/plugin/src/model/anthropic.rs`, beside the socket.

What a person should weigh: nothing new beyond 1. The code goes to the
provider the grant names.

What reaches the container: nothing. Not a security change.

### 3. `kind: "endpoint"`: a plugin that is only a provider

An endpoint plugin adds one to four providers and nothing else. It has no
view, action, hook, skill, tool or menu item, and it asks for nothing. It
is approved and registered by industry for the factories it names
(industry's spec). A factory never pins one: an endpoint plugin in
`TEHAS_PLUGINS` is refused, "an endpoint plugin is registered by industry,
not pinned in a factory".

```json
{
  "plugin": 1,
  "kind": "endpoint",
  "name": "claude",
  "label": "Claude",
  "license": "Apache-2.0",
  "endpoints": [
    {
      "id": "claude",
      "label": "Claude",
      "api": "anthropic-messages",
      "baseUrl": "https://api.anthropic.com",
      "models": [
        {"id": "claude-sonnet-4-5", "contextWindow": 200000, "maxTokens": 64000, "input": ["text", "image"]}
      ]
    }
  ]
}
```

| Field | Rule |
| --- | --- |
| `plugin` | `1` |
| `kind` | `endpoint` |
| `name` | The rule of `TEHAS_PLUGINS`'s names |
| `label`, `license` | As v1 |
| `build` | Only when an endpoint is served (below), and then required. As v1 |
| `endpoints` | 1 to 4 |
| `endpoints[].id` | The provider's name in `models.json`, by the rule of 1. Unique in the plugin |
| `endpoints[].label` | One line, 1 to 32 characters, control characters refused |
| `endpoints[].api` | `openai-completions` or `anthropic-messages` |
| `endpoints[].baseUrl` | A hosted endpoint: `https://`, a host name, an optional port and path. No user, query or fragment, not an IP literal. As that API's clients expect it: with `/v1` for `openai-completions`, without for `anthropic-messages` |
| `endpoints[].serve` | An endpoint the plugin serves itself, instead of `baseUrl`: `{"health": "<path>"}`. Exactly one of `baseUrl` and `serve` |
| `endpoints[].models` | 1 to 16. Each `id` (1 to 128 of `A-Za-z0-9._:/-`, unique in the endpoint), and optionally `name` (1 to 64, one line), `contextWindow` and `maxTokens` (1 to 10000000), `input` (`["text"]` or `["text", "image"]`), `reasoning` (boolean) |

- A field not in the table is refused, as everywhere. So are `asks`,
  `timeout`, `ui`, `view`, `views`, `actions`, `hooks`, `model`, `skills`,
  `tools` and every later surface: an endpoint plugin is that and nothing
  more.
- No `apiKey`, `headers`, `authHeader`, `compat` or `cost`, at either
  level. A key or a header from the plugin's repository is refused by the
  schema, so no key can be committed into a manifest and pass. Who
  registers the provider fills `compat` and `cost` as `install.sh` fills
  them today.
- `api/plugin-manifest.schema.json` gains the endpoint branch. Its rules
  are the schema's, but two that are the reader's alone: the ids unique in
  the plugin and the model ids unique in an endpoint. Samples:
  `api/plugin-manifest.endpoint.json` (hosted),
  `api/plugin-manifest.endpoint-served.json` (served), and refused ones,
  `api/plugin-manifest.endpoint-key.refused.json` (an `apiKey`) and
  `api/plugin-manifest.endpoint-dup.refused.json` (two ids alike), so
  another reader can test that it agrees.

**A hosted endpoint has no container.** Nothing of the plugin runs. Its
repository is the manifest and what a person reads. The provider reaches
the API at `baseUrl` with a key the operator holds.

**A served endpoint** is a model the plugin runs itself. Its image is built
by the build rules of the security part, from the commit, with no network.
The weights come in the repository or in a base image by digest. The
container:

```
docker run --init --name tehas-endpoint-<name> --pull=never
  --user 10001:10001 --read-only
  --cap-drop ALL --security-opt no-new-privileges
  --network none
  --cgroup-parent <the host's slice>
  --memory <m>g --memory-swap <m>g --cpus <n> --pids-limit 512
  --log-driver none
  --tmpfs /tmp:size=1g,noexec,nosuid,nodev
  --mount type=bind,src=<serve dir>,dst=/run/tehas/serve
  --group-add <the host user's group>
  <image id>
```

- **`--network none`**, as every plugin container. The image listens on
  `/run/tehas/serve/http.sock`, as a v2 service does, in a directory of mode
  2770 with the host user's group. The one client is the host's own
  program in front of it, industry's gateway (industry's spec). It never
  reaches out, and nothing reaches it but through that socket.
- **No mount but the socket's directory.** No `/product`, `/state`,
  `/work`, `/out`, model socket or context. Nothing it is sent is kept past
  its restart.
- **No log.** `--log-driver none`: what it is sent is other programs'
  prompts, and a log would keep them. Its health is the `health` path, not
  its output.
- **No key and no identity.** It is sent the request's body and the
  headers of its API, never a key, a caller's name or a person's.
- **What it answers**: `GET <health>` with 200 when ready; for
  `openai-completions`, `POST /v1/chat/completions` and `GET /v1/models`;
  for `anthropic-messages`, `POST /v1/messages`.
- **No device.** No GPU or other device, as for every plugin. A served
  endpoint runs on the CPU (see "Open points").

`test/fixtures/argv/plugin-endpoint` pins the list, with the host's slice,
memory, CPUs and group as placeholders, beside the other plugin lists. The
host that runs a served endpoint builds the same list, and its own test
compares it with that file. A change there is a security change.

**Two commands for the host that registers it.** Industry needs tehas's
own rules for a manifest and a build, not a copy of them:

| Command | What it does | Exits |
| --- | --- | --- |
| `tehas plugin check <dir> --json` | As v1, and for an endpoint plugin prints `endpoints`: each id, label, API, the `baseUrl`'s origin or `served`, and its models; with a build, the base images | 0; 3 refused, with why; 64 |
| `tehas plugin build <dir> --tag <tag> --slice <slice> [--json]` | New. Needs no settings. Applies `check`'s rules, pulls the base images by digest, refuses an `ONBUILD`, and builds as the security part's build does, with `--network none` and the slice given. Prints the image id. Runs nothing it built | 0; 1 the build failed; 3 refused; 64 |

`tehas plugin check` and `build` read a local folder. The fetch is the
caller's, with the fetch rules of the security part.

What a person should weigh, for a hosted endpoint: the code of every
factory that selects it goes to that API, under the operator's account.
For a served endpoint: the plugin's code reads every prompt sent to it,
closed source included. It has no network, no log and nothing kept, so it
cannot send what it reads anywhere but back in its answers.

What reaches a factory's container: nothing. A factory sees the provider
in its `models.json`, as any other. A security change: a new container
kind, run by industry, pinned by a new argument list.

## Review

`tehas plugin review <name> [<commit>] [--json]` gains, for a plugin that
asks `model`:

- the manifest's `prefer`, when it has one;
- each provider of `models.json` the grant may name: its `api`, its first
  model, whether its endpoint is on this host or another, whether its key
  is set (never the key), and its `TEHAS_PLUGIN_MODEL_DAY_PROVIDERS`
  entry, or that it has none;
- the provider the current grant names, or that it names none;
- the grants line to write, with `@` the first preferred provider that has
  a day entry, and the budget line it needs when there is no entry yet.

`tehas plugin [--json]` gains, for each plugin, the provider its grant
names and its tokens today with that provider. `api/plugin.schema.json`
and `api/plugin-review.schema.json` gain `provider`, `prefer` and the
providers list; their samples gain a plugin with `@`.

## Privacy

- The socket keeps counts, not bodies, whichever provider it sends to.
- It sends the endpoint the request and the key, and nothing of a person:
  no login, no header of the run.
- The context's `provider` and `plugin_runs.provider` are names an operator
  chose. Neither is personal.
- A key is read where it is today, `$TEHAS_MODEL_KEY_<NAME>` from
  `~/.config/tehas/env`. It is never in the context, a log, a command line,
  `review`'s output or a page.

## Changes to earlier parts

| Where | Now | Change | Why |
| --- | --- | --- | --- |
| v1, settings, `TEHAS_PLUGIN_GRANTS_<NAME>` | `model:<tokens>` is 1 to 999999999 | Add `model:<tokens>@<provider>`, with the name rule and the day entry it needs (1) | A grant names the endpoint |
| v1, settings | `TEHAS_PLUGIN_MODEL_DAY` for all plugins | Add `TEHAS_PLUGIN_MODEL_DAY_PROVIDERS` (1) | A paid provider needs a bound of its own |
| v1, the manifest | `kind` `batch`; v2 adds `service` | Add `endpoint` (3) and the field `model` (1) | A plugin that is only a provider; a preference |
| Security part, the model socket | The main endpoint of the main role's provider, chat completions only | The granted provider's main endpoint; a provider whose `api` is `anthropic-messages` through the translation (2) | The owner's decisions 1 and 3 |
| Security part, the context | `model` is `{"id", "tokens"}` | Add `provider` | A run knows which provider its budget is with |
| Security part, the container | One container kind for runs, one for services | Add the served endpoint's (3), pinned in `test/fixtures/argv/plugin-endpoint` | A model a plugin runs itself |
| v1, writing a plugin | `new`, `check`, `try` | `check` prints endpoints; add `tehas plugin build` (3) | Industry runs tehas's rules, not a copy |

## Enforced in one place

| Rule | Where | Test |
| --- | --- | --- |
| The grant's `@<provider>` and the day entries | `console/core/src/settings/rules.rs` | the name rule, two `@`, a grant with no day entry is 78 |
| The provider in `models.json` and its `api` | `console/core/src/plugin/run.rs` (`Grants`), at `tehas update` | an unknown provider, one with no models, an `openai-responses` provider: each refuses the plugin |
| The provider at a run's start | `console/plugin/src/upstream.rs` | a provider removed after the update fails the run before the container starts |
| The three budgets and the header | `console/plugin/src/day.rs` and `model.rs` | a request over the provider's day is 429 and not sent; `Tehas-Tokens-Left` is the least of three |
| The translation | `console/plugin/src/model/anthropic.rs` | each row of the table both ways, a late `system` message, `seed`, `response_format`, an unknown `stop_reason`, usage with cache tokens, against a stub endpoint |
| The endpoint manifest | `console/core/src/plugin/manifest.rs` and `api/plugin-manifest.schema.json` | the samples above; `apiKey` and `headers` refused at both levels; an endpoint plugin in `TEHAS_PLUGINS` refused |
| The served endpoint's container | `console/core/src/sandbox.rs` | `test/fixtures/argv/plugin-endpoint` |
| `tehas plugin build` | `console/core/src/plugin/build.rs` and `dockerfile.rs` | the build rules' cases, run through the settings-free entry |
| Review's output | `console/core/src/plugin/review.rs` | a plugin with `prefer`, a provider with no day entry, a key not set; no key's value in any output |

## Open points

1. **A model named in the grant.** `@<provider>` takes the main role's
   model or the provider's first. A provider with several models may need
   `@<provider>/<model>`. Left out until a plugin needs it: model ids hold
   `/` and `:`, so the syntax needs care.
2. **Anthropic's own features.** Prompt caching, thinking and the Messages
   shape itself are not reachable through the socket. A later revision may
   pass `POST /v1/messages` through for such a provider, with its own
   checks, if a plugin needs them.
3. **A GPU for a served endpoint.** No grant gives a device, in any
   version. A served endpoint runs on the CPU. A device would be a revision
   of its own, and a security change.
4. **A standalone factory.** A factory without industry cannot take an
   endpoint plugin. Its operator writes the provider into `models.json` by
   hand, as today. A grant on such a factory may still name that provider.
5. **Streaming.** The socket answers whole, as v1 does, for both shapes.

## Work

In order. Each is a pull request; a security change says so.

1. **The grant and the budgets.** `@<provider>`,
   `TEHAS_PLUGIN_MODEL_DAY_PROVIDERS`, the update's check, the context's
   `provider`, `plugin_runs.provider`, review's output, the manifest's
   `model.prefer`.
2. **The translation.** `anthropic.rs` and the socket's choice of shape.
3. **The endpoint manifest.** The schema's branch, its samples, the
   reader, the refusal in `TEHAS_PLUGINS`, `tehas plugin check`'s output.
   Industry can take these files with `cargo xtask tehas pull` once
   released.
4. **The served endpoint.** `tehas plugin build`, the argument list and
   its fixture. Security change.
