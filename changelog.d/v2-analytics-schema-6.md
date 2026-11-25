### Changes

- The analytics database, `state/analytics.db`, moves to 2.0's schema, 7, the first time it is opened after the update. The migration drops `samples.watts`, which nothing writes since energy use left core, and `runs.contractor_id`, which no release wrote a value in: outsource was dropped. It keeps every other column and every row. A file of schema 4, from 0.30.x or 1.x, is migrated in place. `tehas analytics copy --json` reports schema 7.

### Adopter action

A program that copies `state/analytics.db` and reads it pins 2.0's schema: `user_version` 7, and `console/analytics/src/schema.pinned` of this release. It no longer finds `samples.watts` or `runs.contractor_id`.
