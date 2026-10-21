# Sessions and their logs over MCP: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A key holder on the MCP server can list the factory's sessions of the last 30 days, open one, see why it failed with which versions, follow its issue and PR links, fetch any of its log files plain or gzipped, and see failures per version pair.

**Architecture:** The analytics program gains `analytics sessions`, which prints a 30-day snapshot as JSON Lines. `libexec/status` writes it each poll to a private directory, `~/tehas/mcp/sessions.jsonl`, never under `live/`. The MCP container is given that directory and `~/tehas/logs` read-only. Four new tools read the snapshot line by line and open a log only by a name the snapshot holds, masking every byte with the console's mask.

**Tech Stack:** Rust (crates `analytics` and `tehas-mcp` in the `console/` workspace, the MCP binary static musl), bash, `rusqlite`, `regex`, `flate2` (new, default pure-Rust backend), shell tests in `test/`.

**Spec:** `docs/specs/2026-10-21-mcp-sessions.md`. Read it with this plan.

## Global Constraints

- The MCP window is 30 days, held once: `SESSIONS_DAYS: i64 = 30` in `console/analytics/src/sessions.rs`, carried to the server in the snapshot's header line as `"days"`.
- The analytics database keeps 365 days. Retention and the schema do not change. No new column.
- Nothing new goes under `~/tehas/live/` or reaches the public site.
- Every request needs a key. No method, path or tool answers before `rpc::gate`. No tool reads its arguments or a file without a key.
- No path comes from a caller. A log is opened only by a name from the snapshot, checked against the pattern for that session's kind and issue, never through a link (`symlink_metadata` is a file).
- Every byte of log content the server sends is masked with `mask()` from `console/src/mask.rs`, shared by `#[path]` as `console/analytics/src/main.rs:62` does. No copy of the mask.
- A log file over `LOG_MAX = 16 MiB` is not served. Plain reads are at most `PLAIN_MAX = 1 MiB` per call. At most `LOGS_AT_ONCE = 2` log calls are served at once; another gets a tool error asking to retry.
- Every result carries `"api": 1`. Field names are camelCase. Times are ISO 8601 UTC with `Z`. A field without a value is left out.
- The MCP container stays read-only root, no capabilities, `no-new-privileges`, its own no-masquerade network, the tehas user. Its new mounts are read-only. Its memory limit goes from 64m to 256m. This is a security change and the PR says so.
- Bash: `set -uo pipefail`, two spaces, `shellcheck -x` clean. Rust: `cargo fmt`, `cargo clippy` clean, no `unsafe`, no blanket allows.
- Prose (comments, manual, commits): plain, short sentences, no em dashes. Examples name the tehas-tehas instance. `scripts/check-generic.sh` passes.
- The manual page that describes a behaviour changes in the same commit as the behaviour.

## Calls this plan makes where the spec left room

- **Choice B**, the private snapshot. The SQL stays in the analytics program, and the MCP binary gets no SQLite.
- **No log column.** `analytics sessions` finds each run's files by the stamp in their names (UTC, `%y%m%d-%H%M%S`): the latest stamp in `[started - 300, ended]`, `ended` being now for a run without an end. This works for old rows too, so the schema stays as it is and tehas-industry has nothing to agree to.
- **The mask runs on whole lines.** Files are capped at 16 MiB, so no line is cut and no overlap logic is needed.
- **Plain `offset` and `bytes` count bytes of the masked text,** and `size` is the masked size. The mask can change lengths, so this is the only consistent choice. The answer is cut back to a UTF-8 boundary, and `next` is where the following read starts.
- **`factory_failures` counts work runs only,** as `v_versions` does. The top five causes per pair, each with at most ten issue links.

## Review Focus

