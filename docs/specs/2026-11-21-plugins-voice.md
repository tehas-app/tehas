# Spec: plugins, a voice chat in each session

**Version**: part of the plugin specification 1.1.0, `2026-11-21-plugins-1.0.0.md`.

**Status**: Draft 2026-11-21. A revision of 1.0.0: it adds surfaces, so it
moves the minor number. Builds on v1, v2 and v3. Nothing is built, and
nothing is built before v3's work has landed. Owner's decisions:

- A fourth plugin, `voice`, from tehas-app/tehas-plugin-voice, adds a voice
  chat button to each chat session.
- While a voice chat is on, every other action on the page is off. Ending
  the voice chat is the only thing a person can do until it ends.
- The default model is Haru-4B (huggingface.co/higurashi-ai/Haru-4B,
  Apache-2.0). It runs in the browser if it can. It cannot (below), so it
  runs on a host on the factory's tailnet that the owner provides.

## Purpose

A person talks to a session instead of typing. What was said comes back to
the session as text the person sends, so the chat's record stays text and
every write still goes through a person's press.

## Why not in the browser

Haru-4B is a full-duplex speech-to-speech model: it listens and speaks at
once, one 133 ms frame of 24 kHz audio at a time. It has about 5 billion
parameters in BF16 (about 10 GB), needs a GPU with about 12 GB, and runs
under its own PyTorch server (`python -m haru.server`). It has no browser
build. So the browser carries audio and the model runs on a speech host.

Haru-4B is a base model. Its card says it does not follow instructions as an
assistant does. It talks; it does not act on the session. The plugin never
gives it tools.

## What the person sees

1. Each chat session in Obeya, the industry wizard and the assistant shows a
   voice button that tehas draws beside the send control. The button names
   the plugin.
2. A press asks the browser for the microphone, then opens the plugin's
   voice view over the whole page (`open: full`).
3. While it is open the rest of the page is locked: every control is
   disabled and takes no input, keyboard shortcuts included. The only
   control is End, which tehas draws, not the plugin.
4. End, the limit, a lost connection or closing the tab ends the voice chat
   and releases the lock.
5. The transcript of the voice chat appears in the session's input as a
   proposed message. The person edits it, sends it or discards it. Nothing
   is sent without the press.

## The lock is tehas's

The lock is a page rule of the host, not plugin code on the dashboard's
origin (widgets stay out, v2 menu part).

| Rule | Text |
| --- | --- |
| Who draws it | The host page sets `inert` on everything but the voice view's frame and the End control, and drops its own shortcuts and actions while the lock is held |
| Who holds it | The host, for one voice view. The plugin asks for it by opening; it cannot take it any other way and cannot hide End |
| How long | At most `voice:<minutes>` (at most 30). At the limit tehas ends the chat |
| Release | End, the limit, the relay closing, the frame unloading, or the tab closing. A release is never the plugin's decision alone |
| Scope | The page the person pressed on. Other tabs and other people are not locked. The server refuses nothing while a page is locked; the lock keeps a person from mixing a voice turn with typed actions on that page |
| Fail closed | A host that cannot set the lock does not draw the button |

## New capabilities

| Capability | What it gives | What a person should weigh |
| --- | --- | --- |
| `chat:button` | tehas draws the plugin's button in each session of the chats the plugin names in `chats`. A press opens the plugin's `full` view with the session's id | The button is in every session of those chats |
| `lock` | The view, opened from the button, holds the page lock above | While it is open, the person can only end it |
| `mic` | The view's frame may ask for the microphone: `Permissions-Policy: microphone=(self)` on the plugin's origin and `allow="microphone"` on its frame. The browser asks the person | What the person says goes to the relay's endpoint |
| `relay:<id>` | The view may open one WebSocket at `/_relay/<id>`, which the gate joins to an endpoint the instance names | The endpoint hears the audio. It is a host the factory's operator chose |
| `voice:<minutes>` | The longest voice chat, at most 30 | |
| `propose` | At the end, the view hands the session text to put in its input, at most 8 KiB | The person sees the whole text and sends it or not |

None is granted by default. A plugin with `lock` needs `chat:button` and a
view with `open: full`.

## The relay

v2's `net:` stays as it is: HTTPS fetches, public addresses only. A speech
host on a tailnet is private and the audio is a stream, so it gets a route
of its own.

The endpoint is a setting of the instance, never of the plugin or of tehas:

```
TEHAS_PLUGIN_RELAYS="voice:speech=wss://speech.example.ts.net:8998/ws"
```

Each entry: `<plugin>:<id>=<url>`. At most 2 per plugin. The plugin's
manifest names the `id`; the grant line grants `relay:<id>`; the instance
gives the URL. A missing entry refuses the plugin at `tehas update`, exit 78
for a malformed one.

