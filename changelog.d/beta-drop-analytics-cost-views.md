### Changes

- The analytics report leaves core (#PRNUM). `tehas analytics report`, the file `live/analytics.json` that `tehasbot status` wrote at each poll, the dashboard's "Last 10 days" section and the MCP tool `factory_stats` are gone. A dashboard key of the MCP server is served three tools. `tehas analytics copy` and the tables stay.
- `state/sizes.jsonl`, which only the report read, is no longer written. The dispatcher's hold on a wide issue measures as before, under the same `TEHAS_SIZE_MAX_*` settings.
- `state/analytics.db` is at schema 8: `runs.cost`, `run_roles.cost` and the views `v_lead`, `v_daily` and `v_versions` go. A file of schema 4 to 7 is migrated in place and keeps every row. `tehas analytics copy --json` reports schema 8.

### Adopter action

A person who read `tehas analytics report` fetches the copy with `tehas analytics copy` and reads it elsewhere. An MCP client stops calling `factory_stats`. A program that reads the analytics copies pins schema 8, reads no `cost` column and works out what the views gave from the tables. `~/tehas/live/analytics.json` and `~/tehas/state/sizes.jsonl` are read by nothing and may be deleted.