1. A snapshot that is missing, half-written, older than ten minutes, or has a header with no `days`. The tools still answer: a tool error naming `sessions.jsonl` when the snapshot is missing or bad, `"stale": true` when it is old. (Task 3)
2. A session whose log file was removed, replaced by a link, or grew past 16 MiB since the snapshot. A tool error naming the word, and nothing opened through a link. (Task 5)
3. The repository in `live/status.json` missing or odd (`a/b/c`, `..`, a space). Sessions are still listed, without links. (Task 4)
4. A secret cut by the caller's `offset`, or sitting on the last line of a file with no newline at the end. It is still masked. (Task 5)
5. Thirty-two clients asking for gzip of the 9 MB transcript at once. Two are served, the rest told to retry, and the container stays under 256m. (Task 5)

---

### Task 1: `analytics sessions` prints the 30-day snapshot

**Files:**
- Create: `console/analytics/src/sessions.rs`
- Modify: `console/analytics/src/main.rs` (the `sessions` subcommand and its header doc)
- Test: `console/analytics/src/tests.rs`

**Interfaces:**
- Produces: `analytics sessions`, exit 0, printing JSON Lines to stdout. Line 1 is the header `{"api":1,"sessions":1,"days":30,"generatedAt":"<iso>"}`, then one session per line, newest `started` first, ties broken by `id` descending. Each session holds:
  - the run: `id, kind, issue, attempt, how, started, ended, seconds, queueWait, outcome, exitCode, status, lastStep, failedStep, cap, parkReason, reviewHeld (bool), failureKind, failureCause, providerOutage (bool), endpoint, pr, tehasVersion, tehasCommit, rukasVersion, rukasCommit, image, resultSchema, input, output, cacheRead, cacheWrite, turns, dispatches, cost, runnerMs, oomKills, forksRefused, concurrent`
  - `steps: [{step, round, started, ended, seconds}]` and `roles: [{role, model, provider, dispatches, turns, input, output, cacheRead, cacheWrite, cost, ms}]`
  - `logs: [{word, name, bytes}]`, only the files that exist as regular files

  Exit 69 when analytics is off, 70 when the database cannot be read.
- Produces: `pub const SESSIONS_DAYS: i64 = 30;` and `pub fn log_files(logs: &Path, kind: &str, issue: i64, started: i64, ended: Option<i64>, now: i64) -> Vec<(String, String, u64)>`.
- Log words. Work: `transcript` → `<base>.jsonl`, `err`, `preflight`, `console` → `<base>.jsonl.<word>`, `result` → `<base>.jsonl.result.json`, `dispatch` → `dispatch-work-<n>.log`. Plan: `progress` → `<base>.jsonl`, `result` → `<base>.json`, `dispatch` → `dispatch-plan-<n>.log`.

- [ ] **Step 1: Write the failing tests** in `tests.rs`, with a temp `TEHAS_HOME` built the way the existing run tests build one:
  - `sessions_lists_thirty_days_newest_first`: runs started 1, 29 and 31 days ago. The output is the header plus two lines, in the order 1 day, then 29 days. The header has `"days":30`.
  - `sessions_carry_steps_roles_and_camel_case`: a run with two steps and one role. Its line has `failureKind`, `rukasVersion`, a `steps` array of 2 and a `roles` array of 1, and no key holding `null`.
  - `log_files_match_the_stamp_window`: files `work-7-261020-100000.jsonl` and `work-7-261021-100000.jsonl`, a run started at `2026-10-20T10:00:30Z` and ended at `T11:00:00Z`. Only the first matches. With `.err` beside it, `err` is listed with its size.
  - `log_files_skip_links_and_other_issues`: a symlink `work-7-…jsonl` and a file `work-77-…jsonl` are not listed for issue 7.
  - `plan_log_words`: `plan-5-<stamp>.jsonl` and `.json` give `progress` and `result`, with `dispatch` when `dispatch-plan-5.log` exists.
