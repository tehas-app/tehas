-- Added to store-v1.4.0.sql for the step to 27 (#2782): a cycle, its
-- approval and its history started by a retry ticked before 2.0, an open park
-- question on #21 and an open planning question on #22.
INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (21, 'issue', 1790000000, 1790000000);
INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (22, 'issue', 1790000000, 1790000000);
INSERT INTO cycles (issue, attempt, started_at, started_by, how, end_reason, ended_at)
  VALUES (21, 2, 1790000400, 'alice', 'retry-tick', 'parked', 1790000600);
INSERT INTO approvals (issue, at, by_account, how, label, body_sha256, comment_id, used_at, used_by)
  VALUES (21, 1790000390, 'alice', 'retry-tick', 'retry-tick',
          'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 2100, 1790000400, 2);
INSERT INTO cycle_history (issue, attempt, at, status, by_account, how, started_at)
  VALUES (21, 1, 1790000300, 'parked', 'alice', 'retry-tick', 1790000200);
INSERT INTO parks (issue, attempt, at, reason, step) VALUES (21, 2, 1790000600, 'failed', 'develop');
INSERT INTO questions (id, issue, attempt, kind, posted_at) VALUES (2101, 21, 2, 'park', 1790000610);
INSERT INTO planning (issue, session, turns, options_json, comment_id, by_account, label, opened_at)
  VALUES (22, 's22', 1, '["Adopt all","Change some"]', 2201, 'alice', 'tehas:plan', 1790000500);
INSERT INTO questions (id, issue, attempt, kind, posted_at) VALUES (2201, 22, 1, 'plan', 1790000510);
