#!/usr/bin/env bash
# Tests for libexec/dispatch: a cycle parked as underspecified or too-large is
# refined once on its own (rule 5a), and what a refine that files does to the
# original issue and the filed one.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

cycles_dir=$T/home/tehas/state/cycles
park_file=$cycles_dir/7.park
history_file=$cycles_dir/plan-7.history.jsonl

# Issue 7 was started by alice's tehas:ready, and its cycle parked at explore:
# intent-park, marked to be refined, its question on record and unanswered.
# The text has not been edited since. Nothing else is queued.
refine_park() { # [<park reason>] [<labels besides the park's, space separated>] [<body>]
  local reason=${1:-underspecified} extra=${2:-} body=${3:-Make it good.} labels
  parked
  labels=$(printf '%s\n' needs-human-attention tehas:question $extra | jq -Rs 'split("\n") | map(select(. != "") | {name: .})')
  jq -n --argjson l "$labels" '[{number: 7, labels: $l}]' >"$fx/asking.json"
  jq -n --argjson l "$labels" --arg b "$body" '{title: "Faster search", body: $b, state: "OPEN", labels: $l}' >"$fx/issue-7.json"
  echo '[]' >"$fx/plans.json"
  echo '[]' >"$fx/refines.json"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-01T00:00:00Z"}]' >"$fx/events-7.json"
  jq -n --arg b "$body" '{data: {repository: {issue: {lastEditedAt: null, editor: null, body: $b, title: "Faster search"}}}}' >"$fx/edit-7.json"
  comments 7 "$(park_body ' ' ' ')" ""
  mkdir -p "$cycles_dir"
  echo alice >"$cycles_dir/7.by"
  jq -cn --arg r "$reason" '{cap: "intent-park", reason: "", park_reason: $r, provider: false, retry: false,
    refine: true, started: 1767225600}' >"$park_file"
}
new_poll() { rm -f "$T"/calls/*; }
refused() { # <what the log says>: the poll started nothing and dropped the mark
  t_eq "$t_rc" 0
  t_eq "$(plan_calls)" ""
  t_eq "$(jq -r .refine "$park_file")" false
  t_contains "$t_out" "skip #7: not refined: $1"
  t_eq "$([ -e "$record" ] && echo kept)" ""
}

t_case "a cycle parked as underspecified is refined with the refine brief, on the approval of its starter"
refine_park
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --brief $record.brief"
t_eq "$(head -n 3 "$record.brief")" '# Faster search

Make it good.'
t_contains "$(cat "$record.brief")" "$(cat "$T_ROOT/lib/refine-brief.md")"
t_eq "$(tail -n 1 "$record.brief")" "This request is issue #7 itself. The issue you file replaces #7: #7 is not a duplicate of this plan."
t_contains "$t_out" "start refining #7 (its cycle parked as underspecified, approved by alice)"
t_eq "$(jq -c '[.by, .label, .auto]' "$record")" '["alice","tehas:refine",true]'
t_contains "$(t_calls gh)" "its cycle parked as \`underspecified\`, and @alice's \`tehas:ready\` authorizes one refine."
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:planning --remove-label tehas:question --remove-label needs-human-attention"
t_lacks "$(t_calls gh)" "--remove-label tehas:refine"
t_eq "$(jq -r .refine "$park_file")" false
t_eq "$([ -e "$park_id" ] && echo kept)" ""
t_eq "$(tail -n 1 "$history_file" | jq -c '[.label, .auto]')" '["tehas:refine",true]'

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

t_case "a park not marked, or marked false, is not refined"
refine_park
jq -c '.refine = false' "$park_file" >"$park_file.new" && mv "$park_file.new" "$park_file"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$t_out" "refin"
refine_park
jq -c 'del(.refine)' "$park_file" >"$park_file.new" && mv "$park_file.new" "$park_file"
t_run "$dispatch"
t_eq "$(plan_calls)" ""

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
rm -f "$park_id"
t_run "$dispatch"
refused "its park question was answered"

t_case "an issue that had a planning session before is not refined"
refine_park
echo '{"kind":"plan","label":"tehas:plan"}' >"$history_file"
t_run "$dispatch"
refused "it has had a planning session"

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

t_case "a cycle started by an account that may not label is not refined"
refine_park
echo mallory >"$cycles_dir/7.by"
t_run "$dispatch"
refused "its cycle was not started by an account that may label (mallory)"

t_case "a cycle with no starter on record is not refined"
refine_park
rm -f "$cycles_dir/7.by"
t_run "$dispatch"
refused "its cycle was not started by an account that may label (unknown)"

t_case "a cycle the fixer started is not refined"
refine_park
t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=fixbot
echo fixbot >"$cycles_dir/7.by"
t_run "$dispatch"
refused "its cycle was not started by an account that may label (fixbot)"

t_case "an issue whose text another account changed since its cycle began is not refined"
refine_park
jq -n '{data: {repository: {issue: {lastEditedAt: "2026-01-02T00:00:00Z", editor: {login: "mallory"}, body: "Other.", title: "Faster search"}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
refused "its text changed since its cycle began"

t_case "a paused factory keeps the mark, and refines once it is resumed"
refine_park
pause_flag '{"at":"2026-01-01T00:00:00Z","by":"alice","reason":"test"}'
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_eq "$(jq -r .refine "$park_file")" true
rm -f "$T/home/tehas/state/pause.json"
new_poll
t_run "$dispatch"
t_contains "$t_out" "start refining #7"

t_case "a turn of another issue in flight keeps the mark for a later poll"
refine_park
mkdir -p "$T/home/tehas/state"
echo '{"session":"s","turns":1,"running":true}' >"$T/home/tehas/state/plan-9.json"
exec 7>"$T/home/tehas/state/plan-9.json.lock"
flock -n 7
t_run "$dispatch"
exec 7>&-
t_eq "$(plan_calls)" ""
t_eq "$(jq -r .refine "$park_file")" true
rm -f "$T/home/tehas/state/plan-9.json" "$T/home/tehas/state/plan-9.json.lock"

t_case "a dry run says what it would do and changes nothing"
refine_park
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would start refining #7 (its cycle parked as underspecified, approved by alice)"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(jq -r .refine "$park_file")" true
t_eq "$([ -e "$park_id" ] && echo kept)" kept

t_case "a dry run of a refused mark keeps the mark"
refine_park
echo mallory >"$cycles_dir/7.by"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: not refined"
t_eq "$(jq -r .refine "$park_file")" true

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
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Replaced by #12. Filed https://github.com/acme/widget/issues/12: Faster search, part 1."
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:question --remove-label tehas:planning --add-label tehas:replaced"
t_contains "$(t_calls gh)" "issue close 7 --repo acme/widget --reason not planned"
t_lacks "$(t_calls gh)" "tehas:question kind=plan"
t_lacks "$(t_calls gh)" "--add-label tehas:question"
t_eq "$([ -e "$record" ] || [ -e "$record.out" ] && echo kept)" ""
t_contains "$t_out" "planning #7 ended: replaced by #12"

t_case "a refine whose plan lists later parts keeps the original open as refined"
filed_by_refine "$more_parts" https://github.com/acme/widget/issues/12
t_run "$dispatch"
t_contains "$(t_calls gh)" "--remove-label tehas:planning --add-label tehas:refined"
t_lacks "$(t_calls gh)" "tehas:replaced"
t_lacks "$(t_calls gh)" "issue close"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "a filed body that cannot be read keeps the original open"
filed_by_refine "x" https://github.com/acme/widget/issues/12
echo '{"body":null}' >"$fx/issue-12.json"
t_run "$dispatch"
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

t_case "a close that fails is repeated by the next poll, and nothing is said twice"
filed_by_refine "$one_part" https://github.com/acme/widget/issues/12
: >"$fx/close-fails"
t_run "$dispatch"
t_contains "$t_out" "WARN: could not close #7"
t_eq "$(jq -r .pending "$record")" true
rm -f "$T"/calls/* "$fx/close-fails"
echo '[]' >"$fx/refines.json"
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue close 7"
t_lacks "$(t_calls gh)" "issue edit 12"
t_lacks "$(t_calls gh)" "sub_issues"
t_lacks "$(t_calls gh)" "Replaced by"
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "a filed issue of another repository gets no links and is named"
filed_by_refine "$one_part" https://github.com/acme/widget/issues/12 https://github.com/other/repo/issues/3
t_run "$dispatch"
t_contains "$(t_calls gh)" "Replaced by #12, https://github.com/other/repo/issues/3."
t_lacks "$(t_calls gh)" "issue edit 3"
t_eq "$(grep -c 'sub_issues' <<<"$(t_calls gh)")" 1

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

t_case "later parts are read from the heading, not from a mention"
t_eq "$(. "$T_ROOT/lib/planning.sh"; printf 'No later parts here.\n## Later parts\n\n- none\n' | plan_later_parts && echo yes || echo no)" no
t_eq "$(. "$T_ROOT/lib/planning.sh"; printf '### Later parts\n1. Next\n## Evidence\n' | plan_later_parts && echo yes || echo no)" yes
t_eq "$(. "$T_ROOT/lib/planning.sh"; printf '## Spec\nLater parts: none\n' | plan_later_parts && echo yes || echo no)" no

t_done
