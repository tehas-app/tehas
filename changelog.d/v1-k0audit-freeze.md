### Changes

- `api/renames-1.0.tsv` lists the names 1.0 removed that it missed: the label `cross-project`, `tehasbot improve`, the feature words `fixer-grant`, `refresh-conflicts` and `refresh-pr-sweep`, and the fields `fixer` and `refreshScan` of `tehas api --json`.
- `TEHAS_REFRESH_CONFLICTS` and `TEHAS_REFRESH_EVERY` are rows of `api/renames-1.0.tsv`, so `tehas update --check` lists a line that still sets one. A value the timed scan took still loads and is dropped; any other stops every command with 78 and a line that says the scan is removed.

### Adopter action

A program that reads `api/renames-1.0.tsv` picks up the new rows. Take `cross-project` off open issues.
