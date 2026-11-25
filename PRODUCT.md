# Product

## Register

brand

## Users

A small team on the tailnet: the people who label issues for tehas and wait
for pull requests to come back. They open the factory's site on port 8080 to
see where their work is, usually on a desktop browser, sometimes on a phone.
The front page is the plain dashboard; the manual is beside it.

Nobody is under pressure when they look at the site. They are checking in,
waiting on a cycle, or showing a colleague what the factory is doing.

## Product Purpose

tehas is a software factory: GitHub issues go in, reviewed pull requests come
out. The site shows the factory's live queue and the running cycle's console,
and the manual that says how the factory is run.

It succeeds when the team keeps it open, can tell at a glance what the factory
is doing, and never has to distrust what it shows. The design may be playful;
the facts may not be vague.

## Brand Personality

Tehas is serious business, but not a serious personality. Three words:
industrious, playful, precise.

The pixel design (`TEHAS_DESIGN=pixel`) carries that personality: the site
drawn as a 16-bit machine would draw it, for a factory that is shown and
demonstrated. The reference for form is Super Mario Bros. 3: its chunky
outlines and its level of detail. The reference for colour is The Chaos Engine
on the Amiga: muted and metallic, moss and mud, steel, brass and rust.

The Amiga demoscene heritage (the Tehdas ASCII logo by Squarepusher and its
credit) belongs in the footer only. It is a signature, not the theme.

A factory that sets nothing is in the business register everywhere:
beesknees26, IBM Plex on white panels with one muted green. Same voice, same
facts, no 16-bit clothing.

Voice is short and plain, in the manner of game interface text: labels and
counts. It may be dry. It does not cheer.

## Anti-references

- Gamified productivity. No points, streaks, badges, leaderboards or confetti
  that score people or their work.
- The dark navy SaaS dashboard with stat tiles and cyan accents.
- Demoscene styling, other than the typeface, spreading beyond the footer.
- Detail beyond the SMB3 level: gradients, soft shadows, smooth shading or
  high-resolution illustration mixed into pixel art.

## Design Principles

1. Decoration maps to something. A sprite or a lamp stands for something
   real in the factory, or it is cut.
2. One world, one level of detail. Everything on a page is drawn as if for
   the same 16-bit machine, at the same pixel scale, from one palette.
3. Read-only and honest. The site never changes factory state and never
   invents progress.
4. Sign it in the footer. The demoscene heritage is credited once, at the
   bottom.

## Accessibility & Inclusion

Best effort, with the art direction winning conflicts. Keep what is cheap:
readable text sizes for status and issue titles, keyboard access to
navigation and issue lists, labels on controls, and the existing
`prefers-reduced-motion` handling for looping animation. No formal WCAG target.
