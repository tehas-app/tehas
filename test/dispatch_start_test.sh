#!/usr/bin/env bash
# Tests for libexec/dispatch: starting a cycle, the poll's own lock, issues
# marked as running with no cycle, left containers and start markers.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "the cycle is started for the issue and told the labeller in a comment"
reset
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "Tehas cycle started on"
t_contains "$(t_calls gh)" "authorized by @alice"

t_case "the poll's lock is not passed on to the cycle it starts"
t_eq "$(find "$T/calls/start.fd8" 2>/dev/null | wc -l)" 0

t_case "the status is refreshed before the poll and after the start"
# Three: the poll's own read, the one start_cycles runs when the starts are
# done, and the poll's end, which #871 leaves to the poll that changed the
# queue - and a start is one.
t_eq "$(t_calls fake-status | wc -l)" 3

# #871: the poll's end runs the status only after a poll that could change
# the queue, so a poll that writes nothing runs none of them.
t_case "a poll that starts nothing runs no status: the queue did not change"
reset
echo '[]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-status)" ""

t_case "for one issue: the running label, then the start comment, then the start"
reset
# fake-order notes what the gh calls held when the start was asked for.
t_stub fake-order 'grep -q -e "--add-label tehas:running" "$T/calls/gh" && : >"$T/calls/order.label"
grep -q "^issue comment $1 " "$T/calls/gh" && : >"$T/calls/order.comment"
flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-start "$1" >/dev/null'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-order TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index TEHAS_INDEX_START_CMD=fake-index-start "$slice_env")
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$T/calls/order.label" "$T/calls/order.comment" 2>/dev/null | wc -l)" 2
label_at=$(grep -n -e "--add-label tehas:running" "$T/calls/gh" | head -n 1 | cut -d: -f1)
comment_at=$(grep -n "^issue comment 7 " "$T/calls/gh" | head -n 1 | cut -d: -f1)
t_eq "$([ "$label_at" -lt "$comment_at" ] && echo before)" "before"

t_case "the poll does not wait for the cycle: it returns while the cycle lives, with its own lock free"
reset
# The cycle outlives the poll and holds its own lock. It must not hold the
# poll's, so the next poll can run. Its output is detached from the test's pipe.
t_stub fake-live '( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; "$T/store-bin" cycle-start "$1"; exec sleep 8 ) >/dev/null 2>&1 &
echo $! >"$T/live.pid"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-live TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index TEHAS_INDEX_START_CMD=fake-index-start "$slice_env")
t_run "$dispatch"
alive=no
kill -0 "$(<"$T/live.pid")" 2>/dev/null && alive=yes
free=$(flock -n "$T/home/tehas/state/dispatch.lock" echo free)
kill "$(<"$T/live.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_eq "$alive" yes
t_eq "$free" free

t_case "the poll's lock is not handed to the index"
reset
t_run "$dispatch"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(find "$T/calls/index.fd8" 2>/dev/null | wc -l)" 0

t_case "a real poll records when it took the lock"
t_eq "$(($(date +%s) - $(<"$T/home/tehas/state/dispatch.taken") < 60))" 1

t_case "a poll that finds the lock held for longer than a turn may take says so once, and exits 0"
reset
taken_file=$T/home/tehas/state/dispatch.taken
rm -f "$T/home/tehas/state/dispatch.stuck"
echo $(($(date +%s) - 6000)) >"$taken_file"
exec {lockfd}>>"$T/home/tehas/state/dispatch.lock"
flock -n "$lockfd"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the dispatcher's lock has been held since"
t_contains "$t_out" "more than 1800s: a poll hangs, and no poll runs until it ends"
t_eq "$(t_calls gh)" ""
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "a poll that finds the lock held for a short while says nothing"
stamp=$(($(date +%s) - 60))
echo "$stamp" >"$taken_file"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_run "$dispatch --dry-run"
t_contains "$t_out" "another poll holds the dispatcher's lock"
t_lacks "$t_out" "WARN"

