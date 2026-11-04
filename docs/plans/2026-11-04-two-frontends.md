# Two frontends: the land in pixel, everything else in beesknees26

> **For agentic workers:** work through the phases in order. Every phase ends
> with its files changed and nothing half done. Tests change with behaviour, in
> the same commit as the behaviour. Do not touch `.pi/`, `AGENTS.md`,
> `scripts/verify.sh` or `scripts/check-generic.sh`.

**Goal:** the pixel look belongs to one page, the landing page of a factory's
site. Every other page is drawn in the business look. The instance settings that
let one theme paint the whole site are gone, and the footers stop advertising
the other frontend.

**Approach:** two frontends inside the same server. Each page names its own
theme file, its own logo and its own share card, so no rewrite decides the look
at request time. `TEHAS_THEME` and `TEHAS_LAYOUT` retire. `theme-paper.css` and
`theme-beesknees.css` are deleted; `theme-pixel.css` and `theme-beesknees26.css`
stay.

**Decisions taken with the owner (4 Nov 2026):** one container, two pinned route
families (not two services). Both `/floor` and `/business` keep their current
pages and paths; only the cross-links go. `paper` and `beesknees` are dropped,
`beesknees26` stays. AI orchestrator: build it, verify it, open a pull request,
stop.

## Why this is the shape

Today `TEHAS_THEME` is one host-wide setting. Both servers answer `/fun/theme.css`
with `theme-<name>.css` for every path (`host/edge/docs.Caddyfile`,
`host/edge/router.Caddyfile`), so a business theme paints the 16-bit floor and
`pixel` paints the manual, the plain dashboard and the Protolab. The overlap is a
property of the routing, not of a bad theme file. Pinning the look per page makes
the overlap impossible instead of configurable.

tehas.app does three jobs at once: marketing, documentation and demo. Only the
demo wants the 16-bit identity. `PRODUCT.md` already limits the demoscene
heritage to the footer and calls the business look "the other view's territory".

## The two frontends

| Family | Paths | Theme file it names | Its sheets | Its logo and share card |
| --- | --- | --- | --- | --- |
| The land | `/`, `/floor` | `theme-pixel.css` | `game-view.css`, `energy-view.css`, `host-view.css` | `factory.svg`, `share.png` |
| The app | `/business`, `/dashboard`, every manual page, `/agents`, `/llms.txt`, the Protolab | `theme-beesknees26.css` | `business.css` for `/business`; `game-view.css` plus `manual.css` for the rest, unchanged this pass | `favicon.svg`, `share-logo.png` |

`/` is always the factory floor. `/business` is always the business page. The
front page is no longer a setting.

## What retires

- `TEHAS_THEME` and `TEHAS_LAYOUT` leave the settings table
  (`console/core/src/settings/rules.rs`) and join `RETIRED`
  (`console/core/src/settings/load.rs:94`). Both keep the existing precedent for
  a line that asks for what this version already does: `TEHAS_THEME=pixel`,
  `TEHAS_THEME=`, `TEHAS_LAYOUT=floor` and `TEHAS_LAYOUT=` load as no-ops. Any
  other value is refused with its own reason, as `TEHAS_CLONES` and
  `TEHAS_DISPATCHER` are. A host that had `TEHAS_LAYOUT=business` or a non-default
  theme stops with 78 until the line is taken out of `instance.env`.
  `instances/tehas-tehas/instance.env` sets neither.
- The rewrites `/fun/theme.css`, `@logo` (`factory.svg` to `favicon.svg`) and
  `@share` (`share.png` to `share-logo.png`) in both Caddyfiles, and the two
  settings in both compose files. Pages name their own files now.
- The `(env "TEHAS_THEME") "pixel"` branches in `host/fun/index.html:25` and
  `host/docs/_layout.html:49`: each page states its own `og:image:alt`.
- `host/fun/theme-paper.css` and `host/fun/theme-beesknees.css`.

## Footers

Only the footers change. The manual's top bar and its contents list keep their
entries, because they navigate inside the app family.

| Where | Before | After |
| --- | --- | --- |
| `host/fun/business.html:155` | Factory floor · Plain dashboard · Manual · Agents · llms.txt | Manual · Agents · llms.txt |
| `host/docs/_layout.html` footer | FACTORY FLOOR · AGENTS · LLMS.TXT | AGENTS · LLMS.TXT |
| `host/fun/index.html:330` | the sentence naming the plain dashboard and its link | that sentence and link gone; the agents page and `/llms.txt` links stay |

