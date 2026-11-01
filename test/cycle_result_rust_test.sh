#!/usr/bin/env bash
# The cases of test/cycle_result_test.sh with the report in Rust
# (TEHAS_CYCLE_REPORT=rust): what they assert does not change.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# The file below loads the library again, with a scratch directory of its own.
rm -rf "$T"
# shellcheck disable=SC2034  # test/cycle_lib.sh reads it
T_CYCLE_REPORT=rust
# shellcheck source=test/cycle_result_test.sh
. "$(dirname "$0")/cycle_result_test.sh"
