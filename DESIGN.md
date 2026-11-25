---
name: Tehas pixel design
description: The 16-bit look of a tehas factory's site, TEHAS_DESIGN=pixel.
colors:
  brass: "#ea3"
  verdigris: "#276"
  rust: "#712"
  moss: "#694"
  moss-shade: "#473"
  steel: "#ccd"
  bone: "#eec"
  bone-shade: "#dca"
  brass-light: "#fc6"
  ink: "#222"
  outline: "#111"
  slate: "#445"
  hairline: "#aab"
  crt-black: "#111"
  crt-amber: "#fc6"
  crt-dim: "#aab"
typography:
  display:
    fontFamily: "Topaz, 'Courier New', monospace"
    fontSize: "32px"
    fontWeight: 400
    lineHeight: 1
  body:
    fontFamily: "Topaz, 'Courier New', monospace"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.5
rounded:
  none: "0px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  lg: "24px"
  xl: "40px"
  section: "96px"
components:
  button:
    backgroundColor: "{colors.bone}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "0 16px"
    height: "48px"
  button-primary:
    backgroundColor: "{colors.brass}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "0 16px"
    height: "48px"
---

# Design System: Tehas pixel design

## 1. Overview

The pixel design draws the manual and the plain dashboard as a 16-bit game
would draw them: panels standing on a mossy field.

Two references, kept apart. Super Mario Bros. 3 sets the map, the outline
weight and the level of detail. The Chaos Engine (Bitmap Brothers, Amiga, art
by Dan Malone) sets the colour: moss, mud, steel, brass, rust and verdigris on
near-black outlines. Shapes are flat and square. Depth is a hard offset shadow,
never a blur. One bitmap typeface sets every word. The register is a factory
sim with a 16-bit feel: industrious, playful, precise.

This system rejects the dark navy SaaS dashboard with stat tiles and cyan
accents, gamified productivity (points, streaks, badges, confetti), demoscene
styling outside the footer, and any detail beyond the SMB3 level: gradients,
soft shadows, smooth shading.

Everything sits on an 8px grid. Motion is stepped, like sprite frames, and
`prefers-reduced-motion` turns it off.

Every colour is an Amiga colour: 12 bits, one hex digit per channel, the 4096
colours the original chipset could show.

**Key Characteristics:**

- A mossy field with bone-faced panels: muted, metallic, never candy-bright.
- Square corners everywhere. Radius is zero.
- 4px near-black outlines on anything that is an object in the world.
- Hard offset shadows with no blur.
- One typeface, Topaz, at two sizes.

## Themes

The site has two designs, and the instance's `TEHAS_DESIGN` picks one for every
page. `beesknees26`, the default, names `host/fun/theme-beesknees26.css` on
every page. `pixel` names `host/fun/theme-pixel.css` on every page; it is for
a site that is also the factory's demo. In both the plain dashboard is at
`/`. Neither server rewrites a stylesheet path: the pages are
templates that name the design's file, logo and share card.

This file describes the pixel design, and the rules below it bind that design.
The pages are drawn from role tokens, and a theme is the file that gives them
values. The stylesheets, the scripts and the markup name no colour, typeface or
shadow of their own; `test/theme_test.sh` holds them to that, and holds each
page to the theme of the instance's design.

Beesknees26 is the business look: ink and one muted green on a near-white
ground, white panels with soft shadows, IBM Plex (bundled, under the OFL), a
light top bar, no motion, and a dark set only where a page lets the viewer
pick it (`--modes` names the sets a theme has). `host/fun/pages.css` takes its
colours from the theme (`--text-faint` is the third text level, `--bar-*` the
top bar), and its sizes and corners too: the drawing's sizes raised to
`--type-small` and `--type-big`, corners in multiples of `--radius`; the
`--stage-*` tokens are for a page that draws the line. No page of tehas links
`pages.css`: Obeya and industry's workshop take its rules. The manual and the
plain dashboard draw their header and footer from `manual.css`.

The colours below by the role token that carries them:

