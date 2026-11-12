#!/usr/bin/env bash
# Tests for tehasbot dispatch: the gate of the lowest issues, and what the
# slots and the room under the review cap reach in live/dispatch.json.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1
t_case "the slots and the room under the review cap reach live/dispatch.json"
reset
slots 3
echo '[{"number":20},{"number":21}]' >"$fx/awaiting.json"
echo '[]' >"$fx/ready.json"
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{free, awaitRoom}' "$dispatch_json")" '{"free":2,"awaitRoom":2}'
reset
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{free, awaitRoom}' "$dispatch_json")" '{"free":0,"awaitRoom":null}'

# --- The gate of the lowest issues (TEHAS_LOW_GATE) -------------------------
# The gate is an instance's own command. The stub answers <status> and writes
# one line for every run, so a case can say how often the poll asked it. The
# paths are written into the stub: the poll hands the gate nothing of its
# environment but PATH, so a stub cannot name them through a variable.
gate_answer() { # <status>: what the gate answers from now on
  t_stub gate-run "echo ran >>\"$T/calls/gate-runs\"
exit $1"
}
gate_runs() { t_calls gate-runs | wc -l; }
gate_held=$T/home/tehas/state/low-gate.held
gate_log=$T/home/tehas/logs/low-gate.log
gate_there() { [ -e "$gate_held" ] && echo there; }
# gate_path: the instance names the stub, and the case puts the stub there or
# leaves it out. It also clears the held file, which reset leaves alone, and
# sets the line to give to slots.
gate_path() {
  rm -f "$gate_held"
  gate_setting=TEHAS_LOW_GATE=$T/bin/gate-run
}
# gate <status>: the instance names a stub that answers <status>.
gate() {
  gate_answer "$1"
  gate_path
}
# gate_up <seconds up> [ignore]: a gate that stays up instead of answering.
# With ignore it takes no notice of being asked to end at the end of its seconds.
gate_up() { # <seconds to stay up> [ignore]
  local body="exec sleep $1"
  if [ "${2:-}" = ignore ]; then
    body=$(printf 'trap "" TERM\n%s' "$body")
  fi
  t_stub gate-run "$body"
  gate_path
}

t_case "with no gate on the instance, the lowest issues start as they did before it"
reset
slots 3
queue 7 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 8 "
t_eq "$(gate_runs)" 0
t_lacks "$t_out" "gate is closed"

t_case "an open gate starts the lowest issues, and answers once for three of them"
reset
gate 0
slots 3 "$gate_setting"
queue 7 8 11
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 8 11 "
t_eq "$(gate_runs)" 1
t_lacks "$t_out" "gate is closed"
t_eq "$(gate_there)" ""

t_case "a closed gate holds the lowest issues while an urgent and a tehas:next issue start"
reset
gate 1
slots 3 "$gate_setting"
queue "7 tehas:urgent" "8 tehas:next" 11 12
label_events 7 tehas:urgent=alice
label_events 8 tehas:next=alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 8 "
t_contains "$t_out" "wait #11: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "wait #12: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
t_eq "$(gate_runs)" 1
t_lacks "$(t_calls gh)" "issue edit 11 "
t_lacks "$(t_calls gh)" "issue edit 12 "

t_case "a gate the host does not have holds the lowest issues, and the poll ends 0"
reset
gate_path
rm -f "$T/bin/gate-run"
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 127)"

t_case "a gate that is not executable holds the lowest issues, and the poll ends 0"
reset
gate_path
rm -f "$T/bin/gate-run"
: >"$T/bin/gate-run"
chmod 644 "$T/bin/gate-run"
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 126)"

t_case "the gate answers once a poll, whatever the number of issues it holds"
reset
gate 1
slots 4 "$gate_setting"
queue 7 8 11 12
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_eq "$(gate_runs)" 1
t_contains "$t_out" "wait #12: the low-priority gate is closed (exit 1)"

t_case "a poll with urgent and tehas:next issues only never asks the gate"
reset
gate 1
slots 3 "$gate_setting"
queue "7 tehas:urgent" "8 tehas:next"
label_events 7 tehas:urgent=alice
label_events 8 tehas:next=alice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 8 "
t_eq "$(gate_runs)" 0
t_lacks "$t_out" "low issues"

