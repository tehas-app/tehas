#!/usr/bin/env bash
# Tests for the gh shim's write log: what gh itself prints, on which stream, and
# with what status, kept by the write and put in the line beside it. The line
# itself is in test/shim_writes_line_test.sh, the kinds of write in
# test/shim_writes_kinds_test.sh and test/shim_writes_reads_test.sh, the log
# itself in test/shim_writes_log_test.sh, the rate-limit back-off in
# test/shim_writes_retry_test.sh, the graphql calls in
# test/shim_writes_graphql_test.sh and the token of the call in
# test/shim_writes_token_test.sh. The real gh is a stub (t_stub) that answers
# what GH_OUT, GH_ERR and GH_RC name: no network, and no GitHub asked of
# anything.
# shellcheck disable=SC2016,SC2154  # stub bodies are expanded when they run; log and t_err come from shim_writes_lib.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/shim_writes_lib.sh
. "$(dirname "$0")/shim_writes_lib.sh" || exit 1

t_case "stdout, stderr and the status of a write are gh's"
reset
# What gh prints reaches the caller on the stream gh chose, and the status is
# gh's own and not the shim's.
T_ENV=(GH_OUT="a word out" GH_ERR="a word on error" GH_RC=7)
shim issue edit 4 -R o/r --title T
t_eq "$t_rc" 7
t_eq "$t_out" "a word out
a word on error"
t_eq "$(lines | wc -l)" 1
t_eq "$(field 5)" "7"

t_done
