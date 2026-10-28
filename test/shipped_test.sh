#!/usr/bin/env bash
# Tests for lib/shipped.sh itself: the ceiling the caller must not pass, the
# query both commands read, and the four verdicts of an answer as the library
# carries them out of the console. What a merged closer is, and what no answer
# is, are decided in console/src/shipped.rs and pinned by that program's own
# tests; these pin the line between them and the callers, so a library that maps
# the console's words onto the wrong status fails here even where no caller's
# case notices.
# shellcheck disable=SC2016  # the GraphQL text is literal
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

. "$T_ROOT/lib/shipped.sh"

# The console of this checkout, which the helpers are asked of. Every case here
# names it, as a caller names the one beside it: the library holds no default,
# so a helper that lost its first argument has nothing to fall back to.
t_console || t_bad "no console: $t_console_why"
console=$T_CONSOLE

# One answer of the batched search, holding the nodes the case names. The
# helpers are exported, and the answer is passed through a variable and not a
# literal: t_run runs each case with env -i, so a function of this file reaches
# the case only as an exported one.
# shellcheck disable=SC2154  # console and the helpers are set above and by the library
export -f tehas_shipped_list tehas_shipped_numbers tehas_shipped_has tehas_shipped_page tehas_shipped_query

answer() { # <node>,...
  local IFS=,
  printf '{"data":{"search":{"nodes":[%s]}}}' "$*"
}
node() { # <number> <closer> [closedAt]: a node of the batched search. The closed
  # time is what the list is ordered by, so a case that asks for an order names
  # one: GitHub answers it as a UTC instant, and the newest close is the first.
  local at=""
  [ -z "${3:-}" ] || at=",\"closedAt\":\"$3\""
  printf '{"number":%s%s,"timelineItems":{"nodes":[{"closer":%s}]}}' "$1" "$at" "$2"
}
export -f answer node
ask() { # <answer> <number>: the status the verdict of the answer is read as, one
  # word. The library holds no helper for a single issue - both callers read the
  # list, and the per-issue reading is the console's own verb - so this is the
  # mapping the library's header names: `merged` is 0, `closed` is 2, and the two
  # words that are no read at all are 1.
  # lib.sh sets pipefail, so a non-zero status would end this function before the
  # word is printed: the status is data here, not a result.
  local said rc=0
  said=$("$console" shipped answer "${2:-}" "${1:-}" 2>/dev/null) || rc=$?
  case $said:$rc in
    merged:0) printf 'rc=0' ;;
    closed:0) printf 'rc=2' ;;
    *) printf 'rc=1' ;;
  esac
}
export -f ask
listed_three() { # [--control <number>]...: the numbers of the three-node answer.
  # paste answers an empty input with no line at all, which is the same word an
  # answer that named nothing gives, so the helper's own status is the thing that
  # tells the two apart and these cases ask for the list alone.
  printf '%s' "$three" | tehas_shipped_numbers "$console" "$@" 2>/dev/null | paste -sd, -
}
export -f listed_three
listed() { # <answer> [--control <number>]...: the numbers the list helper names
  local answer=$1
  shift
  printf '%s' "$answer" | tehas_shipped_numbers "$console" "$@" 2>/dev/null | paste -sd, -
}
export -f listed

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
  t_eq "$(listed "$shape")" ""
done

t_case "an issue number is digits and nothing else"
merged_one=$(answer "$(node 42 '{"merged":true}')")
# Each form is passed through a variable: the harness's env -i would drop a
# literal's leading zero, and the case is about the text the helper is handed.
for form in "" " 42" "42.0" "4.2e1" "1e2" "-42" "+42" "42x" "0x2a" "42;" "42 43" \
  "18446744073709551616" "9007199254740993" "42 42" "4_2"; do
  t_eq "$(ask "$merged_one" "$form")" rc=1
done
# A leading zero is nothing to a number, but it is a thing to a question: the
# digits of a padded form name no issue the answer can be about, so the read is
# not consulted at all. jq's tonumber used to turn one into the number GitHub
# named, which let "042" answer for #42; the console reads the text instead.
for form in "042" "000042"; do
  t_eq "$(ask "$merged_one" "$form")" rc=1
done
# The plain form still answers, so the gate above is a gate and not a refusal.
t_eq "$(ask "$merged_one" 42)" rc=0

t_case "the page is one number, below the ceiling of a search, and reaches the query"
# Read from the console through the library rather than taken as read: a constant
# the caller set on its own would be a second statement of the ceiling, and one
# the library wrote down would drift from the query the same console prints.
page=$(tehas_shipped_page "$console")
[[ $page =~ ^[1-9][0-9]*$ ]] || t_bad "the page is not a count: $page"
[ "$page" -le 256 ] || t_bad "the page $page is past the terms a search holds"
t_contains "$(tehas_shipped_query "$console")" "first: $page"

