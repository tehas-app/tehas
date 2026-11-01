#!/usr/bin/env bash
# The cases of test/cycle_handoff_test.sh with the report in Rust
# (TEHAS_CYCLE_REPORT=rust): what they assert does not change.
# shellcheck disable=SC2034  # test/cycle_lib.sh reads it
T_CYCLE_REPORT=rust
# shellcheck source=test/cycle_handoff_test.sh
. "$(dirname "$0")/cycle_handoff_test.sh"
