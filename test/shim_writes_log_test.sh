#!/usr/bin/env bash
# Tests for the gh shim's write log: that the log itself cannot change the call -
# a log that cannot be written leaves gh's result alone, a home with no state
# directory gets one, and a gh that cannot be run is said. The kinds of write
# are in test/shim_writes_kinds_test.sh and test/shim_writes_reads_test.sh,
# the rate-limit back-off in test/shim_writes_retry_test.sh, the graphql calls
# in test/shim_writes_graphql_test.sh and the token of the call in
# test/shim_writes_token_test.sh. The real gh is a stub (t_stub) that answers
# what GH_OUT, GH_ERR and GH_RC name: no network, and no GitHub asked of
# anything.
# shellcheck disable=SC2016,SC2154  # stub bodies are expanded when they run; log and t_err come from shim_writes_lib.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/shim_writes_lib.sh
. "$(dirname "$0")/shim_writes_lib.sh" || exit 1

t_case "a log that cannot be written changes nothing of gh's result"
reset
# A log that is a directory cannot be opened for appending. gh is called
# exactly as before and its words come back on their own streams; the one word
# about the log is on the error stream, named here as the error stream of the
# run, because t_run keeps no stream of its own.
mkdir -p "$(dirname "$log")"
mkdir "$log"
shim_split issue comment 2 -R o/r
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue comment 2 -R o/r"
t_eq "$t_err" "gh: the write log $log could not be written"
t_eq "$(t_calls gh)" "issue comment 2 -R o/r"
# Nothing was added to a log that cannot be appended to: it is still empty,
# which is what `wc -c` of an empty directory says, and what a log of a write
# that was never recorded must say.
t_eq "$(wc -c <"$log" 2>/dev/null | tr -d '[:space:]' | head -n 1)" 0

# The same call, with the log made a plain file again, is written: the shim
# gives up on the one line and nothing else.
rm -rf "$log"
shim issue comment 1 -R o/r >/dev/null
t_eq "$(lines | wc -l)" 1

t_case "a home with no state directory gets one, and the line"
reset
rm -rf "$T/home/tehas/state"
T_ENV=(TEHAS_GH_TOKEN=ghs_poll)
shim issue comment 3 -R o/r >/dev/null
t_eq "$t_rc" 0
t_eq "$(lines | wc -l)" 1

t_case "a write whose gh cannot be run says so and logs nothing"
reset
rm -f "$T/home/tehas/bin/gh"
shim issue comment 12 -R o/r
t_eq "$t_rc" 127
t_contains "$t_out" "bin/gh"
t_eq "$(lines)" ""

t_done
