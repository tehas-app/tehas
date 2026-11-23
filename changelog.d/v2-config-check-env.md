### Changes

- `cargo xtask config-check` takes `--env <file>`, a file in the form of `~/.config/tehas/env` that stands for the host's. A line `NAME=<redacted>`, unquoted, counts as set with its value unchecked, so a model key that `models.json` names no longer fails a directory that `tehas update` would load.
