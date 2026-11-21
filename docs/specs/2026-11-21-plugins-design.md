# Spec: plugins, a design cascade over beesknees26

**Version**: part of the plugin specification 1.3.0, `2026-11-21-plugins-1.0.0.md`.

**Status**: Draft 2026-11-21. A revision of 1.0.0: it adds surfaces, so it
moves the minor number. Nothing is built, and nothing is built before v1, v2,
v3 and their five plugins have landed. Owner's decisions:

- A theme overrides individual templates, components and styles of the
  pages, in a cascade.
- The base is always beesknees26. A theme replaces resources of it; it
  never starts from nothing.
- Obeya and industry wear the same tokens, so the cascade covers their
  pages too.
- The reference theme is the plugin `design`, from
  github.com/tehas/tehas-plugin-design.
- Forward looking: no legacy is kept. What this needs from v1, v2 or v3
  changes here, and `TEHAS_DESIGN` changes meaning.

## Purpose

A factory, or an industry for its factories, changes how the dashboards
look and, where a person allows it, how parts of them work, without a fork
of tehas. A theme comes from a plugin pinned by commit, like any other
plugin, and goes through the same review and grants.

## Terms

| Term | Meaning |
| --- | --- |
| base | beesknees26: every resource, complete. Layer 0 of every cascade |
| theme | A layer: a set of overrides, each naming a resource by id |
| resource | One thing a theme may override: the tokens, a stylesheet, a slot, a component or an asset |
| catalogue | The list of an app's resource ids, each with its kind and contract version |
| app | A program whose pages wear the design: `tehas` (the dashboard, the business page, the floor, the manual), `industry`, `obeya` |
| bundle | What the resolver makes: for each resource id, the file that won, hashed |

## The cascade

```
layer 0   beesknees26                   always, complete, from the release
layer 1   industry's themes             TEHAS_INDUSTRY_DESIGN, when industry manages the factory
layer 2   the factory's themes          TEHAS_DESIGN
→ for each resource id, the topmost layer that has it wins
→ a style with "extend" adds to what is below it instead of replacing it
→ the bundle is hashed and installed; the browser sees one site
```

- The resolver is pure: `resolve(catalogue, base, layers) -> Bundle |
  Refusal`, in `console/core/src/design.rs`, with no files, network or
  clock inside it. `tehas update` reads the layers and writes the bundle.
- Industry and Obeya use the same resolver, published from this workspace
  as the crate `tehas-design` and pinned as they pin `vendor/tehas`. Each
  resolves its own pages against its own catalogue.
- `pixel` stops being a second complete theme. It becomes a built-in theme
  in `host/fun/themes/pixel/`, in the same format a plugin ships, over
  beesknees26. It is the reference theme and the first proof of the
  cascade.
- `theme-beesknees26.css` and `theme-pixel.css` as separate complete files
  go. `test/theme_test.sh` goes: its rules (every token set, no page uses
  another) move into the resolver's tests.

## Settings

