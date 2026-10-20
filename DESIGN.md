---
name: Tehas game view
description: The software factory as a 16-bit overworld you can look around in.
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
  floor-head:
    backgroundColor: "{colors.bone}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "12px 16px"
  station-sign:
    backgroundColor: "{colors.bone}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    padding: "8px"
    width: "264px"
  issue-row:
    backgroundColor: "{colors.bone}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "12px 16px"
  issue-row-selected:
    backgroundColor: "{colors.brass-light}"
  input:
    backgroundColor: "{colors.bone}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "0 12px"
    height: "48px"
  monitor-screen:
    backgroundColor: "{colors.crt-black}"
    textColor: "{colors.crt-amber}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "16px"
---

# Design System: Tehas game view

## 1. Overview

**Creative North Star: "The Factory Floor"**

The page is a field with signboards standing on it, drawn as a 16-bit game
would draw them. The pipeline is four stations in a row, read left to right:
in line, building, review, shipped. Each station is a signboard with a small
animated scene of the work done there.

The first screen is the whole status at a glance: the four stations side by
side, six doors beneath them in three columns and two rows, and the work order
monitor on the right. The doors lead to everything else, and four of them
carry a count (open, stopped, not on the line, prototypes); the console door
turns Brass while a cycle is running. The issue board, the break
room and the data source follow below.

Two references, kept apart. Super Mario Bros. 3 sets the map, the outline
weight and the level of detail. The Chaos Engine (Bitmap Brothers, Amiga, art
by Dan Malone) sets the colour: moss, mud, steel, brass, rust and verdigris on
near-black outlines. Shapes are flat and square. Depth is a hard offset shadow,
never a blur. One bitmap typeface sets every word. PRODUCT.md calls this
"factory sim with a 16-bit feel": industrious, playful, precise.

This system rejects the dark navy SaaS dashboard with stat tiles and cyan
accents, gamified productivity (points, streaks, badges, confetti), demoscene
styling outside the footer, and any detail beyond the SMB3 level: gradients,
soft shadows, smooth shading.

Everything sits on an 8px grid. Wide screens (1440px and up) set the stations
in a row with the monitor beside them; below that the page is one column and
every panel fills it, and below 1280px the stations stack. On wide screens the
first screen is centred in the window with equal room above and below it, and
the issue board starts under the fold.

Motion is stepped, like sprite frames. One-shot moves answer something the
viewer did or mark data arriving: signs and doors pop in on load, a changed
count latches with a two-frame flash, a changed meter reading counts up or
down to the new one in eight frames with its gauge, a selected ticket's number flies from where
it was pressed to the monitor (blank and ringed in Brass while it waits), which
then prints the ticket at 1200 bps, and a newly chosen stage prints its rows
in. The only loops
are the monitor's cursor, the station scenes, and lamps that blink only while a
cycle is really running. `prefers-reduced-motion` turns all of it off.

Every colour is an Amiga colour: 12 bits, one hex digit per channel, the 4096
colours the original chipset could show.

**Key Characteristics:**

- A mossy field with bone-faced signs: muted, metallic, never candy-bright.
- Four stations in pipeline order. Position in the row is the status.
- Square corners everywhere. Radius is zero.
- 4px near-black outlines on anything that is an object in the world.
- Hard offset shadows with no blur.
- One typeface, Topaz, at two sizes.

## Themes

This file describes the pixel theme, the default. The pages are drawn from
role tokens, and a theme is the file that gives them values:
`host/fun/theme-<name>.css`, served as `/fun/theme.css` for the theme the
instance names in `TEHAS_THEME`. The stylesheets, the scripts and the markup
name no colour, typeface or shadow of their own; `test/theme_test.sh` holds
them to that. The second theme, paper, keeps the layout, the two text sizes and
the sprites and changes the rest: the system typeface, 1px rules, light
screens and soft shadows. The rules below that are about pixels (Topaz, the
Twelve Bit Rule, no blur, stepped lifts) bind the pixel theme only.

The colours below by the role token that carries them:

| Pixel colour | Token | Pixel colour | Token |
| --- | --- | --- | --- |
| Moss, its texture | `--field`, `--field-pattern` | Ink | `--text` |
| Bone | `--surface` | Slate | `--text-quiet` |
| Bone Shade | `--surface-quiet` | Verdigris | `--text-cool` |
| Steel | `--surface-head` | Rust | `--text-warm` |
| Brass | `--accent` | Outline | `--line` (outlines are `--frame`, `--edge` wide) |
| Brass Light | `--accent-soft` | Hairline | `--rule` |
| CRT Black, Amber, Dim | `--screen`, `--screen-text`, `--screen-quiet` | Green, red tag | `--good`, `--bad` |

The canvases cannot use `var()`: `host/fun/theme.js` reads their tokens
(`--band-*`, `--piece-*`, `--game-*`) from the theme.

