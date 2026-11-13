-- The schema of state/tehas.db, schema v14: every table, column, key and
-- index of the state store spec, sections 3.1 to 3.9, 3.13 and 5. Its version
-- is SCHEMA_VERSION in db.rs, kept in PRAGMA user_version. A column added in a
-- later version comes last in its table, where ALTER TABLE puts it, so a store
-- migrated from an older version (db::migrate) and one init made hold the
-- same columns. SQLite cannot change a CHECK in place, so cycles is
-- written as db::STEP_14 rebuilds it and planning_notes as db::STEP_4 does. Sections 3.10 to 3.12
-- are gone: schema 11 drops the eighteen tables no verb writes (#1770), which
-- is db::STEP_11. Nothing here references them, so they are dropped rather
-- than rebuilt. Schema 12 adds start_order (db::STEP_12). Schema 13 adds the
-- started_* columns of approvals (db::STEP_13). Schema 14 lets cycles.how say
-- 'host', a start `tehas start` approved (db::STEP_14).
-- Within one version columns are only added; a change of meaning raises the
-- version. Times are
-- UTC Unix seconds and durations are seconds, as integers: a column named
-- *_at is an instant and *_seconds is a duration, never a file mtime
-- (cycle_progress.written_at is the one exception).
-- Booleans are INTEGER 0 or 1; a list that is only ever read whole is JSON
-- text with a _json suffix. Nothing here holds a token, an address or a key,
-- and nothing a container wrote is kept as a fact (section 3.14).

-- 3.1 meta: the record the files never had. schema mirrors user_version for a
-- reader that cannot PRAGMA.
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- 3.2 issues: every number the factory has an opinion about. An answer that
-- could not be read is a row with kind 'unreadable' and a recent kind_at,
-- never a missing row. checked_at makes the age of the data a query.
CREATE TABLE issues (
  issue        INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL CHECK (kind IN ('issue', 'pr', 'closed', 'unreadable')),
  kind_at      INTEGER NOT NULL,
  title        TEXT,
  body_sha256  TEXT,
  edited_at    INTEGER,
  edited_by    TEXT,
  state        TEXT,
  state_at     INTEGER,
  labels_json  TEXT,
  labels_at    INTEGER,
  pr_number    INTEGER,
  pr_url       TEXT,
  pr_state     TEXT,
  pr_head_sha  TEXT,
  pr_at        INTEGER,
  checked_at   INTEGER NOT NULL
);
CREATE INDEX issues_checked ON issues (checked_at);

-- 3.3 cycles: the start and the end of one run. Was the .taken, .by,
-- .requeued, .done, .rukas, .image, .branch, .oom, .forks, .stalled, .timed,
-- .runner_exit and .status files, whose start time was an mtime.
-- end_reason is the union of what the two writers can say; cycle-end stores
-- a word outside it as 'unknown'. An ended row with end_reason NULL is a start
-- the cycle refused. A run by hand has how 'hand' and no started_by.
-- 'millwright' (v4) is the new name of 'fixer'; both are accepted until the
-- verbs write only the new one. #686: part 5 drops 'fixer' from this list, with
-- the aliases in console/store.
CREATE TABLE cycles (
  issue          INTEGER PRIMARY KEY REFERENCES issues (issue) ON DELETE CASCADE,
  attempt        INTEGER NOT NULL DEFAULT 1,
  started_at     INTEGER NOT NULL,
  started_by     TEXT,
  how            TEXT NOT NULL CHECK (how IN ('label','refresh','retry-tick','outage-retry','fixer','millwright','hand','host')),
  requeued       INTEGER NOT NULL DEFAULT 0,
  rukas_commit   TEXT,
  image          TEXT,
  branch         TEXT,
  end_reason     TEXT CHECK (end_reason IN ('closed','merged','handoff','parked','failed','stopped','timeout','killed','lost','interrupted','stalled','unknown')),
  ended_at       INTEGER,
  oom_kills      INTEGER,
  forks_refused  INTEGER,
  stalled_seconds INTEGER,
  timed_out      INTEGER NOT NULL DEFAULT 0,
  runner_exit    INTEGER,
  watched        INTEGER NOT NULL DEFAULT 0,
  host_saw       TEXT CHECK (host_saw IN ('killed')),
  stop_asked_at  INTEGER,
  stop_said_times INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX cycles_running ON cycles (ended_at, started_at);

-- One row per cycle per tick of the supervisor's 10-second write, the newest
-- 100 of an issue. Was live/cycles/<n>.json, whose mtime was the heartbeat.
-- text (v3) is the driver's last notice after the console's mask, which the
-- published view shows. step_number, step_total, round, round_limit and
-- step_since (v10) say where the step is, as the driver said it; null is not
-- known.
CREATE TABLE heartbeats (
  issue      INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  at         INTEGER NOT NULL,
  running    INTEGER NOT NULL,
  step       TEXT,
  status     TEXT,
  text       TEXT,
  step_number INTEGER,
  step_total INTEGER,
  round      INTEGER,
  round_limit INTEGER,
  step_since INTEGER,
  PRIMARY KEY (issue, at)
);
CREATE INDEX heartbeats_at ON heartbeats (at);

-- The ten last ends, oldest first. Was live/endings.json, which was read,
-- changed and written back with no lock.
CREATE TABLE endings (
  issue       INTEGER NOT NULL,
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER NOT NULL,
  status      TEXT NOT NULL,
  step        TEXT,
  PRIMARY KEY (issue, started_at)
);
CREATE INDEX endings_recent ON endings (ended_at DESC);

-- 3.4 approvals: one approval is one start. A used_at that is null is one
-- start still owed. by_account is the actor of the labelled event, and
-- body_sha256 is the text the approval binds to. input_json (v2) is what the
-- cycle is given with it: links, comment_links, retry_context, refresh_of, and
-- driver when the start is not classic.
-- easy (v9) says a refresh start ran as an easy merge (#1031): a row of the
-- same approval that says 1 has had the one easy merge that approval is owed,
-- and no second one starts on it.
CREATE TABLE approvals (
  issue        INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  at           INTEGER NOT NULL,
  by_account   TEXT NOT NULL,
  how          TEXT NOT NULL,
  label        TEXT NOT NULL,
  body_sha256  TEXT NOT NULL,
  title_sha256 TEXT,
  comment_id   INTEGER,
  refresh_pr   INTEGER,
  refresh_head TEXT,
  used_at      INTEGER,
  used_by      INTEGER,
  input_json   TEXT,
  easy         INTEGER NOT NULL DEFAULT 0 CHECK (easy IN (0, 1)),
  -- started_by, started_how and started_easy (v13) are what the start this approval
  -- ran wrote on its own row: the attempt of the start, the word it ran under, and
  -- whether it ran as an easy merge. The keeping after a start deletes the approvals
  -- of the starts the base no longer rests on, and a record left with only its live
  -- rows answered no refreshes, no easy merge, and no mark for the row of a cycle
  -- whose approval had gone: the poll then took every refresh after the first easy
  -- merge for the full one, and could drop the refresh it owed for want of a count.
  -- These are the words of a start kept where the answers can still reach them. A
  -- row that ran no start has 0, the empty word and 0, which counts it nowhere; the
  -- backfill in `db.rs` writes them for the rows of a store that predates this step.
  started_by   INTEGER NOT NULL DEFAULT 0,
  started_how  TEXT NOT NULL DEFAULT '',
  started_easy INTEGER NOT NULL DEFAULT 0 CHECK (started_easy IN (0, 1)),
  PRIMARY KEY (issue, at)
);
CREATE INDEX approvals_open ON approvals (issue, used_at);

-- 3.5 planning: the session of one issue, the 13 fields of the planning
-- record as columns, its brief and its filed markers kept across turns. auto
-- (v5), here and in plan_history, is a refine the poll started on its own for
-- a parked cycle.
CREATE TABLE planning (
  issue         INTEGER PRIMARY KEY REFERENCES issues (issue) ON DELETE CASCADE,
  session       TEXT,
  turns         INTEGER NOT NULL DEFAULT 0,
  options_json  TEXT,
  pending       INTEGER NOT NULL DEFAULT 0,
  pending_outcome TEXT,
  failed        INTEGER NOT NULL DEFAULT 0,
  running       INTEGER NOT NULL DEFAULT 0,
  started_at    INTEGER,
  comment_id    INTEGER,
  lost          INTEGER NOT NULL DEFAULT 0,
  by_account    TEXT,
  label         TEXT CHECK (label IN ('tehas:plan','tehas:refine')),
  brief         TEXT,
  turn_rc       INTEGER,
  opened_at     INTEGER NOT NULL,
  ended_at      INTEGER,
  auto          INTEGER NOT NULL DEFAULT 0
);
-- text (v2) is the reply the turn answers, as the person wrote it.
-- millwright (v4) was named fixer; RENAME COLUMN keeps its rows and place.
CREATE TABLE planning_answers (
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  turn  INTEGER NOT NULL,
  by_account TEXT NOT NULL,
  millwright   INTEGER NOT NULL DEFAULT 0,
  text       TEXT,
  PRIMARY KEY (issue, turn)
);
-- url is the filed issue's whole URL: it may be in another repository.
CREATE TABLE planning_filed (
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  url TEXT NOT NULL,
  filed_issue INTEGER NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, url)
);
-- (v2) The comments the poll spoke about once in this session: a millwright's
-- reply it refused, and a question whose tick it held. They go with the
-- session. 'millwright-refused' (v4) is the new name of 'fixer-refused'; both
-- are accepted until the verbs write only the new one. #686: part 5 drops
-- 'fixer-refused' from this list, with the aliases in console/store.
CREATE TABLE planning_notes (
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  kind  TEXT NOT NULL CHECK (kind IN ('fixer-refused','millwright-refused','held')),
  comment_id INTEGER NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, kind, comment_id)
);

-- 3.6 the settings, the notes said once, and what the runner answered. Both
-- result tables are keyed by received_at: no file on disk records an attempt.
CREATE TABLE cycle_settings (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  key   TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (issue, key)
);
CREATE TABLE cycle_notes (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  kind  TEXT NOT NULL,
  said_for TEXT NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, kind, said_for)
);
-- The runner's result, kept whole: its shape is the runner's and tehas must
-- not lose a field it does not yet parse.
CREATE TABLE cycle_results (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  received_at INTEGER NOT NULL,
  schema_version INTEGER NOT NULL,
  payload TEXT NOT NULL,
  PRIMARY KEY (issue, received_at)
);
-- written_at is the progress file's mtime, the one *_at taken from a file:
-- the file has no time inside it.
CREATE TABLE cycle_progress (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  received_at INTEGER NOT NULL,
  reason TEXT,
  review_held INTEGER,
  steps_json TEXT,
  roles_json TEXT,
  written_at INTEGER NOT NULL,
  PRIMARY KEY (issue, received_at)
);

