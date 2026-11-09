# The host bridge

A contract between a factory's dashboard and a host that fronts the
factory's domain. For programs: it goes in `host/docs/api.md`, not in the
operating manual or the features page.

A factory without a host looks and works exactly as it does today.

## The path

The dashboard asks its own origin for `GET /host/bridge.json`. tehas never
answers anything under `/host/` itself: `host/edge/docs.Caddyfile` answers
every path under it with 404, which the dashboard reads as "no host".

The front pages and `/dashboard` ask once when a page loads, then every 60
seconds while the page is visible, with `cache: "no-store"`. A failed ask, or
no answer in 5 seconds, keeps the last good answer for up to 5 minutes, then
drops it. Every view draws first from `live/` as today; the bridge's elements
are added when an answer arrives.

## The answer

```json
{
  "bridge": 1,
  "host": "example-host",
  "time": "2026-11-08T12:00:00Z",
  "ticketLink": "/host/ticket/{n}",
  "notices": [
    {"id": "n-1", "level": "info", "text": "Example notice.", "link": "/host/notice/n-1"}
  ],
  "ticketNotes": [
    {"issue": 42, "text": "Example note", "link": "/host/ticket/42"}
  ],
  "menu": [
    {"id": "m-1", "label": "Example page", "url": "https://host.example/page"}
  ]
}
```

| Field | Rule |
| --- | --- |
| `bridge` | The contract's version, `1`. Any other value: the answer is ignored whole |
| `host` | One line, at most 40 characters. Shown nowhere |
| `time` | RFC 3339. An answer more than 5 minutes old is ignored whole |
| `ticketLink` | Optional. Where a ticket of the factory's repository links, with `{n}` for its number once. Starts with `/host/`, at most 200 characters. Absent: tickets link to GitHub |
| `notices` | Optional, at most 5. Lines at the top of the front pages |
| `notices.id` | `a-z`, `0-9` and `-`, 1 to 32 characters, unique in the list |
| `notices.level` | `info`, `warn` or `alert` |
| `notices.text` | One line, 1 to 200 characters. Control characters become spaces |
| `notices.link` | Optional. A path under `/host/`, or an `https://` address, at most 512 characters |
| `ticketNotes` | Optional, at most 200. A short note on a ticket, wherever the dashboard draws that ticket |
| `ticketNotes.issue` | 1 to 999999999. At most 3 notes per ticket; the rest are dropped |
| `ticketNotes.text` | One line, 1 to 60 characters |
| `ticketNotes.link` | Optional, as `notices.link` |
| `menu` | Optional, at most 4. Items in the front pages' menu, each opening its page in the dock as a panel does |
| `menu.id` | `a-z`, `0-9` and `-`, 1 to 32 characters, unique in the list. An item with the id of a panel takes that panel's place |
| `menu.label` | One line, 1 to 24 characters. Control characters become spaces |
| `menu.url` | An `http://` or `https://` address with no user or password, at most 512 characters, not on the dashboard's own origin |

Reading it:

- Not JSON, over 256 KiB, or a bad `bridge` or `time`: ignored whole.
- An element that breaks a rule is dropped alone; the rest is used.
- Unknown fields are ignored, so a later element type needs no new version.
- Text goes into the page with `textContent` only.
- A link is used only if it passes its rule.

## How the dashboard draws it

- **`ticketLink`**: every link to a ticket of the factory's repository goes
  there instead, same tab. Pull requests keep linking to GitHub.
- **`notices`**: a line each at the top of both front pages, in the level's
  tone, beside the alerts banner and drawn the same way.
- **`ticketNotes`**: a small chip beside the ticket's number or title in
  every view that draws it, linked when it has a link.
- **`menu`**: an item each in the menu of both front pages, after the
  panels of `live/panels.json`, four in all, opening its page in the dock as
  a panel does. An item that goes from the answer goes from the menu.

Placement and look in each design is left to the builder.

## Constraints

- The answer is data and never runs code on the dashboard's pages. A broken
  answer costs at most its own elements.
- Pages served under `/host/` share the dashboard's origin. tehas puts
  nothing there and keeps no secret on that origin; a host that fronts the
  domain is trusted as the factory's operator is.
- tehas registers no service worker, and every dashboard page unregisters
  any it finds on its origin.
- `live/` stays public; the bridge does not change it.

## Tests

- `api/host-bridge.schema.json` (JSON Schema 2020-12; unknown fields allowed
  at the top level and in each element), `api/host-bridge.full.json` with
  every field, `api/host-bridge.bare.json` with only `bridge`, `host` and
  `time`.
- The reading module's own tests: the samples, each rule's breach, text never
  markup.
- `/host/bridge.json` answers 404 from `host/edge/docs.Caddyfile`.
- Every dashboard page unregisters service workers.

## Feature

`tehas api` lists `host-bridge`.
