# Spec: plugins v2, inspection and holds

**Version**: part of the plugin specification 1.0.0, `2026-11-21-plugins-1.0.0.md`.

**Status**: Draft 2026-11-21. Part of `2026-11-21-plugins-v2.md`. Nothing is
built.

Every place where something crosses between a plugin and the factory goes
through one check. A person or industry can hold a plugin, which stops every
crossing of it at once. With `TEHAS_PLUGIN_INSPECT=on`, each crossing leaves
a row.

## Crossings

| Crossing | Where it happens |
| --- | --- |
| A skill's text into a chat | `tehas plugin skill`, `skills` |
| A tool's arguments and answer | `tehas plugin call` |
| A model request and answer | the run's model socket |
| A fetch and its answer | `fetch.rs` |
| A hook's verdict | the hook unit, before the status |
| A view's pages | the swap, at the end of a refresh |
| A request to a service | the gate |

## The check

`inspect(crossing) -> Verdict` in `console/core/src/plugin/inspect.rs`, a
library that every program above links: `tehas`, `tehas-plugin` and
`tehas-plugin-gate`. Not a service, so a crossing never waits on another
program.

- `allow`: it goes on.
- `block`: this crossing is refused, with the rule's name.

The rules, in order: a held plugin's crossing is `block`; a size or shape
over its part's cap is `block`; text with a secret's shape (the patterns of
`console/core/src/mask.rs`, used to find, never to rewrite) in a fetch's
request or a tool's arguments is `block`. Model requests are not checked
for it: a plugin granted `product` and `model` sends the code to the model
by design, and the code may hold strings that look like keys.

An LLM detector of prompt injection may be one more rule. It is tehas's or
industry's, never a plugin's, and v2 does not ship one.

## Holds

A hold is a row in the store, `plugin_holds` (name, who, reason, time). Every
program reads it on each crossing, so a hold takes effect without a restart.

- `tehas plugin hold <name> [--reason <text>]`: nothing of the plugin runs,
  its views answer a page saying it is held, its skills and tools are not
  listed, its service is stopped.
- `tehas plugin release <name>` takes away a hold this host made. A hold made
  by industry (`--by industry`) is released by industry.
- A hold covers the name, across pins. A new pin does not release it.
- `--by industry` is a label of who asked, not a lock: anyone who runs
  commands as the factory's user can give it, as with `--login`.
- `tehas plugin` shows the state `held`, who held it, when and why.

## What is kept

With `TEHAS_PLUGIN_INSPECT=on`:

- **A database of its own**, `~/tehas/state/plugin-crossings.db`, mode 600,
  never `tehas.db`, so a flood of rows cannot slow the dispatcher. Rows older
  than 30 days are removed by `tehasbot clean`.
- **A row** per crossing: plugin, commit, kind, time, size, verdict, rule,
  and a keyed hash (HMAC-SHA256, its key in `~/.config/tehas/env`) of the
  payload. A plain hash of a short payload is guessable; a keyed one is not.
- **No payload is kept**, and a chat's row holds no login.
- **Never through MCP.**
- **Rate.** 600 rows a minute per plugin. Past it, rows are counted, not
  written, and the next row says how many were dropped.
- `tehas plugin crossings [<name>] [--since <t>] [--json]` lists the rows.
  Industry reads them for a review across factories.

The model socket answers whole, as v1's does, so an answer is checked
before it is delivered.
