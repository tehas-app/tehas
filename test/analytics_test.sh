#!/usr/bin/env bash
# Tests for tehas-cycle after: how it calls the analytics program. How tehasbot
# status calls it is in the Rust tests of console/src/publish.rs.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

bin=$T/home/tehas/libexec/analytics

# A stub that keeps the facts it was given and how they were kept.
facts_stub() {
  t_stub_analytics 'while [ "$#" -gt 0 ] && [ "$1" != --facts ]; do shift; done
[ "$#" -gt 0 ] || exit 0
cp "$2" "$T/calls/facts.json"; stat -c %a "$2" >"$T/calls/facts.mode"; echo "$2" >"$T/calls/facts.path"'
}
cycles=$T/home/tehas/state/cycles
mkdir -p "$cycles"
now=$(date +%s)

home=$T/home/tehas
# With the store: a run's approval, settings and commit are on the cycle's
# record. A run's facts are tehas-cycle's: run_facts <issue> <attempt> [<forks>].
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
t_stub systemctl
scall=". \"\$T_ROOT/lib/store.sh\"
run_facts() { \"\$T/cycle-bin\" after \"\$1\" < <(printf '%s\\0' --attempt \"\$2\" --started $((now - 600)) --rc 0 --status merged --how retry-tick --pr https://github.com/o/r/pull/3 \\
  --approved \"\$(\"\$T/cycle-bin\" approved \"\$1\" \"\$2\")\" --oom 0 --forks \"\${3:-0}\" --signalled 1 --workdir /nowhere --console true --status-cmd true); }"

t_case "a run's facts are the cycle's line and attempt, and the approval, the settings and the commit on record"
rm -f "$T"/calls/*
facts_stub
T_ENV=(TEHAS_ANALYTICS=on)
t_store_home "$home"
t_approve "$home" 7 alice retry-tick "$(date -u -d 2026-10-24T10:59:00Z +%s)"
t_started "$home" 7 --rukas abcdef1
echo '{"container": {"PI_ENSEMBLE_SUBAGENT_MODEL": "m"}, "host": {"TEHAS_WORK_TIMEOUT": "60"}, "concurrentCycles": 2}' >"$T/settings.json"
TEHAS_HOME=$home flock -w 5 "$cycles/7.lock" "$T_STORE" cycle-image 7 --attempt 1 \
  --image "sha256:$(printf 'e%.0s' {1..64})" --settings "$T/settings.json" >/dev/null
t_run "$scall; run_facts 7 1 1; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_lacks "$t_out" "WARN analytics"
facts=$(cat "$T/calls/facts.path")
t_eq "$(t_calls analytics)" "run 7 --facts $facts --oom 0 --forks 1
event 7 ended"
t_eq "$(jq -c 'del(.line.at)' "$T/calls/facts.json")" "$(jq -n -c --argjson s $((now - 600)) '{line: {started: $s, rc: 0, status: "merged", provider: false, parked: false,
  pr: "https://github.com/o/r/pull/3", how: "retry-tick"},
  attempt: 1, approval: {at: "2026-10-24T10:59:00Z", how: "retry-tick", driver: null},
  settings: {container: {PI_ENSEMBLE_SUBAGENT_MODEL: "m"}, host: {TEHAS_WORK_TIMEOUT: "60"}, concurrentCycles: 2},
  rukas: "abcdef1"}')"
t_eq "$(jq -r .line.at "$T/calls/facts.json" | grep -c '^[0-9-]*T[0-9:]*Z$')" 1
t_eq "$(cat "$T/calls/facts.mode")" 600
t_eq "$([ -e "$facts" ] && echo there || echo gone)" gone
t_eq "$([[ $facts == "$T/home/tehas/tmp/analytics-facts."?????? ]] && echo named)" named

t_case "the driver of the approval reaches the facts, where the record holds it"
rm -f "$T"/calls/*
t_store_answers approval-view 0 '{"api":1,"verb":"approval-view","ok":true,"view":{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","links":[],"driver":"lean"}}'
t_run "$scall; run_facts 7 1"
t_eq "$(jq -c '.approval | [.how, .driver]' "$T/calls/facts.json")" '["label","lean"]'
cp -f --remove-destination "$T_STORE" "$home/libexec/store"

t_case "a fact the store has none of for this attempt, or cannot give, is null"
rm -f "$T"/calls/*
t_run "$scall; run_facts 7 2; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_eq "$(jq -c '[.attempt, .approval, .settings, .rukas, .line.status]' "$T/calls/facts.json")" '[2,null,null,null,"merged"]'
t_store_answers settings-view 1 'store: busy #7: the database is locked'
t_run "$scall; run_facts 7 1"
t_eq "$(jq -c '[.approval.how, .settings, .rukas]' "$T/calls/facts.json")" '["retry-tick",null,null]'
t_store_answers approval-view 78 'store: corrupt: the file is not a database'
t_run "$scall; run_facts 7 1"
t_eq "$(jq -c '[.approval, .settings.concurrentCycles]' "$T/calls/facts.json")" '[null,2]'
t_store_home "$home"
t_eq "$(find "$T/home/tehas/tmp" -type f | wc -l)" 0


t_case "facts that cannot be written are one WARN line, and the program is not called"
rm -f "$T"/calls/*
rm -rf "$T/home/tehas/tmp"
: >"$T/home/tehas/tmp"
t_run "$scall; run_facts 7 1; echo rc=\$?"
t_contains "$t_out" "WARN analytics run: no facts written"
t_contains "$t_out" "rc=0"
t_eq "$(t_calls analytics)" "event 7 ended"
rm -f "$T/home/tehas/tmp"

t_case "with TEHAS_ANALYTICS off, or no program, no facts are made and nothing is called"
rm -rf "$T"/calls/* "$T/home/tehas/tmp"
facts_stub
T_ENV=(TEHAS_ANALYTICS=off)
t_run "$scall; run_facts 7 1; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_lacks "$t_out" "WARN analytics"
rm -f "$bin"
T_ENV=(TEHAS_ANALYTICS=on)
t_run "$scall; run_facts 7 1; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_lacks "$t_out" "WARN analytics"
t_eq "$(t_calls analytics)" ""
t_eq "$([ -e "$T/home/tehas/tmp" ] && echo made || echo none)" none
T_ENV=()

t_done