- [ ] **Step 2: Run them and see them fail.** `cargo test --manifest-path console/Cargo.toml -p analytics sessions` fails: no module `sessions`.
- [ ] **Step 3: Implement `sessions.rs` and the subcommand.** One `SELECT` over `runs` with `started >= now - SESSIONS_DAYS*86400`, steps and roles by `run_id`. Field names map snake_case to camelCase, as `libexec/history` does. Parse the stamp with the same UTC rule `libexec/work:331` writes it. Update the header comment in `main.rs`: the usage line, and "Ten days" becomes 365 where it is stale.
- [ ] **Step 4: Run the tests and see them pass.** Same command, all pass, then `scripts/verify.sh` (to a file; check its exit status and the FAIL lines).
- [ ] **Step 5: Manual.** In `host/docs/analytics.md`, add `analytics sessions` to where the subcommands are listed: what it prints, the 30 days, and that it is for the MCP server.
- [ ] **Step 6: Commit.** `analytics sessions prints the last 30 days of runs`

### Task 2: `libexec/status` writes the private snapshot

**Files:**
- Modify: `libexec/status` (beside `analytics_report`), `lib/analytics.sh` (the header of `tehas_analytics_to`)
- Test: `test/status_analytics_test.sh`

**Interfaces:**
- Consumes: `analytics sessions` (Task 1).
- Produces: `$TEHAS_HOME/mcp/sessions.jsonl`, mode 600, its directory mode 700, written as `sessions.jsonl.$$.new` and then `mv -f`. With analytics off (rc 2), the file is removed. On a failure, a file under `analytics_keep` (600 s) old is kept and an older one removed, as `analytics_report` does.

- [ ] **Step 1: Write the failing cases**, with the stubbed analytics program the file already uses:
  - The snapshot is written, mode 600, its directory mode 700, and its content is what `sessions` printed.
  - Nothing is written under `live/` whose name contains `session`.
  - With analytics off, an old snapshot is removed.
  - A failing `sessions` keeps a snapshot younger than 600 s and removes one that is older (set with `touch -d`).
  - `sessions` failing does not stop `analytics.json` from being written.
- [ ] **Step 2: Run them and see them fail.** `bash test/status_analytics_test.sh` reports the new cases failing.
- [ ] **Step 3: Implement `analytics_sessions()` in `libexec/status`** and call it from the same EXIT trap as `analytics_report`, after it. Make the directory with `mkdir -p -m 700`. In `lib/analytics.sh`, the comment of `tehas_analytics_to` names `sessions` as well as `report`.
- [ ] **Step 4: Run the tests and see them pass,** then the fast gate.
- [ ] **Step 5: Commit.** `status writes the sessions snapshot for the MCP server`

### Task 3: The server reads sessions and logs, and every request needs a key

**Files:**
- Modify: `console/mcp/src/main.rs` (args `--sessions <dir>` and `--logs <dir>`, both required), `console/mcp/src/http.rs` and `console/mcp/src/rpc.rs` (pass `&Dirs`; `instructions`), `console/mcp/src/tools.rs` (`call` signature, `Output`), `console/mcp/Cargo.toml` (`regex = "1"`, `flate2 = "1"`)
- Create: `console/mcp/src/snapshot.rs`
- Modify: `libexec/mcp` (mounts, args, memory, `mkdir`)
- Test: `console/mcp/tests/serve.rs`, `console/mcp/tests/rpc.rs`, `console/mcp/tests/tools.rs`, `test/mcp_test.sh`

