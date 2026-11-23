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

Corrected in 1.6.0 against Haru's server as read at commit `7fe6b76` and a
live run by the voice plugin (`tehas-app/tehas-plugin-voice`,
`docs/spec.md`, C1 to C12). Each correction is made in place below and
listed in "Corrections in 1.6.0".

## Purpose

A person talks to a session instead of typing. What was said comes back to
the session as text the person sends, so the chat's record stays text and
every write still goes through a person's press. With Haru, only the
model's side comes back as text (see "The transcript").

## Why not in the browser

Haru-4B is a full-duplex speech-to-speech model: it listens and speaks at
once, one 133 ms frame of 24 kHz audio at a time. It has about 5 billion
parameters in BF16 (about 10 GB), needs a GPU with about 12 GB, and runs
under its own PyTorch server (`python -m haru.server`). It has no browser
build. So the browser carries audio and the model runs on a speech host.

The browser's part is the codec. A view's policy has `worker-src 'none'`
and no wasm, so Haru's own client's codec, a wasm worker, cannot run in a
view. The policy does not change. The view uses the browser's WebCodecs
for Opus, and a browser without it cannot use voice chat. It never loads
a codec from another site.

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
   proposed message: the model's words, marked as such. The person edits
   it, sends it or discards it. Nothing is sent without the press.

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
| `mic` | The view's frame may ask for the microphone: `Permissions-Policy: microphone=(self)` on the plugin's origin and `allow="microphone; autoplay"` on its frame, so the view plays the model's audio without a second press. The browser asks the person for the microphone. A view whose audio stays suspended shows one "Start" press | What the person says goes to the relay's endpoint |
| `relay:<id>` | The view may open one WebSocket at `/_relay/<id>`, which the gate joins to an endpoint the instance names. With `setup` in the manifest, it may first send one `POST /_relay/<id>/setup` | The endpoint hears the audio. It is a host the factory's operator chose. With `setup`, the view's page also posts a JSON body to one path on that host, which the manifest names |
| `voice:<minutes>` | The longest voice chat, at most 30 | |
| `propose` | At the end, the view hands the host text to put in the session's input, at most 8 KiB, with the `propose` message of "Messages between the host and the view" | The person sees the whole text and sends it or not |

None is granted by default. A plugin with `lock` needs `chat:button` and a
view with `open: full`.

## The relay

v2's `net:` stays as it is: HTTPS fetches, public addresses only. A speech
host on a tailnet is private and the audio is a stream, so it gets a route
of its own.

The endpoint is a setting of the instance, never of the plugin or of tehas:

```
TEHAS_PLUGIN_RELAYS="voice:speech=wss://speech.example.org/api/chat"
```

Haru's socket is `/api/chat`. Haru's server is plain HTTP on `localhost`,
so the address is that of `tailscale serve`, or of a TLS terminator, in
front of it.

Each entry: `<plugin>:<id>=<url>`. At most 2 per plugin. The plugin's
manifest names the `id`; the grant line grants `relay:<id>`; the instance
gives the URL. A missing entry refuses the plugin at `tehas update`, exit 78
for a malformed one.

| Rule | Text |
| --- | --- |
| Address | `wss://` with a host name, an optional port and a path, no user or query. The upgrade may add the one parameter the manifest's `query` names (below). The name is resolved once and the connection goes to the address checked. Private and CGNAT (tailnet) addresses are allowed here because the operator named them. Refused still: loopback, link-local, multicast, "this network", the host's own addresses and the Docker bridge ranges |
| TLS | The system's roots. A tailnet name's certificate (`tailscale cert`) passes |
| What crosses | Binary and text frames, both ways. Nothing of the browser: no cookie, no `Origin`, no `X-Tehas-Login`. tehas sets the request's headers |
| Who | A signed-in person, with v1's session, `Origin` and `Sec-Fetch` rules on the upgrade |
| How many | One relay per login, and `concurrent` (manifest, default 1) per plugin. Over it: the button says the voice chat is in use |
| Rate | At most 64 KiB a second each way. 24 kHz audio needs far less |
| Time | Closed at `voice:<minutes>`, at 60 seconds with no frame, or when the lock is released. Haru closes its own socket after 15 seconds with no audio from the client, so the view sends audio all the time it is open, silence included. Haru queues a talk over its `--max-sessions` with no handshake; the view says "Waiting for the speech host" until the handshake, and the 60 seconds end a wait that is too long |
| Kept | Nothing. Inspection rows keep the start, the end, the login and the byte counts, never audio or text |

### Setup and the query parameter

Haru starts a talk in two steps: `POST /api/session` with the card and the
voice, which answers an id, then the socket at `/api/chat?session_id=<id>`.
So a relay may name a setup path and one query parameter (1.6.0, C1).