t_case "a hold is said once when it begins and once when it ends, with its file between them"
reset
gate 1
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
t_eq "$(gate_there)" "there"
# The marker says only that low issues are held. Each poll's answer is in the
# log line, so the file carries nothing that could go stale.
t_eq "$(<"$gate_held")" ""
t_run "$dispatch"
t_lacks "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 1)"
t_eq "$(gate_there)" "there"
t_eq "$(gate_runs)" 2
gate_answer 0
t_run "$dispatch"
t_contains "$t_out" "low issues may start: TEHAS_LOW_GATE answered 0"
t_lacks "$t_out" "low issues wait"
t_eq "$(gate_there)" ""
t_eq "$(started)" "7 "

t_case "a dry run says the hold and leaves no held file behind"
reset
gate 1
slots 2 "$gate_setting"
queue 7
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
t_eq "$(gate_there)" ""
t_eq "$(t_calls fake-start)" ""

t_case "a dry run says the end of a hold a real poll left, and leaves the file"
reset
gate 0
slots 2 "$gate_setting"
queue 7
: >"$gate_held"
t_run "$dispatch --dry-run"
t_contains "$t_out" "low issues may start: TEHAS_LOW_GATE answered 0"
t_eq "$(gate_there)" "there"

t_case "a hold is said once although the gate answers differently while it lasts"
reset
gate 1
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 1"
gate_answer 127
t_run "$dispatch"
t_lacks "$t_out" "low issues wait"
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 127)"
t_eq "$(gate_there)" "there"
t_eq "$(<"$gate_held")" ""

# With the bound of a minute the same gate would have answered 0 and issue 7
# would have started, so this case is also the one that says the seconds come
# from TEHAS_LOW_GATE_WAIT.
t_case "a gate that outlives its seconds answers as a closed gate"
reset
gate_up 20
slots 2 "$gate_setting" TEHAS_LOW_GATE_WAIT=1
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 124)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 124"
t_eq "$(gate_there)" "there"

# It was asked to end at the second and killed ten seconds after that: the run
# ends by a signal, which is 128 plus 9.
t_case "a gate that ignores the end of its seconds is killed, and holds them too"
reset
gate_up 30 ignore
slots 2 "$gate_setting" TEHAS_LOW_GATE_WAIT=1
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 137)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 137"

t_case "the gate is asked with nothing of the poll's environment but PATH"
reset
t_stub gate-run "env >\"$T/calls/gate.env\"
exit 0"
gate_path
# What the host's own environment holds: the bot's token and the model
# endpoints' keys. The gate is the instance's command, not a party to the poll.
T_ENV+=(GH_TOKEN=gh-secret-token TEHAS_MODEL_KEY_TEST=key-for-the-model)
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" "7 "
gate_env=$(<"$T/calls/gate.env")
t_contains "$gate_env" "PATH="
t_lacks "$gate_env" "gh-secret-token"
t_lacks "$gate_env" "key-for-the-model"
t_lacks "$gate_env" "TEHAS_REPO"
t_lacks "$gate_env" "TEHAS_LOW_GATE_WAIT"

t_case "what the gate prints is kept where the person can read it, and read by nothing else"
reset
t_stub gate-run 'echo "the price feed would not answer"
exit 3'
gate_path
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 3)"
t_contains "$t_out" "low issues wait: TEHAS_LOW_GATE answered 3"
t_lacks "$t_out" "the price feed would not answer"
t_eq "$(<"$gate_log")" "the price feed would not answer"
# Every ask rewrites it: the file holds the last ask's words, not a day of them.
t_run "$dispatch"
t_eq "$(<"$gate_log")" "the price feed would not answer"
t_eq "$(wc -l <"$gate_log")" 1

t_case "a closed gate holds the rest of the lowest issues with no call about them"
reset
gate 1
slots 4 "$gate_setting"
queue 7 8 11 12
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_eq "$(gate_runs)" 1
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "wait #12: the low-priority gate is closed (exit 1)"
# The first issue of the kind paid the checks of the rules above and asked; the
# rest are held by that answer alone. The per-issue calls of those checks name
# the issue in the question this poll asks GitHub.
t_eq "$(t_calls gh | grep -c -e number=8 -e number=11 -e number=12)" 0

t_case "a held marker that is not a regular file is neither read nor written"
reset
gate 1
mkdir -p "$T/home/tehas/state"
echo kept >"$T/home/keep"
ln -s "$T/home/keep" "$gate_held"
slots 2 "$gate_setting"
queue 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(started)" ""
t_contains "$t_out" "wait #7: the low-priority gate is closed (exit 1)"
t_contains "$t_out" "low-gate.held is not a regular file"
t_eq "$(<"$T/home/keep")" "kept"

t_done
