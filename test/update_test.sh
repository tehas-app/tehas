#!/usr/bin/env bash
# Tests for libexec/update, the bash that asks the console whether it answers
# and then execs it. What the console's update does is tested in Rust, in
# console/tests/update.rs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/update_lib.sh
. "$(dirname "$0")/update_lib.sh" || exit 1

t_case "no console stops it with 78, and one that does not answer with 1 and where to look"
instance
platform 0
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T/no-console" GH_TOKEN=ghs_test)
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas update: no console at $T/no-console: run install.sh"
t_stub broken-console 'exit 2'
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T/bin/broken-console" GH_TOKEN=ghs_test)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "the console at $T/bin/broken-console does not answer (exit 2)"
t_contains "$t_out" "troubleshooting"
# The status decides, not the word: a console that says 1 and fails is not asked
# to run the update.
t_stub broken-console 'echo 1; exit 3'
rm -f "$T/calls/broken-console"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "the console at $T/bin/broken-console does not answer (exit 3)"
t_eq "$(t_calls broken-console)" "update api"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE" GH_TOKEN=ghs_test)
t_eq "$(state | cut -d' ' -f2)" "$(head_sha)"
t_eq "$(t_calls install)" ""

t_done
