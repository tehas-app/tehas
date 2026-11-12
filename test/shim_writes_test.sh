#!/usr/bin/env bash
# Tests for the gh shim's write log (host/shim/gh and `console gh` behind it): the
# line a write appends and the six fields of it, and that a write keeps gh's
# own stdout, stderr and exit status whatever the log does. The kinds of write
# are in test/shim_writes_kinds_test.sh and test/shim_writes_reads_test.sh,
# the log itself in test/shim_writes_log_test.sh, the rate-limit back-off in
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
t_case "the write of an issue comment is one line with the right kind, repository and number"
reset
shim issue comment 12 -R o/r --body secret
t_eq "$t_rc" 0
t_eq "$(lines | wc -l)" 1
t_eq "$(field 2)" "issue comment"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "12"
t_eq "$(field 5)" "0"
t_eq "$(field 6)" "-"
t_eq "$(field 1 | grep -cE '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$')" 1
t_eq "$(awk -F'\t' '{ print NF }' <<<"$(lines)")" 6
# Neither what was written nor how the call was paid for reaches the log.
t_lacks "$(lines)" secret
t_lacks "$(lines)" ghs_

t_done