**Interfaces:**
- Produces: `pub struct Dirs { pub live: PathBuf, pub sessions: PathBuf, pub logs: PathBuf }` in `tools.rs`. `tools::call(dirs: &Dirs, name: &str, args: &Value) -> Result<Output, ToolError>`. `pub enum Output { Json(Value), Blob { meta: Value, uri: String, mime: &'static str, blob: String } }`. For `Blob`, `rpc` answers with `content: [{type: "text", text: meta}, {type: "resource", resource: {uri, mimeType: mime, blob}}]` and `structuredContent: meta`.
- Produces: `snapshot::Snapshot::open(dir: &Path, now: i64) -> Result<Snapshot, ToolError>`, which reads the header and gives `days`, `generated_at` and `stale: bool` (older than 600 s). `Snapshot::sessions(&self) -> impl Iterator<Item = Value>` reads one line at a time and skips lines it cannot parse and lines with `started` older than `now - days*86400`.
- Produces: `rpc::INSTRUCTIONS: &str`, sent as `instructions` in the `initialize` result and in the `server/discover` result.
- Container: `-v "$TEHAS_HOME/mcp:/sessions:ro" -v "$TEHAS_HOME/logs:/logs:ro"`, `--memory 256m`, args `--sessions /sessions --logs /logs`. Before `docker run`: `mkdir -p -m 700 "$TEHAS_HOME/mcp"`, so Docker never makes it as root.

- [ ] **Step 1: Write the failing tests.**
  - `serve.rs` `every_method_and_tool_needs_a_key`: for each of `server/discover`, `initialize`, `ping`, `tools/list`, and `tools/call` with every name in `tools::list()`, send it with no `Authorization`, with a wrong key, and with `Basic <key>`. Then send a `GET /mcp` and a `POST /other`, without a key. Every answer is 401, `WWW-Authenticate: Bearer` and an empty body.
  - `rpc.rs` `instructions_name_the_tools_and_the_window`: the `instructions` text in both handshakes contains `factory_failures`, `factory_sessions`, `factory_session`, `factory_log`, `30 days` and `gzip`.
  - `tools.rs` `snapshot_missing_or_bad_is_a_tool_error`: no file, an empty file, and a header without `days` each give `ToolError::File` naming `sessions.jsonl`.
  - `tools.rs` `snapshot_old_is_stale`: a header with `generatedAt` 11 minutes ago gives `stale` true. Five minutes ago gives false.
  - `test/mcp_test.sh`: update the pinned `docker run` line at `:75` to the new mounts, memory and args. Add a case that `$TEHAS_HOME/mcp` exists with mode 700 after `serve`.
- [ ] **Step 2: Run them and see them fail.** `cargo test --manifest-path console/Cargo.toml -p tehas-mcp` and `bash test/mcp_test.sh`.
- [ ] **Step 3: Implement.** Move `live` into `Dirs` everywhere it is passed today. The five existing tools return `Output::Json` unchanged. Share the mask with `#[path = "../../src/mask.rs"] mod mask;` in `lib.rs`, as analytics does. `INSTRUCTIONS` is a few short sentences:
  - what the server is, and that it is read only
  - the order to call the tools: `factory_failures`, then `factory_sessions` with `outcome: "failed"`, then `factory_session`, then `factory_log`
  - that sessions cover the last 30 days and logs are masked
  - to ask for `gzip: true` for a transcript, and to read plain logs in pieces with `offset`
  - the manual's address `/mcp-server`

  Fixtures write `sessions.jsonl` at test time, with times relative to now, in the `Copy` temp-dir pattern in `tests/tools.rs`.
- [ ] **Step 4: Run them and see them pass,** then the fast gate.
- [ ] **Step 5: Manual.** In `host/docs/mcp-server.md`:
  - **What it shows:** the new rule from the spec, and what a key gives on a private repository.
  - **How it runs:** the two new read-only mounts, the 256 MB limit, the `~/tehas/mcp` snapshot.
  - **How an agent connects:** that every request needs a key.
- [ ] **Step 6: Commit.** `The MCP server is given the sessions snapshot and the logs, read only`. The commit body says this is a security change and lists the new mounts.

### Task 4: `factory_sessions` and `factory_session`

**Files:**
- Create: `console/mcp/src/sessions.rs`
- Modify: `console/mcp/src/tools.rs` (list and dispatch)
- Test: `console/mcp/tests/sessions.rs`

