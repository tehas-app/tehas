### Changes

- The MCP tool `factory_disk` now serves the lines of `docker system df` under `full.docker.lines`, masked, as its schema and the manual said.
- `api/disk-cycles.schema.json` now fits what `tehas disk --cycles --full --json` prints: `cut` is left out for a place measured whole, and a container with no command has no `command`, as in `api/disk.schema.json`. `api/disk-cycles.full.json` is a new sample.
- Every MCP tool that takes an `issue` takes 1 to 999999999 and says so in its input schema; a larger number is refused with one message.
- The manual says how to read on after a `factory_search` answer with `next`: `factory_log` from `next.offset`.
- The contract lists `runs.contractor_id` of analytics.db as frozen, says the keys of `rows` in `tehas state check --json` are internal, and says `testFeatures` in `tehas status --json` stays through 1.x.
