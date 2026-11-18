### Changes

- `api/settings-rules.json` marks each row of a feature still being built with `"building": true`, and `api/api.schema.json` no longer requires a building feature's key in `flags`. Such rows and keys are not contract and may change until the release that ships the feature. The contract page gains the proposed rules for 1.x: readers ignore fields they do not know, a 1.x release only adds fields, enum values and exit statuses, and the update to 1.0 comes from 0.30.x, 0.31.x or 0.32.x.
