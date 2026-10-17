#!/usr/bin/env bash
# Tests for libexec/improve: an issue worked on again, and the comment a recheck
# posts on the issue the loop filed.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/improve_lib.sh
. "$(dirname "$0")/improve_lib.sh"

# --- An issue worked on again ------------------------------------------------
OLD=7-261009-100000
ticket() { # <number> [<cycles line>] [<label>] [<state>]: an issue the loop filed
  jq -n -c --argjson n "$1" --arg c "${2-$OLD}" --arg l "${3-tehas:improvement}" --arg s "${4:-OPEN}" '
    {number: $n, title: "Safety net loses a file", state: $s, labels: (if $l == "" then [] else [{name: $l}] end),
     body: "The driver drops a path.\n\n**Cycles:** \($c)\n\n**Whole logs**\n"}'
}
recheck() { # [<jq object to merge in>]
  local more=${1:-}
  [ -n "$more" ] || more='{}'
  jq -n -c --arg rec "$REC" --argjson more "$more" '
    {repo: "acme/rukas", number: 12, cycle: $rec, seen: false, note: "safety net committed 3 of 3 paths"} + $more'
}
rechecks() { printf '%s\n' "$@" | jq -s '{findings: [], followups: .}' >"$fx/findings.json"; }
again() { # issue 7 was worked on again, and acme/rukas#12 rests on its earlier cycle
  ready
  with_rukas
  jq -c '. + {rukas: "0123456789abcdef0123456789abcdef01234567"}' "$base/window/$REC/cycle.json" >"$fx/c.json"
  mv "$fx/c.json" "$base/window/$REC/cycle.json"
  ticket 12 | jq -s . >"$fx/rukas-open.json"
  rechecks "$(recheck)"
}
commented() { find "$T/calls" -name 'comment-*' | wc -l; }

t_case "an issue worked on again: what the new cycle shows is a comment on the filed issue"
again
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "filed=0 issues=none rechecked=1"
t_contains "$(t_calls gh)" "issue comment 12 --repo acme/rukas"
body=$(cat "$T/calls/comment-12")
t_contains "$body" "acme/widget#7 was worked on again: cycle \`$REC\`"
t_contains "$body" 'with pi-rukas at `0123456789abcdef0123456789abcdef01234567`.'
t_contains "$body" "**Seen again:** no"
t_contains "$body" "safety net committed 3 of 3 paths"
t_contains "$body" "https://github.com/acme/factory/releases/download/tehas-improve-logs/ticket-$REC.log.gz"
t_lacks "$(t_calls gh)" "issue close"
t_lacks "$(t_calls gh)" "issue reopen"

t_case "the analyst is given the cycles each filed issue rests on"
t_eq "$(jq -c '.[] | [.number, .cycles]' "$T/calls/issues.json")" "[12,[\"$OLD\"]]"

t_case "a problem seen again says so, on a closed issue too, in the loop's own repository too"
again
echo '[]' >"$fx/rukas-open.json"
ticket 4 "$OLD, 9-261009-110000" tehas:improvement CLOSED | jq -s . >"$fx/closed.json"
rechecks "$(recheck '{"repo":"acme/factory","number":4,"seen":true}')"
t_run "$improve"
t_contains "$(t_calls gh)" "issue comment 4 --repo acme/factory"
t_contains "$(cat "$T/calls/comment-4")" "**Seen again:** yes"

t_case "the comment names the image the cycle ran when the record has one"
again
img=sha256:$(printf 'e%.0s' {1..64})
jq -c --arg i "$img" '. + {image: $i}' "$base/window/$REC/cycle.json" >"$fx/c.json"
mv "$fx/c.json" "$base/window/$REC/cycle.json"
t_run "$improve"
t_contains "$(cat "$T/calls/comment-12")" "with pi-rukas at \`0123456789abcdef0123456789abcdef01234567\`, image \`$img\`."

t_case "a record that names no pi-rukas commit: the comment says so"
again
record "$REC"
t_run "$improve"
t_contains "$(cat "$T/calls/comment-12")" "a commit the record does not name"

t_case "an open issue is not checked for a fix"
again
t_run "$improve"
t_lacks "$(t_calls gh)" "api "
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** no"

FIX=abcdefabcdefabcdefabcdefabcdefabcdefabcd
closed_again() { # issue 12 of acme/rukas closed by $FIX, worked on again, seen
  again
  echo '[]' >"$fx/rukas-open.json"
  ticket 12 "$OLD" tehas:improvement CLOSED | jq -s . >"$fx/rukas-closed.json"
  printf '%s\n' - "$FIX" >"$fx/events"
  rechecks "$(recheck '{"seen":true}')"
}

t_case "seen again in a build without the fix: the comment says not in this build"
for st in behind diverged; do
  closed_again
  echo "$st" >"$fx/compare"
  t_run "$improve"
  t_contains "$(t_calls gh)" "api --paginate repos/acme/rukas/issues/12/events"
  t_contains "$(t_calls gh)" "api repos/acme/rukas/compare/$FIX...0123456789abcdef0123456789abcdef01234567"
  body=$(cat "$T/calls/comment-12")
  t_contains "$body" "**Seen again:** not in this build: the fix \`abcdefabcdef\` is not in \`0123456789ab\`"
  t_lacks "$body" "**Seen again:** yes"
done

