#!/usr/bin/env bash
# Tests for libexec/dispatch: labels on parked and planning issues, planning
# turns run as units, and a bot account in the allowed list.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "a parked issue retried by its labels starts a cycle, and its old question goes"
parked
comments 7 "$(park_body ' ' ' ')" ""
echo '[{"number":7,"labels":[{"name":"tehas:question"}]}]' >"$fx/asking.json"
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --remove-label tehas:question')" 1
t_eq "$([ -e "$park_id" ] && echo kept)" ""

t_case "the question label is only taken off an issue that carries it"
reset
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready')" 1

# tehas:ready from an allowed account starts a cycle whatever else the issue
# carries. Only a cycle that is alive holds it back.
has() { # issue label...: the queue is that one issue, labelled ready by alice, carrying the labels too
  local n=$1 l list='{"name":"tehas:ready"}'
  shift
  for l in "$@"; do list="$list,{\"name\":\"$l\"}"; done
  echo "[{\"number\":$n,\"labels\":[$list]}]" >"$fx/ready.json"
}

t_case "tehas:ready on a parked issue starts a cycle and leaves needs-human-attention to the cycle's restart"
reset
has 7 needs-human-attention
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready')" 1
t_lacks "$(t_calls gh)" "--remove-label needs-human-attention"

t_case "tehas:ready on an issue that awaits a merge starts a cycle and takes that label off"
reset
has 7 tehas:awaiting-merge
echo '[]' >"$fx/awaiting.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --remove-label tehas:awaiting-merge')" 1

t_case "every label of tehas's of an earlier outcome goes with the start"
reset
has 7 needs-human-attention tehas:question tehas:awaiting-merge
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready --remove-label tehas:awaiting-merge --remove-label tehas:question')" 1

t_case "an issue left marked as running with no cycle is started on tehas:ready"
reset
has 7 tehas:running
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready')" 1

t_case "a parked issue labelled by a stranger starts nothing and loses that label"
reset
has 7 needs-human-attention
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"
t_lacks "$(t_calls gh)" "--add-label tehas:running"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'mallory', not an allowed account"

t_case "tehas:ready applied while the issue's cycle runs waits, said in the log and not on the issue"
reset
slots 2
has 7 tehas:running
hold 7
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "wait #7: its cycle is still running"

t_case "and the poll after that cycle ended starts a fresh one"
release
has 7 tehas:awaiting-merge
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "--add-label tehas:running --remove-label tehas:ready --remove-label tehas:awaiting-merge"

t_case "an issue that waits behind its own cycle does not hold back the next one"
reset
slots 2
ready 7 8
hold 7
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
release

t_case "a retry tick takes a tehas:ready that was left on the issue with it"
parked
comments 7 "$(park_body x ' ')" alice
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"},{"name":"tehas:ready"}]}]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls gh | grep -c -x -e 'issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question --remove-label tehas:ready')" 1

t_case "every list of issues is read to a thousand"
reset
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c -e '^issue list .* --limit 1000 ')" 6
t_lacks "$(t_calls gh)" "--limit 100 "

t_case "an answer on a question the session is not waiting on runs no turn"
planning
jq '.comment = 9999' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"
t_contains "$t_out" "skip #7: its question is not among the last 100 comments"

t_case "an answer to a session whose record names no question runs no turn"
planning
jq 'del(.comment)' "$record" >"$record.new" && mv "$record.new" "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "skip #7: the question answered is not the one this session is waiting on"
t_eq "$([ -e "$record" ] && echo kept)" kept

t_case "a newer stray bot comment with a plan marker does not lock the session out"
planning
comments 7 "$(plan_body ' ' x ' ')" alice "$(jq -nc --arg b "$(plan_body ' ' ' ' ' ' | sed 's/turn=1/turn=5/')" '[{databaseId: 4009, author: {login: "tehas-bot"}, body: $b,
  createdAt: "2026-01-01T02:00:00Z", lastEditedAt: null, editor: null}]')"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --session abc-123 --answer $record.answer"
t_eq "$(cat "$record.answer")" "The operator chose: Change some"

t_case "a planning question on record whose marker is a park marker runs no turn and no cycle"
planning
echo '[{"number":7,"labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' >"$fx/asking.json"
comments 7 "$(park_body x ' ')" alice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: the question on record does not belong to this issue"

