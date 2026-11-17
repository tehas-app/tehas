### Changes

- `api/renames-1.0.tsv` lists the names 1.0 removed that it missed: the label `cross-project`, `tehasbot improve`, the feature words `fixer-grant`, `refresh-conflicts` and `refresh-pr-sweep`, and the fields `fixer` and `refreshScan` of `tehas api --json`.
- `TEHAS_REFRESH_CONFLICTS` and `TEHAS_REFRESH_EVERY` are rows of `api/renames-1.0.tsv`, so `tehas update --check` lists a line that still sets one. A value the timed scan took still loads and is dropped; any other stops every command with 78 and a line that says the scan is removed.
- A line that gives a value to a setting of a feature still being built, a `TEHAS_OUTSOURCE_*` setting or `TEHAS_PROTOLAB_ZIP_HOSTS` in this release, stops every command with 78 and names the feature, as its switch set to `on` does. Nothing read these lines while the feature could not be turned on.
- `api/status.schema.json` says the fields of `prototypes` may change in a minor release, as `running[].progress` does, and the status samples no longer name another program in an order's reason. The manual says the feature word `clean` means the hourly cleaner runs on the host, and no longer says `TEHAS_GITHUB_APP_INSTALLATION` and `TEHAS_BOT_LOGIN` are ignored: both stop every command with 78.

### Adopter action

A program that reads `api/renames-1.0.tsv` picks up the new rows. Take `cross-project` off open issues. Take any `TEHAS_OUTSOURCE_*` or `TEHAS_PROTOLAB_ZIP_HOSTS` line out of `instance.env`.