| Rule | Text |
| --- | --- |
| Address | `wss://` with a host name and a port, no user or query. The name is resolved once and the connection goes to the address checked. Private and CGNAT (tailnet) addresses are allowed here because the operator named them. Refused still: loopback, link-local, multicast, "this network", the host's own addresses and the Docker bridge ranges |
| TLS | The system's roots. A tailnet name's certificate (`tailscale cert`) passes |
| What crosses | Binary and text frames, both ways. Nothing of the browser: no cookie, no `Origin`, no `X-Tehas-Login`. tehas sets the request's headers |
| Who | A signed-in person, with v1's session, `Origin` and `Sec-Fetch` rules on the upgrade |
| How many | One relay per login, and `concurrent` (manifest, default 1) per plugin. Over it: the button says the voice chat is in use |
| Rate | At most 64 KiB a second each way. 24 kHz audio needs far less |
| Time | Closed at `voice:<minutes>`, at 60 seconds with no frame, or when the lock is released |
| Kept | Nothing. Inspection rows keep the start, the end, the login and the byte counts, never audio or text |

`console/core/src/plugin/relay.rs`, used by `tehas-plugin-gate`.

## Manifest

```json
{
  "plugin": 1,
  "name": "voice",
  "build": {"dockerfile": "Dockerfile"},
  "views": [{"id": "talk", "open": "full", "path": "/"}],
  "chats": ["obeya", "wizard", "assistant"],
  "button": {"label": "Voice", "view": "talk"},
  "relays": [{"id": "speech", "concurrent": 1}],
  "asks": ["view", "chat:button", "lock", "mic", "relay:speech", "voice:15", "propose"]
}
```

| Field | Rule |
| --- | --- |
| `button` | Optional. `label` 1 to 16 characters, `view` a view with `open: full`. Needs `chat:button` |
| `relays` | Optional, at most 2. Each `id` (`a-z`, `0-9`, 1 to 12) and `concurrent` (1 to 4). Needs `relay:<id>` |
| `chats` | v2's field. With `chat:button` it names the chats that draw the button |

The plugin has a build for its view's files only. No `product`, no `model`,
no `service`: the audio never enters a plugin container.

## Hosts

The button and the lock are drawn by each chat host, as the chats part's
skills are:

| Host | Where |
| --- | --- |
| Obeya | its chat's input row |
| Industry's wizard | the wizard's chat input |
| The assistant | its input |

A host learns the plugin's buttons from the factory API, as it learns skills.
A host that has not built the lock draws no button.

## The speech host

Outside tehas. The plugin's repository says how to run it:

- Haru's server (`github.com/higurashi-ai/Haru`) with `--hf-repo
  higurashi-ai/Haru-4B`, a GPU with about 12 GB, behind `tailscale serve`
  or a TLS terminator, on the factory's tailnet.
- The model is the plugin's default. A different model is a different
  endpoint in `TEHAS_PLUGIN_RELAYS` and a view that speaks its protocol.
- The voice clip Haru speaks with is the speech host's setting. The model
  ships none.

The view speaks Haru's frame protocol over the relay; tehas does not read
the frames.

## The transcript

Haru gives text with each frame. The view joins the person's and the model's
text into a transcript and, at End, passes it to the host with `propose`.
The host shows it in the session's input marked as from the voice plugin
(the chats part's label). It is text the person sends, so it is the person's
message, under the chats part's taint rule while it holds the plugin's text.

## Changes to 1.0.0

| Where | Change | Why |
| --- | --- | --- |
| Web part, the browser's policy | `Permissions-Policy` on every plugin address denies `microphone`, `camera` and `geolocation`; `mic` lifts `microphone` for its view only | No plugin gets a device by accident |
| Web part, the gate | Adds `/_relay/<id>`, the WebSocket upgrade, and its rules | The audio stream |
| v2 menu part, widgets | Unchanged. The button is drawn by the host, from the manifest's text | No plugin code on the dashboard's origin |
| v3, `open: full` | A `full` view opened from a button may hold the lock | The owner's rule |
| Settings | `TEHAS_PLUGIN_RELAYS`, validated in `lib/config.sh` like the other plugin settings | The endpoint is the instance's |

## Open points

1. Haru's server protocol: what the web demo at port 8998 speaks, and whether
   the relay passes it as is. To be read from the Haru repository before Work 3.
2. Whether the voice model gets the session's last messages as its card (its
   system prompt). That sends session text to the speech host. Not in 1.1.0.
3. Whether the lock should also be held server side, refusing that login's
   writes while a voice chat is on.
4. Haru is a research base model. Whether it is good enough for a session, or
   whether the default should move to an instruction-tuned voice model when
   one exists.

## Work

After v3's work has landed, in order. Each is a pull request; a security
change says so.

1. **Settings and manifest.** `TEHAS_PLUGIN_RELAYS`, `button`, `relays`, the
   six capabilities, refused by name until each is enforced. Security change.
2. **The device policy.** `Permissions-Policy` on every plugin address, `mic`.
   Security change.
3. **The relay.** `relay.rs`, `/_relay/<id>`, its caps and inspection rows.
   Security change.
4. **The button and the lock** in each host: Obeya, the wizard, the
   assistant, through the factory API.
5. **`propose`** into the session's input.
6. **The plugin.** tehas-app/tehas-plugin-voice: the view, the frame
   protocol, the speech host's how-to.
7. **The manual.** `host/docs/plugins.md`, `features.md`, the changelog
   fragment.
