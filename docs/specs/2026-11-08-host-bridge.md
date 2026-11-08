# The host bridge

## Why

A factory is a single-project appliance, and it must stay whole on its own.
Some factories are run by a host, tehas-industry, that sees more than one
factory can: the other factories in its network, a ticket page with a chat,
security findings from its night guard, stats across factories. Today each
thing industry wants to show on a factory's dashboard needs its own command or
file in tehas (`tehas panel`, `state/industry/alerts.json`), or it does not
show at all. Ticket links are the latest case: industry has a ticket page, but
the dashboard can only link a ticket to GitHub.

The host bridge is one contract for all of this. A factory on its own looks
and works exactly as it does today, with nothing missing. A factory with a host
gets more, drawn natively by its own dashboard, from data the host bridges in.

## Goals

- One path on the factory's own origin where a host answers with what it adds.
- A small, fixed set of element types that the dashboard draws in its own
  style, from data, never from the host's code.
- Ticket links: with a host, a ticket on the dashboard opens the host's ticket
  page on the factory's own domain.
- A written contract between factory and host, with a schema and samples both
  sides test against, as `tehas status --json` has.

## Non-goals

- Running the host's code inside the dashboard's pages. The host sends data;
  tehas draws it.
- Telling people about it. The bridge is for programs: it is in
  `host/docs/api.md`, not in the operating manual or the features page.
- Moving the panels or the alerts file onto the bridge now. Both keep working
  as they are. Each may move later, as its own change.
- Named places on the front pages for a host's elements, and one shape for
  every seam between factory and host. Both come later, once a few uses of the
  bridge are built and show what the front ends need.
- Anything tehas must know about industry. tehas knows the path and the shape
  of the answer, not who answers or where the data comes from.

## Anti-goals

These would make it a failure even if it worked:

- A standalone factory with an empty space, a broken link, a spinner or an
  error where a host's element would be.
- A host's data that tehas trusts: markup that reaches the page, a link that
  leaves for an address the contract does not allow, or a malformed answer that
  breaks a view.
- Private data in `live/`. Everything there is public. What only signed-in
  people may see never passes through the factory's files.

## How it works

### The path

The dashboard asks its own origin for `GET /host/bridge.json`. tehas never
answers anything under `/host/` itself: `host/edge/docs.Caddyfile` answers
every path under it with 404, so on its own the factory says "no host".

A host that fronts the factory's domain routes `/host/*` to itself. Industry
does this already for `/mcp` on each factory's site. Because the answer comes
from the dashboard's own origin, the browser sends it whatever cookies the
host set on that origin, and the host decides per request what to answer: more
to a person signed in to it, less or nothing to anyone else. The factory never
holds that data.

### When the dashboard asks

The front pages and the plain dashboard at `/dashboard` all ask, so a ticket
links the same way on every page. Once when a page loads, then every 60 seconds while the page is visible,
with `cache: "no-store"`. A failed or slow ask (no answer in 5 seconds) keeps
the last good answer for up to 5 minutes, then drops it and the page draws as
a standalone factory. Nothing waits on the bridge: every view draws first from
`live/` as today, and the host's elements are added when the answer arrives.

### The answer

```json
{
  "bridge": 1,
  "host": "tehas-industry",
  "time": "2026-11-08T12:00:00Z",
  "ticketLink": "/host/ticket/{n}",
  "notices": [
    {"id": "ng-17", "level": "warn", "text": "A dependency has a known vulnerability. A fix is being prepared.", "link": "/host/notice/ng-17"}
  ],
  "ticketNotes": [
    {"issue": 42, "text": "blocked by tehas-lab #7", "link": "/host/ticket/42"}
  ]
}
```

