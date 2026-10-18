#!/usr/bin/env bash
# Tests for libexec/cycle: what it refuses to start, its lock and the notes it
# leaves.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/cycle_lib.sh
. "$(dirname "$0")/cycle_lib.sh" || exit 1

for bad in "" "abc" "07" "0" "7 8" "7x" "-7"; do
  t_case "'$bad' is not an issue: nothing runs and nothing is written"
  reset
  t_run "$cycle $bad"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage: tehasbot cycle <issue>"
  t_eq "$(t_calls gh)$(t_calls fake-work)" ""
  t_eq "$(find "$state" 2>/dev/null | wc -l)" 0
done

t_case "an invalid instance runs nothing, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$cycle 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls fake-work)" ""

# The factory works on open issues only. What the number is decides it, and it
# is asked before a label is changed, a comment is posted or a container starts.
t_case "a closed issue is asked for and refused: no cycle, no label, no comment"
for answer in '{"number":7,"state":"closed"}' '{"number":7,"state":"CLOSED"}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-7.json"
  t_run "$cycle 7"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #7: the issue is closed"
  t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_lacks "$(t_calls gh)" "issue comment"
  # It still said it came up. The dispatcher waits for this file to know its
  # unit ran; a cycle that refuses its number after that is not a start that
  # did not happen.
  t_eq "$([ -e "$state/7.taken" ] && echo taken)" taken
done

t_case "the number of a pull request is refused, open, merged or closed"
for answer in '{"number":9,"state":"open","pull_request":{"url":"u"}}' '{"number":9,"state":"closed","pull_request":{"url":"u"}}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-9.json"
  t_run "$cycle 9"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #9: it is a pull request, not an issue the factory works on"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_eq "$([ -e "$state/9.taken" ] && echo taken)" taken
done

t_case "an answer that cannot be read is a refusal, not a guess"
for answer in '' 'not json' 'null' '[]' '{"message":"Not Found"}'; do
  reset
  printf '%s' "$answer" >"$fx/issue-7.json"
  t_run "$cycle 7"
  t_eq "$t_rc" 65
  t_contains "$t_out" "refuse #7: GitHub's answer for it could not be read"
  t_eq "$(t_calls fake-work)" ""
  t_lacks "$(t_calls gh)" "issue edit"
done

# GitHub could not be reached at all: three tries, then a refusal that says the
# ask failed, which is not the same as an answer that is unreadable.
reset
: >"$fx/issue-fails"
t_run "$cycle 7"
t_eq "$t_rc" 65
t_contains "$t_out" "refuse #7: GitHub could not be asked what the number is"
t_lacks "$t_out" "could not be read"
t_eq "$(grep -c '^api repos/acme/widget/issues/7$' <<<"$(t_calls gh)")" 3
t_eq "$(t_calls fake-work)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$([ -e "$state/7.taken" ] && echo taken)" taken

# A call that only failed is retried, as every other call here is: the cycle
# the dispatcher already authorized still starts.
reset
echo 2 >"$fx/issue-fails-times"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(grep -c '^api repos/acme/widget/issues/7$' <<<"$(t_calls gh)")" 3
t_eq "$(t_calls fake-work)" "7"
t_lacks "$t_out" "refuse #7"

t_case "an open issue is asked for first, before its cycle starts"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | head -1)" "api repos/acme/widget/issues/7"
t_eq "$(t_calls fake-work)" "7"

# --restart is the answer to a handoff: pi-rukas's attention gate lets it
# through and takes needs-human-attention off. tehas removes no such label.
t_case "a cycle restarts only an issue that carries needs-human-attention"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
rm -f "$T"/calls/*
t_run "$cycle 7"
t_eq "$(t_calls fake-work)" "7"
rm -f "$T"/calls/*
echo '{"number":7,"state":"open","labels":[{"name":"tehas:running"},{"name":"needs-human-attention"}]}' >"$fx/issue-7.json"
t_run "$cycle 7"
t_eq "$(t_calls fake-work)" "7 --restart"
t_lacks "$(t_calls gh | grep -e '--remove-label needs-human-attention')" "needs-human-attention"

t_case "it says that it holds its lock, and lets go of it at the end"
t_eq "$(find "$state/7.taken" | wc -l)" 1
t_eq "$(flock -n "$state/7.lock" echo free)" "free"

# A run leaves its history line beside the lock files and suffixed notes.
t_case "a cycle writes its history line and the suffixed notes"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(find "$state" -maxdepth 1 ! -name cycles -printf '%P\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "7.history.jsonl 7.lock 7.park 7.progress.json 7.rukas 7.taken "

t_case "a second cycle for the same issue runs nothing, exit 3"
reset
mkdir -p "$state"
exec 9>"$state/7.lock"
flock -n 9
t_run "$cycle 7"
t_eq "$t_rc" 3
t_contains "$t_out" "a cycle for #7 is already running"
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
t_eq "$(find "$state/7.taken" 2>/dev/null | wc -l)" 0
exec 9>&-

t_case "a cycle for another issue is not held back"
reset
mkdir -p "$state"
exec 9>"$state/8.lock"
flock -n 9
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
exec 9>&-

t_done
