# Product

## Register

brand

## Users

A small team on the tailnet: the people who label issues for tehas and wait
for pull requests to come back. This page is what they see first: the front
page of the factory's site on port 8080. They open it to see where their work
is, usually on a desktop browser, sometimes on a phone. The plain dashboard, a
set of lists, stays at `/dashboard`.

Nobody is under pressure when they look at this page. They are checking in,
waiting on a cycle, or showing a colleague what the factory is doing.

## Product Purpose

tehas is a software factory: GitHub issues go in, reviewed pull requests come
out. The game view is its default dashboard. It shows the
factory (what is open, ready, building, in review, shipped) as a place you can
look around in, with small games built from the factory's own issues.

It succeeds when the team keeps it open, can tell at a glance what the factory
is doing, and never has to distrust what it shows. Being the default raises the
bar on that last point: playful is welcome, vague is not.

The page shows the factory's live queue and the running cycle's console, from
the same files the plain dashboard reads, and says so in the header. Where the
queue cannot be reached it falls back to fictional sample data, clearly marked;
a viewer can also load an export. The queue snapshot lists only tickets on the
line. With the live queue, prototypes come from the Protolab's own list where
the host runs one. Tickets not on the line appear only from an export or the
sample.

## Brand Personality

Tehas is serious business, but not a serious personality. Factory sim with a
16-bit feel. Three words: industrious, playful, precise.

The reference for form is Super Mario Bros. 3: its overworld map, its chunky
outlines and its level of detail. Sprites and scenes should carry about that
much information and no more. The reference for colour is The Chaos Engine on
the Amiga: muted and metallic, moss and mud, steel, brass and rust. The world
should feel like a factory that is busy and satisfying to watch run, as in a
management game.

The Amiga demoscene heritage (the Tehdas ASCII logo by Squarepusher and its
credit) belongs in the footer only. It is a signature, not the theme. The Topaz
typeface is the exception: it sets every word on the page.

Voice is short and plain, in the manner of game interface text: labels, counts,
station names. It may be dry. It does not cheer.

## Anti-references

- Gamified productivity. No points, streaks, badges, leaderboards or confetti
  that score people or their work. Games are a break; a score never changes or
  ranks a work order.
- The dark navy SaaS dashboard with stat tiles and cyan accents. That is the
  other view's territory and the classic prototype's look.
- Demoscene styling, other than the typeface, spreading beyond the footer.
- Detail beyond the SMB3 level: gradients, soft shadows, smooth shading or
  high-resolution illustration mixed into pixel scenes.

## Design Principles

1. The factory is the game. Every scene, station and sprite stands for
   something real in the pipeline. Decoration that maps to nothing is cut.
2. One world, one level of detail. Everything on the page is drawn as if for
   the same 16-bit machine, at the same pixel scale, from one palette.
3. Read-only and honest. The view never changes factory state, always says
   whether it shows sample or real data, and never invents progress.
4. Play is optional. Status is readable without starting a game, and a game
   never holds information that is not available elsewhere on the page.
5. Sign it in the footer. Scene heritage is credited once, at the bottom.

## Accessibility & Inclusion

Best effort, with the art direction winning conflicts. Keep what is cheap:
readable text sizes for status and issue titles, keyboard access to
navigation and issue lists, labels on controls, and the existing
`prefers-reduced-motion` handling for looping animation. No formal WCAG target.
