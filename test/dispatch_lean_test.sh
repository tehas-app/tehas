#!/usr/bin/env bash
# Tests for libexec/dispatch: tehas:lean through the real read of a poll. The
# rules are in console/dispatch/tests/fixtures/lean.json.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# Issue 7 parked with its Retry box ticked by alice. Its last start named the
# driver given, and it carries tehas:lean from alice.
lean_parked() { # <input of the last start, as JSON>
  reset
  echo '[]' >"$fx/ready.json"
  echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:lean"}]}]' >"$fx/asking.json"
  printf '%s' "$1" >"$T/lean-input.json"
  t_approve "$T/home/tehas" 7 alice label 1700000000 --input-file "$T/lean-input.json"
  t_started "$T/home/tehas" 7
  TEHAS_HOME=$T/home/tehas flock -w 5 "$T/home/tehas/state/cycles/7.lock" "$T_STORE" cycle-end 7 --attempt 1 \
    --status failed --parked >/dev/null || t_bad "the store refused the end of #7"
  t_park "$T/home/tehas" 7 failed 4001
  label_events 7 tehas:lean=alice
  comments 7 "$(park_body x ' ')" alice
}

t_case "a retry tick reads the record of an issue that is no candidate, and keeps its driver"
lean_parked '{"links":[],"driver":"lean"}'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_eq "$(approval .driver)" lean

t_case "a retry tick of a classic start stays classic through the same read"
lean_parked '{"links":[]}'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
t_eq "$(approval .driver)" null

t_done
