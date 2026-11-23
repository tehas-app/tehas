### Changes

- `cargo xtask config-check --settings <instance dir>`, run in a checkout of 2.0, runs on an instance's settings the checks `tehas update` runs before it installs, so a host can be checked before it is updated. It prints `dropped <NAME>: <reason>` for each setting 2.0 drops with a warning, `fail <NAME>: <why>` for each it refuses, then `pass` (exit 0) or `fail` (exit 78). 2.0 rewrites no setting.

### Adopter action

Before you update a host to 2.0, run `cargo xtask config-check --settings <instance dir>` in a checkout of the 2.0 release, fix every `fail` line, and run it again until it passes. It does not read `~/.config/tehas/env`: take a `GH_TOKEN` line or a renamed setting out of that file too.
