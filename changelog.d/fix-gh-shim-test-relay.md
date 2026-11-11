### Changes

- `test/gh_shim_test.sh` no longer fails inside a cycle's container, where its issue writes went through the relay to GitHub. Every cycle on v0.27.0 stopped on `verify-baseline-red` because of it.
