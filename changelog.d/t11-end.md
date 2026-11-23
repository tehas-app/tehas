### Changes

- One function decides how a work run ended, and four ends in the analytics change. A run `tehas stop` stopped is `stopped` and a lost run is `failed`; both were `parked`. A run that parked is never `merged`, `stopped` or offered: a parked run that claimed a merge the report did not find, and a parked run whose issue closed or that was interrupted, end as `handoff`, `failed` or `parked`. Runs recorded before the update keep their outcome.
- The outcome `pr` is `offered`: a run that left a change for review and did not park. The analytics' `runs.outcome`, the report's `runs` counts, and the MCP tools `factory_sessions`, `factory_stats`, `factory_outcomes` and `factory_park` use the one word; `pr` is no longer taken. `prShare` is `changeShare`, `v_versions.pr_share` is `change_share` and `oneAttempt.pr` is `oneAttempt.change`: each counts the runs that reached a change, offered or merged.

### Adopter action

A program that reads the analytics copies, or an MCP client, reads and filters by `offered` where it read `pr`. A program that reads the analytics copies and compares counts of `runs.outcome` across the update sees the four moves from then on: runs that counted as `parked` now count as `stopped` or `failed`, and parked runs that counted as `merged` or `stopped` now count as `handoff`, `failed` or `parked`.