t_case "a poll that finds the lock held does not change when it was taken"
t_eq "$(<"$taken_file")" "$stamp"

t_case "a poll that finds the lock held still takes the ended cycle's request, so the path unit does not start poll after poll"
again=$T/home/tehas/state/dispatch.again
: >"$again"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$again" 2>/dev/null | wc -l)" 0
exec {lockfd}>&-

t_case "a poll takes the request an ended cycle left, and a dry run leaves it"
reset
: >"$again"
t_run "$dispatch --dry-run"
t_eq "$(find "$again" 2>/dev/null | wc -l)" 1
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$again" 2>/dev/null | wc -l)" 0

t_case "while a cycle is running nothing is started"
reset
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 cycles are running (limit 1)"
release

t_case "an issue marked as running with no cycle behind it is handed to a human"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "park #9: it is marked tehas:running and no cycle holds its lock"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "issue comment 9 --repo acme/widget --body Tehas found this issue marked \`tehas:running\` with no cycle behind it"
t_contains "$(t_calls gh)" "logs/dispatch-work-9.log"
t_contains "$(t_calls gh)" "To retry, apply \`tehas:ready\`. If it is on the issue already, nothing more is needed."
t_lacks "$(t_calls gh)" "remove \`needs-human-attention\`"

t_case "and the poll goes on to the queue, and starts nothing for that issue"
t_eq "$(t_calls fake-start)" "7"

t_case "an issue whose cycle ended after the list was read is not parked"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo '{"number":9,"state":"open","labels":[{"name":"tehas:awaiting-merge"}]}' >"$fx/work-9.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #9: tehas:running is gone from it since the list was read"
t_lacks "$(t_calls gh)" "issue edit 9"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"

t_case "the issue is read again before it is parked, and a failed read ends the poll"
reset
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/work-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""
rm -f "$fx/work-fails"

t_case "a closed issue marked as running with no cycle loses the mark and is not parked"
reset
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 9 --repo acme/widget --remove-label tehas:running')" 1
t_lacks "$(t_calls gh)" "issue comment 9"
t_contains "$t_out" "clear #9: it is closed, marked tehas:running, and no cycle holds its lock"

# An epic: an issue with open sub-issues. GitHub answers the count on the issue
# the poll reads anyway, in sub_issues_summary. The stub gives the work answer
# alone, so an epic's fixture names its labels and its text too: tehas:ready as
# the queue lists it, and the body alice's label approved.
epic() { # <total> <completed>
  jq -cn --argjson t "$1" --argjson c "$2" '{number: 7, state: "open", title: "Faster search", body: "Make it fast.",
    labels: [{name: "tehas:ready"}], sub_issues_summary: {total: $t, completed: $c}}' >"$fx/work-7.json"
}

t_case "an issue with open sub-issues is not started: its tehas:ready comes off and the issue says why once"
reset
epic 13 0
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: it is an epic with 13 sub-issues open, and an epic is never started"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas took \`tehas:ready\` off this issue and started nothing. It has 13 sub-issues open"
t_contains "$(t_calls gh)" "An epic is never started. Apply \`tehas:ready\` to one of its sub-issues instead."
t_eq "$(grep -c '^issue comment 7 ' <<<"$(t_calls gh)")" 1

t_case "and the ask costs no call to GitHub beyond the issue it already read"
bash_reads && t_eq "$(grep -c -x 'api repos/acme/widget/issues/7' <<<"$(t_calls gh)")" 1

t_case "when its label cannot be changed nothing is said on it, and the poll goes on"
reset
epic 3 1
echo "issue edit 7" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: label change on #7 failed: --remove-label tehas:ready"
t_lacks "$(t_calls gh)" "issue comment 7"
t_eq "$(t_calls fake-start)" ""
rm -f "$fx/edit-fails-on"

