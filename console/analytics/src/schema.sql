-- The schema of state/analytics.db. Its version is SCHEMA_VERSION in db.rs,
-- kept in PRAGMA user_version. Within one version columns are only added; a
-- change of meaning raises the version. Times are UTC Unix seconds, durations
-- seconds, unless a name says ms. The test schema_is_pinned compares this
-- with schema.pinned.

-- One row per run, written when the run ends. gate_runs is how many runs of
-- the product's gate the cycle made and gate_ms their milliseconds in all, as
-- its stream said; both are NULL when it recorded none, which is no gate rather
-- than a gate of no time, and 0 is not stored. driver is the driver the
-- approval bound; NULL is classic, as on every run from before the column.
-- tool_calls and tool_errors are the tool calls the run's children started and
-- those that ended in an error, as the result's usage counts them, and
-- result_driver is the driver the result says ran it; all three are NULL where
-- the result has none, which is no count rather than none.
CREATE TABLE runs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('work', 'plan')),
  issue INTEGER NOT NULL,
  attempt INTEGER,
  how TEXT,
  started INTEGER NOT NULL,
  ended INTEGER,
  seconds INTEGER,
  queue_wait INTEGER,
  outcome TEXT CHECK (outcome IN ('offered', 'merged', 'handoff', 'parked', 'failed', 'refused', 'stopped')),
  status TEXT,
  exit_code INTEGER,
  last_step TEXT,
  failed_step TEXT,
  cap TEXT,
  park_reason TEXT,
  review_held INTEGER,
  failure_kind TEXT,
  failure_cause TEXT,
  provider_outage INTEGER,
  endpoint TEXT CHECK (endpoint IN ('main', 'backup')),
  pr INTEGER,
  tehas_version TEXT,
  tehas_commit TEXT,
  rukas_version TEXT,
  rukas_commit TEXT,
  image TEXT,
  input INTEGER,
  output INTEGER,
  cache_read INTEGER,
  cache_write INTEGER,
  turns INTEGER,
  dispatches INTEGER,
  oom_kills INTEGER,
  forks_refused INTEGER,
  concurrent INTEGER,
  runner_ms INTEGER,
  result_schema INTEGER,
  gate_runs INTEGER,
  gate_ms INTEGER,
  driver TEXT CHECK (driver IN ('classic', 'lean')),
  tool_calls INTEGER,
  tool_errors INTEGER,
  result_driver TEXT CHECK (result_driver IN ('classic', 'lean'))
);
CREATE INDEX runs_ended ON runs (ended);
CREATE INDEX runs_issue ON runs (issue, started);

-- The runs of the product's gate that the cycle made, from its stream: one
-- row each, in the order they finished. The milliseconds are the run of the
-- gate's own time, not the wait for another run of it that held the
-- container's CPU share. The gate of one cycle is a handful of rows, never a
-- stream of them, so the bound is a guard against a runner that lost its mind
-- and is not read as a figure. The report reads its figures from the two gate
-- columns of runs, not from these rows: they are the per-run detail, for a
-- reader who fetches a copy. Per-run reads walk the primary key, so there is
-- no index of their own here.
CREATE TABLE run_gate_runs (
  run_id TEXT NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  command TEXT,
  tier TEXT,
  step TEXT,
  ok INTEGER,
  ms INTEGER,
  at INTEGER,
  PRIMARY KEY (run_id, seq)
);

-- The steps of a run. round is the step's turn in the run: 1 the first
-- time it ran, 2 the second.
CREATE TABLE run_steps (
  run_id TEXT NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
  step TEXT NOT NULL,
  round INTEGER NOT NULL,
  started INTEGER,
  ended INTEGER,
  seconds INTEGER,
  PRIMARY KEY (run_id, step, round)
);

-- The usage of each role in a run, and the model it was given. tool_calls and
-- tool_errors as on runs, for the role.
CREATE TABLE run_roles (
  run_id TEXT NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  model TEXT,
  provider TEXT,
  dispatches INTEGER,
  turns INTEGER,
  input INTEGER,
  output INTEGER,
  cache_read INTEGER,
  cache_write INTEGER,
  ms INTEGER,
  tool_calls INTEGER,
  tool_errors INTEGER,
  PRIMARY KEY (run_id, role)
);

-- The first time an issue reached each event.
CREATE TABLE issue_events (
  at INTEGER NOT NULL,
  issue INTEGER NOT NULL,
  event TEXT NOT NULL CHECK (event IN ('ready', 'started', 'ended', 'awaiting', 'merged', 'closed', 'stopped')),
  PRIMARY KEY (issue, event)
);
CREATE INDEX issue_events_at ON issue_events (at);

-- One row per poll of the dispatcher.
CREATE TABLE samples (
  at INTEGER PRIMARY KEY,
  in_line INTEGER,
  running INTEGER,
  awaiting INTEGER,
  stopped INTEGER,
  planning INTEGER,
  cycle_limit INTEGER,
  paused INTEGER,
  held TEXT
);

-- schema 2: run_gate_runs, and gate_runs and gate_ms on runs. A database of
-- schema 1 is moved aside whole under its version, as the manual says, and a
-- host that had no gate on the record before loses nothing of it: the column
-- is NULL, which is no gate, and no number the report prints.
-- schema 3: driver on runs, and v_versions leaves empty tokens out of its
-- median.
-- schema 4: tool_calls, tool_errors and result_driver on runs, tool_calls and
-- tool_errors on run_roles. v0.30.0 wrote 4 already, so a database of schema 2
-- or 3 is moved aside whole under its version too, as schema 1 is. Within 4,
-- v1.0.0 added contractor_id on runs.
-- schema 5: watts on samples goes; energy use left core in 2.0.
-- schema 6: contractor_id on runs goes; outsource was dropped in 2.0.
-- schema 7: the words of 2.0's events. runs.outcome says offered where it said
-- pr (its CHECK and every row), runs.how says approved where it said label,
-- and v_versions names the share of the runs that reached a change, offered
-- or merged, change_share (it was pr_share).
-- schema 8: cost on runs and on run_roles goes, and the views v_lead,
-- v_daily and v_versions go: the factory collects, the industry analyses.
-- db::open migrates a database of schema 4, 5, 6 or 7 in place, the only
-- versions it migrates.

-- schema, retention_days and created. Nothing about the host.
CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
