#!/usr/bin/env bash
# Tests for libexec/dispatch: a cycle parked as underspecified or too-large is
# refined once on its own (rule 5a), and what a refine that files does to the
# original issue and the filed one.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

home=$T/home/tehas

# Issue 7's cycle was started on <who>'s tehas:ready of <body> and parked at
# explore as intent-park, its question 4001 on record and unanswered. The text
# has not been edited since. Nothing else is queued.
refine_park() { # [<park reason>] [<one more label>] [<body>] [<who>]
  local reason=${1:-underspecified} extra=${2:-} body=${3:-Make it good.} labels
  reset
  echo '[]' >"$fx/ready.json"
  labels=$(printf '%s\n' needs-human-attention tehas:question "$extra" | jq -Rs 'split("\n") | map(select(. != "") | {name: .})')
  jq -n --argjson l "$labels" '[{number: 7, labels: $l}]' >"$fx/asking.json"
  jq -n --argjson l "$labels" --arg b "$body" '{title: "Faster search", body: $b, state: "OPEN", labels: $l}' >"$fx/issue-7.json"
  t_approve "$home" 7 "${4:-alice}" label 1700000001 --body-sha "$(printf '%s' "$body" | sha256sum | cut -d' ' -f1)"
  t_started "$home" 7
  TEHAS_HOME=$home flock -w 5 "$home/state/cycles/7.lock" "$T_STORE" cycle-end 7 --attempt 1 --status parked \
    --cap intent-park --park-reason "$reason" --parked >/dev/null || t_bad "refine_park: the store refused the end"
  t_park "$home" 7 intent-park 4001
  comments 7 "$(park_body ' ' ' ')" ""
}
new_poll() { rm -f "$T"/calls/*; }
spent() { t_row park-show 7 -- '.park.retries'; }
refused() { # <what the log says>: the poll started nothing and spent the refine
  t_eq "$t_rc" 0
  t_eq "$(plan_calls)" ""
  t_eq "$(spent)" 1
  t_contains "$t_out" "skip #7: not refined: $1"
  t_eq "$(rec_open)" none
}

t_case "a cycle parked as underspecified is refined with the refine brief, on the word of its starter"
refine_park
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $keep --brief $home/tmp/plan-7.brief."
t_eq "$(rec_brief | head -n 3)" '# Faster search

Make it good.'
t_contains "$(rec_brief)" "$(cat "$T_ROOT/lib/refine-brief.md")"
t_eq "$(rec_brief | tail -n 1)" "This request is issue #7 itself. The issue you file replaces #7: #7 is not a duplicate of this plan."
t_contains "$t_out" "start refining #7 (its cycle parked as underspecified, started by alice)"
t_eq "$(t_row plan-show 7 -- '[.planning.by, .planning.label, .planning.auto] | tostring')" '["alice","tehas:refine",true]'
t_contains "$(t_calls gh)" "its cycle parked as \`underspecified\`, and the start of that cycle by @alice authorizes one refine."
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:refine --remove-label tehas:question --remove-label needs-human-attention"
t_eq "$(spent)" 1
# The session's start took the park's question off the record.
t_eq "$(park_q)" none
t_eq "$(t_row history-show 7 --kind plan -- '.rows[-1] | [.label, .auto] | tostring')" '["tehas:refine",true]'

t_case "a cycle parked as too large is refined, and the comment names the reason"
refine_park too-large
t_run "$dispatch"
t_contains "$(t_calls gh)" "its cycle parked as \`too-large\`"

t_case "the next poll starts no second session"
refine_park
t_run "$dispatch"
new_poll
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$t_out" "start refining #7"

t_case "a park of another reason is not refined"
refine_park premise-unsound
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$t_out" "refin"

t_case "a closed issue is not refined"
refine_park
jq '.state = "CLOSED"' "$fx/issue-7.json" >"$fx/i" && mv "$fx/i" "$fx/issue-7.json"
t_run "$dispatch"
refused "it is closed"

for l in tehas:blocked tehas:refined tehas:replaced tehas:running; do
  t_case "an issue that carries $l is not refined"
  refine_park underspecified "$l"
  t_run "$dispatch"
  refused "it carries $l"
done

t_case "an issue whose park question was answered is not refined"
refine_park
TEHAS_HOME=$home "$T_STORE" question-answered 7 --id 4001 --by alice >/dev/null
t_run "$dispatch"
refused "its park question was answered"

t_case "an issue that had a planning session before is not refined"
refine_park
t_plan_end "$home" 7 1700000100 '{"status":"not-started"}'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$t_out" "refin"

t_case "an issue a planning session filed is not refined"
refine_park underspecified "" "Replaces #3

Make it good."
t_run "$dispatch"
refused "a planning session filed it"

t_case "a sub-issue of a person's epic is refined: a parent alone is no sign of a refine"
refine_park underspecified "" "Part of #3.

Make it good."
t_run "$dispatch"
t_contains "$t_out" "start refining #7"

t_case "a cycle started by an account that may no longer label is not refined"
refine_park underspecified "" "Make it good." mallory
t_run "$dispatch"
refused "its cycle was not started by an account that may label (mallory)"

t_case "an issue whose text another account changed since its cycle began is not refined"
refine_park
jq -n '{data: {repository: {issue: {lastEditedAt: "2099-01-02T00:00:00Z", editor: {login: "mallory"}, body: "Make it good.", title: "Faster search"}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
refused "its text changed since its cycle began"

t_case "an allowed account's edit since the park stops the refine too: a person is working on the text"
refine_park
jq '.body = "Make it fast: 50 ms."' "$fx/issue-7.json" >"$fx/i" && mv "$fx/i" "$fx/issue-7.json"
t_run "$dispatch"
refused "its text changed since its cycle began"

t_case "a Retry ticked while no slot is free is not overridden by the refine"
refine_park
comments 7 "$(park_body x ' ')" alice
hold 9
t_run "$dispatch"
release
t_eq "$(plan_calls)" ""
t_lacks "$t_out" "start refining #7"
t_contains "$t_out" "skip #7: not refined: a box of its park question is ticked"
t_eq "$(park_q)" 4001
t_eq "$(spent)" 0
t_lacks "$(t_calls gh)" "--add-label tehas:planning"

t_case "a refine whose first label change fails opens no session and is not tried again"
refine_park
echo "--add-label tehas:planning" >"$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "could not mark #7 as planning"
t_eq "$(rec_open)" none
t_eq "$(spent)" 1
new_poll
rm -f "$fx/edit-fails-on"
t_run "$dispatch"
t_eq "$(plan_calls)" ""

t_case "a paused factory refines nothing, and refines once it is resumed"
refine_park
pause_flag '{"at":"2026-01-01T00:00:00Z","by":"alice","reason":"test"}'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_eq "$(spent)" 0
rm -f "$home/state/pause.json"
new_poll
t_run "$dispatch"
t_contains "$t_out" "start refining #7"

t_case "a turn of another issue in flight leaves the refine for a later poll"
refine_park
t_plan "$home" 9 '{"session":"xyz","turns":1,"running":true}'
echo 9 >"$fx/plan-runs"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "refining #7 waits: the turn of #9 is still running"
t_eq "$(spent)" 0

t_case "a dry run says what it would do and changes nothing"
refine_park
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would start refining #7 (its cycle parked as underspecified, started by alice)"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(spent)" 0
t_eq "$(park_q)" 4001

t_case "a dry run of a refused refine spends nothing"
refine_park underspecified "" "Make it good." mallory
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: not refined"
t_eq "$(spent)" 0

# --- A refine that files --------------------------------------------------------
# Issue 7 carries alice's tehas:refine, and the session's first turn files
# issue 12 (and whatever else is named), as libexec/plan reports a confirm.
filed_by_refine() { # <body of #12> <url>...
  to_refine
  jq -cn --arg b "$1" '{body: $b}' >"$fx/issue-12.json"
  echo '{"number":12,"id":5012,"state":"open"}' >"$fx/work-12.json"
  jq -cn --args '{session: "abc-123", text: "Filed \($ARGS.positional[0]): Faster search, part 1.", question: null,
    filed: $ARGS.positional}' "${@:2}" >"$fx/plan-result.json"
}
one_part='Search answers in 50 ms.

## Deliverables
1. ...

## Later parts
None.'
more_parts='Search answers in 50 ms.

## Later parts
- Cache the index
- Warm it on start

## Evidence
x - lib/a.sh:1'

t_case "a refine that files one issue links both ways and closes the original as replaced"
filed_by_refine "$one_part" https://github.com/acme/widget/issues/12
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 12 --repo acme/widget --body Replaces #7

Search answers in 50 ms."
t_contains "$(t_calls gh)" "api -X POST repos/acme/widget/issues/7/sub_issues -F sub_issue_id=5012"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Replaced by #12.

Filed https://github.com/acme/widget/issues/12: Faster search, part 1."
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question --remove-label tehas:planning --add-label tehas:replaced"
t_contains "$(t_calls gh)" "issue close 7 --repo acme/widget --reason not planned"
t_lacks "$(t_calls gh)" "tehas:question kind=plan"
t_lacks "$(t_calls gh)" "--add-label tehas:question"
t_eq "$(rec_open)" none
t_eq "$([ -e "$record.out" ] && echo kept)" ""
t_contains "$t_out" "planning #7 ended: tehas:replaced by https://github.com/acme/widget/issues/12"

t_case "a refine whose plan lists later parts keeps the original open as refined"
filed_by_refine "$more_parts" https://github.com/acme/widget/issues/12
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:refined"
t_lacks "$(t_calls gh)" "tehas:replaced"
t_lacks "$(t_calls gh)" "issue close"
t_eq "$(rec_open)" none

t_case "a filed body that cannot be read keeps the original open"
filed_by_refine "x" https://github.com/acme/widget/issues/12
echo '{"title":"no body here"' >"$fx/issue-12.json"
t_run "$dispatch"
t_contains "$t_out" "WARN: could not read #12, which refining #7 filed"
t_contains "$(t_calls gh)" "--add-label tehas:refined"
t_lacks "$(t_calls gh)" "issue close"

t_case "a filed body that already names the original is not written again"
filed_by_refine "Replaces #7

$one_part" https://github.com/acme/widget/issues/12
t_run "$dispatch"
t_lacks "$(t_calls gh)" "issue edit 12"
t_contains "$(t_calls gh)" "issue close 7"

t_case "a sub-issue link that fails is a warning, and the rest goes on"
filed_by_refine "$one_part" https://github.com/acme/widget/issues/12
rm -f "$fx/work-12.json"
: >"$fx/work-12-fails"
t_run "$dispatch"
t_contains "$t_out" "WARN: could not link #12 as a sub-issue of #7"
t_contains "$(t_calls gh)" "issue close 7"

t_case "a label change that fails is repeated by the next poll, and Replaced by is said once"
filed_by_refine "$one_part" https://github.com/acme/widget/issues/12
echo "--add-label tehas:replaced" >"$fx/edit-fails-on"
t_run "$dispatch"
t_contains "$t_out" "could not mark #7 tehas:replaced; the next poll replaces it"
t_lacks "$(t_calls gh)" "Replaced by"
t_lacks "$(t_calls gh)" "issue close"
t_eq "$(rec_open)" open
new_poll
rm -f "$fx/edit-fails-on"
echo '[]' >"$fx/refines.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "--add-label tehas:replaced"
t_eq "$(t_calls gh | grep -c 'Replaced by')" 1
t_contains "$(t_calls gh)" "issue close 7"
t_eq "$(rec_open)" none

t_case "an original that carries the mark already is only closed: nothing is said or linked again"
filed_by_refine "$one_part" https://github.com/acme/widget/issues/12
echo '{"title":"Faster search","body":"Make it fast.","labels":[{"name":"tehas:replaced"}]}' >"$fx/issue-7.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue close 7"
t_lacks "$(t_calls gh)" "issue edit 12"
t_lacks "$(t_calls gh)" "sub_issues"
t_lacks "$(t_calls gh)" "Replaced by"
t_eq "$(rec_open)" none

t_case "a filed address that is not an issue address is left out, and the session still ends"
filed_by_refine "$one_part" https://github.com/acme/widget/issues/12 https://github.com/acme/widget/issues/12/ "https://github.com/acme/widget/issues/13#x" "@mallory"
t_run "$dispatch"
t_contains "$(t_calls gh)" "Replaced by #12."
t_lacks "$(t_calls gh)" "mallory"
t_lacks "$(t_calls gh)" "issue view 13"
t_contains "$(t_calls gh)" "issue close 7"
t_eq "$(rec_open)" none

t_case "a filed issue of another repository gets no links and is named; the original stays open"
filed_by_refine "$one_part" https://github.com/acme/widget/issues/12 https://github.com/other/repo/issues/3
t_run "$dispatch"
t_contains "$(t_calls gh)" "Replaced by #12, https://github.com/other/repo/issues/3."
t_lacks "$(t_calls gh)" "issue edit 3"
t_eq "$(grep -c 'sub_issues' <<<"$(t_calls gh)")" 1
t_contains "$(t_calls gh)" "--add-label tehas:refined"

t_case "the repository is compared without its case"
filed_by_refine "$one_part" https://github.com/Acme/Widget/issues/12
t_run "$dispatch"
t_contains "$(t_calls gh)" "Replaced by #12."
t_contains "$(t_calls gh)" "issue close 7"

t_case "a plan session that files is posted as before"
to_plan
jq -cn '{session: "abc-123", text: "Filed https://github.com/acme/widget/issues/12.", question: null,
  filed: ["https://github.com/acme/widget/issues/12"]}' >"$fx/plan-result.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "Filed in this turn: https://github.com/acme/widget/issues/12"
t_lacks "$(t_calls gh)" "Replaced by"
t_lacks "$(t_calls gh)" "issue close"

t_case "a refine turn that filed nothing asks as before"
to_refine
t_run "$dispatch"
t_contains "$(t_calls gh)" "<!-- tehas:question kind=plan issue=7 turn=1 -->"
t_lacks "$(t_calls gh)" "Replaced by"

t_case "a plan has no later parts only when its Later parts section says none"
none_later() { (. "$T_ROOT/lib/planning.sh" && printf '%b' "$1" | plan_no_later_parts && echo yes || echo no); }
t_eq "$(none_later 'No later parts here.\n## Later parts\n\n- none\n')" yes
t_eq "$(none_later '## Later parts\nNone.\n## Evidence\nx - a:1\n')" yes
t_eq "$(none_later '## Spec\nLater parts: none\n')" yes
t_eq "$(none_later '### Later parts\n1. Next\n## Evidence\n')" no
t_eq "$(none_later '## Later Parts\n- Cache it\n')" no
t_eq "$(none_later '**Later parts**\n- Cache it\n')" no
t_eq "$(none_later '## Later parts:\n- Cache it\n')" no
t_eq "$(none_later '### Later parts (not this issue)\n- Cache it\n')" no
t_eq "$(none_later 'Later parts: cache it\n')" no
t_eq "$(none_later '## Deliverables\n1. All of it\n')" no

t_done