t_case "one open sub-issue is said in the one"
reset
epic 2 1
t_run "$dispatch"
t_contains "$t_out" "skip #7: it is an epic with 1 sub-issue open, and an epic is never started"
t_contains "$(t_calls gh)" "It has 1 sub-issue open"

t_case "an issue whose sub-issues are all closed starts as any other"
reset
epic 13 13
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$(t_calls gh)" "An epic is never started"
t_lacks "$t_out" "it is an epic"

t_case "an issue with no sub-issues at all starts too"
reset
epic 0 0
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "it is an epic"

t_case "a GitHub without sub-issues: an answer with no such field starts as any other"
reset
jq -cn '{number: 7, state: "open", title: "Faster search", body: "Make it fast.",
  labels: [{name: "tehas:ready"}]}' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "it is an epic"

t_case "and an answer that is no object at all ends the poll as any unreadable one does"
reset
printf '%s' '[]' >"$fx/work-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
bash_reads && t_contains "$t_out" "ERROR: GitHub's answer for #7 could not be read"
t_eq "$(t_calls fake-start)" ""

t_case "and an answer whose counts are not numbers starts too"
for junk in '"abc"' 'null' '"13"' '[]' '{"total":3}' '{"total":null,"completed":null}'; do
  reset
  jq -cn --argjson s "$junk" '{number: 7, state: "open", title: "Faster search", body: "Make it fast.",
    labels: [{name: "tehas:ready"}], sub_issues_summary: $s}' >"$fx/work-7.json"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" "7"
  t_lacks "$t_out" "it is an epic"
done

t_case "and an answer where more are completed than there are in all starts too"
reset
epic 3 5
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "it is an epic"

t_case "a dry run says it and changes nothing"
reset
epic 13 0
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: it is an epic with 13 sub-issues open, and an epic is never started"
t_contains "$t_out" "would take tehas:ready off #7: it is an epic"
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "issue comment 7"

t_case "a planning session on an epic is allowed"
reset
jq -cn '{number: 7, state: "open", title: "Faster search", body: "Make it fast.",
  labels: [{name: "tehas:plan"}], sub_issues_summary: {total: 4, completed: 0}}' >"$fx/work-7.json"
echo '[]' >"$fx/ready.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
echo '{"title":"Faster search","body":"Make it fast."}' >"$fx/issue-7.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--add-label tehas:planning"
t_lacks "$t_out" "it is an epic"

# The ready list of a finding the improvement loop filed: ready by alice, with
# the loop's own label and cross-project beside it. The list is the only place
# the hold reads from, so the issue is never asked about, which the case below
# checks. The JSON is one line: this file is written by printf, and a line that
# continues another is run as a command of its own.
found_cross()
{
  printf '%s\n' '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:improvement"},{"name":"cross-project"}]}]' >"$fx/ready.json"
  printf '%s\n' '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-10-01T00:00:00Z"}]' >"$fx/events-7.json"
}

t_case "an issue the loop labelled cross-project starts no cycle, and its labels stay"
reset
found_cross
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: it carries cross-project and tehas:improvement, so its fix needs a change in another repository too, and one cycle here cannot make it"
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "issue comment 7"

t_case "and the hold sits before every ask that a start would have made"
# The labels come off the queue the poll already read, so what the poll asks of
# this issue are the reads it makes of any ready issue: the events of the label
# that approved it, and the one ask work_ok makes. None of the asks that follow
# it for an issue this poll takes - a pull request that closes it, an attachment
# its text links - is made for one it holds, and no cycle is started for it.
bash_reads && t_eq "$(grep "^api repos/acme/widget/issues/7" <<<"$(t_calls gh)" | sort -u | tr '\n' ' ')" "api repos/acme/widget/issues/7 api repos/acme/widget/issues/7/events?per_page=100 --paginate "
t_eq "$(t_calls fake-start)" ""

