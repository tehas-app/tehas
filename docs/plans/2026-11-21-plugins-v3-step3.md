# Plan: plugins v3, step 3, against the gate

**Status**: Draft 2026-11-21, tehas-74. Designed against `plugins/gate`
(#2704) before it merges, as the lead asked; no pull request yet. The spec is
`docs/specs/2026-11-21-plugins-v3.md` (plugin specification 1.0.0).

Step 3 is four surfaces: the frame, uploads, `open`, and a plugin's own chat.
Only the frame depends on the gate alone. The other three stand on v2 parts
that are not built: `views` (one view is still the reader's limit, "views:
more than one view is reserved for plugins v2") and `kind: "service"` (the
v2 menu part). This plan builds in that order and says who owns what.

## 1. The frame, `/_frame/<path>`

Needs only the gate. Files of mine: new `console/plugin/src/gate/frame.rs`;
one line each in `gate/path.rs` (`Route::Frame`) and `gate.rs` (its arm);
`Facts::keep` in `gate/catalog.rs`. To agree with tehas-99 while 5b is open.

- **Route.** `["_frame", rest @ ..]` with at least one segment becomes
  `Route::Frame(segments)`, from `Route::Reserved`. The same session check
  as a view's file; the same `Sec-Fetch` and `Origin` rules, GET and HEAD only.
- **What it serves.** A file of `~/tehas/plugins/<name>/keep/`, the kept
  files `keep:` grants, through the same reader as the view's files: no
  link, regular files only, at most `PAGE_MAX`. A plugin without `keep`
  granted gets 404, as an unknown path does.
- **Its policy.** A fixed one, never worked out from the bytes, since the
  file is the plugin's data and not its code:
  `default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; frame-ancestors <own origin>; sandbox allow-same-origin`.
  `sandbox` without `allow-scripts` means no script runs, whatever the file
  holds, so the inline handlers that refuse a view's document need not refuse
  a frame. `frame-ancestors` lets only the plugin's own view frame it. The
  gate's base headers as for every answer (`Cache-Control: private,
  no-store`, `Cross-Origin-Resource-Policy: same-origin`).
- **Types.** `.html` as `text/html`, and the image and font types the kit
  already names. Anything else 404, so a kept file is never served as script.
- **Tests.** In `gate/tests.rs` beside the view's: a frame served with its
  policy and headers, a frame without a session redirected to sign in, no
  `keep` grant 404, a link or a directory 404, a `.js` file 404, a cross-site
  fetch 403, and a document with `<script>` and `onclick=` served with the
  sandbox policy, so nothing in it runs.

## 2. Uploads, `upload:<MiB>`

Needs the gate and somewhere the plugin reads them. A batch plugin reads
them at its next run; a service through its socket (part 4).

- **Route.** `POST /_uploads` with the bytes as the body and `Content-Type`
  one of the manifest's `upload.types`. Same-origin POST only, as an action is,
  and a session.
- **Limits.** At most the grant's MiB, refused at the `Content-Length`
  before a byte is read; at most 32 uploads kept per plugin, the oldest
  dropped. The bytes are checked to be what the type says by their first
  bytes, as the model socket checks images (#2696), and `text/html` passes
  the same reader the capture uses.
- **Where.** `~/tehas/plugins/<name>/uploads/<sha256>.<ext>`, mode 0600,
  never served by the gate. The answer is `{"sha256", "bytes", "type"}`.
- **Inspection.** A crossing row with the size, type and hash, never the
  bytes (v3, "Changes to v2", inspection).
- **Manifest.** `upload` (`types`) and the ask `upload` open in the reader
  in the same pull request, as the v3 rule says: a field opens with its
  enforcement.

## 3. `open: dock|page|full`

Needs `views` (v2 menu). When the v2 menu lands, `open` takes `full` beside
`dock` and `page`. `full` is drawn by tehas's dashboard, not the gate: an
overlay over the dashboard, the plugin's view in a frame of the plugin's own
origin, and a close control tehas draws. Built so the voice plugin's lock
(spec 1.1.0) can extend it: the overlay is one element that owns `inert` of
everything else and one close control a lock can replace with its End.
Owner of the dashboard side: to agree with whoever builds the v2 menu.

## 4. A plugin's own chat, `chat` and `session:<minutes>`

Needs `kind: "service"` (v2 menu part). The view asks the service through
the gate; the gate proxies to the service's unix socket in a bind-mounted
directory (pi-rukas-42's 1.0.1 shape). `chat` gives the service the model
socket (#2676, with `model:images` from #2696); `session:` mounts the
default branch's head read only for at most the granted minutes. The
dependency's skills (`needs:`, #2673 and #2677) go into the instructions,
labelled with their source. The budget is counted per session and against
`TEHAS_PLUGIN_MODEL_DAY`.

## Order

1. The frame, as soon as #2704 merges.
2. Uploads, after the frame.
3. `open: full` and the chat, after the v2 menu part's `views` and
   `kind: "service"`, which this plan does not build; to agree with their
   owner (pi-rukas-42 builds v2's parts).
