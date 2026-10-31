#!/usr/bin/env bash
# Tests for libexec/dispatch: a relabelled start held on the body its last cycle
# parked on, and the retry after a provider outage.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# --- A relabelled start of an issue whose last cycle parked --------------------
# The text the poll reads for #7, in the one answer that carries the edit
# record: unchanged_since keeps the body from it for the approval record, and
# park_holds compares the hash of it with the park's.
issue_body() { # <body>
  jq -cn --arg b "$1" '{data: {repository: {issue: {lastEditedAt: null, editor: null,
    body: $b}}}}' >"$fx/edit-7.json"
}

# The park of the cycle that just stopped, on record: alice's label started it
# on "Words.", the body its approval bound, and it parked on the reason given
# [intent-park], or by hand when the second word is -. Issue 7 carries
# tehas:ready again, by alice, with the body given.
parked_again() { # <body of the issue> [<park reason> [<- for a cycle by hand>]]
  parked "${2:-intent-park}" "${3:-alice}"
  # The endpoint answers: the hold of no resources is asked at rule 3, before a
  # candidate is read, so without it no start is weighed at all. curl is asked
  # for real unless mesh_curl stands in for it, and models.json is the host's
  # own file, so this follows parked, which resets the instance.
  mesh '{"data":[{"id":"test-model"}]}'
  mesh_curl
  park_comments "Words."
  # The body and the title come in the same answer as the edit record, so this
  # fixture is the text the poll reads. An answer with no title binds none.
  issue_body "$1"
  jq -cn --arg b "$1" '{number: 7, state: "open", body: $b,
    labels: [{name: "needs-human-attention"}, {name: "tehas:question"}, {name: "tehas:ready"}]}' \
    >"$fx/work-7.json"
  echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' \
    >"$fx/ready.json"
}