t_case "cross-project alone is no hold: the issue of a person starts as any other"
reset
printf '%s\n' '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"cross-project"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "another repository"

t_case "tehas:improvement alone is no hold either: that is a proposal a person readied"
reset
printf '%s\n' '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:improvement"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "another repository"

t_case "the other issue of the queue still starts beside a held one"
reset
slots 2
printf '%s\n' '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:improvement"},{"name":"cross-project"}]},{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
jq -n '[{event: "labeled", label: {name: "tehas:ready"}, actor: {login: "alice"}, created_at: "2026-10-01T00:00:00Z"}]' \
  >"$fx/events-7.json"
cp "$fx/events-7.json" "$fx/events-8.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
t_contains "$t_out" "skip #7: it carries cross-project"

t_case "a planning session on such an issue is allowed: deciding where the fix lives is its work"
reset
jq -cn '{number: 7, state: "open", title: "Gate runs twice", body: "It does.",
  labels: [{name: "tehas:plan"}, {name: "tehas:improvement"}, {name: "cross-project"}]}' >"$fx/work-7.json"
echo '[]' >"$fx/ready.json"
echo '[{"number":7,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"event":"labeled","label":{"name":"tehas:plan"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
echo '{"title":"Gate runs twice","body":"It does."}' >"$fx/issue-7.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--add-label tehas:planning"
t_lacks "$t_out" "another repository"

t_case "a dry run of a held issue says it and changes nothing"
reset
found_cross
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: it carries cross-project and tehas:improvement"
t_lacks "$t_out" "would run a cycle"
t_lacks "$(t_calls gh)" "issue edit"

t_case "a closed issue whose cycle still holds its lock is left alone"
reset
slots 2
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
hold 9
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue edit 9"
release

t_case "closed issues are asked for too"
reset
t_run "$dispatch"
bash_reads && t_contains "$(t_calls gh)" "issue list --repo acme/widget --state all --label tehas:running"

t_case "a lock file nobody holds does not make it a running cycle"
reset
echo '[{"number":9}]' >"$fx/marked.json"
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/9.lock"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"

t_case "an issue whose cycle holds its lock is left alone"
reset
slots 2
echo '[{"number":9}]' >"$fx/marked.json"
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "issue edit 9"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"
release

t_case "it is found while the slots are full too"
reset
echo '[{"number":8},{"number":9}]' >"$fx/marked.json"
hold 8
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue edit 8"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_eq "$(t_calls fake-start)" ""
release

t_case "when its label cannot be changed nothing is said on it, and the poll goes on"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo "issue edit 9" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: label change on #9 failed"
t_lacks "$(t_calls gh)" "issue comment 9"
t_eq "$(t_calls fake-start)" "7"

t_case "a dry run says so and changes nothing"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would park #9: it is marked tehas:running and no cycle holds its lock"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls docker)" "rm "

t_case "a container left by a cycle whose lock nobody holds is removed, and that is logged"
reset
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "rm -f tehas-work-9"
t_contains "$t_out" "removed the container tehas-work-9: no cycle holds the lock of #9"

t_case "with no container left the removal says nothing"
reset
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_contains "$(t_calls docker)" "rm -f tehas-work-9"
t_lacks "$t_out" "removed the container"

t_case "a closed issue's left container goes too"
reset
echo '[{"number":9,"state":"CLOSED"}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
t_run "$dispatch"
t_contains "$t_out" "removed the container tehas-work-9"

t_case "the container of a cycle that holds its lock is not touched"
reset
slots 2
echo '[{"number":9}]' >"$fx/marked.json"
: >"$fx/orphan-tehas-work-9"
hold 9
t_run "$dispatch"
t_lacks "$(t_calls docker)" "rm "
release

t_case "a paused factory still clears it"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_contains "$t_out" "park #9: it is marked tehas:running and no cycle holds its lock"
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"

