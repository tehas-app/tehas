### Changes

- The MCP tool `factory_log` reads a running cycle's logs: give `issue` instead of `id`, or the running cycle's id, `work-<issue>-<startedAt>`, which names it only while it runs. Before, a cycle's logs could be read only after it ended.
- The MCP tools `factory_sessions`, `factory_session` and `factory_failures` check each answer against the schema they publish. `factory_sessions` leaves out a session whose snapshot line has a field of another type or a word outside its list, and counts it in `leftOut`; `factory_session` answers such a session with an error that names the field. Their schemas list the words of `outcome`, `kind` and `how`.
- `factory_failures` counts in `outcomes` only the seven outcome words. A run with any other word, like a run with none, counts in `outcomesUnknown`.
- `factory_park` gives `latestOutcome` when a later work run of the issue ended without a park, such as `stopped` for a run interrupted or whose issue closed under it, so the park it answers is not taken for the issue's last word.
- `factory_outcomes` marks a week entry with no readable week `unwindowed: true` when `since` or `until` is given: it is kept, as no window can bound it (#2244).
- `tehas top --json` is named as not a public object: it is the dashboard's snapshot and may change in any release (#2241).
