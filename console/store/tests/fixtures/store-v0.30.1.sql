-- A store of tehas v0.30.1 at schema 13: made by that release's store init,
-- with rows of every kind a host keeps, then dumped by sqlite3 .dump.
-- The tests of console/store migrate it (schema_tests.rs).
PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
INSERT INTO meta VALUES('schema','13');
INSERT INTO meta VALUES('created_at','1790000000');
INSERT INTO meta VALUES('tehas_version','v0.30.1');
INSERT INTO meta VALUES('tehas_commit','abc1234');
INSERT INTO meta VALUES('instance','tehas-tehas');
INSERT INTO meta VALUES('repo','example-owner/example-product');
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
INSERT INTO issues VALUES(7,'issue',1790000000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,1790000000);
INSERT INTO issues VALUES(8,'issue',1790000000,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,1790000000);
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
INSERT INTO cycles VALUES(7,2,1790000100,'alice','label',0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,0,NULL,0,NULL,NULL,0);
INSERT INTO cycles VALUES(8,1,1790000200,'tehas-fixer','fixer',0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,0,NULL,0,NULL,NULL,0);
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
INSERT INTO heartbeats VALUES(7,1790000150,1,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
CREATE TABLE endings (
  issue       INTEGER NOT NULL,
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER NOT NULL,
  status      TEXT NOT NULL,
  step        TEXT,
  PRIMARY KEY (issue, started_at)
);
INSERT INTO endings VALUES(7,1789990000,1789990500,'failed',NULL);
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
INSERT INTO approvals VALUES(7,1790000050,'alice','label','tehas:ready','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',NULL,NULL,NULL,NULL,1790000100,2,NULL,0,0,'',0);
INSERT INTO approvals VALUES(8,1790000190,'tehas-fixer','fixer','tehas:ready','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',NULL,NULL,NULL,NULL,1790000200,1,NULL,0,0,'',0);
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
INSERT INTO planning VALUES(7,'s1',1,NULL,1,NULL,0,0,NULL,NULL,0,NULL,'tehas:plan',NULL,NULL,1789999000,NULL,0);
CREATE TABLE planning_answers (
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  turn  INTEGER NOT NULL,
  by_account TEXT NOT NULL,
  millwright   INTEGER NOT NULL DEFAULT 0,
  text       TEXT,
  PRIMARY KEY (issue, turn)
);
INSERT INTO planning_answers VALUES(7,2,'bob',0,NULL);
CREATE TABLE planning_filed (
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  url TEXT NOT NULL,
  filed_issue INTEGER NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, url)
);
INSERT INTO planning_filed VALUES(7,'https://github.com/example-owner/example-product/issues/9',9,1789999450);
CREATE TABLE planning_notes (
  issue INTEGER NOT NULL REFERENCES planning (issue) ON DELETE CASCADE,
  kind  TEXT NOT NULL CHECK (kind IN ('fixer-refused','millwright-refused','held')),
  comment_id INTEGER NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, kind, comment_id)
);
INSERT INTO planning_notes VALUES(7,'fixer-refused',55,1789999500);
INSERT INTO planning_notes VALUES(7,'held',56,1789999600);
CREATE TABLE cycle_settings (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  key   TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (issue, key)
);
INSERT INTO cycle_settings VALUES(7,'TEHAS_WORK_TIMEOUT','3600');
CREATE TABLE cycle_notes (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  kind  TEXT NOT NULL,
  said_for TEXT NOT NULL,
  at INTEGER NOT NULL,
  PRIMARY KEY (issue, kind, said_for)
);
INSERT INTO cycle_notes VALUES(7,'quiet','x',1790000160);
CREATE TABLE cycle_results (
  issue INTEGER NOT NULL REFERENCES cycles (issue) ON DELETE CASCADE,
  received_at INTEGER NOT NULL,
  schema_version INTEGER NOT NULL,
  payload TEXT NOT NULL,
  PRIMARY KEY (issue, received_at)
);
INSERT INTO cycle_results VALUES(7,1790000170,5,'{}');
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
INSERT INTO cycle_progress VALUES(7,1790000170,NULL,NULL,NULL,NULL,1790000165);
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
INSERT INTO parks VALUES(7,1,1789990600,'failed',NULL,NULL,0,NULL);
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
INSERT INTO questions VALUES(900,7,1,'park',1789990700,NULL,NULL,NULL);
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
INSERT INTO cycle_history VALUES(1,7,1,1789990500,'failed',NULL,NULL,'label',NULL,NULL,NULL,NULL,NULL,NULL,NULL,0,0,1,NULL,NULL,NULL);
INSERT INTO cycle_history VALUES(2,8,1,1789991000,'merged',NULL,NULL,'fixer',NULL,NULL,NULL,NULL,NULL,NULL,NULL,0,0,2,NULL,NULL,NULL);
CREATE TABLE plan_history (
  id INTEGER PRIMARY KEY,
  issue INTEGER NOT NULL, turn INTEGER NOT NULL, at INTEGER NOT NULL,
  outcome TEXT NOT NULL, session TEXT,
  started_at INTEGER, rc INTEGER, by_account TEXT, label TEXT, filed_json TEXT,
  auto INTEGER NOT NULL DEFAULT 0,
  seq INTEGER
);
INSERT INTO plan_history VALUES(1,7,1,1789999400,'finished',NULL,NULL,NULL,NULL,NULL,'[]',0,3);
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
INSERT INTO polls VALUES(1790000300,1790000299,NULL,NULL,0,NULL,NULL,NULL,NULL,NULL,0,0,NULL,NULL,NULL,NULL,NULL,0);
CREATE TABLE poll_cost (
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  wall_ms INTEGER NOT NULL, cpu_ms INTEGER NOT NULL, gh INTEGER NOT NULL,
  writes INTEGER, label INTEGER, comment INTEGER, edit INTEGER, other INTEGER,
  starts INTEGER
);
INSERT INTO poll_cost VALUES(1,1790000300,900,120,3,NULL,NULL,NULL,NULL,NULL,0);
CREATE TABLE sweep_log (
  issue INTEGER NOT NULL, at INTEGER NOT NULL,
  action TEXT NOT NULL CHECK (action IN ('kept','removed','would-remove')),
  what TEXT NOT NULL, bytes INTEGER, reason TEXT NOT NULL,
  PRIMARY KEY (issue, at, what)
);
INSERT INTO sweep_log VALUES(7,1790000400,'kept','workspace',NULL,'running');
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
INSERT INTO actions VALUES(1,1790000300,'label-add',7,'{"gh":["issue","edit","7"]}',1790000290,'done',1790000300,1790000300,1790000301,'ok',NULL);
INSERT INTO actions VALUES(2,1790000300,'comment-post',8,'{"gh":["issue","comment","8"]}',1790000290,'attempted',1790000302,1790000302,NULL,NULL,NULL);
CREATE TABLE journal (
  id INTEGER PRIMARY KEY,
  action INTEGER NOT NULL REFERENCES actions (id) ON DELETE CASCADE,
  at INTEGER NOT NULL,
  step TEXT NOT NULL,
  detail TEXT
);
INSERT INTO journal VALUES(1,1,1790000300,'intended',NULL);
INSERT INTO journal VALUES(2,1,1790000301,'settle','ok');
INSERT INTO journal VALUES(3,2,1790000302,'attempt',NULL);
CREATE TABLE start_order (
  id     INTEGER PRIMARY KEY CHECK (id = 1),
  issues_json TEXT NOT NULL,
  only   INTEGER NOT NULL CHECK (only IN (0, 1)),
  until_at INTEGER,
  reason TEXT,
  set_at INTEGER NOT NULL
);
INSERT INTO start_order VALUES(1,'[8,7]',0,NULL,NULL,1790000500);
CREATE TRIGGER cycle_history_order AFTER INSERT ON cycle_history WHEN NEW.seq IS NULL BEGIN
  UPDATE cycle_history SET seq = 1 + max(coalesce((SELECT max(seq) FROM cycle_history), 0),
    coalesce((SELECT max(seq) FROM plan_history), 0)) WHERE id = NEW.id;
