# Plan: retention, backups and growth of host state (#257)

Part of #244. Builds on #683. Independent of #252 (PR #702), except that both
touch `libexec/health` and the changelog.

## Constraint

The gate holds the bash of `bin/`, `lib/` and `libexec/` under a ceiling
(`scripts/bash-lines.max`), and main is close to it. New logic goes in the
store (Rust); the shell only calls it.

## Decisions

D1. **`store backup`** writes `state/backup/tehas-<YYYYMMDD>.db` with
`VACUUM INTO`, through the existing `db::backup` path (0600, never overwrites,
no half copy left). One a day: a copy of today that is already there is kept
and the verb says so. It keeps the newest 7 copies of that name and removes
older ones. Copies of other names (`tehas-<schema>-<time>.db` from `migrate`)
are left alone. It answers the newest copy and its age.

D2. **`store backup --show`** only reads: the newest daily copy and its age, or
null. `tehasbot health` calls it and says `warn` when there is no copy or the
newest is older than 2 days (`checks.backup`). A store that cannot answer is
not a warning of its own; health already stops on an unusable store.

D3. **`store prune --days 90`** removes, in one transaction:

- `endings` rows that ended more than 90 days ago
- `actions` that settled more than 90 days ago, with their `journal` rows
- `heartbeats` older than 90 days of cycles that have ended

A cycle's row (`cycles`) is never removed: the issue's record stays. It answers
the counts.

D4. **`store sweep-locks [--days 30] [--max 3] [--dry-run]`** removes cycle
lock files no one needs:

- the lock is free (`flock -n`), taken by the verb for the removal
- the issue's cycle on record ended more than 30 days ago, or there is no cycle
  on record and the lock file is older than 30 days (mtime)
- at most `--max` a call

"Closed for more than 30 days" in the issue is not something the host knows
yet: nothing writes `issues.kind` until #253's snapshot. The cycle's end is the
host's own evidence that nothing has used the issue since. **Owner sign-off.**
Each removal is a `sweep_log` row (`what = 'lock'`).

D5. **The sweep calls it.** `sweep_workspaces` in `libexec/dispatch` already
removes the console file and the `ensemble-runs` directories of an issue
unused for 7 days, under the issue's lock, three issues a poll. It gains one
call: `sweep-locks --max 3` after it, skipped on a dry run except as
`--dry-run`.

D6. **Logs.** `libexec/improve` removes cycle logs older than 7 days, but only
when the improvement loop runs. That `find` moves to `tehasbot clean`, which
runs every hour on every host. The files are the same: `work-*`,
`dispatch-work-*`, `index-*`, `improve-analyst*.log`, plus `plan-*.jsonl` and
`dispatch-plan-*.log`, which nothing removed before. Long-lived logs
(`dispatch.log`, `clean.log`, ...) are left as they are; a size bound for them
is a separate change.

D7. **`tehasbot clean` runs the store's upkeep**: `backup` (once a day in
effect, since the verb keeps one copy a day), then `prune --days 90`. A
failure is a warning in `clean.log`, like its other steps.

D8. **Restore.** `host/docs/operating.md` gets the procedure: stop the timers,
copy a backup over `state/tehas.db` (and remove `-wal` and `-shm`), run
`store check`, start the timers. A test restores a backup into a fresh home and
runs `tehas dispatch --dry-run` against it.

## Units

1. **Store**: `backup`, `backup --show`, `prune`, `sweep-locks`, with Rust
   tests.
2. **Shell**: clean (D6, D7), the sweep (D5), health (D2), the manual and the
   restore test (D8), the changelog.

## Out of scope

A size bound for long-lived logs. Removing a cycle's row. `issues.kind`.