**Interfaces:**
- Consumes: `Dirs` and `Snapshot` (Task 3). `repo` from `live/status.json`.
- Produces: `pub fn links(repo: Option<&str>, issue: i64, pr: Option<i64>) -> (Option<String>, Option<String>)`. A repo is accepted when it matches `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$` and holds no `..`. Links are `https://github.com/<repo>/issues/<n>` and `/pull/<pr>`.
- Produces: `pub fn find(snap: &Snapshot, id: &str) -> Result<Value, ToolError>`, used by Task 5. An `id` matches `^(work|plan)-[1-9][0-9]{0,8}-[0-9]{9,11}$`.
- `factory_sessions` arguments, all optional:
  - `outcome`: enum `pr, merged, handoff, parked, failed, refused, stopped`
  - `failureKind`, `tehasVersion`, `rukasVersion`: strings of `^[A-Za-z0-9._+-]{1,64}$`, matched exactly. `rukasVersion` also matches `rukasCommit`.
  - `kind`: enum `work, plan`
  - `issue`: integer of at least 1
  - `limit`: 1 to 200, default 50
  - `before`: a session id

  Returns `{api, days, stale, sessions: [...], next?}`. Each session has `id, kind, issue, attempt, started, ended, seconds, outcome, failedStep, failureKind, failureCause, parkReason, tehasVersion, rukasVersion, issueUrl, prUrl`. `next` is the last id when more remain.
- `factory_session`: `{id}` required. Returns `{api, stale, session}`, with every snapshot field from Task 1 copied by an allowlist, plus `issueUrl` and `prUrl`. `logs` becomes `[{file: word, bytes}]`, and `name` is never returned.
- Both tools have a full `outputSchema` naming every field, and every input property has a `description`.

- [ ] **Step 1: Write the failing tests** in `tests/sessions.rs`:
  - `filters_combine`: `outcome: failed` and `rukasVersion: v0.14.0` together return only the sessions matching both.
  - `paging`: with `limit: 2`, sessions come newest first. `before: next` continues with no repeat and no gap. A `before` not in the snapshot is a tool error.
  - `window`: a line 31 days old in the snapshot is not listed, and `factory_session` on its id is a tool error.
  - `bad_ids`: `work-1-1/../x`, `work-01-123456789`, `plan-1-12` and `""` are tool errors, and no file is opened (the logs dir in the test does not exist).
  - `links_and_odd_repos`: `tehas-app/tehas` gives both links. `a/b/c`, `../x`, `a b/c`, and no `repo` at all give sessions without links. A session without `pr` has no `prUrl`.
  - `allowlist`: a snapshot line with an extra key `"secret"` and `logs[].name` never appears in any output path (walk it as `tools.rs` `paths()` does).
  - `schema_matches_output`: every path in the output of each tool is in its `outputSchema`, and the other way round for the fixture that fills every field.
  - `inputs_described`: every input property of both tools has a non-empty `description`.
- [ ] **Step 2: Run them and see them fail.**
- [ ] **Step 3: Implement `sessions.rs`.** Filters and paging go over `Snapshot::sessions()`, one line at a time, never collecting more than `limit + 1`.
- [ ] **Step 4: Run them and see them pass,** then the fast gate.
- [ ] **Step 5: Manual.** Add both tools to the table in `host/docs/mcp-server.md`, with their arguments, fields and the 30-day window.
- [ ] **Step 6: Commit.** `MCP lists the last 30 days of sessions and opens one`

### Task 5: `factory_log`

**Files:**
- Create: `console/mcp/src/logs.rs`
- Modify: `console/mcp/src/tools.rs`
- Test: `console/mcp/tests/logs.rs`

