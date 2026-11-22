### Changes

- `cargo xtask config-check --settings <instance dir>`, run in a checkout of 2.0, checks an instance's settings against 2.0's load before a host is updated. It prints `dropped <NAME>: <reason>` for each setting 2.0 drops with a warning, `fail <NAME>: <why>` for each it refuses, then `pass` (exit 0) or `fail` (exit 78). 2.0 rewrites no setting.

### Adopter action

Before you update a host to 2.0, run `cargo xtask config-check --settings <instance dir>` in a checkout of the 2.0 release, and fix every `fail` line.
