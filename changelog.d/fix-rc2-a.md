### Changes

- A settings load refused for a setting tehas 1.0 renamed or removed, or for a line of a feature still being built, now lists every such line of `instance.env` and `~/.config/tehas/env` at once. An update from 0.30.x shows them all in one run instead of one per run.
- `TEHAS_OVERLAP_HOLD` is a row of `api/renames-1.0.tsv`. `TEHAS_OVERLAP_HOLD=0`, the old default, now loads and is dropped, as a blank value is; any other value still stops every command with 78.
- A malformed `TEHAS_ENERGY` or `TEHAS_MCP` in `instance.env` now names `instance.env` in its message, not `~/.config/tehas/env`.
- The poll reads the disk floor as the settings load does. A `TEHAS_CYCLE_MIN_FREE` such as `05g` no longer switches the floor off; a value it cannot read stops the poll with 78.
- The manual says that the ceilings of `TEHAS_WORK_TIMEOUT` (25200) and `TEHAS_PLAN_TIMEOUT` (7200) are checked by the load only, not by `console settings valid`, and no longer names a `low-gate` rule.
