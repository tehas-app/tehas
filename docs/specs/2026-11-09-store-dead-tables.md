# The state store drops the tables nobody writes (#1770)

Recorded 2026-11-09 from issue #1770 and a read of `main` at `03d5bee2`.

## What is wrong

`console/store/src/schema.sql` creates tables that no store verb writes and no
reader reads. They are carried through every schema bump (`db::STEP_N`,
`COUNTED_TABLES`, `HISTORY`, `tests/schema_v1.sql`), counted by `store check`,
and copied by `store copy`. A reader of the schema takes them for the source of
the pause, the limit, the model endpoints and the refresh requests, when those
are files and git reads.

## Findings

Eighteen tables have no writer and no reader outside the tests. The issue names
sixteen, not fifteen, and misses two.

| table | where it appears | non-test verb that writes it |
| --- | --- | --- |
| branch_checks | schema, COUNTED_TABLES, `exercise.rs:69` | none: `store test-write` is a test verb, debug-only |
| check_product | schema, COUNTED_TABLES | none |
| cycle_locks | schema, COUNTED_TABLES, deleted at `cycle.rs:538` and `import.rs:1086` | none: `lock.rs:3` says it "never writes `cycle_locks`" |
| edge_takeover | schema, COUNTED_TABLES | none |
| endpoints | schema, COUNTED_TABLES | none |
| energy_samples | schema, COUNTED_TABLES | none |
| health_samples | schema, COUNTED_TABLES | none |
| improve_cap | schema, COUNTED_TABLES | none |
| index_state | schema, COUNTED_TABLES | none |
| limits | schema, COUNTED_TABLES | none |
| pause_flag | schema, COUNTED_TABLES | none: the pause is `state/pause.json` (`host::pause`) |
| platform_state | schema, COUNTED_TABLES | none |
| poll_demand | schema, COUNTED_TABLES | none |
| queue_snapshot | schema, COUNTED_TABLES | none |
| refresh_hold | schema, COUNTED_TABLES | none |
| refresh_requests | schema, COUNTED_TABLES, `HISTORY` | none: requests are read from files (`host::requests`) |
| refined | schema, COUNTED_TABLES, `HISTORY` | none: the record is `state/refined.jsonl` |
| release_history | schema, COUNTED_TABLES | none |

`grep -rn "INTO <t>\|FROM <t>\|UPDATE <t>\|DELETE FROM <t>"` over
`console/store`, `console/analytics`, `console/mcp`, `console/dispatch`,
`libexec`, `bin`, `lib` and `test` finds no non-test hit for any of them. The
store's own counts and copies name tables through `COUNTED_TABLES` with
`format!`, so they are not a writer.

The comment above `health_samples` says prune bounds it; `pruned`
(`console/store/src/upkeep.rs:351`) deletes from `endings`, settled actions with
their journal, `heartbeats`, `sweep_log`, `poll_cost`, `polls`, `cycle_history`
and `plan_history`, and touches neither sample table.

## Decision

Drop all eighteen in schema 11, in one step, and take them out of
`COUNTED_TABLES` and `HISTORY`. A table with no writer fails the build from now
on, which is only enforceable if nothing counted is left without a writer.

## Done when

- A store test that, for each table in `COUNTED_TABLES`, names a non-test verb
  that writes it, so a table nobody writes fails the build.
- Migrating a database of schema 10 drops the eighteen and keeps every other row.
- A changelog fragment under `### Changes`, and `### Adopter action` for what an
  adopter does about the bump.

## Out of scope

- Wiring a store writer for the pause, the limit, the model endpoints or the
  refresh requests. The files stay the source; this change only stops the schema
  claiming otherwise.
- The analytics program's own schema (`console/analytics/src/schema.sql`, its own
  `user_version`).
- `tehas:blocked` on #1770, which names no issue it waits for.
