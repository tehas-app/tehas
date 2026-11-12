#!/usr/bin/env bash
# Tests for the gh shim's write log: the HTTP status of a refusal, the last
# field of the line, taken from what gh said on its error stream. The status gh
# ended with is in test/shim_writes_status_test.sh, the line itself in
# test/shim_writes_line_test.sh, the kinds of write in
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

t_case "a forbidden call gives 403, and a call that says both gives 403"
reset
T_ENV=(GH_ERR="HTTP 403: Resource not accessible by integration" GH_RC=1)
shim pr merge 5 --repo o/r
t_eq "$t_rc" 1
t_eq "$(field 6)" "403"
T_ENV=(GH_ERR="HTTP 403: forbidden
HTTP 429: too many" GH_RC=1)
shim issue close 9 -R o/r
t_eq "$(field 6)" "403"

t_done
