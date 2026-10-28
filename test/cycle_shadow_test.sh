#!/usr/bin/env bash
# Tests for the report's shadow in libexec/cycle: tehas-cycle decides beside
# the bash's report while state/cycle-shadow.on is there, and logs where the
# two differ in logs/cycle-shadow.log.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

home=$T/home/tehas
shadow_log=$home/logs/cycle-shadow.log
# shadow_on: tehas-cycle installed, and the shadow asked for.
shadow_on() {
  t_cycle || { t_bad "no tehas-cycle: $t_cycle_why"; return 1; }
  cp "$T_CYCLE" "$home/libexec/tehas-cycle"
  mkdir -p "$home/state"
  : >"$home/state/cycle-shadow.on"
}
# The shadow's files of #7, which go when it has compared.
leftovers() { find "$state" -name 'shadow-7.*' -o -name '7.shadow*' 2>/dev/null | wc -l; }

t_case "with the shadow off, nothing of it runs or is written"
reset
rm -f "$home/state/cycle-shadow.on"
t_cycle && cp "$T_CYCLE" "$home/libexec/tehas-cycle"
ended aborted develop
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$([ -e "$shadow_log" ] && echo logged || echo none)" none
t_eq "$(leftovers)" 0
# The bash asked once for the pull requests of no branch: only the bash's
# calls are there.
t_eq "$(t_calls gh | grep -c '^pr list')" 0

t_case "a park the shadow decides alike is logged as the same, and its files go"
reset
shadow_on
ended aborted develop feature/issue-7-x
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(wc -l <"$shadow_log")" 1
t_eq "$(jq -c '[.issue, .attempt, .same, .differs]' "$shadow_log")" '[7,1,true,[]]'
t_contains "$(jq -r .done "$shadow_log")" "done #7 rc=20 status=aborted step=develop"
t_eq "$(leftovers)" 0
t_eq "$([ -d "$home/state/cycle-shadow" ] && echo kept || echo none)" none
# The shadow asks GitHub on its own: the bash's three tries are not its.
t_eq "$(t_calls gh | grep -c '^pr list')" 2

t_case "a merged cycle the shadow decides alike is the same"
reset
shadow_on
ended merged "done" feature/x
merged_pr
t_run "$cycle 7"
t_eq "$(jq -c '[.same, .differs]' "$shadow_log")" '[true,[]]'
t_contains "$(t_calls gh)" "The change was merged: https://github.com/acme/widget/pull/9"

t_case "a report that differs is logged with both sides and kept for a person"
reset
shadow_on
ended merged "done" feature/x
merged_pr
# The shadow's one ask fails; the bash's first works. The shadow parks a
# cycle the bash reports as merged.
echo 1 >"$fx/pr-fails-times"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "The change was merged"
t_eq "$(jq -c '[.same, (.differs | index("writes") != null), (.differs | index("end") != null)]' "$shadow_log")" '[false,true,true]'
t_contains "$(jq -c .end "$shadow_log")" '"bash":["7","--attempt","1","--status","merged"'
t_eq "$(find "$home/state/cycle-shadow" -name '*-7-1.json' | wc -l)" 1
# The shadow, which could not read the pull requests, parked it.
t_eq "$(jq -c '[(.bash.end | index("--parked")), (.report.end | index("--parked") != null)]' "$home"/state/cycle-shadow/*-7-1.json)" '[null,true]'
t_eq "$(leftovers)" 0

t_case "the shadow's files are named so a poll never takes them for an older tehas's records"
reset
shadow_on
# A compare that never comes keeps the files a poll could see mid-cycle.
mv "$home/libexec/tehas-cycle" "$home/libexec/tehas-cycle.real"
printf '%s\n' '#!/usr/bin/env bash' '[ "$1" = compare ] || exec "$0.real" "$@"' >"$home/libexec/tehas-cycle"
chmod +x "$home/libexec/tehas-cycle"
ended aborted develop
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(find "$state" -name 'shadow-7.*' -printf '%f\n' | sort | tr '\n' ' ')" "shadow-7.end shadow-7.gh shadow-7.json shadow-7.park "
t_eq "$(find "$state" -name '[1-9]*shadow*' | wc -l)" 0
rm -f "$state"/shadow-7.*

t_case "a shadow that cannot run leaves the report as it was"
reset
mkdir -p "$home/state"
: >"$home/state/cycle-shadow.on"
ended aborted develop
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_lacks "$t_out" "shadow"
t_eq "$([ -e "$shadow_log" ] && echo logged || echo none)" none
t_eq "$(leftovers)" 0

t_done
