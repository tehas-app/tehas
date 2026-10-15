# shellcheck shell=bash
# Questions the factory asks on an issue, and the answers it accepts. Text in,
# text out: nothing here calls GitHub, so every function can be tested with a
# string. Sourced by libexec/dispatch and libexec/cycle.
#
#   q_clean [max characters]         stdin to stdout: an agent's words, safe to
#                                    post, cut at the limit [60000]
#   q_render <kind> <issue> <turn> <result.json>
#                                    the body of a question comment
#   q_answer <bot> <allowed logins> [comment id]
#                                    stdin: the issue's comments as GraphQL
#                                    answers them; stdout: what was answered
#   q_untick                         stdin to stdout: a body with every box empty
#
# <kind> is plan (a planning turn) or park (a cycle that stopped).

# What an agent wrote goes to a public issue. Tokens, database passwords,
# private IPv4 and IPv6 unique-local addresses and tailnet names are masked: a copy in sed of the
# console's rules for those (console/src/mask.rs), so this file needs no
# program beside it. Three more things are taken out because the comment is
# parsed again later or read by people: a comment marker, a task-list box, and
# an @ that would notify somebody. Carriage returns go first: the web interface
# turns a lone one into a line end, and the escaping below only looks at line
# starts, so a lone one would start an unescaped box.
q_clean() {
  local zw=$'\xe2\x80\x8b' max=${1:-60000}
  tr -d '\r' | sed -E \
    -e 's/(^|[^A-Za-z0-9_])(gh[pousr]_|github_pat_)[A-Za-z0-9_]{16,}/\1\2***/g' \
    -e 's#(postgres(ql)?://[^:[:space:]/]+:)[^@[:space:]]+@#\1***@#g' \
    -e 's/(^|[^A-Z0-9_])([A-Z0-9_]*(TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*=)[^[:space:]]+/\1\2***/g' \
    -e 's/(^|[^0-9.])(10\.[0-9]{1,3}|192\.168|172\.(1[6-9]|2[0-9]|3[01])|169\.254|100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}/\1[private address]/g' \
    -e 's/(^|[^0-9A-Fa-f:])[fF][cCdD][0-9A-Fa-f]{2}(:[0-9A-Fa-f]{0,4}){2,}/\1[private address]/g' \
    -e 's/[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.ts\.net/[private name]/g' \
    -e 's/<!--/<\\!--/g' \
    -e 's/^([[:space:]]*[-*+][[:space:]]+)\[([ xX])\]/\1\\[\2]/' \
    -e "s/@([A-Za-z0-9])/@$zw\\1/g" |
    jq -Rrs --argjson max "$max" '.[0:$max] | sub("\n+$"; "")'
}

# GitHub refuses a comment of more than 65 536 characters, and a question that
# cannot be posted would be tried again on every poll for ever. So each part
# has a limit of its own and the whole always fits: the message 40 000, the
# question 4 000, and at most Q_MAX_OPTIONS options of 300 for the label and
# 600 for the description. That is under 63 000 with the fixed lines. The
# record of a planning session lists the same first Q_MAX_OPTIONS options, so
# a ticked box and its option keep the same place.
Q_MAX_OPTIONS=20

q_render() {
  local kind=$1 issue=$2 turn=$3 file=$4 n i label desc
  jq -r '.text // ""' "$file" | q_clean 40000 || return 1
  if [ "$(jq -r '.question != null' "$file")" = true ]; then
    printf '\n'
    jq -r '.question.question // ""' "$file" | q_clean 4000
  fi
  printf '\n'
  n=$(jq '(.question.options // []) | length' "$file") || return 1
  if [ "$n" -gt "$Q_MAX_OPTIONS" ]; then
    printf 'Options left out: %s. A reply can name another choice.\n\n' "$((n - Q_MAX_OPTIONS))"
    n=$Q_MAX_OPTIONS
  fi
  for ((i = 0; i < n; i++)); do
    label=$(jq -r --argjson i "$i" '.question.options[$i].label // "" | gsub("[\r\n]+"; " ")' "$file" | q_clean 300)
    desc=$(jq -r --argjson i "$i" '.question.options[$i].description // "" | gsub("[\r\n]+"; " ")' "$file" | q_clean 600)
    printf -- '- [ ] %s\n' "$label"
    [ -z "$desc" ] || printf '  %s\n' "$desc"
  done
  case "$kind" in
    plan)
      printf -- '- [ ] End this planning session\n\n'
      printf 'Tick one box, or reply in a comment. Only an answer from an account that may authorise work counts.\n'
      ;;
    park)
      printf -- '- [ ] Retry from a clean start\n- [ ] Leave it parked\n\n'
      printf 'Tick one box. Only a tick from an account that may authorise work counts.\n'
      ;;
    *) return 1 ;;
  esac
  printf '\n<!-- tehas:question kind=%s issue=%s turn=%s -->\n' "$kind" "$issue" "$turn"
}

# The question the bot asked, the boxes ticked in it and the newest reply
# after it from an allowed account, last edited by none or by an allowed
# account. A marker proves nothing unless the
# bot wrote the comment: anyone can paste one on a public issue.
#
# With a comment id the question is that comment and no other. An agent in a
# container posts as the bot too, so the caller names the comment the host
# itself has on record. Without an id it is the newest comment of the bot
# with a marker: good enough to tell a person something, never to start work.
# The marker's issue number is reported for the caller to compare.
q_answer() {
  jq -c --arg bot "$1" --arg allowed "$2" --arg id "${3:-}" '
    ($allowed | split(" ")) as $list
    | (.data.repository.issue.comments.nodes // []) as $c
    | "<!-- tehas:question kind=(?<kind>[a-z]+) issue=(?<issue>[0-9]+) turn=(?<turn>[0-9]+) -->" as $re
    | ($c | to_entries | map(select((.value.author.login // "") == $bot and (.value.body | test($re))
                                    and ($id == "" or (.value.databaseId | tostring) == $id))) | last) as $q
    | if $q == null then {asked: false} else
        ($q.value.body | capture($re)) as $m
        | ($q.value.body | split("\n") | map(select(test("^- \\[[ xX]\\] ")) | test("^- \\[[xX]\\]"))) as $boxes
        | ($boxes | to_entries | map(select(.value) | .key)) as $ticks
        | ($q.value.editor.login // "") as $ed
        | ($c[($q.key + 1):] | map(select((.author.login // "") as $a | ($list | index($a))
                  and ((.editor.login // "") as $e | $e == "" or ($list | index($e))))) | last) as $r
        | {asked: true, comment: $q.value.databaseId, asked_at: ($q.value.createdAt // ""), body: $q.value.body, kind: $m.kind,
           issue: ($m.issue | tonumber), turn: ($m.turn | tonumber), boxes: ($boxes | length),
           tick: (if ($ticks | length) == 0 then {status: "none"}
                  elif ($list | index($ed)) == null then {status: "stranger", by: $ed}
                  elif ($ticks | length) > 1 then {status: "many", by: $ed}
                  else {status: "one", by: $ed, index: $ticks[0], at: ($q.value.lastEditedAt // "")} end),
           reply: (if $r == null then null else {by: $r.author.login, text: $r.body, at: $r.createdAt, id: $r.databaseId} end)}
      end'
}

q_untick() { sed -E 's/^- \[[xX]\] /- [ ] /'; }
