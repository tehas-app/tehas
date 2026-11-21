# Spec: plugins, hooks around the stages

**Status**: Draft 2026-11-21. Part of the plugins spec, `2026-11-21-plugins.md`, which holds the terms, the settings and the manifest. Runs before and after a stage of an issue, outside pi-rukas. Nothing is built.

## Hooks around the stages

A plugin may run at the edges of an issue's stages, outside pi-rukas and
outside the cycle's container: before a stage starts, or after it ends and
before tehas acts on its result. Nothing runs inside a stage.

| Point | When | What tehas does with a `fail` (gate only) |
| --- | --- | --- |
| `before:plan` | Before a planning turn starts | No turn; the issue is parked with the plugin's reason |
| `after:plan` | After the plan is written, before it is posted | The plan is posted with the notes, and the issue waits for a person |
| `before:work` | After the approval check, before the container starts | No cycle; parked as a start that never ran |
| `after:work` | After the cycle pushed and ended, before the report labels the issue | `needs-human-attention` in place of `tehas:awaiting-merge`, with the notes |
| `before:refresh`, `after:refresh` | The same around a refresh | The same as for work |

`after:work` is the place for extra verification of a cycle's change.

### What a hook gets and gives

- **In**: `/run/context.json`, read only: the point, the issue's number and
  title, the base commit, the branch and its head commit, the plan's text
  for `after:plan`, the cycle's result for `after:work`. With `product`
  granted, `/product` is a clean checkout of the branch's head, not the
  cycle's workspace.
- **Out**: `/out/_tehas/verdict.json`, `pass`, `fail` or `note`, with a
  summary of at most 1000 characters and at most 50 findings, each with a
  file, a line and a text of at most 500. tehas checks it against
  `api/plugin-verdict.schema.json`, masks secret-looking text, and posts it
  itself, as quoted text in the park comment or as a comment on the pull
  request. The plugin holds no token.

### Hook, gate and their limits

- `hook:<point>` gives notes only. A `fail` from a hook is posted as a note,
  and the issue goes on.
- `gate:<point>` lets a `fail` hold the issue as the table says. A person
  grants it, and should only to a plugin they trust to stop the line.
- **A verdict can hold, never let through.** No plugin can mark an issue
  ready, approve a pull request, merge, or skip tehas's own gate or review.
  A `pass` changes nothing.
- **Bounded.** Each hook has the manifest's `timeout`, at most the
  instance's `TEHAS_PLUGIN_HOOK_TIMEOUT`, counted in the stage's own
  deadline. A hook that fails to run or runs out of time is a `note` for a
  hook, and for a gate it holds the issue with the reason `plugin:<name>`,
  since a person asked for that gate. A gate that holds every issue is seen
  on the dashboard and in `tehas blockers`, and taking the grant away ends
  it.
- **Order.** At one point, hooks run in the order of `TEHAS_PLUGINS`, one
  after another. Every hook runs; any gate's `fail` holds.
- The cycle's container is given nothing new: hooks run in their own
  containers, before it starts or after it ended (hard rule 4).

The manifest names its hooks:

```json
"hooks": [{"at": "after:work", "mode": "gate", "timeout": 600}]
```
