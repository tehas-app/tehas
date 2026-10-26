#!/usr/bin/env bash
# Tests for `tehas-dispatch poll` (console/dispatch/src/poll.rs): one poll of
# the queue in Rust, run as libexec/dispatch runs it once its settings are
# loaded. Its frame: the dispatcher's lock, the checks before any call, what
# it publishes and the end of every poll that held the lock.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_dispatch || t_bad "no tehas-dispatch: $t_dispatch_why"

home=$T/home/tehas
# The bash poll runs health from beside it; the binary is built elsewhere, so
# it is named.
rpoll=". \"$T_ROOT/lib/config.sh\" && tehas_load && TEHAS_HEALTH_CMD=\"$T_ROOT/libexec/health\" \"$T_DISPATCH\" poll"
untimed() { sed -E 's/^[0-9T:-]+Z //' <<<"$1"; }

t_case "a poll that finds the lock held changes nothing"
reset
mkdir -p "$home/state"
exec 7>>"$home/state/dispatch.lock"
flock -n 7
t_run "$rpoll"
exec 7>&-
t_eq "$t_rc" 0
t_eq "$(t_calls gh)" ""
t_eq "$([ -e "$home/state/dispatch.taken" ] && echo taken)" ""

t_case "a lock held for longer than half an hour is said once"
reset
mkdir -p "$home/state"
echo $((EPOCHSECONDS - 4000)) >"$home/state/dispatch.taken"
exec 7>>"$home/state/dispatch.lock"
flock -n 7
t_run "$rpoll"
t_contains "$t_out" "WARN: the dispatcher's lock has been held since"
t_run "$rpoll"
exec 7>&-
t_lacks "$t_out" "WARN: the dispatcher's lock"

t_case "a store that is not installed, or records of an older tehas, end the poll with 78"
reset
rm -f "$home/libexec/store"
t_run "$rpoll"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"
reset
mkdir -p "$home/state/cycles"
: >"$home/state/cycles/7.approved.json"
t_run "$rpoll"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: records of an older tehas at $home/state/cycles/7.approved.json; run install.sh again"
rm -f "$home/state/cycles/7.approved.json"

t_case "a dry run logs what the bash poll's dry run logs"
reset
echo '[]' >"$fx/ready.json"
t_run "$dispatch --dry-run"
b=$(untimed "$t_out")
t_run "$rpoll --dry-run"
t_eq "$(untimed "$t_out")" "$b"
t_eq "$t_rc" 0

t_case "a poll that starts a cycle: journaled, its children without the poll's lock, its end recorded"
reset
t_run "$rpoll"
t_eq "$t_rc" 0
t_contains "$t_out" "start #7 (labelled by alice)"
t_eq "$([ -e "$T/calls/fake-start" ] && echo started)" started
t_eq "$([ -e "$T/calls/start.fd8" ] && echo inherited)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running"
# Every write of the start was journaled and settled: none is left open.
t_eq "$(TEHAS_HOME=$home "$home/libexec/store" journal-open | jq -c .actions)" "[]"
t_eq "$(TEHAS_HOME=$home "$home/libexec/store" poll-show | jq -r '.last != null')" true
t_eq "$([ -e "$T/calls/tidy.after-start" ] && echo tidied)" tidied
t_eq "$(jq -r '.time | type' "$home/live/dispatch.json" 2>/dev/null)" string

t_case "a poll that starts a cycle logs what the bash poll logs, line for line"
reset
t_run "$dispatch"
b=$(untimed "$t_out")
reset
t_run "$rpoll"
t_eq "$(untimed "$t_out")" "$b"

t_case "a poll takes the ask for it, and a dry run leaves it"
reset
echo '[]' >"$fx/ready.json"
mkdir -p "$home/state"
: >"$home/state/dispatch.again"
t_run "$rpoll --dry-run"
t_eq "$([ -e "$home/state/dispatch.again" ] && echo kept)" kept
t_run "$rpoll"
t_eq "$([ -e "$home/state/dispatch.again" ] && echo kept)" ""

t_case "a lock nothing used for 30 days is swept, and said as the bash says it"
reset
echo '[]' >"$fx/ready.json"
mkdir -p "$home/state/cycles"
: >"$home/state/cycles/5.lock"
touch -d '40 days ago' "$home/state/cycles/5.lock"
t_run "$rpoll --dry-run"
t_contains "$t_out" "would remove the lock of #5: unused for 30 days"
t_eq "$([ -e "$home/state/cycles/5.lock" ] && echo kept)" kept
t_run "$rpoll"
t_contains "$t_out" "removed the lock of #5: unused for 30 days"
t_eq "$([ -e "$home/state/cycles/5.lock" ] && echo kept)" ""

t_case "a real poll writes its demand with its time, and its cost counts its start"
reset
t_run "$rpoll"
t_eq "$(jq -r '.time | type' "$home/live/demand.json" 2>/dev/null)" string
t_eq "$(jq -r .waiting "$home/live/demand.json" 2>/dev/null)" 1
t_eq "$(TEHAS_HOME=$home "$home/libexec/store" poll-show | jq -r .last.starts)" 1

t_done
