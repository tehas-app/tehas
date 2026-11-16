### Changes

- The millwright's old name `fixer` is gone from what tehas answers. The state store's schema 16 writes `millwright` in every row that still said `fixer`, so `tehas history` names one word. The `plan.json` of `tehas export` loses the duplicate `answers[].fixer`; read `answers[].millwright`. The MCP session tools answer `millwright` for an old run and no longer take `fixer` as a `how` filter.

### Adopter action

A reader of `tehas export`'s `plan.json` that reads `answers[].fixer` reads `answers[].millwright`. An MCP client that filters `factory_sessions` by `how: "fixer"` filters by `millwright`. `api/renames-1.0.tsv` lists the renamed key and the `how` value.