| Rule | Text |
| --- | --- |
| `setup` | A path on the endpoint's origin: it starts with `/`, parts of `[A-Za-z0-9._-]`, none `.` or `..`, at most 64 bytes. `tehas plugin review` prints it |
| The request | `POST /_relay/<id>/setup` from the view, under the checks of the upgrade and the web part's rules for a `POST`: the session, `Origin`, `Sec-Fetch-Site`. The body is one JSON object, at most 8 KiB |
| What the gate does | It sends the body to `https://<endpoint's host and port><setup>` as `application/json`, with the relay's address rules and TLS, and nothing of the browser. It passes back the answer's JSON object, at most 1 KiB, with the endpoint's status. Any other answer is 502 |
| How many | One setup for each socket. A setup counts as that login's one relay: a second one before the first socket closed is refused, and a setup with no upgrade within 30 seconds is forgotten |
| `query` | A parameter's name, 1 to 32 characters of `a-z`, `0-9` and `_`. The upgrade may carry that one parameter, with a value of 1 to 64 characters of `A-Za-z0-9_-`. The gate appends it to the endpoint's URL. Any other parameter, or a second one, is 400 |
| What tehas reads | Neither body. It checks that each is one JSON object within its size. Inspection rows keep the sizes |

A security change: the view's page, in a signed-in person's browser, sends
a body it chose to a path the plugin named on a host the operator named.

`console/core/src/plugin/relay.rs`, used by `tehas-plugin-gate`.

## Manifest

```json
{
  "plugin": 1,
  "kind": "batch",
  "name": "voice",
  "label": "Voice",
  "license": "Apache-2.0",
  "build": "Dockerfile",
  "timeout": 60,
  "views": [{"id": "talk", "label": "Voice", "path": "/", "open": "full"}],
  "chats": ["obeya", "wizard", "assistant"],
  "button": {"label": "Voice", "view": "talk"},
  "relays": [{"id": "speech", "concurrent": 1, "setup": "/api/session", "query": "session_id"}],
  "asks": ["view:talk", "chat:button", "lock", "mic", "relay:speech", "voice:15", "propose"]
}
```

Until 1.6.0 this example did not pass the reader (C10): `build` was an
object, the ask was `view`, the view had no `label`, and v1's `kind`,
`label`, `license` and `timeout` were missing. `build` is a path, a view of
`views` is asked for as `view:<id>` and has a `label`, and a plugin with a
build needs `kind` and `timeout`.

```
TEHAS_PLUGIN_GRANTS_VOICE="<commit> view:talk chat:button lock mic relay:speech voice:15 propose"
```

| Field | Rule |
| --- | --- |
| `button` | Optional. `label` 1 to 16 characters, `view` a view with `open: full`. Needs `chat:button` |
| `relays` | Optional, at most 2. Each `id` (`a-z`, `0-9`, 1 to 12) and `concurrent` (1 to 4), and optionally `setup` and `query` ("Setup and the query parameter"). Needs `relay:<id>` |
| `chats` | v2's field. With `chat:button` it names the chats that draw the button |

The plugin has a build for its view's files only. A run copies them to
`/out` and does nothing else. No `product`, no `model`, no `service`: the
audio never enters a plugin container.

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

The hosts are pages of industry, on another origin than the dashboard. The
view's `frame-ancestors` names them through `TEHAS_PLUGIN_FRAMERS` (1.6.0,
`2026-11-22-plugins-1.6.md`). The host sets `allow="microphone; autoplay"`
on the frame.

### Messages between the host and the view

The host draws End, and the view holds the transcript. 1.1.0 named no
message between them, so the view could not learn of End or hand over the
text before its frame went (C4). They talk with `window.postMessage`:

| From | Message | Meaning |
| --- | --- | --- |
| host | `{"tehas": 1, "kind": "hello"}` | Sent when the frame has loaded. The view keeps the event's `origin` as the host's, for a browser that gives no `location.ancestorOrigins` |
| host | `{"tehas": 1, "kind": "end"}` | End was pressed, or the limit came. The view stops, closes the socket and answers |
| view | `{"tehas": 1, "kind": "propose", "text": "..."}` | The transcript, at most 8 KiB. Sent once, and only when there is text. Needs `propose` |
| view | `{"tehas": 1, "kind": "ended", "why": "..."}` | Always the view's last message, after the proposal, whether End came from the host or the talk stopped by itself. `why` is one line, at most 140 characters, shown as text. The host releases the lock and closes the view |

- The host takes a message only from the frame it opened, and only from
  the plugin's origin. It reads `text` and `why` as untrusted text: the
  proposal is marked as from the voice plugin, and neither is ever markup.
- The view takes a message only from its parent window. It posts `propose`
  only to the host's origin, from `location.ancestorOrigins` or from a
  host message's `origin`, never to `*`. With neither known it sends
  `ended` alone, which carries no transcript.
- A host that gets no `ended` within 2 seconds of its `end` closes the
  frame anyway. The transcript is then lost, and the host says so.
- A message of another shape or kind is dropped by both sides.

## The speech host

Outside tehas. The plugin's repository says how to run it:

