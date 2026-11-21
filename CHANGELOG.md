# Changelog

What each release of tehas changes, and what an adopter must do after
updating to it. Newest first. A release's section is `## vX.Y.Z (YYYY-MM-DD)`
with `### Changes` and `### Adopter action`; "None." is a valid action.
`scripts/tag-release.sh` refuses to tag without an Adopter action, and the
release workflow takes the release's page from its section
(`scripts/release-notes.sh`). Work in progress goes under `## Unreleased`;
`scripts/tag-release.sh` renames it to the version in a pull request before it
tags a release, and starts a new empty one above it. A prerelease's page is
what is new under `## Unreleased` since the tag before it.

A pull request does not edit this file. It adds its lines in
`changelog.d/<issue>.md` (named after the branch, each `/` a `-`, when there is
no issue), under `### Changes` and `### Adopter action`, either of which may be
left out. `scripts/release-notes.sh Unreleased` shows them in the Unreleased
section, and `scripts/tag-release.sh` moves them into the release's section in
a pull request before it tags, with the head's `scripts/collect-changelog.sh`.

## Unreleased

### Changes

- `tehas wake [--json]` asks the factory for a poll now and returns at once, the same ask as `tehas dispatch --soon`, for a webhook receiver or a scheduler. `tehas api` lists the feature `wake` (#2436).
- `tehas order --json` adds `starts`: for each ranked issue, whether the dispatcher's last poll started it, or a reason code and text for why not, taken from the poll's own decision. `tehas blockers [--json]` is new. It shows the holds between issues the last poll read, any cycles among them, and the issue at the end of each chain with what holds it. `tehas api` lists `start-reasons` and `blockers`. Which issues start does not change (#2442).
- With outsource on, a factory kills a `docker save` of its sandbox image that runs past 30 minutes and answers 503, so one hung save no longer stalls every contractor's image download (#2536).
- `bootstrap.sh user` and `check` read the instance's `PI_RUKAS_FOLLOW` from `instance.env`, as the sandbox build does, so the host's pi-rukas clone follows the same commit as the image. `tehas update` no longer prints its `bootstrap.sh user` line on every run for an instance that sets its own `PI_RUKAS_FOLLOW`. Instance settings that do not load stop `user` and `check` with 78 (#2603).
- A release's pinned pi-rukas commit is fetched by its id when no tag or branch upstream holds it any more, so `bootstrap.sh user` and the sandbox build still find it. One that cannot be had stops with a message that names it (#2604).
- With `TEHAS_MILLWRIGHT_EPICS=on`, a `tehas:ready` on an epic that a person or an outsider took off no longer approves the epic's sub-issues for the millwright. Tehas's own removal still keeps the approval. The count of waiting work and the epic's comment read the label the same way the starts do (#2605).
- A product can pin its own Node in the sandbox: an instance layer puts it first on PATH, and a tehas Node bump no longer moves the Node its tests run on. The image keeps tehas's Node in `/opt/tehas-node`, and pi and the npm tools tehas ships run it by its path. `instances/example/product-node.Dockerfile` is the example (#2607).
- `TEHAS_EDGE_ONE_LEVEL=on` keeps the edge's names one level under the parent of a `TEHAS_DOMAIN` that is itself a subdomain, where Cloudflare's free certificate covers them: with `demo.example.org`, a preview is `uat-12-demo.example.org`, a ticket `lab-7-demo.example.org` and the list `lab-demo.example.org`. The pull request's notice, the dashboard's Previews and the Protolab's links say these names; the proxy in front maps each onto the router's `<label>.<domain>` (`edge.md`, "A domain that is itself a subdomain"). Off by default, and refused with a domain of two labels (#2618).
- A factory that acts as a GitHub App now gets previews of its own cycles' pull requests. They are opened as `<slug>[bot]`, which GitHub's `author_association` calls `NONE`, so `tehasbot preview` counted them as opened by someone outside the repository and built none. The App's login, from `state/github/login`, now counts as the repository's own, for a branch in the repository only; any other bot still does not (#2621).
- The bundled SQLite is 3.54.1 (rusqlite 0.41). SQLite 3.51.1 could deadlock when two threads of one process opened and closed the same database at once, which stopped every later store call of the model proxy's job channel and hung the contractor tests in `tehas check-product --sandbox` (#2536). The update also fixes RUSTSEC-2026-0459.
- The kit's seven modules carry JSDoc types and check clean under `tsc --checkJs --strict`, held by a workspace test with a pinned tsc 7.0.2. A program that copies the kit gets its types from the modules and `vendor/solid.d.ts`, with no copies of its own. The dashboard's script grows by 5123 bytes, nearly all of them comments (#2629).
- A second `pageStore` of the same name whose actions read the state while they are made now gets the store, where it threw a TypeError (#2629).
- The release's pins step moves the kit's Solid too: `scripts/pins.sh check` lists solid-js, @solidjs/signals, @solidjs/web and @solidjs/html with their newest 2.x on npm, and `move` takes each to it with its tarball and integrity and rebuilds the kit's `vendor/solid.js` with `tools/vendor-solid`. A 3.x is held back, and a release whose Solid dependencies the other pins do not meet is kept back with the reason (#2632).
- A park comment no longer tells a person to run pi-rukas's `/work <n> --restart` or `--allow-protected`, which a tehas host has no way to run. It says what to do instead: change a protected path yourself, then tick a retry box on the comment (#2637).
- A new manual page, Start (`/start`), for a coding agent a developer sends there: what to find in the repository first, the install in order, the five places it stops to ask a person, and what is not built yet. The overview, the For agents page and `llms.txt` link it.
- The kit ships `vendor/solid.d.ts`, types for the names its Solid exports, and lists it in `kit/MANIFEST`. A program that copies the kit can check its pages with `tsc --checkJs` against it. An install does not copy a `.d.ts` file into the home, so no page changes (#2629).
- `tehas <part> [words]` runs `tehas-<part>` from `PATH`, so the other parts of the tehas family are commands of tehas: `tehas industry ...` runs industry. A command of tehas always wins, and `industry`, `kaizen`, `kata`, `monozukuri` and `yokoten` are kept for the family. `tehas help <part>` shows the part's help.
- Each release carries `tehas-front-vX.Y.Z-linux-x86_64`, a small signed `tehas` for a machine that runs no factory, such as the one that runs industry. It knows only its help and the parts' commands.
- Plugins, a feature still being built: a release refuses `TEHAS_PLUGIN=on`, and a line of `TEHAS_PLUGINS`, `TEHAS_PLUGIN_GRANTS_<NAME>`, `TEHAS_PLUGIN_ACCESS`, `TEHAS_PLUGIN_LIMITS` or `TEHAS_PLUGIN_MODEL_DAY`, with 78, and `tehas api --json` lists `plugins` in `flags` as `false`. This release brings the settings' rules, the reader of a plugin's manifest with `api/plugin-manifest.schema.json`, and `tehas plugin [--json]`, which exits 78 while the feature is off. Nothing is fetched, built or run yet.
- The files in `api/` are dedicated to the public domain under CC0-1.0, in `api/LICENSE`, so a program may copy the schemas and samples freely.

### Adopter action

On a host whose instance sets `PI_RUKAS_FOLLOW`, run `./bootstrap.sh user` once after updating to move the clone to the image's commit.

Nothing is required. To keep a product's tests on its own Node, copy `instances/example/product-node.Dockerfile` to `sandbox.Dockerfile` in the instance directory (or add its lines to yours), set the version and its sum, run `~/tehas/sandbox/build.sh` and `tehas check-product --sandbox`. A layer that unpacks a Node over `/usr/local` should move it to a prefix of its own.

## v1.1.0 (2026-11-20)

### Highlights

- **Solid 2 front-end kit.** `host/fun/kit/` carries Solid 2 as one vendored module, with no build step. The dashboard at `/dashboard` is now drawn by Solid islands on it and looks the same. `host/docs/frontend.md` describes the kit.
- **Releases pin pi-rukas.** A release's `host/versions.env` pins the pi-rukas commit its release gate built against, so every install of a release gets the same pi-rukas. pi stays pinned at 1.5.2. An install from a checkout still follows the newest pi-rukas release.
- **Node 26.** Node moves from 22.23.5 to 26.13.1 (LTS), on the host and in the sandbox image, where a product's tests run too.
- **Proofs on the release gate.** Kani proves the label grant decision, retries, the slice arithmetic, the zip path rule and placement on every release.

### Changes

- The dashboard's times beside the issues, the queue and the previews show; they were empty before. Every island that reads a `/live/` file shares one poll of it.
- The kit's feeds, streams (server-sent events and NDJSON) and presses cover what a hand-written page client did: headers, credentials, backoff, optimistic posts. See `host/docs/frontend.md`.
- The docs server sends `Cache-Control: no-cache` for files under `/fun/` without a content hash, so a browser no longer keeps an old copy after an update. Hashed files in subfolders are cached for ever, as the others are.
- An install stops, before it installs anything, on a symbolic link in `~/tehas/fun` or `~/tehas/docs`, and names the path. It copies the dashboard's pages last, so a failed install leaves the old pages working.
- `versions.rukas.latest` in `tehas status` and the factory API names the release a pinned pi-rukas commit is; a host installed from a release showed nothing.
- An instance's `PI_RUKAS_FOLLOW` in `instance.env` still wins for the sandbox image. `bootstrap.sh` does not read it (#2603).
- A `TEHAS_CYCLE_RESERVE` too large for the slice no longer overflows the memory the error message names (#2446).

### Adopter action

Run `./bootstrap.sh user` after the update (the update's `later:` line says so). It installs Node 26.

A product's tests now run on Node 26 in the sandbox. A test that depends on Node 22's behaviour or error text may fail, and `tehas setup` then stops at `check-product --sandbox` with exit 3. Fix the test and run `tehas setup` again. Node 26 no longer ships corepack, so a `sandbox.Dockerfile` that runs `corepack enable` must install yarn or pnpm itself.

If anything in `~/tehas/fun` or `~/tehas/docs` is a symbolic link, replace it before you update. Move your own files out of `~/tehas/fun/kit/` and `~/tehas/fun/factory/`: the install now removes what the release does not have there.

A program that builds pages on the kit copies `host/fun/kit/` whole at a tehas tag and checks it against `kit/MANIFEST`. `host/docs/frontend.md` says how.

## v1.0.1 (2026-11-20)

### Changes

- `tehas labels` pins a label that was there with no pin when GitHub says it was created within a minute of a label already pinned, as one `tehas labels` run made them. So the update to 1.0 pins the `tehas:blocked` an older tehas made, and `tehas setup` no longer stops on it. A label renamed to a pinned name keeps its creation time and is still refused (#2581).
- The poll's comment read costs a tenth of what it did. It no longer asks every comment for its edit history: it asks only the comments GitHub says were edited, by their ids, and puts each history back where the comment read gave it before. A chunk of ten issues' comments now costs GitHub 1 point instead of 10. What the rules read is unchanged: an edit by an account the start rules do not take still holds a start, every edit counting. When a history cannot be read, the rules that judge it hold or wait: a start held on an issue's edits stays held, and a box the millwright ticked in a planning question waits for the next poll without acting or writing a note. The finish asks no longer pay for a history read they do not use.

### Held back

- Node 26.13.1 is out, a new major version. `host/versions.env` keeps 22.23.5: the move is a person's call.

### Adopter action

A host that stopped on `not pinned: the label tehas:blocked ... was there before with no pin` updates to this release, and its setup pins the label. If `tehas labels` still names it, the label was not made with the others: check it on GitHub and pin it by hand as the message says.

## v1.0.0 (2026-11-19)

### Changes

- The pinned ctx7 moves from 0.11.1 to 0.12.2.
- The schema of `tehas status --json` and the manual say the fields of `contractors` may change while outsource is still being built.
- The features page says what has been seen on a host and what only the tests cover. Rows of features still being built say so first, and stale claims are gone (#1790).
- The manual names no command that does not exist: `tehasbot clean`, `tehasbot review` and `tehas-cycle finish` where it said `tehas clean`, `tehas review` and `tehas finish`. `tehasbot clean --help` names `console clean ended` in place of a script that is gone (#1790).
- Troubleshooting lists the four lines the poll logs for a `tehas:finish` ask (#1695).
- The features page's Known limits name what a person running tehas should know
  before 1.0 and what to do about each: the token, network and model key a
  cycle's container holds, what the cycle's token may write, GitHub's bad
  minutes, the issue read again after the approval check, the lean driver,
  `live/` files, the MCP server on a private network, a ticked retry under a
  limit of 0, and a kept installation token.
- The operating manual has "Updating from 0.30.x to 1.0": edit the settings,
  drain, update, labels, and every setting, label, command form, field and
  exit status 1.0 renamed or removed, with what to do for each.
- The manual's front end page says the pages read the files under `/live/` add-only, and a committed sample of each file the pages read is now tested against the page code.
- `host/release/install --version` takes a prerelease too, such as `v1.0.0-beta.1`, so a new host can install a beta.
- `api/renames-1.0.tsv` lists the names 1.0 removed that it missed: the label `cross-project`, `tehasbot improve`, the feature words `fixer-grant`, `refresh-conflicts` and `refresh-pr-sweep`, and the fields `fixer` and `refreshScan` of `tehas api --json`.
- `TEHAS_REFRESH_CONFLICTS` and `TEHAS_REFRESH_EVERY` are rows of `api/renames-1.0.tsv`, so `tehas update --check` lists a line that still sets one. A value the timed scan took still loads and is dropped; any other stops every command with 78 and a line that says the scan is removed.
- A line that gives a value to a setting of a feature still being built, a `TEHAS_OUTSOURCE_*` setting or `TEHAS_PROTOLAB_ZIP_HOSTS` in this release, stops every command with 78 and names the feature, as its switch set to `on` does. Nothing read these lines while the feature could not be turned on.
- `api/status.schema.json` says the fields of `prototypes` may change in a minor release, as `running[].progress` does, and the status samples no longer name another program in an order's reason. The manual says the feature word `clean` means the hourly cleaner runs on the host, and no longer says `TEHAS_GITHUB_APP_INSTALLATION` and `TEHAS_BOT_LOGIN` are ignored: both stop every command with 78.
- The public surface is pinned in the repository: `api/commands.txt` lists every `tehas` command, its subcommands and the flags of each, with the flags only a unit or the dashboard writes marked internal; `api/api.schema.json` with samples is the schema of `tehas api --json`, and `api/features.txt` holds the words of its `features`. The gate fails a command, a subcommand, a flag or a feature word that changes without them.
- `tehas start --json`, `tehas state check --json`, `tehas refresh --pr N --json`, `tehas export --json` and `tehas analytics copy --json` each have a schema and samples in `api/`, and the manual names every field. `tehas top --json` is marked not public.
- `console forge`, an internal command the manual offered to bash callers, is gone: nothing called it, and the contract lists `console` subcommands as internal.
- When a lean run ends not ready because the repository's `.pi/work.json` names no `verify.test`, or does not load, the park comment now says the repository needs the setting, links the manual, and says the issue's text is fine. Before, it asked for a better `pi-spec v1` block. The retry boxes are unchanged.
- The park of a lean start that `tehas work` refused now names what was not supported and who is at fault. When the sandbox image's pi-rukas lacks it, the comment says to rebuild the image to keep the issue on lean, or to take `tehas:lean` off to run it on classic, said as a change of driver. When tehas passed `work-lean` a flag it never takes, the comment says this is a fault in tehas and asks for an update, not for the label to come off; a Retry of that park is not held for the label.
- `dispatch.log` has a start's line before the lines of the cycle it starts. Before, a start refused at once could log `done #N` before `start #N`.
- A start refused before its runner ran logs one line that names the refusal, in place of the warnings about a missing result and progress.
- A classic start refused by `tehas work`'s check now ends as `refused`, as a refused lean start does, and not as `unknown`.
- `/health` and `tehas status` say when the last start was refused, with a few words of why, rather than nothing: `The last start was refused before anything ran: #7, the sandbox image lacks what it was given.` The status does not fail on it.
- The shell tests link `lib/` into their scratch trees with one helper that replaces a link already there, so an interrupted or repeated run no longer leaves a stray `lib/lib` in the checkout.
- The manual says `.pi/worktree-setup` is run with POSIX `sh`, not by its first line; that a preview can send same-site requests to a host that fronts the dashboard's domain, and what such a host does about it; and that a kept GitHub token is used for up to 50 minutes after the app loses the repository.
- A plan's title and body, the comment that ends a planning session and the comment on an issue a refine replaced are masked before they reach GitHub, as other planner text already was. A plan's title is cut at 256 characters.
- The crates' build scripts in `install.sh` are no longer given the model keys or any other variable whose name holds KEY, PASSWD or CREDENTIAL, in any case.
- A poll that cannot read the issues labelled `tehas:plan` or `tehas:refine` no longer ends: it logs a warning, starts no planning session and still starts cycles from the ready queue (#1702).
- The Protolab's log says why a pass found nothing: the open tickets could not be read, with gh's reason, or GitHub's answer was not a list. A pass with no GitHub credential says so, and a file GitHub refused to it is listed on the page as asked for without one. The unpacker's line about a zip is masked for tokens before it is logged.
- A Retry ticked in a park comment now counts as waiting work in `demand.waiting`, so a program that holds the limit at 0 while nothing waits raises it and the retry starts (#1776).
- Store schema 18: an outsourced job has two tokens. The model token goes into the container's `models.json` and reaches only the model paths. The job token stays with the contractor program and reaches only the job channel. Each is refused on the other's paths with 403. Both are made together, end together, and survive a restart of the model proxy. Outsource is still being built and cannot be switched on.
- `tehas setup` rebuilds the sandbox image when the instance's `sandbox.Dockerfile` changed. It used to leave an image on the current pi-rukas commit as it was, so the change reached cycles only after `tehas update`.
- The poll's tidy gives `tehas:shipped` to a closed issue that a merged pull request closed when it tidies several issues at once. Its read of what closed them now names each issue by number in the repository; the search it used before matched the issues' text, so it often found none of them and logged that no merged pull request was read as the closer.
- The docs server no longer serves the live console of a running cycle unless the instance sets `TEHAS_SITE_CONSOLE=on`. Without it `/live/console-<n>.txt` and its `.now` file answer 404 and the console panels of the site stay empty. A full key of the MCP server still reads the console.
- An archive attached to an issue is read more strictly: an entry that claims more than the room left is refused before any of it is unpacked, an entry whose place in the archive cannot be worked out is left out as damaged, an unpacked file never replaces one already in the directory, and a retry holds an archive to the files it gave, each to its own bytes, with none added or gone (#2248, #2258).
- Outsource, still being built: the proxy's listener serves a contractor the factory's sandbox image (`/outsource/v1/image`, by the contractor's key), and by a job's token its code index (`index`), a git bundle of the product's tip (`bundle`) and the issue's images and files (`attachments`). The contractor program takes the product, the index and the files from there instead of cloning or fetching them, and loads the factory's image when the factory names another (#2363, #2365, #2380, #2381).
- With outsource on, a cycle's vipune calls go to the factory through the cycle's issue relay, in-house and on a contractor alike, and the factory runs the pinned vipune for the cycle's own database. The image's `vipune` is a stand-in that runs the real one when the factory does not ask for its calls. Outsource is still being built and cannot be switched on (#2366).
- Outsource, still being built: the proxy's listener has an egress tunnel route, `/outsource/v1/egress`, an HTTP/1.1 Upgrade that takes a job's model token and reaches only GitHub, the registries and `TEHAS_EGRESS_HOSTS`, never a model endpoint, a private address or the factory itself. The contractor program's clone and salvage push go through it (#2378).
- Outsource (still being built): a job names the factory's account, and before the container starts the contractor program makes that account the owner of the cycle's folders when the container runs as it: the workspace, the code index cache, the relay folder and the cycle's other writable folders. The code index refuses a cache folder it does not own (#2364).
- A container's `--cpus` is capped at the CPUs the Docker daemon counts. A daemon started before the host was given more CPUs refused every cycle's install with exit 125; tehas now sizes the containers for Docker's count and logs a line naming both numbers.
- A start that a step of `tehas work` stopped before the runner ran is parked with a comment that names the driver and, for a failed product install, what failed, such as `docker refused --cpus 47.00, it counts 16 CPUs`, instead of `status unknown, last step -`.
- The contract freezes the host namespace: `/host/` on the factory's origin belongs to the host that fronts the factory's domain, tehas serves nothing under it, and the host bridge stays `bridge: 1`. New tests hold the docs server's 404 for any path under `/host/`, a ticket note's link rule, and a host menu item opening in the dock.
- The manual's page "The host" at `/host` is served again: the docs server keeps its 404 for the host's namespace to paths under `/host/`.
- The sandbox image hands `/home/vscode/.npm` to the cycle's user, so npm in a cycle no longer fails on a root-owned cache. The image build stops when that user cannot write to npm's cache (#2407).
- The sandbox image sets `ORT_INTRA_OP_NUM_THREADS=1`, so vipune no longer writes onnxruntime affinity errors to stderr on every call under a cycle's cpuset. The build's smoke run now also runs vipune on one CPU of a cpuset and refuses an image whose vipune writes to stderr there (#2408).
- The move of the pinned tools takes a prerelease as the release it writes to, such as `v1.0.0-beta.2`, so the release script moves pins before a beta too.
- The prototype build refuses an issue type event whose actor or stamp is not text, and keeps the published page, instead of reading it as an account that is gone and taking the page down (#2242).
- The pause flag `state/pause.json` is read once, under the 1 MiB bound and not through a link. A flag over the bound, or a link, reads as unreadable and keeps the factory paused (#2251).
- `analytics prune` names an expired kept-aside database it could not delete in a `WARN` line and exits 73, where it used to exit 0 (#2261).
- `TEHAS_POLL_SECS` sets how often the dispatcher timer polls, from 30 to 3600 seconds, 120 by default. `tehas setup` and `tehas update` apply it as a drop-in for `tehas-dispatch.timer` and restart the timer, and remove the drop-in at the default. `tehas status` names an interval off the default on its poll line.
- `tehas dispatch --soon` asks for a poll and returns at once: it writes `state/dispatch.again`, and `tehas-dispatch.path` starts the poll, or starts it after the poll that is running. Another program on the host, such as a webhook receiver, can use it to wake the dispatcher.
- `install.sh` installs outsource's model proxy (`libexec/tehas-proxy`), its unit `tehas-outsource-proxy.service` and the contractor program (`libexec/tehas-contractor`) only while `TEHAS_OUTSOURCE` is on. With it off nothing of the feature is installed. `scripts/pack-release.sh` takes the two programs with `--proxy` and `--contractor`. Outsource is still being built, so a release does neither (#2278).
- The manual's outsource page says how to switch the feature on, lists all its settings, says where a cycle runs, and has a Known limits section: trusted hosts only (#2278).
- The pinned gh moves from 2.104.0 to 2.105.0.
- The pinned Playwright moves from 1.65.0 to 1.65.1.
- `TEHAS_DISK_WAIT` is read by one rule: one or two digits with no leading zero, 10 seconds for anything else. `console disk` and the dispatcher's disk read no longer take `+5` or `05` as a wait (#2197).
- When the dispatcher checks whether a comment it may have posted is on the issue, it now takes one `[bot]` off a login, as the poll does, so a comment by `slug[bot][bot]` no longer counts as the bot's (#2200).
- A park owed a retry after a provider outage whose record the dispatcher cannot read is now logged. One with no start time is spent once instead of being skipped in silence every poll (#2204).
- A directory a product's test left closed in a cycle's scratch (mode `000`, owned by the factory's account) no longer stops every later start of the issue: the start opens it up and empties the scratch. When the scratch still cannot be made, the park names the folder instead of only "exit 78" (#2455).
- `api/settings-rules.json` marks each row of a feature still being built with `"building": true`, and `api/api.schema.json` no longer requires a building feature's key in `flags`. Such rows and keys are not contract and may change until the release that ships the feature. The contract page gains the rules for 1.x: readers ignore fields they do not know, a 1.x release only adds fields, enum values and exit statuses, and the update to 1.0 comes from 0.30.x, 0.31.x or 0.32.x.
- The features page lists the poll's hold while the sandbox cannot push and the settling of a GitHub write that may have reached GitHub, both covered by the tests and not yet seen on a host. It drops the ticked retry limit fixed in #1776, the bot account's token and the separate Spec pin row, and names two limits: a park the host caused holds its issue until the body changes or Retry is ticked (#1742), and nothing deletes the branches of closed issues (#1791).
- `api/surface-1.0.txt` lists the public surface of 1.x: every public command, subcommand and flag, setting, role, label and `--json` field. The gate fails when one goes away. `host/docs/contract.md` names it.
- `tehas update` and `tehas update --check` on a 1.0 host list every setting tehas 1.0 renamed or removed that the instance still sets, as the manual says, instead of only the first.
- 1.0 updates a host on 0.30.x, 0.31.x or 0.32.x. A 0.30.x host can name only a release candidate or the release with `tehas update --to`, not an alpha or a beta; update to v0.32.0 first to try a beta. The store's tests now migrate the stores v0.31.0 and v0.32.0 made, and a shell test updates a home a v0.32.0 install left.
- The update manual names a known limit: a program that manages the factory and writes 0.30 names such as `TEHAS_TICKET_CONCURRENCY` into its `instance.env` stops the update with 78 until the manager writes 1.0's names.
- `tehas dev` no longer refuses to start, with exit 78 and "is not a directory of this account", when the cleaner takes the clone's build output just as the command makes it. The directory is made again once the command holds the clone's lock (#2477).
- `spec-pipe`, still being built: the manual defines the pull request spec block, version 1, and tehas can read it. The manifest's `spec` entry names the block version a cycle writes in `block`, and a runner that lists `issueSpec` is expected to write it. Nothing writes or checks the block yet (#2480).
- `spec-pipe`, still being built: with the feature on, a handoff whose approval pins a spec waits for a merge only when its pull request's spec block names that sha256. A pull request with no block, a block of another sha256 or one tehas cannot read parks the issue with `needs-human-attention` and the park reason `spec-block`, and stays open (#2481).
- The release tarball now carries `tehas-proxy` and `tehas-contractor`, the two programs of outsource. A host installed from a release still installs them only while `TEHAS_OUTSOURCE` is on (#2463).
- `console slice room` names a reserve that is no size as what it is:
  `TEHAS_CYCLE_RESERVE=<word> is not a size` instead of "the slice was not
  probed", so the line points at the setting and not at
  `/sys/fs/cgroup/tehas.slice`. Its reserve field prints the setting's own
  word where a size is printed in bytes, and `nosize` when the name is unset,
  so the line keeps its places. A pressure figure too big to count counts as
  the slice stalling, not as a slice that never stalled (#2235).
- `install.sh --lab-build` installs a build from a checkout that reads `TEHAS_TEST_FEATURES`, so a lab host can prove a feature that is still being built. A release tree is refused with 78. While the variable is set, health warns, `tehas status` prints a `features` line and `tehas status --json` has `testFeatures`; a release that finds it set says it does not read it. See "Proving a building feature on a lab host" in the manual (#2489).
- The header of the docs and the business page shows a new wordmark, teHas in IBM Plex Sans Bold with the upper right of the H cut at the crossbar.
- Part of the lean driver, still being built: a release refuses
  `TEHAS_DRIVER=lean`, so nothing changes on a host yet. With lean on and
  `TEHAS_DRIVER=lean`, a start with no driver label runs lean when the issue's
  text holds a `pi-spec v1` block and classic when it does not, and the poll
  logs which and why. `tehas:lean` and `tehas:classic` from an account in
  `TEHAS_ALLOWED_LABELERS` then each choose the driver for one issue (#1099).
- `install.sh --lab-build` builds with the marker `TEHAS_LAB_BUILD=on` in place of debug assertions, so a lab build reads `TEHAS_TEST_FEATURES` and none of the tests' other hooks (#2500).
- Health warns on a lab build even with `TEHAS_TEST_FEATURES` unset, and `checks.testFeatures` on `/health` and `testFeatures` of `tehas status --json` carry a new field `lab` (#2501).
- `build.sh --if-moved`, which `tehas setup`, `tehas update` and `bootstrap.sh user` run, now rebuilds the sandbox image when the instance's `sandbox.Dockerfile`, or a file it copies, changed since the image was built. The instance's layer carries the sum as the label `tehas.layer` (#2496).
- `install.sh` builds the programs without the caller's rustflags or release profile settings (`RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`, `CARGO_TARGET_<triple>_RUSTFLAGS`, `CARGO_PROFILE_RELEASE_*`) and without the rustflags of cargo's configuration files, so `-C debug-assertions` there can no longer turn on the store's test hooks. The release workflow builds the same way (#2512).
- The pinned bun moves from 1.4.4 to 1.4.5.
- Health now compares the sandbox image's instance layer too, as `build.sh --check` does. `checks.rukas` gains `layer` (the image's `tehas.layer` label), `instanceLayer` (the sum of the instance's `sandbox.Dockerfile` and what it copies) and `layerMoved`, and `/health` says `warn` while the layer moved. The MCP server's `factory_status` passes the three fields on (#2520).
- The front pages, the manual, `llms.txt` and both share images name tehas "The Lean Software Factory". A new manual page, `/lean-software-factory`, says what makes it lean and links the Pi software factory page, which stays.
- The sandbox build's vipune check no longer refuses a good image when dockerd turns down the CPU it picked. In a container whose cpuset moved after dockerd started (Incus with `limits.cpu`), `tehas update` to a 1.0 beta failed with "Requested CPUs are not available". The check now runs again on a CPU both the build and dockerd name (#2519).
- `tehas status --json` has `previews` on a host that runs previews: each open pull request with a preview, by `number`, with the preview's `state` and, while it serves, its `url`. `tehas api` names the feature `status-previews`.
- A host bridge menu item can ask to open as a page of its own: with `"open": "page"` the front pages draw it as a plain link that opens its page in a new tab, with no dock or frame. An item without the field, or with any other value, opens in the dock as before.
- The review loop is now a feature still being built, as no run of it was
  seen on a host. A release refuses `TEHAS_REVIEW=on`, and a
  `TEHAS_REVIEW_REPO` line, with exit 78 and names `review`. `tehas api
  --json` lists `review` in `flags` as `false`, `install.sh` no longer
  installs `tehas-review.service` or `tehas-review.timer`, and `tehasbot
  review` exits 0 at once. `TEHAS_REVIEW` and `TEHAS_REVIEW_REPO` stay on the
  frozen surface with their meaning. `tehas update --check` lists every line
  of a building feature an instance sets, not only the first. While review is
  off, an update stops and disables `tehas-review.timer` where an earlier
  release left it enabled, and keeps its unit files (#2488).
- A plan written onto its issue whose edit timed out or got a 5xx is no longer taken as written when the plan kept the issue's title, or when the issue's text starts `Replaces #`. The next poll writes it again unless the issue already holds the plan's text. A cycle's note in the body of an issue that a refine filed is no longer taken as written when the edit did not land (#2542).
- A start from `tehas start N` that gives up before its cycle runs, on a GitHub error or a busy store, keeps the host's approval, so the next poll tries again. Before, the approval was deleted and the issue never started. When the cycle's unit does not come up, the comment now says to run `tehas start N` again, not to apply `tehas:ready` (#2543).
- When GitHub times out or answers 5xx on the `tehas:running` mark of a start, the poll reads the issue once. A mark that was made starts the cycle. Before, the start was undone and the next poll parked the issue with `needs-human-attention` (#2544).
- When GitHub fails the read of the control issue's labels, `tehasbot status` takes `tehas:pause` from the issue's events, and when that read fails too it keeps the pause switches of the snapshot before and names `pause` in `stale`. A paused factory is no longer shown as taking work, or as paused by a lift nobody made (#2545).
- MCP `factory_status` leaves out `hold` and `held` when the poll's report is more than ten minutes older than the health file, as a paused poll does not rewrite it (#2546).
- A poll that cannot list `~/tehas/state/cycles` now ends with exit 1 before it acts. It used to read that as no cycle running: it removed the containers of live cycles, parked their issues and could start past the limit (#2547).
- A cycle killed before it reported (a power loss, the OOM killer) no longer reads as running. `tehas status` says `killed before it reported: tehas stop N ends its record` for such a host start, and `tehas state pack` and `load` refuse with that reason in place of `a cycle is running for #N`. The store's `approval-hosts` gives each cycle its lock's state (#2548).
- When `install.sh` stops before it installs anything after a release update switched `~/tehas/current`, `tehas update` points current and the sandbox uid tag back where they were, so a plain `tehas update` reinstalls the version the host ran (#2551).
- A host that runs from its platform checkout at a tag newer than the newest release, such as a prerelease, moves to the release of that tag with `tehas update` instead of back to the newest. An unpublished tag changes nothing (#2552).
- `sandbox/build.sh` now says that the uid tag names the platform image without the instance layer when the layer of `sandbox.Dockerfile` fails to build or fails its smoke run, instead of saying the tag was left as it was (#2553).
- Going back to a release whose sandbox image is still tagged no longer puts that image back when it was built from another `sandbox.Dockerfile` than the instance has now; the sandbox build the update owes runs as on any other update (#2554).
- A classic retry of a parked issue now takes `needs-human-attention` off on the host before its runner starts, so a pull request that waits only for a merge gets `tehas:awaiting-merge` instead of being parked again (#2556).
- A start that `tehas work` refused with exit 4 is no longer parked as an image or file that could not be given. The park lists the possible causes and points to the comment that names the real one, and `tehas status` says the same (#2557).
- Building the sandbox image survives a flaky apt mirror: each apt step retries its fetches, and when it still fails it runs once more after 30 seconds. CI, the release gate and the release workflow retry a tool download whose transfer fails, and still check it against its pinned sum.
- 1.0 is the core factory. outsource, spec-pipe, protolab-zip-url, prototype-file, lean and review are planned for 1.1 and stay off in 1.0. The contract lists the settings, commands, fields and labels each adds in 1.1, under the same names.
- A settings load refused for a setting tehas 1.0 renamed or removed, or for a line of a feature still being built, now lists every such line of `instance.env` and `~/.config/tehas/env` at once. An update from 0.30.x shows them all in one run instead of one per run.
- `TEHAS_OVERLAP_HOLD` is a row of `api/renames-1.0.tsv`. `TEHAS_OVERLAP_HOLD=0`, the old default, now loads and is dropped, as a blank value is; any other value still stops every command with 78.
- A malformed `TEHAS_ENERGY` or `TEHAS_MCP` in `instance.env` now names `instance.env` in its message, not `~/.config/tehas/env`.
- The poll reads the disk floor as the settings load does. A `TEHAS_CYCLE_MIN_FREE` such as `05g` no longer switches the floor off; a value it cannot read stops the poll with 78.
- The manual says that the ceilings of `TEHAS_WORK_TIMEOUT` (25200) and `TEHAS_PLAN_TIMEOUT` (7200) are checked by the load only, not by `console settings valid`, and no longer names a `low-gate` rule.
- `tehas start`, `limit`, `order`, `panel`, `host-message`, `freeze`, `thaw`, `pause` and `drain` stop with 78 before they write anything or call GitHub when `TEHAS_REPO`, `TEHAS_CONTROL_ISSUE` or `TEHAS_ALLOWED_LABELERS` is missing or malformed, as their exit tables say.
- A command whose reader goes away exits 141 in every case now: `tehas top --once`, `tehas top --json`, `tehas check-product`, `tehas init-product` and the help texts did not.
- `tehas refresh` exits 78 when `tehas-dispatch` is not installed or cannot run, where it exited 127 or 126.
- `tehas order` exits 78 when the state store is not installed or cannot be used, as `tehas history` does, where it exited 1. Its exit table names the store, not a file.
- `api/order.schema.json` lists `unreadable` among the states of `tehas order --json`, with a sample.
- `tehas order set --json` and `tehas order clear --json` are in the help text and on the public surface.
- `tehas mcp key` and `tehas mcp serve` refuse any other word with 64. `tehas mcp key --bogus-flag` printed a key.
- The MCP tool `factory_disk` now serves the lines of `docker system df` under `full.docker.lines`, masked, as its schema and the manual said.
- `api/disk-cycles.schema.json` now fits what `tehas disk --cycles --full --json` prints: `cut` is left out for a place measured whole, and a container with no command has no `command`, as in `api/disk.schema.json`. `api/disk-cycles.full.json` is a new sample.
- Every MCP tool that takes an `issue` takes 1 to 999999999 and says so in its input schema; a larger number is refused with one message.
- The manual says how to read on after a `factory_search` answer with `next`: `factory_log` from `next.offset`.
- The contract lists `runs.contractor_id` of analytics.db as frozen, says the keys of `rows` in `tehas state check --json` are internal, and says `testFeatures` in `tehas status --json` stays through 1.x.
- `install.sh` writes the store's backup copies, and removes them again, before it copies the first file. A `state/backup` that is a link out of the home, or a disk with no room for the copies, now stops the install with "nothing was installed", and `tehas update` puts `current` back, instead of leaving a new store that refuses the old database.
- `tehas update` writes `state/release-history` before it moves `current`, so a history it cannot write leaves `current` where it was and the message saying so is true.
- A cycle that waits on its issue's lock while `tehas stop` removes the lock file now takes the lock on the new file, where the dispatcher and the cleaner look.
- The state store checks the whole database once a poll (`poll-begin`) and in `upkeep`, `prune`, `migrate` and `check`, not on every heartbeat and record. On an old factory this saves a full read of the database every 10 seconds per running cycle.
- `store upkeep` also cuts `logs/preview.log`, `logs/protolab.log` and `logs/clean.log` past 64 MiB, as it does the dispatcher's log.
- `install.sh` no longer keeps `libexec/github-token` while a cycle runs; no cycle from before v0.30 can still run.
- The dispatcher judges every edit of an issue's body after the approval, not only the last. An edit by an account outside `TEHAS_ALLOWED_LABELERS` now holds the issue even when an allowed account saved the body after it, and so does an edit history longer than the 100 edits the poll reads. The poll reads that history only for edited issues it may start. This is a security fix.
- A GitHub write whose connection broke after it was made (a reset, an EOF, a GOAWAY, a closed connection) is read back before it is sent again, as a timeout or a 5xx already was. Before, a comment could be posted twice.
- `tehasbot status` reads the list of every issue for the breakroom's words at most once an hour, and keeps the words between reads. Each status run was about ten GitHub requests heavier before.
- Security fix: when `TEHAS_ALLOWED_LABELERS` names a GitHub account called `unknown`, a label application the pins refuse, an edit and a title change by no account no longer count as that account's. They still show as `unknown` in the log. The account's own labels count as before (#2449).
- Security fix: when the millwright's app is called `unknown`, a `tehas:ready` or `tehas:plan` the pins refuse, or one by no account, no longer counts as the millwright's. It is taken off as any unallowed label is. The millwright's own labels count as before (#2449).
- Security fix: a label taken off after it was applied no longer approves anything, even when the poll's list of the issue's labels still has it. The poll logs `skip #<n>: tehas:ready was taken off by '<who>' since it was applied` and changes nothing on the issue. The same holds for `tehas:finish`, the driver labels and `tehas:prototype` (#2449).
- Security fix: when `TEHAS_ALLOWED_LABELERS` names a GitHub account called `unknown`, an edit in an issue's edit history that names no account holds the start after an approval, as the last edit does (#2449).
- On a read-only instance (`TEHAS_READ_ONLY=on`), the GitHub token a cycle's
  container, a planning turn or a manual session is given reads only: Contents,
  Pull requests and Issues read. Before, it could write, and only the image's
  `gh` and the container's git configuration kept a cycle from writing. A token
  kept from before the instance turned read-only is made again. `tehas update`
  asks a read-only instance's cycle token for no push and says
  `Cycle token:  OK (read-only: ...)`.
- A release's page on GitHub no longer holds every change since the last release. A release `vX.Y.Z` is published with its own section of `CHANGELOG.md`, and the release workflow publishes nothing when it has none, where it used to take the whole of `Unreleased`. A prerelease without a section of its own is published with what is new since the tag before it, under a first line `New since <tag>.`. `scripts/release-notes.sh` runs `xtask release-notes`.
- `scripts/tag-release.sh` names a release's section itself: for a release whose head has no `## vX.Y.Z` section, its changelog pull request (`changelog-<tag>`) renames `## Unreleased` to `## vX.Y.Z (YYYY-MM-DD)` with a new empty `## Unreleased` above it, with or without changelog fragments. A prerelease whose tag before has no `Unreleased` section is not tagged.
- `CHANGELOG.md` has the sections of v0.31.0 and v0.32.0, which were under `Unreleased`, and `Unreleased` holds only the changes since v0.32.0.
- `tehas check-product --sandbox`, which `tehas setup` runs, stops the product's gates after `TEHAS_VERIFY_TIMEOUT` seconds (1800 by default): the container is killed and the check fails with `the gates did not end within <n>s`, so a gate that hangs no longer holds `tehas setup` or an upgrade forever (#2536).
- The contractor's own tests end a run of the program that takes more than a minute and fail, instead of waiting on it forever (#2536).
- Security fix: when the millwright's start, finish or refresh rests on an approval of `tehas start`, an edit or a title change made in the same second by a GitHub account called `host` now holds it, as a change by any outside account does (#2571).
- A ticked Retry on a question no longer waits with `skip #<n>: its text was changed by 'unknown' after <who> ticked retry` when only allowed accounts edited the issue after the tick. The poll now reads the edit history of the edited issues with a question too.
- A text file unpacked from a zip the issue links is held to 1 MB, as a linked one is, and is left out with "Over the 1 MB limit of a file." when it is larger.
- An image unpacked from a zip is left out with "Over the 10 MB limit of an issue." when it is larger than what is left of the issue's 10 MB of images.
- Security fix: `tehas:blocked` is now a pinned label, like `tehas:ready`. A lone `tehas:blocked` starts an issue once its blockers close, so another label renamed to `tehas:blocked` no longer brings its old applications with it as approvals. `tehas labels` pins it. A `tehas:blocked` the pins refuse starts nothing but still holds the issue it is on.
- `tehas history`, `tehas export` and `tehas state pack` exit 141 with nothing on standard error when their reader goes away, as every other command does. They exited 1, which `tehas history` documents as a busy state store.
- The exit tables of `tehas limit`, `tehas pause` and `tehas drain`, and their help texts, list status 1 for a limit or pause flag that cannot be written. The tables of `tehas refresh` and `tehas dispatch --soon` name the request or file that could not be written.
- `tehas limit set` refuses `--json` with status 64. It took the flag, which was never documented, only when another word followed it. Read the limit back with `tehas limit --json`.
- Security fix: every container tehas starts from the sandbox image now has `--pull=never`. When the local image is gone, for example after `docker system prune -a`, the start stops with `No such image` instead of pulling an image of that name from Docker Hub and giving it the cycle's token and model keys. Build the image again with `~/tehas/sandbox/build.sh`.
- A push check that Docker could not start (exit 125, 126 or 127, or a line of Docker's own) no longer records a push fault or tells the issue the sandbox cannot push. The start stops with 78 and says why, as the poll's probe already did.
- With `TEHAS_EGRESS=on`, each Docker call that sets up the egress proxy ends within 60 seconds, and a start waits at most 15 minutes for `state/egress.lock`. A dockerd that stops answering no longer holds a start and its issue's lock for good.
- The token renewer of a `tehas work` that was killed outright ends within a second, so it no longer keeps the issue's lock and a later `tehas work` of that issue can start.
- The installer and `tehas update` on an aarch64 or arm64 machine now stop at once with "this machine has no tehas release", instead of failing to download a tarball that was never built. Releases are built for x86_64 only, and the manual now says so.
- `tehas state copy` and `tehas state load` examples and samples show the schemas 1.0 prints, and the manual lists the refusal of a copy older than v0.30.0.
- The manual says that a freeze moves only tehas's own time limit: the runner's deadline and the cycle unit's 8-hour stop still count while a cycle is frozen.
- llms.txt and the analytics pages no longer describe the Pi RPC path, which is gone.
- `install.sh` stops with exit 78 before it copies anything when the instance's `sandbox.Dockerfile` cannot be read. It used to copy the new programs first and then say nothing was installed, so `tehas update` put the old version back over half-installed programs.
- `install.sh` refuses an argument it does not take with exit 64 and a usage line. A mistyped `--check`, such as `--chek` or `--tests --check`, used to run a real install.
- The manual says what the store's dry run before the first copy does: it takes the write lock for a moment and needs room in `state/backup` for up to two copies of the database.
- A changelog fragment whose Adopter action is only "None." no longer adds a stray "None." after a release's real Adopter action steps. It still gives "None." to a section that has no Adopter action.
- On a host that runs a prerelease newer than the newest release, such as `v1.0.0-rc.2` while the newest release is `v0.32.0`, `tehas update --check` no longer names the older release as the newest or says `tehas update --to` moves the host back. It says the host is on a prerelease ahead of the newest release, and names the newest prerelease of the same version, such as `v1.0.0-rc.3`, when there is a newer one for `tehas update --to` to move to.

### Adopter action

Before updating from 0.30.x to 1.0, edit the renamed and removed settings in
`instance.env` and `~/.config/tehas/env`, then run `tehas drain --wait 3600`
until no cycle runs. Then run `tehas update`, `tehas labels` and
`tehas limit clear`. While a setting is left to fix, `tehas update` stops with
78, names it and switches nothing. The manual's section "Updating from 0.30.x
to 1.0" in `operating.md` lists every name and what to do for each.

A program that reads `api/renames-1.0.tsv` picks up the new rows. Take `cross-project` off open issues. Take any `TEHAS_OUTSOURCE_*` or `TEHAS_PROTOLAB_ZIP_HOSTS` line out of `instance.env`.

An instance whose public site should keep showing the live console sets `TEHAS_SITE_CONSOLE=on` in `instance.env`, then runs the docs server's `docker compose ... up -d` line again.

If a command logs `its Docker daemon counts <n>`, restart the Docker daemon when no cycle runs (`sudo systemctl restart docker`) so containers get the host's CPUs.

Rebuild the sandbox image (`install.sh` rebuilds it) for the fixes to reach cycles.

On a host with a `sandbox.Dockerfile`, the first `tehas update` or `tehas setup` after updating rebuilds the image once, because the image there has no `tehas.layer` label yet.

On a host with a `sandbox.Dockerfile` whose image was built before the `tehas.layer` label, health says `warn` until `tehas update` rebuilds the image.

A host that set `TEHAS_REVIEW=on` (or `1`) or `TEHAS_REVIEW_REPO` in
`instance.env` takes those lines out before it updates. Otherwise the update
stops with exit 78 and names them. Nothing to do for an enabled
`tehas-review.timer`: the update disables it.

A host that already has `~/tehas/state/labels` has no pin for `tehas:blocked`. Until it has one, a lone `tehas:blocked` starts nothing, and the poll logs `refused: the label tehas:blocked (<id>) has no pin`. Run `tehas labels`: it names the label's id and exits 1. Check on GitHub that the label called `tehas:blocked` is the one tehas made, then add the line `tehas:blocked<tab><id>` to `~/tehas/state/labels` as the command says. A host with no such file pins it on its own.

## v0.32.0 (2026-11-17)

### Changes

- A cycle's workspace holds only recent history: the default branch alone and
  `TEHAS_CLONE_DEPTH` commits of it, 100 by default, where it held every branch
  of the product checkout with its full history. The clone stays local, so
  GitHub traffic does not change; the host no longer packs and copies the
  product's whole history for every cycle. Planning turns, reviews, the code
  index and `tehas check-product` clone the same way; a later planning turn
  and a review deepen theirs to the commit they start from. A refresh deepens its
  workspace until it holds the commit the pull request's branch started from,
  so pi-rukas's ancestry checks answer as in a full clone, and stops with 78
  and the reason when it cannot. `tehas finish` takes a shallow workspace. A
  value that is not a whole number of 0 or more stops every command with 78
  (#2359).
- The manual says a repository with no `.pi/work.json` has no `verify.test` for lean, so a spec with a `test` criterion ends not ready, and the features page records the first lean cycle seen on a factory: the not-ready path, end to end.
- The poll ends a sandbox push fault on its own. Once
  `state/push-fault.json` is ten minutes old, the poll runs the dry-run push
  a start runs, with the App's cycle token and no cycle spent on it, in a
  fresh copy of the checkout as a start's workspace is. It asks only while
  the poll could start a cycle. A pass
  takes the file off and the queue starts; a refusal writes the fault fresh
  and holds starts for another ten minutes. When the poll cannot ask at all,
  it records nothing and the next start's own check decides, as before
  (#2130).
- While the fault stands, `tehas status` prints
  `push      the sandbox cannot push: <git's line> (<login>)`,
  `tehasbot health` fails with it, and `tehas status --json` carries an
  optional `pushFault` object (#2130).
- `tehas update` and `install.sh --check` prove that the GitHub App's cycle token can push, with a dry-run push from the product checkout. When it cannot, they print `Cycle token:  CANNOT PUSH` with the app's login, git's message and what the app needs, and exit 1; a drained factory stays drained (#1740).
- A new optional feature, `spec-pipe` (`TEHAS_SPEC_PIPE`), is registered and still being built: a release refuses it set to `on` with exit 78, and `tehas api --json` lists it in `flags` as `false`. With it on, an issue names its spec with one line of its body, `Spec: <link> sha256:<64 hex digits>`, the link a file attached on GitHub; a line tehas cannot read starts nothing and the poll says why once.
- With the `spec-pipe` feature on (still being built, so a release refuses it), a start of an issue with a `Spec:` line fetches the file and holds its bytes to the sha256 the line names, and the start's approval keeps the link and that sha256. Bytes of another sha256 or a link GitHub refuses with a 4xx start nothing: the label comes off, or the host's approval, outage retry or refresh request that asked is used up, and the issue says why once. A 5xx or a network failure changes nothing and the next poll tries again.
- With the spec pipe on (still being built, so a release refuses it), a cycle is given the spec its approval pinned, fetched again and held to that sha256, and the approved title and body as `approved.json`, both named in the manifest of the issue's directory under `spec` and `approvedText`. A cycle with a spec starts only on a sandbox image whose pi-rukas lists `issueSpec`.
- The manual has a front-end page, `/frontend`: what the dashboard and the front pages may use, how their parts are named, the island contract, the styling rules, the kit and the byte budgets.
- A workspace test holds each page and island under its line in `host/fun/pages.max`, set at today's sizes. A page that grows fails the gate until its line is raised in the same change. Nothing on screen changes.
- `tehas update` reads releases from the repository built into tehas, or the
  one `TEHAS_RELEASE_REPO` in `instance.env` names, and checks their signature
  with the key built into tehas. It no longer reads the repository from
  `~/tehas/versions.env` or the key from `~/tehas/release/minisign.pub`, both
  of which a release writes. An instance that names another repository with
  `TEHAS_RELEASE_REPO` still checks with the built-in key.
- `tehas update` asks the console whether it can run the update by its exit
  status as well as its answer, so a console that answers and fails is not
  asked to update.
- A shell test runs the update on a home a v0.30.x install left
  (`test/update_from_030_test.sh`). [Update](/tools-update) says
  how a v0.30.x host moves to 1.0.
- A new manual page, the contract, says what is public and stays compatible through 1.x and what is internal: the settings and the knob rule, the cycle inputs, analytics.db at schema 4, the pull request spec block, what a feature still being built leaves visible, and one table of exit statuses per public command.
- Every public command gives an exit status one meaning: 1 is a failure, 3 is "the command worked and the answer is no or not yet", 78 is settings only.
- `tehas status` exits 3, not 1, on `fail`, a stale health file or no health file yet.
- `tehas drain` exits 3, not 1, when cycles still run at the end of the wait.
- `tehas update --check` exits 3, not 1, when there is something to update. It still exits 1 when it finds something an update does not fix, or when the sandbox build's own check fails. `sandbox/build.sh --check` exits 3, not 1, when the pi-rukas commit moved.
- `tehas start` exits 3, not 1, when GitHub has no open issue of that number. GitHub not answering is still 1.
- `tehas check-product` exits 3, not 78, when a check fails.
- `tehas setup` exits 1 when a step fails, and 78 only when the step stopped on its settings. It no longer passes on a step's other statuses.
- `tehas analytics` exits 78, not 69, when analytics is off, and 1, not 69, 70 or 73, when the program is not installed, the database cannot be read or the file cannot be made.
- The forge page describes an internal trait: tehas 1.0 promises no forge interface.
- `api/renames-1.0.tsv` lists each changed exit status as a row of kind `value`.
- The MCP tool `factory_log` reads a running cycle's logs: give `issue` instead of `id`, or the running cycle's id, `work-<issue>-<startedAt>`, which names it only while it runs. Before, a cycle's logs could be read only after it ended.
- The MCP tools `factory_sessions`, `factory_session` and `factory_failures` check each answer against the schema they publish. `factory_sessions` leaves out a session whose snapshot line has a field of another type or a word outside its list, and counts it in `leftOut`, for that page; `factory_session` answers such a session with an error that names the field. Their schemas list the words of `outcome`, `kind` and `how`.
- `factory_failures` counts in `outcomes` only the seven outcome words. A run with any other word, like a run with none, counts in `outcomesUnknown`.
- `factory_park` gives `latestOutcome` when a later work run of the issue ended without a park, such as `stopped` for a run interrupted or whose issue closed under it, so the park it answers is not taken for the issue's last word.
- `factory_outcomes` marks a week entry with no readable week `unwindowed: true` when `since` or `until` is given: it is kept, as no window can bound it (#2244).
- `tehas top --json` is named as not a public object: it is the dashboard's snapshot and may change in any release (#2241).
- The dispatcher's poll reads `TEHAS_PLAN_TIMEOUT` and `TEHAS_PLAN_MAX_TURNS`
  by the rules the settings load uses. A value it cannot read stops the poll
  with exit 78 before the lock and before any call, instead of becoming the
  default (#2233).
- A limit file whose `cycles` is not plain digits, such as `1.0` or `01`,
  reads as `unreadable` everywhere. Before, the status said `ok` while the
  dispatcher read it as 0 cycles and started nothing.
- `TEHAS_EGRESS_HOSTS` has one rule for the settings check and the proxy's
  list. A word the check refuses, such as `a_b.example.com` or `x.org:0443`,
  can no longer reach the proxy by another reading.
- `PI_ENSEMBLE_HOST_ALIASES` is a ruled setting: each comma-separated entry
  must be `name:address` or `name=address`, the address an IPv4 or IPv6
  address (bare or in brackets) or `host-gateway`. Any other value stops the
  load with exit 78, and no container is started with it.
- `tehas disk` and `tehas setup` refuse a value they read from a settings file
  that a shell would expand (a `$` or a backtick outside single quotes), with a
  line that names the key and not its value. Before, they took it as written
  while the shell that sources the same file expanded it. For `tehas setup`
  this covers the model key and the instance values it reuses.
- `TEHAS_WORK_REPO` is a ruled setting: an absolute path, else the load stops
  with exit 78.
- The host's own settings have rules too, listed under `host` in
  `api/settings-rules.json` and in a Host settings table of the manual:
  `TEHAS_HOME`, `TEHAS_INSTANCE_DIR`, `TEHAS_MODEL_KEY_<NAME>`, `TEHAS_MCP`,
  `TEHAS_MCP_KEYS`, `TEHAS_MCP_DASHBOARD_KEYS`, `TEHAS_ENERGY`,
  `TEHAS_ENERGY_URL` and `TEHAS_UPDATE_LOCK_WAIT`. A value its rule refuses
  stops every command with exit 78, with a line that never shows the value.
  `TEHAS_UPDATE_LOCK_WAIT` outside 0 to 300 now stops `tehas update` and
  `install.sh` with 78 instead of becoming 10.
- Every other `TEHAS_*` name tehas reads is internal and listed with why, and
  a test holds new names to a rule or to that list.
- A cycle input that the image's pi-rukas does not list no longer stops the cycle, or a weekly review, when a polyfill the image carries declares it. The polyfill is named in the image label `tehas.rukas.polyfills` and lists its inputs in the label `tehas.rukas.polyfill.<name>.inputs`. A name the runner lists in any scope is never opened this way.
- Outsource, still being built and refused in a release, gains its commands
  and its contractor. `tehas outsource invite|list|status|revoke|drain`
  manage the contractors of a factory. The model proxy's listener also
  answers the job channel: a contractor joins with a one-time code, takes the
  job in its hand, checks in, renews the cycle's token, relays its issue
  writes, which the factory checks as the local relay does and makes with its
  own token, and posts the result. `tehas-contractor join|run|drain|doctor`
  runs a job with the same `tehas work` and container as the factory, and
  stops its cycle in an orderly way when it loses the factory: the stop
  signal, the grace, a push of the branch while the job is current, a report,
  and a wipe of the job's files. A cycle on a contractor gets exactly the
  mounts and variables of a cycle at home. With outsource off nothing changes
  (#2274).
- A new setting, `TEHAS_OUTSOURCE_CONTRACTORS`, lists the contractors the job
  channel answers. Empty, the default, refuses every contractor (#2274).
- Outsource, still being built, gains placement in the dispatcher. With the
  feature on, a start from the ready queue goes to a contractor only when its
  repository is in `TEHAS_OUTSOURCE_REPOS`, the issue has lost fewer than
  `TEHAS_OUTSOURCE_RETRIES` jobs, and a contractor fits: heard from within
  `TEHAS_OUTSOURCE_TIMEOUT`, not revoked, draining, resting or busy, with this
  host's image and the room a cycle needs. The one with the most room wins.
  Anything else, or a read that fails, runs the cycle in-house. The start's
  line in `dispatch.log` says where it runs. A release still refuses
  `TEHAS_OUTSOURCE=on`, so nothing changes on a factory (#2275).
- With outsource on, the factory stamps who built each cycle: the start comment, the report's comments and the pull request's body end with `Built by contractor: <name> (job <id>)` or `Built by: in-house`, and each commit a contractor's cycle adds to its pull request carries a `Built-by:` trailer, which the factory rewrites when it is missing or wrong. Commits on the branch from before the cycle are left as they are. Outsource is still being built, so no instance writes the stamp yet (tehas#2276).
- `state/analytics.db` gains `runs.contractor_id`, the contractor that ran a cycle, inside schema 4: a file written by 0.30.x gains the column, empty, the first time it is opened, and `tehas analytics copy` carries it. It stays empty until outsource ships (tehas#2276).
- With outsource on, the contractors show where a person and a program look: `tehasbot status` writes `live/contractors.json`, `tehas status` prints an `outsource:` line and says of each running cycle whether it is outsourced to a contractor or in-house, `tehas status --json` has an optional `contractors` object (with a new sample, `api/status.outsourced.json`), and the MCP server has `factory_contractors`, served to dashboard keys too. With outsource off nothing changes: no file, no field, no line and no tool. Outsource is still being built, so no factory has it on yet (#2277).
- The Protolab can take a zip named by address and sha256, `Zip: <address> sha256:<hex>`, from the hosts of the new `TEHAS_PROTOLAB_ZIP_HOSTS`, empty by default. Bytes of another sha256 are refused, redirects are followed only on those hosts, and the zip is held to the limits of an attached one. The feature `protolab-zip-url` is still being built: `TEHAS_PROTOLAB_ZIP_URL=on` stops every command with 78, and `tehas api --json` lists it in `flags` as `false`.
- `tehas prototype file` opens a ticket for the Protolab with the ticket text, a `Zip:` line and a `Spec: <link> sha256:<hex>` line, and labels it `tehas:prototype`, all with the caller's own gh login; a gh signed in as the bot, or a login off `TEHAS_ALLOWED_LABELERS`, is refused with exit 3. Still being built: `TEHAS_PROTOTYPE_FILE=on` is refused with exit 78 and `flags` lists `prototype-file` as `false`.
- The Protolab takes a file only from text an account in `TEHAS_ALLOWED_LABELERS` wrote and nobody off the list edited last, never from a quoted line. Being an owner, member or collaborator of the repository is no longer enough, and the bot's own comments never count.
- A ticket that changed with no new link and no new title, such as after a plain comment, fetches no file again: its page only takes the new time.
- The lean driver is a feature still being built, `lean` in
  `tehas api --json` `flags`, switched by the new setting `TEHAS_LEAN`. It
  stays off in this release: `TEHAS_LEAN=on` and `TEHAS_DRIVER=lean` stop
  every command with 78 and name the feature, and `tehas work --driver lean`
  exits 78. A start that would run lean (`tehas:lean`, or a retry, outage
  retry or refresh of a lean start) starts nothing: the poll takes the label
  that asked off and says once that lean is still being built. Classic never
  runs in its place. `tehas:classic` and an unlabelled start run classic as
  before. The labels stay in the label set.
- The state store is at schema 17. `install.sh` brings a store of any v0.30 release, or of 15 or 16, forward with every row; the new columns start empty. With outsource on, a job keeps the sha256 of its model proxy token (never the token), so a running job's token still works after the proxy restarts. A contractor's `hello` carries a report (sandbox image id, pi-rukas version, free memory and disk), the store keeps the last one, placement can fit a contractor whose report has the room, and `live/contractors.json`, `tehas status --json` and the MCP tool `factory_contractors` show it. With outsource off nothing writes these columns and status, the live files and the MCP catalog are as before. Outsource is still being built, so no factory has it on yet.

### Adopter action

Nothing to do for the new default. To keep full history in every workspace,
as before, set `TEHAS_CLONE_DEPTH=0` in `instance.env`.

An instance that sets `TEHAS_RELEASE_REPO` to a repository whose releases are
signed with another key can no longer update from it: take the line out to
update from tehas's own releases.

A script or a program that reads these exit statuses reads the new numbers: 3 where it read 1 from `tehas status`, `tehas drain`, `tehas update --check` and `tehas start`, 3 where it read 78 from `tehas check-product`, 78 or 1 where it read 69, 70 or 73 from `tehas analytics`, and 1 where it read 78 from a `tehas setup` whose product check failed.

If a command stops with exit 78 after the update:

- `PI_ENSEMBLE_HOST_ALIASES is not valid`: write each entry as
  `name:address`, such as `models.example:192.0.2.10`.

- `TEHAS_WORK_REPO is not valid`: give an absolute path of letters, digits
  and `._-`, with no slash at the end.

- `TEHAS_HOME is not valid` or `TEHAS_INSTANCE_DIR is not valid`: give an
  absolute path of letters, digits and `._-`, with no slash at the end.

- `<name> is not valid` for a `TEHAS_MODEL_KEY_<NAME>`, `TEHAS_MCP`,
  `TEHAS_MCP_KEYS`, `TEHAS_MCP_DASHBOARD_KEYS`, `TEHAS_ENERGY` or
  `TEHAS_ENERGY_URL`: fix that line of `~/.config/tehas/env`.

- `TEHAS_UPDATE_LOCK_WAIT is not valid`: give a whole number from 0 to 300,
  or unset it.

- `holds a $ or a backtick` from `tehas disk` or `tehas setup`: write the
  value itself in that file, such as `TEHAS_HOME` or `TEHAS_SCRATCH_DIR` in
  `instance.env`.

A limit file edited by hand with `cycles` such as `1.0` or `01` now holds
every start as `unreadable`: run `tehas limit set` to write it again.

Files linked by a member or collaborator who is not in `TEHAS_ALLOWED_LABELERS` leave the Protolab at the first pass after the update. Add the person to the list, or have an allowed account link the files.

An instance with `TEHAS_DRIVER=lean` in `instance.env` must take the line out
or set it to `classic` before updating, or every command stops with 78. Take
`tehas:lean` off any issue that should run, so it runs on classic.

## v0.31.0 (2026-11-17)

### Changes

- `scripts/verify.sh` says what a step did not check: the name lint with no
  `scripts/check-generic.names` reads `skip  the names in platform files
  (no names list)` instead of `ok`, and the shell step reads `skip` with the
  reasons its runner gave when the runner reported checks it did not run, as
  it does for a missing shellcheck. A step that checked everything still
  reads `ok`, and one that failed still reads `FAIL`.
- The pass the gate keeps per tree now knows what the run could see. The
  private names list is hashed as itself, which `git add -A` cannot do
  because git ignores the file, and so is `CI`: a pass made without the list
  is not replayed once the list is there, and a plain pass is not replayed
  under CI. `cargo fmt --version` and `cargo clippy --version` join the tools
  the key holds, so a pass made on a toolchain without rustfmt or clippy is
  not replayed on one that has them.
- The timed refresh scan is gone. `tehasbot refresh` runs only as
  `tehasbot refresh --pr N`, which asks for the refresh of one pull request;
  without `--pr N` it exits 64. `tehas-refresh.timer` and its service are no
  longer installed, and `install.sh` stops, disables and removes them on a host
  that has them. `TEHAS_REFRESH_CONFLICTS` and `TEHAS_REFRESH_EVERY` are no
  longer settings. `tehas api` no longer lists the features `refresh-conflicts`
  and `refresh-pr-sweep`, no longer has `refreshScan`, and its text no longer
  has the `refresh     scan` line (#1977).
- The dispatcher starts every refresh request it finds, with or without
  `"asked": "pr"`, after the same start checks. Nothing holds a request for a
  setting any more, so `refreshHold` names only an image that cannot refresh.
  The refresh lane keeps its room while a request waits or a refresh runs, and
  each container's CPU share and pids limit now count the lane's slots on every
  host, not only on one with the scan turned on (#1977).
- A planning turn that ends after a person closed its issue is dropped: the
  poll reads the issue's state before it posts, and a closed issue gets none of
  the turn — no plan, no boxes, no question — with its session closed and one
  line in the log saying so. A state that cannot be read holds the post to the
  next poll rather than closing the session on an answer GitHub did not give
  (#2214).
- The cycle end, the attachment fetch, the top view and the state move run their
  programs and reads under a bound. The cycle end reads only the tail of the run
  log for its diagnostics. The fetcher child, the `gh` reads of the issue, the
  `tehasbot top` publisher's child, the `stty`/`getconf` helpers of the view, and
  the `systemctl --user` and `tar` calls of `tehas state pack` and `unpack` each
  run within a time bound that stops them when they never answer, so one hung
  program no longer waits forever. What pack and unpack refuse, and the output
  and exit statuses of the answering programs, are unchanged (#2228).
- The importer of an older tehas's records no longer reads a value it cannot
  read as a plausible one. A `filed` entry that is not a string or not an issue
  URL, an `rc` or `turns` that is negative, an `options` list of more than the
  20 `plan-end` keeps, and a branch that is not a branch of that issue each hold
  the file with a line naming the value, as a record with a malformed `answers`
  list already did. Before, such an entry was dropped, a negative number became
  no number, a long list was cut, and a bad branch became no branch (#2234).
- `store journal-open` answers a row whose stored action arguments are not a
  JSON object with `"args_unreadable": true` and no `args`, so a reader cannot
  take an unreadable action for one that took no arguments. A readable row is
  unchanged (#2234).
- `analytics report` reads the counted columns of the `samples` table
  carefully, so one cell of the wrong type leaves its own figure out instead
  of ending the report. A REAL in `paused` is no pause, a cell that is not
  text in `held` names no hold, and a word in `in_line`, `planning` or
  `running` is no count: the sample is then not counted for the availability
  or the planned line that depends on it, and the report publishes. Before,
  such a cell ended the whole report: `analytics report` and
  `analytics report --json` both failed with an error and exited 70. The `at`
  and `watts` columns are read as they were, so a bad cell in either of them
  still ends the report (#2243).
- The git configuration a cycle's container is given now counts its own keys.
  `GIT_CONFIG_COUNT` is the number of `GIT_CONFIG_KEY_<i>` entries the container
  is given, built in one place, so a guard key added later cannot be dropped by
  git because its number was not raised. The variables a container is given are
  the same as before, in the same order (#2247).
- The two `tehas:` kind labels are spelled in one place. `Kind::label` is a
  `const fn` and the constants that name the improvement and prototype labels
  are built from it, so a rename reaches every command at once (#2250).
- The MCP server reads the header line of its two published snapshots,
  `sessions.jsonl` and `outcomes.jsonl`, under the crate's read bound. A file
  whose first line is longer than the bound is answered `could not be read`
  with no path and no content in it, instead of that line being buffered
  first. The bound is the header's alone: the entry lines of the same files
  are still read without one (#2252).
- An imported park whose own record gives no reason takes the reason of its
  cycle's history line through the list of end reasons, so a status the store
  does not know is recorded as `unknown` in the park as it is in the cycle
  (#2257).
- The planning polls write and read the outcome of a turn carefully. The
  document is written to a sibling of `state/plan-N.json.out` and renamed into
  place, and the sibling is now made new rather than opened over what already
  holds its name: a leftover of a turn that died between the write and the
  rename is taken away first, so the document a later poll reads is the one
  this keep wrote and not the leftover's bytes. A link or a directory under
  the sibling's name is left where it stands and the keep fails naming the
  file, since to follow it would write through a name the run does not own;
  the poll then says so and the next poll tries again. Reading the outcome is
  guarded the same way: the file is taken as a regular file under the size the
  console shares with its readers, so a link, a directory, or a file that grew
  past that bound is no outcome, which is the answer the poll already gives for
  a turn that left no words of its own (#2259).
- `tehas setup` rebuilds the sandbox image when it is not on the pi-rukas commit the host follows, so `check-product --sandbox` no longer gates in an older image (#2287).
- The manual pages carry structured data for search and answer engines: each page as an article with a breadcrumb, and the questions on the Pi software factory page as an FAQ. The front page names the licence, the author and the source repository (#2288).
- `/sitemap.xml` gives each page a `<lastmod>` date: when its file last changed on the host, and for the front page the last update to a new version (#2289).
- `/llms-full.txt` serves `llms.txt` and the whole manual as Markdown in one text, for agents that read a site in one fetch (#2290).
- `/favicon.ico` serves the site icon instead of a 404 (#2291).
- A call whose output could not be read (a failed or panicked pipe reader) is no longer taken for a program that printed nothing: the poll's line says the answer could not be read and the call is asked again, instead of a "garbled" store answer or a gh read settled with no retry. Output with a byte that is not UTF-8 is kept, with the byte replaced, rather than lost whole.
- When the state store is busy as a cycle's container starts, `tehas work` asks it once more a second later before it lets the cycle run without the image on the cycle's record, and its line now names the image id the container runs.
- A start no longer deletes the workspace kept beside it before the newer one is in its place, and says when it replaces one; it names what a killed cycle left (container id, kill counts, signal, an outlived container) before it clears it, and says when a log it writes cannot be written.
- A drained factory says what frees it. While `tehas drain` holds the factory,
  the dispatcher's hold line ends `; tehas limit clear starts the factory`, and
  `tehas update` that ended well on such a factory prints one line last naming
  the same command and saying this version has run no cycle yet. A limit that
  merely holds every slot, and one that expired, keep their words; the new line
  of an update changes no exit status, and `--check` and `--pending` print
  nothing new (#2229).
- `TEHAS_DRIVER` in the instance chooses the driver an approved start runs
  when no label chooses one: `classic`, the default, or `lean`. With `lean`,
  the new label `tehas:classic` from an account in `TEHAS_ALLOWED_LABELERS`
  runs that issue on classic, read from the label's events as `tehas:lean` is.
  An issue that carries both labels starts nothing, and the poll says why once.
  The park of a refused lean start under `TEHAS_DRIVER=lean` asks for
  `tehas:classic` instead of asking for `tehas:lean` to come off (#1099).
- A lean start takes `needs-human-attention` off with its other labels:
  `work-lean` takes no `--restart`, which is what clears it for classic.
- The front page title and heading say tehas is open source.
- A retry of a lean cycle starts it: `work-lean` is no longer given
  `--restart`, which it does not take, so the image's answer no longer refuses
  the start with exit 78 ("the flag `--restart`"). Every cycle is a fresh
  clone, so a lean run has no state to discard; classic restarts are
  unchanged.
- A refresh whose conflict check judged nothing no longer runs: when the read of
  the merge runs out of time, when git cannot be started, when the request's
  record holds no pull request or a head that is no commit, when git writes no
  tree object, or when it answers a status that is no answer, `tehas work` exits
  78, says which of them in the cycle log and writes no note, so no container
  starts. A merge that is clean runs as before, and so does one made by a git
  that has no such read of its own, with what git said in the log (#2227).
- A product's install, or the sandbox's push check, that cannot open its
  preflight log says that, with the path of the log and the fault of the system,
  and stops the start with 78. Neither ends as exit 127, with the lines of the
  step before it quoted as its own output, and the install's fault is no longer
  told as a sandbox that refused to push (#2227).
- `scripts/test.sh` names the paths it was given when none of them holds a test file or a script to lint, so a runner probe such as the lean driver's can see which path it refused. The exit status stays 3.
- Who may authorise work no longer depends on how GitHub spelled the account.
  One rule reads a login everywhere: gh's `app/<slug>`, REST's `<slug>[bot]`
  and GraphQL's `<slug>` are the same account when the poll weighs a start, a
  millwright's comment or the factory's own pull requests, and one rule reads
  `TEHAS_ALLOWED_LABELERS` wherever it is read, so a name is matched the same
  way by `tehasbot status`, `tehasbot pause` and `tehasbot protolab` as by the
  poll. A label event whose `created_at` is there but is no text is now read
  the strict way everywhere: the answer says it cannot be read and the poll
  holds the start, instead of reading the event as one with no time and
  ordering it ahead of the stamped ones (#2230).
- A factory can turn outsource on. With `TEHAS_OUTSOURCE=1` and the
  repositories of `TEHAS_OUTSOURCE_REPOS`, its state store holds the
  contractors, the jobs, each issue's outsourced attempts and the one-time
  invite codes, and the store answers the verbs that move a job through
  `offered`, `running`, `stopping`, `finished`, `released` and `lost`, with a
  row of history for every change. A job whose contractor goes quiet past
  `TEHAS_OUTSOURCE_TIMEOUT` is lost `TEHAS_OUTSOURCE_GRACE` seconds later, a
  contractor that lost one is given no new job for `TEHAS_OUTSOURCE_REST`
  seconds, and an issue that lost `TEHAS_OUTSOURCE_RETRIES` of them runs
  in-house. The state store moves to schema 15: the update lays the five tables in
  every store, empty. `store init --outsource` and `store migrate --outsource`
  turn the feature on in the store; without the flag every verb of it is
  refused and writes nothing. `tehas state load` moves the five tables with
  the rest of a copy and refuses a store that holds a job. Nothing sends a job to
  a contractor yet: no installed command calls these verbs, so turning the
  setting on changes nothing about what the factory does.
  [The settings table](/tools#instance-settings) and
  [Features](/features) say what is built.
- With `TEHAS_OUTSOURCE=1` the factory can run `tehas-proxy`, the model proxy
  of outsource: a contractor asks it for the model with its job's token, and
  the factory forwards to the endpoint it has configured. The real key never
  leaves the factory. The proxy reads the contractors and jobs from the state
  store, passes a streamed reply as it arrives, counts requests per contractor
  and per job, forwards only the model paths a cycle needs, and never logs a
  body. It listens on `TEHAS_OUTSOURCE_PROXY_ADDR` (`127.0.0.1:8090`), and
  `TEHAS_OUTSOURCE_PROXY_IDLE` (`900`) bounds a silent stream; it must be longer
  than `TEHAS_OUTSOURCE_TIMEOUT` or every command stops with 78. With
  outsource off nothing changes (#2273).
- A person's `tehas:ready` after a parked cycle and an automatic refine starts
  the issue again. GitHub can cut an issue's label timeline right after the
  label its last cycle started on, and the poll then read that old approval and
  skipped the issue on every poll with "its approval of ... started a cycle
  already". A candidate whose newest `tehas:ready` or `tehas:blocked` on the
  page is no newer than the approval of its last start now has its label events
  read whole.
- `tehasbot github-token --check` reads every page of the GitHub App's installation repositories, so an app installed on more than 100 repositories no longer reports that it does not reach `TEHAS_REPO`.
- The sandbox image no longer leaves a root-owned npm cache in `/home/vscode/.npm`, where npm in a cycle failed with `EACCES`. The image build stops if root owns anything there.
- A cycle's `manifest.json` starts with `"version": 1`, the format it is written in. A field is only added within a version.
- The attachment checker, the dispatcher and the Protolab read attachment links by one rule and fetch them by one fetch. Nothing else changes in what they take.
- `tehas setup --labellers` is now `--labelers`, spelled as the setting it
  writes, `TEHAS_ALLOWED_LABELERS`. The old spelling exits 64 and names the
  new one.
- `tehas setup --token-file` and `--app-installation` are gone. Both are now
  unknown options: each exits 64 and says why.
- `tehas analytics export <file>` is now `tehas analytics copy --to <file>`,
  with the same copy and the same `--json` object. The old form exits 64 and
  names the new one.
- `tehas pause` takes its reason as `--reason TEXT`, as `tehas freeze` and
  `tehas limit set` do. A bare reason exits 64 and names the flag.
- `tehas update` keeps no internal exit status 99. Its console verbs `args`,
  `gate` and `prepare`, used only by the updater of releases before 0.25, are
  gone; 1.0 updates from 0.30.x.
- `tehas status --json`: where a running cycle's step is moves into one object, `running[].progress` (`step`, `stepNumber`, `stepTotal`, `round`, `roundLimit`, `stepSince`). It is driver-specific and may change in a minor release. The schema now names `readOnly`, which the object already printed, and a third sample, `api/status.readonly.json`, shows it.
- `tehas history --json`: the list is `runs`, not `cycles`: it holds cycles and planning turns. Where the step a cycle ended in was moves into `runs[].progress` (`step`, `stepNumber`, `stepTotal`, `round`), driver-specific like the one of status. `tehas export --json` prints `runs` too, and each `history.json` of an export directory carries `progress`.
- `tehas export` writes `export.json` at the top of the export directory, `{"version": 1}`, the layout of the directory, and `tehas export --json` gains the same `version`.
- `tehas start --json`: the approval's sums are `bodySha256` and `titleSha256`, camelCase like every other `--json` object.
- `tehas refresh --pr N --json` (and its old name under `tehasbot`): the head is `headSha`, and the object gains `"api": 1`, as every other `--json` object has.
- `tehas disk --json` and `live/disk.json`: `generatedAt` is now `time`, the word `tehas status --json` uses for when an answer was written. The MCP tool `factory_disk` answers `time` too.
- The MCP tool `factory_disk` serves no path of the host: `dockerRoot` is gone, a file system's `paths` are roles (`home`, `docker`, `tmp`, `scratch`), and a place's `path` is its path under the instance's home, or its name alone outside it. A file system's `why` names a path by its role, and a `mount` that is a device path or a network export is left out. `tehas disk --json` and `live/disk.json` keep the absolute paths for a person on the host.
- Nine settings have new names: `TEHAS_INDEXING` is `TEHAS_SITE_INDEXING`, `TEHAS_ANALYTICS_SRC` is `TEHAS_SITE_ANALYTICS_SRC`, `TEHAS_ANALYTICS_SITE` is `TEHAS_SITE_ANALYTICS_NAME`, `TEHAS_ANALYTICS_COOKIE` is `TEHAS_SITE_ANALYTICS_COOKIE`, `TEHAS_CONCURRENCY` is `TEHAS_WORKSTREAMS`, `TEHAS_TICKET_CONCURRENCY` is `TEHAS_MAX_CYCLES`, `TEHAS_REFRESH_MAX` is `TEHAS_REFRESH_MAX_PER_PR`, `TEHAS_OWNER` is `TEHAS_ASSIGNEE`, and `TEHAS_INDUSTRY_VERSION` in `~/.config/tehas/env` is `TEHAS_MANAGER_VERSION`. The values are the same. The old manager name is ignored rather than refused, so a factory whose manager still writes it keeps running, without the version in its health.
- Auto sizing is removed: `TEHAS_CONTAINER_MEMORY`, `TEHAS_CYCLE_CPUS` and `TEHAS_CYCLE_CEILING` are gone, `auto` is refused for `TEHAS_MAX_CYCLES` and `TEHAS_SLICE_CONTAINERS`, and `tehas status --json` no longer has `room.slots`.
- `TEHAS_BOT_LOGIN`, `TEHAS_MILLWRIGHT_LOGIN` and `TEHAS_GITHUB_APP_INSTALLATION` were ignored with a warning and are now refused: the GitHub App gives both logins and the installation is looked up.
- Every setting row of `api/renames-1.0.tsv` still set in `instance.env` or `~/.config/tehas/env` stops every command with 78 and a line that names the new key or why the old one went. `tehas update` and `tehas update --check` list every such line before anything is installed and install nothing until the instance is clean.
- The health file's `checks.industry` is `checks.manager`, the MCP tool `factory_status` serves `manager` in place of `industry`, and the front page footer says `manager vX.Y.Z`.
- Every switch setting takes `on`, `off`, `1` and `0`; the manual and the
  example instance write `on` and `off`. `1` and `0` keep working, and
  `TEHAS_ANALYTICS` now takes them too. Any other value stops every command
  with exit 78, as before.
- `TEHAS_ENERGY` is a switch like the others: `on`, `off`, `1` or `0`. Any
  other value stops every command with exit 78.
- Optional features are named in one registry, each either shipped or still
  being built. A feature still being built cannot be switched on: its switch
  set to `on` stops every command with exit 78 and names the feature. In this
  release `outsource` is still being built, so `TEHAS_OUTSOURCE=on` (or `1`)
  is refused and `tehas-proxy` stays off.
- `tehas api --json` has a new `flags` object, every optional feature and
  whether it is on here (`{"outsource": false}` today), and the word `flags`
  in `features`. `tehas api` prints a `flags` line.
- `tehas api` feature words: `store-copy` is now `state-copy`;
  `refresh-pr-starts`, `refresh-pr-lane` and `refresh-pr-json` are folded
  into `refresh-pr`; `host-message-health` is folded into `host-message`.
  `api/renames-1.0.tsv` lists each.
- `tehas refresh --pr N [--json]` is the public way to ask for the refresh of one pull request. `tehasbot refresh --pr N` stays as an alias through 1.x and prints the same words and object (#2009).
- A refresh finds the factory's own `gh` with no login `PATH`: it puts `~/tehas/shim` and `~/tehas/bin` first before it reads GitHub, and says `gh not found` when no `gh` can be run (#2009).
- `tehas github check` checks the GitHub App's token, and `tehas github writes` prints the GitHub writes of the last day. They replace `tehasbot github-token --check` and `tehasbot github-writes` in the manual and in `tehas setup`; the old forms still work.
- `api/renames-1.0.tsv` lists the renames of 1.0, one row each.
- The MCP server's cycle `status` lists exactly the words a cycle can carry: it adds `not-started`, `stopped`, `parked` and `failed`, and drops `completed`, which no cycle said. A word outside the list is served as `unknown`.
- The cycle supervisor can take a job waiter in place of `tehas work`: `tehas-cycle cycle <issue> --runner job` reports a cycle run on another host from the notes and exit status the waiter leaves, and reads no workspace or image of the factory. It is refused with 78 unless the outsource feature is on, and outsource is still being built in this release. In-house cycles are unchanged.
- The improvement loop is gone: `tehasbot improve`, `tehas-improve.timer` and its service, the analyst's prompt and the record each cycle left under `~/tehas/tmp/improve`. `TEHAS_IMPROVE_REPO`, `TEHAS_IMPROVE_RUKAS_REPO`, `TEHAS_IMPROVE_RUKAS_SKIP` and `TEHAS_IMPROVE_MAX_OPEN` are no longer read, and an instance that still sets one of them to a value stops every command with 78 and names it. A blank value, and `TEHAS_IMPROVE_RUKAS_SKIP=0`, are ignored (#1927).
- `TEHAS_CYCLE_REPORT` is no longer read: the cycle's report has one form. An instance that still sets it to a value, `rust` too, stops with 78 and names it; a blank value is ignored (#2194).
- The `cross-project` label is gone from the label list, and the dispatcher no longer holds an issue that carries it. `tehas:improvement` stays, described as a proposal the factory filed: the review files under it, and the millwright still does not start such an issue.
- `install.sh` and `tehas update` stop and disable `tehas-improve.timer` and remove both of its unit files, as they do for the old refresh timer.
- The easy merge is gone, and with it `TEHAS_EASY_MERGE`. It never ran on a
  host: no installed command started it, and every refresh already ran the
  full refresh cycle. Every refresh is now the full refresh cycle, the
  dispatcher no longer marks an approval for an easy merge, and the start
  comment of a refresh is the one of the full refresh. Loading refuses
  `TEHAS_EASY_MERGE` with exit 78 when it is set to 1 or any value but 0.
- `tehas api` no longer lists the old names of the millwright: the field `fixer` and the feature word `fixer-grant` are gone from `tehas api --json` and from the feature line of `tehas api`.
- tehas updates a host from v0.30.0 or later only. The state store migrates a database from schema 12, the one v0.30.0 wrote, and refuses an older one with a line that says to update to a v0.30 release first. `install.sh` asks it before it copies anything (`store migrate --dry-run`), so such a host stops with exit 78, nothing installed, and the database is left as it was. The store's tests migrate the stores the v0.30 releases made.
- `store import` is gone, and `install.sh` no longer imports the record files a tehas before the state store kept under `state/`: a v0.30 install imported them. A poll that still finds one stops with 78 and says to move it out of `state/`.
- `install.sh` no longer takes a home without the `.tehas-home` marker for tehas's by the files an older install left: every install since v0.30.0 writes the marker.
- `analytics.db` at schema 2 or 3 is no longer migrated: it is kept aside under its version, as any other version is. v0.30.0 wrote schema 4.
- The manual, the MCP tool descriptions and the footer code describe the program that runs a factory by its role instead of by a product name.
- A write to GitHub that timed out or got a 5xx answer is no longer sent again blindly: the action journal keeps it as `unknown`, and a poll reads GitHub for it (a comment by its body, a label by name) before it is sent again; a cycle's report reads it back itself before its next try. A comment whose post timed out after GitHub took it is no longer posted twice, and a park question is no longer followed by a second, plain park comment (#1754).
- The state store moves to schema 16. `install.sh` and `tehas update` migrate it as usual.
- The millwright's old name `fixer` is gone from what tehas answers. The state store's schema 16 writes `millwright` in every row that still said `fixer`, so `tehas history` names one word. The `plan.json` of `tehas export` loses the duplicate `answers[].fixer`; read `answers[].millwright`. The MCP session tools answer `millwright` for an old run and no longer take `fixer` as a `how` filter.
- The release workflow also publishes signed prereleases: a tag `vX.Y.Z-alpha.N`, `vX.Y.Z-beta.N` or `vX.Y.Z-rc.N` is built, signed and published as a GitHub prerelease, never as the latest release. `scripts/tag-release.sh --pre alpha|beta|rc` tags the next one.
- `tehas update` reads alphas and betas beside release candidates and orders them alpha, beta, rc, then the release. A plain update passes over every prerelease; `tehas update --to <tag>` installs one by name.
- `tehas status --json` names the commit the installed release was built from in `versions.platform.commit`, and the health file's `checks.platform` carries it.
- The gate workflow runs on pull requests into `main` and pushes to `main`
  only. A pull request onto a release train runs no gate; the train's pull
  request into `main` gates the batch.
- The release workflow builds and signs on a clean GitHub-hosted runner and
  installs its own Rust, jq and minisign at pinned versions.
- The release job signs only after a person approves its run: the signing key lives in the repository's `release` environment. `SECURITY.md` says how to report a vulnerability privately.
- With `TEHAS_MILLWRIGHT_EPICS=1`, the millwright's label on a sub-issue whose edit record could not be read no longer starts it: the label comes off and the issue says the record could not be read, as when an outsider changed its text.
- One rule says which setting names are secret: a name that holds TOKEN, SECRET, PASSWORD, PASSWD, KEY or CREDENTIAL, in any case. The console's mask and `tehas review` use the same rule.
- `tehas review` takes a setting of a documented family, such as `PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST`, when the image lists the family, instead of stopping with exit 78.
- The MCP tool `factory_console` masks the console tail it returns, as the log tools do.
- The pinned ctx7 moves from 0.11.0 to 0.11.1.

### Held back

- Node 26.13.1 is out, a new major version. `host/versions.env` keeps 22.23.5: the move is a person's call.

### Adopter action

None.

If the scan was on, make sure something else asks for refreshes with
`tehasbot refresh --pr N`, such as the industry that runs the factory: after
the update nothing looks for conflicts on its own. Take the
`TEHAS_REFRESH_CONFLICTS` and `TEHAS_REFRESH_EVERY` lines out of
`instance.env`. A value they took before still loads and does nothing; any
other value stops every command with 78.

None.

None.

Run `tehas labels` to make and pin `tehas:classic`. Until it is pinned, an
application of it counts as nobody's, and a start under `TEHAS_DRIVER=lean`
over it starts nothing. Nothing else changes unless you set `TEHAS_DRIVER=lean`.

Nothing to do. `store init --outsource` and `store migrate --outsource` take
the new flag, but `install.sh` and `tehas update` do not pass it yet. Setting
`TEHAS_OUTSOURCE=1` in `instance.env` today is only validated and loaded: the
store stays off until a release passes the flag, and `tehas update` turns off
a store that was turned on by hand with `store migrate --outsource`. The
settings of the feature and their defaults are in
`instances/example/optional.env`.

Nothing in `instance.env` changes. Change scripts and notes that call the old
forms:

- `tehas setup --labellers "a b"` to `tehas setup --labelers "a b"`

- `tehas analytics export <file>` to `tehas analytics copy --to <file>`

- `tehas pause <reason>` to `tehas pause --reason "<reason>"`

- drop `--token-file` and `--app-installation` from `tehas setup`

A program that reads `--json` objects of tehas reads the new names. Every renamed field is a row of `api/renames-1.0.tsv`.

- `tehas status --json`: read `running[].progress.step` and the other step fields under `progress`, not on `running[]` itself.

- `tehas history --json` and `tehas export --json`: read `runs`, not `cycles`, and the step of a line under `runs[].progress`.

- `tehas start --json`: read `approval.bodySha256` and `approval.titleSha256`.

- `tehas refresh --pr N --json`: read `headSha`, not `head_sha`.

- `tehas disk --json`, `live/disk.json` and the MCP tool `factory_disk`: read `time`, not `generatedAt`.

- The MCP tool `factory_disk`: `dockerRoot` is gone; read the roles in `filesystems[].paths` and the home-relative `full.places[].path`.

Before you update, edit `instance.env`: rename `TEHAS_INDEXING=` to `TEHAS_SITE_INDEXING=`, `TEHAS_ANALYTICS_SRC=` to `TEHAS_SITE_ANALYTICS_SRC=`, `TEHAS_ANALYTICS_SITE=` to `TEHAS_SITE_ANALYTICS_NAME=`, `TEHAS_ANALYTICS_COOKIE=` to `TEHAS_SITE_ANALYTICS_COOKIE=`, `TEHAS_CONCURRENCY=` to `TEHAS_WORKSTREAMS=`, `TEHAS_TICKET_CONCURRENCY=` to `TEHAS_MAX_CYCLES=`, `TEHAS_REFRESH_MAX=` to `TEHAS_REFRESH_MAX_PER_PR=` and `TEHAS_OWNER=` to `TEHAS_ASSIGNEE=`. Take out any `TEHAS_CONTAINER_MEMORY`, `TEHAS_CYCLE_CPUS`, `TEHAS_CYCLE_CEILING`, `TEHAS_BOT_LOGIN`, `TEHAS_MILLWRIGHT_LOGIN` and `TEHAS_GITHUB_APP_INSTALLATION` lines. Have the program that manages the factory write `TEHAS_MANAGER_VERSION=` in `~/.config/tehas/env` instead of `TEHAS_INDUSTRY_VERSION=`. A program that reads `checks.industry` or `factory_status.industry` reads `manager` instead. `tehas update --check` lists every line still to change.

- In `instance.env`, take out `TEHAS_OUTSOURCE=1` (or set it to `off`):
  outsource is still being built and the line now stops every command.

- In `~/.config/tehas/env`, if `TEHAS_ENERGY` holds a value other than `on`,
  `off`, `1` or `0`, set it to `on` (sample the mesh) or `off`.

- A program that reads `tehas api --json` and tests for `store-copy`,
  `refresh-pr-starts`, `refresh-pr-lane`, `refresh-pr-json` or
  `host-message-health` tests for `state-copy`, `refresh-pr` or
  `host-message` instead.

A program or script that runs `tehasbot refresh --pr N` keeps working, and can move to `tehas refresh --pr N`. A person runs `tehas github check` and `tehas github writes` from now on.

Take `TEHAS_IMPROVE_REPO`, `TEHAS_IMPROVE_RUKAS_REPO`, `TEHAS_IMPROVE_RUKAS_SKIP`, `TEHAS_IMPROVE_MAX_OPEN` and `TEHAS_CYCLE_REPORT` out of `instance.env` before you update: with any of them set to a value, every command stops with 78. The update removes the timer. `~/tehas/tmp/improve` and `~/tehas/logs/improve.log` are left on the host; remove them when you no longer need them. The `cross-project` label stays on GitHub until you delete it.

Take `TEHAS_EASY_MERGE` out of `instance.env`. A line set to 1 stops every
command with 78 until it is gone; a blank value and `TEHAS_EASY_MERGE=0` are
ignored.

A program that reads `tehas api --json` reads `millwright` instead of `fixer`, and checks for the feature `millwright-grant` instead of `fixer-grant`.

A host on a release older than v0.30.0 updates to a v0.30 release first (`tehas update --to v0.30.6`), then to this one.

A reader of `tehas export`'s `plan.json` that reads `answers[].fixer` reads `answers[].millwright`. An MCP client that filters `factory_sessions` by `how: "fixer"` filters by `millwright`. `api/renames-1.0.tsv` lists the renamed key and the `how` value.

## v0.30.6 (2026-11-15)

### Changes

- The pinned ctx7 moves from 0.8.2 to 0.11.0.
- `scripts/verify.sh --changed` now runs the shell tests that drive the built
  programs for a change under `console/`, and every shell test for a change to
  `console/core/` or a crate it reaches. The pick used to name only the files
  holding `console/`, `t_store` or `t_attach`, so a change to the cycle, the
  dispatcher or the console skipped the tests that run them through `t_cycle`,
  `t_dispatch` or `t_console` (the headless runner, the container's argument
  lists, the settings), and a change to the settings library reached only 68 of
  the 160 shell tests. A cycle quotes this step as its proof, so a regression
  there could pass it. (#1759)
- `tehasbot health` no longer reads `state/industry/alerts.json`, the alerts
  file tehas-industry wrote into a factory: `checks.industry` holds only
  `version`, an industry alert no longer makes the health `warn`, and the MCP
  `factory_status` answer drops `industry.alerts`, `alertsTime`,
  `alertsStale` and `alertsUnreadable`. The schema and sample of the file are
  gone from `api/` (#1972).
- The banner at the top of the front pages shows only host messages, from
  `tehas host-message` (#1972).
- The host bridge has no `notices` field. An answer that still carries one is
  read as before and the field is ignored (#1972).
- `tehas store park-show` answers `cap`, the cap that stopped the parked run,
  read from the same `cycle_history` row its `park_reason` already comes from.
  A park with no history row answers `null`, as `park_reason` does (#2203).
- A start that was refused before anything ran is no longer reported as a
  failed cycle. `tehasbot cycle` ends such a start as `refused` instead of
  `unknown`, and `/health` leaves a `refused` ending out of `checks.failed` and
  says nothing of it as the last cycle's end. Nothing of the start ran, so
  nothing of it failed: a host whose last start was refused reads `pass` again.
  The park comment, its Retry boxes and the park's reason are unchanged, and
  `cycles.end_reason` still holds `unknown`. (#2206)
- A `Retry` box ticked on a park no longer starts a cycle that stops at
  `existing-pr-detected`. When an open pull request closes the issue, the poll
  holds the issue the way it holds a `tehas:ready`: nothing starts, the box is
  unticked, and a note on the issue names the pull request and says to merge or
  close it. (#2207)
- When an issue carries `tehas:question` and none of the comments the poll read
  carries the question it has on record, the poll now says what holds the issue
  and what clears it: `skip #N: its question is not among the last 100
  comments; tehas:question is set and no comment carries the question, so the
  issue waits until its open points are answered or the label is taken off`.
  The old line read as a comment-ordering problem, but the label is the hold
  and a new comment does not lift it.
  [troubleshooting](/troubleshooting#a-tick-in-the-park-comment-starts-nothing).
  (#2208)
- When `tehas-cycle finish` pushed `finish/issue-<n>` but could not open the draft pull request, or could not read its URL, the refusal now says the branch stands on the remote and that it must be given a pull request by hand or deleted before finish is asked again.
- `tehas labels` now also makes `tehas:improvement` and `cross-project`, the two labels the improvement loop applies, with their colours and descriptions. A test fails when a program names a label that `host/labels.tsv` does not have.
- The dispatcher reads the record that a refresh hold was said from the same path and at the same length its writer uses, so the two can no longer drift apart and a hold be said again on every poll.
- `factory_issue`, `factory_sessions`, `factory_session` and `factory_failures` of the MCP server answer a tool error naming the file when `status.json`, `running.json` or `health.json` is there but cannot be read, instead of answering as if the issue were listed nowhere or had no links.
- `factory_failures` counts runs with no outcome on record in `outcomesUnknown` rather than under an invented `unknown` outcome, and lists runs with a version missing after every pair that names both versions.
- A cycle's scratch directory under `TEHAS_SCRATCH_DIR` is now named by one rule in the code, used by the mount, the disk watch and the cleanup, so the three can no longer point at different directories.
- A cycle that hands off at the cap `lens-fix-reposition` with its review's findings still open now says why it parks, as one at `awaiting-human-merge` already did, instead of parking with no reason given.
- `factory_disk` answers `stale` when the published reading is more than three hours old, and refuses a `disk.json` of another `api` or with no file system instead of answering it as a healthy host.
- The egress proxy logs a connection it drops on its own side (a head that never came whole, a failed write, an accept that failed) and tells a name that does not resolve from one with no address.
- `factory_log`: a plain read masks the log only as far as the piece it returns instead of the whole file, so `size` is given only by a read that reaches the end of the file. `factory_search` keeps only the lines that match while it reads, and checking a log's name no longer compiles a pattern on every call.
- `tehasbot mcp serve` now reads `TEHAS_MCP_KEYS` and `TEHAS_MCP_DASHBOARD_KEYS` with the same rule as the MCP server before it starts the container: every line is read and any whitespace separates keys, so the check no longer passes a list the server then refuses, or refuses one the server would take.
- The dispatcher and the dashboard now read `state/pause.json` through the same reader as `tehas pause` and the health page. A link at that name whose target is gone, or a flag that names a key twice, now holds every start instead of reading as no flag.
- The keys and words of a poll's `slots` object are named once, in the shared core crate: the dispatcher writes them from that list, `tehas status` checks them against it, and a test holds `api/status.schema.json` to it. A bound word added in one place and not the others now fails the tests instead of dropping `room.slots` from `tehas status --json` without an error.
- With `TEHAS_WORK_DESCRIBE=1`, `tehas work` now says which way the descriptions of the images went wrong: describe failed, describe could not be started (nothing was attempted), or its end could not be read (it is killed, and whether it wrote them is not known). It used to print "could not be written" for all three.
- A planning turn whose container was killed or never started, before the runner wrote its session record, no longer deletes the record the previous turn kept. The retry the issue is offered now resumes the session instead of finding it gone.
- The list of state labels that tidy takes off a closed issue is now checked against `host/labels.tsv`: a label added to the table must be declared either a state or one a closed issue keeps, or the tests fail.
- The preview sweep lists containers by their whole id and removes one only by a 64-character id: a short hex word, which Docker would take as a prefix of some other container's id, is skipped.
- `scripts/test.sh` given paths with no test file and no linted script among them now exits 3 instead of passing, and `scripts/verify.sh --changed` prints a `skip` line for the shell tests in that case instead of `ok`, so a green step always means checks ran.
- The factory header of the default design shows the tehas logotype beside the logo mark, with FACTORY in small capitals next to it. The pixel design is unchanged.
- The gate is green again: the label table test knows the improvement loop's two labels.
- The pinned Pi moves from 1.5.0 to 1.5.2.

### Adopter action

Update the industry that runs the factory to a release that writes no alerts
file before updating tehas. A `~/tehas/state/industry/alerts.json` left by an
earlier industry is no longer read and may be deleted. A host that shows a
message on the factory does it with `tehas host-message`.

None. The next `tehas update` runs the labels step, since the table changed.

A client that reads `size` from a plain `factory_log` answer that has `next` takes it from the last piece or from a gzip read.

## v0.30.5 (2026-11-14)

### Changes

- The slots line of `tehas status` and the dispatcher's hold line no longer say `host-bound, no reading` for a host whose CPUs or slice answered but hold less than one cycle or two containers: they name the reading and say `at the floor`, and `room.slots.*.bound` is `cpu` or `memory` there instead of `host`.
- `tehasbot store publish --health` no longer leaves the outage marker behind when a failing health report cannot be written, so `/health` keeps answering from the report that is on disk; a marker that cannot be removed is now reported as `remove live/health.down`, not as a write.
- `tehas clean` reads the sizes and creation times of the images it weighs with one `docker image inspect` per step, instead of one or two per image.
- The names of the issue lists in `live/status.json` are now defined once and shared by the writer, the health page, the analytics sampler and the MCP server, so a rename can no longer silently zero the queue counts.
- `tehasbot clean` stops measuring one path after a million names or ten seconds, so a huge build tree no longer holds up the rest of the pass. A size cut short there is logged as `at least <n> bytes`, and so is the run's closing total; the path is still removed.
- The gate now fails when a program of the Cargo workspace is missing from install.sh's list, the release build or the release tarball's flags, unless it is named as not installed with a reason. `tehas-merge` is named so: it is built and tested but not installed yet.
- The gate's line for the workspace's tests now counts what ran, as in `ok    the workspace's tests (827 passed, 3 ignored, 41 test binaries)`, and the gate fails when `cargo test` exits 0 but prints no `test result:` line, so a green no longer hides a run in which no Rust test ran.
- `scripts/test.sh` stops with exit 2 and `test.sh: cannot start <file>: <reason>`
  when a test file's task process cannot be started, instead of skipping it and
  letting a record already on disk for that file read as a pass.
- `scripts/verify.sh --changed` prints a `skip` line for the shell tests when the change holds no test file and no shell script to lint, instead of an `ok` line for a step that ran nothing. `scripts/test.sh` given only such paths now exits 3 rather than 0.
- `tehas update` from releases runs the new version's settings check under a limit of 120 seconds and reads at most 64 KiB of what it says; a check that runs out of time or says more stops the update with nothing switched, instead of waiting and growing without end.
- A `TEHAS_` key that could not be searched for in the new version's tree is no longer reported as unknown to it; it gets a warning of its own.

### Adopter action

None.

## v0.30.4 (2026-11-14)

### Changes

- The poll holds starts while the sandbox cannot push. While
  `state/push-fault.json` is on record and younger than ten minutes the poll
  starts no cycle and logs `holding: the sandbox cannot push: <git's line>
  (<login>)` once, so every ready issue does not fail the same push check in
  turn. Past ten minutes the poll starts as it did before, and a start whose
  push check passes takes the fault off again. Running cycles, planning turns
  and refreshes are not held by it (#2129).
- `tehas-cycle finish` now reads every page of the open pull requests before it pushes, and refuses when the list cannot be read whole, so an issue's pull request past the first 100 no longer lets it open a second one.
- `store load --from` no longer says a cycle is running when a cycle lock under `state/cycles` cannot be probed: it still refuses, and now says it cannot tell whether a cycle runs, with the probe's reason.
- Removing a label no longer reports success when gh failed for another reason whose message happens to contain the words "not found": only an HTTP 404, or a reason that is exactly "Not Found", counts as the label already being gone.
- The split of a container's words into its limits and its security options is made in one place, and a cycle whose limits are not the expected fourteen words now refuses to start (exit 78) instead of moving words between the two lists.
- A running cycle whose lock file or `state/cycles` directory was removed is no longer shown as lost after two minutes of quiet; only a lock file found free marks it lost.
- `tehas work` says again when the heartbeat or the publish of the live view fails in a new way or fails again after working, instead of once per cycle.
- A cycle that did not start now says why, in the poll's log and in the comment on the issue: the exit code of the unit's start, a start that ran out of time, a state store that stayed busy or failed during the wait, or a unit that started and put no attempt on record.
- A planning turn whose `systemctl` cannot be asked now counts as still running, so the dispatcher no longer reports it as cut off while it may be at work. A `state/cycles` that cannot be listed now counts as cycles that may be running: the code index is left alone and auto-finish holds its records.
- `tehas update` now fails with exit 1 and names the file when it cannot remove the record of follow-up steps after the last one ran. It used to exit 0 and leave the record, so `install.sh --check` stayed red and the next update ran the same steps again.
- A planning turn whose unit `systemd-run` accepted but which never took its lock or ended within the start wait is now logged as `could not start the planning turn` and its session ended as not started, instead of being reported as running.
- When the poll cannot mark a pull request as refreshing, the ERROR line that refuses the start now ends with GitHub's reason (for example `HTTP 404` for a missing label, or a rate limit).
- A status command run by the poll that fails or is killed at its bound now logs `WARN: the status command ... exited <n>` (or `was killed at its bound or did not start`) instead of passing in silence, and the health warning names the command and how it ended.
- A cycle's history row and park record keep the pull request number when GitHub spells the repository's owner or name in another case than the settings, or the address ends in a slash. Before, the number was dropped without a word and the run showed no pull request.
- The `verify` and `release-gate` workflows check the runner's cargo, rustc, rustfmt and clippy before the gate runs. A runner without them now fails with one error that names the runner and the missing tools, instead of a failure in every Rust test that reads as the change's.

### Adopter action

None.

## v0.30.3 (2026-11-14)

### Changes

- A cycle whose sandbox cannot push now records the fault in
  `state/push-fault.json`, ends its start as `push-fault`, and posts a short
  "no cycle" comment with the git line instead of parking the issue with retry
  boxes; the issue keeps `tehas:ready` so a person can retry after they fix the
  host (#1741).
- The settings tests now fail when the example instance names a setting with no rule, or when a retired setting is still kept or still has a rule.
- The dispatcher stops with exit 78 when `TEHAS_ISSUE_TYPES` is malformed, instead of reading it as unset and polling by the label alone, which missed issues marked by their issue type.
- `tehas plan` now refuses a container whose `/tmp` size is not a whole number of megabytes, as `tehas work` already did: both commands check the container's limits and security options with one shared check beside the code that makes them.
- The dispatcher no longer settles a journalled sub-issue link as done when the record names no readable sub-issue id: the action stays open with a warning, and only a linked row with the same id counts as proof. A journal line for an action with no intended time now says "of unknown age" instead of "0s old".
- A refine turn whose refine brief cannot be read no longer starts on the issue's text alone: the dispatcher logs an ERROR naming the file and starts nothing. A next planning turn that cannot set up its keep directory or answer file names the path and the reason, and a workspace sweep that cannot look at a leftover warns and no longer reports the issue's leftovers as removed.
- A start that `tehas work` refuses with 78 because the sandbox image gave no capabilities answer, lacks what the cycle is given, or the workspaces' directory was not made by tehas now names that cause in its park comment, from the new note `state/cycles/<issue>.refused`. Before, only the run log on the host said why.
- `tehas help dispatch` and the poll's `tehas:blocked holds it` log line now name the millwright, not "the fixer", as the account whose `tehas:blocked` label and blocker comment count besides an allowed account, as the dispatcher and the manual already had it.
- A cycle stopped by its container's process limit now says what that limit was and which settings it follows (`TEHAS_TICKET_CONCURRENCY`, `TEHAS_SLICE_CONTAINERS`), in its park comment and in the reason on its record; `TEHAS_SLICE_CONTAINERS` is now among the settings a cycle's record keeps.
- `TEHAS_VERIFY_TIMEOUT` now also bounds the whole gate on a cycle's combined tree: tehas passes it to pi-rukas as `PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS`, which before was left at pi-rukas's own 30 minutes whatever the instance set, and a cycle's record of its settings shows the number in effect.
- The health line for idle polls over their budget now says what tripped it: for each figure over, the median of the last hour's idle polls in the units of `TEHAS_POLL_BUDGET`, the budget, and how many of those polls were over it. `store poll-idle --budget` adds `over_polls` with those counts.
- The "no resources" hold now says why the model endpoint check failed: `refused the request with HTTP 401` (or the code it got), `could not be asked: curl could not be run`, or `does not answer` only for an endpoint that gave no answer, in the dispatcher's log, `live/dispatch.json` and `tehas status`.
- When the attachment checker gives no answer before a start, `tehas work` now says why: it did not answer within 60 seconds, it exited with a status, or its answer could not be read. The comment on the issue and the line in the cycle's log no longer report a slow or failed check as a broken manifest or approval record.
- A provider key in `models.json` gives a container a variable only when it reads as `$TEHAS_MODEL_KEY_<NAME>`, the same rule that refuses the file at load: a key such as `$PATH` no longer names a variable on the container's command line.
- A cycle's record no longer gives a `capReason` that only repeats the cap word: a cap the driver gave no words for reads `no evidence from the driver`, and memory kills in the container are said in the `capReason`, the cycle's `err.txt` line and its console file.
- A dispatcher poll that ends on comments, an edit record or pull requests it could not read now names that read, `ERROR: unreadable comments of #7`, and adds gh's own words when a gh call is what failed (`: gh api graphql failed: HTTP 502`). Before, it logged `ERROR: gh api graphql failed` even when no gh call had failed.
- The dispatcher judges a quiet cycle (stop, ask again, or leave it to a person) on the poll's own clock in both halves of the poll, so a slow state store can no longer make it act on a different instant than the one it decided on.

### Adopter action

None. An instance that raised `TEHAS_VERIFY_TIMEOUT` above 1800 now gets that bound on the combined tree too; check that `TEHAS_WORK_TIMEOUT` leaves room for it.

## v0.30.2 (2026-11-14)

### Changes

- The manual gives the right `tehas work` exit statuses: the by-hand run of an issue now says what the runner's own statuses are (0 merged, 10 and 11 a handoff, 20 aborted, 21 interrupted, 30 a crash or the runner's own limit, 40 refused) and what exit 2 means, instead of the 0, 1 and 2 it used to name, which read a handoff as a failure and an abort as nothing.
- The commands pages list every command: `tehas panel`, `tehas host-message`, `tehas mcp key`, `tehas state copy --to` and `tehas state load --from` in the list of a person's commands, `tehasbot mcp serve` and the unit `tehas-mcp.service` in the list of what the units run, each with a line and a link to the page that describes it.
- `tehas tidy` is named by its right name: the command that takes the state labels off closed issues is `console tidy run`.
- A test says every command `console/src/front.rs` routes for `tehas` and `tehasbot` is named in a command block of the commands pages.
- The manual and the example instance give the current review bar, build count
  and pi-rukas follow rule (#1768). The review severity pages say the bar is
  HIGH, pi-rukas's default since v0.14.50, where some of them said MEDIUM; the
  `TEHAS_BUILD_TIMEOUT` row says install.sh builds seven programs, each with at
  most two attempts, where it said four attempts in all; the `tehas update`
  help, `bootstrap.sh` and the bootstrap page say the sandbox image and the
  host's pi-rukas clone move to the commit `PI_RUKAS_FOLLOW` names, not to a
  branch's head.
- `instances/example/optional.env` is the reference for the optional settings:
  it gained the lines it was missing, `TEHAS_BUILD_TIMEOUT`, `PI_RUKAS_FOLLOW`,
  `PI_RUKAS_IMAGE_DIGEST`, `TEHAS_ENERGY_SOURCE_URL`, `TEHAS_MILLWRIGHT_APP` and
  `TEHAS_GITHUB_APP_ID`, and the Commands page points at it beside
  `instance.env`. A test keeps every optional setting of the loader's table
  listed there.
- The offline install of the new-project page names no release: it uses a
  `<tag>` placeholder and says where the newest tag comes from. README marks
  `.pi/settings.json` optional and describes `api/` as the schemas and samples
  of the factory API's JSON objects.
- The overlap hold is gone: `TEHAS_OVERLAP_HOLD`, the `[paths: ...]` match
  the dispatcher ran on an issue's body, its read of the running cycles'
  branches and the store verb `paths-overlap`. To keep two issues that change
  the same files apart, a host holds one of them out of the start order with
  `tehas order set --only` (#1951).
- `tehas start <n>` approves issue n for a start from the host, with no label and no GitHub account of its own. The approval is a row of the state store of its own kind, `host`, and binds the issue's title and text. The next poll starts the issue under the usual rules: the slots and the limit, a pause, blockers, an open pull request, a park and the start order. Nothing starts at once. `tehas start <n> --clear` takes back an approval no cycle has used, and `--json` answers with what was recorded. The start comment, `tehas history` and `tehas status` say the cycle was started from the host, and the millwright's retries and finish asks rest on it as on a person's label. `tehas api` lists the feature `host-start` (#1967).
- The state store is at schema 14: `cycles.how` may say `host`. `install.sh` migrates the store and keeps every row.
- `tehasbot refresh --pr N --json` prints one object,
  `{asked, pr, issue, head_sha, why}`, on standard output for exit 0, 3 and
  1, so a caller reads why a refresh was declined without parsing standard
  error (#1978).
- `tehas api` says whether the timed refresh scan is on: `refreshScan` in
  `--json`, and a `refresh     scan on|off` line without it. A new feature
  name, `refresh-pr-json`, marks a release that has both (#1978).
- When a signal stops a cycle, a gate's end line that matches no started run no longer closes another run that is still going: the `verify-aborted` lines now name the runs that were really cut short.
- A cycle whose log could not be read is no longer reported as "no step started": `tehas work` says the stream could not be read and leaves no `.nostart` note.
- Lines of a cycle's log that are no JSON, such as a line cut when the container was killed, are counted in `progress.json` (`unparsed`) and named in `tehasbot cycle`'s log instead of vanishing.
- A cycle's failed-end record (`state/cycles/<issue>.end-failed.json`) whose `attempt`, `tries` or `said` is missing or of the wrong type is no longer read with a stand-in value: the poll and `tehas status` treat it as unreadable, so a damaged record cannot re-arm the poll's tries or show "0 of 3 tries made".
- The end of a cycle refuses a facts record with no repository, start time, current time or signal flag, instead of reporting a false runtime or outcome and writing to an empty repository path.
- A work cycle whose token renewer cannot start (for example `libexec/tehas-cycle` missing after an interrupted install) is no longer run silently without one: the cycle is refused and the work log says why.
- `TEHAS_GITHUB_RENEW_EVERY` must be a positive whole number of seconds; a value such as `5m` or `0` now fails the settings load with exit 78. Before, a malformed value was silently replaced by the default of 300 seconds.
- A cycle whose end-failed record cannot be written, or whose earlier record cannot be removed, now ends with exit 70 and an `ERROR` line that names the file and the error, instead of exit 0 and a bare warning.
- A cycle's `progress.json` now says `"from":"bad-result"` when the run left a result with something in it that gives nothing for the issue (another schema version, no JSON, a link, or no entry with the issue's number), instead of reading the same as a run that wrote no result, which stays `"progress"` (no file, or an empty one, as a runner that crashed or never started leaves it).
- The cycle's facts no longer read `.pi/work-state` from the current directory when the workspace path is not absolute, as an unset `TEHAS_WORK_DIR` left it.
- The report of an ended cycle refuses facts that hold no whole number for the exit status, the start or the time, instead of reading 0 and taking the success arms; a store answer that is not an object reads as "malformed", not as an answer.
- Describing an image that cannot be read leaves "not described: the image could not be read" instead of sending empty bytes to the model.
- The cycle reads an issue's entry of the runner's result one way everywhere: the timeout note on the issue, the record of a failed end and the report now describe the same run when a result file holds more than one, and none of them takes an issue number written as a fraction.
- `tehas work`: the end line of a cycle that ended at a cap now says when the progress file could not be written or read, instead of showing a bare cap or a reason left by an earlier run, and says when the driver's log could not be read for diagnostics.
- `tehas work` no longer leaves the note of a runner that never started (`state/cycles/<N>.nostart`), nor its "no step started" line in the error file, when the container wrote a result it could not read: the note now means the result file holds nothing, as the manual says.
- `tehasbot cycle` now says when the `.nostart` note of a cycle that wrote no result is a link, could not be read, or holds no exit status of 1 to 999. A note that could not be read still parks the cycle as `runner-never-started`, with its exit status said as not known.
- A cycle whose container was killed with 137 from outside, out of memory or otherwise, goes on record with the cap `container-killed`, a `capReason` naming the step and the memory kills, and a line in `console.txt` and `err.txt`.
- A cycle that died after `commit-pr` now finds its pull request when the driver names the branch in a later event rather than as a step, so its record and comment name the pull request it opened.
- `instances/example/optional.env` documents `TEHAS_GITHUB_RENEW_EVERY`.

### Adopter action

- An instance that sets `TEHAS_OVERLAP_HOLD` to a value now stops every
  command with 78 and is told to take the line out; a blank value is ignored.
  Take the line out, and hold an issue with `tehas order set --only` instead.

If an instance sets `TEHAS_GITHUB_RENEW_EVERY`, make sure it is a whole number of seconds, such as `300`.

## v0.30.1 (2026-11-13)

### Changes

- The store no longer runs its whole-file check when a command only opens it to
  read, and the dispatcher no longer prunes the analytics database on every poll.
  Both were paid about ten times in each two-minute poll, on a file neither of
  them can change. A damaged store is still refused: every verb that writes meets
  the check before it changes a byte, and `store check` still answers for the file
  (#1769).
- `tehas disk --full --json` prints what the daemon still holds back. The count
  of stopped containers and the list of them, which before came only from a run
  that also published, are now part of `--full` itself: a reader that must not
  write the host asks for both figures with the flag alone. The command's own
  reads still write nothing, and `--publish` is still the one form that does.
- The answer of `tehas disk` has a JSON Schema and two samples in `api/`:
  `disk.schema.json`, `disk.sample.json` for the plain answer and
  `disk.full.json` for the one with `full`. A program that reads the object can
  copy the samples into its own tests.
- `tehas update` no longer hangs at its end when a sandbox build ignores the
  TERM of its time limit: every call the update bounds is now killed 60 seconds
  after its limit, and a call's output stops being read a few seconds after the
  call itself exits, even if something it started still holds the output open.
- `tehas stop` now also clears a cycle's `.disk`, `.comments` and unfinished `.runner_exit.new` / `.status.new` notes, and a new cycle clears the unfinished `.new` notes an earlier one left. Every step that clears a cycle's notes reads one list, so a new note cannot be missed by one of them.
- With `TEHAS_OVERLAP_HOLD=1`, a start that goes ahead because a running cycle's branch could not be read is now logged (`#N starts unchecked for overlap: ...`). Before, it looked the same as a start that overlapped nothing.
- `tehas index` now fails (exit 1, with a warning naming the file) when the index was built but its record `state/index.json` could not be written, instead of reporting success. The tip is noted as failed, so the dispatcher's poll no longer runs the indexer again at the same commit on every poll.
- A park owed a refine whose record the dispatcher cannot read is no longer skipped in silence every poll: one with no time its cycle began is logged as a skip and spent like any other refusal, and one with no readable issue number is logged as a WARN line.
- `analytics outcomes` (and the MCP tool `factory_outcomes`) no longer drops runs from `failureKinds` and `parkReasons`: a kind or reason that is recorded but is not a valid word now counts under `unrecognised`, and runs that share one second are each counted in every map.
- A store refusal of one read inside a batched poll read (such as a busy database) is now read with its own word and exit, as the single call reads it, so a transient lock skips that one issue instead of ending the poll as an unusable store.
- A retry that is refused before its container runs (a failed push check or install, a product with no gate, a stop) now puts the stopped cycle's workspace back at `~/tehas/work/<name>-<issue>`. Before, the refused start left its own empty clone there and the finished workspace under `.before-retry`, which the end of the cycle then removed.
- A new `tehas:ready` on a parked issue whose park question is no longer on record now starts the cycle. Before, the label was held on the unchanged body while every box of that park was refused, so only an edit of the body could restart it.
- A cycle's read of its issue from GitHub before it starts is now bounded: each of its three tries ends after 60 seconds, so a `gh` that never answers no longer holds the issue's lock and a slot until the unit's 8-hour limit. The refusal line now says how the last try ended.
- The text tehas posts to an issue, a park or a planning question, is now masked by the same rules as the console: provider keys, bearer values, passwords in any URL and any `*KEY*`, `*PASSWD*` or `*CREDENTIAL*` assignment are hidden there too.
- A preview whose state file cannot be written now says why on standard error (the directory, `pull.json` or `state.json`, with the system's error) instead of failing without a word.
- Every GitHub call of the improvement loop now ends after 300 seconds, so a `gh` client that stops answering no longer holds the run, and its lock, until the unit's 90-minute limit.
- A cycle's start no longer waits without end on Docker before its start on record: the read of the sandbox image's pi-rukas commit is cut after 30 seconds, and a daemon that has not answered by then costs only the commit, so the start goes on record well inside the dispatcher's 300-second wait instead of holding the issue's lock past it.
- CI now installs the pinned node of `host/versions.env`, checksummed, before the gate, so the front page's tests run on every pull request and in the release gate instead of printing a `skip` line.
- `tehas drain` says on stderr when the poll's lock could not be tried at all, instead of reporting it only as a poll that held its lock for the whole wait. It still reports not drained in that case.
- `analytics plan` now refuses an empty `--rc` as a usage error instead of recording a plan with no exit status; the issue number, `--at` and `--rc` are checked by the workspace's one whole-number rule.
- `install.sh --check` also checks the owner and mode of
  `~/.config/tehas/env` and `~/.config/tehas/instance-dir`: a file another
  account can write is a finding, as for `instance.env`.
- The indexing container runs with no capabilities and `no-new-privileges`.
- Text the host posts drops control and invisible characters, and the release
  notes `tehas update` prints do too.
- `console gh` counts the attached forms of `gh api` options, such as
  `-X=POST` and `-fbody=x`, as writes.
- The mask hides each key of `TEHAS_MCP_KEYS` on its own, not only the whole
  list.
- A `tehas:finish` the poll ignores no longer counts toward the three asks a
  poll runs, and no longer keeps a failed end from being finished by the poll.
- The poll reads every open pull request for the hold on a closing keyword,
  not the newest 100, and `tehasbot refresh` reads them all before it removes a
  request. More than 5000 open pull requests is a read that failed.
- The note that a `tehas:blocked` hold has cleared counts as said only when
  the bot wrote it.
- Every service in `host/systemd` sets `NoNewPrivileges=yes`, and `tehas-mcp`,
  `tehas-top` and `tehas-energy` have `MemoryMax=256M`. The cycle's unit
  passes the issue to its shell as an argument.
- The docs server runs as the edge router does: a read-only root, every
  capability dropped but `NET_BIND_SERVICE`, and `no-new-privileges`.
- The dashboard draws a link from the queue file only when it is a web
  address.
- With `TEHAS_EGRESS=1`, the push check and the install of the product's
  dependencies before a cycle are on the cycle's network too.
- The product's `.pi/settings.json` in a workspace is read only as a plain
  file of at most 64 KiB; the manual names the files the host reads there.
- A bounded call of the poll holds no more of a program's answer than its
  ceiling.
- The issue titles and bodies a cycle posts through the relay are masked as
  the console masks a log, and their `@` mentions notify nobody.
- The mask and the store's check share one list of token shapes: the mask now
  hides Slack tokens, AWS key ids, JWTs and private key blocks, and the check
  also knows a JWT.
- The MCP server's `factory_events` cursor hashes the masked start of a log.
  A cursor from an earlier version is taken once as a replaced log: that log
  is read from its start and named in `gaps`.
- The MCP server closes a connection whose answer is not read within 10
  seconds and one more for each 128 KiB of it.
- The egress proxy sends a plain `http://` request with the `Host` of its
  allowed target, not the one the client sent.
- The sandbox base image is built from pinned inputs: ubuntu by digest, and
  node, gh, bun and rustup at the versions and checksums of `versions.env`
  rather than from install scripts. pi, ctx7 and parallel-cli come from the npm
  trees in `host/npm`, which `install.sh` now installs to `~/tehas/npm`. The
  first stage builds with `RUST_VERSION` instead of stable. A change to any of
  these pins asks for a sandbox build.
- After an easy merge, a pull request that conflicts again is refreshed the easy
  way again, and one whose named check went red gets the full refresh. The two
  rules of #1031 were there but read nothing: `history-show` answered no easy
  mark for a refresh row, so every refresh after the first easy merge took the
  full path, and the PULL query never asked for the checks of a pull request, so
  the red-check rule could not see a check. A cycle's easy mark now lives on the
  approval of its start, is answered by `history-show`, and is kept past the
  keeping of a later start and past the end of a cycle that never ran, so the
  record of a host still says which start ran as an easy merge and never offers
  the same easy merge twice (#1751).
- With `TEHAS_REFRESH_CONFLICTS` other than 1, a refresh request the timed run of `tehasbot refresh` wrote (one without `"asked"`) no longer waits for good. The timed run exits at once with the setting off, so the dispatcher sweeps such a request itself at each poll, whatever the room: it drops one whose issue is closed or no longer waits for a merge, or whose pull request is closed, merged, moved, no longer conflicts or is no longer one the rule keeps, and `refreshHold` clears in the poll that drops the last one. A request whose issue or pull request could not be read stays. `tehas api` lists the feature `refresh-pr-sweep`. The usage of `tehasbot refresh` no longer says drafts are left out: they are kept as any other pull request (#1973).
- `scripts/tag-release.sh` no longer tags a release whose changelog section lists no changes. A section with an Adopter action of "None." and no `### Changes` lines under it used to pass; the tagger now refuses it, names the section it read and tags nothing.
- The dispatcher's read of the last poll report, and the cycle's read of the runner's progress when it ends at a cap, now use the shared bounded reader: a link, or a file larger than 1 MiB, counts as no report instead of being read whole.
- A planning turn whose reply adds images is no longer refused when only the removal of the session's earlier image directory fails: the new images are in place, and the leftover `.old` directory is named on stderr. A swap that does fail now says which step failed and the OS error.
- The runner, the report, finish and the dispatcher now decide by one rule whether a pull request belongs to an issue. Before, a body such as `Resolved #7` or `does not fix #7` counted for one and not the other, so finish could open a second pull request, or refuse one, where the report saw the cycle's own.
- The GitHub token renewer no longer waits so long between asks that the token runs out: `TEHAS_GITHUB_RENEW_EVERY` is capped at 540 seconds, which keeps the token valid through one failed ask, and a value that is not a whole number above 0 falls back to 300.
- `finish` no longer hangs on a git or gh call whose program left a process holding its output open: the call's streams get the grace after it ends, a call stopped at its bound says so in the refusal (a program that died of a signal keeps its own words), and the grace after TERM ends as soon as the call does.
- A cycle's stall, time-limit or `tehas stop` note that could not be written or read is now said: the watch records a lost note in the cycle's error log, and the end report warns of a note it cannot read or that holds no count of seconds, instead of reporting the run as one that was never stopped.
- A cycle's token renewer no longer hangs on a token command that never ends: one renewal is stopped after 60 seconds, with what it started, logged as a failed renewal and retried after 30 seconds, and a stop of the cycle ends the renewer at once even while a renewal runs.
- A cycle that hands off with its pull request open is moved to `tehas:awaiting-merge` only when its progress says the review held nothing (`reviewHeld: false`). A progress file that is silent on the review, not JSON or missing now parks the issue with a warning and a comment saying so, instead of releasing it or claiming the review ran out of rounds.

### Adopter action

Nothing is needed for the rows: every write of the analytics program prunes the
tables it just wrote. The poll's run of `analytics prune` was also the only
automatic sweep of the `analytics.db.schema-<version>` files a migration leaves
beside the database, and the only `PRAGMA optimize`. Both are `analytics prune`
alone, and it now runs only when asked:

```
~/tehas/libexec/analytics prune
```

If `install.sh --check` names `~/.config/tehas/env` or
`~/.config/tehas/instance-dir`, make it the tehas user's own and writable by
no one else (`chmod go-w`).

With `TEHAS_EGRESS=1`, a product whose install fetches from a host outside the
proxy's list now fails its install before the cycle starts: add that host to
`TEHAS_EGRESS_HOSTS`.

The next `tehas update` rebuilds the sandbox image. The build passes the npm
trees as a named build context, so it needs Docker's BuildKit builder (buildx),
which the base build already needed.

## v0.30.0 (2026-11-13)

### Changes

- The console's operations (setup, update, clean, health, flags, stop, the store
  calls and the modules they share) are a library of their own in
  `console/ops/`, which builds and tests alone. Commands, output and exit
  statuses are unchanged; the help of `tehas disk`, `tehas limit`, `tehas pause`
  and `tehas resume` names the new source path (#1806).
- With `TEHAS_REFRESH_CONFLICTS` other than 1 and `TEHAS_REFRESH_SLOTS` above 0, a refresh `tehasbot refresh --pr N` asked for runs in the refresh lane, not in a cycle slot. The lane is kept only while such a request waits or a refresh runs, and the pull requests marked `tehas:refreshing` are listed and their stale marks cleared at those times too. A request without `"asked"` keeps no lane. With `TEHAS_REFRESH_SLOTS=0` refreshes take cycle slots as before. `tehas api` lists the feature `refresh-pr-lane` (#1877).
- `tehas-cycle finish` counts only the factory's own open pull requests, as the poll does: one on a branch of the repository that the bot or an account of `TEHAS_ALLOWED_LABELERS` opened. A pull request from a fork, or from any other account, that names the issue no longer stops it. It reads the list through GraphQL and refuses when `TEHAS_ALLOWED_LABELERS` names no account.
- A running cycle is stopped for a merged or closed pull request only when
  that pull request is on its issue's own branch, `feature/issue-N`. A pull
  request on another branch whose description says `Fixes #N` no longer stops
  the cycle of #N.
- The poll decides a cycle's end from the host's own record. A push or a pull
  request that only the cycle's result says failed is on record and in
  `tehas status`, but the poll no longer finishes it on its own: it waits for
  `tehas:finish`.
- A cycle whose result names a provider outage is parked under its cap and
  waits for a person, with a retry box. It is no longer started again on its
  own. A park recorded as `provider-outage` by an earlier version keeps its
  one retry.
- A refused `tehas:finish` tells the issue the rule's own words. What git or
  gh said goes to `dispatch.log` on a line of its own.
- `tehas labels` makes `tehas:prototype` and pins it in `~/tehas/state/labels`
  with the granting labels. The Protolab puts no ticket at the prototype stage
  while the label under that name is not the pinned one, and does not publish
  a title changed after the labelling by an account outside
  `TEHAS_ALLOWED_LABELERS`. With `TEHAS_ISSUE_TYPES`, who set the type is
  checked by user id as well as login.
- `tehas update` on a host that updates from its checkout
  (`TEHAS_UPDATE_FROM=git`) updates only the default branch of origin. A
  checkout on another branch, or one whose `origin/HEAD` is not set, now
  changes nothing and exits 1. To install another branch, run `./install.sh`
  in the checkout by hand.
- The command printed for root when `/usr/local/bin/tehas` is not current now
  reads `sudo <root-owned copy>/bootstrap.sh dev <user>`. The bootstrap page
  says to run `bootstrap.sh` as root only from a copy root owns, never from
  the tehas user's checkout or `~/tehas/current`.
- `TEHAS_MILLWRIGHT_APP=<slug>[bot]` in `instance.env` names a second GitHub App the factory trusts as its millwright in place of its own app. The load gives it to `TEHAS_MILLWRIGHT_LOGIN`; the factory still acts as its own app, whose `tehas:ready` then counts as any outsider's. A value that is not `<slug>[bot]` stops every command with 78. A `TEHAS_MILLWRIGHT_LOGIN` line still warns and is ignored.
- The business front page no longer draws a "Token use" table from the host
  bridge, and `tehas api` no longer lists the feature `host-bridge-tokens`. A
  host bridge answer that still carries `tokens` is read as before and the
  field is ignored. The factory's own analytics of its cycles stay (#1968).
- The factory floor's work order no longer shows the runs, tokens and time of
  live tickets: `tehasbot status` no longer writes `live/orders.json`, the
  analytics program has no `orders` command and the docs server no longer
  serves the file (#1968).

### Adopter action

On a host where `tehas:prototype` already exists, the labels step of the
update says `not pinned: the label tehas:prototype (<id>) was there before
with no pin` and stays owed. Check on GitHub that it is your prototype label,
add the line `tehas:prototype<tab><id>` to `~/tehas/state/labels`, and run
`tehas labels` again. Until then the Protolab lists no ticket.

On a host with `TEHAS_UPDATE_FROM=git`, check that the platform checkout is on
origin's default branch and that `origin/HEAD` is set:
`git -C ~/tehas/platform remote set-head origin --auto` sets it. Before root
next runs `bootstrap.sh`, make root's own copy as the bootstrap page says
(`sudo git clone <the platform repository> /opt/tehas`) and run it from there.

A `live/orders.json` left by an earlier release is no longer read or served
and may be deleted.

## v0.29.0 (2026-11-13)

### Changes

- The manual's by-hand commands for the docs server and the edge router read `. ~/tehas/env.sh` first (#1697). The four `docker compose -f ~/tehas/edge/...` lines of `host.md` ran compose straight, and compose takes `TEHAS_DESIGN`, `TEHAS_INDEXING`, `TEHAS_ANALYTICS_*` and `TEHAS_HOST_ADDR` from the environment: recreated from a shell that had not sourced `~/tehas/env.sh`, a host of `TEHAS_DESIGN=pixel` came up in beesknees26 and the ports bound to `127.0.0.1`. `test/manual_test.sh` now fails when a line of the manual runs compose over a file of `host/edge` with no `. ~/tehas/env.sh` on that line or on the line before.
- The settings table of the manual has a row for every setting the loader takes. Five were missing: `TEHAS_CYCLE_LOGIN`, which the load refuses with 78 and a row now says so; `TEHAS_SCRATCH_DIR`, `TEHAS_TOP`, `TEHAS_UPDATE_FROM` and `TEHAS_RELEASE_REPO`, each described in prose elsewhere and now listed beside the rows they belong with. `TEHAS_ENERGY_SOURCE_URL` has a row too. A new test, `test/settings_table_test.sh`, reads the loader's table in `console/core/src/settings/rules.rs` and fails when one of its settings has no row in the table, or a row names no setting (#1698).
- A retried cycle is no longer reported by an earlier attempt's closed or
  merged pull request. The end of a cycle took the bot's pull requests of its
  branch with no bound on when they were opened, so a retry that parks before
  it opens one could be reported as `pr-closed` or `merged` by the pull request
  of the attempt before it: the issue got the wrong comment, and a failed cycle
  was recorded as merged. A closed or merged pull request now counts only when
  it was opened at or after the second the cycle took its lock, as the
  dispatcher already does, or it is the pull request a refresh was approved
  for. An open pull request on the branch still counts whoever opened it
  (#1747).
- A review of a branch whose newest commit is older than a week no longer reviews the whole history. It says there is nothing new and starts no container, as a branch whose record already names the tip does. A git that cannot read that week is said, and the run exits 2 without reviewing (#1750).
- A GitHub write that times out no longer holds the poll. A call that runs out
  of time is TERMed and KILLed as a process group, so a `gh` the shim started
  and left behind does not keep the poll's pipe open and keep the poll from
  running again (#1755).
- The long-lived logs are rolled past a size cap. `store upkeep`, which `tehasbot clean` runs once an hour, now cuts `logs/dispatch.log`, `logs/review-timer.log`, `logs/refresh.log` and `state/github-writes.log` by copy and truncate when one is over 64 MiB, keeping one `.1` copy of it. Nothing is renamed, because the dispatcher's unit and every cycle hold their log open for append. The cap is one figure for the four, with no setting for it, and the store's answer names the logs it cut under `rolled`. A log that is a link, a directory, empty or not there at all is left alone (#1757).
- A poll that finds the dispatcher's lock held leaves
  `state/dispatch.again` alone. It used to remove that ask before it looked at
  the lock, so a second poll started while one ran dropped the ask of the
  running one and the freed slot waited for the 2-minute timer instead of
  `tehas-dispatch.path` starting a poll when it ended (#1758).
- host.md's store restore no longer gives steps of its own, which paused the
  factory and deleted the database's `-wal` and `-shm` while the poll, health,
  energy and the MCP server still ran: a paused poll writes the store every two
  minutes. The page links [Backups and
  restore](/operating#backups-and-restore) instead, which drains, stops
  `tehas-*.timer`, `tehas-*.path`, `tehas-energy.service` and
  `tehas-mcp.service`, swaps the copy, runs `tehas state check` and starts them
  again. The copies `install.sh` makes before a migration are called
  `tehas-<schema>-<time>.db` there, the name the store writes (#1766).
- The dashboard's files are compressed and cached. The docs server sends every
  answer compressed, and the dashboard's scripts and stylesheets are installed
  under a name that holds the hash of their own bytes and served `immutable`,
  while the pages keep their names and are served `no-cache`. A browser now
  asks about a page and reuses what it has of the files it names, and a new
  install is seen at once because a changed file gets a new name. Files read
  with Range requests (the console of a running cycle, and the health page) are
  sent uncompressed, as before. The theme stylesheet, the fonts and the logos
  keep their names. (#1811)
- Each release carries the rules of the instance settings as `api/settings-rules.json`, what the new `console settings rules --json` prints, so a tool with no factory can check `instance.env` values. `tehas api` lists the feature `settings-rules` (#1879).
- A renamed GitHub App no longer leaves the factory under its old login. The login in `~/tehas/state/github/login` was looked up once and kept for good, so after a rename a cycle listed its pull requests by a login that names nobody, found none, and parked a pull request that waited only for a merge as `needs-human-attention`; its title then read `HALT: `. Each new token now asks GitHub for the app's login and rewrites the file when it changed.
- The factory no longer writes the titles of its pull requests. The `WIP: ` and `HALT: ` prefixes are gone: a poll and a cycle's end no longer set them, `console pr-title` is removed, and `tehas api` no longer lists the feature `pr-title`. The labels of the linked issue carry the state: `tehas:running`, `tehas:awaiting-merge` and `needs-human-attention`. The open pull requests on the poll's first page are read with only their number and body, which is all the hold on a closing keyword reads (#1921).
- An easy merge is no longer refused because the branch changed files of its
  own. The merge of a conflicting pull request checked what its merge commit
  changed against the files that conflicted, and read that with
  `git diff <head> <commit>`, which also names every file the branch changed
  since the branch point and git merged clean. A pull request that conflicts
  in one file while the branch moved any other was therefore refused almost
  always, and the cycle fell back to a full refresh. The check now reads the
  commit with `git diff-tree --cc`, which names only the files the merge
  itself wrote (#1746). That read also asks git not to quote names, so a file
  under a directory whose name is outside ASCII is still recognised as the file
  it is, instead of being refused as one nobody asked for.
- A stopped cycle shows as stopped on the live pages, not as running and then
  lost. `cycle-end --stopped` and `--refused` set only the end on the record:
  they wrote neither the closing heartbeat the page reads nor a row among the
  ten ends, so a cycle stopped after it had sent a heartbeat was published as
  still running until it went quiet for two minutes, and as `lost` from then on
  on every publish until the issue started again. Health, the dashboard and the
  MCP sessions named a lost cycle where a person had stopped one or the
  supervisor had taken a start too late. The two ends now write the closing
  heartbeat and the ending that a reported end writes, carrying the step and
  the notice of the last heartbeat the cycle sent. `cycle-show` answers
  `heard_at` with the newest heartbeat that said the cycle was at work, so the
  closing row of an end is never the second the issue was last heard from and
  the poll still stops a container that went on after its record was ended
  (#1748).
- A stopped cycle whose docker client hangs gives its slot back after the stop
  wait instead of after 8 hours. The watch bounded the calls that stop a
  container, but not the wait on the attached `docker run` of that container: a
  dockerd that stops answering left the client alive, and the cycle kept the
  issue's lock and its slot and wrote no report until the unit's own time limit
  ended it. The watch now waits `TEHAS_STOP_WAIT` for that client, kills it, and
  says that its container may still run: the poll's reap removes it (#1756).
- After the GitHub App's slug changes, the bot's user id in `state/account-ids` is recorded under the new login before the kept login is rewritten, and the dispatcher finds its own questions by that user id, so a ticked Retry box on a parked issue is acted on again. A question on record written by another account is logged with its author (#1916).
- `tehas order` sets the order ready issues start in: `tehas order set --issues 12,8` starts #12 and #8 first, then the rest lowest number first; with `--only` the rest wait. The order is kept in the state store, so it stays through restarts until it is set again or cleared; `--for` or `--until` give it an expiry. With no order, ready issues start lowest number first. `tehas status` shows the order, `status --json` carries it as `order`, and `tehas api` lists the feature `start-order` (#1918).
- The labels `tehas:urgent` and `tehas:next`, and the settings `TEHAS_PRIORITY_FIELD`, `TEHAS_LOW_GATE` and `TEHAS_LOW_GATE_WAIT`, are gone. A line that still sets one of the three settings stops loading with 78; a blank one is ignored (#1918).
- The state store is at schema 12 (table `start_order`); install.sh migrates it (#1918).
- One reader of the times in the live files. `tehas_core::time::from_iso` is now
  the only parser of `YYYY-MM-DDTHH:MM:SSZ`, and the copies in the console's
  snapshot, the dispatcher's host read and the MCP's tools call it instead of
  each parsing on their own. It checks every separator, wants digits in every
  field with no sign, and refuses a month 13, a day past the month's last day
  (leap years included), an hour 24 and a minute or second 60. A time one
  reader took and another refused now reads the same everywhere: the readers of
  `health.json`, `energy.json`, a cycle's record and a session snapshot refuse
  the same malformed times, so a file with one is stale rather than fresh for
  some tool and unreadable for another.
- The poll reads the comments of every park it may lift in one batched call,
  beside the questions' and the blocked issues', instead of one call per parked
  issue (#1773).
- The install tests run `install.sh` from a fixture built from the checkout's
  tracked files, in the scratch directory of the run, instead of from the
  checkout itself. An untracked file in a working tree no longer changes what
  they report, and an uncommitted edit to a tracked file still does. A case of
  `test/install_fresh_test.sh` pins both (#1800).
- The console's commands (`tehas stop`, `tehas history`, `tehas export`,
  `tehas status`, `tehas state`, `tehasbot health` and the rest) call the state
  store in their own process instead of starting `libexec/store` for each read
  and write. Their output and exit statuses are unchanged, and the program stays
  installed for the scripts, the dispatcher and the cycle (#1807).
- `tehas host-message set <id> --level info|warn|alert --text TEXT [--link URL]`, `clear <id>` and `list [--json]` keep up to 20 short messages for a factory in `state/host-messages.json`, and `tehas api` lists the feature `host-message` (#1873).
- The MCP server masks the same values as the host: the secret settings and the literal model keys of `models.json`, wherever they stand in a log. The server gets salted sums of them, not the values.
- The pinned Node moves from 22.23.4 to 22.23.5.
- The pinned gh moves from 2.103.3 to 2.104.0.
- The pinned Rust moves from 1.99.1 to 1.100.0.
- `tehasbot refresh --pr N` asks for the refresh of one pull request at once, whatever `TEHAS_REFRESH_CONFLICTS` and `TEHAS_REFRESH_EVERY` say. It applies the timed run's rule, writes the request with `"asked": "pr"`, and exits 3 when the pull request is not one the factory refreshes or does not conflict. `tehas api` lists the feature `refresh-pr` (#1874).
- The messages of `tehas host-message` show in the health file as `checks.hostMessages`, in `tehas status`, in the MCP `factory_status` answer and in the front pages' banner. An `alert` or `warn` message makes the factory warn with its text, and `tehas api` lists the feature `host-message-health` (#1875).
- With `TEHAS_REFRESH_CONFLICTS` other than 1 the dispatcher starts a refresh that `tehasbot refresh --pr N` asked for (`"asked": "pr"`), in a cycle slot, and counts it in the demand of `tehas status`. A request without `"asked"` still waits, and `refreshHold` names the setting only while such a request waits. The sandbox image's hold and every other start check are as before. `tehas api` lists the feature `refresh-pr-starts` (#1876).
- Attachment links are taken only from GitHub's attachment folders. A part of
  a link's path that is `.` or `..`, or holds an escaped `.`, `/` or `\`, ends
  the link there, curl asks for the path as written, and the bot's token goes
  only with a whole attachment link. This holds for the images and files a
  cycle is given and for the Protolab's files.
- The improvement loop's analyst is given the body of an issue only when the
  issue carries `tehas:improvement` or is of the improvement type. Of any other
  issue it is given the title and labels, and the Cycles lines of its body are
  no longer read.
- MCP answers say which text comes from outside the factory: the handshake's
  instructions and the descriptions of the tools that return issue titles,
  failure causes, park reasons, console, log, event or decision text call it
  third-party text, to be read as data. An issue title is served on one line,
  without control or direction characters, and cut to 200 characters.
- Only the factory's own pull requests hold an issue. An open pull request takes `tehas:ready` off an issue it closes only when its branch is in the product repository and the bot or an account in `TEHAS_ALLOWED_LABELERS` opened it. A pull request from a fork, or by any other account, holds nothing.
- A planning turn carries only files inside its tree from one turn to the next: no part of the session's path is followed through a link, special files are left out, and one carry takes at most 1024 files and 32 MB.
- A preview's build runs with no network and in `tehas.slice`, under the slice's memory limit. A `FROM` image is still pulled; a `RUN` step reaches nothing.
- The release workflow publishes only a tag whose commit is on `main` and that
  an admin of the repository pushed. A tag on any other commit, or one pushed by
  a bot or an account without admin rights, publishes nothing.
- An edit in the second of an approval ends it. GitHub's times count whole
  seconds, so an edit or rename of an issue by another account in the second
  `tehas:ready` was applied, or a retry was ticked, now counts as after it, and
  the issue waits until it is approved again. The approver's own change in that
  second is still the text it approved.
- The scratch cap counts every file a cycle writes: a scratch under `TEHAS_SCRATCH_DIR` with more than 500,000 files and directories counts as over `TEHAS_SCRATCH_MAX`, and as the largest under `TEHAS_CYCLE_STOP_FREE`, instead of as the part that was read.

### Held back

- Node 26.13.1 is out, a new major version. `host/versions.env` keeps 22.23.5: the move is a person's call.

### Adopter action

- After renaming the factory's GitHub App, run `console ids record` (or `tehas update`) so `state/account-ids` holds the new login; until then the millwright's labels do not count.

- A pull request whose title still starts with `WIP: ` or `HALT: ` keeps it: tehas does not change it again. Take the prefix off by hand before merging if you do not want it in the merge commit. A program that read `pr-title` from `tehas api` should read the issue's labels instead.

Take `TEHAS_PRIORITY_FIELD`, `TEHAS_LOW_GATE` and `TEHAS_LOW_GATE_WAIT` out of `instance.env` and `optional.env` before updating. To keep an issue first, run `tehas order set --issues <n>`. The labels `tehas:urgent` and `tehas:next` order nothing now; delete them from the repository with `gh label delete tehas:urgent` and `gh label delete tehas:next` once no issue needs them.

A `preview/Dockerfile` whose `RUN` steps fetch from the network (a package install, a download) now fails to build. Put what it needs in the commit or in the `FROM` image. To try a recipe by hand, build it with `docker build --network none`.

## v0.28.0 (2026-11-12)

### Changes

- A write from a cycle's container that reads its body from standard input as
  `--body-file=-` is now sent once, like `--body-file -` already was. Before, a
  rate limit made the shim send it a second time after its input was used up,
  so the second write went out with an empty body (#1699).
- A poll that cannot read the bot's pull requests no longer ends. It logs `WARN: the bot's pull requests could not be read, so no cycle is stopped for its pull request this poll` once, stops no cycle for a pull request, and goes on: a running cycle whose issue is closed is still stopped, the rest of the clean-ups run, and the ready queue still starts. Before, one failed `gh pr list --author <bot>` stopped the whole poll with exit 1, so no other clean-up ran and no cycle started.
- A poll that cannot read the pull requests marked `tehas:refreshing` no longer ends. It logs `WARN: the pull requests marked tehas:refreshing could not be read, so no pull request is cleared of tehas:refreshing this poll` once, clears no mark, and goes on: the rest of the clean-ups run and the ready queue still starts. Before, one failed `gh pr list --label tehas:refreshing` stopped the whole poll with exit 1, so nothing else in it ran. (#1701)
- The analytics report credits a merge only to the pi-rukas version of the run that made it. Before, every version whose runs reached a pull request on an issue counted the merge, so an older release was credited with work it did not ship (#1749).
- The Energy use section of the dashboard no longer links a repository of
  tehas's own. The energy file names the mesh software in `source` and links
  nothing unless the host sets `TEHAS_ENERGY_SOURCE_URL` in
  `~/.config/tehas/env` to an `https://` address, which the page then links
  under `DATA SOURCE`. With no such setting the name is plain text. (#1774)
- The `--json` object of `tehas limit` has a JSON Schema in the source,
  `api/limit.schema.json`, and four samples beside it, one for each state, that
  its tests keep in step with what the command prints. A program that reads the
  object can copy the samples into its own tests (#1808).
- The manual names every file the docs server publishes under `/live/`: a new
  section of the factory API page, "Files under live/", holds one row per name,
  with the program that writes it and what reads it. The docs server now answers
  404 under `/live/` for any name that list does not hold, so a file a new
  writer puts under `live/` is not served until the table and the edge's list
  name it alike. This touches what the edge serves. Every name served today is
  in the list and still served; the two health addresses, `/health` and
  `/healthz`, keep reading the health file outside the list as they do today
  (#1809).
- When every blocker of a `tehas:blocked` issue has closed and the issue does not start in that poll, the dispatcher says once in a comment that the hold has cleared and that the label stays on until the start. An issue carrying `tehas:blocked` and `tehas:next` together no longer reads as held (#1860).
- The `--json` object of `tehas freeze` and `tehas thaw` has a JSON Schema in
  the source, `api/freeze.schema.json`, and four samples beside it that its
  tests keep in step with what the commands print: no cycle running, a freeze
  asked of a running cycle, a cycle the watch has paused, and a thaw asked of
  that one. A program that reads the object can copy the samples into its own
  tests (#1863).
- The `--json` object of `tehas history` has a JSON Schema in the source,
  `api/history.schema.json`, and two samples beside it that its tests keep in
  step with what the command prints: `api/history.sample.json`, a listing of
  three cycle ends and a planning turn with the cursor to keep, and
  `api/history.empty.json`, a factory with nothing ended. A program that reads
  the listing can copy the samples into its own tests (#1864).
- The `--json` object of `tehas panel` has a JSON Schema in the source,
  `api/panel.schema.json`, and two samples beside it that its tests keep in
  step with what the command prints: `api/panel.sample.json`, one panel set, and
  `api/panel.empty.json`, a factory with none. The same object is
  `state/panels.json` and the `live/panels.json` the dashboard reads, so a
  program that reads either can copy the samples into its own tests (#1865).
- `tehas disk --cycles` says what each running cycle writes: for every container the daemon lists under the name a cycle's container has, its issue, the bytes of its writable layer and the bytes of block IO it has written. A figure that could not be read is `null` with the reason, never 0. `tehas api` lists the new feature `cycle-writes` (#1872).
- A program that crashes during `scripts/test.sh` leaves no core file in the
  checkout. Every task of the run starts at the core limit of 0, which its
  test file and everything that file starts inherit, so the kernel is never
  asked to write a dump where a later check of the tree would read one. At the
  end of the run the checkout is read for the files the limit meant to prevent,
  so a host where the limit did not take effect fails the run instead of
  passing, and only `core` and `core.<pid>` are taken for a dump: `core.txt` is
  a file a test meant to write. `TEHAS_TEST_CORES` set to anything keeps the
  dumps and names them, which is how a person sees what a crash really leaves,
  and `TEHAS_TEST_CORE_PATTERN` stands in for the host's
  `/proc/sys/kernel/core_pattern` for such a run. A host with no `prlimit` is
  told so once and the run stops, as it is for a missing cargo. A factory's
  final verify failed on dozens of `core.<pid>` files its workspace root had
  collected this way (#1887).
- `tehas drain --json` has a published contract: `api/drain.schema.json` in the source says what the object holds and `api/drain.sample.json` and `api/drain.running.json` are samples of it, made by the command. The drain section of the manual names its fields. Nothing the command prints has changed (#1894).

### Adopter action

To keep a link under `DATA SOURCE`, set `TEHAS_ENERGY_SOURCE_URL` in
`~/.config/tehas/env` to the address of the mesh's project and restart
`tehas-energy.service`. Nothing to do to lose it: the old link was one
repository named in the platform, and it is gone for every host.

## v0.27.0 (2026-11-11)

### Changes

- The factory acts as one GitHub App, named by `TEHAS_GITHUB_APP_ID` alone. The load refuses an instance without it. The app's login is read from `state/github/login` and given to `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN`, so the millwright may be the bot itself.
- `TEHAS_GITHUB_APP_INSTALLATION`, `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN` in `instance.env` each print a warning and are ignored once the login file exists.
- `GH_TOKEN` in `~/.config/tehas/env`, `GH_CYCLE_TOKEN` and `TEHAS_CYCLE_LOGIN` are refused with exit 78. A cycle's pull requests are the app's own, so the host no longer counts a second account as the factory's.
- A cycle's `gh issue create`, `gh issue edit` and `gh issue comment` go through the host, which runs them with the app's token and refuses labels, assignees, state, another repository and any `<!-- tehas:` marker. A cycle comments only on its own issue and on issues it created, edits only issues it created, and has at most 50 such writes. The cycle's own token reads issues only.
- `tehas setup --new-app` makes the factory's GitHub App from a manifest: it prints one link for your browser, takes back the address GitHub opens, and writes the app's key and `TEHAS_GITHUB_APP_ID`. `tehas setup --app-id <id> --app-key-file <key>` takes an app you have. `--token-file` is refused; `--app-installation` is ignored, as the installation is looked up. Setup no longer writes `TEHAS_GITHUB_APP_INSTALLATION` or `TEHAS_BOT_LOGIN`, and ends with `tehasbot github-token --check`.
- A machine user's token is no longer a way to run a factory. Each refusal prints `<NAME> is no longer used: the factory acts as a GitHub App. Run tehas setup --new-app or give --app-id and --app-key-file.`
- `install.sh` keeps `libexec/github-token` while a cycle that started before the update runs, so that cycle can still renew its token, and removes it at the next run with no cycle. A new cycle renews its token with `tehasbot github-token`. The next release drops this.
- `install.sh` and `tehas update` show a setting that is no longer used as a warning and go on; a refused setting still stops them.
- A planning turn and a dev session renew their narrowed token every 5 minutes while they run (`tehasbot github-token --session`), so a session longer than an hour keeps GitHub.
- A comment of the millwright's counts as its answer to a planning question only when its first line is `<!-- tehas:millwright issue=<n> action=answer -->`. The millwright is the bot, so the host's own notes are its comments too, and they no longer count as replies.
- The narrowed token of a container has Contents and Pull requests write, Issues read and Metadata read. GitHub lets Pull requests write rename the repository's labels, so each granting `tehas:` label is pinned to its GitHub id in `state/labels`: `tehas labels` pins the labels it makes, a host from before the pins is pinned by its first poll, and after that the poll pins nothing. An application of a label with another id, or of a granting label with no pin, grants nothing and is logged.
- Once the app's login is known (the first token minted after the update), the millwright is the app. A separate millwright account loses its rights from then on: its `tehas:ready` is taken off as an unauthorised label, and its marked comments carry no grant.
- The manual's GitHub App page starts with `tehas setup --new-app`, and the pages on the host, the millwright and bootstrap describe one app for everything the factory does.
- A Retry ticked in a park comment is no longer lost when `tehas:question` is taken off and `tehas:ready` applied by hand: the tick lifts the park's hold, and the issue starts on the next poll (#1829).
- The state store moves to schema 11 and drops eighteen tables that nothing wrote or read, such as `pause_flag`, `limits` and `health_samples`. The pause, the limit and refresh requests were always kept in files under `state/`. `store check` now counts 23 tables (#1770).

### Adopter action

Set `TEHAS_GITHUB_APP_ID` in `instance.env` (`tehas setup --new-app` makes the app), take `GH_TOKEN` and `GH_CYCLE_TOKEN` out of `~/.config/tehas/env`, and take `TEHAS_CYCLE_LOGIN` out of the settings. Take `TEHAS_GITHUB_APP_INSTALLATION`, `TEHAS_BOT_LOGIN` and `TEHAS_MILLWRIGHT_LOGIN` out too, to stop their warnings.

There is nothing to configure for the millwright: the app is the millwright. Whatever acted as a separate millwright account must act as the app from the update on.

If you delete and recreate a `tehas:` label, remove its line from `~/tehas/state/labels`, delete the label, and run `tehas labels` to make and pin it.

None beyond `tehas update`. The store's migrate compacts the file after the drop, so it rewrites `state/tehas.db` once; the backup under `state/backup/` is taken before, as on every update.

## v0.26.0 (2026-11-11)

### Changes

- The pinned ctx7 moves from 0.8.2 to 0.10.0.
- `tehas update` builds the dispatcher again when the refresh help text changes. The text now lives with the dispatcher, so it counts in the source sum that decides what is due (#1694).
- A poll that cannot read the issues carrying `tehas:question` no longer ends. It logs `WARN: the issues with a question could not be read, so no question is answered this poll` once, answers no question and starts no retry, and goes on to open a planning session and start the ready queue as it does when that list is empty. Before, one failed `gh issue list` for that label stopped the whole poll with exit 1, so no cycle started either.
- CI checks the locked dependencies against published advisories: the gate downloads OSV-Scanner at the version `host/versions.env` pins, checks it against the checksum that release publishes, and fails on a known advisory in `console/Cargo.lock` or the npm lockfiles under `host/npm/` and `host/sandbox/npm/`. An advisory carried on purpose is recorded in `osv-scanner.toml` with its id, its reason and the date to look again, after which the gate fails on it until it is fixed or the date moves with a new reason (#1784).
- `install.sh --check` no longer reports `Sandbox: BEHIND` for an image built after the last status run. It read the health file, which still named the image before, so an update that rebuilt the image on a new pi-rukas release stopped at its own check until the next poll. When the image's `tehas.rukas` label is not the image the health read, the line is `Sandbox: NOT READ` and is not a finding.

### Adopter action

None.

## v0.25.7 (2026-11-10)

### Changes

- The manual links the factory floor at `/floor` on every page that names it. With the default design the floor is not at `/`, so those links sent a reader or an agent to the business page (#1696).
- The business front page shows token use in its Performance section when the factory's host sends `tokens` in `/host/bridge.json`: a table by day and one by model. `tehas api` lists the feature `host-bridge-tokens` (#1797).
- With a GitHub App, a cycle no longer needs a user account. Its container gets a second token from the app, for the product repository only, with Contents, Pull requests and Issues write, renewed while the cycle runs. Pull requests and comments from a cycle carry the app's `<slug>[bot]` login. `GH_CYCLE_TOKEN` and `TEHAS_CYCLE_LOGIN` are no longer needed with an app (#1814).
- The pinned Playwright moves from 1.64.1 to 1.65.0.

### Adopter action

With a GitHub App: nothing is required. A `GH_CYCLE_TOKEN` in `~/.config/tehas/env` is no longer used; remove it and revoke the token on GitHub. You can remove `TEHAS_CYCLE_LOGIN` from `instance.env` once no open pull request of that account is left for the factory to follow. `tehasbot github-token --cycle --check` shows that the app can make the cycle's token.

## v0.25.6 (2026-11-09)

### Changes

- `analytics.db` reads the pi-rukas result of schema 5 again. Since the result moved to schema 5, a run's usage, roles and pi-rukas version from the result were left empty (#1793).
- `analytics.db` keeps each run's tool calls and failed tool calls, in total and per role, and the driver the result says ran it (`result_driver`) beside the one the approval bound. They need pi-rukas v0.14.66 or later and are NULL before it. Analytics schema 4; a file at schema 2 or 3 is migrated in place (#1793).
- The host bridge can add up to four items to the front pages' menu, each
  opening its page in the dock as a panel does. Without a host the menu is as
  it was.

### Adopter action

A program that pins the analytics schema at 3 moves its pin to 4 before it reads a copy from this release: the export's `schema` is 4.

## v0.25.5 (2026-11-08)

### Changes

- The dashboard reads `/host/bridge.json` on its own origin, where a host that fronts the factory's domain can give a link for tickets, notices at the top of the front pages and short notes on tickets. tehas answers 404 under `/host/` itself, so a factory without such a host is unchanged. The dashboard pages unregister any service worker on their origin. `tehas api` lists the feature `host-bridge`. The contract is in the API page, "The host bridge".

### Adopter action

None.

## v0.25.4 (2026-11-08)

### Changes

- A cycle whose push or pull request failed on GitHub's side (a 5xx, a 429, a rate limit, a timeout or a connection error) is finished by a later poll with the same `tehas-cycle finish` that `tehas:finish` runs: at most 3 tries, at least 10 minutes apart, within an hour. A 4xx that is the request's own, a failed test, a review or a park still waits for the ask. The cycle's record says which end step failed and whose fault it was, and `tehas status` lists a cycle waiting to be finished (#1726).
- A refresh whose pull request conflicts with the branch no longer runs a cycle that cannot finish. pi-rukas up to v0.14.58 stops every such refresh on `consolidated-verify-conflict`: its develop step puts the pull request's own commits back onto the branch and they conflict again. On an image whose `--capabilities` does not set `features.continuePrMergeVerify`, `tehas work` asks git first whether the merge conflicts, starts no container when it does, and parks the issue as a stopped refresh, reason `refresh-conflict`, naming the conflicting files. A clean merge runs the refresh as before (#1727).
- `install.sh --check` no longer reports `Sandbox: BEHIND` for a sandbox image built from a pi-rukas release that came out after the host last asked upstream for its target: the status run asks again as soon as the image moves off the target it keeps, and an image on a later release of the line followed is ahead, not behind (#1732).
- `bootstrap.sh user` no longer builds the sandbox image with the `build.sh` of an installed tehas of another release. On a machine with an older install, as a factory whose first setup never finished, it ran that release's build, which refused the tools just installed (`sandbox: vipune is 0.20.3, versions.env names 0.15.0`), and the install stopped. It now says the installed tehas is another release and leaves the build to `install.sh` (#1743).
- `tehas freeze <issue>` and `tehas thaw <issue>` hold one running cycle still without ending it, and let it go on. The watch pauses the container. While it is frozen, neither the stall nor the time limit counts, and on the thaw the cycle has the time it had left. A program that watches the host disk can hold the cycle that writes most instead of stopping it. `tehas api` lists the feature `freeze` (#1745).
- A running refresh keeps `tehas:refreshing` when `TEHAS_BOT_LOGIN` is set without `[bot]`. Before, every poll took it off (#1752).
- Health no longer warns that the slots object is not the shape when a hold text holds a quote (#1753).
- The shell test runner (`scripts/test.sh`) is stricter. A test file that exits 0 without the count line of `t_done` fails, named `ended before t_done`, so a stray `exit 0` no longer passes the cases after it. A missing shellcheck fails the run in CI or with `TEHAS_TEST_STRICT` set; elsewhere it is still skipped, now as a `SKIPPED:` line in column 1. A file with skipped cases now counts toward the case total (#1760).
- `tehas-cycle finish` rebases in a repository of its own beside the kept workspace and pushes to the instance's repository on GitHub; it runs no git command in the workspace.
- The read-only status reads a workspace's pull request file only when it is a plain file.

### Adopter action

A factory upgrade that stopped at `Sandbox: BEHIND` right after a pi-rukas release: after updating, run the check again (`install.sh --check`, or the factory's own check) once a status run has passed.

## v0.25.3 (2026-11-07)

### Changes

- The pinned ctx7 moves from 0.8.2 to 0.9.0.
- A release is gated on the speed lane: `.github/workflows/release-gate.yml` runs the whole gate on one commit on a `[self-hosted, speedlane]` runner and builds the sandbox base image and the sandbox image with `host/sandbox/build.sh`, with a smoke run of vipune in it. `scripts/tag-release.sh` tags a commit once a run of it on that exact commit is green, starting one and waiting for it when there is none, and runs no gate of its own then; `--local-gate` runs the gate on the machine as before, and `--no-verify` runs none. A batch pull request titled `Express Lane ...` runs `verify` on the speed lane too (#1718).
- `install.sh --check` no longer calls drift what Pi 1.5.0 writes itself: a `~/.pi/agent/settings.json` that differs from the installed one only in the `defaultProvider` and `defaultModel` Pi set when a person picked a model, or in Pi's key order, passes. A key tehas sets that differs is still drift. `~/.pi/agent/mcp.json` is installed at mode 600. The generic Pi settings put `httpIdleTimeoutMs` at the top level, where Pi 1.5 reads it (Pi 1.5 ignored it under `retry.provider`); an instance `settings.json` with it nested has it moved there at install, and the cycle's `provider retry:` log line reads either place (#1723).

### Held back

- Node 26.13.0 is out, a new major version. `host/versions.env` keeps 22.23.4: the move is a person's call.

### Adopter action

A factory whose upgrade to v0.25.2 stopped at `install.sh --check` with `differs: ~/.pi/agent/settings.json` or `mode: ~/.pi/agent/mcp.json is 600, wants 644` (seen on koala's tehas-lab) passes the check after `tehas update` to v0.25.3: the update installs the new settings.json and sets mcp.json to 600. An instance with its own `settings.json` can move `httpIdleTimeoutMs` out of `retry.provider` to the top level itself; install does it either way.

## v0.25.2 (2026-11-07)

### Changes

- A first `tehas setup` on a new factory with a GitHub App no longer stops at `console ids record` with "gh exited 4". With no `GH_TOKEN`, the step makes the app's installation token from its key, as `tehasbot github-token` does, and stops with 78 when no token can be made (#1714).
- The sandbox base builds again: vipune and oo are built on Ubuntu 24.04, the distro of the image they run in, not on Debian bookworm, whose glibc 2.36 could not link the onnxruntime that vipune's embeddings download. On v0.25.1 `tehas update` stopped at the sandbox step with `undefined symbol: __isoc23_strtol`. The base build now also runs vipune and oo, so a binary that does not start stops the build (#1716).

### Adopter action

A factory that stopped on v0.25.1 at the sandbox step (`update: ... follow-up(s) are not done, recorded in .../state/update-pending`) runs `tehas update` again after updating to this release; it runs the pending follow-ups, `sandbox` among them, and `update-pending` empties. A factory that did not stop has nothing to do.

## v0.25.1 (2026-11-07)

### Changes

- A cycle whose pull request is merged, but whose runner was stopped mid-step (a lens-fix fixer killed by the end of the cycle, say), is now recorded as `merged` with exit status 0, not with the runner's exit. The runner's own exit is still kept and named in the issue comment (#673).
- A cycle's record for the improvement loop keeps the whole reason it ended with, up to 2000 characters, where it was cut at 200 mid-word; a longer one ends ` (cut)`. Its new `handoffDelivered` says whether pi-rukas's handoff reached a comment on the issue or its pull request (`null` with no handoff), and a handoff that reached none is a `WARN` line in the cycle's log (#718).
- With `TEHAS_OVERLAP_HOLD=1` the dispatcher holds a start while a running cycle's branch changes a path the issue's body names, and logs `wait #N: a running cycle on #M changes <path>`. Off by default (#725).
- Troubleshooting has an entry for the `verify-baseline-red` park: what it means, how to find the red case when pi-rukas names no assertion, and `TEHAS_BRANCH_CHECK`, which holds new cycles while the branch is red instead (#751).
- A cycle pi-rukas ended at a child's run bound (`child-timeout:<step>`) is on the improvement loop's record with the status `child-bound`, not `aborted`, so it is not read as an abort (#752).
- The manual says how `TEHAS_VERIFY_TIMEOUT`, the gate's wall time on a loaded host and `TEHAS_TICKET_CONCURRENCY` have to agree, how to read the gate's longest runs from `analytics.db`, and what this repository's own gate takes under load (#803).
- A cycle's `manifest.json` says in `linksUnavailable` why it lists no image: the text links no file, or the instance gives a cycle none. It is null when every link is listed (#822).
- A drafted plan whose title only names the issue it plans (`feat: Plan issue #608`) is refused like `EPIC: EPIC`: Update this issue keeps the person's title and body and the comment says why (#1015).
- The sandbox image is built from a lean base that `sandbox/build.sh` makes on the host from the pi-rukas clone at the commit followed (`sandbox/base.Dockerfile`), instead of upstream's devcontainer image. The user is made at the tehas user's uid in the first layer, so the remap no longer stores every file twice, and Playwright brings only its headless shell. The image went from 11.5G to 5.6G on nessie and 3.8G in a local build of v0.14.54 (#1548).
- A host keeps the sandbox images of two builds, the current one and the one before it, whatever their age; `tehasbot clean` no longer waits for `TEHAS_CLEAN_AFTER` to take older ones (#1548).
- A directory that holds cargo's `CACHEDIR.TAG` is build output whatever its name, at a cycle's end and in `tehasbot clean` (#1548).
- What a closed issue left on the host goes at the next poll; an open issue's kept workspace waits `TEHAS_WORKSPACE_KEEP_DAYS` days, 7 by default. The tree a retry moved aside goes when that retry ends (#1548).
- A pull request adds its changelog lines in `changelog.d/<issue>.md`, with `### Changes` and `### Adopter action`, instead of editing `CHANGELOG.md`, so two open pull requests no longer conflict there. With no issue the file is named after the branch, each `/` a `-` (`scripts/collect-changelog.sh --name <branch>` prints it). `scripts/release-notes.sh Unreleased` shows the fragments' lines in the Unreleased section, ordered by issue number. `scripts/tag-release.sh` counts their Adopter action, shows the collected section with `--dry-run`, and before it tags has the head's own `scripts/collect-changelog.sh` move them into the release's section in a pull request from `changelog-<tag>`. A tagged release's notes are unchanged (#1606).
- A factory run by tehas-industry shows the industry's alerts about it: paused too long, frozen, or held still by one ticket. `tehasbot health` reads `state/industry/alerts.json` into `checks.industry.alerts` and warns with the first alert's text, so `tehas status` and the MCP `factory_status` say it, and the factory floor and the plain dashboard show a banner at the top while an alert is current (#1642).
- The status tests expect the sandbox image of the user who runs them, so `tehas setup`'s product check passes on a factory that builds tehas under a user other than uid 1000 (#1644).
- `host/sandbox/build.sh` no longer refuses every instance `sandbox.Dockerfile` as "not built on" the image of the run: it now reads the image layers one id per line, where it read Docker's one bracketed line as a single layer (#1653).
- The factory finishes a stopped cycle when the issue asks: a comment that starts with `<!-- tehas:millwright issue=<n> action=finish -->` and gives the reason, then the new label `tehas:finish`, from an allowed labeller or from the millwright within its grant. The poll takes the label off, runs `tehas-cycle finish`, and comments with the draft pull request's URL or the one line of why not. `tehas api` lists the feature `finish`, and `factory_decisions` answers each ask taken as the decision `finish`. `tehas update` creates the label (#1665).
- A new instance setting, `TEHAS_DESIGN`, draws the whole site in one design: `beesknees26`, the default, or `pixel`. With no setting every page is beesknees26, `/` included, with the beesknees26 logo and share card, and `/` is the business page. `TEHAS_DESIGN=pixel` draws every page in pixel, the manual, `/business`, `/dashboard` and the Protolab included, with the animated factory and its share card, and `/` is the factory floor. `/floor` and `/business` are always there in either design. Any other value stops every command with 78 (#1668).
- `TEHAS_THEME=beesknees26` and `TEHAS_LAYOUT=business` now ask for the default and load as no-ops, as an empty value does. `TEHAS_THEME=pixel`, `TEHAS_LAYOUT=floor` and every other value of the two retired settings stop every command with 78 and say to use `TEHAS_DESIGN` (#1668).
- The pinned Pi moves from 1.4.0 to 1.5.0.
- The pinned rustup moves from 1.29.2 to 1.29.3.
- The pinned vipune moves from 0.15.0 to 0.20.3.
- The pinned oo moves from 0.5.0 to 0.10.1.
- The pinned Caddy moves from 2.11.7 to 2.11.9.
- A new setting, `TEHAS_ISSUE_TYPES`, lets the organisation's issue types mark the kinds `improvement` and `prototype` in place of the `tehas:improvement` and `tehas:prototype` labels. The improvement loop files its issues with the type, the dispatcher holds an issue of the type as it holds one with the label, and the Protolab lists the issues of the type set by an allowed account. A type the organisation does not have stops each with 78. Unset, the labels work as before (#813).
- `tehas status --json` has a `versions` key: the tehas release the host runs against the newest one, and the pi-rukas version the sandbox image runs against the one the host follows, each with `behind`, read from the health file and `null` where it cannot be read. `tehas api` names the feature `status-versions` (#1680).
- `TEHAS_EGRESS=1` limits where a cycle, a planning turn, a review and a `tehas dev` session can connect: they go on an internal Docker network whose one way out is a proxy that allows only GitHub, the npm and crates.io registries, the endpoints in models.json and the hosts of `TEHAS_EGRESS_HOSTS`. Off by default (#417).
- The host's gh shim sends a write that GitHub refused for a rate limit once more after a minute, and only that call, unless it was typed at a terminal, as the container's shim does. `tehasbot github-writes` shows the GitHub writes of the last day by hour, with the busiest minute, against GitHub's limits (#649).
- A cycle's container, every agent in it and the push check before it, is given the cycle token `GH_CYCLE_TOKEN` from `~/.config/tehas/env` as its `GH_TOKEN`, never the host's own token or the GitHub App's. The app's token directory is no longer mounted into a cycle. Without `GH_CYCLE_TOKEN`, `tehas work` exits 78 before it calls GitHub, Docker or git (#746, part of #51).
- `TEHAS_CYCLE_LOGIN` in `instance.env` names the account of the cycle token. The host counts it as the factory's own beside `TEHAS_BOT_LOGIN`: the cycle's report finds the cycle's pull request by it, the poll stops a cycle whose pull request is merged or closed, the refresh refreshes its conflicted pull requests, a comments retry reads its pull requests and leaves out its comments, and `console pr-title` keeps its titles. With a GitHub App, `tehas work` exits 78 without it. The load refuses it in `TEHAS_ALLOWED_LABELERS`, or as `TEHAS_MILLWRIGHT_LOGIN`.
- A GitHub read that fails on a bad minute (a 5xx, a bare 403, or its time bound) is tried once more in `tehasbot status` and `tehas tidy` too, as in the poll, and the poll now waits its second before the retry rather than after it (#823).
- `tehasbot status` and `tehas tidy` name gh's error in their WARN lines, and `tehasbot status` writes its snapshot when a read fails: the part that read fed is kept from the file before and named in `stale`, so the pause, all-hands and the rest stay current (#823).
- Health counts the GitHub calls that failed in the last ten minutes (`checks.github`) and warns from three; `tehas status` prints a `github` line with the count and the last error (#823).
- `TEHAS_READ_ONLY=1` makes an instance that writes nothing to GitHub. It reads issues and code and runs cycles as usual. Every write of the host's `gh` and of a cycle's container is refused and said as `read-only: would ...`, a cycle's branch stays in its workspace with the pull request it would have opened, and the dispatcher keeps its starts in `state/read-only` instead of the labels. `tehas status --json` says `"readOnly": true` and the dashboard lists the results. Off by default (#608).
- `tehas state copy --to <file> [--json]` writes a copy of the state store as it is at one moment to a new file, without stopping a running cycle, and says the copy's schema (the number `tehas state check` reports), its rows and the time it took. It refuses a name that is taken, a directory that is not there and one under `live/`, in one line, and writes nothing else. `--json` prints the object of `api/state-copy.schema.json`, and `tehas api` lists the feature `store-copy` (#1707).
- `tehas state load --from <file> [--json]` fills a drained factory's new or empty state store from a copy made by `tehas state copy`, the inverse of the copy, to restore a factory on a new host. It refuses, in one line and with the store as it was, a factory that is not drained or runs a cycle, a store that holds history, a copy that is not there or does not pass its integrity check, and a copy of a newer schema, naming both numbers; a copy of an older schema is brought forward as an update would. The rows are loaded in one transaction. `--json` prints the object of `api/state-load.schema.json`, and `tehas api` lists the feature `state-load` (#1710).

### Held back

- Node 26.13.0 is out, a new major version. `host/versions.env` keeps 22.23.4: the move is a person's call.

### Adopter action

Run the query in "The gate's bound and the cycles at once" on the operating page. When the longest run of the gate at the cycles you run is near `TEHAS_VERIFY_TIMEOUT` or above it, raise the bound or lower `TEHAS_TICKET_CONCURRENCY`, and write the measurement beside the number in `instance.env`.

The next `tehas update` builds the base on the host, which needs the pi-rukas clone in `~/tehas/repos/pi-rukas` that `bootstrap.sh user` keeps, and network for the base's downloads. The upstream `ghcr.io/.../pi-rukas:sha-*` images the host pulled before go with `tehasbot clean` once they are `TEHAS_CLEAN_AFTER` old; other images outside the rule, such as a `pi-rukas:latest`, are removed by hand. Set `TEHAS_WORKSPACE_KEEP_DAYS` in `instance.env` to keep stopped workspaces for fewer or more days.

tehas.app keeps the pixel design with one line that tehas-settings adds to `hosts/tehas-tehas/instance.env`: `TEHAS_DESIGN=pixel`. The docs server reads it when its container is created: add the line before the update, or run `docker compose -f ~/tehas/edge/docs.compose.yaml up -d --force-recreate` after adding it. The Protolab's pages are drawn again at its next pass. Other hosts need nothing: they get beesknees26 everywhere, `/` included. A host that still has `TEHAS_THEME=pixel` or `TEHAS_LAYOUT=floor` takes the line out, and sets `TEHAS_DESIGN=pixel` if it wants the pixel design.

To turn it on, set `TEHAS_EGRESS=1` in `instance.env` after `install.sh`, and add the hosts the product's gate fetches from beyond GitHub, npm and crates.io to `TEHAS_EGRESS_HOSTS`. `docker logs tehas-egress` names every host it refused. It needs Docker 27.1 or later: an older Docker stops the container with 78.

Before you update, make a fine-grained token for the bot account, for the product repository only: Contents, Pull requests and Issues read and write, Metadata read, nothing else. Put it in `~/.config/tehas/env` as `GH_CYCLE_TOKEN=...`. A host updated without it starts no cycle until it is there. Commits, pull requests and comments made from inside a cycle then carry the bot account's name, not the app's. With a GitHub App, also put `TEHAS_CYCLE_LOGIN=<that account's login>` in `instance.env`, or no cycle starts.

Nothing for an instance that does not set it. To run one read-only, run `install.sh` first so the sandbox image has the `gh` that refuses writes, then set `TEHAS_READ_ONLY=1` in `instance.env`.

## v0.25.0 (2026-11-06)

### Changes

- `lib/config.sh` keeps what a login shell, `install.sh` and an update need: `tehas_load`, `tehas_instance_dir` and `tehas_require_instance_settings`, which now asks the console (`console settings require`). `tehas_require` and `tehas_models` are gone; `install.sh` asks `console models used` for the models the agents use (#1590).
- `tehas status --json` says where each running cycle's step is: `stepNumber`, `stepTotal`, `round`, `roundLimit` and `stepSince` on each item of `running`, each left out when not known (#1579). The round is counted from the step's `step-started` lines when the driver gives none; the number, the count and the limit appear once the driver puts them on those lines. A line of `tehas history --json` carries `stepNumber`, `stepTotal` and `round` of the step the cycle ended in, and `prototypes` lists the issues with a Protolab page and when each was built. The state store is at schema 10: `store migrate`, which `install.sh` runs, adds the columns.
- `tehasbot cycle`, `tehas work` and `tehas index` have no script of their own: the console loads the settings and execs `tehas-cycle`, which takes the lock (`state/cycles/<issue>.lock`, `state/index.lock`) and hands it down to the programs it starts, the docker client among them, with rustix's safe `fcntl_setfd` (#1587, #1588). The waits and exit statuses are as before: a second cycle of an issue waits 5 seconds and exits 3, a second `tehas work` exits 5, a second index run exits 3. The dispatcher starts the index as `bin/tehas index --before-cycle`. `install.sh` removes `libexec/cycle`, `libexec/work` and `libexec/index`.
- `tehas dispatch`, `plan`, `dev`, `check-product` and `tehasbot dispatch`, `review`, `improve`, `github-token` go from `bin/tehas` and `bin/tehasbot` straight to the console. Their seven scripts in `libexec/` are gone and `install.sh` removes them; `libexec/github-token` stays until no cycle that started before the update runs, since such a cycle renews its token through it. The gh shim, the poll and a cycle's token renewer run `bin/tehasbot github-token`, and a planning turn runs `bin/tehas plan` from its copy of the code. `libexec/dev-login` stays: the sudo rule of `bootstrap.sh dev` names that one file (#1586).
- `install.sh` asks the console for the checksum of each program's source (`console update sources`), read from the workspace's members and what each takes by path, in place of seven `find | sha256sum` blocks with their own filters (#1592). The attachment checker is now built again when `console/core/` changes, which it links. The first install after this builds each program once, since `tests/` and `src/tests.rs` of every crate are now left out of its source.
- `install.sh` asks the console what is wrong with the instance (`console update findings`) in place of its own stat and jq checks (#1592): settings another account can write, the load and the settings an instance has to set, and `models.json` against the models the agents and the product's session use, with the console's own rule for the keys. The findings and their words are as before; a `providers` that is not a map of provider names is now one too, as it already was for `console models keys-ok`.
- `install.sh` asks the console for Pi's `settings.json` and `models.json` (`console update pi`) in place of two jq programs (#1592). The defaults and what an instance gives are as before. The keys are now written in sorted order, so the first install after this copies both files once; Pi reads them the same. A model list or settings file that is JSON but not shaped as Pi reads it (a provider or model that is not an object) is named as such rather than as not valid JSON.
- `install.sh` asks the console for the rest (#1592): the files an older tehas installed and this one removes are a data file, `console/src/update/retired.list` (`console update retired`), in place of a list in the script; the `--check` report is `console update report`, and the notes a real run ends with for the slice and the sandbox image are `console update notes`, in place of `slice_state`, `images_say` and two jq programs. The lines and the exit status are as before. A `state/check-product.json` with no `failures` list is now one `Product: NOT READ` line, where it was a line made from it and then that one.
- What a release packs is the plain list `lib/release.list` (sections `[dirs]`, `[plain]`, `[extra]`), read by `scripts/pack-release.sh`, in place of the bash library `lib/release.sh`. `install.sh` installs the list and removes an installed `lib/release.sh` (#1585).
- The health file has `checks.rukas`: the pi-rukas commit the sandbox image runs (its `tehas.rukas` label) and its version, the commit the host follows and its release, the commit of the host's pi-rukas clone, and `behind` (#1534). An image behind the commit followed makes the health `warn`, and `install.sh --check` says `Sandbox: BEHIND`. `bootstrap.sh user` on an installed host ends with `sandbox/build.sh --if-moved`, so moving the clone moves the image. Before, a host on `PI_RUKAS_FOLLOW=release:0` could move its clone to a new release while cycles kept the old image, and nothing said so.
- The health file has `checks.industry` when the factory is run by tehas-industry, from `TEHAS_INDUSTRY_VERSION` in `~/.config/tehas/env`. A factory without it, or with a value that is no version, has no block and is decided as before. The MCP `factory_status` answer carries `rukas` and `industry` too.
- The footers of the two front pages read their versions from the health file (`tehas v0.24.6 · pi-rukas v0.14.54 · industry v0.9.0`), not from `fun/version.txt` and `fun/rukas-version.txt`. The image build no longer writes `rukas-version.txt`.
- `tehas state check [--json]` prints the state store's schema, integrity check and rows of each table, with `--json` the store's own object, and `tehas api` lists the feature `state-check`. A program that moves a factory read it from `~/tehas/libexec/store check`, a path under the home that may change in any release.
- A planning turn no longer offers a drafted plan that the checks of the issue's text refuse (#1553). The comment still shows the plan, and the question names the rule and lists the open points, with only **End this planning session** to tick. Two rules are new: a plan with lines that end in `[context param]` (the request or tehas's briefs echoed back) and a plan whose Acceptance criteria are pi-rukas's placeholder are refused, on the tick of **Update this issue** and for a plan with no open points too. The planner is given the issue's title without its conventional-commit type, so a drafted title no longer reads `chore: chore:`, with the type given as the planner's `--type` (`chore: ...` is planned as a chore, `research: ...` as a spike), and the `criteria` of a request's `pi-spec v1` block as an `ACCEPTANCE CRITERIA:` block, which pi-rukas carries into the draft.
- The retry boxes of a park say what a retry is given: `Retry with the issue and all its comments` and `Retry with the issue text only`, with a line on what each means. A tick of the comments retry records the text of every comment on the issue and on the bot's pull requests for it (conversation comments, review bodies, inline review comments) up to the tick, by an account of `TEHAS_ALLOWED_LABELERS` that may write to the repository then, the bot's own left out; `tehas work` writes them to `comments.json` in the format of trail-openers/pi-rukas#2020 and mounts it even with images and files off. It no longer needs `TEHAS_WORK_COMMENTS`, and `retry-context.md`, which nothing read, is gone. The comments box is offered only when the sandbox image lists `issueComments` or carries the issue-comments polyfill. The old labels still count on comments posted before; they go two releases from now (#1492).
- A start whose approval says driver `lean` (`tehas:lean`) runs `pi-rukas-headless work-lean` instead of being refused, and `tehas work <issue> --driver lean|classic` picks the driver by hand. The image is checked for the `work-lean` flags, `leanHandoffHost`, `leanDeadline` and `workLeanResultSchema` 5; one without them, or a lean restart or refresh, is 78 and parks, and classic never runs in lean's place. `.pi/lean-state/` is excluded in a cycle's workspace.
- The health file has `checks.mcp`: whether the MCP server is on, installed and answers a `POST /mcp` with no key (#1418). A server that is on and does not answer makes the health `warn`, so `/health` tells a stopped MCP server from a factory that is gone.
- An allowed account is its login and its GitHub user id, not the login alone. `install.sh` and `tehas update` record the id of each login of `TEHAS_ALLOWED_LABELERS` and `TEHAS_MILLWRIGHT_LOGIN` in `~/tehas/state/account-ids`, and a label, retry tick, planning answer or comment counts only when its account carries that id. A login that was renamed and registered by someone else authorises nothing. A login that does not resolve, or whose id changed, stops both with 78 and names it; `tehas update --accept-id <login>` records the new id (#1493).
- `bin/tehas` and `bin/tehasbot` are a few lines that run the console, which chooses the command, prints the usage and refuses an unknown one. With no console every command but `tehas setup` stops with 78 and one line, `tehas: no console at <path>: run install.sh`; `tehas top` and `tehas export` ended 1 there before. `tehas index` asks `tehas-cycle` for the checkout and the branch, and `lib/config.sh` loses `tehas_checkout` and `tehas_branch` (#1346).
- Health says when the cleaner's hourly upkeep failed to make a backup: `There is no backup of the state store: the last upkeep failed at <time>: <word>.`, and `checks.upkeep` carries the last upkeep's time, result and the store's word. The cleaner records it in `state/upkeep.json`. Before, health told the reader to wait for the cleaner that was already failing (#1419).
- The dispatcher reads blockers from GitHub's native blocked-by links (#811). An issue with an open blocked-by link is not started, whatever label asks for it, and the log says `wait #N: a blocked-by link holds it`. A link only holds: once every blocker is closed the issue starts under the usual approval rules, so it still needs `tehas:ready` from an allowed labeller. The `<!-- tehas:blocked by=N -->` marked comment keeps working for this release beside the links; a forge answer without the field holds nothing by links. See [Ordering the queue](/operating).
- `TEHAS_PRIORITY_FIELD` (unset) lets an organisation's single-select issue field order the queue in place of `tehas:urgent` and `tehas:next` (#812): `Urgent` counts as `tehas:urgent` when an allowed account made the newest change to the field, and `High` as `tehas:next` when an allowed account or the millwright did. On a repository of a personal account, which has no issue fields, the labels order the queue as before. A field an organisation does not have stops the poll with 78. Unset, the labels order the queue as before.
- The manual: [The forge interface](/forge), what tehas asks of the site its issues live on, as a contract for a second adapter: each term of the trait, its GitHub mapping, and what a forge must offer at minimum (#815).

- The business page's header shows the logo (`/fun/favicon.svg`), as the manual's header does, in place of the old mark it still drew beside the new favicon.
- A new optional setting, `TEHAS_OWNER`: one person's GitHub login. When the dispatcher marks an issue `tehas:running` for a cycle, the same call assigns the issue to that account, so GitHub shows it as taken (#806). An assign GitHub refuses does not fail the claim: the issue is marked running without it and `logs/dispatch.log` says so. A bot's login or more than one login stops every command with 78, and `tehas setup` refuses a login GitHub cannot assign on `TEHAS_REPO`. Unset, nobody is assigned, as before.
- The shell tests use the workspace's build, in `CARGO_TARGET_DIR` or `console/target`, instead of a second one in `console/target/shell`, which `scripts/test.sh` now removes. Each checkout's build is about 170 MB smaller (#1360).
- `scripts/select-tests.sh`, the selection behind `scripts/verify.sh --changed`, is a short exec of `xtask select-tests` in a new workspace crate for development tasks, `console/xtask` (`tehas-xtask`), with its cases as Rust tests. It selects what it selected before; a change under `console/xtask/` asks for the full gate, as a change to the script did (#1595). Nothing in it is installed.
- `scripts/pack-release.sh` is a short exec of `xtask pack-release` (`console/xtask/src/pack.rs`), with its checks as Rust tests. It packs what it packed before, with the same messages and exit statuses; it now needs cargo, which the release runner has, or a build named in `TEHAS_XTASK` (#1595).
- `scripts/tag-release.sh` is a short exec of `xtask tag-release` (`console/xtask/src/tag.rs`), with its decisions as Rust tests: the next version, the remote's head, the changelog's section and its Adopter action, which polyfills upstream still works on, and the pull requests' text. It tags, holds and refuses as before, with the same messages and exit statuses, and still runs git, gh, docker and the head's scripts. It no longer runs `scripts/release-notes.sh` (#1595).
- `scripts/test.sh` is a short exec of `xtask test` (`console/xtask/src/run.rs`), with its decisions as Rust tests: the pool's size, the load wait, the order of the files, the case count, the slowest files and the block of what failed. It runs, prints and exits as before; each check runs as an `xtask test-task` process that records its status. The runner now needs cargo, or a build named in `TEHAS_XTASK` (#1595).

- An open issue whose title starts with an epic marker (`EPIC: ...`, `[Epic] ...`) gets the new `epic` label at each poll. tehas only adds it, and a person's removal is final. The label is for people: sub-issue links still make an epic for the dispatcher and the panel. `live/epics.json` gains `unlinked`, and `tehasbot health` warns about an open issue labelled `epic` with no sub-issues (`checks.epics`).
- An ending never carries a word that says the cycle is still at work. `store
  cycle-end` records `running` or an empty status as `unknown` in `endings` and
  in the heartbeat of that end, and the health page names such a row, from a
  file written before this, as `not an end` instead of listing a finished run as
  a failure that is still running. The two words and their mapping are one set
  in `tehas_core::grammar`, read by both sides (#1417).
- `tehas panel set <id> --label TEXT --url URL` names a page of the project, such as a support chat or a status page, and both front pages show it: an item in the menu opens it in a sandboxed frame in a dock at the side. Up to four, published to `live/panels.json`; `tehas api --json` lists the feature `panels`.

- `scripts/tag-release.sh` moves every pinned tool to the newest minor and patch of its major before it tags: `host/versions.env`, `host/npm/` and `host/sandbox/npm/`, each with its checksum, digest or lockfile, from the tool's own source (`scripts/pins.sh`, `console pins`). When anything moves it opens a pull request on `pins-<tag>` and tags after that is merged; a later major is listed under `### Held back` for a person. A move that fails stops the release; `--no-pins` skips it. The release that makes the move needs cargo, curl, tar and npm where it runs.
- `tehas update` runs in the console from start to end: `console update run` holds `state/update.lock`, runs install.sh with the lock's descriptor (still named by `TEHAS_UPDATE_LOCK_FD`), and has the console install.sh left, or `libexec/console.prev`, finish the run. `libexec/update` only checks that the console answers and execs it. The verbs the script of the release before calls one by one (`update args|wait|gate|prepare|api|finish`) keep working for this release (#1589).

### Adopter action

A park's comment offers the comments retry only on a sandbox image whose
pi-rukas reads the comments blob (trail-openers/pi-rukas#2020) or that carries
the issue-comments polyfill; until then it offers `Retry with the issue text
only` and `Leave it parked`. Nothing to set. A comments retry now asks GitHub
for each author's permission on the repository, which the bot's token already
may read.

None for the pi-rukas and industry blocks: `tehasbot status` writes
`live/rukas.json` at its next run. A `~/tehas/fun/rukas-version.txt` an older
build left can be removed by hand.

To run lean, rebuild the sandbox image on pi-rukas v0.14.47 or later. A
lean start on an older image parks with what the image lacks.

Run `tehas update` (or `./install.sh`) once with GitHub reachable: until the
user ids are recorded, no label, tick, answer or comment of an allowed account
counts, and the poll says so on standard error. If it stops with 78 on a login
whose id changed, check that the account is still the person you mean, then
run `tehas update --accept-id <login>`, or take the login out of the settings.

Name an issue's blockers with GitHub's blocked-by links from now on. The `<!-- tehas:blocked by=N -->` comment still counts in this release and stops counting after it.

To have claimed issues assigned, set `TEHAS_OWNER` in `instance.env` to a person's login with access to the repository. Nothing to do otherwise.

The `epic` label is new in `host/labels.tsv`, so an update runs its `labels`
step; a host that skips it runs `tehas labels` once. Until the label exists,
the poll's write of it fails with a `WARN:` line and nothing else changes.

## v0.24.7 (2026-11-05)

### Changes

- A cycle's scratch has a size limit (#1547). With `TEHAS_SCRATCH_DIR` set, tehas work measures the cycle's scratch every 10 seconds and stops the cycle when it holds more than `TEHAS_SCRATCH_MAX` (20g by default), or when the scratch's file system has less than `TEHAS_CYCLE_STOP_FREE` free (5g by default) and this cycle's scratch is the largest there. The cycle ends `stopped` at the cap `disk`, and its report says which bound it passed. Before, one cycle that copied its container's root into its scratch filled the host's disk and stopped the dispatcher and the other cycles.
- `tehas status`, `tehas status --json` (`held`) and the MCP `factory_status` answer (`held`) name each ready issue the dispatcher holds on its own, with the reason and what clears it: for now, an issue whose last cycle parked and whose body has not changed since (#1437). When every issue in line is held that way and no cycle runs, the health line is a `warn` that names them instead of "Idle, taking work.". Before, a factory whose one ready issue was held reported `pass` and taking work for hours.
- `scripts/verify.sh` checks the workspace's format and lints on every run, not only with `--full`; in CI a missing rustfmt or clippy fails. `tehas setup` runs `--full` on a factory that builds tehas, so unformatted code that passed CI stopped setup three times. `console/dispatch` is formatted again (#1486).
- A cycle that hands off on `lens-fix-reposition` with its pull request open and its review not held waits for a merge (`tehas:awaiting-merge`) instead of parking. The branch already holds the first review round's fix; only the second round's could not be laid on it. It was the most common park from 2026-10-31.
- `console flags pause` takes only `set` and `clear`: `show` and `read`, which nothing called, are gone (#1365). The dashboard's reader of the flag uses the same reading as the health file.
- The console's cycle report reads the stalled and timed stop files once each, so a file whose first word is no count of seconds names neither a stop nor a step (#1207). Before, the two reads of one file could disagree and report a stalled cycle with a step of an unreadable file.
- A factory pull request that waits for a person is titled `HALT: ` instead of `STUCK: `. The next poll renames the open ones.
- `tehas drain` prints its answer when a poll holds its lock for the whole wait: `not drained:`, or `drained: false` with `--json`. It printed nothing, which stopped `industry factory upgrade` with `unreadable factory answer`.
- The libraries no command sources are gone: `lib/attachments.sh`, `cycles.sh`, `labels.sh`, `millwright.sh`, `questions.sh`, `sandbox.sh`, `slice.sh`, `store.sh` and `history-lines.jq`. What still had a caller is in the console or the store now, and `install.sh` removes a host's copies in its stale-file step.
- The host's pinned tools are back to what v0.24.x released: Node 22.23.4, gh 2.103.3, bun 1.4.4, jq 1.8.2, Pi 1.4.0, ctx7 0.8.2, parallel-cli 0.9.4, Playwright 1.64.1, Rust 1.99.1, rustup 1.29.2, codebase-memory-mcp 0.12.1, Caddy 2.11.7 and CI's actions/checkout v7.0.1, each with its checksum, digest or lockfile. A merge had put the earlier pins back in `host/versions.env`, the npm trees beside it, the Caddy digests and the two workflows (#1475).
- The site is two frontends with two pinned looks. The front page (`/`, `/floor`) names `theme-pixel.css` and every other page names `theme-beesknees26.css`, each with its own logo and share card, so no server rewrites a look. `TEHAS_THEME` and `TEHAS_LAYOUT` are retired, the `paper` and `beesknees` themes are gone, the front page is always the factory floor and `/business` is always the business page. The footers no longer link from one frontend to the other (#1488).
- `analytics event --at <t>` keeps the row when the time is a little ahead of the host's clock: within an hour it is stored at now and said on standard error, because the dispatcher passes the time GitHub recorded for the label. Beyond the hour it is still a usage error and no row is written.
- The `sizes` figures count no lead for a ship whose `merged` is stamped before the `ready` behind it, as `refine` already counts it; a negative median lead no longer reaches the report.
- `store prune` also removes the cycle and plan history of an issue whose cycle ended more than its window ago, keeping that issue's last row of each of the two tables, so the state store no longer grows with every issue the factory has ever closed. `store upkeep`, what `tehasbot clean` calls once an hour, prunes with a window of 90 days (`PRUNE_DAYS` in `console/store/src/upkeep.rs`). `tehas history` still answers for a long-closed issue with the line it keeps, and an issue whose cycle ended without a history line of its own keeps all of its rows (#1414).

### Adopter action

`console flags pause show` and `console flags pause read` are gone: a script
which called one of them now gets 64 and the usage. The words of the flag are
what `tehas status` prints, and the object is what `tehasbot status` publishes
in `live/status.json` under `pause.flag`.

A filter, script or search that looks for `STUCK: ` in pull request titles
now looks for `HALT: `.

On a host that sets `TEHAS_SCRATCH_DIR`, a cycle whose scratch holds more
than 20g, or that has the largest scratch when its disk has less than 5g free,
is now stopped at the cap `disk`. Set `TEHAS_SCRATCH_MAX` or
`TEHAS_CYCLE_STOP_FREE` in the instance if a product needs more.

Take `TEHAS_THEME` and `TEHAS_LAYOUT` out of `instance.env`. A line that asks
for what this version does anyway (`TEHAS_THEME=pixel`, `TEHAS_LAYOUT=floor`,
or an empty value) loads and is dropped; any other value stops every tehas
command with 78 and names the line. A host that had `TEHAS_LAYOUT=business`
now has the factory floor at `/` and the business page at `/business`. Run the
two `docker compose ... up -d` lines again: neither server is given those
settings now.

## v0.24.6 (2026-11-04)

### Changes

- A cycle that hands off on `lens-fix-reposition` with its pull request open and its review not held waits for a merge (`tehas:awaiting-merge`) instead of parking. The branch already holds the first review round's fix; only the second round's could not be laid on it. It was the most common park from 2026-10-31 (#1482).
- A cycle's run log past 200 KB is read from its last 200 KB, and the report says what was left out. Such a cycle is parked with its retry boxes rather than handed off, since a log read from its end cannot say nothing came after its last step (#1474).
- A plan that writes over an issue's body keeps the issue's `pi-spec v1` block. A new `tehas:lean` start whose issue has no single valid `pi-spec v1` block starts no cycle: the poll takes `tehas:ready` off and says why once (#1463).

### Adopter action

None. An issue labelled `tehas:lean` needs its `pi-spec v1` block before
`tehas:ready` starts a cycle.

## v0.24.5 (2026-11-03)

### Changes

- `tehas-cycle`'s report reads each stop file of a cycle once, so a file whose first word is no count of seconds can no longer name a stop in one read and a step in another (#1388).
- A test of `tehas-cycle`'s watch waits until the script it wrote can run. On a loaded host it failed now and then, which stopped `tehas setup`'s sandbox gate (#1470). The wider fix is #1471.

### Adopter action

None.

## v0.24.4 (2026-11-03)

### Changes

- `tehas-cycle` writes its standard output as the workspace's other programs do, so `scripts/verify.sh --full` passes its lints again. v0.24.3 failed them, which stopped `tehas setup` at `check-product --sandbox` on a host whose product is tehas.

### Adopter action

None.

## v0.24.3 (2026-11-03)

### Changes

- With `TEHAS_MILLWRIGHT_EPICS=1`, an allowed labeller's `tehas:ready` on an epic approves its open sub-issues for the millwright, which may then plan and start them on that approval (#1465). Off unless set.
- A plan draft that fails the text check changes nothing on the issue (#1394).
- A GitHub read that gives up at its first attempt says why it stopped in the poll's log (#1464).
- The libraries no command sources are gone: `lib/attachments.sh`, `cycles.sh`, `labels.sh`, `millwright.sh`, `questions.sh`, `sandbox.sh`, `slice.sh`, `store.sh` and `history-lines.jq`. What still had a caller is in the console or the store now, and `install.sh` removes a host's copies in its stale-file step (#1400).

### Adopter action

None. To let a person's `tehas:ready` on an epic approve its parts for the
millwright, set `TEHAS_MILLWRIGHT_EPICS=1` (see `instances/example/optional.env`).

v0.24.3 fails `tehas setup` on a host whose product is tehas: use v0.24.4.

## v0.24.2 (2026-11-03)

### Changes

- `tehas drain` prints its answer when a poll holds its lock for the whole wait: `not drained:`, or `drained: false` with `--json`. It printed nothing, which stopped `industry factory upgrade` with `unreadable factory answer` (#1456).

### Adopter action

None.

## v0.24.1 (2026-11-03)

### Changes

- `console flags pause` takes only `set` and `clear`: `show` and `read`, which nothing called, are gone (#1365). The dashboard's reader of the flag uses the same reading as the health file.
- A factory pull request that waits for a person is titled `HALT: ` instead of `STUCK: `. The next poll renames the open ones (#1455).
- `tehas-cycle work` runs a cycle's start, and `libexec/work` keeps only the settings and the issue's lock. The container's mounts and environment do not change (#1446).

### Adopter action

`console flags pause show` and `console flags pause read` are gone: a script
which called one of them now gets 64 and the usage. The words of the flag are
what `tehas status` prints, and the object is what `tehasbot status` publishes
in `live/status.json` under `pause.flag`.

A filter, script or search that looks for `STUCK: ` in pull request titles
now looks for `HALT: `.

## v0.24.0 (2026-11-02)

### Changes

- `tehas-cycle cycle` reports every cycle: the report of `libexec/cycle` in bash is gone, and so is the cycle's shadow that compared the two. `TEHAS_CYCLE_REPORT` takes only `rust`, which changes nothing; `bash`, or any other value, is exit 78. Going back to the bash report means installing an earlier release (#1396).
- The gate fails when `cargo test` leaves anything in its TMPDIR, and the workspace's tests make their directories with one guard that removes them (#1445).

### Adopter action

- An instance whose `instance.env` sets `TEHAS_CYCLE_REPORT=bash` starts no cycle after the update: take the line out, or set it to `rust`.
- `~/tehas/state/cycle-shadow.on`, `~/tehas/state/cycle-shadow/` and `~/tehas/logs/cycle-shadow.log` are read by nothing now, and can be removed.

## v0.23.1 (2026-11-02)

### Changes

- A program of the workspace whose standard output is closed early (`tehas status | head -1`) ends 141 without a panic, instead of 101 with "failed printing to stdout: Broken pipe".
- The park comment of a cycle that stopped with its pull request open names it (`stopped with pull request #N open`, `(draft)` for a draft) instead of saying it stopped without one.
- `tehas status` on a release candidate names the newest release and says the candidate is newer, instead of "newest unknown".
- The sandbox image builds with the Rust `RUST_VERSION` names in host/versions.env, with rustfmt and clippy, instead of the stable toolchain the pi-rukas base happened to carry; the build stops when rustc answers another version.

### Adopter action

None.

## v0.23.0 (2026-11-02)

### Changes

- `tehas-cycle cycle` starts and supervises a cycle, and `libexec/cycle` loads the settings and holds the lock for it.
- A planning turn (`tehas plan`) runs in the console; libexec/plan is a path that execs it.
- The poll tries a GitHub read that failed again, within the bounds of its retry rule, and logs gh's own words for a read that still fails (#943).
- tehas-mcp and analytics declare the Rust 1.89 that tehas-core needs.
- A product's dependencies are installed by the factory, in the sandbox, from the lockfiles at the root of its commit and one directory down (bun, npm, pnpm, yarn, cargo, uv, poetry, go, bundler, composer), or from `.pi/install-cmd` when the product names its own. `tehas work` runs the install in the cycle's workspace before the cycle starts; a failed install or a tool the image lacks starts no cycle (exit 78). `tehas check-product` lists the installs, and `--sandbox` runs them in its clone and the gates in a worktree of the clone, as in a cycle (#1404).
- `tehas check-product` and `tehas init-product` work under `~/tehas/tmp/<name>`, the product's repository name.

### Adopter action

None. A product whose `.pi/worktree-setup` links its dependencies from the main checkout now finds them there; one whose install the lockfiles cannot tell can name it in `.pi/install-cmd`.

## v0.22.1 (2026-11-02)

### Changes

- Dependencies updated: the workspace's crates within their versions; on the host Node 22.23.4, gh 2.103.3, bun 1.4.4, jq 1.8.2, parallel-cli 0.9.4, Rust 1.99.1, rustup 1.29.2 and codebase-memory-mcp 0.12.1, each by checksum or lockfile.
- update's lock waits and its record of steps owed are read by the console; lib/update-steps.sh is gone.
- The console workspace moves to rusqlite 0.41 (bundled SQLite 3.54.0), sha2 0.11 and signal-hook 0.4.
- Pi on the host is 1.4.0, the version pi-rukas is verified against.
- Playwright in the sandbox image is 1.64.1 (Chromium 156).
- ctx7 on the host is 0.8.2.
- Caddy 2.11.7 for the docs server, the router, the previews and the MCP server's container.
- The resize page says what each exit of `tehas drain` means before a restart, and its live virtio-mem command uses `--live`.
- factory_decisions answers an easy merge's start line, and its words are checked against the dispatcher's source.
- CI checks out with actions/checkout v7.0.1.
- Release candidates: `scripts/tag-release.sh --rc` tags `vX.Y.Z-rc.N`, published as a prerelease; `tehas update` never moves to one unless `--to` names it, and `tehas api` and `tehas status` read candidate versions.

### Adopter action

- After the update, run `./bootstrap.sh user` (the update's `later:` line
  says so): it installs the new tools into `~/tehas/bin`. The update rebuilds
  the sandbox image, since its jq, codebase-memory-mcp and Playwright pins are new.

## v0.22.0 (2026-11-02)

### Changes

- The console drops verbs nothing calls (label actor, label pause-holds, every shipped verb but page), keeps one copy of the host names and of the version compare, and bash loses four functions only tests called.
- `tehas history` runs in the console, in process, with its output unchanged; `libexec/history`, `lib/history-listing.jq` and `lib/history-repeats.jq` are gone, and an instance without a valid `TEHAS_REPO` now gets exit 78.
- tehas export is a verb of the console and libexec/export is gone. An invalid TEHAS_REPO or an unreadable file of models now stops it with 78, it makes no directory on a refused argument or a link into the home, and it no longer takes a console from the PATH.
- tehas setup runs in the console. libexec/setup is left with finding or building the console, and tehas setup --help reads the console's text.
- `tehas dev` and its login start from the console; `libexec/dev` and `libexec/dev-login` are thin paths kept for the argv pin and the sudo rule. Nothing changes for a person.
- `console run` is the one way into a command under the settings: libexec/with-settings, clean, preview, protolab, refresh and init-product and lib/disk.sh are gone, libexec/dispatch, review, improve, check-product and github-token are a line each, and an update restarts tehas-top.service.
- `factory_decisions` lists the dispatcher's `hold` decisions beside start, skip, wait and drop.
- A refresh's brief tells its developer to run the gate in `.pi/verify-cmd`, not an unnamed gate.
- The resize manual says it applies only to a host that is a virtual machine under libvirt; a container host needs a drain before a restart.

### Adopter action

None.

## v0.21.0 (2026-11-02)

### Changes

- `tehas status`, `disk`, `state`, `limit`, `pause`, `analytics` and `tehasbot health` run the console directly, and seven wrapper scripts are gone.
- The measure of an issue and the free-disk reading are the console's: `console size` and `console disk free` replace the jq and df in lib/sizing.sh and lib/disk.sh.
- The cleaner's image names and the held set are the console's, and libexec/clean is a wrapper.
- `tehasbot refresh` runs in tehas-dispatch; each GitHub call has a 30 s bound.
- check-product and init-product run in the console.
- tidy is a console verb, `console tidy run`; `libexec/tidy` is gone.
- tehas top and tehasbot top run in the console; libexec/top, libexec/top-publish and lib/top.sh are gone, and an update restarts tehas-top.service.
- tehas api, resume, drain and labels run in the console; the labels live in host/labels.tsv and the update's labels step follows that file.
- tehas stop runs in the console; libexec/stop is gone.
- `tehas mcp` runs in the console and `libexec/mcp` is gone; a new console no longer restarts `tehas-mcp.service`, a new server binary still does.
- The store answers the approval, plan and settings views itself; lib/store.sh loses its jq shapes.
- What an image says and what a runner left are read by tehas-cycle (`caps`, `runner`); sandbox/build.sh needs libexec/tehas-cycle beside it.
- Image descriptions and the private names are verbs of tehas-cycle; lib/describe.sh and lib/private.sh are gone.
- `tehasbot improve` runs in the console, and its unit allows 90 minutes, longer than the analyst's 60-minute bound.
- The previews' network is checked and made by the console (`console preview network`); `install.sh` no longer sources `lib/edge.sh`.
- A planning turn's preparation, docker wait and end moved from `libexec/plan` to `console plan`.
- The GitHub App token is made in the console, and the JWT and the token reach curl on standard input, not on its command line.
- The rules of `tehas.slice` are the console's: `console slice` answers for the commands and `lib/slice.sh` is wrappers.
- `tehasbot protolab` runs in the console; every page rebuilds once, and each gh call of a pass is held to 30 s.
- tehasbot preview runs in the console; libexec/preview only loads the settings, and lib/edge.sh is gone.
- The docker options of every container come from one Rust builder (rule 4: nothing widens).
- tehas index is a program.
- tehasbot review is prepared by the console.
- tehas dev starts its session from Rust.
- What follows a cycle's record, from the live view to the next poll, is done by tehas-cycle; a pinned cycle runs its own copy of it.
- The Rust report says a repeat park, a develop timeout's last attempt and a quiet stop, has no outage fallback, and the shadow's log names the arm.
- A poll and a cycle can write to GitHub through one journal, and a poll's settle leaves the actions of a running cycle.
- tehas-cycle can report a cycle through the action journal, with TEHAS_CYCLE_REPORT=rust; the deadline is the time limit less the handoff allowance.
- The poll is tehas-dispatch alone: the bash poll is removed, `TEHAS_DISPATCHER=bash` stops with 78, and a missing program stops with 78.
- `lib/planning.sh` and `lib/live.sh` are removed, and an install takes them off a host.
- The libraries lose the functions only the bash poll called.
- The dispatcher's shadow is removed: the poll's unit appends to `logs/dispatch.log` and nothing else.
- The bash left behind by the poll's removal is gone: `lib/limit.sh` and five functions only tests called.
- `tehasbot status` is a verb of the console; libexec/status, lib/status.sh and lib/analytics.sh are gone.
- The watch of a cycle is `tehas-cycle watch`, and `lib/headless.sh` is gone.
- A start is approved, read and measured in `tehas-cycle start-check`, and `lib/sizing.sh` is gone.
- The workspace, the push check and the image record of a cycle are `tehas-cycle prepare`, and `lib/improve.sh` is gone.
- models.json is read by the programs; lib/models.sh is gone.
- Who applied a label is read in one place, the console.
- The files of an issue and of a reply are fetched and judged by libexec/attach; lib/attachments.sh is a thin wrapper.
- `tehas_load` asks the console, and `lib/config.sh` is a thin loader. A load needs `libexec/console`; `install.sh` builds it first.
- tehas update reads, checks and switches releases in the console; lib/update-release.sh is gone.
- tehas update runs its follow-ups in the console; a --to to a version before this finishes with console.prev.
- install.sh asks the console which steps a changed file owes.
- libexec/update is a thin wrapper round the console; a console that does not start stops an update, and the manual says how to recover.

### Adopter action

Remove `TEHAS_DISPATCHER=bash` from the settings if it is there: the bash poll is
gone and that value stops every command with 78. `rust` and a blank value still
load.

Settings now need `libexec/console`, so a host needs its update to finish:
`install.sh` builds or installs the console first, and a command with no console
stops with 78. A `models.json` whose `providers` is not a map now stops the poll
with 78.

An update now runs in the console. If a release's console does not start, it
cannot update itself: recover with `~/tehas/versions/<previous>/install.sh`, as
the manual's troubleshooting page says.

## v0.20.1 (2026-11-01)

### Changes

- The MCP tool `factory_disk` is served. v0.20.0 described it in the manual
  only (#1200).
- `tehas disk` reads `instance.env` past a comment or a blank line, so it now
  includes `TEHAS_SCRATCH_DIR`. Disk and setup share one reader of the file
  (#1201).
- `libexec/status` is told when the poll ran it, and reports `from_poll=yes`
  (#1202).
- The energy sampler is `console energy`, and `libexec/energy` is gone. It no
  longer falls back to `fleet` (#1225).
- `libexec/attach` composes what a cycle is told it was given. A cycle given
  nothing that has a record now says the record could not be read when attach
  is missing, and a start stops when a kept manifest cannot be compared
  (#1233).
- `scripts/verify.sh --changed` runs `cargo test -p` for the crates a change can
  affect, not the whole workspace (#911).
- Golden files pin every container's whole argument list, in order (#1234).
- A factory pull request whose issue carries `needs-human-attention` loses
  `STUCK: ` when it is proven fine to merge: clean on GitHub, and the last
  six-pass review is newer than its head and lists nothing above MEDIUM (#1305).

### Adopter action

Set `PI_ENSEMBLE_SUBAGENT_PROVIDER` or `TEHAS_ENERGY_URL` for the energy sampler,
or set `TEHAS_ENERGY=off`. With none of them `console energy` exits 78 and its
service retries every 15 s.

## v0.20.0 (2026-11-01)

### Changes

- `scripts/test.sh` sizes its pool from the memory it runs in as well as the
  CPUs: at most two tasks a GiB of `/sys/fs/cgroup/memory.max`. On a 48 CPU host
  that is 8 in `check-product`'s sandbox and 24 in a cycle's container, where
  the kernel used to kill shellcheck. `TEHAS_TEST_JOBS` still wins (#1182).
- A factory pull request's title says `WIP: ` while a cycle works on it,
  `STUCK: ` when it waits for a person and nothing when it is ok to merge. Each
  poll and each cycle's end keep it true, and `tehas api` names the feature
  `pr-title` (#1179).
- A `tehas:plan` session from an allowed person that drafts a plan with no open
  points makes it the issue's text and starts a cycle, with no tick. This
  changes who approves a build: that person's `tehas:plan` does, and nobody
  reads the plan first. With any open point, in a `tehas:refine` session, or
  from the millwright, it asks as before (#1178).
- `tehas-cycle finish` opens a draft pull request from a finished cycle's
  branch (#1131).
- A millwright's guidance comment lifts the hold on a parked issue, and its
  `action=retry` and `action=change` notes now reach the cycle. Before, they
  were all lost. A failed read of a parked issue's comments keeps the hold and
  says nothing on the issue (#1130).
- `tehas disk` says how full a host is and what fills it, and
  `tehas disk --publish` writes that to `live/disk.json`. It leaves out
  `TEHAS_SCRATCH_DIR` for now (#1201). The MCP tool `factory_disk` is described
  in the manual but not served yet (#1200) (#1110).
- The MCP server has a key that sees only what the dashboard shows. A full key
  now serves only known tools, and a setting given twice is refused (#1146).
- `tehasbot clean --dry-run` prunes nothing. Before, it really ran
  `docker builder prune`. It now counts the bytes it would free (#1111).
- `tehasbot clean` logs the size of the cargo build output of a manual session,
  `~/tehas/dev/<name>.state/target`, on every run, and takes it only under
  `TEHAS_CLEAN_MIN_FREE`, once it has gone untouched for `TEHAS_CLEAN_AFTER`
  hours, and only of a clone whose session is closed. `tehas dev` now holds a
  lock for the whole session, `~/tehas/state/dev/<name>.lock`, so a second
  session of the same clone waits for it (#1112).
- Status leaves health to the poll when the poll runs it (#1169).
- Health copies the slots of `live/dispatch.json` only when they have the shape
  the status API promises (#1170).

### Adopter action

Tell whoever applies `tehas:plan` on your instance that the label can now start
a cycle.

## v0.19.2 (2026-10-31)

### Changes

- An idle poll runs `tehasbot status` only when its file is five minutes old,
  not on every poll (#1153).
- A poll reads the comments of blocked issues side by side and the states of
  their blockers in one call, not one or two calls per issue in turn (#1161).
- `analytics sessions --out` and `analytics outcomes --out` write their file
  only when a run changed. Each leaves a `.stamp` file beside its output to
  tell (#1162).
- The watch loop of `tehas work` reads a cycle's step and notice from the
  console, not from seven processes every 10 s (#1082).
- Health no longer calls a cycle failed when it ended closed because its issue
  shipped (#1133).
- One lock probe in tehas-core answers for the store, the dispatcher, the
  health page and the cleaner, so they agree about one lock (#1098).
- `tehas-merge` says `model: true` on every stop after a session and only then,
  takes `--head` and `--base` in upper case, and no longer prints `calls`. A
  command that fills its pipe no longer waits out the 900 s bound, and output
  it cannot read whole refuses the merge (#1149).

### Adopter action

None.

## v0.19.1 (2026-10-31)

### Changes

- `tehas setup` with a GitHub App runs `install.sh` before it clones on a first
  install. The app's `gh` and git helper come from the install, so the clone
  had no login and setup stopped (#1154).
- The settings check refuses `auto` for the cycle and container counts until
  cycles can use it (#1107).
- The GitHub token renewer runs in Rust, logs a failed renewal and tries again
  sooner (#1079).
- A cycle tries its end comment three times and exits non-zero when its end is
  not on record (#1080).
- One report type in tehas-core names the fields and why words of
  `live/dispatch.json` (#1097).
- The watch loop stops a cycle by the cycle's own clock, not the wall clock
  (#1070).
- The shell tests build their binaries with no debug info
  (`CARGO_PROFILE_DEV_DEBUG=0` on that one cargo command), which is most of
  the size of `console/target/shell`. The gate's own `cargo test` keeps its
  debug info (#1115).

### Adopter action

Replace `auto` in `TEHAS_TICKET_CONCURRENCY` or `TEHAS_SLICE_CONTAINERS` with a
number, if an instance has it. Otherwise none.

## v0.19.0 (2026-10-30)

### Changes

- `TEHAS_SCRATCH_DIR` is a new optional setting. Set, a cycle's `/tmp` is
  `<dir>/<name>-<issue>` on that disk in place of the tmpfs in its memory. It
  needs a file system of its own, because nothing limits a cycle's size inside
  it. Unset, nothing changes.
- A cycle removes the build output of its workspace as it ends, where the
  hourly cleaner took it after 24 hours or under 30 GB free. Sources, `.git`
  and sessions stay.
- A planning question's boxes get a comment of their own: the plan text, then
  the question, so ticks land on the right box (#1118).
- `bootstrap.sh clock` writes the chrony drop-in on a host that runs chrony. It
  asked `timedatectl` and wrote nothing on a real host (#1106).
- `tehasbot status` finds the shipped issues that lack `tehas:shipped` with one
  call to the console, where it took two processes (#1083).
- `lib/shipped.sh` is gone. `tidy` and `status` call the console directly
  (#1076).
- A new logo. It is the favicon in every theme. The paper, beesknees and
  beesknees26 themes also show it in the header and on the share card. The
  pixel theme keeps its animated factory and its share card.
- The edge router answers `/fun/theme.css`. It rewrote the path before it
  stripped `/fun`, so the rewrite never matched and the Protolab got a 404 for
  its theme.
- `tehas state pack -` and `tehas state unpack -` move a stopped factory's
  `state/`, `repos/`, `live/` and `mcp/` to another machine as one tar, and
  `tehas api` lists the feature `state-move`. The live token is not moved.
  See the factory API.
- The state store has a read-only verb `store copy --to <file>`: a consistent
  copy of the store at a name the caller gives.
- `tehas limit set` leaves the checking of its arguments to the console. A bad
  argument with missing instance settings is now exit 78, not 64, and
  `tehas limit set --json ...` is accepted.
- `tehas:lean` chooses the lean driver for one issue and its approval record
  binds the choice. Lean cannot run yet: it needs pi-rukas#1920, so `tehas work`
  refuses such a start with 78, and the issue is parked with what to do. Classic
  never runs in its place. An issue without the label runs as it did. Only the
  Rust poll reads the label: the bash poll starts nothing on an issue that
  carries it.
- Analytics records which driver ran each cycle, and `tehas analytics report`
  compares the drivers in a `drivers` key and a `Drivers` table. Runs from
  before read as classic.
- The analytics database moves to schema 3, in place, with no loss. Empty
  token counts are no longer zeros: `versions.medianTokens` and
  `tokens.perShipped` leave those runs out, and a pair's median rests on at
  least three runs that reported usage.
- The dispatcher chooses an easy merge or the full refresh for each refresh
  start, behind `TEHAS_EASY_MERGE` (off). With it on, a pull request whose
  approval has had no easy merge yet starts as one, its approval record is
  written with the store's `--easy` mark and the dispatch log reads
  `start #<issue>: easy merge of PR #<n>`. The store answers `easy` for an
  approval, which is how the poll knows the one merge is spent. The full
  refresh runs instead when the last easy merge pushed nothing, or pushed and
  the pull request then failed the check `TEHAS_BRANCH_CHECK` names, or
  conflicts again; with no check named, only a new conflict asks for the full
  one, and a pull request whose check is still running waits. An easy merge
  counts against `TEHAS_REFRESH_MAX` like any refresh. Until part 3 lands the
  container still runs the full refresh, so today only the record and the log
  line differ (#1031).
- A cycle the dispatcher stops for being quiet says so in its comment and its
  end record (#1078).
- `tehasbot health` does its work in one console command, `console health run`,
  and `libexec/health` is a short wrapper. A run starts about half the
  processes, and what it checks is unchanged (#1084).

### Adopter action

Run `tehas labels` to make `tehas:lean`.

Roll back to a release before schema 3 only if you accept this: an older
program moves a schema 3 `state/analytics.db` aside whole and starts a new one.
A second rollback replaces that kept copy.

## v0.18.0 (2026-10-30)

### Changes

- The counts of cycles and containers can follow the host: `auto` for
  `TEHAS_TICKET_CONCURRENCY` and for `TEHAS_SLICE_CONTAINERS`. A number keeps
  working exactly as it did, so no instance changes unless it chooses to. With
  `auto` the dispatcher works both counts out on every poll from what the poll
  reads anyway: `tehas.slice`'s `MemoryMax` over `TEHAS_CONTAINER_MEMORY` (8g)
  for the containers, and `nproc` over `TEHAS_CYCLE_CPUS` (4) for the cycles,
  each capped by `TEHAS_CYCLE_CEILING` (16) for an endpoint that cannot serve
  what a big host asks for. A VM given more CPUs or memory is used at the next
  poll, and one that lost them holds new starts from the next poll: a running
  cycle is never stopped. `tehas status` prints the count in force and what
  bounded it (`slots     cycles auto: 6, cpu-bound (...)`) and the status
  object carries the same as `room.slots`. The arithmetic is in the
  dispatcher's Rust, in `console/dispatch/src/slots.rs` with its tests; a host
  whose reading answers nothing is held to one cycle and two containers rather
  than given the ceiling (#1041).
- The millwright's feed, off by default: with `TEHAS_MILLWRIGHT_FEED=1` the
  millwright's `tehas:ready` starts the first cycle of an issue an allowed
  labeller wrote, its text unchanged since, at most
  `TEHAS_MILLWRIGHT_MAX_FEEDS` (2) at once. The Rust dispatcher only. The
  queue's page now asks GitHub for each issue's author and creation time.
- A health file that could not read the failed units says so. When the
  `systemctl --user --failed 'tehas-*'` read does not answer, `libexec/health`
  leaves `--units` out and the console writes `checks.units` as `null` instead
  of `[]`, so the file carries no units check rather than a passed one, and
  raises no warning from a list it never read. An empty read is still `[]`
  (#1055).
- `tehas.slice` follows memory added to a running host: `bootstrap.sh slice`
  also installs a udev rule and `tehas-slice-reload.service`, a rate-limited
  oneshot that reloads systemd when a memory block comes online, so the
  slice's `MemoryMax=75%` is worked out against the memory the host has now.
  `bootstrap.sh check` reports the rule and the unit beside the slice (#1040).
- `bootstrap.sh clock` sets a VM up so its clock is right within seconds of a
  restore: qemu-guest-agent, and chrony allowed to step the clock at any time
  (#1044). `tehas-cycle clock` measures a cycle's running time on a clock that
  stops while the VM is saved (#1042, part 1).
- The manual: [Resize or restart the host](/resize), a host's CPUs and memory
  changed live, and a hypervisor restart that saves the VM with its cycles
  running, without a drain (#1045).
- The easy merge in Rust: the crate `tehas-merge` and the store's easy mark,
  part 1 of #1031. The store moves to schema 9 and migrates in place.
- `tehas update`'s edge step no longer takes over the Protolab server of before
  v0.3.0 (#1064).
- `tehasbot status` logs a WARN line whenever it writes no snapshot (#1056).
- The approval record's shape checks refuse a value with a newline on the end
  (#1029).
- Smaller bash: retired settings are refused by one loop (#1068), the dead
  `tehas_top_host` and `console top host` go (#1071), and the cycle's end drops
  the fallback for runner images without a schema 5 result (#1065). The
  ceiling is lower.

### Adopter action

- Run `sudo ./bootstrap.sh slice` on each host to install the memory-online
  rule and its reload unit. Without it the slice keeps working as before, and
  memory added to a running host reaches it only at the next daemon-reload.
- On a host that is a VM, run `sudo ./bootstrap.sh clock` once, so the clock
  is right soon after a restore.
- The store migrates to schema 9 on its own at the update: no drain is needed.
- The millwright's feed and `auto` slot counts are off until an instance sets
  them.

## v0.17.0 (2026-10-29)

### Changes

- The mask that the console, its live file and the MCP server's logs share
  hides more: a provider key written as a literal `apiKey` or `backupApiKey`
  in `$HOME/.pi/agent/models.json`, where before it hid only the value of a
  `TEHAS_MODEL_KEY_<NAME>` setting. The file is read once per run, as a host
  file is read elsewhere: no link is followed, and over 1 MiB it is refused.
  A missing, unreadable or malformed one adds nothing, and so does a value
  that names a variable, holds white space, is under 8 or over 512 characters,
  or is past the first 64 of the file (#986).
- `TEHAS_MILLWRIGHT_LOGIN` takes a GitHub App's login, `<slug>[bot]`, as
  `TEHAS_BOT_LOGIN` does. Before, the setting refused it with 78, and since an
  invalid setting stops every command, the dispatcher stopped too. The
  millwright's labels, comments, ticks and replies count whether GitHub names
  the App `<slug>[bot]` (REST) or `<slug>` (GraphQL), and no other account
  does. The approval record and the planning record name an App millwright
  without `[bot]`. Loading the settings now also stops with 78 when the
  millwright is `TEHAS_BOT_LOGIN`, in either form (#1051).
- Four MCP tools to follow a factory without SSH (#1027). `factory_events`
  gives the lines of `dispatch.log` and `refresh.log` after a cursor, masked,
  each with a kind; the cursor follows a copy-and-truncate rotation and names
  a log it lost in `gaps` (#1023). `factory_decisions` lists the dispatcher's
  last start, skip, wait and drop decisions, from the last 1 MiB of
  `dispatch.log` (#1024). `factory_park` says why an issue's last stopped run
  of the last 30 days stopped: the park marker's fields and the runner's
  result (#1025). `factory_sessions` and `factory_failures` name the sandbox
  image of each run (`image`, `imageBuilt`), and `factory_failures` can split
  a version pair by image (#1026).
- A planning turn records the sandbox image it started in, through
  `state/plan-<n>.json`, and the Rust cycle report passes the image's build
  time to analytics as `libexec/cycle` does (#1026).
- Health no longer says the last cycle failed while a cycle with no known
  start is running (#1001).
- An issue closed by a merged pull request is marked shipped even when its
  close event names a commit (#1028).
- Footers spell the version as `tehas`, not `TEHAS` (#1012).
- The store's concurrency test and the flags clock test no longer fail on a
  loaded host (#1022).

### Adopter action

None. A factory whose `TEHAS_MILLWRIGHT_LOGIN` line was commented out because
it held a `[bot]` login can have it back.

## v0.16.2 (2026-10-29)

### Changes

- `tehas tidy` decides its label changes in the console. The names it takes
  off, the answer of the search and which issue the read of what closed them
  is asked about are `console tidy`'s (`console/src/tidy.rs`, with its Rust
  tests beside the source); the command keeps the settings, the two searches
  and the label edits, and went from 216 lines of bash to 183. Nothing changes
  for users: the same labels come off, and `tidy --dry-run` says what it said
  before (#956). The shell test now pins all nine names end to end, so the list
  the console holds is the list a poll searches for, takes off and logs; the
  manual names `tehas:refine` among them.
- A drafted plan in a `tehas:plan` session offers only **Update this
  issue** and **End this planning session**. **File as a new issue** stays
  for `tehas:refine` sessions, which exist to file one (#1010).
- Less bash: the lines and the `--json` object of `tehas status` and the
  frame of `tehas top` are drawn by the console. `console status show|json`
  and `console top frame` replace the jq of `lib/status.sh` and `lib/top.sh`,
  with the same words, the same object and the same exit statuses (#888).
- Less bash: the health file is built by the console. `console health
  report` replaces the jq of `tehasbot health`, with the same checks, the same
  `output` and the same file; the command keeps the store calls, the failed
  units and the publish (#889).
- The work order on the dashboard shows CREATED, CHANGED, CLOSED, RUNS,
  TOKENS, TIME and ENERGY for each issue the snapshot lists, and leaves out a
  figure it does not have instead of showing `-`. `analytics orders` writes
  `live/orders.json` (runs, seconds and tokens per issue), which `tehasbot
  status` runs before its report and removes when analytics is off; the
  shipped list's query asks for title, URL and dates again (#994, #995, #996,
  #997).

- The gate's kept pass no longer misses a same-size edit made in the
  second of the last index write: its copy of the index keeps the index's
  time, so git checks that file again (#983).
- Less bash: `tehas pause` and `tehas resume` hand the pause flag to the
  console. `console flags pause show|set|clear|read` reads, validates, writes
  and prints `~/tehas/state/pause.json`, the same file in the same shape with
  the same words as before, and the two commands keep only their settings,
  their argument checks and the hand-off; `tehas resume` still reads the
  `tehas:pause` label of the control issue itself, because only `gh` does that.
  `flags pause clear` takes a name of the flag away only when it holds a
  regular file of this account, so `tehas resume` never follows a symlink left
  under one of them and says so instead (#962).

### Adopter action

None.

## v0.16.1 (2026-10-29)

### Changes

- An update needs no drain. Each cycle runs from its own copy of `bin/`,
  `libexec/` and `lib/` in `~/tehas/run`, made by `tehas-cycle run` when its
  unit starts, and a planning turn the same way. Running cycles end on the
  version they began on, and the slots stay full through an update (#988).
- The business page orders its panels as the production line, the last 10
  days, the issue board with the open epics beside it, and the console
  (#1002).
- The pixel and business footers show the pi-rukas version of the sandbox
  image beside the tehas version. `host/sandbox/build.sh` writes it to
  `~/tehas/fun/rukas-version.txt` (#998).
- The filing decisions of `tehasbot review` move from bash into the store,
  with Rust tests; the bash keeps the GitHub calls (#987).

### Adopter action

Drain once more for the update to this release (`tehas drain --wait 3600`,
update, `tehas limit clear`): the cycles running now started before they had a
copy of their own. Later updates need no drain.

## v0.16.0 (2026-10-29)

### Changes

- The floor and the business page show the open epics as a stack: one row
  per epic with a block per sub-issue and `n/m closed`; a click lists the
  sub-issues. `tehasbot status` writes `live/epics.json` at every poll from
  `console epics`. Only sub-issues of `TEHAS_REPO` are named; others are only
  counted (#981).
- The business page shows the console of the running cycles, with a tab per
  running cycle and one for the last ended cycle (#937).
- The business line sits on one row while it fits, and has a Planning stage
  while an open issue carries `tehas:plan` or `tehas:planning`.
  `status.json` gains `planned`, the list behind the `planning` count (#976).
- `tehasbot review` files its findings as one issue in `TEHAS_REVIEW_REPO`
  for a person to triage, labelled `tehas:improvement` and never
  `tehas:ready` (#508). With `TEHAS_REVIEW=1` the new `tehas-review.timer`
  runs it once a week; the timer is installed but not enabled (#509).
- `tehas work` sets a cycle's child bound from `TEHAS_CHILD_TIMEOUT`
  (default 7200 s, at most `TEHAS_WORK_TIMEOUT`) and one verify bound,
  instead of leaving both to pi-rukas's defaults (#932).
- An idle poll costs less: the poll takes its GitHub token once and gives it
  to every `gh` it runs, and to the tidy, status and health it starts at its
  end (#938, #965); it asks the store its per-issue questions in one
  `store batch` call, not about 43 (#941).
- The Rust dispatcher fails closed on what GitHub leaves out. A rename with
  no time counts as after the approval, and a check run is green only on
  `success`, `neutral` or `skipped` (#961). A parked refresh with no start
  time counts as a park since the approval (#970). An open journal action
  with a missing or malformed id, issue or time is left open with a warning,
  not settled as 0 (#967). A `poll-begin` answer with no time makes no write
  (#971). A hold whose comment failed is told again on the next poll (#959).
- The mask that the console, its live file and the MCP server's logs share
  hides more: the value of every setting the program was started with whose
  name says it is secret (a name with TOKEN, SECRET, PASSWORD, PASSWD, KEY or
  CREDENTIAL in it), as literal text wherever it appears; any upper-case
  assignment of such a name; and keys in the shape model providers give
  (`sk-` and a long run) (#984).
- `console forge` gives bash one command over the forge interface: labels,
  label events, who approved, and putting a label on or off (#948).
- The MCP server has a `factory_search` tool: a bounded search of a
  session's masked logs, by issue or by session ids, with a cursor to go on
  (#839, #946).
- Tests can make one chosen `gh` call fail (#942), and the dispatch tests
  stub health unless a case reads it (#963).
- Which open pull requests get a preview is chosen by the console
  (#892, `console preview choose`, console/src/preview.rs): the rules of
  which pull request is ours (a head in the product repository, opened by an
  owner, member or collaborator), which of those are the newest
  `TEHAS_PREVIEW_MAX` and which say "over-limit", and when a list is not one
  the previews may be chosen from, all move from bash to Rust. `libexec/preview`
  keeps the paging of GitHub, the builds and the removals, and loses the four
  jq programs that made the choice; the 31 lines they took pay for the call to
  the console, the reading of the pages, and the three helpers of the choice,
  so the tree's bash does not grow and the ceiling goes down with it. A
  console that is not installed builds no previews (78) and says to run
  `install.sh`. Nothing changes for a preview that serves: the same list, the
  same names, the same notices.
- The Protolab's pages are written by a Rust verb, `console protolab`, and not
  by the jq programs of `libexec/protolab`. The page of a ticket, the lab's
  front page and `live/protolab.json` are made in `console/protolab.rs`, which
  also reads the attachment links of a ticket. The command keeps the settings,
  the calls to GitHub, the downloads and the order of the work. The pages are
  the pages they were, line for line. A host needs the console installed
  beside the command, which `install.sh` does: a pass that finds none says so
  and exits 78 before it calls GitHub.
- The limit of the factory is read, checked, written and printed by the
  console (`~/tehas/libexec/console flags limit`), not by bash
  (`libexec/limit`, `lib/limit.sh`), which are thin callers of it now (#895).
  `tehas limit` and the pages of `tehas status` say the same as they did.
- What closed an issue is now the console's to decide. `console shipped`
  (`console/src/shipped.rs`) holds the page of the read, its GraphQL query and the
  rule that reads its answer, and `lib/shipped.sh` is the names of those verbs and
  the call of them: `tehas tidy` and `tehasbot status` ask the same verb the same
  question, so the label one gives and the list the other shows cannot drift.
  Neither command runs without the console beside it, as `tehasbot clean` already
  requires, and says to run `install.sh` when it is not there. The whole-answer test
  is a verb of its own (`console shipped whole`) rather than a jq program in the
  library, and `tehas tidy` no longer reads the errors key before asking the
  console: the one rule is read once. Nothing a host
  configures changed (#893).
- The forge interface, and its GitHub adapter, as a library: `tehas_core::forge`
  (#947). One trait in tehas terms - an issue's labels, the newest event of a
  label, who applied the label that approves work, and a label put on or taken
  off - and a `GitHub` adapter that backs each term with labels as the bash
  does today, reading the events with `--paginate` and running gh through a
  `GhRunner` so its rules are tested without gh or a network. The approval
  term reads applications alone, as `tehas_label_actor` does: a label taken
  off answers the state term, never who approved. A caller asks the same term
  of any label - `applied_by(issue, label)` answers who put `tehas:urgent` on
  an issue, and `approval()` is that term of `tehas:ready`. The adapter runs gh
  through a `Bounded` runner that holds the program, gives one call a bound of
  its own and stops the group with TERM and KILL, so no gh can hold a poll and
  the lock it carries. A removal takes its label encoded and reads 404 only
  where the status sits, so an error page that happens to carry the three
  digits is not read as a label taken off. Nothing calls it yet: the
  next issues of #800 give bash a command for it and move
  `libexec/tidy`, `libexec/status` and `lib/labels.sh` onto that command.
- The Rust poll no longer takes an approval from a label timeline GitHub cut
  short. A page asks for 100 open issues at a time and 100 label events per
  issue, and GitHub answers a page over its budget for nested items with
  timelines cut short and no error: on 2026-10-28 the page gave one issue its
  oldest 49 label events and the issues after it none at all. The guard read
  the timeline only for a label no event applied, so a cut one that still
  held an older application of the label was taken for the whole story, and
  the poll skipped the issue on every poll with the line that its earlier
  approval had started a cycle already. An issue's events are now read from
  the REST list when the newest event of a label the issue carries is not an
  application or the label has no event, and the open issues are read in
  pages of 25, which stays well under that budget. The page count, not the
  points, is what the poll pays in for those pages: each one is a `gh`
  process run before any decision. The bot's own
  `tehas:running` mark is excepted from that comparison: it is applied by
  the bot alone, so guarding it would spend a paginated read on every running
  issue on every poll, and the park of a mark with no cycle behind it rests
  on the lock file rather than the timeline. The exception is the queue's
  own, not the guard's: the control issue is asked about with no label
  exempt, so a page that cut its `tehas:pause` history short cannot leave the
  factory running, and an issue the page gave no timeline at all is read
  again whatever it carries. (#928)

### Adopter action

None is required. `tehas tidy`, `tehasbot status` and the preview and Protolab
commands now need the console beside them: where an old
`libexec/console` lacks their verbs they exit 78 and say to run `install.sh`.

To run the weekly review, set `TEHAS_REVIEW=1` and `TEHAS_REVIEW_REPO` in the
instance and enable `tehas-review.timer` by hand: an update installs it but
does not turn it on.

## v0.15.2 (2026-10-28)

### Changes

- `scripts/check-bash-lines.sh` prints the room left on every run, and the
  lines over when over the ceiling (#931). A ceiling with a leading zero is
  read in base 10, and one of more than 18 digits is refused as no number.

### Adopter action

None.

## v0.15.1 (2026-10-28)

### Changes

- The report's shadow names its files `state/cycles/shadow-<issue>.*`, where
  it named them `<issue>.shadow.*` (#923). A poll that ran while a cycle ended
  took `<issue>.shadow.park` for the record of an older tehas, stopped with 78
  and said to run `install.sh`. Only a host with `state/cycle-shadow.on` was
  touched, one poll at a time.
- An idle poll costs less (#871). A poll that wrote nothing to GitHub and
  started, stopped or turned nothing runs neither the status nor the
  analytics sample at its end; the next poll that does work runs them. Its
  cost is still recorded. A poll settles the action journal only when it holds
  an open action. The report counts a stretch with no sample as not
  available, so `availability` falls while a factory is paused or idle.
- Gate minutes per cycle in the analytics report (#870): each run records how
  many runs of the product's gate the cycle made and their time
  (`runs.gate_runs`, `runs.gate_ms`, and a table `run_gate_runs`), from the
  runner's progress log. The analytics database moves to schema 2.
- The MCP server's journal line names the tool of each `tools/call`, never
  its arguments (#836).
- `store review-issue` renders the six-lens review's findings as the title
  and body of an issue, in Rust (#883). Nothing files it yet.
- Tests: the dispatcher's apply tests, the store's busy and lock tests and
  the MCP server's deadline and logs tests wait out short bounds, not the real
  ones (#909, #908, #910); the installed programs keep theirs, and `tehas-mcp`
  takes `--io-timeout` for its tests only. The MCP outcomes test no longer
  fails when the clock reads 42.
- `scripts/verify.sh` keeps the whole output of each run's steps in a log,
  the newest 20 under `TEHAS_VERIFY_LOGS` (default `$TMPDIR/tehas-verify`),
  and a failed run names it on its last line, `log: <path>` (#930).

### Adopter action

- The analytics database moves to schema 2 with no migration. On the first
  run after the update the schema 1 file is moved aside whole, as
  `state/analytics.db.schema-1`, kept 365 days, and a new one starts: the
  ten-day report and `tehas analytics export` start empty. A program that
  reads the export, such as tehas-warehouse, must take schema 2.
- With `state/cycle-shadow.on` there, drain before you update (`tehas drain
  --wait`), then `tehas limit clear`: a cycle that ends across the update
  leaves its `<issue>.shadow.*` files, which stop every poll until removed.

## v0.15.0 (2026-10-27)

### Changes

- `TEHAS_SLICE_CONTAINERS`, 4 by default, sets how many containers
  `tehas.slice` holds. Each container's memory ceiling is the slice over this
  number, and admission, the CPU share and the pids limit follow it. Before,
  four was fixed.
- `tehas setup` checks the settings, asks GitHub and the endpoint and writes
  the instance's files in the console (`console setup files`, #882);
  `libexec/setup` keeps git, the clone and the install steps (452 to 201
  lines). What those first steps say goes to standard error, and
  `models.json` is written with its keys sorted. From a checkout with no built
  console, setup builds it with cargo first.
- A planned issue's title is the work in full. The first turn asks the console
  for its descriptor (`console plan descriptor`, `console/plan.rs`) instead of
  cutting the brief's first line and adding `(issue #<n>)` to it: the number
  goes on only when the title has fewer than six words and the whole still
  fits what pi-rukas keeps of a title, so a long title is never cut at a word
  boundary. A brief whose first line is `Title: <text>` is named by `<text>`,
  so a half of a split is named for its own work and not the whole issue it
  came from (#899).
- With `TEHAS_DISPATCHER=rust`, an open issue marked `tehas:planning` with no
  planning session on record is handed to a person: it gets
  `needs-human-attention` in place of the mark and a comment to apply
  `tehas:plan` again. GitHub can make a label change and still answer it as
  failed; the poll then took its turn back and the mark stayed, holding the
  issue and those blocked by it.
- With `TEHAS_DISPATCHER=rust`, a paused poll no longer reads the comments,
  parks, approvals and blockers of the issues it would decide on: only the
  queue and what its clean-up needs (#254).
- The dispatcher in Rust is the default: every poll is `tehas-dispatch poll`.
  It ran beside the bash poll as a shadow on tehas-tehas from 2026-10-26, and
  has run its queue since 2026-10-27. A failed read of GitHub names gh's
  error by the same fixed words as the bash poll's line (#254).
- `tehasbot health` warns when an idle poll costs more than its budget
  (`checks.poll_cost`): the medians of the last hour's polls that started no
  cycle, against `TEHAS_POLL_BUDGET`, by default `cpu=5,wall=30,writes=0`
  (#744).
- Gates side by side do not oversubscribe the host: the test runner holds
  back a new test file while the load average is at least the host's CPUs,
  where it was 1.5 times (#869).
- The MCP server has an eleventh tool, `factory_outcomes`: counts of how the
  runs ended for a year back, per version pair or per UTC week, from the
  analytics (#838).
- A park's comment offers a third box, **Retry from the issue alone**: a
  retry whose start names no comment of the issue or of its pull request, for
  when the discussion is what keeps a cycle stopping (#717).
- An idle poll, `tehas health` and `tehas status` run fewer processes: the
  store and the queue snapshot are read once per question rather than once per
  field (#736).
- A first planning turn passes `--issue <n>` when the image lists that flag
  for `plan`, so the planner does not take the issue for a duplicate of itself
  (pi-rukas v0.14.28 and later).
- `install.sh` reports a leftover staged sandbox recipe as stale and takes it
  back when it installs; the build test also checks that no step crossed the
  cut between the layers every instance user shares and those of one uid
  (#735).
- The business page shows the version in its footer.

### Adopter action

None. A host that wants more cycles side by side sets
`TEHAS_SLICE_CONTAINERS` and gives the host the memory for it. To keep the
bash poll for now, set `TEHAS_DISPATCHER=bash` in `instance.env`; it is
removed in a later release.

## v0.14.5 (2026-10-27)

### Changes

- A drafted plan offers **Update this issue** beside **File as a new
  issue**. The first makes the plan the issue's own title and text and ends
  the session: the discussion stays in the comments, and the last one holds
  the text the plan replaced. Apply `tehas:ready` to build it (#874).
- Every planning session gives the planner the issue brief,
  `lib/issue-brief.md`: what a good issue body is. The refine brief keeps
  only what is refine's own (#875).
- What a planning turn says is shaped in the console (`console plan`), not
  in jq. A host with no console beside `libexec/plan` runs no turn and stops
  with 78.
- `tehas export` copies in the console (`console export run`), one process
  for the whole export rather than one mask per file (#876). Protolab images
  are now copied byte for byte; before, the mask read them as text and
  replaced their bytes. A file that cannot be read fails its cycle (exit 1)
  instead of being left out without a word.
- The MCP server has a tenth tool, `factory_issue` (#837): `{"issue": N}`
  answers the stage the dashboard lists the issue under, its title, the step
  and status of a cycle that owns it now, and every run of it of the last 30
  days oldest first, with each run's id, so one call answers "what happened to
  issue N" instead of three. It reads only the files the server already reads
  and calls nothing.
- A park that repeats the last park of the same issue says so in its comment,
  and its history line carries a `repeat` count, so a retry on the same text
  is seen to be likely to stop the same way (#716).
- AGENTS.md: the rule that jq programs must also run on jq 1.7 goes, now
  that hosts, the sandbox image and CI all run jq 1.8.1 (#867).
### Adopter action

None.

## v0.14.4 (2026-10-27)

### Changes

- The poll tests' page stub parses in jq 1.7, the sandbox image's (#859): an
  object value built with `+` unparenthesised failed every cycle's gate
  inside its container while CI, on a newer jq, was green.
- A store migration in a second already taken writes its backup under the
  next free name (#863).
- AGENTS.md: keep jq to a minimum, and to what jq 1.7 runs (#861).
- `tehasbot clean` runs its steps in the console (`console clean run`), not in
  bash. What it removes and keeps is unchanged. A host with no console beside
  the command stops with 78 and says to run `install.sh` (#862).
- One jq everywhere (#860): `host/versions.env` pins jq 1.8.1 by its
  checksum, and the host (`bootstrap.sh user`, into `~/tehas/bin`), the
  sandbox image (ahead of the base image's jq 1.7) and CI all run it. A jq
  program that parses in one now parses in all three. Before, a test stub
  that jq 1.7 cannot parse passed CI and failed every cycle's gate (#849,
  #859). `bootstrap.sh check` reports jq's version against the pin.

### Adopter action

- After the update, run `./bootstrap.sh user` (the update's `later:` line
  says so): it installs jq 1.8.1 into `~/tehas/bin`. The update rebuilds the
  sandbox image with it, since its jq pin is new.

## v0.14.3 (2026-10-27)

### Changes

- `tehas drain` says drained only when no poll can still start a cycle
  (#795): after it writes the limit it takes the dispatcher's lock, bounded
  by `--wait`, so a poll that read the old limit has ended and its cycles
  are counted.
- A failed GitHub call in the poll names gh's error by words of a fixed list
  (#853), such as `ERROR: gh api graphql failed: HTTP 502`. Nothing else of
  gh's error is logged, since an agent can write it.
- A container that is starting does not hold the poll (#848): a
  `memory.stat` without `slab_reclaimable` yet reads it as 0, in the bash and
  the Rust dispatcher alike.
- A planned issue's title is the work, not "Plan issue #N" (#818): the first
  planning turn names the issue's title and then its number.
- A develop timeout leaves a note in the issue's body, and a second one on
  unchanged text offers no retry and points to `tehas:plan` (#832, #833).
- The install-list check names a new file under `bin/`, `lib/` or `libexec/`
  and says what to do with it, and the manual says a deliverable that
  creates one names `install.sh` and `test/install_lib.sh` (#784).
- The manual says how an issue's `Out of scope` section fences a cycle's
  work (#719).
- The snapshot behind `tehas top` is put together by the console (#846,
  `console top snapshot`, console/src/snapshot.rs): the factory, queue,
  cycle, units, energy and console sections, with the same JSON. `lib/top.sh`
  keeps the wait between two readings of the host and the frame, and loses
  156 lines. Without the console `tehas top` and `tehasbot top-publish` say to
  run `install.sh` instead of drawing a frame without a host.

- `bootstrap.sh user` no longer stops in silence when pi-rukas re-points a tag
  (#793). The clone of pi-rukas is a copy of upstream, so its fetch takes the
  tags by force; a fetch or a checkout that fails names the step and gives
  git's own words. `scripts/tag-release.sh` and `scripts/tag-deploy.sh` fetch
  the remote's tags the same way, which is what let tehas's own tags be re-made
  after the history rewrite.

- The host section of `tehas top` is read and computed by the console
  (#844, `console top reading|host`, console/src/top.rs) instead of bash, awk and
  jq in `lib/top.sh`, which loses 176 lines. Same figures, same JSON. Without
  the console the section is `null`.

- The preview notice in a pull request's description is written by the
  console (#841). The record, the read of the description, the decision
  whether the block at the top is the bot's and the write move from bash to
  `console/src/notice.rs`; `libexec/preview` loses 310 lines. A host without
  the console builds no previews (78) and says to run `install.sh`.

- `TEHAS_DISPATCHER=rust` in the instance settings makes libexec/dispatch
  hand each poll to `tehas-dispatch poll` (#254, part 4c). The default stays
  `bash`. A Rust poll first settles the writes a poll before it left open in
  the action journal, from what GitHub says now, and never repeats one. See
  "The dispatcher in Rust" in the operating page for how it differs.
- The Rust dispatcher reads an issue's label events on their own when
  GitHub's page of the queue left them out (#254). GitHub answers a large page
  with empty timelines and no error; read as given, an allowed account's
  label had nobody behind it, and the poll took it off. Found by the shadow
  on tehas-tehas before any host ran the Rust poll.
- `tehasbot review` runs its container from Rust (#829): the console reads
  the range, checks the image's inputs, bounds the run and writes the record.
  `libexec/review` keeps the sandbox and loses 155 lines. The wait on the
  `docker run` client is now bounded too (#774). A host without the console
  reviews nothing (78) instead of keeping the logs unmasked.

### Adopter action

- `tehasbot review`, the preview notice and `tehas top` now run from the
  console: without it they stop and say to run `install.sh` (#829, #841,
  #846). A host installed with `install.sh` has the console; one that copies
  files by hand must install it.

## v0.14.2 (2026-10-26)

### Changes

- The improvement loop never files in the product repository (#820). Its
  findings are model-written from what cycles saw, so they go to a repository
  of their own where a person triages them. Loading stops with exit 78 when
  `TEHAS_IMPROVE_REPO` or `TEHAS_IMPROVE_RUKAS_REPO` names `TEHAS_REPO`.
- The conflict refresh takes a handed-off pull request, draft or not (#817).
  `tehas:question` no longer drops it, and the refresh's start takes
  `tehas:question` off the issue with `needs-human-attention`.
- The `gh` shim counts a GraphQL query as a read, not a write (#792): only a
  `mutation` is logged in `state/github-writes.log`, so the `poll` line of
  `tehas status` no longer shows a poll's reads as writes.
- The cycle's shadow (#256, part 2): while `state/cycle-shadow.on` is there,
  `tehas-cycle` decides each cycle's report beside `tehasbot cycle` from the
  same files and its own GitHub reads, and logs what differs from the bash's
  writes, park and end to `logs/cycle-shadow.log`. A report that differed is
  kept in `state/cycle-shadow/`. install.sh builds and installs
  `libexec/tehas-cycle`, and a release carries it. It acts on nothing.
- install.sh rebuilds `tehas-dispatch` when `console/core` changes, which it
  uses since the question comments' text moved there.
- libexec/cycle no longer has a check that a progress file is newer than the
  cycle's lock: it compared with a file named by a number and never held.
- `tehas-cycle report <facts.json>` decides a cycle's report in Rust (#256,
  part 1): from what the cycle left and GitHub's answers it says what
  libexec/cycle does, its warnings, label changes, comments and park question,
  the end and the park on record, the analytics and the improvement record.
  Its 265 fixture cases are the reports of the bash cycle tests, traced. It
  writes nothing, and nothing calls it yet. The question comments' text
  (q_clean, q_render) is now in `console/core`, shared with the dispatcher.
- `tehas-dispatch poll [--dry-run]` runs one poll of the queue in Rust
  (#254, part 4a): the dispatcher's lock and its hang warning, the checks
  before any call, the snapshot and its decision, live/demand.json, health,
  live/dispatch.json, the unused locks swept, the actions in the order of the
  poll's lines, and the end of every poll that held the lock (tidy, its cost
  and the analytics). Nothing chooses it yet: libexec/dispatch still polls.
- The Rust poll passes the gates of a start as the bash poll does (#254,
  part 4b): systemd's answer for the cycle's unit, the code index (left alone
  while a cycle runs, waited for while it is refreshed, refreshed first when
  it is due), the checkout's refresh before a start or a planning turn, and
  the low gate's run. Nothing chooses it yet.
- The poll's cost is on the factory API (#745, part 3 of #734): `tehas api
  --json` carries `poll`, the last poll's wall and CPU time, its `gh` calls,
  its writes by kind, its starts and its age in whole minutes, beside the
  polls, CPU time and writes of the last hour, so a program that keeps a copy
  of them can add the cost of many hosts up. `store poll-show --api --now
  <unix>` shapes them, and `tehas api` prints the store's own words for them
  on a `poll` line of its plain output. A store that cannot answer is
  `"poll": null`, not a failed command. A new test pins what a poll that
  starts nothing may cost: 0 writes and fewer processes than a ceiling written
  in the test, counted on a host with 500 lock files, 50 shipped issues and 5
  blocked ones, so a change that raises the cost of an idle poll has to change
  the test.
- The dispatcher in Rust can end a planning turn (#254, part 3a): it
  collects the turn's exit status, posts its question as the bash poll writes
  it, replaces the issue a refine filed for, and ends a session that was cut
  off, lost or closed. The state store's journal takes three more kinds of
  write: `issue-edit`, `sub-issue-add` and `issue-close`. Nothing calls it
  yet.
- The dispatcher in Rust can start a planning turn (#254, part 3b): a first
  turn's brief and its session on record, a next turn's answer, the issue
  marked planning before the turn's unit starts, and the three spends of a
  park's retry. Nothing calls it yet.
- The fixer's old names are no longer read (#799). A `TEHAS_FIXER_*`
  setting stops every command with 78 and names its `TEHAS_MILLWRIGHT_*`
  name. The dispatcher no longer counts the marker `tehas:fixer` or the
  label `tehas:fixer-escalated`, `tehas labels` no longer makes that label,
  and `store plan-answer` no longer takes `--fixer`. Records written as
  `how: "fixer"` still count as the millwright's, and `tehas api` still lists
  `fixer-grant` and `fixer`, as API version 1 removes no field.

### Adopter action

- Set `TEHAS_IMPROVE_REPO` (and `TEHAS_IMPROVE_RUKAS_REPO`, if set) to a
  repository other than `TEHAS_REPO` before updating, for example a private
  backlog, and give the bot's account write access to its issues.
- Rename `TEHAS_FIXER_LOGIN`, `TEHAS_FIXER_MAX_RETRIES` and
  `TEHAS_FIXER_MAX_PLAN_ANSWERS` in `instance.env` to `TEHAS_MILLWRIGHT_*`
  before updating. A millwright's program must write
  `<!-- tehas:millwright ... -->` and `tehas:millwright-escalated`. The label
  `tehas:fixer-escalated` may be deleted from the product repository.

## v0.14.1 (2026-10-26)

### Changes

- Each poll's cost record says how many cycles it started (#786), and
  `store poll-idle --since <seconds>` gives the median CPU, wall time and
  writes of the polls that started none, for the health check's budget. The
  state store moves to schema 8, which adds the column `poll_cost.starts`;
  the install migrates it and keeps a backup. A poll of an older tehas has no
  answer there, which is not counted as idle.
- The query `tehas status` and `tehas tidy` send for shipped work had one
  closing brace too many since v0.14.0 (#731), so GitHub refused it: status
  wrote no snapshot of shipped work and tidy added no `tehas:shipped`. The
  brace is gone, and `test/shipped_test.sh` checks that the query's brackets
  balance, since the tests stub GitHub.
- The dispatcher in Rust can start cycles (#254, part 2a): its approval on
  record with the comments, pull request comments and notes the bash poll
  gives it, the issue marked running before its unit starts, the wait for the
  start, and the undo when a start gives up. Nothing calls it yet: the bash
  poll still starts every cycle.
- The dispatcher in Rust can stop cycles (#254, part 2b): a cycle not heard
  from, decided from the state store's answer as the bash poll decides it, a
  cycle whose work is done, and the workspaces of issues unused for 7 days.
  Nothing calls it yet.

### Adopter action

- Drain before you install (`tehas drain`): the state store moves from schema
  7 to 8 (#786). After the update, `tehas limit clear`.

## v0.14.0 (2026-10-26)

### Changes

- The state store keeps the action journal (#254): `store poll-begin`,
  `journal-record`, `journal-attempt`, `journal-settle` and `journal-open`
  record what a poll means to write before it writes it, and how the write
  ended. A comment is recorded by its sum and length, not its text. `store
  prune` now also removes old rows of `polls`, never one an unsettled action
  names. Nothing calls the journal yet: the dispatcher in Rust that writes
  through it (`console/dispatch`, `apply`) is not used by the poll.
- The fixer is now the millwright (#676, first stage). Its settings are
  `TEHAS_MILLWRIGHT_LOGIN`, `TEHAS_MILLWRIGHT_MAX_RETRIES` and
  `TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS` (#688); the `TEHAS_FIXER_*` names still
  load, with a warning, until a later release removes them. `lib/fixer.sh` is
  `lib/millwright.sh` (#689), and analytics and the MCP server say
  `millwright` and still read records that say `fixer` (#687). The entries
  for #686, #690 and #691 below say what the store, the dispatcher and
  `tehas api` do with both names.
- A cycle's `/tmp` is half the container's memory ceiling, about 2.7 GiB on a
  5.35 GiB ceiling, in place of 8 GiB (#761). Files in a tmpfs are memory the
  container cannot get back, so a full `/tmp` used to end in the kernel killing
  the agents; now it gives "no space left on device". A container with no
  usable ceiling is not started.
- `tehas work` counts the memory kills that end a cycle (#762): once the
  container has ended, it also counts the kernel log's `oom-kill:` lines for
  the container's own cgroup, and keeps the larger of that and the cgroup's
  last count. Reading the kernel log needs the tehas user in
  `systemd-journal`; without it the count is the cgroup's, as before.
- The host's `gh` shim appends one line per GitHub write to
  `state/github-writes.log` (#650), and the container's `gh` shim tries a
  rate-limited call once more (#681).
- `tehas status` adds `tehas:shipped` only to a shipped issue that lacks it,
  not to every shipped issue on every poll (#731).
- A cycle's kept record keeps its `dispatch-slow` events, so a child that
  repeats itself shows as such (#723).
- `tehasbot review` runs pi-rukas `lens-review` on the default branch since
  the last reviewed commit (#507). It files nothing yet, and no timer runs it.
- `scripts/check-bash-lines.sh` names the files that grew, and how many lines
  to take out, when the bash is over its ceiling (#757).
- Tests: `test/sandbox_test.sh` no longer pins `libexec/work` line numbers
  (#738). The two slowest dispatch test files are split, and two timing cases
  count or bound what load cannot stretch, so the gate no longer times out on
  a loaded runner.
- Each poll records what it cost in the state store: its wall time, its CPU
  time, its own `gh` calls and the writes the host shim logged while it ran,
  by kind (#743). `tehas status` shows them on a `poll` line under the
  `factory` line, with the sums of the last hour, and `store poll-show` gives
  them as JSON. The state store moves to schema 7, which adds the table
  `poll_cost`; the install migrates it and keeps a backup.
- A poll's count of the running cycles no longer grows with the lock files of
  every issue the host ever ran (#732). The new state store verb `locks-held`
  tries every lock under `state/cycles` in one process; a held lock counts
  whether or not the store has the cycle on record. A probe of one lock no
  longer runs `true`, and the 7-day workspace sweep dates an issue's names
  with one `stat` in place of one `find` a name. On a dry-run poll over 1,002
  lock files, flock ran 4,009 times before and once after, for the poll's own
  lock, and `true` no longer runs at all.
- `tehas history` and `tehas export` give every line a cursor of its own,
  `<seconds>-<issue>-<place>`, as `1760700000-12-41`, and list the lines in the
  order the state store wrote them (#754). A program that keeps `next` no
  longer misses a line that was written after its read for the same second,
  and a cycle and a planning turn of one issue that ended in one second no
  longer share a cursor. A cursor of the old shape, `1760700000-12`, is still
  taken and read as before. The state store moves to schema 6, which numbers
  every line of history in one sequence; the install migrates it and keeps a
  backup.
- `tehasbot cycle` starts fewer processes at the end of a cycle: it reads the
  start, the result and the pull requests in one jq call each, and the marker
  files and words it filters without head, tr, dirname or date. The park
  marker, the improvement record and the comment cleaner filter text the same
  way. Nothing it decides changes. Four test files wait less: review_test,
  stop_test, top_view_test and top_frame_test.
- Loading the settings starts no subshell and no program but `nproc`: 2
  process starts a load instead of 17, about 14 ms instead of 44 ms. Every
  command and every test loads them.
- The state store's verbs take and write the millwright's names (#686). An
  `approval-record --how millwright` and the old `--how fixer` both write a row
  of `how` `millwright`, and `approval-current` answers `millwright_starts`
  beside the `fixer_starts` the shell still reads, both the same number. A
  `plan-answer` is marked by `--millwright` or the old `--fixer`, and
  `plan-show` gives `millwright` beside `fixer` in each answer, both the same
  mark. A `plan-note` of either `--kind millwright-refused` or the old
  `fixer-refused` is written as `millwright-refused`, and `plan-show` lists it
  under `millwright_refused` beside `fixer_refused`. `history-show --how
  millwright` keeps the rows of both words. An old imported record with `how:
  fixer`, `fixer_starts`, an answer's `fixer` or a `fixer_refused` list becomes
  rows under the new names. A record that names one of those fields twice, or
  gives it under both names with two different values, is held: the import says
  what the file named twice rather than keeping the value it read last. The old
  spellings are accepted until the shell's own readers move to the new ones, and
  no refusal of a verb names them.
- `tehas-dispatch.service` removes `state/dispatch.again` before the poll
  runs, so a poll that stops on bad settings no longer makes
  `tehas-dispatch.path` start polls until systemd fails it (#737).
  `tehasbot health` warns when a tehas unit has failed (`checks.units`), and
  `install.sh` resets the failed tehas units and starts a failed
  `tehas-dispatch.path` again.
- The factory writes the millwright's names and reads both (#690).
  `tehas labels` makes `tehas:millwright-escalated` beside
  `tehas:fixer-escalated`, and a start or a refresh treats the two alike: a
  start takes both off, either drops a refresh. A comment marked
  `<!-- tehas:millwright issue=<n> action=... -->` counts as one marked
  `<!-- tehas:fixer ... -->`, as a retry's context and as a planning answer,
  and a refused reply names the new marker. The dispatcher tells the store
  `--how millwright`, `--millwright` and `--kind millwright-refused`. The Rust
  dispatcher reads the same. The old label and marker are accepted until a
  later release drops them.
- `tehas api` names the millwright (#691). `--json` lists the feature
  `millwright-grant` beside `fixer-grant` and has the field `millwright` beside
  `fixer`, both the login or `null`. The old names stay until a release removes
  them. The text form prints `millwright` in place of `fixer`, with its labels
  two columns wider. The manual says "millwright" where it said "fixer", and
  "The fixer's grant" in `operating.md` is now "The millwright's grant"
  (`#the-millwrights-grant`).

- The dispatcher's shadow (#253): `tehas-dispatch`, the dispatcher's rules
  in Rust, decides beside every poll of `tehas-dispatch.service`, while
  `~/tehas/state/dispatch-shadow.on` is there, from its own
  snapshot (one GraphQL query of the open issues in pages of 100, the state
  store and the host) and adds one line a poll to `logs/dispatch-shadow.log`:
  whether it agrees with the bash poll, and the lines only one of them
  logged. It acts on nothing; the bash poll still does the work. It is off
  unless that file is there: when on, measured against this repository's
  queue, it adds about 9 s of wall time before each poll, 3 s of CPU and 20
  GitHub calls, all reads, about 10 GraphQL points, since it reads GitHub
  again rather than reusing the poll's reads, one call for each issue that
  asks a question. `install.sh` and the release
  build and install `libexec/tehas-dispatch`; a host without it polls as
  before.

### Adopter action

- None for the shadow: it is off until `touch ~/tehas/state/dispatch-shadow.on`.
- A program that reads `tehas history` or `tehas export` and checks the shape
  of a cursor must take the third number. One that only keeps `next` and
  passes it back needs no change.
- A program that reads the field `fixer` or the feature `fixer-grant` of
  `tehas api` moves to `millwright` and `millwright-grant` before a release
  removes the old names.
- Drain before you install (`tehas drain`): the state store moves from schema
  5 to 7 (#754, #743). The install migrates it and keeps a backup.
- After the update, run `tehas labels` to make `tehas:millwright-escalated`.
- Rename `TEHAS_FIXER_LOGIN`, `TEHAS_FIXER_MAX_RETRIES` and
  `TEHAS_FIXER_MAX_PLAN_ANSWERS` in the instance's settings to their
  `TEHAS_MILLWRIGHT_*` names. The old names load, with a warning, until a
  later release removes them.
- Optional: add the tehas user to `systemd-journal` so `tehas work` can count
  the memory kills at a container's end (#762).

## v0.13.0 (2026-10-25)

### Changes

- The files under `live/` that describe the cycles and the factory's health
  have one writer, the state store's `publish` (#252). `tehas work` records a
  heartbeat on the store every 10 seconds (`cycle-heartbeat`) in place of
  writing `live/cycles/<n>.json`, and the poll's stuck rule reads it. The
  store is schema 3 (`heartbeats.text`); `install.sh` migrates it. The shapes
  of the files are unchanged. `status.json`, `platform.json` and
  `energy.json` are written through temporary names of their own.
- `tehasbot clean` keeps the host's state in bounds (#257): it removes the
  logs of cycles, planning turns and index runs older than 7 days whether or
  not the improvement loop is on, makes a copy of the state store once a day
  (`store backup`, `state/backup/tehas-<YYYYMMDD>.db`, 7 kept) and removes
  rows older than 90 days (`store prune`). The poll's sweep removes the lock
  file of an issue nothing has used for 30 days (`store sweep-locks`).
  `tehasbot health` warns when the newest backup is missing or more than two
  days old (`checks.backup`). The manual has a restore procedure.
- The console, analytics, the MCP server and the store share one library,
  `console/core`: the mask, the clock and dates, the bounded file read and two
  argument grammars. A change there rebuilds all four programs.
- `store migrate` writes the backup's path as JSON. A path with a control
  character in it was written in a form no JSON reader takes.
- The bytes of a file attached to an issue are read by a new program,
  `libexec/attach` (`console/attach`), and no longer by the shell: the size,
  sha256, image type and text check of every fetched file, and the unpacking of
  a zip for a cycle and for the Protolab. The zip is read by its central
  directory instead of `zipinfo` and `unzip`. The rules and the reasons are as
  before. `lib/unzip.sh` is gone.
- `tehas:blocked` on its own now starts the issue once every issue its marker
  names is closed. It approves the start as `tehas:ready` does: only from an
  allowed labeller, and only while the text is the one it approved. The start
  takes `tehas:blocked` off. A lone `tehas:blocked` from the fixer still only
  holds the issue.
- The state store is schema 4 (#685). A cycle's start may say `millwright`
  beside `fixer`, a planning note `millwright-refused` beside `fixer-refused`,
  and `planning_answers.fixer` is named `millwright`. Every row is kept. The
  verbs still write and print the old words.
- A cycle that parks at explore as `underspecified` or `too-large` is refined
  once on its own, on the word of the account that started it
  (`host/docs/operating.md`, "Refined on its own"). The store lists such parks
  (`park-show --refine`) and spends each once (`park-retry`). The store is
  schema 5: `planning.auto` and `plan_history.auto` mark such a session, and
  `plan-begin` takes `--auto`. `tehas history` has `auto` on its turns.
- A refine that files ends the session: the filed issue says `Replaces #<n>`
  and is a sub-issue of the original, and the original says `Replaced by #<m>`
  and is closed with `tehas:replaced`, or kept open with `tehas:refined` when
  later parts remain. This holds for a `tehas:refine` a person applied too.
- The improvement loop uploads a cycle's log only as `console --mask` leaves
  it. A log the mask cannot read stays on the host, and the run says
  `not-masked`.
- A count setting of 10 digits or more, and a model role with no name
  (`PI_ENSEMBLE_MODEL_=`), stop the command with exit 78. They used to load.
- A retry tick held on one issue's image skips that issue; the rest of the
  poll goes on.

### Adopter action

- Drain before you install (`tehas drain`): a cycle started by the earlier
  `tehas work` writes `live/cycles/<n>.json` itself, which the publisher now
  owns. `install.sh` takes the store to schema 5. `tehas limit clear` lifts the drain
  after the install.
- `install.sh` builds and installs `libexec/attach` with the other programs. A
  host without it does not start a cycle given an issue's attachments, and its
  Protolab leaves zips out.
- Before updating, look at the open issues that carry `tehas:blocked` and no
  `tehas:ready`. One whose marker names only closed issues starts on the first
  poll after the update. Name an open blocker in a new marker comment, or take
  the label off, for any that should keep waiting.
- Run `tehas labels` to update the description of `tehas:blocked` and to make
  `tehas:replaced` and `tehas:refined`.

## v0.12.0 (2026-10-24)

### Changes

- The state store's schema is version 2. `install.sh` migrates a version 1
  `state/tehas.db` in place, after its backup, and keeps every row: the step
  only adds columns and one table. The store gains the verbs `history-show`,
  `park-told` and `plan-note`, and `approval-record --input-file` and
  `plan-answer --text-file`.
- The analytics program no longer reads a cycle's history, approval, settings
  and commit files, or a planning record, itself: `analytics run` and
  `analytics plan` take them as facts in a file their caller writes
  (`--facts <file>`). The rows are the same. The file is
  `tmp/analytics-facts.*`; `tehasbot clean` removes one a killed call left.
- A cycle's records are in the state store, no longer files of
  `state/cycles/`: the approval the dispatcher records before the label (in
  place of `<n>.approved.json`), the cycle's start and its account (`.taken`,
  `.by`, `.requeued`, `.rukas`), the image and settings `tehas work` saw as the
  container started (`.image`, `.settings`), the end with its branch (`.branch`)
  and one line of history per end (`<n>.history.jsonl`). An approval starts one
  cycle. A cycle started by `tehas-cycle@.service` needs the dispatcher's
  approval and exits 77 without one, 75 when the store stays busy and 78 when
  it cannot be used, before any label or comment; one typed by hand is on
  record as `hand`. `tehas work` exits 4 when the approval of its start cannot
  be read, and never takes that for a run by hand. `tehas stop` ends the record
  of a cycle killed outright. `tehas dispatch`, `tehasbot cycle`, `tehas stop`,
  `tehas history` and `tehas export` exit 78 when the store is not installed.
  `tehas history` lists the cycles from the store.
- `host/systemd/tehas-cycle@.service` sets `TEHAS_CYCLE_FROM=unit`.
- A park and its question are in the state store, no longer
  `state/cycles/<n>.park` and `state/park-<n>.id`. `tehasbot cycle` puts the
  park on record before it posts, and asks no question (the plain comment,
  no `tehas:question`) when the store does not take it. A tick counts only in
  the question of the issue's current park; a store that cannot be read skips
  the issue for that poll, and never unticks. A start takes the park's
  question off the record and spends its one retry after an outage before the
  issue is marked, and puts the question back when the mark fails. The retry
  after an outage is owed only to a cycle started on an approval, rests on the
  account that started it, and is spent by `tehas stop`; a cycle started by
  hand says it waits for a person. The hold of a relabelled start on an
  unchanged body is told once per park through the store.
- The poll keeps a stuck cycle's episode and its asks to stop in the state
  store, no longer in `state/cycles/<n>.quiet`. The timing and the log lines
  are as before. When the store is busy and cannot record the episode, the
  poll asks nothing on that poll and says so; a store that cannot be used ends
  the poll with exit 78. A cycle whose record has ended, or that has none, is
  not stopped by the poll, which says so on every poll for a person. A dry run
  says what the poll would do about each quiet cycle. `tehas stop` no longer
  removes a `.quiet` file.
- A planning session is in the state store, no longer `state/plan-<n>.json`
  with its `.brief`, `.answer` and `.turn`: the session, its brief, each
  answer with its text, the issues it filed, its open question, the comments
  the poll spoke about once, and one line of history per turn (in place of
  `state/cycles/plan-<n>.history.jsonl`). Each change is one write. What the
  runner carries between turns moves out of `state/` to
  `work/<name>-plan-<n>.keep`, beside the workspace; `.out`, `.rc` and `.lock`
  keep their names. A turn's end the store would not take keeps the turn's
  exit status, and the next poll posts it once. A session pi-rukas lost ends
  with the outcome `lost`. An answer that begins a turn answers its question on
  record, so a second tick in it runs nothing. The poll reads the open
  sessions once: an issue with one open starts no cycle, no refresh and no
  outage retry; a list that cannot be read ends the poll (1 when busy, 78 when
  the store cannot be used). `tehas history` lists the planning turns from the
  store, with `started` and `status`, which it left out before, and `label` on
  every line. `tehas export`'s `plan.json` is the session as the store has it
  (`plan-show`), a new shape. `tehasbot health` exits 1 or 78, writing nothing,
  when the store cannot list the sessions.
- `install.sh` imports the records an older tehas kept as files under
  `state/` (the cycle's, the park question ids, the planning sessions) into the
  store with `store import`, and moves each file to
  `state/imported-<YYYYMMDD>/` once its rows read back. Nothing is deleted.
  It holds `state/dispatch.lock` from before its first copy until after the
  import, and stops before the first copy, installing nothing, while a cycle or
  planning turn runs and such files are there, or when a file cannot be
  imported. Each `state/plan-<n>.json.keep` moves to
  `$TEHAS_WORK_DIR-plan-<n>.keep`.
- `install.sh` no longer leaves the commands of the earlier layout for a later
  run while a poll holds the dispatcher's lock: it waits for the lock instead,
  up to `TEHAS_DISPATCH_LOCK_WAIT` seconds (900 by default).
- A poll that finds a record file of an older tehas in `state/` (one the
  import did not take, or one a cycle that ran the old code across the update
  left) stops with exit 78 before it calls GitHub, and logs `ERROR: records of
  an older tehas at <path>; run install.sh again`. `install.sh` run again
  imports it.
- `tehas status --json` has a JSON Schema, `api/status.schema.json`, and two
  samples the tests make from the code, so a program can test against what
  tehas prints. The factory API page now lists `room.refreshes`, and says that
  `room.cycles` and `room.maxAwaiting` can be below 0. The object is unchanged.

### Adopter action

- Before updating, drain: `tehas drain --wait 3600`, and let planning turns
  end. `install.sh` refuses while a cycle or planning turn runs and old records
  are there. After the update, `tehas limit clear`. Remove
  `~/tehas/state/imported-<YYYYMMDD>/` once satisfied. If the poll logs
  `records of an older tehas`, run `install.sh` again.
- A cycle or poll that was running keeps the commands it started with, and
  those call `analytics run` and `analytics plan` without `--facts`: the new
  program refuses that with exit 64 (`WARN analytics run: exit 64` in the
  cycle's log), so that run or planning turn is missing from the analytics
  report. Nothing else about the run changes.
- Run `install.sh` with its units step, so the cycle unit sets
  `TEHAS_CYCLE_FROM=unit`; a unit file not refreshed is still known by
  systemd's `INVOCATION_ID`.

## v0.11.1 (2026-10-24)

The first published release of v0.11. The v0.11.0 tag has no release: its
build failed on the aarch64 tarball. This release is v0.11.0 without it; see
v0.11.0 below for what changed since v0.10.6.

### Changes

- A release has only the `linux-x86_64` tarball again. The aarch64 cross
  build could not link, and no aarch64 host can run tehas yet.

### Adopter action

- As for v0.11.0.

## v0.11.0 (2026-10-24)

From the default branch. It carries the GitHub App work of v0.10.6 and the
state store, which nothing reads yet. The rest of epic #244 comes in a later
release.

### Changes

- A cycle's container may use every CPU of the host but one, not a fixed
  share. `--cpu-shares` splits them when several gates run at once. A
  resource change of the container: its mounts, environment, memory and pids
  limits do not change.
- The conflict check runs every `TEHAS_REFRESH_EVERY` minutes (1 to 1440,
  default 10). `tehas-refresh.timer` fires every minute and `tehasbot
  refresh` returns at once when its last run is younger than that.
- Refreshes run in their own lane of `TEHAS_REFRESH_SLOTS` (0 to 8, default
  1) and no longer count against `TEHAS_TICKET_CONCURRENCY`. The lane keeps
  a slice slot, so a 4-slot host runs 3 cycles and 1 refresh. `0` keeps the
  old sharing.
- The conflict check also refreshes a pull request handed off with
  `needs-human-attention`, and the refresh clears the handoff.
- `tehas tidy` adds `tehas:shipped` when a merged pull request closed the
  issue, not only for the newest closed issues.
- When free disk falls under `TEHAS_CLEAN_MIN_FREE` (default 30g), `tehasbot
  clean` removes kept workspaces' build output and Docker's build cache at
  once instead of waiting for their age.
- A cycle ended by an abort gets an account in its `err.txt`, as a cycle
  ended at a cap does, instead of an empty file.
- The installer and `tehas update` fetch the release asset for the machine's
  architecture (`uname -m`), and stop before downloading on one they do not
  know.
- `tehas setup` takes a GitHub App in place of a token: `--app-id`,
  `--app-installation` and `--app-key-file`, all three or none. It checks the
  app's permissions and that it reaches the repository before it writes
  anything, then installs the key and writes the three settings.
- Fewer processes per command, for the same results. Loading the settings
  checks each value in the shell, the commands read the clock from the shell,
  and `install.sh` leaves the record of steps owed as it is when a copy adds
  nothing to it. The tests of `install.sh`, the status, the update, planning
  and the cycle record fork 15 to 44% fewer processes.
- `install.sh` builds and installs the state store, `~/tehas/libexec/store`,
  and gives it its database: `store init` makes `~/tehas/state/tehas.db` when
  there is none, and every later install runs `store migrate`, which first
  copies the database into `state/backup/`. `state/` is kept mode 700. A
  store that fails stops the install with exit 1. Nothing reads the database
  yet.
- A release carries the store and a `COMMIT` file with the commit it was
  built from. `install.sh` refuses a tree whose commit it cannot read.
- `tehasbot top --every` reads the clock from bash instead of starting
  `date` between snapshots. A `date` stopped mid-write printed "Broken pipe"
  into the unit's log, and made a test fail now and then.
- The store has the verbs for the cycle, park, question, approval and
  planning records (`store cycle-start`, `park`, `plan-begin` and the rest).
  Nothing calls them yet; the shell commands move to them in later releases.
- The store waits its turn under load: a writer asks for the lock every
  millisecond for up to 5 s, and a reader waits up to 1 s. Tests prove it
  under 8 writers and 8 readers, and with a writer killed mid-transaction.
- `tehas update --pending` runs the install steps an update recorded, for a
  host whose platform checkout cannot move.
- `install.sh` records `tehas labels` as an update step when the labels
  program changes.
- A fresh cycle does not start on an issue whose last cycle parked on the
  same issue text.
- The improvement loop can skip findings about pi-rukas
  (`TEHAS_IMPROVE_RUKAS_SKIP`).
- The name lint checks file names too, and reads them NUL-separated.

### Adopter action

- None. The first install after this release makes the database. Old copies
  in `~/tehas/state/backup/` are not removed; remove them by hand when there
  are too many.
- With `TEHAS_REFRESH_CONFLICTS` on, the refresh lane takes one cycle slot
  from new cycles. Set `TEHAS_REFRESH_SLOTS=0` to keep the old sharing.

## v0.10.6 (2026-10-24)

A patch release from v0.10.5, not from the default branch: it carries the
GitHub App work (#639) and nothing of epic #244.

### Changes

- A factory can act as a GitHub App instead of a machine user.
  `TEHAS_GITHUB_APP_ID`, `TEHAS_GITHUB_APP_INSTALLATION` and `TEHAS_BOT_LOGIN`
  name it, all three or none; the key is `~/.config/tehas/github-app.pem`.
  `tehasbot github-token` makes and keeps its one-hour installation token, and
  `--check` says whether it reaches the repository.
- With an app named, the host's `gh` and git act as the app through
  `~/tehas/shim/gh`, a user `GH_TOKEN` is dropped, and the dispatcher, the
  refresh and the Protolab take the bot's login from `TEHAS_BOT_LOGIN`.
- A cycle's container reads the app's token from `~/tehas/state/github`
  (read-only, never the key) through the image's own `gh`, and `tehas work`
  keeps the token fresh while the cycle runs. A security change: the container
  is given one more read-only directory.

### Adopter action

- None for a factory that keeps its machine user: without the three settings
  nothing changes. The sandbox image is rebuilt by the update.
- To move to an app: make one as `docs/specs/2026-10-24-github-app.md` says
  (under "For adopters"), put its key at `~/.config/tehas/github-app.pem` with
  mode 600, add the three settings to the instance, set git's `user.name` and
  `user.email` for the tehas user to the app's (`<slug>[bot]` and
  `<id>+<slug>[bot]@users.noreply.github.com`), and run
  `tehasbot github-token --check`.

## v0.10.5 (2026-10-23)

### Changes

- The dispatcher starts no cycle while the disk of `~/tehas`, or the one
  Docker keeps its images on, has less than `TEHAS_CYCLE_MIN_FREE` free (10g
  by default). Running cycles and planning turns are not held.
- `tehas stop` leaves a marker, and the cycle's record names who stopped it
  and when.
- The workspace sweep also removes what a cycle left outside its workspace:
  the agents' transcripts, the console file and the planning session's
  transcripts of that issue, on the same 7 day rule.
- A second `tehas update` waits for a running one, `TEHAS_UPDATE_LOCK_WAIT`
  seconds (10 by default), before it says another update is running.
- `tehasbot clean` prints what it logs, so a timer run's output is in
  `journalctl --user -u tehas-clean` too. Each line was written to
  `clean.log` twice.
- The front page's Shipped figure counts every closed issue with
  `tehas:shipped`, not the fifty newest.
- `tehas_headless_mode` answers `error` for an answer it cannot read, never
  `polyfill`.
- The console's state store backs itself up before it migrates.
- `install.sh` starts fewer processes for each file it copies, so an install
  or `tehas update` takes about half the time it did. What it installs and the
  follow-ups it records are unchanged.

### Adopter action

- A host with less than 10 GiB free starts no new cycle after the update. Free
  space, or set `TEHAS_CYCLE_MIN_FREE` in the instance's `instance.env`.

## v0.10.4 (2026-10-23)

### Changes

- `tehasbot clean` lists the base image's repository and the sandbox image in
  two calls of `docker image ls`, which takes at most one name. In v0.10.3 it
  asked both in one call, docker refused, and the clean ended with exit 1
  having removed no old base image.
- The head of the default branch has a preview of its own at
  `uat-main.<domain>`, built from its `preview/Dockerfile` like a pull
  request's and listed first under Previews as `main`. It does not count
  against `TEHAS_PREVIEW_MAX` and nothing removes it.
- `tehas dev` keeps a session's cargo build output in
  `~/tehas/dev/<name>.state/target`, so a later session of the same clone does
  not rebuild from scratch. The cleaner does not touch it.
- `install.sh` installs every `lib/polyfill-*.sh` into `~/tehas/lib/`.
- A sandbox rebuild is asked for only when `versions.env` changes in a line
  the image build reads, not for any change to the file.
- `tehas update --check` on a host installed from a checkout lists the
  releases since the newest tag at or before the checkout's HEAD.
- `tehas api` reports the release tag of a release install, where it said no
  release.
- The console's state store has its first schema and migrations. Nothing reads
  it yet.
- `tehas update` prints `later: <checkout>/bootstrap.sh user` when the sandbox
  image was built on a pi-rukas commit the host's clone is not on, even when
  `versions.env` did not change. Until then `install.sh --check` fails on
  pi-rukas.
- When a test file fails, the gate's summary line names the first failing case
  as well as the file.

### Adopter action

- If `tehas update` prints the `bootstrap.sh user` line, run it as the tehas
  user. A host that followed pi-rukas to a new release since its last
  bootstrap needs it once.
- A proxy in front of the edge router has to pass the name `uat-main` as well
  as `uat-<pr>`. Without a `preview/Dockerfile` on the default branch nothing
  is built.
- After `tehas update`, the first clean removes the old base images v0.10.3
  left behind.

## v0.10.3 (2026-10-22)

### Changes

- `tehasbot clean` removes every old `sha-` tag of the base image's
  repository, not only the one the pin names, so each `tehas update` no longer
  leaves another base image behind. The current base, a base a container was
  made from and a base a kept sandbox tag stands on are kept.
- A `tehas:refine` planning turn is told that the issue it discusses is the
  one its plan replaces, so it no longer parks itself at its first turn as a
  duplicate.
- `tehas update` gives its lock only to `install.sh`. A child that outlived the
  update kept the lock, and the next update said another update was running.

### Adopter action

- None. The first `tehasbot clean` after the update removes the old base
  images that earlier updates left behind, several GB each.

## v0.10.2 (2026-10-22)

### Changes

- `tehas check-product --sandbox` runs the gates on a work branch one empty
  commit past the default branch, as a cycle's gate runs on its own branch. A
  gate that refuses to certify a diff of HEAD against itself, such as a
  file-size ratchet, no longer fails `tehas setup` at the default branch's tip.
- An entry of a zip linked in an issue reaches a cycle as an image only when
  both its bytes and its extension say it is one. A file named `x.html` that
  holds PNG bytes is given as the HTML file it is named.
- `tehas:refine` and `tehas:plan` start a planning session on a parked issue,
  one that carries `tehas:question`, as `tehas:ready` starts a cycle on one.
  The planning queue dropped such an issue without a word.
- The MCP server's tests wait for the log lines they check, so they no longer
  fail now and then on a loaded machine.

### Adopter action

- Run `tehas update` on a host whose `tehas setup` stopped at the sandbox step
  with "compared nothing" or another complaint of a gate about its base, then
  `tehas setup` again.

## v0.10.1 (2026-10-22)

### Changes

- `tehas analytics report` says how issues of each size fare: the starts
  measured in the window from `state/sizes.jsonl`, in the buckets 1-2, 3-4 and
  5+ named files, with how many of each shipped and their median lead time, so
  the threshold of `TEHAS_SIZE_MAX_FILES` can be set from outcomes.
- Both state ledgers the report reads are now bounded the same way, and both
  writers trim: `state/refined.jsonl` keeps its last 500 lines as
  `state/sizes.jsonl` does, the readers take a plain file of at most 1 MiB, and
  a stamp of a year past 2100 is refused before it can overflow the date
  arithmetic.
- `tehas status` writes `live/status.json` on a repository with many issues.
  The list of every issue went to `jq` as one argument, which Linux caps at
  128 KiB, so `jq` failed unseen and the status said the dispatcher had written
  no snapshot. The lists now go in as files, and a snapshot that cannot be
  written says why on standard error. The health file reads a big snapshot,
  and the protolab long comment threads and many tickets, the same way.
- `tehas check-product --sandbox` clones the product as a cycle's workspace is
  made: the default branch checked out, with `origin/<default branch>` and
  `origin/HEAD` set. A gate that diffs against `origin/main` no longer fails
  `tehas setup` for want of a base.
- `tehas status` says a release is out only when the newest is greater than
  the one installed. A cached newest older than the host's release is dropped,
  and the next poll asks GitHub again.
- Every sandbox container (a cycle, a planning turn, a dev session, the push
  check) has a `/tmp` that is a size-bounded tmpfs, so a runaway log ends that
  container at its memory ceiling instead of filling the host's disk.
- The gate, `scripts/verify.sh`, needs cargo and runs the tests of the whole
  workspace. The front page's node tests run where node is installed.

### Adopter action

- Run `tehas update` on a host that shows "No queue snapshot yet" with a long
  issue list, or whose `tehas setup` failed at the sandbox step on a base
  branch.

## v0.10.0 (2026-10-22)

### Changes

- The cleaner removes images too: the sandbox image's dated tags older than
  `TEHAS_CLEAN_AFTER` hours, keeping the newest and any a container or the
  `uid` tag holds, and the pulled base image of that age, by its own name. No
  volume is removed, no image a container was made from, and no preview image.
- A zip linked in an issue reaches the cycle with `TEHAS_WORK_ZIPS=1`, which
  needs `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`: at most 2 archives of
  4 MB each, every file in them checked as a linked file is.
- With `TEHAS_SIZE_MAX_FILES` set, the dispatcher holds a `tehas:ready` issue
  whose approved body names more files than that: the label comes off,
  `needs-human-attention` goes on, one comment says why. Unset, nothing changes.
- A cycle's start records the size of its approved body in
  `~/tehas/state/sizes.jsonl`; `TEHAS_SIZE_MAX_BYTES` and
  `TEHAS_SIZE_MAX_LINES` bound the measure. Nothing reads the ledger yet.
- `tehas setup` gives the host's git the gh credential helper for github.com,
  so a private product is reachable by plain git.
- `install.sh` builds the programs with cargo `--offline` first and fetches
  only when a crate is missing from the cache.
- The MCP server's live-file tools (`factory_status`, `factory_issues`,
  `factory_endings`, `factory_console`, `factory_stats`) describe their output
  and every input in `tools/list`.
- The analytics program opens a new database safely from several processes at
  once.
- The console, the analytics program and the MCP server are one cargo
  workspace.
- tehas can carry a pi-rukas feature that has no release yet as a polyfill,
  `lib/polyfill-<feature>.sh`, used only while the sandbox image's
  `--capabilities` lacks the feature. The tag script removes a polyfill the
  release's pi-rukas no longer needs, in a pull request of its own. None is
  carried yet.
- A cycle whose runner never came up ends with its own cap,
  `runner-never-started`, naming the container's exit status.
- On a host that runs a release, `tehas status` names the installed release
  and whether a newer one is published, instead of the platform checkout.

### Adopter action

- The cleaner now removes old sandbox tags and the pulled base image. A host
  that disabled `tehas-clean.timer` while waiting for this can enable it
  again: `systemctl --user enable --now tehas-clean.timer`. A dated tag older
  than `TEHAS_CLEAN_AFTER` hours (24 by default) is no longer there to roll
  back to; the `release-<version>` tags stay.
- `TEHAS_WORK_ZIPS`, `TEHAS_SIZE_MAX_FILES`, `TEHAS_SIZE_MAX_BYTES` and
  `TEHAS_SIZE_MAX_LINES` are new optional settings; see
  `instances/example/optional.env` and the manual. Nothing changes until set.

## v0.9.0 (2026-10-21)

### Changes

- tehas is installed from a release: a signed tarball (SHA256 and minisign)
  published on GitHub for each tag, unpacked side by side under
  `~/tehas/versions/<version>` with `~/tehas/current` pointing at one.
- `tehas update` installs releases: `--check` shows the installed and newest
  version and the notes between them, `--to vX.Y.Z` pins or rolls back.
- pi-rukas follows the newest tag of a major version, `PI_RUKAS_FOLLOW=release:0`
  before 1.0, so a new pi-rukas arrives with `tehas update`. An instance may set
  `PI_RUKAS_FOLLOW` itself, down to one commit.
- One root command prepares a host, `tehas setup` writes the instance, and
  `tehas check-product` and `tehas init-product` check and scaffold the
  product's files.
- The instance names the session model once, in `instance.env`. Its
  `settings.json` is optional, and a `models.json` entry needs only `id`,
  `baseUrl`, `api` and `apiKey`. A product's `.pi/settings.json` is optional.
- A first install puts the `env.sh` line in `~/.bashrc`, makes the labels and
  enables the core units. A cycle's clone keeps pi-rukas's state files out of
  git by `.git/info/exclude`.
- The MCP server shows a key holder the sessions of the last 30 days, why they
  failed and their masked logs (`factory_failures`, `factory_sessions`,
  `factory_session`, `factory_log`), and its handshake says which tools to call.
- `tehas:refine` is counted: how much it is used and what became of its issues.
  A refined issue's title starts with its conventional-commit type.
- A beesknees26 theme for the front page.
- The gate does not run a fork's pull request on the self-hosted runner.

### Adopter action

- A host installed from a git checkout keeps working from it. To move it to
  releases, follow "Move a checkout host to releases" in the manual
  (`host/docs/host.md`); until then `tehas update` updates the checkout as
  before.
- Read the new settings in `instances/example/` and compare your instance
  with them; `install.sh --check` names any that are missing.
- An MCP key now reads 30 days of sessions and their logs, not only what the
  dashboard shows. On a factory that works on a private repository that
  includes code, issue text and model output: check who holds a key.
