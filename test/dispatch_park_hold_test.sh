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
  comments 7 "$(park_body ' ' ' ')" ""
  # The body and the title come in the same answer as the edit record, so this
  # fixture is the text the poll reads. An answer with no title binds none.
  issue_body "$1"
  jq -cn --arg b "$1" '{number: 7, state: "open", body: $b,
    labels: [{name: "needs-human-attention"}, {name: "tehas:question"}, {name: "tehas:ready"}]}' \
    >"$fx/work-7.json"
  echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"},{"name":"tehas:question"}]}]' \
    >"$fx/ready.json"
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
