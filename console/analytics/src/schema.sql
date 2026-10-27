-- The schema of state/analytics.db. Its version is SCHEMA_VERSION in db.rs,
-- kept in PRAGMA user_version. Within one version columns are only added; a
-- change of meaning raises the version. Times are UTC Unix seconds, durations
-- seconds, unless a name says ms. The test schema_is_pinned compares this
-- with schema.pinned.

-- One row per run, written when the run ends. gate_runs and gate_ms are what
-- the cycle's stream said of its runs of the product's gate: schema 2 holds
-- gate_ms alone, and the number is the count of these rows.
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
  outcome TEXT CHECK (outcome IN ('pr', 'merged', 'handoff', 'parked', 'failed', 'refused', 'stopped')),
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
  cost REAL,
  oom_kills INTEGER,
  forks_refused INTEGER,
  concurrent INTEGER,
  runner_ms INTEGER,
  result_schema INTEGER,
  gate_runs INTEGER,
  gate_ms INTEGER
);
CREATE INDEX runs_ended ON runs (ended);
CREATE INDEX runs_issue ON runs (issue, started);

-- The runs of the product's gate that the cycle made, from its stream: one
-- row each, in the order they finished. The milliseconds are the run of the
-- gate's own time, not the wait for another run of it that held the
-- container's CPU share. The gate of one cycle is a handful of rows, never a
-- stream of them, so the bound is a guard against a runner that lost its mind
-- and is not read as a figure.
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
CREATE INDEX run_gate_runs_run ON run_gate_runs (run_id, at);

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

-- The usage of each role in a run, and the model it was given.
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
  cost REAL,
  ms INTEGER,
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
  held TEXT,
  watts REAL
);

-- schema 2: run_gate_runs, and gate_runs and gate_ms on runs. A database of
-- schema 1 is moved aside whole under its version, as the manual says, and a
-- host that had no gate on the record before loses nothing of it: the column
-- is NULL, which is no gate, and no number the report prints.

-- schema, retention_days and created. Nothing about the host.
CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- Per shipped issue: when it merged, when it was first accepted as ready,
-- the lead time between them and the work runs it took. An issue counts as
-- shipped by its merged event, or by a work run whose outcome was merged.
CREATE VIEW v_lead AS
WITH shipped AS (
  SELECT issue, min(at) AS merged FROM (
    SELECT issue, at FROM issue_events WHERE event = 'merged'
    UNION ALL
    SELECT issue, ended FROM runs WHERE kind = 'work' AND outcome = 'merged' AND ended IS NOT NULL
  ) GROUP BY issue
)
SELECT
  s.issue AS issue,
  s.merged AS merged,
  e.at AS ready,
  s.merged - e.at AS lead_seconds,
  (SELECT count(*) FROM runs r WHERE r.issue = s.issue AND r.kind = 'work' AND r.started <= s.merged) AS runs
FROM shipped s
LEFT JOIN issue_events e ON e.issue = s.issue AND e.event = 'ready';

-- Per UTC day: issues shipped, runs ended, of them failed, their minutes, and
-- their tokens (input, output and cache read).
CREATE VIEW v_daily AS
WITH days AS (
  SELECT date(ended, 'unixepoch') AS day FROM runs WHERE ended IS NOT NULL
  UNION
  SELECT date(merged, 'unixepoch') FROM v_lead
)
SELECT
  d.day AS day,
  (SELECT count(*) FROM v_lead l WHERE date(l.merged, 'unixepoch') = d.day) AS shipped,
  (SELECT count(*) FROM runs r WHERE date(r.ended, 'unixepoch') = d.day) AS runs,
  (SELECT count(*) FROM runs r WHERE date(r.ended, 'unixepoch') = d.day AND r.outcome = 'failed') AS failed,
  (SELECT coalesce(sum(r.seconds), 0) / 60 FROM runs r WHERE date(r.ended, 'unixepoch') = d.day) AS minutes,
  (SELECT coalesce(sum(coalesce(r.input, 0) + coalesce(r.output, 0) + coalesce(r.cache_read, 0)), 0)
     FROM runs r WHERE date(r.ended, 'unixepoch') = d.day) AS tokens
FROM days d;

-- Per tehas and pi-rukas version pair, of the work runs that ended: how many,
-- the share of each outcome, and the median minutes and tokens. A pi-rukas
-- without a version is named by its commit.
CREATE VIEW v_versions AS
WITH r AS (
  SELECT
    coalesce(tehas_version, '') AS tv,
    coalesce(rukas_version, rukas_commit, '') AS rv,
    outcome,
    coalesce(seconds, 0) AS seconds,
    coalesce(input, 0) + coalesce(output, 0) + coalesce(cache_read, 0) AS tokens
  FROM runs WHERE kind = 'work' AND ended IS NOT NULL
),
ms AS (
  SELECT tv, rv, seconds AS v,
    row_number() OVER (PARTITION BY tv, rv ORDER BY seconds) AS rn,
    count(*) OVER (PARTITION BY tv, rv) AS n
  FROM r
),
mt AS (
  SELECT tv, rv, tokens AS v,
    row_number() OVER (PARTITION BY tv, rv ORDER BY tokens) AS rn,
    count(*) OVER (PARTITION BY tv, rv) AS n
  FROM r
),
pairs AS (
  SELECT tv, rv, count(*) AS runs,
    sum(outcome IN ('pr', 'merged')) AS pr,
    sum(outcome = 'handoff') AS handoff,
    sum(outcome = 'parked') AS parked,
    sum(outcome = 'failed') AS failed
  FROM r GROUP BY tv, rv
)
SELECT
  p.tv AS tehas_version,
  p.rv AS rukas_version,
  p.runs AS runs,
  round(1.0 * p.pr / p.runs, 3) AS pr_share,
  round(1.0 * p.handoff / p.runs, 3) AS handoff_share,
  round(1.0 * p.parked / p.runs, 3) AS parked_share,
  round(1.0 * p.failed / p.runs, 3) AS failed_share,
  (SELECT round(avg(v) / 60.0, 1) FROM ms WHERE ms.tv = p.tv AND ms.rv = p.rv
     AND ms.rn IN ((ms.n + 1) / 2, (ms.n + 2) / 2)) AS median_minutes,
  (SELECT avg(v) FROM mt WHERE mt.tv = p.tv AND mt.rv = p.rv
     AND mt.rn IN ((mt.n + 1) / 2, (mt.n + 2) / 2)) AS median_tokens
FROM pairs p;