| Setting | Rule |
| --- | --- |
| `TEHAS_DESIGN` | Optional. Space-separated theme names, applied in order, at most 3. A name is `pixel` or the name of a plugin in `TEHAS_PLUGINS`. Empty means the base alone. A name twice is refused |
| `TEHAS_INDUSTRY_DESIGN` | Written by industry into the `instance.env` it pushes, the same form. A factory's own `TEHAS_DESIGN` goes above it. Industry's ceiling decides whether a factory may set `TEHAS_DESIGN` at all (`TEHAS_INDUSTRY_DESIGN_LOCKED=1` refuses a factory's line) |

The load checks only the form (hard rule 3). Whether each named plugin is
ready and granted `design` is checked at `tehas update`, after the fetch.

A theme named in `TEHAS_DESIGN` that is not ready, not granted, refused, or
stale stops `tehas update` with 78 and the reason. The live site stays as it
was. tehas never drops one override and serves the rest, and never falls
back to the base on its own.

## The catalogue

Each app publishes its catalogue as API: `api/design.schema.json` for the
form, `api/design.tehas.json` for tehas's ids. Industry and Obeya publish
theirs in their own `api/`. A catalogue is made from the source by
`cargo xtask design`, not written by hand, and the gate checks it is current.

```json
{"design": 1, "app": "tehas", "resources": [
  {"id": "tokens", "kind": "tokens", "contract": 1},
  {"id": "style:business", "kind": "style", "contract": 1},
  {"id": "slot:business.topbar", "kind": "slot", "contract": 1},
  {"id": "component:business-view", "kind": "component", "contract": 1, "types": "design/business-view.d.ts"},
  {"id": "asset:logo", "kind": "asset", "contract": 1, "types": ["image/svg+xml"]},
  {"id": "component:host-bridge", "kind": "component", "contract": 1, "fixed": true}
]}
```

- Ids are `a-z`, `0-9`, `.`, `-` and one `:`, at most 64. An id outside an
  app's catalogue is that app's to refuse; an app reads only the ids with
  its prefix (`industry:`, `obeya:`; tehas's have none).
- `contract` moves when what an override must keep changes: a token's
  kind, a slot's surroundings, a component's exports. A theme names the
  contract it was written for; a theme whose contract is behind is stale.
- `fixed` resources cannot be overridden. They hold a rule a person relies
  on (below).

## What a theme can replace

| Kind | What it is | Rule |
| --- | --- | --- |
| `tokens` | The design tokens: colours, type, radius, shadows, the field, for light and dark | A theme sets some tokens; the rest come from below. Only `:root { }` and `@media (prefers-color-scheme: dark) { :root { } }` blocks, and only names in the catalogue. Each value is checked by the token's kind: colour, length, number, font stack, keyword, or a `data:` image of at most 32 KiB |
| `style` | A stylesheet of a page or part | `replace` or `extend` (loaded after what is below). Any selector. No `@import`, no `url()` but `data:` and the theme's own assets, no `\` escapes in a `url()` |
| `slot` | A region of a page, marked in the source with `data-slot="<id>"` | HTML that replaces the region's content. No `<script>`, `<style>`, `<iframe>`, `<object>`, `<embed>`, `<form>`, `<base>` or `<meta>`, no `on*` attribute, no `style` attribute, no `javascript:` or `data:` address, no `{{`. Every `id` the base region holds that a component reads must stay, which the catalogue lists |
| `component` | A JS module a page loads | Replaces the base module. Needs `design:code` (below). Keeps the exports named in the catalogue's `.d.ts`, checked with the pinned `tsc`. Imports only the kit (`/fun/kit/`) and the theme's own files |
| `asset` | An image, an SVG or a font | Magic bytes match the type the catalogue allows. SVG is parsed and refused with any script, `foreignObject`, event attribute or outside reference. A font at most 1 MiB |

A page itself is never a resource. The front pages are tehas's templates;
the resolver fills their slots when `tehas update` assembles them, so a
theme's markup never runs as a server template. Which built-in page is the
front page is a field of the theme (`front`: `business` or `floor`), and
`pixel` sets `floor`.

## The manifest

A theme is a plugin with no build, text alone, as the `specs` plugin is,
or a part of a plugin that also has views or a build.

```json
"design": {
  "contract": {"tehas": 1, "industry": 1},
  "front": "floor",
  "modes": ["light", "dark"],
  "overrides": {
    "tokens": "design/tokens.css",
    "style:business": {"extend": "design/business.css"},
    "slot:business.topbar": "design/topbar.html",
    "asset:logo": "design/logo.svg",
    "industry:style:workshop.chat": {"extend": "design/chat.css"}
  }
}
```

- `overrides` at most 64, each file in the plugin's repository, read with
  `git cat-file blob` at the pin, at most 256 KiB each and 4 MiB in all.
- `contract` names, for each app the theme touches, the catalogue version
  it was written for. An app the theme does not name it leaves alone.
- A theme may `need` another theme (the `needs` of v3), which then sits
  below it in the cascade. The resolver refuses a cycle and a need that is
  not in the same stack.

## Grants

| Capability | What it gives | What a person should weigh |
| --- | --- | --- |
| `design` | Tokens, styles, slots and assets in the cascade | The theme changes what every person sees on the dashboards, including how a state, a failure or a button reads. It runs no code |
| `design:code` | Components | The theme's code runs on the dashboard's origin as the page's own code, for every person who opens it, and can do what the page can do for them, presses included |

`design:code` is the reason v2's "Widgets" rule changes: a plugin's code on
the dashboard's origin is now possible, only under this grant, only by
replacing a component, never by adding one. `tehas plugin review` shows
each component's diff against the base module beside the grant.

## What stays the host's

These resources are `fixed` in every catalogue. No theme replaces them, with
any grant:

- The gate's sign-in and the plugins menu (v1 web part, v2 menu part).
- The host bridge, `component:host-bridge`.
- The voice lock and its End control (1.1.0), and the proposal into a
  chat's input.
- The chat button row that names a plugin (1.1.0's `chat:button`).
- Anything that confirms a press: the kit's `press` and `request`.
- The CSP. A theme changes no header. The dashboard's policy keeps
  `style-src 'self'`, `font-src 'self'`, `img-src 'self' data:`, and
  `script-src 'self'` with no inline script.

A style can still hide a fixed part (`display: none`). The resolver refuses
a style that names a fixed part's selectors, which the catalogue lists, and
the host draws the lock and End above the page in an element no page
stylesheet reaches (a closed shadow root).

## Inspection

Every theme file crosses the v2 inspection point at fetch, as a skill's
text does: `inspect(crossing)` sees the tokens, styles, slots and
components before the resolver does. A held theme is not ready.

## Where the bundle lives

- tehas: `~/tehas/fun/design/`, every file named by its content's hash, and
  `design.lock.json` beside it: each id, the layer that won, the plugin's
  pin and the hash. The pages name the bundle's files; `console/ops/src/
  update/assets.rs` hashes them as today.
- The factory API serves `design.lock.json` and the files at
  `/fun/design/`, so industry and Obeya can show a factory's design.
- Industry resolves its own pages with `TEHAS_INDUSTRY_DESIGN`, its
  catalogue and the base from its pinned tehas. `crates/web-kit` reads the
  base from the bundle instead of `include_str!` of a complete theme.
  `theme-industry.css` becomes industry's own built-in theme, as `pixel` is
  for tehas.
- Obeya resolves the same way, as a page industry serves.

## Commands

| Command | Does | Exits |
| --- | --- | --- |
| `tehas design [--json]` | The stack, each layer's pin, each override and the layer that won, stale ids | 0; 78 when a named theme is not ready |
| `tehas plugin check` | Runs the resolver over the base with the author's theme alone, and the `tsc` check of its components | 0; 1 with each refusal |
| `tehas plugin try` | Serves the kit's pages with the theme, on the author's machine | as v2 |
| `tehas plugin review` | Adds the theme's overrides, each component's diff and the stale ids a newer tehas would refuse | as v1 |

## Limits

| What | Limit |
| --- | --- |
| Themes in a stack | 3 from a factory, 3 from industry |
| Overrides in a theme | 64 |
| One file | 256 KiB, a font 1 MiB |
| A theme | 4 MiB |
| A `data:` value in tokens | 32 KiB |

## What changes elsewhere

- v1 (`plugins.md`): `design` and `design:code` join the capabilities;
  `TEHAS_DESIGN` names themes.
- v2 menu part: "Widgets" is rewritten to point here.
- v3: `needs` covers themes, and a theme's place in the stack follows it.
- 1.1.0 voice: the lock and End are drawn in a closed shadow root.
- Settings: `TEHAS_DESIGN=beesknees26` is refused with "the base is always
  beesknees26: take the line out", like other retired values.
- The manual: `host/docs/frontend.md` (themes, slots, the catalogue) and
  `host/docs/plugins.md` (the grants), when each part is built.

## The design plugin

`design`, from github.com/tehas/tehas-plugin-design, Apache-2.0, is the
reference theme and the test of every kind. It ships no build.

- Tokens for light and dark, an `extend` of `style:business`, one slot
  (`slot:business.topbar`), one asset (`asset:logo`), and one component
  under `design:code`, so `review` shows a diff a person can read.
- Overrides for one id each of `industry:` and `obeya:`, so the shared
  resolver is proven on all three apps.
- Its tests run the resolver through `tehas plugin check` and load the
  pages with `tehas plugin try`.

## Work

Each is a pull request with its tests, in order:

1. The resolver and the tehas catalogue, pure, with pixel moved into a
   built-in theme over beesknees26. `theme_test.sh` moves into Rust. The
   site looks as it does today.
2. `TEHAS_DESIGN` as a stack, `design` from plugins: tokens, styles and
   assets. `tehas design`.
3. Slots: `data-slot` in the pages, the assembly in `tehas update`.
4. Components and `design:code`, the `.d.ts` contracts, the fixed parts,
   the closed shadow root for the lock.
5. The crate `tehas-design`; industry and Obeya resolve their pages with
   it, and `TEHAS_INDUSTRY_DESIGN`.
6. github.com/tehas/tehas-plugin-design, proven on a host with all three
   apps.

## Open

- Whether `design:code` needs a second person's approval, as a hold does.
- Whether a stale theme should have a grace of one release, shown by
  `review`, before `update` refuses it.