**Interfaces:**
- Consumes: `sessions::find` (Task 4), `Output::Blob` (Task 3), `mask::mask`.
- Produces: `factory_log {id, file, gzip?, offset?, bytes?}`. `file` is an enum of all eight words. A word the session's kind lacks, or a file the session does not list, is a tool error that names the words the session has.
  - **Plain:** `Output::Json({api, id, file, size, offset, next?, text})`, with `bytes` at most `PLAIN_MAX` and defaulting to it.
  - **Gzip:** `Output::Blob`. `meta` is `{api, id, file, size, gzipBytes}`, `uri` is `tehas://sessions/<id>/<file>.gz` and `mime` is `application/gzip`. The `blob` is the base64 of a gzip of the masked text, written by a small encoder in `logs.rs`, with no crate.
  - `offset` and `bytes` with `gzip: true` are a tool error.
- Produces: `pub fn masked(path: &Path) -> Result<String, ToolError>`. It checks `symlink_metadata` is a file and its size is at most `LOG_MAX`, reads it line by line with `from_utf8_lossy`, masks each line, and keeps the newlines as they were, including a last line without one.
- Concurrency: `static LOGS: AtomicUsize`, with a guard that decrements on drop. Above `LOGS_AT_ONCE` the answer is the tool error `two logs are being sent; try again in a few seconds`.
- Name check before opening: the snapshot's `name` must match, for a work session of issue `n`, `^work-<n>-[0-9]{6}-[0-9]{6}\.jsonl(\.err|\.preflight|\.console|\.result\.json)?$` or `^dispatch-work-<n>\.log$`, and the same with `plan` and `.json` for a plan session. It is joined to `dirs.logs` only after that.

- [ ] **Step 1: Write the failing tests** in `tests/logs.rs`:
  - `masks_plain_and_gzip`: a log holding `ghp_` plus 36 letters, `PASSWORD=hunter2`, `https://u:pw@example.com/` and `10.1.2.3`, read plain whole, and gzipped then decoded with `flate2`. None of the four comes out in either.
  - `offset_inside_a_token`: an `offset` that lands in the middle of the token returns text without any part of the token.
  - `last_line_without_newline`: a token on a final line with no `\n` is masked.
  - `pieces_join`: reading a 2.5 MiB fixture in plain pieces, following `next`, and joining them equals the masked whole. `size` is the same in every piece.
  - `utf8_boundary`: an `offset` and `bytes` that would cut a multi-byte character return valid UTF-8, and `next` points to the cut.
  - `link_and_growth`: a snapshot name whose file is now a symlink, a file over 16 MiB (sparse, with `set_len`), and a file that is gone each give a tool error naming the word.
  - `bad_names_never_open`: snapshot names `../x`, `work-8-…jsonl` on an issue-7 session, and `dispatch-work-7.log/x` each give a tool error.
  - `wrong_word_for_kind`: `transcript` on a plan session is a tool error that lists `progress, result, dispatch`.
  - `two_at_once`: holding two guards, a third call gets the retry error. After they drop, it is served.
  - `blob_shape`: in the rpc answer for gzip, `content[1].type` is `resource`, `mimeType` is `application/gzip`, and `structuredContent` has no `blob`.
- [ ] **Step 2: Run them and see them fail.**
- [ ] **Step 3: Implement `logs.rs`.**
- [ ] **Step 4: Run them and see them pass,** then the fast gate.
- [ ] **Step 5: Measure memory.** Run the release binary on a copy of a 9 MB transcript with two gzip calls at once, under `/usr/bin/time -v`. The maximum resident size is well under 256 MB. Write the figure in the commit body.
- [ ] **Step 6: Manual.** In `host/docs/mcp-server.md`, add the tool, the table of words per kind, the limits (16 MiB, 1 MiB, two at once), that offsets count masked bytes, and that the mask is a net, not a wall.
- [ ] **Step 7: Commit.** `MCP serves a session's log files, masked, plain or gzipped`

### Task 6: `factory_failures`

**Files:**
- Modify: `console/mcp/src/sessions.rs`, `console/mcp/src/tools.rs`
- Test: `console/mcp/tests/sessions.rs`