-- 3.7 parks and questions. questions is the only place a comment id is
-- believed: a comment that is not in this table is not the factory's question.
-- A question the host withdrew is deleted. told_at (v2) is when the poll said,
-- once, that the park holds the issue.
CREATE TABLE parks (
  issue    INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  attempt  INTEGER NOT NULL,
  at       INTEGER NOT NULL,
  reason   TEXT NOT NULL,
  step     TEXT,
  pr       INTEGER,
  retries  INTEGER NOT NULL DEFAULT 0,
  told_at  INTEGER,
  PRIMARY KEY (issue, attempt)
);
CREATE TABLE questions (
  id         INTEGER PRIMARY KEY,
  issue      INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  attempt    INTEGER NOT NULL,
  kind       TEXT NOT NULL CHECK (kind IN ('park','plan')),
  posted_at  INTEGER NOT NULL,
  body_sha256 TEXT,
  answered_at INTEGER,
  answered_by TEXT
);
CREATE INDEX questions_issue ON questions (issue, posted_at);

-- 3.8 the histories, one row per end, the 50 newest per issue kept. Were the
-- two JSONL ledgers the shell commands parsed: the cycle's and the planning
-- turn's. The runner's words (cap, park_reason, failure_kind, failure_cause)
-- are claims, stored only cleaned to a fixed set and bound. step_number,
-- step_total and round (v10) are where the step the cycle ended in was, from
-- its last heartbeat of that step.
CREATE TABLE cycle_history (
  id INTEGER PRIMARY KEY,
  issue INTEGER NOT NULL, attempt INTEGER NOT NULL, at INTEGER NOT NULL,
  status TEXT NOT NULL, step TEXT, by_account TEXT, how TEXT, pr INTEGER,
  started_at INTEGER, rc INTEGER, cap TEXT, park_reason TEXT,
  failure_kind TEXT, failure_cause TEXT,
  provider INTEGER NOT NULL DEFAULT 0, parked INTEGER NOT NULL DEFAULT 0,
  seq INTEGER,
  step_number INTEGER, step_total INTEGER, round INTEGER
);
CREATE INDEX cycle_history_issue ON cycle_history (issue, at);
CREATE TABLE plan_history (
  id INTEGER PRIMARY KEY,
  issue INTEGER NOT NULL, turn INTEGER NOT NULL, at INTEGER NOT NULL,
  outcome TEXT NOT NULL, session TEXT,
  started_at INTEGER, rc INTEGER, by_account TEXT, label TEXT, filed_json TEXT,
  auto INTEGER NOT NULL DEFAULT 0,
  seq INTEGER
);
CREATE INDEX plan_history_issue ON plan_history (issue, at);
-- seq is the order the ends were written in, one sequence across both tables,
-- which is what a history cursor names. A row is given the next number as it is
-- written. The newest row of an issue is never the one the 50 are trimmed of, so
-- a number is not given twice.
CREATE UNIQUE INDEX cycle_history_seq ON cycle_history (seq);
CREATE UNIQUE INDEX plan_history_seq ON plan_history (seq);
CREATE TRIGGER cycle_history_order AFTER INSERT ON cycle_history WHEN NEW.seq IS NULL BEGIN
  UPDATE cycle_history SET seq = 1 + max(coalesce((SELECT max(seq) FROM cycle_history), 0),
    coalesce((SELECT max(seq) FROM plan_history), 0)) WHERE id = NEW.id;
