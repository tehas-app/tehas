### Changes

- A question still open when a host moves to 2.0 is asked again in 2.0's words, the old comment's words that asked for a tick saying to reply instead. `tehas update` marks it (store schema 27, `questions.reask`), and the first poll posts it once more as plain lines answered by a reply, records the choices it offers so `tehas question` lists them, adds one line to the old comment that links the new one, and takes the old one off the record. A stop of before 2.0 that offered no retry is not asked again, since 2.0 asks nothing of it: its old comment says it is closed (#2782).
- `retry-tick` is gone. The state store's step to schema 27 rewrites every `retry-tick` in `state/tehas.db` to `reply` (the cycles' how, the approvals' how, label and started how, and the cycle history's how), analytics rewrites `runs.how` in `state/analytics.db` each time it opens the file, and `tehas history`, analytics, the MCP server's `factory_sessions` and the approval record no longer take it (#2782).

### Adopter action

Run `tehas update`, which migrates the state store to schema 27: `state/tehas.db` holds no `retry-tick` after it. `state/analytics.db` keeps its schema; its runs say `reply` from the first analytics run after the update. A program that filtered or counted runs by `how` `retry-tick` reads `reply` instead.