| Pixel colour | Token | Pixel colour | Token |
| --- | --- | --- | --- |
| Moss, its texture | `--field`, `--field-pattern` | Ink | `--text` |
| Bone | `--surface` | Slate | `--text-quiet`, `--text-faint` |
| Bone Shade | `--surface-quiet` | Verdigris | `--text-cool` |
| Steel | `--surface-head` | Rust | `--text-warm` |
| Brass | `--accent` | Outline | `--line` (outlines are `--frame`, `--edge` wide) |
| Brass Light | `--accent-soft` | Hairline | `--rule` |
| CRT Black, Amber, Dim | `--screen`, `--screen-text`, `--screen-quiet`, and the top bar's `--bar-*` | Green, red tag | `--good`, `--bad` |

## 2. Colors

A 32-colour palette in the manner of The Chaos Engine: steel greys, brass,
rust, moss, mud and verdigris, held together by near-black outlines. The
tokens below are the ones the interface uses; the sprites draw on the rest.

### Primary
- **Brass** (`brass`): the action colour and the "something is
  here" colour. The primary button, the selected tab, the stripe under the
  header.

### Secondary
- **Verdigris** (`verdigris`): quiet links and inline text buttons.
- **Rust** (`rust`): ticket numbers and strong text.

### Neutral
- **Moss** (`moss`) with **Moss Shade** (`moss-shade`): the
  field and its sparse pixel texture.
- **Steel** (`steel`): the header band, panel heads and row hover.
- **Bone** (`bone`): panel faces.
- **Bone Shade** (`bone-shade`): the footer and quiet states.
- **Brass Light** (`brass-light`): an open row.
- **Ink** (`ink`): all body text.
- **Outline** (`outline`): object outlines and posts.
- **Slate** (`slate`): captions and secondary text.
- **Hairline** (`hairline`): dividers inside a panel.
- **CRT Black** (`crt-black`), **CRT Amber** (`crt-amber`) and **CRT Dim**
  (`crt-dim`): the console and code blocks. The only dark surfaces.

### Named Rules
**The Twelve Bit Rule.** Every colour, in the stylesheet and the sprites, is written as three hex digits (`#RGB`). A six-digit colour is a bug.
The logo, `host/fun/favicon.svg`, is the one exception to this rule and the
Thirty-Two Rule: it keeps its own colours and rounded corners.

**The Flat Fill Rule.** Every fill is one solid colour. Gradients are
prohibited, including on backgrounds and inside sprites.

**The Thirty-Two Rule.** The whole page, sprites included, draws from one list
of 32 colours, as a single Amiga screen would. A new colour replaces an old one;
it does not join them.

**The One Outline Rule.** Outlines are Outline and text is Ink. Nothing is
outlined in a colour.

## 3. Typography

**Display Font:** Topaz (with Courier New, monospace)
**Body Font:** Topaz (with Courier New, monospace)

**Character:** One bitmap face for everything, as on the machine it comes from.
Hierarchy comes from two sizes and from colour.

### Hierarchy
- **Display** (400, 32px, 1): the page title, panel titles and counts.
- **Body** (400, 16px, 1.5): everything else. Capitals for labels and buttons,
  sentence case for ticket titles and sentences. Cap prose at 65ch.

### Named Rules
**The Two Sizes Rule.** Text is 16px or 32px, whole multiples of the face's
native 8px grid. No other sizes, no `clamp()`, no letter-spacing, no bold and no
italics. The one exception is the footer logo, which drops to 8px on phones so
its 77 columns fit.

## 4. Elevation

Depth is drawn, not lit. An object stands off the field by a hard offset
shadow with zero blur, down and to the right. There is no ambient shadow, no
glow and no blur anywhere.

### Shadow Vocabulary
- **Card** (`box-shadow: 4px 4px 0 #333`, `--shadow-card`): buttons and
  tabs. Panels standing on the field take the Sign shadow instead.
- **Sign** (`box-shadow: 8px 8px 0 #352`, `--shadow-sign`): panels standing
  on grass. The shadow is a darker grass.

### Named Rules
**The Sprite Frame Rule.** Anything that moves, moves in steps. A lamp blinks
only for something that is true.

