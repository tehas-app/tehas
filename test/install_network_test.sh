#!/usr/bin/env bash
# Tests for install.sh: the previews' network is left alone when the console
# that decides it does not answer. What the console decides is in
# console/ops/src/edge.rs.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
real_console

t_case "a console that does not answer leaves the network alone, and the run says so"
fresh
instance
t_env CONSOLE_BIN=
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "The network tehas-previews was not made or checked: the console did not answer."
t_eq "$(t_calls docker)" ""
t_env

t_done