## Accepted for this pass, recorded as open

- The app pages still load `game-view.css` for their chrome (`.topbar`, `.brand`,
  `.menu`, `.footer`, `.ascii`, `.credit`). Under `beesknees26` tokens that draws
  them in IBM Plex with thin rules, soft shadows and rounded corners, which is
  the business look, but it is the floor's drawing of it. The follow-up is a
  chrome sheet factored out of `business.css` so `manual.css`, `_layout.html` and
  `protolab.rs` stop depending on the floor's stylesheet.
- The ASCII logo and its Topaz credit stay in the manual and Protolab footers.
- `/dashboard` stays a page of its own, unlinked from footers. Whether it folds
  into `/business` is the owner's later call.
- `beesknees26` has a dark set and only `/business` has the mode button.

## Plan

### Phase 1: settings retire (Rust)

- [ ] `console/core/src/settings/rules.rs`: drop the `TEHAS_THEME` and
      `TEHAS_LAYOUT` rows from `TABLE`, drop `THEMES` if nothing else uses it,
      and drop the four assertions that name them in its tests.
- [ ] `console/core/src/settings/load.rs`: `RETIRED` grows from 7 to 9 names, and
      `retired_names` gains an explicit reason for each so the fallback "rename it
      TEHAS_MILLWRIGHT_" message is not used:
      `TEHAS_THEME is no longer read: the front page is pixel and every other page is beesknees26, so take the line out`,
      `TEHAS_LAYOUT is no longer read: the front page is always the factory floor and /business is always the business page, so take the line out`.
      Accept the no-op values (`pixel`, empty) for `TEHAS_THEME` and (`floor`,
      empty) for `TEHAS_LAYOUT` in the same `matches!` arm that carries
      `TEHAS_CLONES` and `TEHAS_DISPATCHER`.
- [ ] Re-point the unit tests that used these settings as their example
      (`load.rs` lines near 872, 1017, 1116, 1268) at a setting that still exists,
      and add cases for the two retired names: refused with their reason, and the
      no-op values load.

### Phase 2: the servers stop choosing the look

- [ ] `host/edge/docs.Caddyfile`: `@business` becomes `path /business`;
      `@front path / /floor` stays; delete the theme rewrite, the `@logo` and
      `@share` blocks and their comments.
- [ ] `host/edge/router.Caddyfile`: in `handle @look`, keep `route { uri
      strip_prefix /fun }` for the shared assets and delete the theme rewrite and
      both logo rewrites.
- [ ] `host/edge/docs.compose.yaml` and `host/edge/router.compose.yaml`: drop the
      `TEHAS_THEME` and `TEHAS_LAYOUT` environment lines and the comment lines
      that describe them.

### Phase 3: each page names its own look

- [ ] `host/fun/index.html`: first stylesheet `/fun/theme-pixel.css`; `og:image`
      stays `/fun/share.png`; `og:image:alt` states the pixel factory, no branch.
- [ ] `host/fun/business.html`: first stylesheet `/fun/theme-beesknees26.css`.
- [ ] `host/docs/_layout.html`: first stylesheet `/fun/theme-beesknees26.css`,
      then `game-view.css`, then `manual.css`; the brand image becomes
      `/fun/favicon.svg` at 50 by 50; `og:image` becomes `/fun/share-logo.png`;
      `og:image:alt` states the logo, no branch.
- [ ] `console/src/protolab.rs` `top()`: `/fun/theme-beesknees26.css` and the
      brand image `/fun/favicon.svg` at 50 by 50. Its tests assert no head markup
      today; add one case that a lab page names beesknees26 and no page names
      `/fun/theme.css`.

### Phase 4: the theme files

- [ ] Delete `host/fun/theme-paper.css` and `host/fun/theme-beesknees.css`.
- [ ] `host/fun/theme-pixel.css`: its header says the instance picks a theme with
      `TEHAS_THEME` and that the servers answer `/fun/theme.css`. Both are now
      false; say the front page names this file and every other page names
      `theme-beesknees26.css`.
- [ ] `host/fun/theme-beesknees26.css`: drop the sentence that says it will
      replace beesknees.