## 2. Colors

A 32-colour palette in the manner of The Chaos Engine: steel greys, brass,
rust, moss, mud and verdigris, held together by near-black outlines. The
tokens below are the ones the interface uses; the sprites draw on the rest.

### Primary
- **Brass** (`brass`): the action colour and the "something is
  here" colour. The primary button, the ring around a station whose tickets
  the board is showing, the stopped
  door when something is stopped, the closed gate, the sample marker, the
  stripe under the header.

### Secondary
- **Verdigris** (`verdigris`): counts for the cool ends of the line (in
  line, shipped), game readouts and inline text buttons.
- **Rust** (`rust`): ticket numbers, and counts for the warm stages
  (building, review).

### Neutral
- **Moss** (`moss`) with **Moss Shade** (`moss-shade`): the
  field and its sparse pixel texture.
- **Steel** (`steel`): the header band, panel heads, doors and row hover.
- **Bone** (`bone`): sign and panel faces.
- **Bone Shade** (`bone-shade`): the footer, the monitor case, quiet states.
- **Brass Light** (`brass-light`): the selected ticket, wherever it
  appears.
- **Ink** (`ink`): all body text.
- **Outline** (`outline`): object outlines and posts.
- **Slate** (`slate`): captions and secondary text.
- **Hairline** (`hairline`): dividers inside a panel.
- **CRT Black** (`crt-black`), **CRT Amber** (`crt-amber`) and **CRT Dim**
  (`crt-dim`): the monitor screen, the console, the energy chart and the game
  canvas. The only dark surfaces.

### Named Rules
**The Twelve Bit Rule.** Every colour, in the stylesheet, the canvas and the
sprites, is written as three hex digits (`#RGB`). A six-digit colour is a bug.

**The Flat Fill Rule.** Every fill is one solid colour. Gradients are
prohibited, including on backgrounds and inside sprites.

**The Thirty-Two Rule.** The whole page, sprites included, draws from one list
of 32 colours, as a single Amiga screen would. A new colour replaces an old one;
it does not join them.

**The One Outline Rule.** Outlines are Outline and text is Ink. Nothing is
outlined in a colour.

**The Position Rule.** Which station a ticket stands at says what state it is
in. Colour never carries status by itself.

## 3. Typography

**Display Font:** Topaz (with Courier New, monospace)
**Body Font:** Topaz (with Courier New, monospace)

**Character:** One bitmap face for everything, as on the machine it comes from.
Hierarchy comes from two sizes and from colour.

### Hierarchy
- **Display** (400, 32px, 1): the page title, station names, counts, panel
  titles, the work order number.
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
- **Card** (`box-shadow: 4px 4px 0 #333`, `--shadow-card`): buttons and the
  gate. Panels standing on the field take the Sign shadow instead.
- **Sign** (`box-shadow: 8px 8px 0 #352`, `--shadow-sign`): signs and panels
  standing on grass. The shadow is a darker grass.
- **Ring** (`box-shadow: 0 0 0 4px #ea3`): the station whose tickets the
  board is showing, combined with the sign shadow.

### Named Rules
**The Sprite Frame Rule.** Anything that moves, moves in steps. A lamp blinks
only for something that is true.

**The No Blur Rule.** Every shadow has a blur radius of zero. If a shadow is
soft, it is wrong.

## 5. Components

Objects are chunky and square, like tiles: thick outline, flat face, hard
shadow. Interactive targets are at least 48px tall.

**The Panel.** One recipe for the panels standing on the field: a Bone face
(Bone shade for the console and the break room), a 4px Outline border, a Sign
shadow, the title on a Steel head closed by a 4px Outline rule, and gutters on
the children rather than on the face. Every stylesheet writes it out per panel,
so changing it means every panel in the file.

### Buttons
- **Shape:** square, 4px Outline border, card shadow, 48px tall.
- **Primary:** Brass face. **Default:** Bone face.
- **Hover:** the face changes colour and the button stands 4px further off the
  field. **Active:** it moves 4px down and right onto its shadow. State changes
  snap; nothing eases.
- **Focus:** a 4px Ink outline, offset 4px. On the monitor screen the
  outline is CRT Amber.