END;
CREATE TRIGGER plan_history_order AFTER INSERT ON plan_history WHEN NEW.seq IS NULL BEGIN
  UPDATE plan_history SET seq = 1 + max(coalesce((SELECT max(seq) FROM cycle_history), 0),
    coalesce((SELECT max(seq) FROM plan_history), 0)) WHERE id = NEW.id;
END;
CREATE INDEX issues_checked ON issues (checked_at);
CREATE INDEX cycles_running ON cycles (ended_at, started_at);
CREATE INDEX heartbeats_at ON heartbeats (at);
CREATE INDEX endings_recent ON endings (ended_at DESC);
CREATE INDEX approvals_open ON approvals (issue, used_at);
CREATE INDEX questions_issue ON questions (issue, posted_at);
CREATE INDEX cycle_history_issue ON cycle_history (issue, at);
CREATE INDEX plan_history_issue ON plan_history (issue, at);
CREATE UNIQUE INDEX cycle_history_seq ON cycle_history (seq);
CREATE UNIQUE INDEX plan_history_seq ON plan_history (seq);
CREATE INDEX polls_recent ON polls (at DESC);
CREATE INDEX poll_cost_at ON poll_cost (at);
CREATE INDEX actions_open ON actions (state, intended_at);
CREATE INDEX journal_action ON journal (action, at);
COMMIT;
PRAGMA user_version = 13;