| Field | Rule |
| --- | --- |
| `bridge` | The contract's version, `1`. Any other value: the answer is ignored whole |
| `host` | Who answers, one line, at most 40 characters. Shown nowhere; for logs and debugging |
| `time` | When the host made the answer, RFC 3339. An answer more than 5 minutes old is ignored whole |
| `ticketLink` | Optional. Where a ticket opens, with `{n}` for its number once. Must start with `/host/` and be at most 200 characters. Absent: tickets link to GitHub as today |
| `notices` | Optional, at most 5. Heads-ups shown at the top of the front pages |
| `notices.id` | `a-z`, `0-9` and `-`, 1 to 32 characters, unique in the list |
| `notices.level` | `info`, `warn` or `alert` |
| `notices.text` | One line, 1 to 200 characters. Control characters become spaces |
| `notices.link` | Optional. A path under `/host/`, or an `https://` address, at most 512 characters |
| `ticketNotes` | Optional, at most 200. A short note on a ticket, wherever the dashboard draws that ticket |
| `ticketNotes.issue` | The ticket's number, 1 to 999999999. At most 3 notes per ticket; the rest are dropped |
| `ticketNotes.text` | One line, 1 to 60 characters |
| `ticketNotes.link` | Optional, as `notices.link` |

The rules on reading it:

- An answer that is not JSON, is over 256 KiB, or breaks a rule on `bridge` or
  `time` is ignored whole.
- An element that breaks a rule is dropped, alone. The rest of the answer is
  used.
- A field tehas does not know is ignored. This is how a later element type is
  added without a new version: an older factory skips it.
- All text goes into the page with `textContent`, never as markup.
- A link is used only if it passes its rule. `/host/` paths stay on the
  dashboard's own origin, so the host's pages open where the factory is.

### The element types

- **Ticket link.** With `ticketLink`, every place the dashboard links a ticket
  of the factory's own repository links to `ticketLink` with `{n}` replaced,
  in the same tab. The issue on GitHub stays one step away: the host's ticket
  page links to it. A pull request still links to GitHub. Without
  `ticketLink`, nothing changes.
- **Notice.** A line at the top of both front pages, in the level's tone, with
  its text and, when it has one, a link. Notices sit beside the alerts banner
  from `state/industry/alerts.json` and look the same.
- **Ticket note.** A small chip beside the ticket's number or title, in every
  view that draws the ticket. With a link, the chip links there.

Which views draw tickets, and where exactly each element sits in each design
(beesknees26 and pixel), is left to the builder, within the rules above.

### What tehas tests

As for the status contract:

- `api/host-bridge.schema.json`, a JSON Schema (draft 2020-12), closed except
  that unknown fields are allowed at the top level and in each element.
- `api/host-bridge.full.json`, a sample with every field, and
  `api/host-bridge.bare.json`, a sample with only `bridge`, `host` and `time`.
- The dashboard's reading of the answer is one module with its own tests: the
  samples read as expected, each rule's breach drops what the rule says, and
  text never becomes markup.
- A test of `host/edge/docs.Caddyfile` that `/host/bridge.json` answers 404.
- A test that every dashboard page unregisters service workers.

tehas-industry copies the schema and samples into its `factory-api` fixtures
and checks its own answer against them on every pull request.

### What it says it has

`tehas api` lists the feature `host-bridge`, so a host can tell a factory that
predates the bridge from one where it failed.

## Constraints

- The bridge's answer is data. A broken or hostile answer costs at most its
  own elements and never runs code on the dashboard's pages.
- The pages a host serves under `/host/` are another matter: they share the
  dashboard's origin, so they could script its pages. tehas does not try to
  stop that. A host already runs commands as the factory's user, so it is
  trusted as much as the person who runs the factory. tehas puts nothing under
  `/host/` and keeps no secret on the dashboard's origin. Cookies the host sets
  there are its own. (A call made while writing this spec.)
- A factory that loses its host goes back to standalone in the browser too.
  tehas registers no service worker, so every dashboard page unregisters any
  service worker it finds on its origin, which removes one a past host left
  behind. (A call made while writing this spec.)
- Everything under `live/` stays public, and the bridge does not change that.

## The host's side

What industry does to answer the bridge is industry's to design, in its own
repository: the route, the ticket page on each factory's domain with its
sign-in, where each notice and note comes from, and switching industry's own
ticket links over. This spec only fixes what it must answer.

## Left to the builder

- Where each element sits in each design, and how it looks.
- Whether the reading module is plain JavaScript beside `panels.js` or shared
  with another view.
- The wording of the `api.md` section, within this contract.