t_case "when the issues marked as running cannot be read nothing is dispatched"
# The Rust poll has no list of its own to fail: its one page is the read.
if bash_reads; then
  reset
  rm -f "$fx/marked.json"
  t_run "$dispatch"
  t_eq "$t_rc" 1
  t_contains "$t_out" "ERROR: gh issue list failed"
  t_lacks "$(t_calls gh)" "issue edit"
  t_eq "$(t_calls fake-start)" ""
fi

t_case "an answer that is no list of issues dispatches nothing"
reset
echo '{"oops":1}' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 1
bash_reads && t_contains "$t_out" "ERROR: unreadable list of issues marked as running"
t_eq "$(t_calls fake-start)" ""

t_case "the lock file of a cycle that ended holds nothing back"
reset
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/9.lock"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a cycle that cannot be started is handed to a human, exit 1"
reset
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start the cycle"

t_case "a cycle that never says it holds its lock is handed to a human"
reset
: >"$fx/start-silent"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a cycle that holds its lock but wrote no marker in time is taken as started"
reset
# The holder keeps the cycle's lock for a few seconds with its output detached,
# so the stub returns at once and the test's pipe is not held open.
t_stub fake-late '( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; exec sleep 5 ) >/dev/null 2>&1 &
echo $! >"$T/late.pid"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-late TEHAS_CYCLE_START_WAIT=1 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index TEHAS_INDEX_START_CMD=fake-index-start "$slice_env")
t_run "$dispatch"
kill "$(<"$T/late.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #7 holds its lock but did not say so in 1s; taking it as started"
t_lacks "$(t_calls gh)" "needs-human-attention"

t_case "the start of an earlier cycle on record does not count"
reset
: >"$fx/start-silent"
t_started "$T/home/tehas" 7 --by-hand
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"
t_eq "$(t_row cycle-show 7 -- .cycle.attempt)" 1
# The approval this poll recorded is taken off again: a unit that comes up
# late finds none to use.
t_eq "$(t_row approval-current 7 -- '[.approval, .current] | tostring')" "[null,null]"

# undo_busy <times>: the store answers the undo of an approval busy for its
# first <times> calls, and runs the real store for everything else.
undo_busy() {
  local bin=$T/home/tehas/libexec/store
  rm -f "$bin" "$T/undo.n"
  printf '%s\n' '#!/usr/bin/env bash' \
    "printf '%s\\n' \"\$*\" >>$(t_q "$T/calls/store")" \
    'case " $* " in *" --undo "*)' \
    "  n=\$((\$(cat $(t_q "$T/undo.n") 2>/dev/null || echo 0) + 1)); echo \"\$n\" >$(t_q "$T/undo.n")" \
    "  if [ \"\$n\" -le $1 ]; then echo 'store: busy: the database is locked' >&2; exit 1; fi ;;" \
    'esac' \
    "exec $(t_q "$T_STORE") \"\$@\"" >"$bin"
  chmod 755 "$bin"
}

t_case "a busy store is asked again to take back the approval of a start that did not come up"
reset
: >"$fx/start-silent"
undo_busy 1
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
t_stub fake-stop
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls store | grep -c -- '--undo')" 2
t_eq "$(t_row approval-current 7 -- '.approval | tostring')" null
t_eq "$(t_calls fake-stop)" ""

t_case "when the approval of a start that did not come up stays on record, the unit is stopped"
reset
: >"$fx/start-silent"
undo_busy 9
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
t_stub fake-stop
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls store | grep -c -- '--undo')" 3
t_contains "$t_out" "WARN: could not take the approval of #7 off the record (busy)"
t_eq "$(t_calls fake-stop)" 7
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a cycle that refuses its number after its start still counts as started"
reset
: >"$fx/start-refused"
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$t_out" "did not start"
t_eq "$(t_row cycle-show 7 -- '[.cycle.attempt, (.cycle.ended_at != null), .cycle.end_reason] | tostring')" '[1,true,null]'

