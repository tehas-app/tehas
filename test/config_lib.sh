# shellcheck shell=bash
# What the tests of lib/config.sh share: the load each case runs, and the
# bounds a load defaults to. Source this from a config_*_test.sh file after
# test/lib.sh.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck disable=SC2034  # the variables here are read by the test files
# shellcheck disable=SC2154  # t_out and t_console_why are set by test/lib.sh
T_APP_WORLD=0
load='. "$T_ROOT/lib/config.sh"; tehas_load'
t_console || t_bad "no console: $t_console_why"
# The ceiling of a turn and the bound of one child are the defaults a load gives,
# so a test never hand-copies them; the cases below refuse one more.
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_PLAN_TIMEOUT \$TEHAS_CHILD_TIMEOUT\""
read -r plan_max child_max <<<"$t_out"