t_case "seen again in a build with the fix: yes, and the comment says the fix was in it"
for st in ahead identical; do
  closed_again
  echo "$st" >"$fx/compare"
  t_run "$improve"
  t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** yes, with the fix \`abcdefabcdef\` in this build"
done

t_case "the fix is checked against the runner's build commit when the record has one, not the label's"
closed_again
jq -c '. + {buildCommit: "1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"}' "$base/window/$REC/cycle.json" >"$fx/c.json"
mv "$fx/c.json" "$base/window/$REC/cycle.json"
echo behind >"$fx/compare"
t_run "$improve"
t_contains "$(t_calls gh)" "api repos/acme/rukas/compare/$FIX...1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"
t_lacks "$(t_calls gh)" "compare/$FIX...0123456789abcdef"
t_contains "$(cat "$T/calls/comment-12")" "with pi-rukas at \`1ca933e3cbbae6fff6f41d57973ba2af8ff805fd\`"
t_contains "$(cat "$T/calls/comment-12")" "not in this build: the fix \`abcdefabcdef\` is not in \`1ca933e3cbba\`"

t_case "not seen in a build without the fix says so"
closed_again
echo behind >"$fx/compare"
rechecks "$(recheck)"
t_run "$improve"
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** no, though the fix \`abcdefabcdef\` is not in \`0123456789ab\`"

t_case "a build that is not known, or no answer, says build unknown and never not in this build"
closed_again
record "$REC"
echo behind >"$fx/compare"
t_run "$improve"
t_lacks "$(t_calls gh)" "/compare/"
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** yes, build unknown: the record names no commit of acme/rukas"
closed_again
t_run "$improve"
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** yes, build unknown: GitHub did not say whether the fix"

t_case "an issue closed by hand has no fix, and one of the factory is never compared to pi-rukas"
closed_again
printf '%s\n' "$FIX" - >"$fx/events"
echo behind >"$fx/compare"
t_run "$improve"
t_lacks "$(t_calls gh)" "/compare/"
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** yes"
t_lacks "$(cat "$T/calls/comment-12")" "build"
again
echo '[]' >"$fx/rukas-open.json"
ticket 4 "$OLD" tehas:improvement CLOSED | jq -s . >"$fx/closed.json"
printf '%s\n' "$FIX" >"$fx/events"
echo behind >"$fx/compare"
rechecks "$(recheck '{"repo":"acme/factory","number":4,"seen":true}')"
t_run "$improve"
t_lacks "$(t_calls gh)" "/compare/"
t_contains "$(cat "$T/calls/comment-4")" "**Seen again:** yes, build unknown: the record names no commit of acme/factory"

t_case "no comment on an issue the loop did not file"
again
ticket 12 "$OLD" "" | jq -s . >"$fx/rukas-open.json"
t_run "$improve"
t_eq "$(commented)" 0
t_contains "$t_out" "rechecked=0"

t_case "no comment on a filed issue that rests on no cycle of the same product issue"
for c in "9-261009-100000" "$REC" "77-261009-100000" ""; do
  again
  ticket 12 "$c" | jq -s . >"$fx/rukas-open.json"
  t_run "$improve"
  t_eq "$(commented)" 0
done

t_case "rechecks of the wrong shape are dropped one by one"
again
rechecks "$(recheck '{"number":13}')" "$(recheck '{"repo":"acme/other"}')" "$(recheck '{"cycle":"7-261011-000000"}')" \
  "$(recheck '{"seen":"no"}')" "$(recheck '{"note":""}')" "$(recheck "{\"note\":\"$(printf 'x%.0s' $(seq 1 1501))\"}")" \
  "$(recheck '{"note":"a\u0007b"}')" '"a string"'
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(commented)" 0

t_case "followups that are not a list are no rechecks, and the findings are still filed"
again
jq -n --argjson f "$(finding "The gate runs twice")" '{findings: [$f], followups: "all of them"}' >"$fx/findings.json"
t_run "$improve"
t_contains "$t_out" "filed=1 issues=#101 rechecked=0"

t_case "the same recheck twice is one comment"
again
rechecks "$(recheck)" "$(recheck '{"note":"again"}')"
t_run "$improve"
t_eq "$(t_calls gh | grep -c '^issue comment ')" 1

t_case "at most ten rechecks a run"
again
echo '[]' >"$fx/rukas-open.json"
for i in $(seq 1 12); do ticket "$i" "$OLD" tehas:improvement CLOSED; done | jq -s . >"$fx/rukas-closed.json"
for i in $(seq 1 12); do recheck "{\"number\":$i}"; done | jq -s '{findings: [], followups: .}' >"$fx/findings.json"
t_run "$improve"
t_eq "$(t_calls gh | grep -c '^issue comment ')" 10

t_case "a mention in a recheck does not reach the comment"
again
rechecks "$(recheck '{"note":"thanks @alice <!-- hidden -->"}')"
t_run "$improve"
t_lacks "$(cat "$T/calls/comment-12")" "@alice"
t_contains "$(cat "$T/calls/comment-12")" "&lt;!-- hidden"

t_case "GitHub refuses the comment: the run finishes and the line says so"
again
: >"$fx/gh-comment.rc"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "rechecked=0 not-commented=acme/rukas#12"
t_eq "$(find "$base" -mindepth 1 ! -name window | wc -l)" 0

t_done