- **Text buttons** (a station's "all N" control) are underlined Verdigris.

### Floor Head
The panel at the head of the floor: one of the field's panels, drawn like the
board, its title on a Steel head closed by a 4px Outline rule. Under it one line on what the factory
is, held to 110ch, the one prose measure wider than the 65ch cap, and one
paragraph in Slate that counts the queue (open, and how many stand at each
station, counting stopped and not on the line) and says when no ticket data is
loaded, when the factory is paused, or when the last cycle failed. The room
above the panel belongs to the field, not to its face.

At the head panel stands the live meter, there only while the model
mesh reports (see Energy Use): the draw now in watts and the tokens of the last
minute, each in display type over a gauge of ten blocks, Brass for the filled
ones. The power gauge is read against the chart's scale, the token gauge against
the busiest minute of the last half hour. Where the mesh counts no tokens the
meter shows power alone. It leads to Energy Use.
The meter costs width and is given a column of its own, 336px where the stage
has its monitor column. From 1440px it stands in that column as a card of its
own beside the panel, over the work order monitor: Bone Shade, a 4px Outline,
the sign shadow, the same left and right edges as the monitor column below it,
its top and bottom edges the panel's own whatever rows the panel holds, and the
field in the 24px between them. The panel gives up that room, so its
face ends at the stage's first column. Below 1440px the meter is a Bone Shade
band inside the panel again, over the height of the Steel head and the lines
under it, closed off from the text by a 4px Outline rule in a column of 240px;
below 900px the panel is one column and the meter lies across its foot at full
width. Where the two do not fit side by side it is the meter that moves, never
the text.

### Station Sign
A signboard: the station's scene, the name and count in display type, one
ticket (next up, in work, needs review, latest) with its title held to two
lines, and a text button that filters the board to that station. The scene's
frame sits outside the picture so the art fills it edge to edge. From 1440px
the four signs share the stage's first column equally; from 1280 they share the
whole width of the one-column stage; below 1280px the scene sits beside the text
at its native 240 by 130.

### Factory State Tag
A tag at the far right of the header that says how the factory is. Green (`#8a5`) and
"FACTORY RUNNING!" while it takes work; Brass and "FACTORY PAUSED", blinking,
when it is paused; red (`#a32`, Bone text) and "FACTORY FAILED" when the last
cycle ended as anything but merged or handoff. Green and red are used nowhere
else on the page.

### Doors
Six Steel tiles in three columns (two on narrow screens): a name, a line of
status beneath it, and a count in display type where there is one. The stopped
door turns Brass when anything is stopped. A pressed door moves onto its
shadow. The live meter is not one of the doors: it stands at the head panel,
as a card beside it where the stage has its monitor column.

### Monitor
The selected work order on a cream-cased screen: number in display type, then
title, stage, labels, then the ticket's history (created, changed, tokens, time
and an energy estimate marked with a tilde) and a link when the data has one.
On wide screens it is as tall as the stations and doors beside it,
stands to the right of the stations and stays in view while the page scrolls.
Below 1440px it is a panel in the single column, as wide as the board, and
scrolls away with the page;
pressing a row there still marks the row and prints the order, only the flying
number is skipped when the monitor is off screen. The row that was pressed
latches in Brass for a beat instead, wherever it stands.

### Issue Board
- **Rows:** number in Rust, title, stage. 1px Hairline between rows.
  Hover is Steel; the selected row is Brass Light. The two never match.
  A row you press latches in Brass for a beat, so the press is answered where
  it landed even when the monitor is off screen.
- **Tools:** a labelled search field and stage select, 48px tall with a 4px
  outline, and a clear button that appears only when something is narrowed.

### Protolab
A panel in the board's style: tickets at the prototype stage (open, labelled
`tehas:prototype`) on the left and the lab scene on a Bone Shade ground on the
right. With the live queue a row opens the ticket's Protolab page and says how
many files it holds, with a "GITHUB" link beside it; from an export or the
sample a row selects the ticket and has an "OPEN" link when the data has one. The lab is not a stage, so its rows still
show each ticket's stage. The scene is a 20 second loop: an idea, a sketch, the
notebook handed over, read and set down. It carries no verdict.

### Protolab Page
The Protolab's own site wears the header and footer of the factory floor; its
wordmark reads PROTOLAB. The front page is the Protolab panel alone. A
ticket's page is two objects on the field:

- **The ticket:** a Bone sign with a Steel head, "PROTOTYPE" and the number in
  Rust, both in display type. Under it the title in display type (body size
  on a phone), who opened it and when it changed, and one Brass button to the
  issue.
- **The box:** a Bone Shade panel with a Steel head, "ASSETS", the count and
  how much of the 10 megabytes is used, as a figure and as ten blocks, Slate
  for each started tenth. Everything attached to the ticket is inside it.
- **A file in its frame:** a 4px Outline frame with a card shadow, the picture
  or video on CRT Black, a Bone plate under it with the name, the type and
  size in Slate, and a Verdigris "OPEN". Frames stand two abreast when there
  is room. An HTML prototype takes the whole row at 560px and keeps a page's
  white ground, the one colour here that is not the system's, because it is
  the prototype's own.
- **Lists:** the files an HTML page uses, and what was left out with the
  reason, as Bone lists with a Steel head and Hairline rules.

Nothing on these pages moves, and they carry no script.

