#!/usr/bin/env bash
# The product smoke of .pi/smoke-cmd: tehasbot dispatch takes an issue against
# a stub GitHub. The rest of the suite is console/dispatch/tests/dispatch_start.rs;
# this file goes once .pi/smoke-cmd names another smoke.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "the cycle is started for the issue and told the labeller in a comment"
reset
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "Tehas cycle started on"
t_contains "$(t_calls gh)" "authorized by @alice"

t_done