- Haru's server (`github.com/higurashi-ai/Haru`) with `--hf-repo
  higurashi-ai/Haru-4B`, a GPU with about 12 GB, listening on `localhost`
  only, behind `tailscale serve` or a TLS terminator, on the factory's
  tailnet.
- **Haru's server has no authentication** on any route (C8). Whoever
  reaches its port can talk to the model, use its web demo and upload
  voices. The tailnet's ACL lets only the factory's host reach it. Never
  `--share`, which opens a public tunnel.
- The model is the plugin's default. A different model is a different
  endpoint in `TEHAS_PLUGIN_RELAYS` and a view that speaks its protocol.
- No voice ships with Haru (C5). The speech host installs a clip as
  `haru/assets/voices/en.flac`, and the view sends `reference_audio_id`
  `default`. Without the clip Haru answers 404, and the view says the
  speech host has no voice.
- Haru needs a card, its `system_prompt`, of 1 to 4000 characters (C6).
  The view carries a fixed one in its own files. It holds no text of the
  session: no ticket, no login, no message. Open point 2 stays open.

The view speaks Haru's frame protocol over the relay; tehas does not read
the frames.

## The transcript

Haru gives text only for its own side, in pieces. Nothing transcribes the
person (C3). So the transcript is the model's words only, and it says so
at its top: "Voice chat (the model's words only; what you said is not
written down)". At End the view passes it to the host with `propose`. An
empty transcript proposes nothing. This falls short of the purpose above
until a recogniser is chosen (open point 5).
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

## Corrections in 1.6.0

From the voice plugin's read of Haru's server and its live run. The
numbers are that plugin's.

| # | 1.1.0 said | Now | Where |
| --- | --- | --- | --- |
| C1 | The relay is one WebSocket, and its URL has no query | A relay may name a `setup` path and one `query` parameter. A security change | "Setup and the query parameter" |
| C2 | The example endpoint `wss://speech.example.org:8998/ws` | `wss://speech.example.org/api/chat`, behind `tailscale serve` | "The relay" |
| C3 | The view joins the person's and the model's text | Only the model's side is text. The transcript says so | "The transcript" |
| C4 | No message between the host and the view | `hello` and `end` from the host, `propose` and `ended` from the view, with the 2-second rule | "Messages between the host and the view" |
| C5 | The voice clip is the speech host's setting | The speech host installs `en.flac`; the view sends `default` | "The speech host" |
| C6 | Nothing on the card | A fixed card in the view's files, with no session text | "The speech host" |
| C7 | Nothing on keeping the socket alive | The view sends audio all the time, silence included. The relay's 60 seconds stay | "The relay", Time |
| C8 | Nothing on Haru's access | No authentication: the tailnet's ACL, never `--share` | "The speech host" |
| C9 | `allow="microphone"` on the frame | `allow="microphone; autoplay"` | "New capabilities", `mic` |
| C10 | A manifest the reader refuses | A manifest that follows the reader's rules | "Manifest" |
| C11 | Nothing on the codec | WebCodecs in the view. The policy does not change | "Why not in the browser" |
| C12 | Over `concurrent` the button says in use | Haru also queues talks itself. The view says it is waiting, and the 60 seconds bound the wait | "The relay", Time |

## Open points

1. Settled in 1.6.0. Haru's server protocol was read: binary frames of one
   kind byte and a payload, which the relay passes as they are, after the
   setup of "Setup and the query parameter".
2. Whether the voice model gets the session's last messages as its card (its
   system prompt). That sends session text to the speech host. Not in 1.1.0.
3. Whether the lock should also be held server side, refusing that login's
   writes while a voice chat is on.
4. Haru is a research base model. Whether it is good enough for a session, or
   whether the default should move to an instruction-tuned voice model when
   one exists.
5. The person's side of the transcript. Haru does not transcribe the
   person. A speech recogniser on the speech host as a second relay fits
   the design, since a plugin may have two. It needs the owner's choice of
   model and host. The browser's own speech recognition is ruled out: in
   Chrome it sends the audio to Google.
6. The session of a view framed by a chat host on another site: see the
   open points of `2026-11-22-plugins-1.6.md`.

## Work

After v3's work has landed, in order. Each is a pull request; a security
change says so.

1. **Settings and manifest.** `TEHAS_PLUGIN_RELAYS`, `button`, `relays`, the
   six capabilities, refused by name until each is enforced. Security change.
2. **The device policy.** `Permissions-Policy` on every plugin address, `mic`.
   Security change.
3. **The relay.** `relay.rs`, `/_relay/<id>`, its setup and query
   parameter, its caps and inspection rows. Security change.
4. **The button and the lock** in each host: Obeya, the wizard, the
   assistant, through the factory API, with the frame's `allow` and the
   messages `hello`, `end` and `ended`.
5. **`propose`** into the session's input, from the view's message.
6. **The plugin.** tehas-app/tehas-plugin-voice: the view, the frame
   protocol, the speech host's how-to.
7. **The manual.** `host/docs/plugins.md`, `features.md`, the changelog
   fragment.
