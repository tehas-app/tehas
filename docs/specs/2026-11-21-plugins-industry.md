# Spec: plugins, from industry to factories

**Status**: Draft 2026-11-21. Part of the plugins spec, `2026-11-21-plugins.md`, which holds the terms, the settings and the manifest. Installed once in industry, run sandboxed in each factory. Nothing is built.

## From industry to factories

A person may install a plugin once, in tehas-industry, and have it flow down
to the factories industry owns. Industry manages it; each factory runs its
own copy, sandboxed to that factory, and decides how far industry may go.

### In industry

Industry's settings name the plugin as a factory's would: its URL, its
commit, the grants, and the factories it goes to (by name, or all). A
person grants there, after reading the same review `tehas plugin review
--json` prints, which industry shows. Industry then hands each factory the
plugin through the factory API, as it hands it panels today:

```
tehas plugin adopt <name> --url <git URL> --commit <sha> --grants "<grants>" --from industry --json
tehas plugin drop <name> --from industry
```

What industry adopted is kept in `~/tehas/state/plugins.json`, beside the
instance's own `TEHAS_PLUGINS`, and `tehas plugin --json` says of each
plugin where it came from. A plugin named in both is refused with 78 until
one goes.

### The factory's ceiling

A factory allows industry nothing until its settings say so:

```
TEHAS_PLUGINS_FROM_INDUSTRY="view product model:2m action:* hook:after:work skill:* tool:*"
```

- No line, nothing: `adopt` exits 78 and names the setting.
- An adoption whose grants go past the ceiling is refused whole, with 78
  and what went past. It is never cut down to fit, so what runs is always
  what a person at industry granted.
- `gate:` points are never in a ceiling's `*`: each is named, since a gate
  stops that factory's line.
- `TEHAS_PLUGINS_DENY` names plugins the factory refuses whatever industry
  sends. A factory's own word always wins.

### Sandboxed to each factory

The plugin is one program, but every factory has its own of everything:

- It fetches and builds the pinned commit itself, into its own image, and
  reports the image's digest, so industry can see that every factory runs
  the same build.
- Its own containers, `/state`, views and sign-in, which checks the
  person's access to that factory's own repository.
- Its own keys from that factory's `tehas-proxy`, within that factory's
  model budget.
- Nothing is shared between factories: no state, no output, no key, no
  view. A run never learns which other factories run the plugin.

### What industry manages

Through `tehas plugin --json` on each factory: which plugins run where, at
which pin, with which grants, the last run and its result, and the model
tokens used. From there industry rolls a new pin out to its factories,
pauses or drops a plugin everywhere, and shows where a factory refused it
and why. A new pin that asks for more needs a person's grant at industry
and room under each factory's ceiling, like any grant.

Industry's side, its settings and its view of the plugins, is specified in
tehas-industry. tehas specifies only `adopt`, `drop`, the ceiling and the
JSON.