t_case "the two commands read the library and the console beside them"
for command in tidy status; do
  t_contains "$(cat "$T_ROOT/libexec/$command")" '/lib/shipped.sh"'
  t_contains "$(cat "$T_ROOT/libexec/$command")" 'console=$here/console'
  t_contains "$(cat "$T_ROOT/libexec/$command")" "tehas_shipped_query"
  # Which list verb a command reads is its own: tidy wants the numbers of the
  # nodes and status wants the nodes, so each name is checked in the file that
  # reads it and the control number is given as the console's own flag.
  case $command in
    tidy) t_contains "$(cat "$T_ROOT/libexec/tidy")" 'tehas_shipped_numbers "$console" --control "$control"' ;;
    status) t_contains "$(cat "$T_ROOT/libexec/status")" 'tehas_shipped_list "$console" --control "$control"' ;;
  esac
done

t_case "the query closes every brace it opens, so GitHub can parse it"
# The tests stub gh, so a query GitHub refuses passed them all (v0.14.0).
q=$(tehas_shipped_query "$console")
t_eq "$(tr -cd '{' <<<"$q" | wc -c)" "$(tr -cd '}' <<<"$q" | wc -c)"
t_eq "$(tr -cd '(' <<<"$q" | wc -c)" "$(tr -cd ')' <<<"$q" | wc -c)"
t_eq "$(tr -cd '[' <<<"$q" | wc -c)" "$(tr -cd ']' <<<"$q" | wc -c)"

t_case "the list holds the merged closers, newest first, and the control is none of them"
# The order and the rule are the console's; what is pinned here is that the two
# helpers hand a caller the same list, one of them with the numbers alone, and
# that the control number reaches --control.
three=$(answer "$(node 20 '{"merged":true}' 2026-01-02T00:00:00Z)" \
  "$(node 21 '{"merged":true}' 2026-01-03T00:00:00Z)" \
  "$(node 22 '{"merged":false}' 2026-01-04T00:00:00Z)")
t_eq "$(printf '%s' "$three" | tehas_shipped_list "$console" | jq -sc '[.[].number]')" "[21,20]"
t_eq "$(listed "$three")" "21,20"
t_eq "$(listed_three --control 20)" "21"
# Nothing earned the label, so the read is no answer: the caller logs it as one.
t_eq "$(listed "$(answer "$(node 22 '{"merged":false}')")")" ""
# Both of them are the control issue, so nothing earned the label.
t_eq "$(listed_three --control 20 --control 21)" ""

t_case "the label of a node is read from the node the list kept"
# `carries` says the label is one of the names the node holds, `lacks` that it
# named its labels and the label is none of them, and `unreadable` that it named
# none: a caller edits on the last as on the second, so it never guesses that an
# issue the read did not answer for lacks the label.
has() { # <node> <label>: the status the helper gives. The word the console prints
  # with it is the console's own and the helper sends it away, so what a poll writes
  # to standard error stays its log and nothing else.
  local rc=0
  tehas_shipped_has "$console" "$1" "$2" || rc=$?
  printf 'rc=%s' "$rc"
}
export -f has
t_eq "$(has '{"labels":{"nodes":[{"name":"tehas:shipped"}]}}' tehas:shipped)" rc=0
t_eq "$(has '{"labels":{"nodes":[{"name":"bug"}]}}' tehas:shipped)" rc=1
t_eq "$(has '{"labels":{}}' tehas:shipped)" rc=2
t_eq "$(has '{}' tehas:shipped)" rc=2
# A name with a space in it is one name and no two, and one of "*" is no pattern:
# the name reaches the console as one argument and is compared as text.
t_eq "$(has '{"labels":{"nodes":[{"name":"*"}]}}' '*')" rc=0
t_eq "$(has '{"labels":{"nodes":[{"name":"a"}]}}' 'a b')" rc=1

t_case "a whole answer is told from a failed read, and it is the console that says so"
# The helper asks the console, as `list` judges the answer on its own: a page of
# nothing is an answer and a reply with no list of nodes in it is not, and a library
# that read that rule itself could disagree with the list the same read gave.
whole() { # <answer>: the status the helper gives the reply.
  local rc=0
  tehas_shipped_whole "$console" "${1:-}" || rc=$?
  printf 'rc=%s' "$rc"
}
export -f whole
t_eq "$(whole "$(answer)")" rc=0
t_eq "$(whole "$(answer "$(node 7 '{"merged":true}')")")" rc=0
for shape in '{"data":{"search":{"nodes":null}}}' '{"data":{"search":{"nodes":{}}}}' \
  '{"data":{"search":{"nodes":"7"}}}' '{"data":null}' '{"data":{}}' 'not json' '' '[]'; do
  t_eq "$(whole "$shape")" rc=1
done
# Errors answer no list of nodes, so the reply is no answer even where it holds data.
t_eq "$(whole '{"errors":[{"message":"declined"}],"data":{"search":{"nodes":[]}}}')" rc=1
# An empty errors key is no refusal: gh answers one for a read that went well.
t_eq "$(whole '{"errors":[],"data":{"search":{"nodes":[]}}}')" rc=0

t_done
