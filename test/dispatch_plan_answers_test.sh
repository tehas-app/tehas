#!/usr/bin/env bash
# Tests for tehasbot dispatch: the answers a poll serves, the session id a
# turn's file carries, and the analytics of a planning turn.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "a session id that is not a string in a failed turn's file is no session"
to_plan
echo '{"session":{"a":1}}' >"$fx/plan-fails"
t_run "$dispatch"
t_contains "$(t_calls gh)" "could not start"
t_eq "$(rec_open)" none

t_case "a first turn whose session id is longer than the store keeps is no session, and nothing is asked"
to_plan
jq -n --arg s "$(printf 'a%.0s' {1..200})" '{session: $s, text: "Two ways.", question: {question: "Adopt?", options: [{label: "Adopt all"}]}}' >"$fx/plan-result.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "could not start"
t_lacks "$(t_calls gh)" "kind=plan issue=7"
t_eq "$(rec_open)" none

t_case "line ends in what a turn names are no words of the store's command: the bad address is not filed"
to_plan
jq -n '{session: "abc-123", text: "Two ways.",
  question: {question: "Adopt?", options: [{label: "Adopt\nall --filed x"}, {label: "Change some"}]},
  filed: ["https://github.com/acme\nwidget/issues/41", "https://github.com/acme/widget/issues/42\n",
    "https://github.com/acme/widget/issues/43"]}' >"$fx/plan-result.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(rec | jq -c '[.session, .turns, .running, .options, .filed]')" '["abc-123",1,false,["Adopt all --filed x","Change some"],["https://github.com/acme/widget/issues/43"]]'
t_eq "$([ -e "$record.rc" ] && echo kept)" ""

t_case "an answered question is served before a new plan label and before a ready issue"
planning
comments 7 "$(plan_body x ' ' ' ')" alice
echo '[{"number":8,"labels":[{"name":"tehas:plan"}]}]' >"$fx/plans.json"
echo '[{"number":9,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_contains "$(plan_calls)" "7 --out"
t_lacks "$(plan_calls)" "8 --out"
t_eq "$(t_calls fake-start)" ""

# One issue is in one flow at a time.
t_case "tehas:ready on an issue with an open planning session starts no cycle"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" ""
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]}]' >"$fx/ready.json"
before=$(rec)
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"
t_eq "$(rec)" "$before"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"
t_lacks "$t_out" "would run a cycle"

t_case "tehas:ready on an issue whose planning turn is marked as running starts no cycle"
reset
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:planning"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "skip #7: a planning session is open on it; end it first"

t_case "an issue with a planning session does not hold up the ready issue behind it"
planning
comments 7 "$(plan_body ' ' ' ' ' ')" ""
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"tehas:question"}]},{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
cp "$fx/events-7.json" "$fx/events-8.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "8"
rm -f "$fx/events-8.json"

# The analytics program, where install.sh puts it. A call notes the record as
# the program reads it: its facts.
plan_analytics() { # [<bash body>]
  t_stub_analytics '[ "$1 $3" != "plan --facts" ] || cp "$4" "$T/calls/record.at-analytics" 2>/dev/null
'"${1:-}"
}
# The calls, without the facts file's name, which is new each time.
plan_analytics_calls() { t_calls analytics | grep '^plan' | sed 's/ --facts [^ ]*//' || true; }

t_case "a planning turn that ends is recorded in analytics, from its record before it is rewritten"
to_plan
rm -f "$fx/image.label"
plan_analytics
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_analytics_calls)" "plan 7 --rc 0"
t_eq "$(jq -c '[.running, .turns, (.startedAt | type), .by, .label]' "$T/calls/record.at-analytics")" '[true,0,"string","alice","tehas:plan"]'
t_eq "$(rec | jq -c .turns)" 1

t_case "with the pi-rukas commit of the image's label"
to_plan
echo 'abcdef1|1' >"$fx/image.label"
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc 0 --rukas-image abcdef1"
rm -f "$fx/image.label"

t_case "with the image's id and when it was built"
to_plan
echo "abcdef1|1|sha256:$(printf 'e%.0s' {1..64})|2026-10-29T14:44:16.5Z" >"$fx/image.label"
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc 0 --rukas-image abcdef1 --image sha256:$(printf 'e%.0s' {1..64}) --image-built 2026-10-29T14:44:16.5Z"
t_case "the image is the one the turn started in, not the one the image name points at as it ends"
to_plan
echo "abcdef1|1|sha256:$(printf 'e%.0s' {1..64})|2026-10-29T14:44:16.5Z" >"$fx/image.label"
echo "1234567|1|sha256:$(printf 'f%.0s' {1..64})|2026-10-29T15:00:00Z" >"$fx/image.during"
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc 0 --rukas-image abcdef1 --image sha256:$(printf 'e%.0s' {1..64}) --image-built 2026-10-29T14:44:16.5Z"
t_eq "$(find "$T/home/tehas/state" -name 'plan-7.json.image' | wc -l)" 0
t_case "an image id of another shape is left out, and so is its time"
to_plan
echo "abcdef1|1|sha256:short|2026-10-29T14:44:16.5Z" >"$fx/image.label"
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc 0 --rukas-image abcdef1"
rm -f "$fx/image.label"

t_case "a turn that fails and a turn cut off are recorded with how they ended"
to_plan
: >"$fx/plan-fails"
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc 2"
rm -f "$fx/plan-fails"
planning
t_plan "$T/home/tehas" 7 '{"turns":1,"options":["Adopt all","Change some"],"running":true,"startedAt":"2026-01-01T00:00:00Z"}'
plan_analytics
t_run "$dispatch"
t_eq "$(plan_analytics_calls)" "plan 7 --rc interrupted"

t_case "with TEHAS_ANALYTICS off a planning turn is not recorded"
to_plan
plan_analytics
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls analytics)" ""
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:question"

t_case "a failing or hanging analytics changes nothing in the turn"
to_plan
# The start comment names the minute: two runs either side of one differ
# only there.
gh_calls() { t_calls gh | sed -E 's/ at [0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2} UTC/ at <time> UTC/'; }
t_run "$dispatch"
without_gh=$(gh_calls)
without_record=$(rec | jq -c 'del(.startedAt)')
for body in 'exit 70' 'exec sleep 30'; do
  to_plan
  plan_analytics "[ \"\$1\" != plan ] || $body"
  T_ENV+=(TEHAS_ANALYTICS_WAIT=1)
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_contains "$t_out" "WARN analytics plan: exit"
  t_eq "$(gh_calls)" "$without_gh"
  t_eq "$(rec | jq -c 'del(.startedAt)')" "$without_record"
done

t_done