# The comments of the parked issue, as both polls read them: the bot's park
# question of the cycle that stopped, and (in icomments, which is REST) nothing
# yet. A case that gives the millwright a note writes icomments and calls
# park_comments again, so the one answer the Rust poll reads holds both.
park_comments() { # <body of the parked issue> [<notes as a REST list>]
  comments 7 "$(park_body ' ' ' ')" "" "${2:-[]}"
  local bot
  bot=$(jq -cn --arg b "$(park_body ' ' ' ')" '{id: 4001, user: {login: "tehas-bot"},
    created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z", body: $b}')
  jq -cn --argjson bot "$bot" --argjson after "${2:-[]}" '[$bot] + $after' \
    >"$fx/icomments-7.json"
}

# What a poll decided, from the poll that starts here. t_calls reads one file
# per stub that every t_run of the case appends to, so a second poll of one
# case reads the first poll's calls too: the lines of each poll are cut off with
# this before the call that reads them.
new_poll() { rm -f "$T"/calls/*; }

# A park that holds keeps the labels, the park and the tick of its boxes.
holds() { # <body>: one poll on an issue parked on that text
  new_poll
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit 7"
  t_eq "$(park_of '[.attempt, .retries, .question]')" '[1,0,4001]'
}

t_case "a relabelled start on the body its last cycle parked on is held, said once on the issue"
parked_again "Words."
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Not started: the last cycle parked on \`intent-park\` and the issue text has not changed since. Edit the body, or tick Retry in the park comment."
t_eq "$(park_of '.told_at != null')" true
t_eq "$(grep -c '^issue comment 7' <<<"$(t_calls gh)")" 1

t_case "and the next poll of the same park only logs, with no comment"
parked_again "Words."
holds "Words."
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_lacks "$(t_calls gh)" "issue comment 7"

t_case "the millwright's tehas:ready on the body its last cycle parked on is held too"
# A second develop timeout on unchanged text offers no retry box, and leaves
# the millwright's label as the one way round it: held as a person's is.
parked_again "Words."
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-fixer"},"created_at":"2026-10-02T00:00:00Z"}]' >"$fx/events-7.json"
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"

# --- The millwright's guidance comment lifts the hold -------------------------
# The park of #7 on the body "Words." with the millwright's tehas:ready on it
# and alice's approval of that same body behind it: what a park leaves when the
# millwright is the one account that may yet start it. The instance is what
# gives a cycle the millwright's note, so the settings of the case decide
# whether a comment of guidance lifts anything. The label's instant is the
# minutes given before now, so the notes of the cases are timed from it and the
# window does not drift as the clock moves.
millwright_parked() { # [TEHAS_x=0/1 ...]
  parked_again "Words."
  local mins=${LIFT_MINS:-2}
  local at
  at=$(date -u -d "@$(($(date -u +%s) - mins * 60))" +%Y-%m-%dT%H:%M:%SZ)
  LIFT_LABEL_AT=$at
  t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer "${@:-TEHAS_WORK_FILES=1}"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-fixer"},"created_at":"'"$at"'"}]' >"$fx/events-7.json"
}

# The time of the window at <offset> seconds before the label of the case.
lift_at() { # <seconds>
  date -u -d "${LIFT_LABEL_AT} -$1 seconds" +%Y-%m-%dT%H:%M:%SZ
}

# The time <offset> seconds after the label of the case.
label_later() { # <seconds>
  date -u -d "${LIFT_LABEL_AT} +$1 seconds" +%Y-%m-%dT%H:%M:%SZ
}

# A change note of the millwright, its id, its account, its time and its text.
change_note() { # <id> <login> <time> <text>
  icomment "$1" "$2" "$3" "$3" "<!-- tehas:millwright issue=7 action=change -->
$4"
}

# The comments of the parked issue with <notes> (a REST list of them) added:
# the one write that puts them where both polls read comments.
with_notes() { # <notes as a REST list>
  park_comments "Words." "$1"
}

# The park's question taken off #7, as leaving it parked does.
no_question() {
  echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"}]}]' >"$fx/ready.json"
  echo '[]' >"$fx/asking.json"
  jq -c '.labels |= map(select(.name != "tehas:question"))' "$fx/work-7.json" >"$fx/work-7.new" && mv "$fx/work-7.new" "$fx/work-7.json"
}

# A lifted park starts the cycle, and nothing of the hold is said or told.
lifts() {
  new_poll
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" "7"
  t_lacks "$t_out" "hold #7: its last cycle parked on"
  t_lacks "$(t_calls gh)" "Not started: the last cycle parked on"
  t_eq "$(park_of)" null
}

t_case "the millwright's action=change comment of the ten minutes before its label lifts the hold, and the record says so"
millwright_parked
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Do not run the whole suite." | jq -s .)"
lifts
t_contains "$t_out" "lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the fixer's guidance comment"
t_contains "$t_out" "start #7 (labelled by the fixer by tehas-fixer)"
t_eq "$(approval '[.by, .how, .fixer_starts, .rests_on.by, .rests_on.how]')" \
  '["tehas-fixer","fixer",1,"alice","label"]'
# The lift is its own field: rests_on keeps naming alice's approval, the
# refresh's base, and the note is the comment the cycle is given.
t_eq "$(approval .park_lifted)" \
  "$(sorted '{"source":"millwright:81","by":"tehas-fixer","at":"'"$(lift_at 180)"'","text":"<!-- tehas:millwright issue=7 action=change -->\nDo not run the whole suite."}')"
t_eq "$(approval '.rests_on.body_sha256 == .body_sha256')" true
t_eq "$(approval .retry_context)" "$(approval '[.park_lifted]')"

t_case "the lift needs the instance to give a cycle the note: images lift it, neither setting holds"
millwright_parked TEHAS_WORK_IMAGES=1
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Look at the shot." | jq -s .)"
lifts
millwright_parked TEHAS_WORK_IMAGES=0 TEHAS_WORK_FILES=0
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Reaches no cycle." | jq -s .)"
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_lacks "$t_out" "lift #7"
t_eq "$(park_of '.told_at != null')" true

t_case "an action=retry comment of the same window lifts nothing: that ask is the run again"
millwright_parked
with_notes "$(icomment 81 tehas-fixer "$(lift_at 180)" "$(lift_at 180)" '<!-- tehas:millwright issue=7 action=retry -->
Once more, the same.' | jq -s .)"
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_lacks "$t_out" "lift #7"

t_case "two change notes of the window: the newest of them is the one the record names"
millwright_parked
with_notes "$(
  echo '['
  change_note 81 tehas-fixer "$(lift_at 300)" "First the tests, then the build."
  echo ','
  change_note 82 tehas-fixer "$(lift_at 60)" "Run only the named tests."
  echo ']'
)"
lifts
t_eq "$(approval .park_lifted.source)" 'millwright:82'

t_case "a change note outside the ten minutes, or none at all, lifts nothing"
millwright_parked
with_notes "$(change_note 81 tehas-fixer "$(lift_at 900)" "Out of its time." | jq -s .)"
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_lacks "$t_out" "lift #7"

t_case "a change note of another account, or one saved after the label, lifts nothing"
millwright_parked
with_notes "$(
  echo '['
  change_note 81 mallory "$(lift_at 180)" "Not the millwright."
  echo ','
  icomment 82 tehas-fixer "$(lift_at 120)" "$(label_later 3600)" '<!-- tehas:millwright issue=7 action=change -->
Saved after the label.'
  echo ']'
)"
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"

t_case "a newer action=retry comment takes the lift away: the note on record would ask for the same run"
millwright_parked
with_notes "$(
  echo '['
  change_note 81 tehas-fixer "$(lift_at 180)" "Guidance."
  echo ','
  icomment 82 tehas-fixer "$(lift_at 60)" "$(lift_at 60)" '<!-- tehas:millwright issue=7 action=retry -->
Once more.'
  echo ']'
)"
holds "Words."
t_lacks "$t_out" "lift #7"

t_case "a change note whose marker names another issue lifts nothing"
millwright_parked
with_notes "$(icomment 81 tehas-fixer "$(lift_at 180)" "$(lift_at 180)" '<!-- tehas:millwright issue=77 action=change -->
Another issue.' | jq -s .)"
holds "Words."
t_lacks "$t_out" "lift #7"

t_case "a parked issue that asks no question has its comments read, and the note lifts the hold"
millwright_parked
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Guidance." | jq -s .)"
no_question
lifts

t_case "a change note saved again before the label lifts only where the poll can see its own account did"
# GraphQL names the editor. REST does not, so the bash poll takes any edit as another's.
millwright_parked
with_notes "$(icomment 81 tehas-fixer "$(lift_at 180)" "$(lift_at 60)" '<!-- tehas:millwright issue=7 action=change -->
Second thoughts.' | jq -s .)"
if bash_reads; then holds "Words."; else lifts; fi

t_case "comments that cannot be read keep the hold, untold, and the next poll asks again"
millwright_parked
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Guidance." | jq -s .)"
# With the question open that read ends the poll before any start is weighed.
no_question
mv "$fx/comments-7.json" "$fx/comments-7.kept"
mv "$fx/icomments-7.json" "$fx/icomments-7.kept"
holds "Words."
t_contains "$t_out" "the fixer's comments could not be read"
t_lacks "$(t_calls gh)" "Not started: the last cycle parked on"
t_eq "$(park_of '.told_at != null')" false
mv "$fx/comments-7.kept" "$fx/comments-7.json"
mv "$fx/icomments-7.kept" "$fx/icomments-7.json"
lifts

t_case "a person's tehas:ready on the parked body is held whatever comments exist"
millwright_parked
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Guidance." | jq -s .)"
# The lift belongs to the millwright's own label: the same note under a
# person's label lifts nothing, and the hold is said and told as ever.
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"'"$LIFT_LABEL_AT"'"}]' >"$fx/events-7.json"
holds "Words."
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_lacks "$t_out" "lift #7"

t_case "at the cap of retries the grant refuses the start, so no note lifts the hold"
millwright_parked
past 7 tehas-fixer fixer 2026-10-01T01:00:00Z
past 7 tehas-fixer fixer 2026-10-01T02:00:00Z
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Guidance." | jq -s .)"
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "the fixer has started 2 cycles on it since alice approved it, the limit"
t_lacks "$t_out" "lift #7"

t_case "with no person's approval behind it the millwright's note lifts nothing"
# The park of a cycle started by hand: no approval of a person stands behind
# it, so the grant itself refuses before the park rule is asked (#1130).
at=$(date -u -d "@$(($(date -u +%s) - 120))" +%Y-%m-%dT%H:%M:%SZ)
LIFT_LABEL_AT=$at
parked intent-park -
mesh '{"data":[{"id":"test-model"}]}'
mesh_curl
park_comments "Words."
issue_body "Words."
jq -cn --arg b "Words." '{number: 7, state: "open", body: $b,
  labels: [{name: "needs-human-attention"}, {name: "tehas:question"}]}' >"$fx/work-7.json"
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' \
  >"$fx/ready.json"
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer TEHAS_WORK_FILES=1
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"tehas-fixer"},"created_at":"'"$at"'"}]' >"$fx/events-7.json"
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Guidance." | jq -s .)"
new_poll
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "no person approved an earlier cycle of it"
t_lacks "$t_out" "lift #7"

t_case "a start the lift let through counts against the cap of retries, and a second one is refused"
millwright_parked TEHAS_WORK_FILES=1 TEHAS_MILLWRIGHT_MAX_RETRIES=1
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Guidance." | jq -s .)"
lifts
t_eq "$(approval .fixer_starts)" 1
# The cycle that lift started parks on the same body: the millwright's next
# label is at the cap, and a second note lifts nothing.
t_park "$T/home/tehas" 7 intent-park 4009
new_poll
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "the fixer has started 1 cycles on it since alice approved it, the limit"
t_lacks "$t_out" "lift #7"

t_case "a dry run of the lift says it and changes nothing, and the park keeps its hold"
millwright_parked
with_notes "$(change_note 81 tehas-fixer "$(lift_at 180)" "Guidance." | jq -s .)"
new_poll
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "lift #7: its last cycle parked on intent-park and the body has not changed since, lifted on the fixer's guidance comment"
t_contains "$t_out" "would run a cycle for #7"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"
t_eq "$(park_of '[.retries, .told_at] | tostring')" '[0,null]'

t_case "a dry run of the hold says it and changes nothing, not its park"
parked_again "Words."
new_poll
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "hold #7: its last cycle parked on intent-park and the body has not changed since"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit 7"
t_lacks "$(t_calls gh)" "issue comment 7"
t_lacks "$(t_calls gh)" "api -X"
t_eq "$(park_of .told_at)" null

t_case "a hold whose tell the store would not take tells it again on the next poll, and says so"
parked_again "Words."
store_refuses "park-told 7" 1 'store: busy #7: the database is locked'
holds "Words."
t_contains "$t_out" "WARN: could not record the hold on the park of #7 (busy)"
holds "Words."
t_eq "$(grep -c '^issue comment 7' <<<"$(t_calls gh)")" 1
t_eq "$(park_of .told_at)" null

t_case "a park that could not be read holds the start for this poll"
parked_again "Words, and more."
store_refuses "park-show 7" 1 'store: busy #7: the database is locked'
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: its park on record could not be read (busy)"
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a park a start has spent holds nothing"
parked_again "Words."
TEHAS_HOME=$T/home/tehas "$T_STORE" park-retry 7 >/dev/null
new_poll
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its last cycle parked on"

# The same hold asks nothing of an urgent or tehas:next issue either: the hold
# is of the poll, where its kind is settled, not of the kind itself.
t_case "an urgent issue parked on its own body is held too"
parked_again "Words."
label_events 7 "tehas:urgent=alice"
holds "Words."

t_case "a changed body starts, and the park of the held go is not current after the start"
parked_again "Words, and more."
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its last cycle parked on"
t_eq "$(park_of)" null

t_case "a ticked retry on the body its last cycle parked on starts all the same"
parked_again "Words."
comments 7 "$(park_body x ' ')" alice
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its last cycle parked on"
t_lacks "$(t_calls gh)" "Not started: the last cycle parked on"
t_eq "$(park_of)" null

# outage_parked [<who started the cycle: alice, carol, or - for by hand>]:
# #7's last cycle parked by a provider outage, on "Words.". Only rule 4a asks
# for it: no label of a person's queues it.
outage_parked() {
  parked_again "Words." provider-outage "${1:-alice}"
  echo '[]' >"$fx/ready.json"
  echo '{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}' >"$fx/issue-7.json"
  new_poll
}

t_case "a park of a provider outage is retried as today, on its own body"
# Rule 4a's own path, on an issue whose park names its body: the hold of 4c is
# what would refuse this start, and the outage is what leaves it to 4a. The
# start rests on the account that started the parked cycle, and the issue
# carries no tehas:ready for the label's path.
outage_parked
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "start #7 (requeued by alice)"
t_lacks "$t_out" "its last cycle parked on"
t_lacks "$(t_calls gh)" "Not started: the last cycle parked on"
t_eq "$(park_of)" null
t_eq "$(at_mark '[.by, .how] | tostring')" '["alice","outage-retry"]'
t_eq "$(t_row cycle-show 7 -- '[.cycle.started_by, .cycle.requeued] | tostring')" '["alice",true]'

t_case "the retry rests on who started the parked cycle, not on a later approval of another account"
outage_parked
# bob's approval, later and never used: the retry is alice's.
t_approve "$T/home/tehas" 7 bob label 1767300000
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "start #7 (requeued by alice)"
t_eq "$(at_mark .by)" alice

t_case "a retry whose account may no longer label spends it, and starts nothing"
outage_parked carol
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: its retry after a provider outage is not started; it waits for a person"
t_eq "$(park_of '[.retries, .automatic]')" '[1,false]'
new_poll
t_run "$dispatch"
t_lacks "$t_out" "its retry after a provider outage"

t_case "a dry run of that leaves the retry owed"
outage_parked carol
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its retry after a provider outage is not started; it waits for a person"
t_eq "$(park_of '[.retries, .automatic]')" '[0,true]'

t_case "an open planning session holds the retry after an outage, and spends it (A13)"
outage_parked
t_plan "$T/home/tehas" 7 '{"turns":1,"comment":4002}'
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: its retry after a provider outage is not started; it waits for a person"
t_eq "$(park_of '[.retries, .automatic]')" '[1,false]'

t_case "one retry after an outage, then a person: the retry's own outage park owes none"
outage_parked
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
# The retry's cycle (attempt 2, requeued) is parked by an outage too.
TEHAS_HOME=$T/home/tehas flock -w 5 "$cycles_dir/7.lock" "$T_STORE" cycle-end 7 --attempt 2 --status handoff \
  --provider --parked >/dev/null
t_park "$T/home/tehas" 7 provider-outage 4009
t_eq "$(park_of '[.attempt, .automatic]')" '[2,false]'
new_poll
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$t_out" "its retry after a provider outage"

t_case "an outage park of a cycle started by hand owes no retry"
outage_parked -
t_eq "$(park_of .automatic)" false
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$t_out" "provider outage"

t_case "a retry whose unit never comes up leaves no retry behind"
outage_parked
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(park_of '[.retries, .automatic]')" '[1,false]'
rm -f "$fx/start-fails"
new_poll
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""

t_case "a retry after an outage whose issue cannot be marked is still owed, and the next poll starts it"
outage_parked
printf '%s' '--add-label tehas:running' >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: could not mark #7 as running, not starting a cycle"
t_eq "$(t_calls fake-start)" ""
t_eq "$(park_of '[.retries, .automatic]')" '[0,true]'
rm -f "$fx/edit-fails-on" "$T"/calls/*
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_eq "$(park_of)" null

t_case "a busy store keeps the retry after an outage for the next poll: nothing starts, the park stays"
outage_parked
store_refuses "park-show --automatic" 1 'store: busy: the database is locked'
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the state store could not list the parks owed a retry (busy)"
t_eq "$(park_of '[.retries, .automatic]')" '[0,true]'
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a store that cannot be used for the retry after an outage stops the poll at 78"
outage_parked
store_refuses "park-show --automatic" 78 'store: corrupt: the file is not a database'
t_run "$dispatch"
t_eq "$t_rc" 78
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "ERROR: the state store could not list the parks owed a retry (corrupt)"
t_lacks "$(t_calls gh)" "issue edit 7"

t_case "a park of a cycle started by hand names no body and starts as today"
# Nothing bound a body for a run by hand: no reason to hold.
parked_again "Words." intent-park -
t_eq "$(park_of .body_sha256)" null
new_poll
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "its last cycle parked on"
t_lacks "$(t_calls gh)" "Not started: the last cycle parked on"

t_done