**The No Blur Rule.** Every shadow has a blur radius of zero. If a shadow is
soft, it is wrong.

## 5. Components

Objects are chunky and square, like tiles: thick outline, flat face, hard
shadow. Interactive targets are at least 48px tall.

**The Panel.** One recipe for the panels standing on the field: a Bone face
(Bone Shade for the console), a 4px Outline border, a Sign
shadow, the title on a Steel head closed by a 4px Outline rule, and gutters on
the children rather than on the face. Every stylesheet writes it out per panel,
so changing it means every panel in the file.

### Buttons
- **Shape:** square, 4px Outline border, card shadow, 48px tall.
- **Primary:** Brass face. **Default:** Bone face.
- **Hover:** the face changes colour and the button stands 4px further off the
  field. **Active:** it moves 4px down and right onto its shadow. State changes
  snap; nothing eases.
- **Focus:** a 4px Ink outline, offset 4px.

### Header
A Steel band that stays at the top of the window: the factory sprite and
wordmark, the site menu ending in an underlined link to the project on GitHub,
over a Brass stripe. Below 1280px the menu takes a row of its own that scrolls
sideways; on phones the wordmark gives way to the sprite alone. Everything that
sticks or scrolls to an anchor clears the header's height: `--header` in
manual.css, per width band.

### Production Console
A Bone Shade panel holding a CRT Black screen with CRT Amber text: the running
cycle's output, followed as it grows. It is the only live part of the page,
and is labelled as such.

When more than one cycle can be shown, a row of tabs stands above the screen:
one per running cycle, named by its issue number, and one for the cycle that
finished last, marked ENDED. A tab is a Bone button with the 4px outline; the
selected one is Brass.

### Manual Page
The manual and the plain dashboard wear one header and footer around one text
block. The header's menu is the site's: Dashboard, Manual, GitHub. The text block is a Bone panel with a Sign
shadow whose title is a Steel head, as on the console. Beside it stands a Bone
Shade list of the manual's pages, the current one in Brass; below 900px the
list lies across the top.

- **Text:** lines of at most 80 characters. Second headings sit on a 4px
  Outline rule, third headings are capitals behind a Brass bar.
- **Emphasis:** Topaz has one weight and no italic. Strong text is Rust,
  emphasised text has a dotted rule under it.
- **Code:** inline on Bone Shade; a block is a CRT Black screen in CRT Amber.
- **Tables:** 1px Hairline between rows, figures right-aligned, a missing
  figure a dash. They scroll sideways before they squeeze.

## 6. Do's and Don'ts

### Do:
- **Do** draw every new sprite and scene at the Super Mario Bros. 3 level of
  detail: flat fills, a dark outline, at most one highlight and one shade.
- **Do** keep corners square and shadows hard (blur 0).
- **Do** keep sizes and positions on the 8px grid, and show sprites at whole
  multiples of their native size.
- **Do** make each object stand for something real in the pipeline.
- **Do** take any new colour from the 32-colour list in the Thirty-Two Rule's
  spirit: muted, metallic, The Chaos Engine before Super Mario.
- **Do** keep the Tehdas ASCII logo and its credit in the footer.

### Don't:
- **Don't** add gamified productivity: no points, streaks, badges,
  leaderboards or confetti that score people or their work.
- **Don't** drift toward the dark navy SaaS dashboard with stat tiles and cyan
  accents.
- **Don't** let demoscene styling, other than the typeface, spread beyond the
  footer.
- **Don't** add detail beyond the SMB3 level: gradients, soft shadows, smooth
  shading or high-resolution illustration mixed into pixel scenes.
- **Don't** use `border-radius`, `filter: blur`, `backdrop-filter`, gradient
  text or a coloured stripe down one side of a box.
- **Don't** ease, fade or blur a movement. Use `steps()`, and keep one-shot
  moves under 350ms.
- **Don't** invent progress: no fake commands, random verdicts or motion that
  implies work is happening.
- **Don't** set text at any size other than 16px and 32px.
- **Don't** let copy cheer.
