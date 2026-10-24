#!/usr/bin/env bash
# Tests for lib/shipped.sh itself: the three statuses tehas_shipped_answer
# returns, the ceiling the caller must not pass, and the two commands that read
# the query and the rule from here. The tidy cases reach the helper through
# libexec/tidy only; these pin the contract beside the library, so a change to
# the line between "no answer" and "answered, not a merge" fails here even where
# no caller's case notices.
# shellcheck disable=SC2016  # the jq programs and the GraphQL text are literal
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

. "$T_ROOT/lib/shipped.sh"

# One answer of the batched search, holding the nodes the case names. The
# helpers are exported, and the number is passed through a variable and not a
# literal: t_run runs each case with env -i, so a function of this file reaches
# the case only as an exported one.
# shellcheck disable=SC2016  # $1 and $2 are the helper's own arguments
# shellcheck disable=SC2154  # tehas_shipped_page comes from the sourced library
export -f tehas_shipped_answer tehas_shipped_rule tehas_shipped_query

answer() { # <node>,...
  local IFS=,
  printf '{"data":{"search":{"nodes":[%s]}}}' "$*"
}
node() { # <number> <closer>
  printf '{"number":%s,"timelineItems":{"nodes":[{"closer":%s}]}}' "$1" "$2"
}
export -f answer node
ask() { # <answer> <number>: the status the helper gives, one word
  # lib.sh sets pipefail, so a non-zero status from the helper would fail this
  # function before the word is printed: the status is data here, not a result.
  local rc=0
  tehas_shipped_answer "$2" "$1" || rc=$?
  printf 'rc=%s' "$rc"
}
export -f ask

t_case "a merged closer is 0, an unmerged one is 2, a person's close is 2"
t_eq "$(ask "$(answer "$(node 7 '{"merged":true}')")" 7)" rc=0
t_eq "$(ask "$(answer "$(node 7 '{"merged":false}')")" 7)" rc=2
t_eq "$(ask "$(answer "$(node 7 'null')")" 7)" rc=2
t_eq "$(ask "$(answer "$(node 7 '{"merged":"true"}')")" 7)" rc=2
t_eq "$(ask "$(answer "$(node 7 '{"merged":1}')")" 7)" rc=2
t_eq "$(ask "$(answer "$(node 7 '{"merged":{"state":"merged"}}')")" 7)" rc=2
# A node that names no timeline at all is an issue the read spoke of and whose
# closer it did not see: a closed issue, not a failed read.
t_eq "$(ask '{"data":{"search":{"nodes":[{"number":7}]}}}' 7)" rc=2

t_case "a read that spoke of another issue is no answer: 1, not 2"
both=$(answer "$(node 7 '{"merged":true}')" "$(node 8 '{"merged":false}')")
t_eq "$(ask "$both" 7)" rc=0
t_eq "$(ask "$both" 8)" rc=2
# The issue asked for is in no node of the answer at all: the read never named
# it, which is not the same as naming it and finding no merge.
t_eq "$(ask "$both" 9)" rc=1
t_eq "$(ask '{"data":{"search":{"nodes":[]}}}' 9)" rc=1
t_eq "$(ask '{}' 9)" rc=1

t_case "a read that carries errors is no answer even where it holds a merged closer"
# The four answers are built here and not in the assertions: t_eq's own double
# quotes would end at the first quote of a JSON literal written inside $(...).
merged_nodes='[{"number":7,"timelineItems":{"nodes":[{"closer":{"merged":true}}]}}]'
errors_answer='{"errors":[{"message":"declined"}],"data":{"search":{"nodes":'"$merged_nodes"'}}}'
null_errors='{"errors":null,"data":{"search":{"nodes":'"$merged_nodes"'}}}'
empty_errors='{"errors":[],"data":{"search":{"nodes":'"$merged_nodes"'}}}'
t_eq "$(ask "$errors_answer" 7)" rc=1
t_eq "$(ask '{"errors":[{"message":"declined"}]}' 7)" rc=1
# An errors key that is null or an empty list is no refusal: the data answers.
t_eq "$(ask "$null_errors" 7)" rc=0
t_eq "$(ask "$empty_errors" 7)" rc=0

t_case "an answer whose nodes are no list is no answer"
for shape in '{"data":{"search":{"nodes":null}}}' '{"data":{"search":{"nodes":{}}}}' \
  '{"data":{"search":{"nodes":"7"}}}' '{"data":null}' '{"data":{}}' 'not json' '' '[]'; do
  t_eq "$(ask "$shape" 7)" rc=1
done

t_case "an issue number is digits and nothing else"
merged_one=$(answer "$(node 42 '{"merged":true}')")
# Each form is passed through a variable: the harness's env -i would drop a
# literal's leading zero, and the case is about the text the helper is handed.
for form in "" " 42" "42.0" "4.2e1" "1e2" "-42" "+42" "42x" "0x2a" "42;" "42 43" \
  "18446744073709551616" "9007199254740993" "42 42" "4_2"; do
  t_eq "$(ask "$merged_one" "$form")" rc=1
done
# Digits are digits, however written: jq's tonumber turns them into the number
# GitHub named, so a padded form answers for the same issue as a plain one.
for form in "042" "000042"; do
  t_eq "$(ask "$merged_one" "$form")" rc=0
done
# The plain form still answers, so the gate above is a gate and not a refusal.
t_eq "$(ask "$merged_one" 42)" rc=0

t_case "the page is one number, below the ceiling of a search, and reaches the query"
# Read from the library rather than taken as read: a constant the caller set on
# its own would be a second statement of the ceiling.
page=$(bash -c ". '$T_ROOT/lib/shipped.sh'; printf '%s' \"\$tehas_shipped_page\"")
[[ $page =~ ^[1-9][0-9]*$ ]] || t_bad "tehas_shipped_page is not a count: $page"
[ "$page" -le 256 ] || t_bad "tehas_shipped_page $page is past the terms a search holds"
t_contains "$(tehas_shipped_query)" "first: $page"

t_case "the query and the rule are what the two commands read"
for command in tidy status; do
  t_contains "$(cat "$T_ROOT/libexec/$command")" '/lib/shipped.sh"'
  t_contains "$(cat "$T_ROOT/libexec/$command")" "tehas_shipped_query"
  t_contains "$(cat "$T_ROOT/libexec/$command")" "tehas_shipped_rule"
done

t_done
