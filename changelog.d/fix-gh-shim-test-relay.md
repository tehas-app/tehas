### Changes

- `test/gh_shim_test.sh` passes inside a cycle's container. There its issue writes went to the relay, which refused them, so every cycle on v0.27.0 stopped on `verify-baseline-red`.
