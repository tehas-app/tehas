### Changes

- With outsource on, the factory stamps who built each cycle: the start comment, the report's comments and the pull request's body end with `Built by contractor: <name> (job <id>)` or `Built by: in-house`, and each commit of a contractor's pull request carries a `Built-by:` trailer, which the factory rewrites when it is missing or wrong. Outsource is still being built, so no instance writes the stamp yet (tehas#2276).
- `state/analytics.db` gains `runs.contractor_id`, the contractor that ran a cycle, inside schema 4: a file written by 0.30.x gains the column, empty, the first time it is opened, and `tehas analytics copy` carries it. It stays empty until outsource ships (tehas#2276).
