# Plugin specification 2.0.0: the operator and ordinary plugins (floating)

**Status**: floating, not 2.1. Moved out of the normative file,
`2026-11-25-plugins-2.0.0-operator.md`, at revision 8 (decision 75).
Section numbers are kept, so this file's parts sit between the normative
file's. Every part here lands with the first ordinary plugin, in the next
open minor. Decisions are in `2026-11-25-plugins-2.0.0-history.md`, and a
view's network in `2026-11-25-plugins-view-network.md`.

**Normative**: tehas-standard v3 at main 66b7577, as the normative file
says.

## Why

Under 1.x a plugin is a Docker image, built on the host from the plugin's own
Dockerfile, and each run is a container with all of Node inside it. Plugin
code runs at build time, as root, for plugins that are off as well. Each
plugin carries its own image: about 200 KB of plugin code on a 1.2 GB base.
The container is the only fence around a whole platform.

Under 2.0.0 a plugin is files. A run is a JavaScript engine instance inside
WebAssembly, run by the operator. What a plugin can reach is the set of
functions tehas hands it, one for each grant, and nothing else exists inside
the engine.

## 1. The operator and the engine: drivers

- **Language.** ES2023 modules. The entry is `dist/driver/main.js`, and it
  exports `run(ctx)`. A driver imports the host API as `tehas`.
  - **Drivers are synchronous in 2.1.** The standard's §6 leaves it to the
    host whether `run` may return a promise; a host that can't settle one
    refuses a run that returns one, with a named error. The spike's
    `--script-only` export
    is synchronous: microtasks run, but a promise `run` returns hasn't
    settled when it returns. So `run` returns its result, not a promise,
    and a run that returns a promise is refused with a named error. Async
    drivers come only once the engine's async exports are proven by a
    check of part 9 (WASI 0.3, or componentize-js's async support).
  - **`ctx` is an object** in the API. The WIT may carry it serialized; the
    engine's glue hands `run` the parsed object, under the host's
    `plugin-context` schema.
  - **`ctx.trigger`** names why the run started: `clock` (for `every:` and
    `clock:`), `press` (a view's press, 6.3) or `once` (a run by hand). A
    later minor may add values, and a driver treats one it doesn't know as
    unknown.

  There are no timers beyond `await`. A plugin that needs to wait asks for
  `every:` or `clock:` (decision 33), as in `clock:mon-fri@06:00` or
  `clock:daily@06:00,18:00`. The word matches the trigger these runs
  record (`clock`). `at` stays only the manifest key of a hook point or a
  gate point.

### 1.2 Failure containment

*Floating, not 2.1.* Part 10 cites what binds the adapter (no answer means hold, output rules).

A bad plugin must never take down core, and should cost other plugins as
little as possible (decision 30).

- **Core is outside the blast radius.** The gate, the dashboard and the
  factory's machinery never run plugin code. If the operator dies, plugins
  stop and core carries on: views show their last good output, and the
  dashboard marks plugins as unavailable.
- **A run's failure is the run's.** A trap, a thrown error or a passed limit
  ends that instance only. The last good output stays, and the run is
  recorded with its reason.
- **Gates and phases with no answer: hold** (decision 32, "make the safe
  choice"). No answer is never `allow`.
  - If a gate gets no answer, because the operator is down or the run
    failed or timed out, tehas holds the act, whatever the grant's policy.
  - The grant's `pass` or `hold` policy applies only to an answer that came
    back. A held phase past `max_age` follows its policy, because it was
    answered.
  - A phase already entered stays held, and tehas asks again on the next
    tick once the operator is back.
  - A person's lift (`tehas phase lift`) always works, with the operator
    down or not.
  - The `held[]` item's `text` says "plugin <name> unavailable", and the
    operator's crash loop alert (below) names the held issues.
  - The cost: while the operator is down, a factory with a
    `before:cycle.started` gate starts nothing until the operator is back
    or a person lifts the hold. That's taken over letting work pass a gate
    nobody checked.
- **The operator's own code can't be crashed from a plugin.**
  - Every host function runs under `catch_unwind`. A panic becomes an error
    for that run, is logged, and the run ends. The operator keeps going.
  - Every host function has a fuzz target (part 9), fed with what a plugin
    can send.
  - Host functions are bounded: a cap on each argument and result size, a
    deadline on each call (H2) and budgets per run (H3).
  - systemd restarts the operator if it exits (`Restart=on-failure`, with a
    start limit). A crash loop alerts the factory's owners and leaves core
    running.
- **Runs can't starve each other.**
  - At most one run of each plugin at a time. A `clock` or `once` trigger
    that comes during a run is merged into one queued run.
  - **A press is never merged** (R3 M8). Each press is answered on its own
    (6.3). A press that comes during a run queues apart, at most 4 per
    plugin in arrival order, and runs after the current run; past that the
    host element gets `busy`. A law: two presses during a run give two
    answers, each to its own load, and a fifth queued press gets `busy`.
  - A run starts only when its memory cap fits in what's left of the
    operator's memory budget, so concurrency follows the caps, not the
    typical run (9 MiB for hoststats, 63 MiB for guard's scan, part 9).
    Runs that don't fit wait in a queue, with fair turns between plugins.
  - The operator's systemd unit has its own `MemoryMax` and `CPUQuota`, so
    the operator as a whole can't starve core either.
  - A plugin whose runs keep failing or timing out is backed off, and after
    a set number of failures in a row it's paused, with an alert. An admin
    resumes it.
- **Disk.** Each plugin has a total quota for `/state`, besides the size cap
  on each run's `/out`. A write past it fails in the run. The whole
  operator's data sits on its own quota, so plugins can't fill the disk core
  needs.
- **The model.** `model:<tokens>` is a budget for each run. A budget for each
  plugin and factory per day caps the total. A spent budget fails the call,
  not the run.
- **The engine and wasmtime.** An escape from the sandbox would land in the
  operator's sandbox, not in core: no network, a read-only root, no
  capabilities. wasmtime and the engine are kept current. A wasmtime
  security release is a patch release of tehas.
- **Views.** A view's frame is a sandbox with an opaque origin (H8, part
  6). It's still served from a name under the dashboard's site, so it may
  share the dashboard's browser process. `<tehas-plugin-view>` watches its
  frame, and one that stops answering is unloaded with a "this view stopped
  responding" state. Heavy view work runs in a worker (part 6.1).
- **Plugin output is untrusted data (H4).** Everything a plugin returns
  (`log()` lines, findings, verdict text, a gate's `why`, the review
  summary) is escaped and bounded by the host before anyone sees it:
  - terminal control sequences and other C0 and C1 controls are stripped
    before a terminal shows it (wasmtime's security guidance names this);
  - Markdown and HTML are escaped before a pull request comment or the
    dashboard shows it. Links show only as plain text or point to the
    plugin's own view;
  - each field has a length cap, and so does the total for each run (part
    1.4).
- **No secrets out (H7).** The host scans what a run returns (findings,
  verdict text, `/out` files it will publish) for secret patterns (tokens,
  private key blocks, the host's own credentials) and redacts them before
  publishing, recording that it did. H1 makes this rare; H7 catches what H1
  misses.
- **Tested.** The operator's tests include a plugin that panics a host
  function, one that loops, one that allocates without end, one that fills
  `/state`, and one that triggers itself in a storm. Core and the other
  plugins must keep running through each.

### 1.3 Scaling

*Floating, not 2.1.*

One operator is enough for throughput: a typical run costs about 3 ms and
9 MiB (part 9), and runs come from branch moves, schedules and chats, a few times
a minute for each factory. More operators are for isolation first
(decision 31).

- **One operator for each factory instance.** Several factories on one host
  each get their own operator, with its own system user
  (`tehas-op-<id>`, 10.5), systemd unit, limits, queue, quotas
  and data. A bad plugin on one factory can't touch another's runs.
- **A separate operator tier for third-party plugins (optional).** A factory
  may run third-party plugins in a second operator, apart from our own
  signed plugins, as its own user (`tehas-tp-<id>`), so
  an escape from that sandbox reaches nothing of ours.
  Both operators run the same engine under the same rules.
- **Rules that keep more operators possible later:**
  - A run is stateless: a fresh instance, so any operator of the factory
    can take any run.
  - A plugin reaches files, state, git and the model only through the host
    functions, so the storage behind them can move without changing a
    plugin.
  - Each plugin of a factory belongs to one operator at a time. That keeps
    one run per plugin (part 1.2) and the `/state` quota without
    distributed locks.
  - Core hands runs to its operator over a defined channel, a local socket
    with a queue behind it, and never calls into it in process. Adding an
    operator is routing, not a redesign.
  - The `.cwasm` cache stays on each host, keyed by CPU.
- **Not now: operators on other hosts.** The `product` and `history` grants
  read the narrow view that core prepares on the factory's host (part 5),
  and the model goes through a local socket. A remote operator would need
  a copy of that view, or those calls forwarded back to the factory's
  host. That's built only when it's needed.

## 2. Layout, and who builds

```
tehas-plugin.json   the manifest; its paths point into dist/
src/                source: .ts, .tsx, .jsx, .js, tests, and each module's source
dist/driver/        deployable driver JS; main.js exports run(ctx)
dist/view/          deployable view JS, HTML and CSS
dist/vendor/<lib>/  bundled third-party libraries, verified against upstream (part 3)
dist/wasm/          built WebAssembly modules (part 1.1)
dist/test/          built tests, run by tehas plugin test
skills/             skill files, as in 1.x
tehas.toml          the standard the repository follows, at its root
```

- **A root `tehas.toml`** names the standard the repository follows
  (`[standard]`, with its `url` and `version`) and the toolchains it uses
  (`[platform]`). It's binding for our own plugins, adapters included, and
  recommended for third-party ones.

- **Builds are the plugin builder's job.** The builder picks the tools (tsc,
  esbuild, Babel with Solid's preset, cargo, ...) and commits `dist/` at the
  pinned commit. tehas ships no build command and no transform.
- **tehas deploys `dist/` and nothing else.** It never transforms code, and
  it doesn't compare `dist/` with `src/`. `src/` is kept so people can read
  the source. What tehas checks, vets and runs is `dist/`.
- **TypeScript** is the builder's affair. tehas ships `tehas.d.ts` (part 5),
  and the kit ships `kit.d.ts` and `solid.d.ts` (part 6), so a TypeScript
  builder gets accurate types. Type checking runs in the builder's CI.
- **Tests** are built into `dist/test/`. `tehas plugin test` runs them in the
  engine with a small `test()`/`assert` API and fake host functions. A
  builder can also run their plugin under Node, for a debugger: through
  **jco**, which runs components in Node, or against the same fakes. The
  engine run is the one that counts.

### 2.1 A note for coding standard v3

This note is for the coding standard's working draft in tehas-docs
(`wiki/decisions/coding-standard-v3.md`), which is still written for
containers. The normative `v3/coding.md` already holds the plugin rules R11
to R14. The plugins lead drafted these changes for the draft, not for this
spec's rules:
- **R11, the kit**, plus verified `dist/vendor/` libraries (part 3).
- **R12, typed scripts.** TypeScript or JSDoc in `src/`, checked against
  the generated `tehas.d.ts` (part 5). `dist/` is readable, not minified,
  with no `eval` (part 3).
- **Solid JSX** is allowed in `src/` for views (part 6).
- **Drivers import only `tehas`**, besides their own `dist/` files
  (part 3).
- **xterm** stops being the draft's named exception (console's terminal)
  and becomes a verified vendor library (part 3).

## 4. Our own plugins: fetching plugin repositories

- **Fetching plugin repositories.**
  - A factory fetches over https with no credentials.
  - A public repository fetches as is. Verified on 2026-11-24: hoststats was
    made public as the verification plugin (decision 20), and a fetch with
    no credentials returns its `HEAD`, `4ac3c1c`.
  - A private repository fails the same fetch, so no factory can fetch the
    other plugins today.
  - For private repositories, a lent read token, as the plugins lead proposed on
    2026-11-24 and the owner accepted (decision 29):
    - a lent read token in `TEHAS_PLUGIN_FETCH_TOKEN_FILE`, outside the
      instance directory, read only at fetch time;
    - sent as a header through `GIT_CONFIG_*`, never on a command line, in a
      config, an image or a log;
    - scoped to a URL prefix, with `http.followRedirects=false`.
  - **A private pin with no token fails that plugin only** (decision 35).
    `tehas update` never exits non-zero because one plugin couldn't be
    fetched, and its frozen exit table stays as it is.
    - `tehas update`: the plugin keeps its last approved commit, or stays
      not installed. The update reports `fetch: no-token`, or `fetch:
      failed` for any other error, and exits as it would with no plugins.
    - `tehas plugin --json`: a `fetch` field on the plugin's entry, with
      the same word.
    - `tehas plugin review <name>`, a command about that plugin alone,
      exits 78 when the remote asks for credentials and no
      `TEHAS_PLUGIN_FETCH_TOKEN_FILE` prefix covers the URL. It exits 1 for
      any other fetch failure.
    - A malformed token setting is still malformed settings: 78 for every
      command (hard rule 2). That's a file named but unreadable, or
      readable by group or world, or a bad prefix.

    This matches 1.10's "not granted is not an update failure".

    The host check below still has to pass before any factory relies on
    it.
  - The host check is `tehas plugin review hoststats --json` on a
    tehas-factory 2.1 build. A tehas-factory 2.0 build refuses plugin
    settings with 78. It checks the fetch path on a real host. It's separate
    from `vet.yml`'s fixture run, which is what moves an entry from
    `building` to `vetted`. tehas-plugins#6 lists hoststats as `building`.

## 5. The host API: the `tehas:plugin` WIT world

*Floating, not 2.1.* The versioning, no-`u64`, narrow-view and `git.*`
rules stay in the normative file.

- **A grant opens a group of imports.** The engine is built once for the
  whole world, so every import is linked. The operator refuses a call its
  grants don't cover: it throws "not granted" before any work is done
  (spike). The check is in Rust, in each host function.
| Function | Grant | Replaces in 1.x | Used by |
| --- | --- | --- | --- |
| `fs.read`, `fs.list`, `fs.stat` | any input grant (`product`, `history`, `files`, `cycles`, `issues`, `pages`, `live:`, `feed:`, `feed:advisories`, `industry:logs`) | `node:fs` reads of mounts | every plugin with a driver |
| `fs.write`, `fs.remove` | `/out` always; `/state` with `keep` | `node:fs` writes | every plugin with a driver |
| `fs.read`, `fs.list`, `fs.stat` on `/state` | `keep`: the plugin's own `/state` only | `node:fs` reads | plugins that keep state |
| `git.head`, `git.diff`, `git.show`, `git.tree`, `git.blob`, `git.isAncestor`, `git.defaultBranch`, `git.status`, `git.log`, `git.files` | `product` or `history` | running `git` (`diff`, `show`, `ls-tree`, `cat-file`, `rev-parse`, `merge-base`, `symbolic-ref`, `status`, `log`, `ls-files`, `check-ignore`) | review, wiki, guard, goldfish, floorplan |
| `model.chat(req)` | `model:<tokens>`; images with `model:images` | `http.request` to `/run/tehas/model.sock` | floorplan, given, goldfish, guard, review, rsi, wiki |
| none: the engine's own `crypto.subtle.digest` and `crypto.getRandomValues` (web standard, random from `wasi:random`) | always | `createHash('sha256')`, `randomBytes` | 8 plugins |
| `run.module(path, argv, env)` | always | `execFile(process.execPath, ...)` | floorplan |
| `wasm.load(name)` | a module listed in the manifest | none | any |
| `log(...)` | always | `console` | all |

- **Paths** keep the 1.x mount names (`/work`, `/out`, `/state`, `/product`,
  ...), held by the operator. There are no links and no `..`, and a path
  outside a granted root is refused.
- **`/out` and `/state` under the operator** (R3 M5). Today they're Docker
  volumes of each run's container, read back from the stopped container,
  and `/state` is writable only for a `Refresh` run
  (`console/core/src/sandbox.rs`, the `state` volume). Under the operator:
  - `/out` is a directory per run that the operator creates in its own
    data directory, under the run's output cap. When the run ends, the
    operator closes it, and core reads it back through the operator's
    socket as one bounded archive of the files, never by path. Core takes
    a gate's answer from the run's return value (part 5's shape), and a
    draft and a view's output from `/out`, each under its schema and caps
    (1.2, H4);
  - `/state` is a directory per plugin in the operator's data directory,
    under its quota (1.2). It's writable only in a run of a plugin with
    the `keep` grant. That replaces today's run-kind rule: a plugin that
    had a `Refresh` run in 1.x asks `keep`, and any other run of it reads
    `/state` only if it has `keep`;
  - neither is reachable from core's own files or from another plugin.

  A law: `fs.write` to `/state` without `keep` is refused, and with `keep`
  it lands only in that plugin's directory.
- **`run.module`** runs one `dist/` entry of the same plugin in a fresh
  instance, with the given `argv` and `env`, under the run's grants and
  limits. It returns `{stdout, code}`, and `process.exit` ends that
  instance. It's for command-line code a plugin bundles, such as floorplan's
  upstream scripts.
- **`ctx`**, the argument to `run(ctx)`, is an object (part 1) and carries
  what `/run/context.json` carried, with `trigger`.
- **`feed:advisories`, a new input.** tehas fetches advisory data for the
  product's locked packages, as it does for the other `feed:` inputs, and
  writes it for the plugin to read with `fs.read`. The plugin has no
  network. It comes with guard's pull request. From the release of the
  managing system that drops its patrol until guard's advisories check
  lands on a factory, no patrol runs. That's a known
  gap, not a failure.
- Every host function checks its grant and its paths in Rust, and each has a
  fuzz target.
- **A gate's answer is a shape, not a file** (N2). A gate listener's run
  returns an object of the shape `api/gate-answer.schema.json` gives
  (`{"api": 1, "answer": "allow" | "refuse", "why"}`). The operator checks
  it against the schema and hands it to tehas. There's no
  `/out/_tehas/answer.json` path (#3187). A return that doesn't fit the
  schema is no answer, so the gate holds (part 1.2).
- **The Kept libraries** (the table "Kept for the 2.1 plugins" in
  `docs/architecture.md`) split by where they run (decision 36: "key
  reusable services can stay in the core"). A port removes its library code
  from core in the same release (decision 45: "plan the move as we build
  plugins"). "Kept" means kept until that port:

  | Crate | Under 2.0.0 |
  | --- | --- |
  | `attach`, `items` (store, serving), `issue_feed`, `wiki-collect`, `mesh-feed`, `host-feed` | Stay in core and make a grant's input (`files:`, `history:`, `feed:mesh`, `feed:host`, the lab's issues). The plugin reads it with `fs.read`. No host function |
  | `wiki-render`, split (decision 45: "Split it") | Its Markdown renderer (`render.rs`) stays in core: 1.9's public pages render with it, and so does the wiki plugin, through a host function named by a plugin surface request. The wiki's own check and keep rules (`check.rs`, `publish.rs`) move to tehas-plugin-wiki, as `describe` does |
  | `describe` (given only), `review-render` (review only) | Move to the plugin. The Rust code lives in the plugin's own `src/` and is built to its `dist/wasm/` from there, for `wasm32-unknown-unknown`, and called through `wasm.load`. They must reach "rebuilt from source: matches". One owner (R1), no JS copy, no copy left in core |
  | `items`' page writer | The pure part goes to WebAssembly. The JS writes the result through `fs.write` under its grants |

  - A crate used by one plugin that gains a second user moves back to core
    through a plugin surface request.
  - The kept-for-2.1 marker of a moved crate comes off, and the crate
    leaves the workspace and `kept_for_plugins.rs`, in the pull request
    that ports its plugin. Until then the Kept table's "For" column says
    "until its plugin's port".

## 6. Views

*Floating, not 2.1.* Parts 6 to 6.3 stay as the frame, the port and the actions. Every view's frame is `connect-src 'none'`.

- **Isolation is the security boundary.** Views are served by the gate at
  `plugin-<name>.<domain>` behind the sign-in, with the per-document CSP
  built from hashes and the `Permissions-Policy`, as now. Revision 5 adds
  the sandboxed frame (H8, below).
- **The view's frame is a sandbox (H8).** `<tehas-plugin-view>` frames the
  view in an `<iframe sandbox="allow-scripts">`, without
  `allow-same-origin`:
  - the view gets an opaque origin: no cookies, no storage, no session to
    borrow;
  - no `allow-popups`, `allow-forms`, `allow-top-navigation`,
    `allow-downloads` or `allow-modals`;
  - a `Permissions-Policy` that denies every device and powerful feature
    (camera, microphone, geolocation, clipboard, payment, USB, serial, HID,
    display capture, fullscreen), except what H10 opens;
  - the view's CSP is `default-src 'none'`, scripts and styles from the
    gate by hash, and `connect-src 'none'`, always (the standard's §14). A
    view's network, when it comes, travels over the port (the view network
    spec);
  - its only channel is `postMessage` with the host element, over one
    `MessageChannel` port (6.3). The host sends the granted feeds. The view
    sends presses, which the host checks against the plugin's grants before
    acting. A view sends no result of its own: a press's result comes from
    the driver, as `answer` (6.3). Revision 5 drops revision-4 wording that
    said the view sends "presses and results".

  The cost: the kit's `feed.js` and `press.js` gain a `postMessage`
  transport. Scripts and workers fetched from the opaque origin (a view's
  `.wasm` comes over the port, 6.1)
  need CORS from the gate (`Access-Control-Allow-Origin: null` for exactly
  those files). Those requests carry no cookie, so the gate serves a view's
  files under a path minted for each signed-in load, never to a bare URL
  (owner question in the pull request). A spike comes first (part 9): a
  view on the kit, a feed, a press, and a jSquash codec in a worker, under
  H8 and H9, in Chromium, Firefox and Safari.
- **Devices by two keys (H10), reserved.** Decision 44: "maybe they can
  prompt the user if they want to". 2.1 builds none of it; the words are
  reserved so that no other surface takes them. A device stays denied
  unless both keys say yes:
  - **the admin's key:** the manifest asks `device:<feature>` (`camera`,
    `microphone`, `geolocation`, `clipboard-write`, `display-capture`), and
    an admin gives the grant by hand. Turning a plugin on never gives it
    (part 4);
  - **the person's key:** the host element then delegates exactly that
    feature to the frame (`allow="camera"`, and the same name in the
    frame's `Permissions-Policy`), and the browser shows its own prompt,
    which the person answers as for any site;
  - the host element shows a lasting sign while a device is in use, naming
    the plugin, and the person can stop it there;
  - a device's data stays in the browser, unless the plugin also has a
    grant to hand a result to the host (`attach:<kind>`, a plugin surface
    request);
  - whether a browser delegates a device to an opaque-origin frame is part
    of H8's spike. Where one doesn't, that device isn't offered there.
- **The web component wrapper.** In the dashboard, a core-owned
  `<tehas-plugin-view name="…">` custom element wraps the plugin's frame. It
  owns:
  - its size;
  - the loading, failed and off states;
  - theme tokens, through a narrow `postMessage` contract;
  - removing the view when the plugin is turned off.

  Plugin code never runs in the dashboard's origin. A shadow root isn't a
  security boundary, so mounting plugin components into core pages is
  refused.
- **Inside its origin**, a view is static HTML, or its entry defines exactly
  one custom element.
  Solid 2 from the gate's kit draws it, using the kit's island contract in
  the light DOM.
- **JSX.**
  - Builders may write Solid JSX (`.jsx`, `.tsx`) in `src/`. Solid's `html`
    templates work with no compile at all.
  - The builder compiles with Solid 2's compiler into `dist/view/`:
    `@solidjs/compiler` 2.0.x (Oxc) or `@solidjs/babel-plugin` 2.0.x, DOM
    mode, ESM. Both give the same imports (spike). There's no
    `babel-preset-solid` 2.x release; its newest 2.x is a release candidate.
  - The manifest names the `solid-js` version the output expects (today
    2.0.3), not the compiler's version, and tehas refuses one the kit doesn't
    carry.
  - Compiled output imports Solid's runtime helpers from `@solidjs/web` and
    the author's own names from `solid-js`. The gate's page maps both to the
    kit's Solid with an inline import map, allowed by its sha256 in the CSP
    (verified in Chromium 159; Firefox and Safari still to check).
  - Vetting (part 3) checks every `@solidjs/web` and `solid-js` import in
    `dist/view/` against the kit's export list, so a newer compiler's new
    helper is refused at vetting, not at view load.
  - 2.0 compiler facts for builder docs: `on:`/`attr:` are removed; a
    fragment nested in an element is refused; a custom element's value is an
    attribute unless written `prop:name`.
- **Kit change (core).** The kit's Solid exports only `html`, `render`, the
  signal and store functions and the control-flow components. Compiled JSX
  also needs these 14 (spike, both compilers):
  - `template`, `insert`, `createComponent`, `delegateEvents`;
  - `spread`, `effect`, `memo`, `mergeProps`, `ref`;
  - `setAttribute`, `className`, `setStyleProperty`;
  - `heldRead`, `readShallow`.

  `tools/vendor-solid` exports them from `@solidjs/web`. Cost: +589 bytes
  raw, +210 gzip (123,689 to 124,278); `pages.max` becomes
  `library solid 124278`. `Dynamic` and `Portal` stay out (+5.9 KB raw for
  little glue value). Before freezing the list, compile a wider corpus (SVG
  `class`, dynamic `style` strings, `Repeat`, `Reveal`). Views don't
  hydrate gate HTML, so `hydratable` output is refused. Exporting
  `mergeProps` needs a carve-out in the ratchet that bans that 1.x name in
  `host/fun`.
- **Types.** The kit's `solid.d.ts` alone can't type `.tsx` (49 errors: no
  `JSX` namespace, no `@solidjs/web/jsx-runtime`). Builders add
  `@solidjs/web` at the kit's version as a type-only devDependency, and
  `solid.d.ts` gains `export type Element` and typed `For`, `Show`,
  `Switch`, `Match`, `Loading` and `Errored`. With that, `tsc --strict`
  passes and catches real errors (spike). These are front-end decisions for
  tehas; they change the kit's MANIFEST and byte limit.
- **Size.** `dist/view/` has a byte cap, like core's `pages.max`. Verified
  libraries count toward their own cap.

### 6.1 Heavy work in the browser, in WebAssembly

Decision 26: heavy work offered by a view runs on the client where possible,
in WebAssembly built from C or Rust.

- **Where it runs.**
  - Data that starts in the browser runs in the view's module, on the
    client. That covers a picked or pasted image, a file the user is about
    to upload, a canvas or a recording. Examples: image compression, resizing
    and format conversion (JPEG, PNG, WebP, AVIF), hashing, diffing,
    parsing, PDF rendering and text extraction.
  - Data that is already on the host (repository files, run output) stays
    with the driver and its modules in the operator (part 1.1). It isn't
    shipped to the browser to be worked on.
  - "Where possible" fails in three cases. Then the work goes to the driver,
    or to core through a plugin surface request:
    - the work needs a grant (files, git, the model);
    - the result must be trusted by the host;
    - the browser can't do it within the view's limits.
- **Why.** The work costs the user's machine, not the factory host's. The
  host never decodes untrusted media, and the bytes go up once, already
  compressed. It also keeps the operator's caps small.
- **Layout.** A view's modules go in `dist/view/wasm/<name>.wasm`, with the
  JS that loads them in `dist/view/`. The manifest lists each one as a view
  module.
- **Tooling.** The builder picks it, and tehas consumes only the output
  (decision 8):
  - Rust: `wasm-bindgen` (with `wasm-pack`), or a plain `cdylib` with a few
    hand-written exports.
  - C and C++: Emscripten, or clang with wasi-sdk for code that does no I/O.
  - Ready-made codecs: the Squoosh codecs (mozjpeg, oxipng, libwebp, libavif,
    resize) as packaged by jSquash, and `image`/`photon` in Rust. A packaged
    codec is a verified library in `dist/vendor/`: its hash is checked
    against upstream, and it counts toward the vendor cap (part 3).
  - A vendored `.wasm` is still a view module. The manifest lists it, and it
    turns on `'wasm-unsafe-eval'`. Its upstream hash stands in for the name
    section and the rebuild, as a verified library's does for the
    readable-code rule. The import rule and the no-threads rule still
    apply.
- **CSP.** Only a view that lists view modules gets `'wasm-unsafe-eval'`
  added to its `script-src`. That keyword allows compiling WebAssembly and
  nothing else: no `eval`, no inline script. Under H8 the frame has an
  opaque origin, so `'self'` names nothing:
  - `connect-src` stays `'none'`;
  - the host element sends each vetted module's bytes over the view's
    port, in a `module` message (6.3), and the view compiles them with
    `WebAssembly.compile`. So a module and its JS can't send data
    anywhere.

  Revision 4 said `connect-src 'self'`, and revision 5 opened the module's
  path, which revision 6 closes: modules come over the port. The spike checks this.
- **WebAssembly in the browser is third-party code in a box (H9).**
  - **Pure compute only**, the server's rule. Its imports are memory and a
    short allow-list of glue helpers: for `wasm-bindgen`,
    `__wbindgen_throw` and the string and memory helpers; for Emscripten,
    `emscripten_resize_heap` and a null `fd_write`. Any import that binds a
    web API (`__wbg_*` from `web-sys` or `js-sys`, fetch, DOM, storage,
    WebSocket) is refused at vetting.
  - **Always in a dedicated module worker** (`new Worker(url, {type:
    "module"})`, from `dist/view/`), never on the view's main thread: no
    DOM, bytes in and out through `postMessage`. Import maps don't apply in
    workers, so worker code doesn't import the kit.
  - **The worker's box is decided by the browser spike** (R2 H1). An
    opaque-origin frame can't start a module worker from the gate's URL
    the way revision 6 said: in the browsers checked so far, only a
    `blob:` worker starts, and it inherits the frame's CSP, so it has no
    CSP of its own. Until the spike says otherwise, the rule is that a
    worker runs under the frame's own CSP, `connect-src 'none'` included,
    and gets a module's bytes by `postMessage`, so it fetches nothing and
    sends nothing.
  - **Measured, 2026-11-26, for the spike** (Chromium 159.0.8154.6, the one
    Playwright 1.65.1 pins): in a `sandbox="allow-scripts"` frame with no
    CSP at all, a classic `blob:` worker starts and answers, and a `blob:`
    module worker (`{type: "module"}`) fails with an error event. So the
    module worker this part asks for doesn't start in that frame today; a
    classic `blob:` worker does. Which one a view gets is the spike's
    question, and #3305's if the view code needs module workers. The
    template's simulator tests the classic one.
  - Inside H8's frame, so the worker inherits the opaque origin.
  - A vendored codec (jSquash, Squoosh) is verified by its upstream hash,
    and still meets the import rule and runs in the same box.
- **Rules**, the same audit as part 1.1:
  - name section required;
  - imports recorded, and every import must be satisfied by the plugin's
    own vetted JS (the `wasm-bindgen` or Emscripten glue), within H9's
    allow-list. The glue is plugin JS under the readable-code rules, or
    part of a verified library;
  - threads and shared memory refused, because they need
    `SharedArrayBuffer` and cross-origin isolation headers on the gate;
  - SIMD allowed;
  - our own plugins must reach "rebuilt from source: matches" (part 4).
- **Limits.**
  - 8 MiB per module, and `dist/view/wasm/` has its own cap (part 3).
  - A view module can't gain any grant. All it reaches is what the view's
    JS hands it.
  - A run that is too heavy or too slow is the user's tab's problem, not
    the host's. The view shows progress, and a cancel that ends the worker.
- **Spike.** Not covered by part 9's first spike. Add one check: compress a
  picked image with jSquash's mozjpeg and WebP codecs in a worker, under the
  view CSP plus `'wasm-unsafe-eval'`, in Chromium and Firefox, and record
  the module sizes and timings.

### 6.2 Requested by the example plugin

The example plugin (tehas-app/tehas-plugin-example#1), the template every
plugin starts from, is built against this spec. Its brief asks for things
revision 4 has no surface for. Its builder hadn't reported when revision 5
was written, so this lists what the brief shows.

- **The viewer's login. DECIDED** (decision 54): by the roadmap manager
  under the owner's grant, 2026-11-25, for the owner's review
  (tehas-factory#3344). It's a security change, and its rules below
  stand. The hello world answers "Hello, <login>".
  - A grant `viewer:login`, which the admin gives by hand. The turn-on
    click never gives it (part 4).
  - With it, the host element passes the signed-in person's login to the
    view over its `postMessage` channel (H8). A run started by that
    person's press gets it as `ctx.viewer.login`.
  - Only the login. Nothing else about the account: no id, name, email,
    picture or permission. No token, and the view still has no cookie
    (H8).
  - `ctx.viewer.login` comes from the session of the press's request, which
    the gate checked, never from anything the view's message says.
  - Each press carries a single-use nonce, bound to the session and to that
    load of the frame. The result goes only to that load, and a nonce used
    twice, or from another load, is refused.
  - A run that has it returns only to the view that pressed. Its result
    never becomes the shared last good output that other people see.
  - A run that has `ctx.viewer` runs without `keep`, `record`, `issue:*`
    and `model`, whatever else the plugin is granted, so nothing that
    holds the login is kept, filed, recorded or sent to a model.
  - A login names the person publicly on the code host: it's personal
    data, and the plugin sees it only because the admin granted that.
  - It's for display. It authorises nothing, and a plugin can't act as the
    person with it. The gate checks the person's permission as it does now.
- **A press that gets an answer, the view's channel, the kit's place, the
  manifest's fields, the trigger, `/state` reads and synchronous drivers**
  are settled in 6.3, 2.2 and parts 1 and 5, from the example plugin's
  pull request (tehas-app/tehas-plugin-example#2).
- **The manifest's name.** The brief said `manifest.json`. This spec's name
  is `tehas-plugin.json` (2.2), and it holds.

### 6.3 The view's channel, presses and the kit

A view in its sandboxed frame (H8) talks only by `postMessage` to its
`<tehas-plugin-view>` host element. In the standard's words (§14's frame
item): every message is typed and versioned and checked against the host's
schema in its `api/`, and one that fails is dropped; a press names one of
the manifest's `actions`, carries a single-use nonce and arguments under
that action's schema, and its answer goes only to the frame load that
pressed, never into the output other viewers see. A frame is known by its
port, never its origin: the host element transfers a fresh
`MessageChannel` port for each load, and a new load revokes the old port
and its nonces. A press that acts as the person needs a confirm drawn in
the host element's own page, outside the frame, naming the action's
manifest label. Messages have caps on their bytes and on their number per
load, and one press is in flight at a time, with the numbers set by the
host (below). Each message is an object with `type` and `v` (the channel's
version, `1`).

| `type` | From | Fields | What |
| --- | --- | --- | --- |
| `port` | host | `v`, and exactly one transferred `MessagePort` | the first message of each load; the channel's port (below) |
| `ready` | view | `v` | the view has loaded and listens |
| `output` | host | `v`, `run` (its id and time), `files` (path to text), `next` (a nonce) | the last run's output, pushed by the host on `ready` and after each run |
| `press` | view | `v`, `action` (an id of the manifest's `actions`), `nonce`, `args` (an object under the action's schema) | a button press, with the single-use nonce of 6.2 |
| `answer` | host | `v`, `nonce`, `result` or `error`, `next` (a nonce) | the result of that press, sent only to that frame load |
| `error` | host | `v`, `code`, `text` | a message dropped or a press refused, by a named code |
| `module` | host | `v`, `name`, `bytes` | a vetted view module's bytes (6.1) |

- **The handshake.** The host's first message to each load is
  `{type: "port", v: 1}`, with exactly one transferred `MessagePort`. The
  view accepts only the first such message from its parent, and drops
  anything that comes before it, and any later `port` message. Every other
  message travels on that port.
- **One port per load.** On each load of the frame, the host element makes
  a `MessageChannel` and transfers one of its ports to the frame, in the
  `port` message. All
  traffic uses that port, and the kit accepts messages only on it. A
  message's origin can't tell frames apart, because a sandboxed frame's
  origin is the opaque `"null"`, the same for every view. So the host
  tells frames apart by their port, and treats every `load` event as a new
  load: it revokes the old port and every nonce issued on it.
- **Nonces.** The host issues each nonce, single-use, in `output` and
  `answer` (`next`), bound to the session and to that port.
- **Limits**, this host's numbers, per direction and type (R1 L1). From
  the view: a message is at most 64 KiB, at most 20 a second and 2,000 a
  load, and one press is in flight at a time. From the host: `output` and
  `answer` at most 1 MiB; `module` at most 8 MiB, the module cap (6.1),
  sent once per module per load. A message past a limit is dropped, and
  the host sends `error` with its code (`too-large`, `too-many`, `busy`).
  A law at each limit and one byte over it.
- **Text stays text.** The host element never puts text from a view's
  message into its own DOM except as `textContent`.
- **A press that uses the person's credential needs a confirm first.** An
  action that uses a person's token directly runs only after a confirm
  that the host element draws in its own page, outside the frame, naming
  the action's `label` from the manifest. A law: a press of such an action
  with no confirm is refused.
- **A draft is filed under a person's name only after a confirm shown after
  the run.** When a pressed run leaves a draft (`issue:draft`), the host
  element draws a confirm in its own page, outside the frame, after the
  run. It shows the plugin's name and the exact title and body that will
  be filed. The person confirms or discards it, and nothing is filed
  without the confirm.
  - A draft from a run with no pressing person (`ctx.trigger` `clock` or
    `once`) is refused. It never falls back to the factory's token.
  - Laws: a draft from a press with no post-run confirm is not filed; a
    draft from a `clock` or `once` run is refused.
- **A press runs once and answers.** A press runs the driver's named action
  once, with `ctx.trigger` `press`, within the run's limits, and returns
  its result as `answer` to the pressing frame load only. A press's result
  is never the shared last good output, as 6.2's viewer rule says, and it's
  escaped and capped as part 1.2 says.
- **The kit on a plugin's origin.** The gate serves the kit's `island.js`
  and `kit.css` at the fixed paths `_kit/island.js` and `_kit/kit.css`
  under the per-load path of H8 (part 6).
- **The theme tokens.** Their one owner is the set every theme file sets,
  `host/fun/theme-*.css`, held equal by `test/theme_test.sh`. The host
  element sends those tokens' values for the current theme, light or dark,
  and the kit's `kit.css` uses them (`--bad`, `--text-faint`,
  `--text-quiet`, `--type-small` today).

### 6.4 A view's network: moved

*Floating, not 2.1.* A view's network, `net:` for views, GitHub reads
from a view and their laws moved to their own spec,
`2026-11-25-plugins-view-network.md`, a draft that waits for H8's
browser spike (part 9). Until it lands, every view's frame is
`connect-src 'none'`.

## 7. What goes away, and what stays

*Floating, not 2.1.*

Goes away:
- the plugin Dockerfile, `docker build` on the host, the per-plugin image,
  1.10's shared base image (1.10 part 3), base digests,
  `api/plugin-base.txt` and its release-workflow gate file, the `ONBUILD` rule, build
  timeouts, and building the plugins that are off;
- the per-run container, its Docker volumes and `TEHAS_PLUGIN_LIMITS` as
  Docker flags (the limits move to the operator);
- `node:test` in an image (part 2's `tehas plugin test` replaces it);
- `serve`, `upload`, `chat`, `session`, `net:app` and `net:capture` as
  plugin surfaces (part 8);
- any plugin language other than JS run directly (others come only as
  WebAssembly).

Stays:
- grants, and a person giving them on each factory. For our own plugins,
  the vetted grants come with turning the plugin on (decision 28);
- `tehas plugin review`;
- the gate and its sign-in;
- pins by commit;
- the vetted list;
- `TEHAS_PLUGINS`, `TEHAS_PLUGINS_OFF` and `TEHAS_PLUGINS_MAX` (default 42,
  from 1.10);
- industry's plugin page (1.10);
- `every:` and `clock:` scheduling (`clock:` replaces 1.8's `at:`,
  decision 33);
- hard rule 5: approval of the factory's work is a person's;
- **endpoint plugins, as manifests only.** A hosted endpoint plugin names
  its providers (id, API shape, `baseUrl`, models) and has no code, like a
  skills plugin. It needs no build, image or socket.

**Self-hosted models are endpoints at an address** (decision 34: "we
should proxy these too if possible and supported"). This replaces 1.7's
served endpoint (a model served from the plugin's own image on
`/run/tehas/serve/http.sock`):
- The admin runs the model server, outside tehas. There's no plugin image,
  build or socket.
- The endpoint plugin's manifest names it like a hosted one: id, API shape
  and models. The shape is `openai-completions` or `anthropic-messages`, the
  two supported. A server that speaks neither isn't supported.
- The address comes from industry's settings for that endpoint
  (`[endpoint.<id>]`), not from the plugin.
- Factories reach it only through industry's gateway, the same proxy as
  for hosted endpoints, with the same budgets and logs.
- Industry's endpoint spec (`docs/specs/2026-11-23-plugin-endpoints.md` in
  tehas-industry) swaps its "A served endpoint" part for the address. The
  industry lead owns that page.
- `tehas plugin build`, the `plugin-endpoint` argument list and `serve` are
  `building=plugins` in 2.0, so dropping them breaks nothing frozen.

An off plugin is fetched and vetted. With no build step, turning it on takes
effect immediately.

## 8. The plugins against 2.0.0

*Floating, not 2.1.*

Measured on 2026-11-24 from each repository's `HEAD`. Driver lines exclude
views, tests, prototypes and vendored code.

This table stays as the reference for each port. None of these plugins,
hoststats included, is placed on a release: they float, and each lands when
it's ready, by going on the signed list from its own repository (decision
50, #3331). Each is still pinned to a full commit and vetted.

| Plugin | Driver lines | Needs | Under 2.0.0 |
| --- | ---: | --- | --- |
| mattpocock-skills (`specs`), tutor | 0 | nothing; skills only | as is |
| panels | 0 | nothing (`menu`, `frame`) | as is |
| business, energy, hoststats | 50 each | `fs` | trivial port |
| console | 379 | `fs`; xterm.js as a verified library | small port |
| voice | 563 | `fs`; microphone and speech are on the view side, behind `device:microphone` (H10) | small port |
| lab | 549 | `fs`, `crypto` | small port |
| given | 518 | `fs`, `model`, `crypto` | small port |
| rsi | 1,636 | `fs`, `model`, `crypto` | port |
| review | 790 | `fs`, `git`, `model`, `crypto` | port |
| wiki | 1,467 | `fs`, `git`, `model`, `crypto` | port |
| guard | 1,015 | `fs`, `git`, `model`, `crypto`; `feed:advisories` and `issue:advisory` for the patrol it takes over | port |
| goldfish | 959 | `fs`, `git`, `model`, `crypto` | port |
| floorplan | 1,144 + 20,072 upstream | `fs`, `git`, `model`, `crypto`, `run.module`; upstream and elkjs as verified libraries | supported, with a shim |
| pageticket | 1,137 | `serve`, `upload`, `chat`, `session`, `net:` | **refused under 2.0.0** |

- **floorplan** (the owner: "Support floorplan: yes").
  - Its driver ports like the others.
  - Upstream Floorplan 0.11.0 is written for Node. In the unattended path
    (`refresh.mjs`, `build.mjs`, the workflow's scripts,
    `serve.mjs --export`), it uses:
    - a synchronous `node:fs` subset;
    - `path`, `url` and sha256;
    - `process` (`argv`, `env`, `exit` in 8 scripts, `stdout`);
    - seven git subcommands;
    - itself as a child process.
  - The builder bundles a Node shim of about 500 lines over `tehas.fs` into
    readable `dist/` output (the bundler aliases `node:*` to it).
  - Core adds `run.module` and `git.status`, `git.log` and `git.files`.
  - Upstream's `new Function` is allowed in a verified library. Its patched
    `health.mjs` is the plugin's own code and stays readable.
  - The automatic audit covers each upstream update.
- **pageticket** (the owner: "Leave pageticket out of v2: ok").
  - Its code would port: mostly pure functions, and its server answers only
    `GET`/`HEAD` for its view files and `/health`.
  - Its feature needs request handling, uploads, chat sessions and outside
    page capture. Each of those is a new security surface that needs a
    person's design review, which is application-platform scope.
  - It's refused under 2.0.0 and marked so on the vetted list. It can come
    back as a core or industry feature.
- **About 9,000 driver lines port by hand**, mechanically: `node:` imports
  become `tehas.*`, and git calls become `git.*`.
- The tehas-om plugins (diagnose, sentry, testmap) are Python, for
  tehas-om's own contract, and out of scope.

## 9. Work: the operator and ordinary plugins

*Floating, not 2.1.* Work item 8, the adapter, is in the normative file.


0. **The spike: validate before going forward** (decision 24). It ran on
   2026-11-24 and touched nothing in the trio repos. It's a bake-off of
   StarlingMonkey (v0.5.2 and starlingmonkey-ng) against QuickJS-ng through
   Javy, run on hoststats and then review (which adds `git`). For each
   engine, it checks:
   1. **Imports.** Can the engine be limited to exactly the `tehas:plugin`
      world?
      - For StarlingMonkey: compose it with WASI-Virt (sockets, HTTP and
        filesystem denied) and confirm with `wasm-tools component wit` that
        nothing else is left.
      - For QuickJS through Javy: confirm the runtime's imports.
   2. **Start-up.** Cold, with the `.cwasm` cache, and with a Wizer snapshot.
   3. **Memory per run and module size.**
   4. **Wall time** of a hoststats run and a review run against today's
      container run, plus guard's pattern scan against Node.
   5. **Types.** Does `jco guest-types` on `tehas:plugin` give builders a usable
      `tehas.d.ts`?
   6. **Views.** Load one outside-compiled Solid JSX view against the kit
      with the extra exports.
   7. **Host check.** `tehas plugin review hoststats --json` on a 2.1 build
      (part 4).

   **Results (2026-11-24).** The work and the exact commands are in
   `plugins-2.1-readiness-draft/spike/{starling,quickjs,views}/RESULTS.md`.
   The machine was a shared AMD EPYC 7742, so the times are noisy. The
   1.x baseline is `docker run` of `tehas-plugin-hoststats:sweep`.

   | | StarlingMonkey (componentize-js `--script-only`) | QuickJS-ng (own engine on the `javy` crate) | 1.x container |
   | --- | --- | --- | --- |
   | Imports beyond `tehas:plugin` | `wasi:random`, `wasi:clocks`, `wasi:io/poll` | 9 WASI p1 functions, linked to grant nothing | - |
   | hoststats output against Node | byte-identical | byte-identical | - |
   | Engine size (gzip) | 12.0 MB (3.9 MB) | 1.25 MB (427 KB) | 1.2 GB image |
   | Compiling it (once, at deploy) | 1.2 s, 1.0–1.4 GB peak | 0.44 s | - |
   | `.cwasm` | 43 MB, loads in about 2 ms | 3.7 MB | - |
   | One hoststats run, from the cache | 2.6–3.2 ms | 0.9–1.5 ms | 516 ms median |
   | Memory per run | 9.1 MiB linear | 1.25 MiB linear | 11.1 MB container peak |
   | guard's scan, 1,478 files, against Node | 31.95 s vs 1.29 s (25×) | 39.9 s vs 1.14 s (35×) | - |
   | Types from the WIT | `jco guest-types`, passes `tsc --strict` | mapped by hand | - |
   | Grant limits (outside a grant, `..`, symlinks, read-only roots) | ungranted call refused | all refused | - |
   | Time and memory caps trap a run | - | yes | - |

   What this settles:
   - Both engines are fast enough for glue: milliseconds against half a
     second for today's container.
   - Neither engine is fast at regular expressions over large inputs, and
     reading the files is about ten times slower too. guard's scan loop goes
     into a `dist/wasm/` module (part 1.1), whichever engine is chosen.
   - The operator compiles the engine at deploy, never at run time,
     because compiling it takes up to 1.4 GB.
   - Views: compiled Solid JSX runs under the gate's CSP. The kit change
     and the types are in part 6.

   Not run yet:
   - the review plugin (which adds `git`);
   - fuel running out, and the time and memory caps trapping a run on
     StarlingMonkey (it ran with fuel on and off, but nothing hit a cap);
   - the automatic vetting checks of part 3;
   - check 7, the host check, which needs a 2.1 build on a host;
   - Firefox and Safari for the view's import map;
   - the client-side image check of part 6.1.

   When they run (the roadmap manager, 2026-11-24): low priority, through
   the shared build slot script only, never ahead of alpha builds. The host
   check runs on a 2.1 build after tehas alpha.1.

   **Chosen engine: StarlingMonkey through componentize-js** (decision 27), for:
   - the typed WIT world, from which `tehas.d.ts` is generated, as part 5
     wants;
   - building from a Bytecode Alliance release rather than maintaining an
     engine crate of tehas's own;
   - standard web APIs and fresh clocks.

   It costs 9 MiB per run instead of 1.25 MiB, and a 12 MB engine. That
   needs a cap on concurrent runs, which the operator needs anyway.
1. The `tehas:plugin` WIT world in `api/`, the generated `tehas.d.ts`, the
   fakes and `tehas plugin test`.
2. `tehas-operator`: the engine, the limits, the host functions, a fuzz
   target for each, and `run.module`.
3. `tehas plugin check` and `tehas plugin review` for part 3: the import
   rules, readable JS, verified libraries, grant fit, the module audit and
   the `dist/` hash.
4. The signed vetted list: `vet.yml`, Sigstore keyless signing,
   `api/plugins-signers`, serial and expiry, revocation, and auto-approval
   at `tehas update`.
5. Views: the `<tehas-plugin-view>` wrapper, the kit's JSX exports and
   `JSX` types, and the import map.
6. The 13 plugins with drivers float and port to `src/`/`dist/` when
   ready (part 8, decision 50); the 3 with skills or menus only stay as
   they are. hoststats is the first candidate to prove the ordinary path
   (#3328), on no release's schedule. Refuse pageticket on the vetted
   list.
7. Remove the 1.x image, build and container paths. There's no
   compatibility layer.

## 11. Threats of the operator and views

The rows of the normative file's 11.4 that belong to the parts here.

| Threat | Rule (section) | Law or test | Evidence |
| --- | --- | --- | --- |
| A plugin calls an ungranted host function | Grant checks in each host function (part 5) | the ungranted-call refusal; a fuzz target per host function | passed (spike, 2026-11-24); fuzz proposed |
| A gate passes with no answer | No answer means hold (1.2, part 5's gate shape) | the schema's samples; **OPEN**: a law that a malformed answer holds | proposed |
| `/state` written without `keep` | `/state` writable only under `keep` (part 5) | part 5's `/state` law | proposed |
| A press is lost in a merged run | Presses never merged (1.2) | 1.2's press law | proposed |
| A plugin's output carries terminal or HTML tricks | Output escaped and capped (H4) | **OPEN**: a law for control sequences and HTML in each output field | proposed |
| A sibling view reads another view's messages | One port per load, frames known by port (6.3) | 10.10's window-message test; **OPEN**: a law that a revoked port's nonce is refused | proposed |
| A view floods the host element | Caps per direction and type, one press in flight (6.3) | 6.3's limit law | proposed |
| A view injects markup into the host's page | Text only as `textContent` (6.3) | **OPEN**: a test that markup in a message renders as text | proposed |
| A press acts as the person without consent | The pre-press confirm, the post-run confirm for drafts (6.3) | 6.3's laws | proposed |
| The viewer's login leaks into shared output | `viewer:login`'s rules (6.2) | **OPEN**: a law that a run with `ctx.viewer` has no `keep`, `record`, `issue:*` or `model` | proposed |
| A view sends data out directly | `connect-src 'none'` (6) | **OPEN** until H8's spike runs (part 9) | proposed |
| A view borrows the dashboard's session or devices | The sandboxed frame, devices by two keys (H8, H10) | **OPEN** until H8's spike runs (part 9) | proposed |
| A view module calls web APIs | Browser WebAssembly boxed (H9) | **OPEN**: a vetting test that a `__wbg_*` import is refused | proposed |

## Appendix A. Bytecode Alliance projects reviewed (2026-11-24)

All 105 public repositories of github.com/bytecodealliance were reviewed.
Language bindings, docs, governance, mirrors, samples and compiler internals
aren't relevant here.

Used in this spec:

| Project | Where | For |
| --- | --- | --- |
| wasmtime (with Cranelift) | 1 | the operator's runtime and its ahead-of-time compiled cache |
| StarlingMonkey | 1, 9 | the chosen engine, built through componentize-js (decision 27) |
| starlingmonkey-ng | 1, 9 | the next StarlingMonkey; no release yet, so not tested |
| componentize-js (`--script-only`) | 1, 9 | builds the chosen engine, StarlingMonkey (decision 27) |
| Javy | 9 | tested in the spike, not chosen |
| WASI-Virt | 1 | removing standard input and output from the engine, once, at its build |
| Wizer (`wasmtime wizer`) | 1 | not used now: componentize-js snapshots the engine itself; a later option for snapshots of a plugin's loaded code |
| wasm-tools (`wasmparser`, `wasmprinter`, `wit-component`) | 1.1, 3 | module checks, disassembly in review, component inspection |
| wit-bindgen, jco | 2, 5 | the `tehas:plugin` WIT world: Rust bindings, generated `tehas.d.ts`, local runs in Node |
| approved-contributors | 4 | an allowlist for pull requests on core plugin repositories |
| actions | 4 | wasmtime and wasm-tools in `vet.yml` |

Watched, not used yet:
- **nightmonkey** and **weval**: ahead-of-time JS-to-WebAssembly
  compilation. The speed isn't needed.
- **wasmtime record-replay** (a prototype): deterministic replay of a
  plugin run, for debugging and incident audits.
- **opentelemetry-wasi**: traces from plugin runs.
- **wasm-pkg-tools** and **rust-oci-wasm**: the engine or plugins as OCI
  artifacts by digest, an alternative to git and GitHub releases.
- **cargo-component**, **componentize-go**, **componentize-py**,
  **componentize-dotnet** and **wstd**: builders' tools for WebAssembly
  modules in other languages. Pointers for builder documentation, more
  relevant if components replace core modules later.
- **sightglass**: wasmtime's benchmark suite.

An idea for core, beyond plugins: tehas parses untrusted content (issue
text, pull request contents). The operator's setup could run risky parsers
(image decoding, archives, markdown) inside WebAssembly instead of in the
main process. That's to be looked at once the operator exists.