**Interfaces:**
- Consumes: `Snapshot`, `links`.
- Produces: `factory_failures {}` returning `{api, days, stale, versions: [...]}`, ordered by runs, descending. Each entry is:
  - `tehasVersion` and `rukasVersion`, where the latter falls back to `rukasCommit`
  - `runs`, and `outcomes`, a count per outcome
  - `failureKinds`, a count per kind
  - `causes`: the top five as `{cause, kind, n, issues: [issueUrl…]}`, at most ten links each, unique, newest first

  Work sessions only. Full `outputSchema`.

- [ ] **Step 1: Write the failing tests.**
  - `failures_per_version_pair`: two pairs. The counts are right, and plan sessions are left out.
  - `top_causes_bounded`: seven causes give five entries. A cause that failed twelve issues gives ten links.
  - `failures_without_repo`: `causes[].issues` is empty and the counts stay.
  - Extend `schema_matches_output` to this tool.
- [ ] **Step 2: Run them and see them fail.**
- [ ] **Step 3: Implement it** in one pass over `Snapshot::sessions()`, with a `BTreeMap` per pair.
- [ ] **Step 4: Run them and see them pass,** then the fast gate.
- [ ] **Step 5: Manual.** Add the tool's row in `host/docs/mcp-server.md`.
- [ ] **Step 6: Commit.** `MCP counts failures per version pair over 30 days`

### Task 7: Agents can find it: `llms.txt`, `agents.md`, features

**Files:**
- Modify: `host/docs/llms.txt`, `host/docs/agents.md`, `host/docs/features.md` (the MCP row), `host/docs/mcp-server.md` (an MCP client config example)
- Test: whatever test already renders `llms.txt` or checks the docs' links (find it with `grep -rl llms test/`). Add a case that the rendered `llms.txt` has the MCP section and names no host but through `{{ $base }}`.

**Interfaces:**
- Consumes: the tool names and arguments from Tasks 3 to 6.

- [ ] **Step 1: Write the failing test case** above.
- [ ] **Step 2: Run it and see it fail.**
- [ ] **Step 3: Write the section.** In `llms.txt`, add a fourth "if you are here to do something" path, "Ask this factory over MCP". It gives:
  - the URL `{{ $base }}/mcp` and the header `Authorization: Bearer <key>`
  - that a key comes from the factory's owner, and the agent should ask its person and never guess
  - the nine tools in one line each, and the order to call them for a failure

  `agents.md` gets the same section and a client config block to paste. Check whether MCP 2026-07-28 defines a server card at `/.well-known/`. If it does, note in the PR that a card could follow. Do not add one in this work.
- [ ] **Step 4: Features row.** List the four new tools, "Covered by the Rust and shell tests", and "Not proven" until the proof below.
- [ ] **Step 5: Run the test and see it pass,** then `scripts/verify.sh --full`. Quote its last lines in the PR.
- [ ] **Step 6: Commit.** `llms.txt tells agents how to ask a factory over MCP`

### After merge: proof on tehas-tehas (a person deploys)

- [ ] `~/tehas/mcp/sessions.jsonl` exists, mode 600, and is refreshed each poll. `ls ~/tehas/live` shows no session file.
- [ ] Through `https://tehas.app/mcp`:
  - without a key, every method gets 401
  - with a key, `factory_failures` answers, then `factory_sessions {"outcome":"failed"}`
  - `factory_session` on one id, with its `issueUrl` opening the issue
  - `factory_log` `transcript` with `gzip: true` on the largest transcript in the window: it decodes and the gzip blob is complete
  - `factory_log` `err` plain
- [ ] If the large gzip answer is cut through Cloudflare or Caddy, or by `IO_TIMEOUT` (10 s per write in `http.rs`), raise only the write timeout, in a follow-up PR. Leave the request deadline as it is.
- [ ] Mark the features row proven only when the pi-rukas developer has done the above from their own client.
