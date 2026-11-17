-- 3.15 outsource: laid in every store by init and by the step to 15, as
-- schema 15 wrote it. The columns of schema 17 are db::STEP_17's. Rows
-- are written only where the instance has TEHAS_OUTSOURCE=1, which the store
-- keeps as the meta row outsource = 1 (db::has_outsource). Times are UTC
-- Unix seconds, as everywhere here. The table of record is jobs, whose one
-- row is one cycle the factory gave to a contractor; every change of its
-- state leaves a row of job_history beside it, so a restart replays what
-- happened and a person can read a job's whole life. The factory never holds
-- a private key: a contractor gives its public key when it joins, and the
-- registered keys are the allowlist.

-- The contractors that may work for this factory. A key is what the joining
-- host holds; the list is the allowlist, so there is no allowlist setting.
-- revoked_at is `tehas outsource revoke`: a revoked contractor's check-ins and
-- results are refused and its open jobs are lost. resting_until is the rest
-- period a lost job costs it, and draining is `tehas outsource drain`: it is
-- asked to take no new job and to end the one it has within the grace period.
CREATE TABLE contractors (
  name          TEXT NOT NULL PRIMARY KEY,
  public_key    TEXT NOT NULL,
  added_at      INTEGER NOT NULL,
  added_by      TEXT,
  last_seen_at  INTEGER,
  revoked_at    INTEGER,
  draining      INTEGER NOT NULL DEFAULT 0 CHECK (draining IN (0, 1)),
  resting_until INTEGER,
  jobs_done     INTEGER NOT NULL DEFAULT 0,
  jobs_lost     INTEGER NOT NULL DEFAULT 0
);

-- The one-time invite codes. code_sha256 is the hash of the code as it was
-- printed, never the code: a reader of the store cannot mint an invitation
-- from a row. used_at marks the one time it was spent; an expired or spent
-- code is refused, and a new invite replaces the row of the same name so one
-- name never holds two live codes.
CREATE TABLE join_codes (
  name        TEXT NOT NULL PRIMARY KEY,
  code_sha256 TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  expires_at  INTEGER NOT NULL,
  used_at     INTEGER
);
CREATE INDEX join_codes_code ON join_codes (code_sha256);

-- One cycle given to a contractor. The row is the record of who works on the
-- issue now: state is one of the six words the spec's job state machine
-- draws, and every change of it writes a row of job_history with its own at,
-- who said so, and what was seen. attempt rises each time a job is given out
-- for the issue, and a result is accepted only from the issue's current
-- attempt, so a contractor that comes back late cannot overwrite the attempt
-- that replaced it. offered_at and expires_at bound an offer no contractor has
-- taken; last_check_in_at is what the timeout is counted from, and is the
-- start of the grace the factory waits for a salvage push.
CREATE TABLE jobs (
  id               INTEGER PRIMARY KEY,
  issue            INTEGER NOT NULL REFERENCES issues (issue) ON DELETE CASCADE,
  contractor       TEXT NOT NULL REFERENCES contractors (name),
  attempt          INTEGER NOT NULL,
  state            TEXT NOT NULL CHECK (state IN ('offered','running','stopping','finished','released','lost')),
  offered_at       INTEGER NOT NULL,
  expires_at       INTEGER NOT NULL,
  last_check_in_at INTEGER,
  lost_reason      TEXT CHECK (lost_reason IN ('timeout','revoked','expired')),
  result_sha256    TEXT,
  updated_at       INTEGER NOT NULL
);
CREATE INDEX jobs_issue ON jobs (issue, attempt);
CREATE INDEX jobs_open ON jobs (contractor, state);
CREATE INDEX jobs_check_in ON jobs (state, last_check_in_at);

-- Every state change of a job, one row each, in the order it was said: what a
-- restart replays and what a person reads as a job's history. from_state is
-- null for the first row of a job, and said_by is who moved it: the factory's
-- word is factory, a contractor's is its name.
CREATE TABLE job_history (
  id         INTEGER PRIMARY KEY,
  job        INTEGER NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
  at         INTEGER NOT NULL,
  from_state TEXT CHECK (from_state IN ('offered','running','stopping','finished','released','lost')),
  to_state   TEXT NOT NULL CHECK (to_state IN ('offered','running','stopping','finished','released','lost')),
  said_by    TEXT NOT NULL,
  detail     TEXT
);
CREATE INDEX job_history_job ON job_history (job, at);

-- What the outsourced attempts of an issue have cost, kept apart from the
-- cycles' own attempt number: this is what tells a later part that an issue
-- has spent its outsourced attempts and must run in-house, and it survives
-- the sweep and the prune of the jobs it counts.
CREATE TABLE issue_attempts (
  issue          INTEGER NOT NULL PRIMARY KEY REFERENCES issues (issue) ON DELETE CASCADE,
  given          INTEGER NOT NULL DEFAULT 0,
  lost           INTEGER NOT NULL DEFAULT 0,
  last_given_at  INTEGER
);
