# Changelog

What each release of tehas changes, and what an adopter must do after
updating to it. Newest first. A release's section is `## vX.Y.Z (YYYY-MM-DD)`
with `### Changes` and `### Adopter action`; "None." is a valid action.
`scripts/tag-release.sh` refuses to tag without an Adopter action, and the
release workflow takes the release's page from its section
(`scripts/release-notes.sh`). Work in progress goes under `## Unreleased`; a
person renames it to the version in the release's pull request and starts a new
empty one above it.

## Unreleased

### Changes

- The snapshot behind `tehas top` is put together by the console (#846,
  `console top snapshot`, console/src/snapshot.rs): the factory, queue,
  cycle, units, energy and console sections, with the same JSON. `lib/top.sh`
  keeps the wait between two readings of the host and the frame, and loses
  156 lines. Without the console `tehas top` and `tehasbot top-publish` say to
  run `install.sh` instead of drawing a frame without a host.

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

None.

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