### Phase 5: footers

- [ ] The three footer changes in the table above.

### Phase 6: tests

- [ ] `test/theme_test.sh`, the file that asserts the old mechanism:
  - "there are at least two themes" becomes exactly two, `theme-pixel.css` and
    `theme-beesknees26.css`.
  - "every page loads the theme before its own styles" now asserts the pin: the
    land names `theme-pixel.css`, every other page in `markup` names
    `theme-beesknees26.css`, and no file anywhere names `/fun/theme.css`.
  - Replace the two cases that assert the servers rewrite `/fun/theme.css` and
    swap `factory.svg` and `share.png` by theme with cases that the rewrites are
    gone, and that the land's markup names `factory.svg` and `share.png` while the
    app's markup names `favicon.svg` and `share-logo.png`.
  - Keep the token parity, Twelve Bit Rule, "nothing outside a theme names a
    colour" and canvas token cases; drop `paper` and `beesknees` from the loops.
  - Add a case for the footers: no app page links to `/floor` or `/dashboard`.
- [ ] `test/config_test.sh`: the two settings leave the refusal value list (near
    line 354) and the `names` list (near line 1509); `settings words` cases for
    them (near 653 and 662) go; the `TEHAS_THEME=`, `TEHAS_THEME=neon` and
    `TEHAS_THEME=neon|TEHAS_TOP=9` lines in the instance heredoc become retired
    name cases, with a no-op line each; the recorded `sha256sum` of the
    `settings valid` output and the recorded loads output are recomputed from the
    new run.
- [ ] `test/cycle_report_test.sh:38` uses `TEHAS_THEME=nothing-this-version-knows`
    to stand for an unknown setting; use a name that is neither a setting nor
    retired, for example `TEHAS_NOT_A_SETTING=1`.
- [ ] Run the gate on the change, then once in full: `scripts/verify.sh
    --changed`, then `scripts/verify.sh`. Quote both in the handoff.

### Phase 7: the manual and the record

- [ ] `DESIGN.md` Themes section: two frontends, each pinned; `pixel` binds the
      landing page and the rules that follow it bind that page; `beesknees26` is
      the look of every other page; a theme file is still a token set and no
      stylesheet names a colour. Say plainly that the app pages still draw their
      chrome from `game-view.css` and what the follow-up is.
- [ ] `PRODUCT.md`: the Register, Users and Brand Personality sections speak of one
      page. Add the scope: the 16-bit identity is the front page's; the manual,
      the plain dashboard, `/business` and the Protolab are in the business
      register, and the demoscene signature stays in footers.
- [ ] `host/docs/host.md`: rewrite "Themes and layouts" for two pinned frontends,
      drop the theme table and the layout table, and say what an adopter with
      either line in `instance.env` must do.
- [ ] `host/docs/tools.md`: drop the `TEHAS_THEME` and `TEHAS_LAYOUT` rows from
      the settings table (lines 77 to 78).
- [ ] `host/docs/overview.md`: drop the Theme row (85) and the Front page row (86);
      keep the Factory floor and Plain dashboard rows (87, 91 to 93) since both
      pages still exist.
- [ ] `host/docs/features.md`: the Themes row (99) and the Business page row (100)
      describe settings; restate them as pinned frontends, and keep the honesty
      about what has been seen in a browser and what has not been seen on a host.
- [ ] `CHANGELOG.md` under `## Unreleased`: one `### Changes` line for the two
      frontends and one `### Adopter action` line naming the two lines to take out
      of `instance.env`.

### Phase 8: land it

- [ ] Branch `two-frontends` off `main`. One commit per phase, or fewer if a phase
      is one logical change. Subjects in the imperative, no issue numbers.
- [ ] Open a pull request and stop. Never push to `main`. The pull request text
      says: what retires, that a host with a non-default `TEHAS_THEME` or
      `TEHAS_LAYOUT=business` stops with 78 until its `instance.env` is edited,
      that no instance in this repository sets either, and that the app pages keep
      the floor's chrome stylesheet until the follow-up.
- [ ] Follow-ups to file as issues later, not in this change: a business chrome
      sheet for `manual.css`, `_layout.html` and `protolab.rs`; whether `/dashboard`
      folds into `/business`; a mode button on the manual pages.