t_case "a cycle that ended at once still counts as started"
reset
# fake-start writes the marker and is gone: no lock is held when the poll looks.
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "needs-human-attention"
t_lacks "$t_out" "did not start"

# The analytics program, where install.sh puts it. Each call notes what the
# poll had done when it came: the approval record, the start, the tidy, and
# how many times the status was refreshed.
analytics_stub() { # [<bash body>]
  t_stub_analytics 'printf "%s:%s%s%s%s\n" "$*" \
  "$("$T/store-bin" approval-current 7 | jq -e ".approval != null or .current != null" >/dev/null && echo " approved")" \
  "$([ -e "$T/calls/fake-start" ] && echo " started")" \
  "$([ -e "$T/calls/fake-tidy" ] && echo " tidied")" \
  " status=$(wc -l <"$T/calls/fake-status" 2>/dev/null || echo 0)" >>"$T/calls/analytics.moment"
'"${1:-}"
}

t_case "a start records ready once the approval is written and started once the cycle runs; the poll ends with a sample and a prune"
reset
analytics_stub
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls analytics)" "event 7 ready
event 7 started
sample
prune"
t_eq "$(<"$T/calls/analytics.moment")" "event 7 ready: approved status=1
event 7 started: approved started status=1
sample: approved started tidied status=3
prune: approved started tidied status=3"
t_lacks "$t_out" "analytics"

t_case "ready is recorded at the time tehas:ready was accepted, as the approval record says it"
reset
analytics_stub
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(approval .at)" "2026-01-01T00:00:00Z"
t_eq "$(t_calls analytics)" "event 7 ready --at 1767225600
event 7 started
sample
prune"

t_case "an approval GitHub gave no time for is on record at the poll's, and ready has none"
reset
analytics_stub
before=$(date +%s)
t_run "$dispatch"
t_eq "$(($(t_row approval-current 7 -- .current.at) >= before))" 1
t_eq "$(t_calls analytics | head -n 1)" "event 7 ready"

t_case "a cycle that does not start is ready and not started"
reset
analytics_stub
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls analytics)" "event 7 ready
sample
prune"

t_case "a poll that starts nothing, a held poll and a paused poll take no sample; a poll GitHub fails still does"
reset
analytics_stub
echo '[]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics)" "prune"
reset
analytics_stub
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(t_calls analytics)" "prune"
reset
analytics_stub
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_eq "$(t_calls analytics)" "prune"
reset
# The poll's read of GitHub fails before any write, so nothing marks the queue
# changed: the sample of it would say what the poll never read, so it is the
# next poll's.
analytics_stub
rm -f "$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls analytics)" "prune"

t_case "a dry run and TEHAS_ANALYTICS=off call no analytics"
reset
analytics_stub
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics)" ""
reset
analytics_stub
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_ANALYTICS=off
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls analytics)" ""

t_case "a failing analytics is a warning, and the poll starts its cycle and ends as without it"
reset
analytics_stub 'exit 70'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "WARN analytics event: exit 70"
t_contains "$t_out" "WARN analytics sample: exit 70"
t_contains "$t_out" "WARN analytics prune: exit 70"
t_eq "$(grep -c "WARN analytics" <<<"$t_out")" 4
reset
analytics_stub 'exit 70'
rm -f "$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1

t_case "an analytics that hangs is cut off, and the poll goes on"
reset
analytics_stub 'exec sleep 30'
T_ENV+=(TEHAS_ANALYTICS_WAIT=1)
before=$(date +%s)
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(($(date +%s) - before < 20))" 1
t_eq "$(t_calls fake-start)" "7"
t_eq "$(grep -c "WARN analytics .*: exit 124" <<<"$t_out")" 4

# --- The approval and the start on the state store ------------------------------
t_case "a state store that is not installed stops the poll at 78 before any call to GitHub"
reset
rm -f "$T/home/tehas/libexec/store"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""