t_case "a question that posted but could not be labelled is not lost"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
printf '%s' '--add-label tehas:question' >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(jq -r '[.comment, .pending] | @tsv' "$record")" "4002	true"
t_eq "$([ -s "$record.out" ] && echo kept)" kept
rm -f "$T"/calls/* "$fx/edit-fails-on"
echo '[]' >"$fx/asking.json"
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c 'issues/7/comments')" 0
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"
t_eq "$(jq -r .pending "$record")" false
t_eq "$([ -e "$record.out" ] && echo left)" ""

t_case "a later turn cut off by a restart is reported and can be answered"
planning
echo '[]' >"$fx/asking.json"
jq '.running = true' "$record" >"$record.new" && mv "$record.new" "$record"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it was cut off"
t_contains "$(t_calls gh)" "--add-label tehas:question --add-label needs-human-attention"
t_eq "$(jq -r '[.session, .turns, .failed, .running] | @tsv' "$record")" "abc-123	2	true	false"
t_eq "$(plan_calls)" ""

t_case "the container of a cut-off turn is removed before the turn is reported"
t_contains "$(t_calls docker)" "rm -f tehas-plan-7"
t_case "a poll does not wait for a planning turn: a ready issue starts while it runs"
to_plan
echo 7 >"$fx/plan-runs"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "planning #7: turn 1 runs as tehas-plan-7.service"
t_eq "$(jq -r .running "$record")" true
t_lacks "$(t_calls gh)" "kind=plan"
echo '[{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-8.json"
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "8"
t_eq "$(jq -r .running "$record")" true
t_lacks "$(t_calls gh)" "kind=plan"
t_eq "$(plan_calls)" ""

t_case "the poll after a turn ends posts its outcome, once, though systemd still shows its unit"
echo '[]' >"$fx/ready.json"
# The first poll took tehas:plan off the issue.
echo '[]' >"$fx/plans.json"
cp "$fx/plan-result.json" "$record.out"
echo 0 >"$record.rc"
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=1 -->"
t_eq "$(jq -r '[.session, .turns, .running] | @tsv' "$record")" "abc-123	1	false"
t_eq "$([ -e "$record.rc" ] && echo left)" ""
rm -f "$fx/plan-runs"
rm -f "$T"/calls/*
t_run "$dispatch"
t_lacks "$(t_calls gh)" "kind=plan"
t_eq "$(plan_calls)" ""

t_case "a turn's exit status other than 0 is reported as a failed turn"
planning
echo '[]' >"$fx/asking.json"
jq '.running = true' "$record" >"$record.new" && mv "$record.new" "$record"
echo 2 >"$record.rc"
t_run "$dispatch"
t_contains "$(t_calls gh)" "This planning turn did not finish: it exited with status 2"
t_eq "$(jq -r '[.turns, .failed, .running] | @tsv' "$record")" "2	true	false"
t_lacks "$(t_calls docker)" "rm -f tehas-plan-7"

t_case "an answer waits while another session's turn runs"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
echo '{"session":"xyz","turns":1,"options":[],"pending":false,"failed":false,"running":true}' >"$T/home/tehas/state/plan-9.json"
echo 9 >"$fx/plan-runs"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "planning #7 waits: the turn of #9 is still running"
t_eq "$(plan_calls)" ""
t_eq "$(jq -r .turns "$record")" 1
rm -f "$T/home/tehas/state/plan-9.json"

t_case "a turn whose unit cannot start is reported as a failed turn"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
t_stub fake-plan-start 'exit 1'
t_run "$dispatch"
t_contains "$t_out" "ERROR: could not start the planning turn of #7"
t_contains "$(t_calls gh)" "This planning turn did not finish: it could not be started as a unit"
t_eq "$(jq -r '[.failed, .running] | @tsv' "$record")" "true	false"
t_stub fake-plan-start 'n=$1; shift
[ ! -e "$T/fx/plan-runs" ] || exit 0
"$@"'

t_case "a turn that does not end: the poll returns, the turn holds its lock, and a second poll starts no second turn"
to_plan
# The turn waits for a file; the launcher runs it in the background, as
# systemd-run leaves a unit running.
cp "$T/bin/fake-plan" "$T/bin/fake-plan.keep"
t_stub fake-plan 'out=""
while [ "$#" -gt 0 ]; do [ "$1" = --out ] && out=$2; shift; done
[ ! -e "/proc/$$/fd/9" ] || : >"$T/calls/plan.fd9"
while [ ! -e "$T/fx/plan-done" ]; do sleep 0.1; done
cp "$T/fx/plan-result.json" "$out"'
t_stub fake-plan-start 'n=$1; shift
setsid "$@" </dev/null >/dev/null 2>&1 &'
rm -f "$fx/plan-done"
began=$(date +%s)
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(( $(date +%s) - began < 20 ))" 1
t_contains "$t_out" "planning #7: turn 1 runs as tehas-plan-7.service"
for _ in $(seq 50); do ! flock -n "$record.lock" true 2>/dev/null && break; sleep 0.1; done
t_eq "$(flock -n "$record.lock" true 2>/dev/null && echo free || echo held)" held
t_eq "$(jq -r '.running, (.startedAt | test("^[0-9-]+T[0-9:]+Z$"))' "$record" | paste -sd ' ')" "true true"
# The first poll took tehas:plan off; a label put back by hand finds the turn running.
rm -f "$T"/calls/*
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-plan-start)" ""
t_lacks "$(t_calls gh)" "kind=plan"
t_eq "$(jq -r .running "$record")" true
t_case "a turn that ends writes its status and asks for a poll, which posts its outcome"
rm -f "$T/home/tehas/state/dispatch.again"
: >"$fx/plan-done"
for _ in $(seq 50); do [ -e "$record.rc" ] && [ -e "$T/home/tehas/state/dispatch.again" ] && break; sleep 0.1; done
t_eq "$(cat "$record.rc" 2>/dev/null)" 0
t_eq "$([ -e "$T/home/tehas/state/dispatch.again" ] && echo asked)" asked
t_eq "$(t_calls plan.fd9)" ""
echo '[]' >"$fx/plans.json"
rm -f "$T"/calls/*
t_run "$dispatch"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=1 -->"
t_eq "$(jq -r '[.turns, .running] | @tsv' "$record")" "1	false"
t_stub fake-plan-start 'n=$1; shift
[ ! -e "$T/fx/plan-runs" ] || exit 0
"$@"'
mv "$T/bin/fake-plan.keep" "$T/bin/fake-plan"

t_case "a first turn cut off by a restart ends visibly"
planning
echo '[]' >"$fx/asking.json"
echo '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true}' >"$record"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "a first turn cut off by a restart says nothing while its label change fails"
planning
echo '[]' >"$fx/asking.json"
echo '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true}' >"$record"
: >"$fx/edit-fails"
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(jq -r .running "$record")" true

t_case "a record of a turn in flight or waiting to be posted is not cleaned up"
planning
echo '[]' >"$fx/asking.json"
jq '.running = true' "$record" >"$record.new" && mv "$record.new" "$record"
t_run "$dispatch --dry-run"
t_eq "$([ -e "$record" ] && echo kept)" kept
planning
echo '[]' >"$fx/asking.json"
jq '.pending = true | del(.comment)' "$record" >"$record.new" && mv "$record.new" "$record"
cp "$fx/plan-result.json" "$record.out"
: >"$fx/post-fails"
t_run "$dispatch"
t_eq "$([ -e "$record" ] && echo kept)" kept
t_eq "$(jq -r .pending "$record")" true

t_case "the end of a session is said once"
planning
comments 7 "$(plan_body ' ' ' ' x)" alice
printf '%s' '--remove-label needs-human-attention' >"$fx/edit-fails-on"
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$([ -e "$record" ] && echo kept)" kept
rm -f "$T"/calls/* "$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(t_calls gh | grep -c 'Planning session ended by alice.')" 1
t_contains "$(t_calls gh)" "--remove-label needs-human-attention"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "a session whose record is gone is not announced while its labels cannot be removed"
planning
rm "$record"
comments 7 "$(plan_body x ' ' ' ')" alice
: >"$fx/edit-fails"
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue comment"

t_case "a dry run reports a cut-off turn and changes nothing"
planning
echo '[]' >"$fx/asking.json"
jq '.running = true' "$record" >"$record.new" && mv "$record.new" "$record"
before=$(cat "$record")
t_run "$dispatch --dry-run"
t_contains "$t_out" "would report the cut-off planning turn of #7"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "api -X"
t_eq "$(cat "$record")" "$before"

t_case "a file in the state directory that is not a session record is left alone"
reset
echo '[]' >"$fx/ready.json"
mkdir -p "$T/home/tehas/state"
echo '{"running":true}' >"$T/home/tehas/state/plan-x.json"
echo 'not json' >"$T/home/tehas/state/plan-9.json"
t_run "$dispatch"
t_eq "$([ -e "$T/home/tehas/state/plan-x.json" ] && [ -e "$T/home/tehas/state/plan-9.json" ] && echo kept)" kept
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(grep -c 'planning record of #9 is not readable' <<<"$t_out")" 1

t_case "a bot account in the allowed list stops the poll before any question is read"
parked
comments 7 "$(park_body x ' ')" alice
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice tehas-bot'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "the bot account tehas-bot is in TEHAS_ALLOWED_LABELERS"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X"

t_case "the bot is refused as an allowed labeller even with no question open"
reset
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice tehas-bot'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "the bot account tehas-bot is in TEHAS_ALLOWED_LABELERS"
t_contains "$(t_calls gh)" "api user"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "an instance that lists the bot is refused before its slots are swept"
reset
echo '[{"number":9}]' >"$fx/marked.json"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice tehas-bot'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "the bot account tehas-bot is in TEHAS_ALLOWED_LABELERS"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(t_calls fake-start)" ""

t_case "an own login that GitHub answers without one ends the poll before anything is written"
reset
echo '[{"number":9}]' >"$fx/marked.json"
echo '{"message":"Bad credentials"}' >"$fx/user.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the bot's own login could not be read"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(t_calls fake-start)" ""

t_done