END;
CREATE TRIGGER plan_history_order AFTER INSERT ON plan_history WHEN NEW.seq IS NULL BEGIN
  UPDATE plan_history SET seq = 1 + max(coalesce((SELECT max(seq) FROM cycle_history), 0),
    coalesce((SELECT max(seq) FROM plan_history), 0)) WHERE id = NEW.id;
END;

-- 3.9 the poll's own records. The live files stay as views; the rows are the
-- source.
CREATE TABLE polls (
  at            INTEGER PRIMARY KEY,
  lock_taken_at INTEGER NOT NULL,
  stuck_said_for INTEGER,
  held          TEXT,
  short_by      INTEGER NOT NULL DEFAULT 0,
  slice_room INTEGER, slice_committed INTEGER, slice_max INTEGER, slice_why TEXT,
  endpoint      TEXT CHECK (endpoint IN ('main','backup')),
  paused        INTEGER NOT NULL DEFAULT 0,
  low_gate_held  INTEGER NOT NULL DEFAULT 0,
  cycles_running INTEGER, in_line INTEGER, awaiting INTEGER, planning INTEGER,
  exit_code     INTEGER,
  settled       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX polls_recent ON polls (at DESC);
-- What each poll that held the lock cost, written as it ends (poll-record).
-- writes and its four kinds are NULL when there was no write log to count.
CREATE TABLE poll_cost (
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  wall_ms INTEGER NOT NULL, cpu_ms INTEGER NOT NULL, gh INTEGER NOT NULL,
  writes INTEGER, label INTEGER, comment INTEGER, edit INTEGER, other INTEGER,
  starts INTEGER
);
CREATE INDEX poll_cost_at ON poll_cost (at);

-- 3.13 the sweep's decisions, so the rules that keep or remove a workspace
-- are auditable rather than inferred from which directories are left. Append-
-- only unless `store prune` bounds it, as it bounds polls and poll_cost.
CREATE TABLE sweep_log (
  issue INTEGER NOT NULL, at INTEGER NOT NULL,
  action TEXT NOT NULL CHECK (action IN ('kept','removed','would-remove')),
  what TEXT NOT NULL, bytes INTEGER, reason TEXT NOT NULL,
  PRIMARY KEY (issue, at, what)
);

-- 5. the action journal: actions holds what the reconciler decided to do and
-- journal every attempt at it, so a poll that died mid-action can be read
-- afterwards. poll_at names the poll that intended the action and has no ON
-- DELETE: an unsettled action keeps its poll's row past prune.
CREATE TABLE actions (
  id          INTEGER PRIMARY KEY,
  poll_at     INTEGER NOT NULL REFERENCES polls (at),
  kind        TEXT NOT NULL,
  issue       INTEGER NOT NULL,
  args_json   TEXT NOT NULL,
  before_snapshot INTEGER NOT NULL,
  state       TEXT NOT NULL CHECK (state IN ('intended','attempted','done','failed','abandoned')),
  intended_at INTEGER NOT NULL,
  attempted_at INTEGER,
  settled_at  INTEGER,
  result      TEXT,
  evidence_json TEXT
);
CREATE INDEX actions_open ON actions (state, intended_at);
CREATE TABLE journal (
  id INTEGER PRIMARY KEY,
  action INTEGER NOT NULL REFERENCES actions (id) ON DELETE CASCADE,
  at INTEGER NOT NULL,
  step TEXT NOT NULL,
  detail TEXT
);
CREATE INDEX journal_action ON journal (action, at);
-- The start order `tehas order` sets: the issues to start first, as a JSON
-- list in order, whether every other ready issue waits, and when it expires.
CREATE TABLE start_order (
  id     INTEGER PRIMARY KEY CHECK (id = 1),
  issues_json TEXT NOT NULL,
  only   INTEGER NOT NULL CHECK (only IN (0, 1)),
  until_at INTEGER,
  reason TEXT,
  set_at INTEGER NOT NULL
);
