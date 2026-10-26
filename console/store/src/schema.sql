-- The schema of state/tehas.db, schema v5: every table, column, key and
-- index of the state store spec, section 3.1 to 3.13. Its version is
-- SCHEMA_VERSION in db.rs, kept in PRAGMA user_version. A column added in a
-- later version comes last in its table, where ALTER TABLE puts it, so a store
-- migrated from an older version (db::migrate) and one init made hold the
-- same columns. SQLite cannot change a CHECK in place, so cycles and
-- planning_notes are written as db::STEP_4 rebuilds them.
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
-- the aliases in console/store and the two lines of lib/store.sh.
CREATE TABLE cycles (
  issue          INTEGER PRIMARY KEY REFERENCES issues (issue) ON DELETE CASCADE,
  attempt        INTEGER NOT NULL DEFAULT 1,
  started_at     INTEGER NOT NULL,
  started_by     TEXT,
  how            TEXT NOT NULL CHECK (how IN ('label','refresh','retry-tick','outage-retry','fixer','millwright','hand')),
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

-- The cache of what the flocks say, never the truth: a cycle is running while
-- its lock is held, and this table only records that a lock was found held the
-- last time something looked. probe_rc: 0 free, 1 held, 2 could not tell.
CREATE TABLE cycle_locks (
  issue      INTEGER PRIMARY KEY REFERENCES cycles (issue) ON DELETE CASCADE,
  held       INTEGER NOT NULL,
  seen_at    INTEGER NOT NULL,
  probe_rc   INTEGER NOT NULL
);

-- One row per cycle per tick of the supervisor's 10-second write, the newest
-- 100 of an issue. Was live/cycles/<n>.json, whose mtime was the heartbeat.
-- text (v3) is the driver's last notice after the console's mask, which the
-- published view shows.
CREATE TABLE heartbeats (
  issue      INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  at         INTEGER NOT NULL,
  running    INTEGER NOT NULL,
  step       TEXT,
  status     TEXT,
  text       TEXT,
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
-- cycle is given with it: links, comment_links, retry_context, refresh_of.
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
-- three JSONL ledgers the shell commands parsed. The runner's words (cap,
-- park_reason, failure_kind, failure_cause) are claims, stored only cleaned to
-- a fixed set and bound.
CREATE TABLE cycle_history (
  id INTEGER PRIMARY KEY,
  issue INTEGER NOT NULL, attempt INTEGER NOT NULL, at INTEGER NOT NULL,
  status TEXT NOT NULL, step TEXT, by_account TEXT, how TEXT, pr INTEGER,
  started_at INTEGER, rc INTEGER, cap TEXT, park_reason TEXT,
  failure_kind TEXT, failure_cause TEXT,
  provider INTEGER NOT NULL DEFAULT 0, parked INTEGER NOT NULL DEFAULT 0,
  seq INTEGER
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
CREATE TABLE refined (
  id INTEGER PRIMARY KEY,
  issue INTEGER NOT NULL, at INTEGER NOT NULL, kept INTEGER, outcome TEXT
);

-- 3.9 the poll's own records and the samples of the published views. The live
-- files stay as views; the rows are the source.
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
CREATE TABLE poll_demand (
  at INTEGER PRIMARY KEY REFERENCES polls (at) ON DELETE CASCADE,
  waiting INTEGER NOT NULL, oldest_at INTEGER, refresh_hold TEXT
);
CREATE TABLE queue_snapshot (
  at INTEGER PRIMARY KEY,
  in_line INTEGER, running INTEGER, awaiting INTEGER, stopped INTEGER,
  completed INTEGER, planning INTEGER, all_hands INTEGER, all_hands_ignored_at INTEGER,
  paused INTEGER, pause_reason TEXT, snapshot_age_seconds INTEGER
);
-- Append-only and unbounded unless prune bounds them; prune keeps the same
-- 365 days the analytics program keeps.
CREATE TABLE health_samples (
  at INTEGER PRIMARY KEY, ok INTEGER NOT NULL, why TEXT, down INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE energy_samples (
  at INTEGER PRIMARY KEY, endpoint TEXT, watts REAL, source TEXT, ok INTEGER NOT NULL
);
CREATE TABLE platform_state (
  key TEXT PRIMARY KEY, value TEXT NOT NULL
);

-- 3.10 the pause, the limit, the branch checks and the product check. A
-- pause_flag that cannot be read is paused: unreadable says so. The four
-- singletons hold at most one row each, which the key of 1 enforces.
CREATE TABLE pause_flag (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  at INTEGER NOT NULL, reason TEXT NOT NULL, by_account TEXT NOT NULL,
  unreadable INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE limits (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  cycles INTEGER, plan INTEGER, until_at INTEGER, reason TEXT, set_at INTEGER NOT NULL
);
CREATE TABLE branch_checks (
  name TEXT NOT NULL, head_sha TEXT NOT NULL, at INTEGER NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('red','done','pending','none','unreadable')),
  note TEXT,
  PRIMARY KEY (name, head_sha)
);
CREATE TABLE check_product (
  at INTEGER PRIMARY KEY, ok INTEGER NOT NULL, failures_json TEXT
);

-- 3.11 refresh requests. at is a Unix second here and an ISO-8601 string in
-- the file the verb replaces; the import converts it rather than letting
-- SQLite coerce text into an INTEGER column.
CREATE TABLE refresh_requests (
  issue INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  at INTEGER NOT NULL,
  head_sha TEXT NOT NULL, pr INTEGER NOT NULL,
  by_account TEXT NOT NULL,
  settled_at INTEGER, outcome TEXT,
  PRIMARY KEY (issue, at)
);
CREATE TABLE refresh_hold (
  id INTEGER PRIMARY KEY CHECK (id = 1), reason TEXT NOT NULL, since_at INTEGER NOT NULL
);

-- 3.12 the endpoint, the index, the releases, the edge takeover and the
-- improvement cap. The files beside them stay files where install.sh reads
-- them before the command exists on a fresh host.
CREATE TABLE endpoints (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  main TEXT NOT NULL, backup TEXT, settled_at INTEGER NOT NULL, source TEXT
);
-- COMMIT is a keyword of SQLite, so the column of index_state that the spec
-- names `commit` is quoted here. A reader that quotes it and one that does not
-- both name the same column; only writing it in a CREATE is refused.
CREATE TABLE index_state (
  "commit" TEXT PRIMARY KEY, built_at INTEGER, failed_at INTEGER,
  state TEXT NOT NULL CHECK (state IN ('current','failed','due'))
);
CREATE TABLE release_history (
  tag TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (tag, at)
);
CREATE TABLE edge_takeover (
  id INTEGER PRIMARY KEY CHECK (id = 1), made_at INTEGER NOT NULL, note TEXT
);
CREATE TABLE improve_cap (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  cap INTEGER NOT NULL CHECK (cap BETWEEN 1 AND 5),
  set_at INTEGER NOT NULL
);

-- 3.13 the sweep's decisions, so the rules that keep or remove a workspace
-- are auditable rather than inferred from which directories are left.
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
