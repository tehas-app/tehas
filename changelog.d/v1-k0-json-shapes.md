### Changes

- `tehas status --json`: where a running cycle's step is moves into one object, `running[].progress` (`step`, `stepNumber`, `stepTotal`, `round`, `roundLimit`, `stepSince`). It is driver-specific and may change in a minor release. The schema now names `readOnly`, which the object already printed, and a third sample, `api/status.readonly.json`, shows it.

### Adopter action

A program that reads `--json` objects of tehas reads the new names. Every renamed field is a row of `api/renames-1.0.tsv`.

- `tehas status --json`: read `running[].progress.step` and the other step fields under `progress`, not on `running[]` itself.
