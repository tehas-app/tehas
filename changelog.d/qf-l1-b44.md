### Changes

- The sessions snapshot and the plain dashboard now call the pull request
  a run left by 2.0's word. A session line of `analytics sessions`, of
  `~/tehas/mcp/sessions.jsonl`, and the answer of the MCP tool
  `factory_session` carry `change`, the number of the change, where they
  carried `pr`; `prUrl` still links it. The dashboard's read-only list
  keys the pull request a start would have opened `change` too. The
  `pr` column of analytics.db keeps its name, as does the change's
  address on a history line.

### Adopter action

A program that read a session line's `pr` number (the sessions snapshot
or `factory_session`) reads `change`, the same number under its new
name. `prUrl`, the history line's `pr` address and the analytics copy's
`pr` column are unchanged.