t_case "a label that cannot be changed takes the approval off the record again, and the last start's stays current (A2)"
reset
past 7 bob label 2026-01-01T00:00:00Z
printf '%s\n' "--add-label tehas:running" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not mark #7 as running, not starting a cycle"
t_eq "$(t_calls fake-start)" ""
t_eq "$(t_row approval-current 7 -- '[.approval, .current.by] | tostring')" '[null,"bob"]'
t_eq "$(t_row cycle-show 7 -- .cycle.attempt)" 1

t_case "an attempt on record that cannot be read before the label changes no label (D2)"
reset
t_store_answers cycle-show 1 'store: busy #7: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: #7: the store could not say which attempt is on record (busy), not starting a cycle"
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_eq "$(t_calls fake-start)" ""
t_eq "$(t_row approval-current 7 -- '[.approval, .current] | tostring')" '[null,null]'

t_case "a running list that cannot be read ends the poll before it acts on any cycle (D2)"
reset
hold 9
t_store_answers cycle-show 1 'store: busy #0: the database is locked'
t_run "$dispatch"
release
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the state store could not list the running cycles (busy)"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""

t_case "an approval starts one cycle: a second poll on the same label event starts nothing (A3)"
reset
queue 7
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "skip #7: its approval of 2026-10-01T00:00:00Z started a cycle already"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_eq "$(t_row cycle-show 7 -- .cycle.attempt)" 1

# A store at the installed path whose cycle-show of #7 answers busy while
# $T/busy-left counts down, or takes 2 seconds while $T/slow is there, and
# runs the real store for everything else.
slow_store() {
  rm -f "$T/home/tehas/libexec/store" "$T/busy-left" "$T/slow" "$T/looks"
  printf '%s\n' '#!/usr/bin/env bash' \
    'if [ "$1 ${2:-}" = "cycle-show 7" ]; then' \
    "  n=\$(cat $(t_q "$T/busy-left") 2>/dev/null || echo 0)" \
    "  if [ \"\$n\" -gt 0 ]; then echo \$((n - 1)) >$(t_q "$T/busy-left"); echo 'store: busy #7: the database is locked' >&2; exit 1; fi" \
    "  [ ! -e $(t_q "$T/slow") ] || { echo look >>$(t_q "$T/looks"); $(t_q "$(command -pv sleep)") 2; }" \
    'fi' \
    "exec $(t_q "$T_STORE") \"\$@\"" >"$T/home/tehas/libexec/store"
  chmod 755 "$T/home/tehas/libexec/store"
}

t_case "a store that is busy during the wait still finds the start"
reset
slow_store
# Three looks must fit in the wait, which a loaded runner cannot promise in 2
# seconds. The start is found at the third look, so the case takes no longer.
T_ENV=("${T_ENV[@]/#TEHAS_CYCLE_START_WAIT=*/TEHAS_CYCLE_START_WAIT=10}")
# The store is busy for the first two looks after the unit is started.
t_stub fake-start 'echo 2 >"$T/busy-left"
flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-start "$1" >/dev/null'
t_run "$dispatch"
default_start
t_eq "$t_rc" 0
t_lacks "$t_out" "did not start"
t_lacks "$t_out" "taking it as started"
t_eq "$(<"$T/busy-left")" 0

t_case "the wait is bounded by the clock, however long each look takes"
reset
slow_store
: >"$fx/start-silent"
t_stub fake-start ': >"$T/slow"'
t_run "$dispatch"
default_start
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"
# TEHAS_CYCLE_START_WAIT is 2 and a look takes 2 seconds: the clock ends the
# wait after one look, two at most. A count of looks would make four. A loaded
# runner makes looks slower, never more of them, so this does not time out.
t_eq "$(($(wc -l <"$T/looks") <= 2))" 1

t_done
