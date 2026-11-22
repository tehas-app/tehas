### Changes

- The analytics database, `state/analytics.db`, moves to schema 5 the first time it is opened after the update. The migration drops `samples.watts`, which nothing writes since energy use left core, and keeps every other column and row. A file of schema 4 that lacks `runs.contractor_id` gains it on the way. `tehas analytics copy --json` reports schema 5.

### Adopter action

A program that copies `state/analytics.db` and reads it pins the new schema: `user_version` 5, and `console/analytics/src/schema.pinned` of this release. It no longer finds `samples.watts`.