### Header
A Steel band that stays at the top of the window: the factory sprite and
wordmark, the data source tag (Brass for sample data or a stale queue, Bone
for a loaded export, absent while the page shows the fresh live queue), the
section menu ending in an underlined link to the project on GitHub, over a
Brass stripe. Below 1280px the
menu takes a row of its own: on the factory floor its links wrap into as many
rows as they need, where the manual and Protolab headers, which hold four
links, keep one row that scrolls sideways; on phones the wordmark gives way
to the sprite alone. Everything that sticks or scrolls to an anchor clears the
header's height, so the clearance a page uses is the band that page really
makes: `--header` in game-view.css, per width band, and taller on the factory
floor where the menu wraps.

### Production Console
A Bone Shade panel holding a CRT Black screen with CRT Amber text: the running
cycle's output, followed as it grows. A tag in the panel head says LIVE (Brass),
ENDED or NO CONSOLE, and its door says the same in words. It and the Factory
Energy panel are the only live parts of the page, and both are labelled as such.

When more than one cycle can be shown, a row of tabs stands above the screen:
one per running cycle, named by its issue number, and one for the cycle that
finished last, marked ENDED. A tab is a Bone button with the 4px outline; the
selected one is Brass. The tag in the panel head describes the selected cycle.
The door and the lamp say LIVE while any cycle runs. The plain dashboard has
the same row.

### Energy Use
A Bone Shade panel in the console's style, shown only when the model backend
reports power: a Steel head with the count of hosts reporting and a tag that
says LIVE (Brass) or STALE; the mesh's draw now and its kWh in display type; a
CRT Black screen with the last 30 minutes as a stack of columns; a table of
machines and a table of models. From 1280px the models stand beside the screen
and the machines run beneath both; below that the three stack. It spans the
stage under the monitor's column.

- **Chart:** a 480 by 136 canvas at its native size. 120 columns
  of 4 pixels, one per 15 seconds. Each column is flat blocks, one per machine,
  in the order of the rows beneath. No lines, no curves, nothing translucent.
  A missed sample is an empty column. Scale labels are page text beside the
  canvas, in CRT Dim.
- **Band colours** (`--band-1` to `--band-8`): `#ea3`, `#4a9`, `#c53`, `#7cb`, `#ccd`, `#b97`, `#694`,
  `#fc6`, in that order. From the ninth machine the list starts again as a
  one-pixel checker with CRT Black. A row's swatch is drawn the same way.
- **Tables:** 1px Hairline between rows, figures right-aligned, a missing
  figure a dash. Only machines serving the factory and the models it uses are
  listed, so no row carries a mark. RAM is ten blocks, Slate for each started tenth in use.
  A model's engines follow its hosts, and leave the table with them on narrow
  screens.
- **Data source:** one line of small print at the foot of the panel, Slate
  with the name an Ink link, as in the page's footer.
- **Stillness:** nothing here animates and no figure flashes. The figures
  change every 15 seconds, and motion that often would say nothing.

### Manual Page
The manual and the plain dashboard wear the same header and footer as the
factory floor, around one text block. The header's menu is the site's: Factory
floor, Dashboard, Manual, GitHub. The text block is a Bone panel with a Sign
shadow whose title is a Steel head, as on the console. Beside it stands a Bone
Shade list of the manual's pages, the current one in Brass; below 900px the
list lies across the top.

- **Text:** lines of at most 80 characters. Second headings sit on a 4px
  Outline rule, third headings are capitals behind a Brass bar.
- **Emphasis:** Topaz has one weight and no italic. Strong text is Rust,
  emphasised text has a dotted rule under it.
- **Code:** inline on Bone Shade; a block is a CRT Black screen in CRT Amber.
- **Tables:** as in Energy Use, and they scroll sideways before they squeeze.

### Breakroom
A full-width panel on a Bone Shade ground with a Steel head, like the other
panels. Inside it a CRT Black cabinet: the 480 by 272 canvas scaled to fill
the width, beside a bone panel with the readout and one play button. On a
narrow screen the bone panel goes below the canvas.

The wall is a grid of 28 by 28 pixel cells, sixteen columns by six rows, one
letter in each. A word is written on tetrominoes: one of the seven pieces for
its first four letters, another for the next four, a third for nine or ten. The
pieces of a word touch one another and share a colour; pieces of words that
touch differ in colour. With words enough for the wall a round picks the ones
whose pieces fill it exactly, so it is a packed rectangle with no holes, hanging
from the top of the canvas, and never bigger than those twenty-four pieces
however many words there are; with a small pool every word goes on the wall and
its lower edge may be uneven. A hit takes the one piece it touched, and a word
scores when its last piece goes.

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
- **Do** say in the header when the data is not the fresh live queue: sample,
  a loaded export or a stale queue.
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
- **Don't** let copy cheer. The one exclamation mark on the page is the factory
  state tag's "FACTORY RUNNING!".
